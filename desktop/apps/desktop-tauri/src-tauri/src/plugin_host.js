/**
 * Plugin JavaScript host — one Node process per plugin.
 *
 * The Tauri shell spawns this script with the system `node` binary and speaks
 * NDJSON JSON-RPC over stdio:
 *
 *   shell → host   { jsonrpc, id, method, params }
 *   host  → shell  { jsonrpc, id, result | error }
 *   host  → shell  { jsonrpc, id, method, params }        (host services)
 *   shell → host   { jsonrpc, id, result | error }
 *
 * Plugin code never sees stdout: `console.*` is redirected to stderr so a
 * chatty plugin cannot corrupt the protocol. Methods: load, unload,
 * runCommand, executeTool, updateSettings, themes.
 */

"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { AsyncLocalStorage } = require("node:async_hooks");
const { spawn } = require("node:child_process");
const { createRequire } = require("node:module");
const { pathToFileURL } = require("node:url");
const readline = require("node:readline");

const PROTOCOL_STDOUT = process.stdout;

function writeLine(message) {
  PROTOCOL_STDOUT.write(`${JSON.stringify(message)}\n`);
}

function stringify(value) {
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

// A plugin calling console.log must not interleave with our protocol frames.
for (const channel of ["log", "info", "warn", "error", "debug", "trace"]) {
  console[channel] = (...args) => {
    const text = args
      .map((arg) => (typeof arg === "string" ? arg : stringify(arg)))
      .join(" ");
    process.stderr.write(`[plugin:${channel}] ${text}\n`);
  };
}

class HostError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

// ── host calls (host → shell reverse RPC) ───────────────────────────────────

let hostSequence = 1;
const hostPending = new Map();

function hostCall(method, params, timeoutMs = 15_000) {
  return new Promise((resolve, reject) => {
    const id = hostSequence++;
    const timer = setTimeout(() => {
      hostPending.delete(id);
      reject(new HostError("HOST_TIMEOUT", `${method} timed out`));
    }, timeoutMs);
    hostPending.set(id, (ok, value) => {
      clearTimeout(timer);
      if (ok) resolve(value);
      else reject(value);
    });
    try {
      writeLine({ jsonrpc: "2.0", id, method, params });
    } catch (error) {
      clearTimeout(timer);
      hostPending.delete(id);
      reject(error);
    }
  });
}

// Tool calls carry the only session identity plugin code may use. The host
// resolves it on its side; plugins never pass a session id themselves.
const toolSession = new AsyncLocalStorage();

// Extension loop handlers run with the session their event came from. The
// events are fire-and-forget and may overlap, so the identity travels with
// async context instead of a process-wide "current session".
const extensionSession = new AsyncLocalStorage();

function bridgeCall(channel, payload, timeoutMs = 30_000) {
  const sessionId = toolSession.getStore()?.sessionId;
  return hostCall(
    "bridge.invoke",
    {
      channel,
      payload: payload ?? {},
      ...(sessionId ? { sessionId } : {}),
    },
    timeoutMs,
  );
}

// ── per-process plugin state ────────────────────────────────────────────────

const runtime = {
  pluginId: "",
  root: "",
  manifest: null,
  settings: {},
  settingsDefaults: {},
  appInfo: { version: "", locale: "en" },
  commands: new Map(),
  tools: new Map(),
  themes: new Map(),
  services: new Map(),
  // Declared in the manifest, registered during onLoad, started and stopped
  // by the broker around the plugin's own lifetime.
  serviceDeclarations: new Map(),
  serviceRuntime: new Map(),
  eventHandlers: new Map(),
  module: null,
  settingsSchema: new Map(),
  // Agent extensions (spec 07-plugins/16): modules loaded from
  // `contributes.agentExtensions`, their commands, tools, flags and events.
  extensionGranted: false,
  extensionCommands: new Map(),
  extensionTools: new Map(),
  extensionFlags: new Map(),
  extensionEventHandlers: new Map(),
  extensionSessions: new Set(),
  extensionDiagnostics: new Map(),
  extensionEntries: [],
  extensionErrors: new Set(),
  extensionActive: null,
  shimsInstalled: false,
  workspace: null,
};

function manifestToolSchema(toolName) {
  const entries = runtime.manifest?.contributes?.agentTools;
  if (!Array.isArray(entries)) return undefined;
  const entry = entries.find((item) => item && item.name === toolName);
  return entry && typeof entry.schema === "object" ? entry.schema : undefined;
}

function emptySchema() {
  return { type: "object", properties: {} };
}

function normalizedKeywords(value) {
  if (!Array.isArray(value)) return [];
  return value.filter((entry) => typeof entry === "string" && entry.trim()).map((entry) => entry.trim());
}

// ── agent extensions (spec 07-plugins/16) ──────────────────────────────────
//
// `contributes.agentExtensions` modules are the pi CLI's `ExtensionAPI`
// contract. They run in this plugin's process under the same trust decision as
// the plugin itself. Commands, the UI bridge and the loop events the Go
// sidecar emits are supported; `registerTool` is still recorded with a
// diagnostic because extension tools are not executed in this build.

const EXTENSION_SHIMS = {
  typebox:
    "const make = (kind) => (...args) => ({ type: kind });\n" +
    "export const Type = new Proxy({}, { get: (_target, key) => (typeof key === 'string' ? make(key) : undefined) });\n" +
    "export const Static = undefined;\n" +
    "export default Type;\n",
  "@earendil-works/pi-coding-agent":
    "export const defineTool = (definition) => definition;\n" +
    "export const isToolCall = (value) => Boolean(value) && typeof value === 'object' && value.type === 'toolCall';\n" +
    "export const isToolResult = (value) => Boolean(value) && typeof value === 'object' && value.type === 'toolResult';\n" +
    "export const isAssistantMessage = (value) => Boolean(value) && typeof value === 'object' && value.role === 'assistant';\n" +
    "export const isUserMessage = (value) => Boolean(value) && typeof value === 'object' && value.role === 'user';\n" +
    "export const isTextContent = (value) => Boolean(value) && typeof value === 'object' && value.type === 'text';\n" +
    "export const isImageContent = (value) => Boolean(value) && typeof value === 'object' && value.type === 'image';\n" +
    "export const isThinkingContent = (value) => Boolean(value) && typeof value === 'object' && value.type === 'thinking';\n" +
    "export default { defineTool };\n",
  "@earendil-works/pi-tui":
    "class Inert { constructor() {} dispose() {} render() { return []; } setText() {} }\n" +
    "export const Text = Inert;\n" +
    "export const Box = Inert;\n" +
    "export const Container = Inert;\n" +
    "export const Spacer = Inert;\n" +
    "export const Markdown = Inert;\n" +
    "export const Input = Inert;\n" +
    "export const SelectList = Inert;\n" +
    "export const Loader = Inert;\n" +
    "export const TruncatedText = Inert;\n" +
    "export const visibleWidth = () => 0;\n" +
    "export const truncateToWidth = (value) => String(value ?? '');\n" +
    "export default { Text, Box, Container };\n",
  "pi-ai": "export default {};\n",
  "@earendil-works/pi-ai": "export default {};\n",
  "pi-agent-core": "export default {};\n",
  "@earendil-works/pi-agent-core": "export default {};\n",
};

function extensionShimUrl(code) {
  return `data:text/javascript;charset=utf-8,${encodeURIComponent(code)}`;
}

const EXTENSION_SHIM_URLS = Object.fromEntries(
  Object.entries(EXTENSION_SHIMS).map(([specifier, code]) => [specifier, extensionShimUrl(code)]),
);

/**
 * Teach Node how to resolve the pi kernel imports an extension may make.
 * `typebox` becomes a schema-shaped stub; the tui package exports inert
 * classes. A Node build without `registerHooks` reports a load diagnostic when
 * a module really imports one of these.
 */
function installExtensionShims() {
  if (runtime.shimsInstalled) return;
  runtime.shimsInstalled = true;
  let moduleApi;
  try {
    moduleApi = require("node:module");
  } catch {
    return;
  }
  if (typeof moduleApi.registerHooks !== "function") {
    reportExtensionDiagnostic(
      "*",
      "stub_symbol",
      "this Node build cannot resolve the pi kernel modules; extension imports will fail",
      "registerHooks",
    );
    return;
  }
  try {
    moduleApi.registerHooks({
      resolve(specifier, context, nextResolve) {
        const shim = EXTENSION_SHIM_URLS[specifier];
        if (shim) return { url: shim, shortCircuit: true };
        return nextResolve(specifier, context);
      },
    });
  } catch (error) {
    reportExtensionDiagnostic(
      "*",
      "stub_symbol",
      error?.message || String(error),
      "registerHooks",
      error,
    );
  }
}

function reportExtensionDiagnostic(extensionId, kind, message, member, error) {
  const id = String(extensionId || "*");
  const key = `${id}\u0000${kind}\u0000${member ?? ""}`;
  const existing = runtime.extensionDiagnostics.get(key);
  if (existing) {
    existing.count += 1;
    return;
  }
  const diagnostic = {
    extensionId: id,
    kind,
    message: String(message ?? ""),
    member: String(member ?? ""),
    count: 1,
  };
  if (error && error.stack) diagnostic.stack = String(error.stack);
  runtime.extensionDiagnostics.set(key, diagnostic);
}

function extensionDiagnostics() {
  return [...runtime.extensionDiagnostics.values()];
}

/**
 * One diagnostic per (extension, member), emitted when the extension actually
 * touches the member -- not when the API object is built, or every extension
 * would carry a page of diagnostics for members it never used.
 */
function unsupportedExtensionMember(extensionId, member, neutral) {
  return (...args) => {
    reportExtensionDiagnostic(
      extensionId,
      "unsupported_api",
      `pi.${member} is not available in this desktop build`,
      member,
    );
    return typeof neutral === "function" ? neutral(...args) : neutral;
  };
}

function extensionUi(sessionId, extensionId, extensionLabel, request, timeoutMs) {
  return hostCall(
    "extension.ui",
    { sessionId, extensionId, extensionLabel, request },
    timeoutMs ?? 6 * 60_000,
  );
}

function extensionExec(command, args, options) {
  const active = runtime.extensionActive;
  const cwd =
    (options && typeof options.cwd === "string" && options.cwd) ||
    (active && active.cwd) ||
    (runtime.workspace && runtime.workspace.path) ||
    runtime.root ||
    process.cwd();
  const list = Array.isArray(args) ? args.map((entry) => String(entry)) : [];
  return new Promise((resolve, reject) => {
    const child = spawn(String(command), list, {
      cwd,
      env: { ...process.env, ...(options?.env && typeof options.env === "object" ? options.env : {}) },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    let settled = false;
    let timer;
    const settle = (fn, value) => {
      if (settled) return;
      settled = true;
      if (timer) clearTimeout(timer);
      fn(value);
    };
    if (options?.timeout) {
      timer = setTimeout(() => child.kill("SIGTERM"), Number(options.timeout));
    }
    child.stdout.on("data", (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString();
    });
    child.on("error", (error) => settle(reject, error));
    child.on("close", (code) => settle(resolve, { stdout, stderr, code }));
  });
}

function extensionContext(extensionId, extensionLabel, sessionId, cwd) {
  const ui = {
    notify: async (message, level) => {
      await extensionUi(sessionId, extensionId, extensionLabel, {
        kind: "notify",
        message: String(message ?? ""),
        level: level === "warning" || level === "error" ? level : "info",
      }, 15_000);
    },
    confirm: async (title, message) => {
      const reply = await extensionUi(sessionId, extensionId, extensionLabel, {
        kind: "confirm",
        title: String(title ?? ""),
        message: String(message ?? ""),
      });
      return reply?.value === true;
    },
    select: async (title, options) => {
      const reply = await extensionUi(sessionId, extensionId, extensionLabel, {
        kind: "select",
        title: String(title ?? ""),
        options: (Array.isArray(options) ? options : []).map((entry) =>
          typeof entry === "string" ? entry : String(entry?.value ?? entry?.label ?? ""),
        ),
      });
      return reply?.value ?? undefined;
    },
    input: async (title, placeholder) => {
      const reply = await extensionUi(sessionId, extensionId, extensionLabel, {
        kind: "input",
        title: String(title ?? ""),
        placeholder: typeof placeholder === "string" ? placeholder : undefined,
      });
      return reply?.value ?? undefined;
    },
    setStatus: async (key, text) => {
      await extensionUi(sessionId, extensionId, extensionLabel, {
        kind: "setStatus",
        key: String(key ?? ""),
        text: text === undefined || text === null ? undefined : String(text),
      }, 15_000);
    },
    setWorkingMessage: async (text) => {
      await extensionUi(sessionId, extensionId, extensionLabel, {
        kind: "setWorkingMessage",
        text: text === undefined || text === null ? undefined : String(text),
      }, 15_000);
    },
  };
  for (const member of [
    "setWidget",
    "setFooter",
    "setHeader",
    "setTitle",
    "custom",
    "overlay",
    "onTerminalInput",
    "setWorkingVisible",
    "setWorkingIndicator",
    "setHiddenThinkingLabel",
    "pasteToEditor",
    "editor",
    "addAutocompleteProvider",
    "setEditorText",
    "getEditorText",
  ]) {
    ui[member] = unsupportedExtensionMember(extensionId, `ui.${member}`, () => undefined);
  }
  return {
    sessionId,
    cwd: cwd || (runtime.workspace && runtime.workspace.path) || runtime.root,
    ui,
    modelRegistry: { get: () => undefined, list: () => [] },
    isIdle: () => true,
    abort: () => {},
    hasPendingMessages: () => false,
    getContextUsage: () => null,
    compact: unsupportedExtensionMember(extensionId, "compact", async () => {}),
    getSystemPrompt: async () => "",
    waitForIdle: async () => {},
    newSession: unsupportedExtensionMember(extensionId, "newSession", async () => ({})),
    fork: unsupportedExtensionMember(extensionId, "fork", async () => ({})),
  };
}

function activeExtensionSession() {
  const active = extensionSession.getStore() || runtime.extensionActive;
  if (!active || !active.sessionId) {
    throw new HostError("SESSION_REQUIRED", "this extension call needs an active session");
  }
  return active;
}

/// Loop events the Go sidecar really emits, under the names extensions use.
const EXTENSION_LOOP_EVENTS = new Set([
  "agent_start",
  "agent_end",
  "turn_start",
  "turn_end",
  "message_start",
  "message_update",
  "message_end",
  "tool_execution_start",
  "tool_execution_update",
  "tool_execution_end",
]);

function buildExtensionApi(extensionId, extensionLabel) {
  const api = {
    registerCommand: (name, options) => {
      const command = requireString(name, "command.name");
      if (runtime.extensionCommands.has(command)) {
        reportExtensionDiagnostic(
          extensionId,
          "rejected_registration",
          `command "${command}" is already registered`,
          command,
        );
        return;
      }
      runtime.extensionCommands.set(command, {
        name: command,
        description: String(options?.description ?? ""),
        handler: typeof options?.handler === "function" ? options.handler : null,
        extensionId,
        extensionLabel,
      });
    },
    registerTool: (definition) => {
      const name = String(definition?.name ?? "").trim();
      if (!name) return;
      runtime.extensionTools.set(name, { ...definition, name, extensionId, extensionLabel });
      reportExtensionDiagnostic(
        extensionId,
        "unsupported_api",
        "extension tools are recorded but not executed in this desktop build",
        "registerTool",
      );
    },
    on: (event, handler) => {
      if (typeof handler !== "function") return;
      const key = String(event ?? "");
      if (
        !["session_start", "session_shutdown"].includes(key) &&
        !EXTENSION_LOOP_EVENTS.has(key)
      ) {
        reportExtensionDiagnostic(
          extensionId,
          "unsupported_api",
          `pi.on("${key}") is not emitted by this desktop build`,
          `on:${key}`,
        );
        return;
      }
      const handlers = runtime.extensionEventHandlers.get(key) || [];
      handlers.push({ handler, extensionId, extensionLabel });
      runtime.extensionEventHandlers.set(key, handlers);
    },
    registerFlag: (name, options) => {
      runtime.extensionFlags.set(String(name ?? ""), options?.default);
    },
    getFlag: (name) => runtime.extensionFlags.get(String(name ?? "")),
    exec: extensionExec,
    getActiveTools: async () => [...runtime.extensionTools.keys()],
    getAllTools: async () => [...runtime.extensionTools.keys()],
    setActiveTools: async () => {},
    getCommands: async () =>
      [...runtime.extensionCommands.values()].map((command) => ({
        name: command.name,
        description: command.description,
      })),
    setModel: async () => false,
    getThinkingLevel: async () => null,
    setThinkingLevel: async () => {},
    setSessionName: async (title) => {
      const active = activeExtensionSession();
      await hostCall("extension.session.rename", {
        sessionId: active.sessionId,
        title: String(title ?? ""),
      });
    },
    getSessionName: async () => {
      const active = activeExtensionSession();
      const reply = await hostCall("extension.session.get", { sessionId: active.sessionId });
      return reply?.title ?? undefined;
    },
    sendUserMessage: unsupportedExtensionMember(extensionId, "sendUserMessage", async () => {}),
    cwd: (runtime.workspace && runtime.workspace.path) || runtime.root,
    sessionManager: {
      list: () => [],
      get: () => undefined,
      getMessages: () => [],
    },
    modelRegistry: { get: () => undefined, list: () => [] },
  };
  for (const member of [
    "registerShortcut",
    "registerMarkdownTransformer",
    "switchSession",
    "appendEntry",
    "setLabel",
    "navigateTree",
    "shutdown",
    "sendMessage",
    "registerMessageRenderer",
    "registerEntryRenderer",
  ]) {
    api[member] = unsupportedExtensionMember(extensionId, member, null);
  }
  return api;
}

async function emitExtensionEvent(event, payload, sessionId, cwd) {
  const handlers = [...(runtime.extensionEventHandlers.get(event) || [])];
  for (const entry of handlers) {
    try {
      await extensionSession.run({ sessionId, cwd }, () =>
        withTimeout(
          Promise.resolve().then(() =>
            entry.handler(payload, extensionContext(entry.extensionId, entry.extensionLabel, sessionId, cwd)),
          ),
          30_000,
          "EXTENSION_HANDLER_TIMEOUT",
          `${event} handler`,
        ),
      );
    } catch (error) {
      reportExtensionDiagnostic(
        entry.extensionId,
        "handler_error",
        error?.message || String(error),
        event,
        error,
      );
    }
  }
}

async function ensureExtensionSession(sessionId, cwd) {
  if (!sessionId || runtime.extensionSessions.has(sessionId)) return;
  runtime.extensionSessions.add(sessionId);
  await emitExtensionEvent("session_start", { reason: "startup" }, sessionId, cwd);
}

function extensionReport() {
  const entries = runtime.extensionEntries.length;
  const hasManifestEntries = Array.isArray(runtime.manifest?.contributes?.agentExtensions)
    && runtime.manifest.contributes.agentExtensions.length > 0;
  if (!hasManifestEntries) return null;
  return {
    state: runtime.extensionErrors.size > 0 ? "error" : "loaded",
    toolNames: [...runtime.extensionTools.keys()],
    commandNames: [...runtime.extensionCommands.keys()],
    diagnostics: extensionDiagnostics(),
    commands: [...runtime.extensionCommands.values()].map((command) => ({
      name: command.name,
      description: command.description,
      extensionId: command.extensionId,
      extensionLabel: command.extensionLabel,
    })),
    entries: entries > 0 ? runtime.extensionEntries : [],
  };
}

async function loadAgentExtensions(granted) {
  const manifest = runtime.manifest;
  const declared = Array.isArray(manifest?.contributes?.agentExtensions)
    ? manifest.contributes.agentExtensions
    : [];
  if (declared.length === 0) return null;
  runtime.extensionGranted = Boolean(granted);
  if (!runtime.extensionGranted) {
    reportExtensionDiagnostic(
      "*",
      "load_error",
      "the agent.extension permission is not granted; modules were skipped",
    );
    return extensionReport();
  }
  installExtensionShims();
  for (const relative of declared) {
    if (typeof relative !== "string" || !relative.trim()) continue;
    const entryPath = path.resolve(runtime.root, relative);
    if (!insideRoot(entryPath) || !fs.existsSync(entryPath)) {
      reportExtensionDiagnostic(entryPath, "load_error", `entry not found: ${relative}`);
      runtime.extensionErrors.add(entryPath);
      continue;
    }
    let id = entryPath;
    try {
      id = fs.realpathSync(entryPath);
    } catch {}
    const label = String(manifest?.name || path.basename(entryPath));
    let loaded;
    try {
      loaded = await importModule(entryPath);
    } catch (error) {
      reportExtensionDiagnostic(id, "load_error", error?.message || String(error), undefined, error);
      runtime.extensionErrors.add(id);
      console.log(`[extension] load failed ${entryPath}: ${error?.message || error}`);
      continue;
    }
    // A CommonJS extension exports the factory itself; an ESM one exposes it
    // as `default`; either may name it `activate`.
    const factory =
      typeof loaded === "function"
        ? loaded
        : typeof loaded?.default === "function"
          ? loaded.default
          : typeof loaded?.activate === "function"
            ? loaded.activate
            : null;
    if (!factory) {
      reportExtensionDiagnostic(id, "factory_error", "extension has no default export function");
      runtime.extensionErrors.add(id);
      continue;
    }
    try {
      await withTimeout(
        Promise.resolve().then(() => factory(buildExtensionApi(id, label))),
        30_000,
        "EXTENSION_LOAD_FAILED",
        "extension activation",
      );
      runtime.extensionEntries.push({
        id,
        entry: entryPath,
        label,
        extensionId: id,
        root: runtime.root,
        source: "plugin",
      });
      console.log(`[extension] loaded ${label} (${entryPath})`);
    } catch (error) {
      reportExtensionDiagnostic(id, "factory_error", error?.message || String(error), undefined, error);
      runtime.extensionErrors.add(id);
    }
  }
  return extensionReport();
}

async function runExtensionCommand(params) {
  const sessionId = requireString(params?.sessionId, "sessionId");
  const name = requireString(params?.name, "name");
  const command = runtime.extensionCommands.get(name);
  if (!command) {
    throw new HostError("COMMAND_NOT_FOUND", `extension command "${name}" is not registered`);
  }
  if (typeof command.handler !== "function") {
    throw new HostError("COMMAND_NOT_FOUND", `command "${name}" has no handler`);
  }
  const cwd =
    (typeof params?.cwd === "string" && params.cwd) ||
    (runtime.workspace && runtime.workspace.path) ||
    runtime.root;
  runtime.extensionActive = { sessionId, cwd };
  try {
    await ensureExtensionSession(sessionId, cwd);
    await withTimeout(
      Promise.resolve().then(() => command.handler(String(params?.args ?? ""), extensionContext(
        command.extensionId,
        command.extensionLabel,
        sessionId,
        cwd,
      ))),
      15 * 60_000,
      "EXTENSION_COMMAND_TIMEOUT",
      `extension command ${name}`,
    );
    return { ok: true };
  } finally {
    runtime.extensionActive = null;
  }
}

/** Local text helpers so the huge API surface below stays readable. */
function requireString(value, field) {
  if (typeof value !== "string" || !value.trim()) {
    throw new HostError("PLUGIN_INVALID", `${field} must be a non-empty string`);
  }
  return value.trim();
}

function buildApi() {
  const pluginApi = {
    app: {
      getVersion: async () => runtime.appInfo.version || (await hostCall("app.info")).version,
      getLocale: async () => runtime.appInfo.locale || (await hostCall("app.info")).locale,
      getAppearance: () => hostCall("app.appearance"),
      setTheme: (themeId) => hostCall("app.setTheme", { themeId: String(themeId ?? "") }),
    },
    themes: {
      upsert: async (input) => {
        if (!input || typeof input !== "object") {
          throw new HostError("PLUGIN_INVALID", "theme input must be an object");
        }
        const id = requireString(input.id, "theme.id");
        const label = requireString(input.label, "theme.label");
        const base = input.base === "light" ? "light" : "dark";
        const css = typeof input.css === "string" ? input.css : "";
        if (!css.trim()) throw new HostError("PLUGIN_INVALID", "theme.css must not be empty");
        const reply = await hostCall("themes.upsert", { theme: { id, label, base, css } });
        runtime.themes.set(id, reply.theme);
      },
      remove: async (themeId) => {
        const id = requireString(themeId, "themeId");
        await hostCall("themes.remove", { themeId: id });
        runtime.themes.delete(id);
      },
      list: () => hostCall("themes.list"),
    },
    plugin: {
      getId: () => runtime.pluginId,
      getManifest: () => runtime.manifest,
      getSettings: async () => ({ ...runtime.settingsDefaults, ...runtime.settings }),
      setSettings: async (partial) => {
        if (!partial || typeof partial !== "object" || Array.isArray(partial)) {
          throw new HostError("PLUGIN_INVALID", "setSettings requires an object");
        }
        const reply = await hostCall("settings.set", { values: partial });
        runtime.settings = { ...runtime.settings, ...(reply.values || {}) };
      },
      getDataPath: () => hostCall("plugin.dataPath"),
    },
    commands: {
      register: async (command) => {
        if (!command || typeof command !== "object") {
          throw new HostError("PLUGIN_INVALID", "command must be an object");
        }
        const id = requireString(command.id, "command.id");
        const title = requireString(command.title, "command.title");
        if (typeof command.run !== "function") {
          throw new HostError("PLUGIN_INVALID", "command.run must be a function");
        }
        runtime.commands.set(id, {
          id,
          title,
          category: typeof command.category === "string" ? command.category : undefined,
          keywords: normalizedKeywords(command.keywords),
          run: command.run,
        });
      },
      unregister: async (id) => {
        runtime.commands.delete(String(id ?? ""));
      },
    },
    ui: {
      openPanel: async (opts) => {
        await hostCall("ui.openPanel", { title: String(opts?.title ?? "") });
      },
      closePanel: async () => {
        await hostCall("ui.closePanel", {});
      },
      showToast: async (message, level) => {
        await hostCall("toast", {
          message: String(message ?? ""),
          level: level === "warn" || level === "error" ? level : "info",
        });
      },
      notify: async (input) => {
        await hostCall("notify", {
          title: String(input?.title ?? ""),
          body: String(input?.body ?? ""),
        });
      },
      getNotificationPermission: async () => "granted",
      requestNotificationPermission: async () => "granted",
      showNativeNotification: async (input) => {
        await hostCall("notify", {
          title: String(input?.title ?? ""),
          body: String(input?.body ?? ""),
        });
        return { shown: true, permission: "granted" };
      },
    },
    project: {
      create: (input) => bridgeCall("project.create", { path: input?.path }),
    },
    workspace: { get: () => hostCall("workspace.get") },
    desktop: {
      listOperations: () => bridgeCall("desktop.listOperations", {}),
      invoke: (input) => bridgeCall("desktop.invoke", input ?? {}, 60_000),
    },
    fs: {
      readText: (pathFromRoot) => bridgeCall("fs.readText", { path: pathFromRoot }),
      stat: (pathFromRoot, grantId) =>
        bridgeCall("fs.stat", { path: pathFromRoot, grantId }),
      readRange: async (pathFromRoot, byteOffset, length, grantId) => {
        const result = await bridgeCall("fs.readRange", {
          path: pathFromRoot,
          byteOffset,
          length,
          grantId,
        });
        return {
          bytes: Buffer.from(result?.bytes ?? "", "base64"),
          totalSize: result?.totalSize ?? 0,
        };
      },
      readPreview: (pathFromRoot) =>
        bridgeCall("fs.readPreview", { path: pathFromRoot }),
      openDefault: (pathFromRoot) =>
        bridgeCall("fs.openDefault", { path: pathFromRoot }),
      reveal: (pathFromRoot) => bridgeCall("fs.reveal", { path: pathFromRoot }),
      writeText: (pathFromRoot, content) =>
        bridgeCall("fs.writeText", { path: pathFromRoot, content }),
      glob: (pattern) => bridgeCall("fs.glob", { pattern }),
      list: (pathFromRoot) => bridgeCall("fs.list", { path: pathFromRoot }),
      remove: (pathFromRoot) => bridgeCall("fs.remove", { path: pathFromRoot }),
      requestDirectory: () => bridgeCall("fs.requestDirectory", {}, 5 * 60_000),
    },
    agent: {
      registerTool: async (tool) => {
        if (!tool || typeof tool !== "object") {
          throw new HostError("PLUGIN_INVALID", "tool must be an object");
        }
        const name = requireString(tool.name, "tool.name");
        if (!/^[a-zA-Z0-9_-]+$/.test(name)) {
          throw new HostError(
            "PLUGIN_INVALID",
            "tool.name may only contain letters, digits, underscores and dashes",
          );
        }
        const description = requireString(tool.description, "tool.description");
        if (typeof tool.execute !== "function") {
          throw new HostError("PLUGIN_INVALID", "tool.execute must be a function");
        }
        const declared = manifestToolSchema(name);
        const schema =
          tool.schema && typeof tool.schema === "object" ? tool.schema : declared || emptySchema();
        const risk = ["low", "medium", "high"].includes(tool.risk) ? tool.risk : "medium";
        runtime.tools.set(name, {
          name,
          description,
          risk,
          planSafeActions: normalizedKeywords(tool.planSafeActions),
          schema,
          execute: tool.execute,
        });
      },
      unregisterTool: async (name) => {
        runtime.tools.delete(String(name ?? ""));
      },
      complete: (input) => bridgeCall("agent.complete", input ?? {}, 90_000),
    },
    models: { list: () => bridgeCall("models.list", {}) },
    session: {
      getLlmContext: () => bridgeCall("session.getLlmContext", {}),
      list: (input) => bridgeCall("session.list", input ?? {}),
      get: (input) => bridgeCall("session.get", input ?? {}),
      listMessages: (input) => bridgeCall("session.listMessages", input ?? {}),
      import: (input) => bridgeCall("session.import", input ?? {}),
      importBatch: (input) => bridgeCall("session.importBatch", input ?? {}),
      rename: (input) => bridgeCall("session.rename", input ?? {}),
      delete: (input) => bridgeCall("session.delete", input ?? {}),
    },
    services: {
      register: (service) => {
        if (!service || typeof service !== "object") return;
        const id = String(service.id ?? "").trim();
        if (!id || typeof service.start !== "function") return;
        runtime.services.set(id, service);
      },
      unregister: async (id) => {
        runtime.services.delete(String(id ?? ""));
      },
    },
    bus: {
      // No cross-plugin broker is hosted in this build: publish has no
      // subscribers and subscribe yields an unsubscribe that does nothing.
      publish: async () => {},
      subscribe: async () => async () => {},
    },
    clipboard: {
      readText: () => bridgeCall("clipboard.readText", {}),
      writeText: (text) =>
        bridgeCall("clipboard.writeText", { text: String(text ?? "") }),
      getHistory: () => bridgeCall("clipboard.getHistory", {}),
    },
    shell: {
      openExternal: (url) => bridgeCall("shell.openExternal", { url }),
    },
    // The guest is host-owned (ADR 0170, ADR 0255). Every call crosses the
    // bridge so the shell enforces `browser.cdp`, bounds clamping and the CDP
    // allowlist; this process only forwards arguments.
    browser: {
      navigate: (input) => bridgeCall("browser.navigate", input ?? {}),
      action: (input) => bridgeCall("browser.action", input ?? {}),
      setBounds: (hole) => bridgeCall("browser.setBounds", hole ?? {}),
      setVisible: (visible) =>
        bridgeCall(
          "browser.setVisible",
          typeof visible === "boolean" ? { visible } : (visible ?? {}),
        ),
      getState: () => bridgeCall("browser.getState", {}),
      openExternal: () => bridgeCall("browser.openExternal", {}),
      snapshot: () => bridgeCall("browser.snapshot", {}),
      screenshot: (input) => bridgeCall("browser.screenshot", input ?? {}),
      click: (input) => bridgeCall("browser.click", input ?? {}),
      fill: (input) => bridgeCall("browser.fill", input ?? {}),
      evaluate: (input) => bridgeCall("browser.evaluate", input ?? {}),
      console: (input) => bridgeCall("browser.console", input ?? {}),
      cdp: (input) => bridgeCall("browser.cdp", input ?? {}),
    },
    net: { fetch: (input) => bridgeCall("net.fetch", input ?? {}, 60_000) },
    events: {
      on: (event, handler) => {
        if (typeof handler !== "function") return;
        const key = String(event ?? "");
        const handlers = runtime.eventHandlers.get(key) || new Set();
        handlers.add(handler);
        runtime.eventHandlers.set(key, handlers);
      },
      off: (event, handler) => {
        const key = String(event ?? "");
        const handlers = runtime.eventHandlers.get(key);
        if (!handlers) return;
        if (typeof handler === "function") handlers.delete(handler);
        else handlers.clear();
      },
    },
  };
  return pluginApi;
}

// ── module loading ──────────────────────────────────────────────────────────

function insideRoot(target) {
  const root = path.resolve(runtime.root);
  const resolved = path.resolve(target);
  return resolved === root || resolved.startsWith(`${root}${path.sep}`);
}

async function importModule(entry) {
  const requireFromRoot = createRequire(path.join(runtime.root, "package.json"));
  try {
    return requireFromRoot(entry);
  } catch (error) {
    if (
      error &&
      (error.code === "ERR_REQUIRE_ESM" ||
        error.code === "ERR_REQUIRE_ASYNC_MODULE" ||
        error.code === "ERR_UNKNOWN_FILE_EXTENSION")
    ) {
      const url = `${pathToFileURL(entry).href}?t=${Date.now()}`;
      return import(url);
    }
    throw error;
  }
}

function normalizeModule(loaded) {
  if (loaded && typeof loaded.onLoad === "function") return loaded;
  if (loaded && loaded.default && typeof loaded.default.onLoad === "function") {
    return loaded.default;
  }
  return loaded || {};
}

function withTimeout(promise, timeoutMs, code, label) {
  let timer;
  const guard = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new HostError(code, `${label} timed out`)), timeoutMs);
  });
  return Promise.race([promise, guard]).finally(() => clearTimeout(timer));
}

// ── methods ─────────────────────────────────────────────────────────────────

async function loadPlugin(params) {
  const pluginId = requireString(params?.pluginId, "pluginId");
  const root = requireString(params?.root, "root");
  const manifest = params?.manifest;
  if (!manifest || typeof manifest !== "object") {
    throw new HostError("PLUGIN_INVALID", "manifest is required");
  }
  const main = requireString(manifest.main, "manifest.main");

  runtime.pluginId = pluginId;
  runtime.root = root;
  runtime.manifest = manifest;
  runtime.settings = { ...(params.settings || {}) };
  runtime.workspace = params?.workspace ?? null;
  runtime.extensionCommands.clear();
  runtime.extensionTools.clear();
  runtime.extensionFlags.clear();
  runtime.extensionEventHandlers.clear();
  runtime.extensionSessions.clear();
  runtime.extensionDiagnostics.clear();
  runtime.extensionEntries = [];
  runtime.extensionErrors.clear();
  runtime.extensionActive = null;
  runtime.extensionGranted = params?.extensionGranted === true;
  runtime.services.clear();
  runtime.serviceRuntime.clear();
  runtime.serviceDeclarations.clear();
  const declaredServices = Array.isArray(manifest?.contributes?.services)
    ? manifest.contributes.services
    : [];
  for (const entry of declaredServices) {
    if (entry && typeof entry.id === "string" && entry.id) {
      runtime.serviceDeclarations.set(entry.id, entry);
    }
  }
  const definitions = Array.isArray(manifest?.contributes?.settings)
    ? manifest.contributes.settings
    : [];
  runtime.settingsDefaults = {};
  for (const definition of definitions) {
    if (!definition || typeof definition.key !== "string") continue;
    if (definition.default !== undefined) {
      runtime.settingsDefaults[definition.key] = definition.default;
    }
    runtime.settingsSchema.set(definition.key, definition);
  }
  runtime.appInfo = {
    version: String(params?.app?.version ?? ""),
    locale: String(params?.app?.locale ?? "en"),
  };

  const entry = path.resolve(root, main);
  if (!insideRoot(entry)) {
    throw new HostError("PLUGIN_INVALID", "manifest.main escapes the plugin directory");
  }
  if (!fs.existsSync(entry)) {
    throw new HostError("PLUGIN_LOAD_FAILED", `entry point not found: ${main}`);
  }

  globalThis.pi = buildApi();
  const loaded = normalizeModule(await importModule(entry));
  runtime.module = loaded;
  if (typeof loaded.onLoad === "function") {
    await withTimeout(
      Promise.resolve().then(() => loaded.onLoad()),
      15_000,
      "PLUGIN_LOAD_FAILED",
      "onLoad",
    );
  }

  const extension = await loadAgentExtensions(runtime.extensionGranted);

  return {
    commands: [...runtime.commands.values()].map((command) => ({
      id: command.id,
      title: command.title,
      category: command.category,
      keywords: command.keywords,
    })),
    tools: [...runtime.tools.values()].map((tool) => ({
      name: tool.name,
      description: tool.description,
      risk: tool.risk,
      planSafeActions: tool.planSafeActions,
      schema: tool.schema,
    })),
    themes: [...runtime.themes.values()],
    extension,
  };
}

// ── resident services ───────────────────────────────────────────────────────
//
// The manifest declares a service; `pi.services.register` only supplies its
// handlers. The broker starts declared services after `load` returns and stops
// running ones before `onUnload`, so a service never outlives the API it used.

const SERVICE_BUDGET_MS = 5_000;
const SERVICE_LIMIT = 4;

function declaredServiceIds(explicit) {
  if (Array.isArray(explicit) && explicit.length) {
    return explicit.map((entry) =>
      typeof entry === "string" ? entry : String(entry?.id ?? ""),
    );
  }
  return [...runtime.serviceDeclarations.keys()];
}

async function startServices(params) {
  const results = [];
  for (const id of declaredServiceIds(params?.services).slice(0, SERVICE_LIMIT)) {
    if (!id) continue;
    const current = runtime.serviceRuntime.get(id);
    if (current && (current.state === "running" || current.state === "starting")) {
      results.push({ id, state: "running" });
      continue;
    }
    const service = runtime.services.get(id);
    if (!service || typeof service.start !== "function") {
      const message = `service "${id}" is not registered`;
      runtime.serviceRuntime.set(id, { state: "failed", message });
      results.push({ id, state: "failed", message });
      continue;
    }
    runtime.serviceRuntime.set(id, { state: "starting" });
    try {
      await withTimeout(
        Promise.resolve().then(() =>
          service.start({
            log: (message) => {
              process.stderr.write(`[plugin:service:${id}] ${String(message ?? "")}\n`);
            },
          }),
        ),
        SERVICE_BUDGET_MS,
        "SERVICE_START_TIMEOUT",
        `service "${id}" start`,
      );
      runtime.serviceRuntime.set(id, { state: "running" });
      results.push({ id, state: "running" });
    } catch (error) {
      const message = error?.message || String(error);
      runtime.serviceRuntime.set(id, { state: "failed", message });
      results.push({ id, state: "failed", message });
    }
  }
  return { services: results };
}

async function stopServices() {
  const results = [];
  for (const [id, status] of runtime.serviceRuntime) {
    if (status.state !== "running" && status.state !== "starting") continue;
    const service = runtime.services.get(id);
    if (service && typeof service.stop === "function") {
      try {
        await withTimeout(
          Promise.resolve().then(() => service.stop()),
          SERVICE_BUDGET_MS,
          "SERVICE_STOP_TIMEOUT",
          `service "${id}" stop`,
        );
      } catch (error) {
        // A failing stop must not block unload; the plugin is going away.
        process.stderr.write(
          `[plugin-host] service ${id} stop failed: ${error?.message || error}\n`,
        );
      }
    }
    runtime.serviceRuntime.set(id, { state: "stopped" });
    results.push({ id, state: "stopped" });
  }
  return { services: results };
}

async function unloadPlugin() {
  let warning;
  const loaded = runtime.module;
  // Services stop while the plugin still has its API, before onUnload.
  await stopServices();
  // Extensions see their session end before the process drops the handlers.
  for (const sessionId of [...runtime.extensionSessions]) {
    try {
      await withTimeout(
        emitExtensionEvent("session_shutdown", { reason: "shutdown" }, sessionId, null),
        5_000,
        "EXTENSION_SHUTDOWN_TIMEOUT",
        "session_shutdown",
      );
    } catch (error) {
      reportExtensionDiagnostic(
        "*",
        "handler_error",
        error?.message || String(error),
        "session_shutdown",
        error,
      );
    }
  }
  runtime.extensionCommands.clear();
  runtime.extensionTools.clear();
  runtime.extensionFlags.clear();
  runtime.extensionEventHandlers.clear();
  runtime.extensionSessions.clear();
  runtime.extensionDiagnostics.clear();
  runtime.extensionEntries = [];
  runtime.extensionErrors.clear();
  runtime.extensionActive = null;
  if (loaded && typeof loaded.onUnload === "function") {
    try {
      await withTimeout(
        Promise.resolve().then(() => loaded.onUnload()),
        5_000,
        "PLUGIN_UNLOAD_FAILED",
        "onUnload",
      );
    } catch (error) {
      warning = error && error.message ? error.message : String(error);
    }
  }
  runtime.module = null;
  runtime.commands.clear();
  runtime.tools.clear();
  runtime.themes.clear();
  runtime.services.clear();
  runtime.serviceRuntime.clear();
  runtime.serviceDeclarations.clear();
  runtime.eventHandlers.clear();
  const reply = { ok: true };
  if (warning) reply.warning = warning;
  return reply;
}

async function runCommand(params) {
  const commandId = requireString(params?.commandId, "commandId");
  const command = runtime.commands.get(commandId);
  if (!command) {
    throw new HostError("COMMAND_NOT_FOUND", `command "${commandId}" is not registered`);
  }
  await command.run(params?.args);
  return { ok: true };
}

async function executeTool(params) {
  const toolName = requireString(params?.toolName, "toolName");
  const tool = runtime.tools.get(toolName);
  if (!tool) {
    throw new HostError("TOOL_NOT_FOUND", `tool "${toolName}" is not registered`);
  }
  const context = {
    sessionId: params?.sessionId,
    turnId: params?.turnId,
    mode: params?.mode,
    modelKey: params?.modelKey,
    thinkingLevel: params?.thinkingLevel,
    log: (message) => {
      hostCall("log", { message: String(message ?? "") }).catch(() => {});
    },
  };
  return toolSession.run(context, async () => {
    try {
      const content = await tool.execute(params?.args, context);
      return { ok: true, content: content === undefined ? null : content };
    } catch (error) {
      const message = error && error.message ? error.message : String(error);
      const errorCode =
        error && typeof error.code === "string" && error.code ? error.code : "TOOL_FAILED";
      return { ok: false, errorCode, content: { error: message } };
    }
  });
}

function updateSettings(params) {
  if (params?.values && typeof params.values === "object") {
    runtime.settings = { ...runtime.settings, ...params.values };
  }
  return { ok: true };
}

function listThemes() {
  return [...runtime.themes.values()].map((theme) => ({
    id: theme.id,
    themeId: theme.themeId,
    label: theme.label,
    base: theme.base,
  }));
}

async function dispatch(method, params) {
  switch (method) {
    case "load":
      return loadPlugin(params);
    case "unload":
      return unloadPlugin();
    case "runCommand":
      return runCommand(params);
    case "executeTool":
      return executeTool(params);
    case "runExtensionCommand":
      return runExtensionCommand(params);
    case "services.start":
      return startServices(params);
    case "services.stop":
      return stopServices();
    case "updateSettings":
      return updateSettings(params);
    case "themes":
      return { themes: listThemes() };
    default:
      throw new HostError("METHOD_NOT_FOUND", `unknown method ${method}`);
  }
}

// ── wire protocol ───────────────────────────────────────────────────────────

function reply(id, outcome) {
  if (outcome.ok) {
    writeLine({ jsonrpc: "2.0", id, result: outcome.result });
  } else {
    writeLine({
      jsonrpc: "2.0",
      id,
      error: { code: -32000, message: outcome.message, data: { errorCode: outcome.code } },
    });
  }
}

async function handleLine(line) {
  const text = line.trim();
  if (!text) return;
  let message;
  try {
    message = JSON.parse(text);
  } catch (error) {
    process.stderr.write(`[plugin-host] dropped malformed frame: ${error}\n`);
    return;
  }
  if (message.method && message.id !== undefined && message.id !== null) {
    try {
      reply(message.id, { ok: true, result: await dispatch(message.method, message.params) });
    } catch (error) {
      const code = error && typeof error.code === "string" ? error.code : "PLUGIN_ERROR";
      const messageText = error && error.message ? error.message : String(error);
      reply(message.id, { ok: false, code, message: messageText });
    }
    return;
  }
  if (message.id !== undefined && message.id !== null) {
    const pending = hostPending.get(message.id);
    if (!pending) return;
    hostPending.delete(message.id);
    if (message.error) {
      const code = message.error.data?.errorCode || "HOST_ERROR";
      pending(false, new HostError(code, String(message.error.message ?? "host error")));
    } else {
      pending(true, message.result);
    }
    return;
  }
  // Fire-and-forget parent → child frame for extension loop events (ADR 0040
  // frame shape). No id, no reply, no backpressure.
  if (message.method === "extension.event") {
    const params = message.params || {};
    const event = String(params.event ?? "");
    if (!event) return;
    const sessionId = params.sessionId ? String(params.sessionId) : undefined;
    const cwd =
      (typeof params.cwd === "string" && params.cwd) ||
      (runtime.workspace && runtime.workspace.path) ||
      runtime.root;
    const payload =
      params.payload && typeof params.payload === "object" ? { ...params.payload } : {};
    if (sessionId) payload.sessionId = sessionId;
    void (async () => {
      try {
        await ensureExtensionSession(sessionId, cwd);
        await emitExtensionEvent(event, payload, sessionId, cwd);
      } catch (error) {
        process.stderr.write(
          `[plugin-host] ${event} dispatch failed: ${error?.message || error}\n`,
        );
      }
    })();
    return;
  }
  if (message.method === "shutdown") {
    process.exit(0);
  }
}

// A thrown timer or a rejected promise must not take the host down: the
// plugin's own error is logged, the process (and the plugin) stays loaded.
process.on("uncaughtException", (error) => {
  process.stderr.write(`[plugin-host] uncaught exception: ${error?.stack || error}\n`);
});
process.on("unhandledRejection", (error) => {
  process.stderr.write(`[plugin-host] unhandled rejection: ${error?.stack || error}\n`);
});

const reader = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
reader.on("line", (line) => {
  void handleLine(line);
});
reader.on("close", () => process.exit(0));
