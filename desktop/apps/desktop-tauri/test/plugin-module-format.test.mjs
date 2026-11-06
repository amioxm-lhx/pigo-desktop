import assert from "node:assert/strict";
import test from "node:test";
import { fork } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { register } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

/**
 * Plugin module format is the plugin's decision (ADR 0008).
 *
 * Node classifies a `.js` file as ESM when any package.json above it says
 * `"type": "module"`. The unpacked build tree sits inside this checkout, whose
 * `apps/desktop/package.json` says exactly that, so a bundled CommonJS plugin
 * was loaded as ESM, lost `require`, and failed to restore with "require is not
 * defined in ES module scope" — while the same plugin worked in the installed
 * layout. Only a package.json inside the plugin directory may speak for the
 * plugin; anything else is decided by the plugin's own source.
 */

const here = dirname(fileURLToPath(import.meta.url));
const desktopRoot = join(here, "..");
const hostProcessEntry = join(desktopRoot, "electron/main/plugin-host-process.mjs");

register(pathToFileURL(join(here, "helpers/ts-import-hooks.mjs")));
const { PluginRuntime } = await import("../electron/main/plugin-runtime.ts");

/** Real host process, forked instead of Electron's utilityProcess. */
function forkPluginProcess({ entry }) {
  const child = fork(entry, [], { stdio: ["ignore", "pipe", "pipe", "ipc"] });
  return {
    postMessage: (message) => {
      if (child.connected) child.send(message);
    },
    onMessage: (handler) => child.on("message", handler),
    onExit: (handler) => child.on("exit", (code) => handler(code ?? 0)),
    kill: () => child.kill(),
  };
}

function createRuntime(t) {
  const runtime = new PluginRuntime({
    hostEntry: hostProcessEntry,
    spawnProcess: forkPluginProcess,
    audit: () => {},
  });
  t.after(async () => {
    for (const loaded of runtime.listLoaded()) await runtime.unload(loaded.manifest.id);
  });
  return runtime;
}

/**
 * Build `<root>/plugins/demo.format/{manifest.json,main.js}` with an optional
 * package.json above the plugin directory and one inside it.
 */
function writePluginFixture({ id, main, above, own }) {
  const root = mkdtempSync(join(tmpdir(), "pi-plugin-format-"));
  if (above) writeFileSync(join(root, "package.json"), JSON.stringify({ type: above }));
  const dir = join(root, "plugins", id);
  mkdirSync(dir, { recursive: true });
  if (own) writeFileSync(join(dir, "package.json"), JSON.stringify({ type: own }));
  writeFileSync(
    join(dir, "manifest.json"),
    JSON.stringify({
      schemaVersion: 1,
      id,
      name: id,
      version: "0.0.1",
      main: "main.js",
    }),
  );
  writeFileSync(join(dir, "main.js"), main, "utf8");
  return dir;
}

async function runCommand(runtime, commandId) {
  const command = runtime.getCommands().find((entry) => entry.id === commandId);
  assert.ok(command, `command not registered: ${commandId}`);
  await command.run();
}

test("a CommonJS plugin loads under an ancestor that declares type module", async (t) => {
  const runtime = createRuntime(t);
  const dir = writePluginFixture({
    id: "demo.format",
    above: "module",
    main: `
      const path = require("node:path");
      module.exports = {
        async onLoad() {
          await pi.commands.register({
            id: "demo.format.probe",
            title: "Probe",
            run: () => pi.ui.showToast("cjs:" + path.join("a", "b")),
          });
        },
      };
    `,
  });

  await runtime.loadFromPath(dir);
  runtime.drainToasts();
  await runCommand(runtime, "demo.format.probe");
  // require() exists in the plugin's module scope: the plugin is CommonJS.
  assert.deepEqual(runtime.drainToasts(), [`cjs:${join("a", "b")}`]);
});

test("an ESM plugin loads with or without its own package.json", async (t) => {
  const runtime = createRuntime(t);
  const main = `
    export async function onLoad() {
      await pi.commands.register({
        id: "demo.format.probe",
        title: "Probe",
        run: () => pi.ui.showToast("esm:" + pi.plugin.getId()),
      });
    }
  `;
  for (const own of [undefined, "module"]) {
    const dir = writePluginFixture({ id: "demo.format", main, above: "module", own });
    await runtime.loadFromPath(dir);
    runtime.drainToasts();
    await runCommand(runtime, "demo.format.probe");
    assert.deepEqual(runtime.drainToasts(), ["esm:demo.format"]);
  }
});

test("a genuinely broken plugin still fails to load", async (t) => {
  const runtime = createRuntime(t);
  const dir = writePluginFixture({
    id: "demo.format",
    above: "module",
    // Parseable as neither format: the ESM fallback must not mask it.
    main: "module.exports = { async onLoad( { };",
  });
  await assert.rejects(() => runtime.loadFromPath(dir), /PLUGIN_LOAD_FAILED|SyntaxError|Unexpected/);
});
