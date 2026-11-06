/**
 * Put one finalized draft where the console cannot reach: the site itself.
 *
 * The company writes the piece, the owner approves it, and then the last click
 * -- the one that publishes under his real name -- had nowhere to happen. Nine
 * employees spent a day producing "ready to send, nothing sent". This script is
 * that missing click, done through the site's own comment box inside the
 * signed-in session the collector already uses.
 *
 * The collector window is re-used rather than a fresh one opened: the app
 * denies popups, and the sweep window is the only page that carries the site
 * cookies. That window is closed the moment the sweep returns, so the script
 * holds it open the way a slow page does -- an image whose load never finishes
 * defers the document load event, which is what the collector waits on. That
 * buys about a minute, enough to type into the comment editor and press the
 * site own publish button. Nothing is forged and no endpoint is called.
 *
 * The target page is read, never guessed: the sweep is pinned to the draft
 * target URL and unpinned again on the way out, so a scheduled sweep is
 * unaffected.
 *
 * Usage (PowerShell), with the app running and CDP enabled:
 *   node apps/desktop/test/manual/staff-publish-live.mjs --content fa56208e --url https://zhuanlan.zhihu.com/p/1997658098362508695
 *   node apps/desktop/test/manual/staff-publish-live.mjs --content fa56208e --url <url> --dry
 *
 * Without --dry this posts publicly, under the owner account, as soon as it
 * runs. It refuses to click unless the editor holds the draft.
 */
import { DatabaseSync } from "node:sqlite";
import { homedir } from "node:os";
import { join } from "node:path";

const args = process.argv.slice(2);
const flag = (name) => {
  const index = args.indexOf("--" + name);
  return index === -1 ? null : (args[index + 1] ?? "");
};
const CONTENT = flag("content");
const TARGET = flag("url");
const PORT = Number(flag("port") ?? 9222);
const DRY = args.includes("--dry");

if (!CONTENT || !TARGET) {
  console.error("usage: node staff-publish-live.mjs --content <id> --url <targetUrl> [--dry]");
  process.exit(2);
}

const db = new DatabaseSync(join(homedir(), ".pi-agent-go-desktop", "pi.sqlite"), { readOnly: true });
const row = db.prepare("SELECT id, title, body FROM staff_contents WHERE id LIKE ?").get(CONTENT + "%");
if (!row) throw new Error("no draft with id starting " + CONTENT);
const paragraphs = String(row.body).trim().split(/\n\s*\n/).map((part) => part.trim()).filter(Boolean);
if (paragraphs.length === 0) throw new Error("draft body is empty");
console.log("draft:", row.id, "|", row.title);
console.log("target:", TARGET, "| paragraphs:", paragraphs.length);

const origin = "file:///D:/software/PI-Agent-Go-Desktop/resources/app.asar/out/renderer/index.html";
const pages = async () => (await (await fetch("http://127.0.0.1:" + PORT + "/json/list")).json()).filter((target) => target.type === "page");

function open(webSocketUrl) {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(webSocketUrl);
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
        send(method, params = {}, timeoutMs = 30000) {
          const id = nextId++;
          return new Promise((res, rej) => {
            const timer = setTimeout(() => { pending.delete(id); rej(new Error("timeout " + method)); }, timeoutMs);
            pending.set(id, { resolve: (v) => { clearTimeout(timer); res(v); }, reject: (e) => { clearTimeout(timer); rej(e); } });
            socket.send(JSON.stringify({ id, method, params }));
          });
        },
        close() { socket.close(); },
      }),
    );
  });
}

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function attach(match) {
  for (let attempt = 0; attempt < 200; attempt++) {
    const target = (await pages()).find(match);
    if (target) return { id: target.id, client: await open(target.webSocketDebuggerUrl) };
    await wait(60);
  }
  throw new Error("no matching page target");
}

/** A navigation the collector starts tears the context down; that is not an error. */
async function evalRetry(client, expression, tries = 3) {
  for (let attempt = 0; attempt < tries; attempt++) {
    try {
      const result = await client.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true }, 25000);
      if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails).slice(0, 200));
      return result.result.value;
    } catch (error) {
      if (/Execution context was destroyed|Cannot find context|timeout/.test(String(error))) {
        await wait(200);
        continue;
      }
      throw error;
    }
  }
  return null;
}

const HOLD = "(() => {" +
  "if (window.__piHold) return 'holding';" +
  "window.__piHold = 0;" +
  "const again = () => { const img = new Image(); img.onerror = () => { window.__piHold++; again(); }; img.onload = () => { window.__piHold++; again(); }; img.src = 'https://www.zhihu.com:9/pi-hold.png?t=' + Math.random(); document.documentElement.appendChild(img); };" +
  "again(); return 'holding'; })()";

const FIND = "(() => {" +
  "const live = (el) => !!el.offsetParent;" +
  "const editor = [...document.querySelectorAll('[contenteditable=\"true\"]')].filter(live)[0];" +
  "const submit = [...document.querySelectorAll('button')].filter((b) => (b.innerText || '').trim() === '\u53d1\u5e03' && /Button--primary/.test(String(b.className)))[0];" +
  "if (!editor) { window.scrollTo(0, document.body.scrollHeight); return JSON.stringify({ ok: false }); }" +
  "editor.scrollIntoView({ block: 'center' });" +
  "const box = editor.getBoundingClientRect();" +
  "const sbox = submit ? submit.getBoundingClientRect() : null;" +
  "return JSON.stringify({ ok: !!submit, x: Math.round(box.x + box.width / 2), y: Math.round(box.y + box.height / 2), len: (editor.innerText || '').length," +
  " submit: submit ? { x: Math.round(sbox.x + sbox.width / 2), y: Math.round(sbox.y + sbox.height / 2), disabled: !!submit.disabled } : null, text: (editor.innerText || '').slice(0, 40) }); })()";

const renderer = await attach((target) => target.url.startsWith(origin));
const channels = JSON.parse(await evalRetry(renderer.client, "JSON.stringify(window.piDesktop.channels.invoke)"));
const call = (name, ...rest) =>
  evalRetry(renderer.client, "window.piDesktop.invoke(" + JSON.stringify(channels[name]) + rest.map((value) => "," + JSON.stringify(value)).join("") + ")");
const listed = await call("staffSitesList");
const previousSeed = (listed?.sites ?? []).find((site) => site.channel === "zhihu")?.seedUrl ?? "";

try {
  await call("staffSiteUpdate", "zhihu", { seedUrl: TARGET });
  const before = new Set((await pages()).map((target) => target.id));
  await evalRetry(renderer.client, "window.piDesktop.invoke(" + JSON.stringify(channels.staffSiteCollect) + ",'zhihu').catch(() => 0); 1");
  const sweep = await attach((target) => !before.has(target.id) && /zhihu\.com/.test(target.url));
  const client = sweep.client;
  const started = Date.now();

  for (let attempt = 0; attempt < 40; attempt++) {
    const held = await evalRetry(client, "location.hostname.indexOf('zhihu.com') !== -1 ? (" + HOLD + ") : 'no'");
    if (held === "holding") break;
    await wait(150);
  }
  console.log("window held open at +" + (Date.now() - started) + "ms");

  let found = null;
  for (let attempt = 0; attempt < 60; attempt++) {
    const raw = await evalRetry(client, FIND);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (parsed.ok) { found = parsed; break; }
    }
    await wait(250);
  }
  if (!found) throw new Error("the comment editor or its publish button never appeared");
  console.log("comment editor ready; it currently holds " + found.len + " characters");

  const click = async (x, y) => {
    await client.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
    await client.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 });
    await client.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 });
  };
  const key = async (name, code, virtualKeyCode, modifiers) => {
    await client.send("Input.dispatchKeyEvent", { type: "rawKeyDown", key: name, code, windowsVirtualKeyCode: virtualKeyCode, modifiers });
    await client.send("Input.dispatchKeyEvent", { type: "keyUp", key: name, code, windowsVirtualKeyCode: virtualKeyCode, modifiers });
  };

  await click(found.x, found.y);
  await wait(250);
  await key("a", "KeyA", 65, 2);
  await wait(120);
  await key("Backspace", "Backspace", 8, 0);
  await wait(400);
  for (let index = 0; index < paragraphs.length; index++) {
    if (index > 0) { await key("Enter", "Enter", 13, 8); await wait(250); }
    await client.send("Input.insertText", { text: paragraphs[index] });
    await wait(350);
  }

  const typed = JSON.parse((await evalRetry(client, FIND)) ?? "{}");
  if (!typed.ok) throw new Error("the publish button is not available; refusing to publish");
  if (typed.len < row.body.length - paragraphs.length - 4) throw new Error("the editor does not hold the draft; refusing to publish");
  if (typed.submit?.disabled) throw new Error("the publish button is disabled; refusing to publish");
  console.log("the site own editor holds the draft; publish button is live");

  if (DRY) {
    console.log("--dry: stopping here, nothing was published");
  } else {
    await click(typed.submit.x, typed.submit.y);
    await wait(3000);
    console.log("pressed the site publish button");
    const proof = await evalRetry(client, "JSON.stringify({ editorsEmpty: [...document.querySelectorAll('[contenteditable=\"true\"]')].every((el) => (el.innerText || '').trim().length === 0) })");
    console.log("editor after publish:", proof);
  }
  client.close();
} finally {
  await call("staffSiteUpdate", "zhihu", { seedUrl: previousSeed });
  renderer.client.close();
  console.log("target page unpinned; watchlist back to " + JSON.stringify(previousSeed));
}
