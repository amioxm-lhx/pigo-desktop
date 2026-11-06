import { spawn } from "node:child_process";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
const dataDir = join(process.env.USERPROFILE, ".pi-agent-go-desktop");
const hostBinary = fileURLToPath(new URL("../../../../target/release/pi-desktop-host-core.exe", import.meta.url));
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
  child.stderr.on("data", () => {});
  return { call(method, params) { const id = nextId++;
    return new Promise((res, rej) => { pending.set(id, { resolve: res, reject: rej });
      child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params: params ?? {} }) + "\n"); }); } };
}
const { PROTOCOL_VERSION } = await import("@pi-desktop/shared");
const child = spawn(hostBinary, [], { env: { ...process.env, PI_DESKTOP_DATA_DIR: dataDir }, stdio: ["pipe", "pipe", "pipe"] });
const host = connect(child);
try {
  await host.call("app.handshake", { protocolVersion: PROTOCOL_VERSION });
  const r = await host.call("staff.memory.query", { scope: "shared", limit: 5 });
  console.log(JSON.stringify(r.items?.[0], null, 1));
} finally { child.stdin.end(); setTimeout(() => child.kill(), 800); }
