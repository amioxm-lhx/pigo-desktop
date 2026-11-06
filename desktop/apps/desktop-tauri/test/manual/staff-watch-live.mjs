/**
 * The return leg, end to end, against the running app.
 *
 * The last mile of the funnel used to be the owner's typing: read the replies
 * off the site and key them in. This is the proof that it is not any more --
 * it asks the resident watch for one pass, prints what that pass read, and
 * then reads the database to see whether the reading actually landed in the
 * tables the managers decide from.
 *
 * Usage (PowerShell), with the app running and CDP enabled:
 *   node apps/desktop/test/manual/staff-watch-live.mjs [--port 9222]
 *
 * Read-only except for the pass itself: a pass that finds nobody answered
 * writes no metric row and files no card, so running this twice is harmless.
 */
/**
 * The return leg, end to end, against the shipped app.
 *
 * Reads the watch status, forces one pass, and then reads the database: what
 * came back is only real if it landed in the tables the managers read.
 */
import { DatabaseSync } from "node:sqlite";
import { homedir } from "node:os";
import { join } from "node:path";

const PORT = 9222;
const pages = async () =>
  (await (await fetch("http://127.0.0.1:" + PORT + "/json/list")).json()).filter((t) => t.type === "page");

function open(url) {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    let nextId = 1;
    const pending = new Map();
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      const waiter = pending.get(message.id);
      if (!waiter) return;
      pending.delete(message.id);
      if (message.error) waiter.reject(new Error(JSON.stringify(message.error)));
      else waiter.resolve(message.result);
    });
    socket.addEventListener("error", reject);
    socket.addEventListener("open", () =>
      resolve({
        send(method, params = {}, timeoutMs = 240000) {
          const id = nextId++;
          return new Promise((res, rej) => {
            const timer = setTimeout(() => {
              pending.delete(id);
              rej(new Error("timeout " + method));
            }, timeoutMs);
            pending.set(id, {
              resolve: (v) => { clearTimeout(timer); res(v); },
              reject: (e) => { clearTimeout(timer); rej(e); },
            });
            socket.send(JSON.stringify({ id, method, params }));
          });
        },
        close() { socket.close(); },
      }),
    );
  });
}

async function attach(match) {
  for (let attempt = 0; attempt < 200; attempt++) {
    const target = (await pages()).find(match);
    if (target) return await open(target.webSocketDebuggerUrl);
    await new Promise((r) => setTimeout(r, 150));
  }
  throw new Error("no matching page target");
}

async function evaluate(client, expression, timeoutMs = 240000) {
  const result = await client.send(
    "Runtime.evaluate",
    { expression, awaitPromise: true, returnByValue: true },
    timeoutMs,
  );
  if (result.exceptionDetails) {
    throw new Error(JSON.stringify(result.exceptionDetails).slice(0, 400));
  }
  return result.result.value;
}

const renderer = await attach((t) => /out\/renderer\/index\.html/.test(t.url));
const channels = JSON.parse(await evaluate(renderer, "JSON.stringify(window.piDesktop.channels.invoke)", 30000));

const before = new DatabaseSync(join(homedir(), ".pi-agent-go-desktop", "pi.sqlite"), { readOnly: true });
const stamp = (db) => ({
  contents: db.prepare("SELECT views, replies FROM staff_contents WHERE status='published'").all(),
  leads: db.prepare("SELECT COUNT(*) n FROM staff_leads").get().n,
  metrics: db.prepare("SELECT day, label, views, replies FROM staff_metrics ORDER BY updated_at DESC LIMIT 5").all(),
});
console.log("BEFORE", JSON.stringify(stamp(before)));

const status = await evaluate(
  renderer,
  "window.piDesktop.invoke(" + JSON.stringify(channels.staffWatchStatus) + ")",
  30000,
);
console.log("STATUS", JSON.stringify(status));

const outcome = await evaluate(
  renderer,
  "window.piDesktop.invoke(" + JSON.stringify(channels.staffWatchRun) + ")",
  300000,
);
console.log("OUTCOME", JSON.stringify(outcome, null, 1));

renderer.close();

const after = new DatabaseSync(join(homedir(), ".pi-agent-go-desktop", "pi.sqlite"), { readOnly: true });
console.log("AFTER", JSON.stringify(stamp(after), null, 1));
console.log(
  "LEADS",
  JSON.stringify(
    after
      .prepare("SELECT substr(id,1,8) id, stage, site, author, quote, tags_json, note FROM staff_leads ORDER BY created_at DESC LIMIT 3")
      .all(),
    null,
    1,
  ),
);
