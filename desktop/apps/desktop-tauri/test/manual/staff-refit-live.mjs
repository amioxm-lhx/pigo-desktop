/**
 * Apply this build's corrected blueprints and rules to an install already on
 * disk.
 *
 * The console never rewrites an employee on its own: a blueprint is data, not
 * a migration, so a corrected graph only reaches an existing employee when the
 * owner adopts it by name. This script is that adoption, driven through the
 * product's own RPC rather than by editing the database behind its back, plus
 * the two rule changes that live in memory rather than in a graph.
 *
 * Usage (PowerShell):
 *   node apps/desktop/test/manual/staff-refit-live.mjs
 */
import { spawn } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const dataDir = process.env.PI_DESKTOP_DATA_DIR ?? join(process.env.USERPROFILE, ".pi-agent-go-desktop");
const hostBinary =
  process.env.PI_LIVE_HOST ??
  fileURLToPath(new URL("../../../../target/release/pi-desktop-host-core.exe", import.meta.url));
if (!existsSync(hostBinary)) {
  console.error("build host-core first: cargo build --release -p host-core");
  process.exit(2);
}
const dbPath = join(dataDir, "pi.sqlite");
if (!existsSync(dbPath)) {
  console.error("no database at " + dbPath);
  process.exit(2);
}

// A copy of the whole database, not just the file: a WAL that was never
// checkpointed holds writes the main file does not.
const stamp = new Date().toISOString().replace(/[:.]/g, "-");
const backupDir = join(dataDir, "refit-backups", stamp);
mkdirSync(backupDir, { recursive: true });
for (const suffix of ["", "-wal", "-shm"]) {
  const from = dbPath + suffix;
  if (existsSync(from)) copyFileSync(from, join(backupDir, "pi.sqlite" + suffix));
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
      try {
        message = JSON.parse(line);
      } catch {
        continue;
      }
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
      const payload = JSON.stringify({ jsonrpc: "2.0", id, method, params: params ?? {} }) + "\n";
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          pending.delete(id);
          reject(new Error("timeout: " + method));
        }, timeoutMs);
        pending.set(id, {
          resolve: (value) => {
            clearTimeout(timer);
            resolve(value);
          },
          reject: (error) => {
            clearTimeout(timer);
            reject(error);
          },
        });
        child.stdin.write(payload);
      });
    },
  };
}

const { PROTOCOL_VERSION } = await import("@pi-desktop/shared");
const child = spawn(hostBinary, [], {
  env: { ...process.env, PI_DESKTOP_DATA_DIR: dataDir },
  stdio: ["pipe", "pipe", "pipe"],
});
const host = connect(child);

try {
  await host.call("app.handshake", { protocolVersion: PROTOCOL_VERSION });

  const before = await host.call("staff.presets.list", {});
  for (const preset of before.presets ?? []) {
    console.log(
      "preset " + preset.id.padEnd(16) + " installed=" + preset.installed + " changed=" + preset.changed,
    );
  }

  const adopted = await host.call("staff.presets.install", {
    ids: ["scout", "interestAnalyst", "writer", "poster"],
    adoptBlueprint: true,
  });
  console.log("adopted: " + JSON.stringify(adopted.updated ?? adopted));
  console.log("installed: " + JSON.stringify(adopted.installed ?? []));

  const after = await host.call("staff.presets.list", {});
  for (const preset of after.presets ?? []) {
    console.log(
      "after " + preset.id.padEnd(16) + " installed=" + preset.installed + " changed=" + preset.changed,
    );
  }
} finally {
  child.stdin.end();
  setTimeout(() => child.kill(), 1500);
}
