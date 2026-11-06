import assert from "node:assert/strict";
import test from "node:test";
import { spawn } from "node:child_process";
import { copyFileSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import readline from "node:readline";

const here = dirname(fileURLToPath(import.meta.url));
const desktopRoot = join(here, "..");
const hostScript = join(desktopRoot, "src-tauri/src/plugin_host.js");

const PLUGIN_MAIN = `
  module.exports = {
    async onLoad() {
      await pi.agent.registerTool({
        name: "probe",
        description: "probe",
        schema: { type: "object", properties: {} },
        execute: async () => {
          const fsText = await pi.fs.readText("a.txt");
          const models = await pi.models.list();
          const context = await pi.session.getLlmContext();
          const completion = await pi.agent.complete({
            modelKey: "p/m",
            messages: [{ role: "user", content: "hi" }],
          });
          const history = await pi.clipboard.getHistory();
          return { fsText, models, context, completion, history };
        },
      });
    },
  };
`;

function startHost(scriptPath) {
  const child = spawn(process.execPath, [scriptPath], {
    stdio: ["pipe", "pipe", "pipe"],
  });
  const pending = new Map();
  const calls = [];
  let sequence = 1;

  const interface_ = readline.createInterface({ input: child.stdout });
  interface_.on("line", (line) => {
    const message = JSON.parse(line);
    if (message.method && message.id != null) {
      if (message.method === "bridge.invoke") {
        calls.push({
          channel: message.params?.channel,
          payload: message.params?.payload,
          sessionId: message.params?.sessionId,
        });
        const channel = message.params?.channel;
        let result = null;
        if (channel === "fs.readText") result = "hello";
        else if (channel === "models.list") result = [{ key: "p/m" }];
        else if (channel === "session.getLlmContext") {
          result = { sessionId: message.params?.sessionId ?? null };
        } else if (channel === "agent.complete") result = { text: "ok" };
        else if (channel === "clipboard.getHistory") result = [];
        child.stdin.write(
          `${JSON.stringify({ jsonrpc: "2.0", id: message.id, result })}\n`,
        );
        return;
      }
      const result =
        message.method === "settings.get"
          ? { values: {} }
          : message.method === "settings.set"
            ? { values: {} }
            : { ok: true };
      child.stdin.write(
        `${JSON.stringify({ jsonrpc: "2.0", id: message.id, result })}\n`,
      );
      return;
    }
    if (message.id != null) {
      const entry = pending.get(message.id);
      if (!entry) return;
      pending.delete(message.id);
      if (message.error) entry.reject(message.error);
      else entry.resolve(message.result);
    }
  });

  function request(method, params) {
    return new Promise((resolve, reject) => {
      const id = sequence++;
      pending.set(id, { resolve, reject });
      child.stdin.write(
        `${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`,
      );
    });
  }

  return {
    request,
    calls,
    close: () => {
      interface_.close();
      child.kill();
    },
  };
}

test("plugin Node APIs reach the host bridge with the tool session", { timeout: 15000 }, async (t) => {
  const root = mkdtempSync(join(tmpdir(), "pi-node-api-plugin-"));
  // The embedded host runs from the data directory, outside the app package
  // whose package.json declares `"type": "module"`. Mirror that layout or Node
  // treats the CommonJS host script as ESM.
  const scriptCopy = join(root, "plugin_host.js");
  copyFileSync(hostScript, scriptCopy);
  writeFileSync(
    join(root, "manifest.json"),
    JSON.stringify({
      schemaVersion: 1,
      id: "demo.node",
      name: "Node API",
      version: "0.0.1",
      main: "main.js",
      permissions: [
        "agent.tool.register",
        "fs.read",
        "models.list",
        "session.read",
        "agent.complete",
        "clipboard.read",
      ],
    }),
    "utf8",
  );
  writeFileSync(join(root, "main.js"), PLUGIN_MAIN, "utf8");

  const host = startHost(scriptCopy);
  t.after(() => {
    host.close();
    rmSync(root, { recursive: true, force: true });
  });

  const manifest = JSON.parse(
    readFileSync(join(root, "manifest.json"), "utf8"),
  );
  await host.request("load", {
    pluginId: "demo.node",
    root,
    manifest,
    settings: {},
    app: { version: "0", locale: "en" },
  });

  const result = await host.request("executeTool", {
    toolName: "probe",
    args: {},
    sessionId: "s1",
    turnId: "t1",
    mode: "agent",
  });
  assert.equal(result.ok, true, JSON.stringify(result));
  assert.equal(result.content.fsText, "hello");
  assert.deepEqual(result.content.models, [{ key: "p/m" }]);
  assert.deepEqual(result.content.context, { sessionId: "s1" });
  assert.deepEqual(result.content.completion, { text: "ok" });
  assert.deepEqual(result.content.history, []);
  assert.deepEqual(
    host.calls.map((entry) => entry.channel).sort(),
    [
      "agent.complete",
      "clipboard.getHistory",
      "fs.readText",
      "models.list",
      "session.getLlmContext",
    ],
  );
  for (const call of host.calls) {
    assert.equal(call.sessionId, "s1", `${call.channel} should carry the tool session`);
  }
});
