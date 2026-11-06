import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const routerSrc = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), "../src-tauri/src/router.rs"),
  "utf8",
);

test("loadDev is not a raw plugin row", () => {
  // The renderer sends no path (Electron main opened the picker). A row in
  // PLUGIN_ROW_CHANNELS would forward those empty params straight to host-core,
  // which answers "path required" before the wrapped arm can run.
  const start = routerSrc.indexOf("const PLUGIN_ROW_CHANNELS");
  assert.ok(start >= 0, "PLUGIN_ROW_CHANNELS missing");
  const rows = routerSrc.slice(start, routerSrc.indexOf("];", start));
  assert.doesNotMatch(rows, /pi-desktop\/plugin\/loadDev/);
});

test("loadDev picks a folder before calling host-core", () => {
  const start = routerSrc.indexOf('"pi-desktop/plugin/loadDev" =>');
  assert.ok(start >= 0, "loadDev arm missing");
  const handler = routerSrc.slice(
    start,
    routerSrc.indexOf('"pi-desktop/plugin/settings/get"', start),
  );
  assert.match(handler, /native::pick_folders\(&app\)/);
  assert.match(handler, /return ok\(json!\(\{ "canceled": true \}\)\)/);
  assert.match(handler, /"plugins\.loadDev",\s*json!\(\{ "path":/);
  // The Node runtime must reconcile right after the registry commits.
  assert.match(handler, /crate::plugin_host::sync/);
});
