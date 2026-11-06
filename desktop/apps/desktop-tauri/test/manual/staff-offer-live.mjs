/**
 * Put the numbers about our own product into shared memory.
 *
 * The last two runs of this company lost their whole day to the same missing
 * input: every employee that touches a customer reached the point of writing
 * "what we sell, what it costs, how to try it" and had nothing to write. The
 * closer said it out loud -- "须先补产品名/试用期/成本三类真实数字" -- and then
 * produced nothing. A shop with no price list cannot quote, and an answer with
 * no numbers is an opinion.
 *
 * So this is the price list. It is published the same way an employee's
 * finding would be, through the product's own memory gate, because that is
 * how every employee will read it back.
 *
 * Usage (PowerShell):
 *   node apps/desktop/test/manual/staff-offer-live.mjs
 */
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
const backupDir = join(dataDir, "refit-backups", "offer-" + stamp);
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
          resolve: (value) => { clearTimeout(timer); resolve(value); },
          reject: (error) => { clearTimeout(timer); reject(error); },
        });
        child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params: params ?? {} }) + "\n");
      });
    },
  };
}

/// What we sell, with the numbers an answer needs to be worth reading.
const OFFER_FACTS = [
  "我方产品与真实数字（业主口述，2026-09-21）",
  "",
  "产品名：PI-Agent-Go-Desktop",
  "它是什么：装在你自己的 Windows 电脑上的一人公司控制台，里面有一支九人编制的 AI 团队。",
  "",
  "编制（对外可以点名，都是真实存在的员工）：侦察员、选题员、笔杆子、地推员、接线员、跟单员、售后员，外加两名中层：线索主管、内容主编。",
  "",
  "可以直接对外说的数字（都是真的）：",
  "- 九个员工，每个员工配自己的一把 API key、自己选的模型、自己的排期。",
  "- 员工之间共享一份记忆：上一轮抓回来的原话、结论、定稿，下一轮所有人开工前都读得到。",
  "- 数据全部落在本机 SQLite，不上传任何服务器；没有多租户，没有云账号，不用把数据交出边界。",
  "- 中层不用逐条请示老板就能派活，但有每日上限，默认 5 件活/天。",
  "- 每个员工的工作流是一张可视化节点图，可以自己改、可以锁版本。",
  "- 排期按天/按分钟，应用开着就常驻跑。",
  "- 从一线原话到成交，五个看板：商机池、内容工厂、战报、弹药库、AI 团队。",
  "",
  "价格（业主 2026-09-21 定，改这一处全场跟着改）：",
  "- 早鸟买断 ¥1980，前 50 名；之后正价 ¥2980。",
  "- 先演示后付款：我拿十分钟给你看它跑一轮，你觉得它真能干活再谈钱。",
  "",
  "怎么找我：知乎私信我，或者在我主页留言。",
].join("\n");

/// The three things an employee must never say, because they are not true.
const OFFER_LIMITS = [
  "对外说话的三条硬边界（业主 2026-09-21 定）",
  "",
  "1. 不要说「它能自动帮你发帖/发私信/发邮件」。系统不代替任何人发布，它把成稿和目标位置准备好，最后那一下是人点的。说了就是骗。",
  "2. 不要承诺任何效果数字：不保证曝光、不保证涨粉、不保证成交。能给的只有「你自己的数据留在你自己的机器上」这类确定的事。",
  "3. 不要报价之外的数字：没有「原价 9980 现价 2980」这种话，价格就是价格。",
  "另外：对方的预算、公司名、联系方式，没经他同意不要写进任何对外内容。",
].join("\n");

const { PROTOCOL_VERSION } = await import("@pi-desktop/shared");
const child = spawn(hostBinary, [], {
  env: { ...process.env, PI_DESKTOP_DATA_DIR: dataDir },
  stdio: ["pipe", "pipe", "pipe"],
});
const host = connect(child);

try {
  await host.call("app.handshake", { protocolVersion: PROTOCOL_VERSION });

  const existing = await host.call("staff.memory.query", { scope: "shared", limit: 100 });
  const live = (item) => item.status === "active";
  for (const key of ["offer/facts", "offer/limits"]) {
    const clash = (existing.items ?? []).filter((item) => item.status !== "retired" && item.key === key);
    for (const item of clash) {
      await host.call("staff.memory.retire", { id: item.id });
      console.log("retired the previous " + key + " (" + item.id.slice(0, 8) + ")");
    }
  }

  const facts = await host.call("staff.memory.publish", {
    scope: "shared",
    kind: "fact",
    key: "offer/facts",
    text: OFFER_FACTS,
    sourceKind: "user",
    sourceRef: "业主口述 2026-09-21",
    confidence: 1,
  });
  console.log("published offer/facts " + facts.item.id);

  const limits = await host.call("staff.memory.publish", {
    scope: "shared",
    kind: "constraint",
    key: "offer/limits",
    text: OFFER_LIMITS,
    sourceKind: "user",
    sourceRef: "业主口述 2026-09-21",
    confidence: 1,
  });
  console.log("published offer/limits " + limits.item.id);

  const after = await host.call("staff.memory.query", { scope: "shared", limit: 50 });
  console.log("\nshared memory now:");
  for (const item of after.items ?? []) {
    console.log("  [" + item.kind + "] " + item.key + "  " + String(item.text).split("\n")[0].slice(0, 48));
  }
} finally {
  child.stdin.end();
  setTimeout(() => child.kill(), 1500);
}
