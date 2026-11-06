import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const desktopRoot = join(here, "..");
const read = (relative) => readFileSync(join(desktopRoot, relative), "utf8");

const hostSource = read("src-tauri/src/plugin_host.js");
const hostRust = read("src-tauri/src/plugin_host.rs");
const bridgeRust = read("src-tauri/src/plugin_bridge.rs");

const bridgedChannels = [
  "fs.readText",
  "fs.stat",
  "fs.readRange",
  "fs.readPreview",
  "fs.openDefault",
  "fs.reveal",
  "fs.writeText",
  "fs.glob",
  "fs.list",
  "fs.remove",
  "fs.requestDirectory",
  "net.fetch",
  "shell.openExternal",
  "project.create",
  "models.list",
  "session.getLlmContext",
  "session.list",
  "session.get",
  "session.listMessages",
  "session.import",
  "session.importBatch",
  "session.rename",
  "session.delete",
  "clipboard.readText",
  "clipboard.writeText",
  "clipboard.getHistory",
  "agent.complete",
];

test("plugin Node APIs use the permission-gated host bridge", () => {
  for (const channel of bridgedChannels) {
    assert.ok(
      hostSource.includes(`bridgeCall("${channel}"`),
      `plugin_host.js should route ${channel} through bridgeCall`,
    );
  }
  assert.doesNotMatch(
    hostSource,
    /unsupported\("(fs\.|net\.|shell\.|project\.|models\.|session\.|clipboard\.|agent\.complete)/,
    "bridged APIs must not remain stubs",
  );
  assert.match(hostRust, /"bridge\.invoke"/);
  assert.match(
    bridgeRust,
    /pub\(crate\) async fn invoke_loaded/,
    "the host reuses the panel bridge for permissions and scopes",
  );
});

test("tool session identity travels with bridge calls", () => {
  assert.match(hostSource, /const toolSession = new AsyncLocalStorage\(\)/);
  assert.match(hostSource, /toolSession\.run\(context/);
  assert.match(hostSource, /toolSession\.getStore\(\)\?\.sessionId/);
});

test("channels with no shell equivalent stay explicitly unsupported", () => {
  assert.match(hostSource, /navigate: unsupported\("browser\.navigate"\)/);
  assert.match(hostSource, /invoke: unsupported\("desktop\.invoke"\)/);
});
