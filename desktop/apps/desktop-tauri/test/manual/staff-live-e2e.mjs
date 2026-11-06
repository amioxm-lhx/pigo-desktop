/**
 * Live end-to-end for the staff pipeline: real host-core, real Go agent
 * sidecar, real provider.
 *
 * The unit and wire suites stop at the credential boundary. This script goes
 * one step further and spends a real model call, because the questions left
 * after those suites are the ones only a live provider can answer: does the
 * employee's own key authenticate the launch, does the compiled step reach the
 * model as the prompt the driver composed, and does the answer land back in the
 * run's session and outcome.
 *
 * It stands in for Electron main: it spawns host-core and the sidecar, answers
 * host.proxy the way the sidecar bridge does, and composes the prompt with the
 * driver's own composer (electron/main/runtime/staff-prompt.ts).
 *
 * Usage (PowerShell):
 *   $env:PI_BASE_URL="https://api.deepseek.com"
 *   $env:PI_MODEL_ID="deepseek-flash"
 *   $env:PI_API_KEY="sk-..."
 *   node apps/desktop/test/manual/staff-live-e2e.mjs
 *
 * The key is read from the environment only, stored in the scratch data
 * directory's SecretStore, and never printed: every report line masks it.
 * Set PI_LIVE_TOOLS=1 to hand the model host-core's real tool catalog instead
 * of an empty one; the default keeps the probe on the model path so a tool
 * permission prompt cannot stall it.
 */
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const { PROTOCOL_VERSION } = await import("@pi-desktop/shared");
const { composeStepPrompt, messageText } = await import(
  "../../electron/main/runtime/staff-prompt.ts",
);

const baseUrl = (process.env.PI_BASE_URL ?? "").trim();
const modelId = (process.env.PI_MODEL_ID ?? "").trim();
const apiKey = (process.env.PI_API_KEY ?? "").trim();
if (!baseUrl || !modelId || !apiKey) {
  console.error("PI_BASE_URL, PI_MODEL_ID and PI_API_KEY are all required.");
  process.exit(2);
}
/** Never print the key: reports carry this form only. */
const mask = (value) => value.slice(0, 6) + "…" + value.slice(-4);

const dataDir = mkdtempSync(join(tmpdir(), "pi-staff-live-"));
const hostBinary =
  process.env.PI_LIVE_HOST ??
  fileURLToPath(new URL("../../../../target/debug/pi-desktop-host-core.exe", import.meta.url));
const sidecarBinary =
  process.env.PI_LIVE_SIDECAR ??
  fileURLToPath(new URL("../../../../sidecar/build/pi-go-desktop-agent.exe", import.meta.url));

if (!existsSync(hostBinary)) {
  console.error("host-core binary not found: " + hostBinary);
  console.error("build it with: cargo build -p host-core --locked");
  process.exit(2);
}
if (!existsSync(sidecarBinary)) {
  console.log("building the agent sidecar...");
  const build = spawnSync(
    process.execPath,
    [fileURLToPath(new URL("../../../../scripts/build-agent.mjs", import.meta.url))],
    { stdio: "inherit" },
  );
  if (build.status !== 0 || !existsSync(sidecarBinary)) {
    console.error("sidecar build failed");
    process.exit(2);
  }
}

/** NDJSON JSON-RPC client over a child process's stdio. */
function connect(child) {
  let buffer = "";
  const pending = new Map();
  const replies = [];
  const notifications = [];
  const handlers = new Set();
  let nextId = 1;
  child.stdout.on("data", (chunk) => {
    buffer += chunk.toString("utf8");
    let index;
    while ((index = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, index);
      buffer = buffer.slice(index + 1);
      if (!line.trim()) continue;
      let message;
      try { message = JSON.parse(line); } catch { continue; }
      if (message.method && message.id !== undefined) {
        for (const handler of handlers) handler(message);
        continue;
      }
      if (message.method) { notifications.push(message); continue; }
      replies.push(message);
      if (!pending.has(message.id)) continue;
      const waiter = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) waiter.reject(new Error(JSON.stringify(message.error)));
      else waiter.resolve(message.result);
    }
  });
  child.stderr.on("data", () => {});
  return {
    replies,
    notifications,
    onRequest(handler) { handlers.add(handler); return () => handlers.delete(handler); },
    call(method, params, timeoutMs = 60000) {
      const id = nextId++;
      const payload = JSON.stringify({ jsonrpc: "2.0", id, method, params: params ?? {} }) + "\n";
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          pending.delete(id);
          reject(new Error("timeout: " + method));
        }, timeoutMs);
        pending.set(id, {
          resolve: (value) => { clearTimeout(timer); resolve(value); },
          reject: (error) => { clearTimeout(timer); reject(error); },
        });
        child.stdin.write(payload);
      });
    },
    send(message) { child.stdin.write(JSON.stringify(message) + "\n"); },
  };
}

const slept = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const ok = [];
const check = (label, condition, detail) => {
  if (!condition) throw new Error("FAILED: " + label + (detail ? " — " + detail : ""));
  ok.push(label);
};

const hostChild = spawn(hostBinary, [], {
  env: { ...process.env, PI_DESKTOP_DATA_DIR: dataDir },
  stdio: ["pipe", "pipe", "pipe"],
});
const host = connect(hostChild);
let sidecarChild = null;
let sidecar = null;

try {
  await host.call("app.handshake", { protocolVersion: PROTOCOL_VERSION });
  check("host-core handshake at protocol v" + PROTOCOL_VERSION, true);

  // The provider row deliberately holds no key: the employee's own credential
  // profile is the only place the live key lives.
  const providerReply = await host.call("providers.create", {
    name: "DeepSeek (live e2e)",
    type: "openai_compatible",
    protocol: "openai",
    baseUrl,
    authKind: "api_key",
    apiStyle: "chat_completions",
    defaultModelId: modelId,
    models: [{ id: modelId, contextWindow: 128000, maxTokens: 8192, thinkingLevels: ["off"] }],
  });
  const providerId = providerReply.provider.id;
  check(
    "the provider row carries no key of its own",
    providerReply.provider.hasSecret === false,
    "hasSecret=" + providerReply.provider.hasSecret,
  );

  const profileReply = await host.call("staff.profile.create", {
    label: "DeepSeek live profile",
    providerId,
    baseUrl,
    secretValue: apiKey,
  });
  const profileId = profileReply.profile.id;
  check(
    "the employee profile holds the key without echoing it",
    profileReply.profile.hasSecret === true &&
      !JSON.stringify(profileReply).includes(apiKey),
  );

  const graph = {
    nodes: [
      { id: "trigger1", kind: "trigger", config: { mode: "manual" }, position: { x: 0, y: 0 } },
      {
        id: "produce1",
        kind: "produce",
        title: "Draft the reply",
        instruction:
          "Compute 17 + 25. Reply with exactly one line and nothing else: " +
          "STEP RESULT: <the sum>. Do not use any tool.",
        config: { artifact: "live-probe" },
        position: { x: 260, y: 0 },
      },
    ],
    edges: [{ from: "trigger1", to: "produce1" }],
  };
  const workflowReply = await host.call("staff.workflow.create", {
    name: "Live probe",
    status: "active",
    graph,
  });
  const workflowId = workflowReply.workflow.id;
  check("the workflow compiles to version 1", workflowReply.workflow.latestVersion === 1);

  const employeeReply = await host.call("staff.employee.create", {
    name: "Scout",
    role: "runs the live probe",
    status: "running",
    credentialProfileId: profileId,
    providerId,
    modelId,
    workflowId,
    workflowVersion: 1,
    schedule: {},
    quota: { maxRunsPerDay: 1 },
  });
  const employeeId = employeeReply.employee.id;
  check("the employee row carries no key material", !JSON.stringify(employeeReply).includes(apiKey));

  const started = await host.call("staff.run.start", { employeeId, trigger: "manual" });
  const runId = started.run.id;
  const sessionId = started.run.sessionId;
  check("staff.run.start opened a session", Boolean(sessionId));
  const step = started.steps[0];
  check("the compiled step excludes the trigger and carries its instruction",
    started.steps.length === 1 && step.kind === "produce" && step.instruction.includes("17 + 25"));

  const binding = await host.call("staff.run.binding", { runId });
  check("staff.run.binding hands out the profile key", binding.apiKey === apiKey);
  check("the binding carries the endpoint and model",
    binding.baseUrl === baseUrl && binding.modelId === modelId);

  // ---- the real turn -----------------------------------------------------

  sidecarChild = spawn(sidecarBinary, [], { stdio: ["pipe", "pipe", "pipe"] });
  sidecar = connect(sidecarChild);
  await sidecar.call("sidecar.configure", { dataDir });

  const toolCatalog = [];
  sidecar.onRequest(async (message) => {
    const method = String(message.params?.method ?? "");
    const params = message.params?.params ?? {};
    try {
      if (method === "tools.list") {
        if (process.env.PI_LIVE_TOOLS === "1") {
          const catalog = await host.call("tools.list", {});
          toolCatalog.push(...(catalog.tools ?? []));
          sidecar.send({ jsonrpc: "2.0", id: message.id, result: catalog });
          return;
        }
        sidecar.send({ jsonrpc: "2.0", id: message.id, result: { tools: [] } });
        return;
      }
      if (method === "session.get") {
        sidecar.send({ jsonrpc: "2.0", id: message.id, result: await host.call("session.get", params) });
        return;
      }
      if (method === "tools.execute" || method === "tools.abort") {
        sidecar.send({ jsonrpc: "2.0", id: message.id, result: await host.call(method, params) });
        return;
      }
      sidecar.send({
        jsonrpc: "2.0",
        id: message.id,
        error: { code: -32601, message: "host method not allowed from sidecar: " + method },
      });
    } catch (error) {
      sidecar.send({
        jsonrpc: "2.0",
        id: message.id,
        error: { code: -32000, message: String(error) },
      });
    }
  });

  const content = composeStepPrompt({
    employeeName: employeeReply.employee.name,
    index: 0,
    stepCount: started.steps.length,
    step,
    notes: [],
  });
  const turn = await host.call("session.beginTurn", { sessionId, providerId, modelId });
  const turnId = turn.turnId;
  const userMessage = {
    id: "staff-" + runId + "-" + step.nodeId,
    role: "user",
    content,
    createdAt: new Date().toISOString(),
    status: "complete",
  };
  await host.call("session.appendMessage", { sessionId, message: userMessage, turnId });

  let assistantText = "";
  let usage = null;
  let failure = null;
  /** agent.event arrives as notifications; fold them into the turn's outcome. */
  const drainEvents = () => {
    for (const message of sidecar.notifications.splice(0)) {
      if (message.method !== "agent.event") continue;
      const envelope = message.params ?? {};
      const event = envelope.event ?? {};
      if (event.type === "message_end") {
        const text = messageText(event.message);
        if (text && (event.message?.role === "assistant" || !event.message?.role)) {
          assistantText = text;
        }
        if (event.message?.usage) usage = event.message.usage;
      }
      if (event.type === "error") failure = event.error?.code ?? "AGENT_ERROR";
    }
  };
  const accepted = await sidecar.call(
    "agent.prompt",
    {
      sessionId,
      mode: "agent",
      turnId,
      provider: {
        id: providerId,
        name: "DeepSeek (live e2e)",
        baseUrl,
        modelId,
        apiKey,
        authKind: "api_key",
        apiStyle: "chat_completions",
      },
      commandShell: { id: "windows-powershell", label: "Windows PowerShell", dialect: "powershell", available: true },
      scratchDir: join(dataDir, "scratch", sessionId),
      attachmentsDir: join(dataDir, "attachments"),
      content,
      userMessageId: userMessage.id,
      attachments: [],
    },
    300000,
  );
  check("the sidecar accepted the prompt", accepted.accepted === true);

  const deadline = Date.now() + 240000;
  while (!assistantText && !failure && Date.now() < deadline) {
    drainEvents();
    await slept(250);
  }
  drainEvents();
  check("the live model answered", Boolean(assistantText), failure ? "error " + failure : "no reply");
  check("the answer carries the composed step result", assistantText.includes("42"), assistantText.slice(0, 200));
  console.log("model reply: " + assistantText.trim());
  if (usage) console.log("usage: " + JSON.stringify(usage));

  // ---- recording ---------------------------------------------------------

  await host.call("staff.run.step", { runId, step: 1 });
  const assistantMessage = {
    id: "assistant-" + turnId,
    role: "assistant",
    content: assistantText,
    createdAt: new Date().toISOString(),
    status: "complete",
  };
  await host.call("session.appendMessage", { sessionId, message: assistantMessage, turnId });
  await host.call("staff.run.finish", { runId, status: "succeeded" });

  const runs = await host.call("staff.employee.runs", { employeeId, limit: 5 });
  check("the run recorded its outcome", runs.runs[0].status === "succeeded");
  check("the run records the profile that paid", runs.runs[0].credentialProfileId === profileId);

  const session = await host.call("session.get", { id: sessionId });
  const roles = (session.session?.messages ?? []).map((message) => message.role);
  check("the session keeps the question and the answer",
    roles.includes("user") && roles.includes("assistant"), roles.join(","));
  check("the transcript never stores the key", !JSON.stringify(session).includes(apiKey));

  const leaked = host.replies.filter((reply) => JSON.stringify(reply).includes(apiKey));
  check("only staff.run.binding carried the key",
    leaked.length === 1 && leaked[0].result?.apiKey === apiKey,
    leaked.length + " replies");

  console.log("");
  console.log("LIVE E2E OK: " + ok.length + " checks (key " + mask(apiKey) + ", model " + modelId + ")");
  for (const label of ok) console.log("  - " + label);
  if (process.env.PI_LIVE_TOOLS === "1") {
    console.log("  tool catalog offered: " + toolCatalog.length + " tools");
  }
} catch (error) {
  console.error("LIVE E2E FAILED:", error.message);
  console.error("last host replies:", JSON.stringify(host.replies.slice(-2)));
  console.error("last sidecar output:", JSON.stringify(sidecar?.notifications.slice(-3) ?? []));
  process.exitCode = 1;
} finally {
  try { sidecarChild?.stdin.end(); sidecarChild?.kill(); } catch {}
  try { hostChild.stdin.end(); hostChild.kill(); } catch {}
  await slept(300);
  try { rmSync(dataDir, { recursive: true, force: true }); } catch {}
}
