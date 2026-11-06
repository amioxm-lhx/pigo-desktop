import { spawn } from "node:child_process";
import { existsSync, copyFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const dataDir = process.env.PI_DESKTOP_DATA_DIR ?? join(process.env.USERPROFILE, ".pi-agent-go-desktop");
const hostBinary = fileURLToPath(new URL("../../../../target/release/pi-desktop-host-core.exe", import.meta.url));
const dbPath = join(dataDir, "pi.sqlite");
const backupDir = join(dataDir, "refit-backups", "icp-" + new Date().toISOString().replace(/[:.]/g, "-"));
mkdirSync(backupDir, { recursive: true });
for (const suffix of ["", "-wal", "-shm"]) {
  if (existsSync(dbPath + suffix)) copyFileSync(dbPath + suffix, join(backupDir, "pi.sqlite" + suffix));
}
console.log("backup: " + backupDir);

function connect(child) {
  let buffer = ""; const pending = new Map(); let nextId = 1;
  child.stdout.on("data", (chunk) => {
    buffer += chunk.toString("utf8"); let index;
    while ((index = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, index); buffer = buffer.slice(index + 1);
      if (!line.trim()) continue;
      let m; try { m = JSON.parse(line); } catch { continue; }
      if (m.id === undefined || !pending.has(m.id)) continue;
      const w = pending.get(m.id); pending.delete(m.id);
      if (m.error) w.reject(new Error(JSON.stringify(m.error))); else w.resolve(m.result);
    }
  });
  child.stderr.on("data", (chunk) => process.stderr.write(chunk));
  return { call(method, params) { const id = nextId++;
    return new Promise((res, rej) => { pending.set(id, { resolve: res, reject: rej });
      child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params: params ?? {} }) + "\n"); }); } };
}

const { PROTOCOL_VERSION } = await import("@pi-desktop/shared");
const ICP = [
  "客户画像与 A 级判据（2026-09-21 重建）",
  "",
  "会买的只有一种人：自己就是决策人，并且已经在公开平台上替自己的生意说话的人。",
  "三个硬条件必须同时成立才算 A 级：",
  "1. 身份：一人公司 / 独立开发 / 独立顾问 / 小团队主理人。出现「我们老板」「我们公司」「领导要求」的，一律不是。",
  "2. 意图：他自己的原话里有一个第一人称的获客或提效动作——「我每周要写」「我的客户问我」「我一个人做不过来」。只有情绪和吐槽、没有动作的，不是。",
  "3. 可答性：我们手上有能回答这条问题的真实经验或真实数字。答不出来的，不是。",
  "",
  "不会买的三种人，不要为他们花额度：",
  "- 在职员工：没有预算也没有决策权，他痛的是老板，不是获客。",
  "- 被迫承受 AI 的人：会共鸣、会点赞，然后走掉。",
  "- 纯围观者：看热闹，不提问。",
].join("\n");

const child = spawn(hostBinary, [], { env: { ...process.env, PI_DESKTOP_DATA_DIR: dataDir }, stdio: ["pipe", "pipe", "pipe"] });
const host = connect(child);
try {
  await host.call("app.handshake", { protocolVersion: PROTOCOL_VERSION });
  const existing = await host.call("staff.memory.query", { scope: "shared", limit: 100 });
  const already = (existing.items ?? []).find((i) => i.key === "persona/icp" && i.status === "active");
  if (already) {
    console.log("persona/icp already active: " + already.id);
  } else {
    const icp = await host.call("staff.memory.publish", {
      scope: "shared",
      kind: "playbook",
      key: "persona/icp",
      sourceKind: "user",
      sourceRef: "user/2026-09-21/persona-rebuilt",
      confidence: 0.9,
      text: ICP,
    });
    console.log("persona/icp published: " + icp.item.id);
  }
  const check = await host.call("staff.memory.query", { scope: "shared", limit: 100 });
  console.log("--- active shared memory ---");
  for (const item of (check.items ?? []).filter((i) => i.status === "active")) {
    console.log(String(item.kind).padEnd(10) + " " + String(item.key).padEnd(20) + String(item.text).slice(0, 44).replace(/\n/g, " / "));
  }
} finally { child.stdin.end(); setTimeout(() => child.kill(), 1200); }
