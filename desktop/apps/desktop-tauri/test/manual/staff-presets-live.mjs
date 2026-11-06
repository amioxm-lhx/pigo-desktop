/**
 * Live acceptance for the built-in employees.
 *
 * The unit suites prove the blueprints parse and that a second install is a
 * no-op. This script proves the thing the owner actually asked for: on a fresh
 * data directory, with a real key, the console ends up staffed by two running
 * employees -- and the scout can take a real step against a real model.
 *
 * Usage (PowerShell):
 *   $env:PI_BASE_URL="https://api.deepseek.com"
 *   $env:PI_MODEL_ID="deepseek-flash"
 *   $env:PI_API_KEY="sk-..."
 *   node apps/desktop/test/manual/staff-presets-live.mjs
 */
import { spawn } from "node:child_process";
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
const mask = (value) => value.slice(0, 6) + "…" + value.slice(-4);

const dataDir = mkdtempSync(join(tmpdir(), "pi-staff-presets-"));
const hostBinary =
  process.env.PI_LIVE_HOST ??
  fileURLToPath(new URL("../../../../target/debug/pi-desktop-host-core.exe", import.meta.url));
const sidecarBinary =
  process.env.PI_LIVE_SIDECAR ??
  fileURLToPath(new URL("../../../../sidecar/build/pi-go-desktop-agent.exe", import.meta.url));
if (!existsSync(hostBinary) || !existsSync(sidecarBinary)) {
  console.error("build host-core and the agent first");
  process.exit(2);
}

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

  // A fresh data directory offers both blueprints, neither of them filled in.
  const before = await host.call("staff.presets.list", {});
  check("a fresh console offers two blueprints", before.presets.length === 2);
  check("nothing is staffed yet", before.presets.every((preset) => !preset.installed));
  const names = before.presets.map((preset) => preset.name);
  check(
    "the blueprints are the scout and the analyst",
    names.includes("侦察员") && names.includes("选题员"),
    names.join(", "),
  );

  const seeded = await host.call("staff.presets.install", {
    apiKey,
    baseUrl,
    modelId,
    providerName: "DeepSeek 预设",
  });
  check("both employees are hired", seeded.installed.length === 2, JSON.stringify(seeded.installed));
  check("the install answers with a credential profile", Boolean(seeded.credentialProfileId));
  check(
    "the key is not echoed back by the install",
    !JSON.stringify(seeded).includes(apiKey),
  );

  const employees = (await host.call("staff.employee.list", {})).employees;
  check("the console holds exactly two employees", employees.length === 2);
  const scout = employees.find((employee) => employee.name === "侦察员");
  const analyst = employees.find((employee) => employee.name === "选题员");
  check("the scout is on duty", scout?.status === "running");
  check("the analyst is on duty", analyst?.status === "running");
  check("the scout has a next slot", Boolean(scout.nextRunAt), String(scout.nextRunAt));
  check("the analyst has a next slot", Boolean(analyst.nextRunAt));
  check(
    "both employees run on the owner's own profile",
    scout.credentialProfileId === seeded.credentialProfileId &&
      analyst.credentialProfileId === seeded.credentialProfileId,
  );
  check(
    "the employee rows never hold the key",
    !JSON.stringify(employees).includes(apiKey),
  );

  // The scout sweeps the one site that has a collector, and its step says so:
  // naming a site nobody can read would send the employee off to fetch it by
  // hand, which is what the login wall stops.
  const scoutGraph = await host.call("staff.workflow.get", { id: scout.workflowId, version: 1 });
  const scan = JSON.stringify(scoutGraph);
  check("the scout sweeps zhihu", scan.includes("zhihu"));
  check(
    "and only the sites that have a collector",
    ["juejin", "bilibili", "wechatMp", "wechatChannels"].every(
      (channel) => !scan.includes(channel),
    ),
  );
  check(
    "the sweep step is written around what the driver collected",
    scan.includes("[collected:") && scan.includes("skipped"),
  );
  const probeStep = (scoutGraph.graph?.nodes ?? []).find((node) => node.kind === "probe");
  check("the probe step is still there", Boolean(probeStep));
  check("the probe step is scoped to one site", (probeStep.config?.sources ?? []).length === 1);

  // A second install must not hire anyone twice.
  const again = await host.call("staff.presets.install", { apiKey, baseUrl, modelId });
  check("a second install hires nobody", again.installed.length === 0, JSON.stringify(again.installed));
  check("a second install skips both", again.skipped.length === 2);
  check(
    "the console still holds two employees",
    (await host.call("staff.employee.list", {})).employees.length === 2,
  );

  // ---- one real step of the scout ---------------------------------------

  const started = await host.call("staff.run.start", { employeeId: scout.id, trigger: "manual" });
  const runId = started.run.id;
  const sessionId = started.run.sessionId;
  check("a manual run opens a session", Boolean(sessionId));
  const step = started.steps[0];
  check("the scout's first step is the sweep", step.kind === "probe", step.kind);

  const binding = await host.call("staff.run.binding", { runId });
  check("the binding resolves the seeded profile's key", binding.apiKey === apiKey);
  check("the binding carries the seeded endpoint", binding.baseUrl === baseUrl);

  sidecarChild = spawn(sidecarBinary, [], { stdio: ["pipe", "pipe", "pipe"] });
  sidecar = connect(sidecarChild);
  await sidecar.call("sidecar.configure", { dataDir });
  sidecar.onRequest(async (message) => {
    const method = String(message.params?.method ?? "");
    const params = message.params?.params ?? {};
    try {
      if (method === "tools.list") {
        sidecar.send({ jsonrpc: "2.0", id: message.id, result: { tools: [] } });
        return;
      }
      if (method === "session.get") {
        sidecar.send({ jsonrpc: "2.0", id: message.id, result: await host.call("session.get", params) });
        return;
      }
      sidecar.send({
        jsonrpc: "2.0",
        id: message.id,
        error: { code: -32601, message: "host method not allowed: " + method },
      });
    } catch (error) {
      sidecar.send({ jsonrpc: "2.0", id: message.id, error: { code: -32000, message: String(error) } });
    }
  });

  const content = composeStepPrompt({
    employeeName: scout.name,
    index: 0,
    stepCount: started.steps.length,
    step,
    notes: [],
  });
  check("the step prompt names the site it sweeps", content.includes("zhihu"), content.slice(0, 400));
  check(
    "and tells the employee to read what the driver collected",
    content.includes("no equivalent") && content.includes("[collected:"),
    content.slice(0, 400),
  );
  const turn = await host.call("session.beginTurn", { sessionId, providerId: scout.providerId, modelId });
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
  const drain = () => {
    for (const message of sidecar.notifications.splice(0)) {
      if (message.method !== "agent.event") continue;
      const event = message.params?.event ?? {};
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
        id: scout.providerId,
        name: "DeepSeek 预设",
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
  check("the sidecar accepted the probe step", accepted.accepted === true);
  const deadline = Date.now() + 240000;
  while (!assistantText && !failure && Date.now() < deadline) {
    drain();
    await slept(250);
  }
  drain();
  check("the seeded employee reached the model", Boolean(assistantText), failure ? "error " + failure : "no reply");
  console.log("model reply: " + assistantText.trim().slice(0, 400));
  if (usage) console.log("usage: " + JSON.stringify(usage));

  await host.call("staff.run.step", { runId, step: 1 });
  await host.call("session.appendMessage", {
    sessionId,
    message: {
      id: "assistant-" + turnId,
      role: "assistant",
      content: assistantText,
      createdAt: new Date().toISOString(),
      status: "complete",
    },
    turnId,
  });
  await host.call("staff.run.finish", { runId, status: "succeeded" });
  const runs = await host.call("staff.employee.runs", { employeeId: scout.id, limit: 5 });
  check("the scout's run recorded its outcome", runs.runs[0].status === "succeeded");
  check("the run records the seeded profile", runs.runs[0].credentialProfileId === seeded.credentialProfileId);

  const leaked = host.replies.filter((reply) => JSON.stringify(reply).includes(apiKey));
  check(
    "only staff.run.binding carried the key",
    leaked.length === 1 && leaked[0].result?.apiKey === apiKey,
    leaked.length + " replies",
  );

  console.log("");
  console.log("PRESETS LIVE OK: " + ok.length + " checks (key " + mask(apiKey) + ")");
  for (const label of ok) console.log("  - " + label);
} catch (error) {
  console.error("PRESETS LIVE FAILED:", error.message);
  console.error("last host replies:", JSON.stringify(host.replies.slice(-2)));
  process.exitCode = 1;
} finally {
  try { sidecarChild?.stdin.end(); sidecarChild?.kill(); } catch {}
  try { hostChild.stdin.end(); hostChild.kill(); } catch {}
  await slept(300);
  try { rmSync(dataDir, { recursive: true, force: true }); } catch {}
}
