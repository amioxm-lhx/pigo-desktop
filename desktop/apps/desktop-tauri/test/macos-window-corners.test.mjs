import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { loadStyles } from "./helpers/styles.mjs";

const styles = await loadStyles();
const config = JSON.parse(
  await readFile(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
);

test("macOS rounds the frameless window and drops the radius in fullscreen", () => {
  // The native window is transparent (vibrancy), so the shell's own clip is
  // what gives the window its macOS corner — and macOS shapes the shadow from
  // the same alpha, so the two always agree.
  assert.match(
    styles,
    /:root\[data-platform="darwin"\] #root\s*\{\s*border-radius:\s*var\(--radius-sm\);/,
  );
  assert.match(
    styles,
    /:root\[data-platform="darwin"\]\[data-fullscreen="true"\] #root\s*\{\s*border-radius:\s*0;/,
  );
  // Every child region (sidebar glass, main pane, splash) is clipped by the
  // shell, so a rounded corner cannot be painted over.
  assert.match(styles, /#root[\s\S]*?overflow:\s*hidden/);
});

test("the fixed window-controls band does not paint over the top-right corner", () => {
  // The controls are position: fixed, so #root's overflow clip never reaches
  // them; without their own radius the opaque band squares off the corner.
  assert.match(
    styles,
    /:root\[data-platform="darwin"\] \.window-controls\s*\{\s*border-top-right-radius:\s*var\(--radius-sm\);\s*overflow:\s*hidden;/,
  );
  assert.match(
    styles,
    /:root\[data-platform="darwin"\]\[data-fullscreen="true"\] \.window-controls\s*\{\s*border-top-right-radius:\s*0;/,
  );
});

test("the native sidebar material is rounded to the same radius as the clip", () => {
  // The material sits behind the webview and is square on its own; without a
  // radius it fills the corner the clip just cut out, which reads as a square
  // window. Ten points matches --radius-sm (10px).
  const window = config.app.windows[0];
  assert.equal(window.windowEffects.effects[0], "sidebar");
  assert.equal(window.windowEffects.radius, 10);
  assert.equal(window.transparent, true);
});

test("the radius stays off every other platform", () => {
  assert.doesNotMatch(
    styles,
    /:root\[data-platform="win32"\] #root\s*\{[^}]*border-radius/,
  );
  assert.doesNotMatch(
    styles,
    /:root\[data-platform="linux"\] #root\s*\{[^}]*border-radius/,
  );
});
