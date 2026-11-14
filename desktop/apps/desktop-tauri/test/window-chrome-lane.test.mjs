import assert from "node:assert/strict";
import test from "node:test";
import { loadStyles } from "./helpers/styles.mjs";

const styles = await loadStyles();
// Collapse the source's line wrapping so a selector and its body can be matched
// as one run of text; the partials are hand-wrapped for 80 columns.
const flat = styles.replace(/\s+/g, " ");

function block(selector) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return flat.match(new RegExp(`(?:^| )${escaped} \\{[^}]*\\}`))?.[0] ?? "";
}

function token(name) {
  return Number(flat.match(new RegExp(`(?:^| )${name}: (\\d+)px;`))?.[1]);
}

test("preview-mode header offsets itself by the chrome row's action lane", () => {
  /*
    Preview mode moves the shell's sidebar-toggle/new-task tiles onto the
    window's left edge at z-index 200, above the panel header. If the header
    keeps its 12px inset those tiles cover the first tab's icon, so every
    platform has to start the tabs after that lane — macOS additionally behind
    its hiddenInset traffic lights.
  */
  assert.equal(
    token("--ds-preview-action-lane-width"),
    56,
    "the shared lane token should stay one action group wide",
  );
  assert.match(
    flat,
    /\.app-shell\.work-panel-maximized \.work-panel-header \{ padding-left: calc\(8px \+ var\(--ds-preview-action-lane-width\)\)/,
    "the preview header must clear the two-tile action lane on every platform",
  );
  assert.match(
    flat,
    /\.app-shell\.work-panel-maximized:not\(\.sidebar-collapsed\) \.work-panel-header \{ padding-left: calc\(16px \+ var\(--ds-titlebar-nav-size\)\)/,
    "with the sidebar open the row carries a single tile, so the lane shrinks",
  );
  // macOS keeps its traffic-light inset on top of the same lane.
  assert.match(
    flat,
    /:root\[data-platform="darwin"\]:not\(\[data-fullscreen="true"\]\) \.app-shell\.work-panel-maximized\.sidebar-collapsed \.work-panel-header \{ padding-left: calc\(76px \+ var\(--ds-preview-action-lane-width\)\)/,
  );
  assert.match(
    flat,
    /:root\[data-platform="darwin"\]\[data-fullscreen="true"\] \.app-shell\.work-panel-maximized\.sidebar-collapsed \.work-panel-header \{ padding-left: calc\(8px \+ var\(--ds-preview-action-lane-width\)\)/,
  );
});

test("the lane token and the action tile cannot drift apart", () => {
  const tile = token("--ds-titlebar-nav-size");
  const lane = token("--ds-preview-action-lane-width");
  assert.ok(tile > 0, "the action tile size should be a token, not a literal");
  assert.match(
    block(".title-nav-btn"),
    /height: var\(--ds-titlebar-nav-size\)/,
    ".title-nav-btn should take its box from the shared tile token",
  );
  assert.match(block(".title-nav-btn"), /width: var\(--ds-titlebar-nav-size\)/);
  assert.match(
    block(".titlebar-nav"),
    /gap: 4px/,
    "the lane token is derived from this gap; keep them in sync",
  );
  // lane = two tiles + the row gap + an 8px buffer before the first tab.
  assert.equal(lane, tile * 2 + 4 + 8);
});
