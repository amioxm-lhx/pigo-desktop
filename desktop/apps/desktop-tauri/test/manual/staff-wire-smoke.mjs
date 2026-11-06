/**
 * Staff wire smoke test. Manual, not part of `node --test test/*.test.mjs`.
 *
 * The Rust tests call staff::handle directly, so this is the check that the
 * rpc/mod.rs dispatch arm, the JSON shapes, and the credential boundary line up
 * over the real NDJSON transport.
 *
 *   cargo build -p host-core --locked
 *   node apps/desktop/test/manual/staff-wire-smoke.mjs
 *
 * It runs against a scratch PI_DESKTOP_DATA_DIR, waits for one 5-minute
 * schedule slot to come due (that is the minimum cadence the host accepts), and
 * exits non-zero on the first failed check.
 */
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const dataDir = mkdtempSync(join(tmpdir(), "pi-staff-smoke-"));
const hostBinary =
  process.env.STAFF_SMOKE_HOST ??
  fileURLToPath(new URL("../../../../target/debug/pi-desktop-host-core.exe", import.meta.url));

const child = spawn(
  hostBinary,
  [],
  { env: { ...process.env, PI_DESKTOP_DATA_DIR: dataDir }, stdio: ["pipe", "pipe", "pipe"] },
);

let buffer = "";
const pending = new Map();
let nextId = 1;
const replies = [];

child.stdout.on("data", (chunk) => {
  buffer += chunk.toString("utf8");
  let index;
  while ((index = buffer.indexOf("\n")) >= 0) {
    const line = buffer.slice(0, index);
    buffer = buffer.slice(index + 1);
    if (!line.trim()) continue;
    let message;
    try { message = JSON.parse(line); } catch { continue; }
    replies.push(message);
    if (message.id == null || !pending.has(message.id)) continue;
    const { resolve, reject } = pending.get(message.id);
    pending.delete(message.id);
    if (message.error) reject(new Error(JSON.stringify(message.error)));
    else resolve(message.result);
  }
});
child.stderr.on("data", () => {});

function call(method, params, timeoutMs = 60000) {
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
}

const slept = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const ok = [];
const check = (label, condition) => {
  if (!condition) throw new Error("FAILED: " + label);
  ok.push(label);
};

try {
  await call("app.handshake", { protocolVersion: 11 });
  check("app.handshake", true);

  const provider = await call("providers.create", {
    name: "Smoke provider",
    type: "openai_compatible",
    protocol: "openai",
    baseUrl: "https://provider.invalid/v1",
    authKind: "api_key",
    secretValue: "sk-provider-key",
    defaultModelId: "smoke-model",
  });
  const providerId = provider.provider.id;
  check("providers.create", Boolean(providerId));

  const profile = await call("staff.profile.create", {
    label: "Smoke profile",
    providerId,
    baseUrl: "https://profile-override.invalid/v1",
    secretValue: "sk-profile-key",
  });
  check("staff.profile.create records a secret without echoing it",
    profile.profile.hasSecret === true && !JSON.stringify(profile).includes("sk-profile-key"));

  const graph = {
    nodes: [
      { id: "trigger1", kind: "trigger", config: { mode: "schedule" }, position: { x: 0, y: 0 } },
      { id: "probe1", kind: "probe", title: "Probe", instruction: "post a probe", config: { channel: "juejin" }, position: { x: 260, y: 0 } },
      { id: "memory1", kind: "memory", config: { operation: "read", scope: "shared", limit: 5 }, position: { x: 520, y: 0 } },
    ],
    edges: [
      { from: "trigger1", to: "probe1" },
      { from: "probe1", to: "memory1" },
    ],
  };

  const validation = await call("staff.workflow.validate", { graph });
  check("staff.workflow.validate accepts the graph", validation.valid === true && validation.steps === 2);

  const badGraph = { nodes: [{ id: "probe1", kind: "probe", config: { channel: "v2ex" } }], edges: [] };
  const rejected = await call("staff.workflow.validate", { graph: badGraph });
  check("a graph without a trigger is rejected with a reason",
    rejected.valid === false && Boolean(rejected.error));

  const workflow = await call("staff.workflow.create", { name: "Smoke flow", status: "active", graph });
  const workflowId = workflow.workflow.id;
  check("staff.workflow.create writes version 1", workflow.workflow.latestVersion === 1);

  const employee = await call("staff.employee.create", {
    name: "Scout",
    role: "finds leads",
    status: "running",
    credentialProfileId: profile.profile.id,
    providerId,
    modelId: "smoke-model",
    thinkingLevel: "low",
    workflowId,
    workflowVersion: 1,
    schedule: { everyMinutes: 5, jitterMinutes: 0 },
    quota: { maxRunsPerDay: 4 },
  });
  const employeeId = employee.employee.id;
  check("staff.employee.create schedules the next run", Boolean(employee.employee.nextRunAt));
  check("the employee row carries no key material", !JSON.stringify(employee).includes("sk-"));

  const started = await call("staff.run.start", { employeeId, trigger: "manual" });
  const runId = started.run.id;
  check("staff.run.start opens a session", Boolean(started.run.sessionId));
  check("compiled steps exclude the trigger", started.steps.length === 2);
  check("step config travels with the step", started.steps[0].config.channel === "juejin");

  const binding = await call("staff.run.binding", { runId });
  check("staff.run.binding returns the profile key", binding.apiKey === "sk-profile-key");
  check("the profile base URL overrides the provider", binding.baseUrl === "https://profile-override.invalid/v1");
  check("the binding carries provider, model, thinking level, and employee",
    binding.providerId === providerId && binding.modelId === "smoke-model" &&
    binding.thinkingLevel === "low" && binding.employeeId === employeeId);

  const wrote = await call("staff.memory.write", {
    scope: "employee",
    ownerId: employeeId,
    kind: "signal",
    key: "interest:juejin",
    text: "Juejin readers ask about self-hosting",
    sourceKind: "user",
    confidence: 0.7,
  });
  check("staff.memory.write stores the item", wrote.item.scope === "employee");

  const queried = await call("staff.memory.query", { scope: "employee", ownerId: employeeId });
  check("staff.memory.query reads it back", queried.items.length === 1);

  await call("staff.memory.publish", {
    scope: "shared",
    kind: "fact",
    key: "channel:juejin",
    text: "Juejin tolerates long-form technical posts",
    sourceKind: "user",
    sourceRef: "https://juejin.cn/post/1",
  });
  const shared = await call("staff.memory.query", { scope: "shared" });
  check("a published item is visible to every employee", shared.items.length === 1);

  let denied = false;
  try {
    await call("staff.memory.write", {
      scope: "shared", kind: "constraint", text: "only the user may write this",
      sourceKind: "run", sourceRef: "run-1", runId,
    });
  } catch { denied = true; }
  check("a run-scoped write cannot claim the user, and a run cannot write a constraint", denied);

  await call("staff.run.step", { runId, step: 1 });
  await call("staff.run.finish", { runId, status: "succeeded" });
  const runs = await call("staff.employee.runs", { employeeId, limit: 5 });
  check("the run records its outcome", runs.runs[0].status === "succeeded");

  console.log("waiting for the scheduled slot to come due...");
  let claims = [];
  for (let attempt = 0; attempt < 40 && claims.length === 0; attempt += 1) {
    await slept(10000);
    const batch = await call("staff.scheduler.claim", { limit: 4 });
    claims = batch.claims ?? [];
  }
  check("staff.scheduler.claim takes the due slot and opens its session",
    claims.length === 1 && Boolean(claims[0].sessionId));
  check("a claim carries the pinned workflow version and compiled steps",
    claims[0].workflowVersion === 1 && claims[0].steps.length === 2);
  const second = await call("staff.scheduler.claim", { limit: 4 });
  check("claiming twice cannot start the same slot", (second.claims ?? []).length === 0);

  const withProfileKey = replies.filter((reply) => JSON.stringify(reply).includes("sk-profile-key"));
  check("exactly one reply carries the profile key, and it is the binding reply",
    withProfileKey.length === 1 && withProfileKey[0].result.apiKey === "sk-profile-key");
  const withProviderKey = replies.filter((reply) => JSON.stringify(reply).includes("sk-provider-key"));
  check("no reply carries the provider key", withProviderKey.length === 0);

  console.log("SMOKE OK: " + ok.length + " checks");
  for (const label of ok) console.log("  - " + label);
} catch (error) {
  console.error("SMOKE FAILED:", error.message);
  console.error("last replies:", JSON.stringify(replies.slice(-2), null, 2));
  process.exitCode = 1;
} finally {
  child.stdin.end();
  child.kill();
  await slept(300);
  try { rmSync(dataDir, { recursive: true, force: true }); } catch {}
}
