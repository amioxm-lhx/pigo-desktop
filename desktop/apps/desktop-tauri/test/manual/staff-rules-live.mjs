import { spawn } from "node:child_process";
import { existsSync, copyFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const dataDir = process.env.PI_DESKTOP_DATA_DIR ?? join(process.env.USERPROFILE, ".pi-agent-go-desktop");
const hostBinary =
  process.env.PI_LIVE_HOST ??
  fileURLToPath(new URL("../../../../target/release/pi-desktop-host-core.exe", import.meta.url));
const dbPath = join(dataDir, "pi.sqlite");
const stamp = new Date().toISOString().replace(/[:.]/g, "-");
const backupDir = join(dataDir, "refit-backups", "memory-" + stamp);
mkdirSync(backupDir, { recursive: true });
for (const suffix of ["", "-wal", "-shm"]) {
  if (existsSync(dbPath + suffix)) copyFileSync(dbPath + suffix, join(backupDir, "pi.sqlite" + suffix));
}
console.log("backup: " + backupDir);

function connect(child) {
  let buffer = "";
  const pending = new Map();
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
      if (message.id === undefined || !pending.has(message.id)) continue;
      const waiter = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) waiter.reject(new Error(JSON.stringify(message.error)));
      else waiter.resolve(message.result);
    }
  });
  child.stderr.on("data", (chunk) => process.stderr.write(chunk));
  return {
    call(method, params, timeoutMs = 60000) {
      const id = nextId++;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => { pending.delete(id); reject(new Error("timeout: " + method)); }, timeoutMs);
        pending.set(id, {
          resolve: (v) => { clearTimeout(timer); resolve(v); },
          reject: (e) => { clearTimeout(timer); reject(e); },
        });
        child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params: params ?? {} }) + "\n");
      });
    },
  };
}

const { PROTOCOL_VERSION } = await import("@pi-desktop/shared");

const PLAYBOOK = [
  "留言与发布的话术 v2（2026-09-21 重建）",
  "",
  "分两层，不要混：",
  "",
  "一、在别人的帖子和评论区（留言）",
  "唯一目标是让对方点开我们的主页：不是成交，也不是把话说完。三段、不超过四句：",
  "1. 用他自己的词接住他的处境，不复述他的观点，也不先夸。",
  "2. 给一条他今天就能动手的具体做法（一个动作、一张表、一个标记），不要给一整套方法论。",
  "3. 说清我是干什么的，用一句会让人想点开主页看看的话；最后问一个只有他本人能回答的问题。",
  "不提产品名、不放外链、不谈价格。",
  "",
  "二、在我们自己发布的回答和主贴里（发布）",
  "这是成交主战场，必须提产品、必须留联系方式。四段：",
  "1. 第一句直接回答那个问题，不铺垫、不自我介绍。",
  "2. 给出我们自己跑出来的真实数字：次数、结果、失败的地方。有截图就配截图。",
  "3. 说明我在做什么、怎么找我。",
  "4. 问一个只有他本人能回答的问题。",
  "",
  "禁止：不对不同的人发同一段话；不夸大我们没有验证过的效果（没发出去就不要说发出去了）；不在别人的评论区放外链。",
].join("\n");

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

const child = spawn(hostBinary, [], {
  env: { ...process.env, PI_DESKTOP_DATA_DIR: dataDir },
  stdio: ["pipe", "pipe", "pipe"],
});
const host = connect(child);

try {
  await host.call("app.handshake", { protocolVersion: PROTOCOL_VERSION });

  const found = await host.call("staff.memory.query", { scope: "shared", kind: "playbook", limit: 50 });
  for (const item of found.items ?? []) {
    console.log("playbook " + item.id + " key=" + item.memoryKey + " status=" + item.status);
    const replaced = await host.call("staff.memory.supersede", {
      id: item.id,
      scope: "shared",
      kind: "playbook",
      key: item.memoryKey || "outreach/playbook",
      sourceKind: "user",
      confidence: 0.8,
      text: PLAYBOOK,
    });
    console.log("  superseded -> " + replaced.item.id + " v" + replaced.item.version);
  }

  const icp = await host.call("staff.memory.publish", {
    scope: "shared",
    kind: "fact",
    key: "persona/icp",
    sourceKind: "user",
    confidence: 0.9,
    text: ICP,
  });
  console.log("icp published: " + icp.item.id + " key=" + icp.item.memoryKey);

  const check = await host.call("staff.memory.query", { scope: "shared", limit: 50 });
  for (const item of (check.items ?? []).filter((i) => i.status === "active")) {
    console.log("active  " + String(item.kind).padEnd(10) + " " + String(item.memoryKey).padEnd(20) + String(item.text).slice(0, 40).replace(/\n/g, " / "));
  }
} finally {
  child.stdin.end();
  setTimeout(() => child.kill(), 1500);
}
