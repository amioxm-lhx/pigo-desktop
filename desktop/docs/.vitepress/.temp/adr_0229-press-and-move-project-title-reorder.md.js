import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"ADR 0229: Press-and-move project title reorder","description":"","frontmatter":{},"headers":[],"relativePath":"adr/0229-press-and-move-project-title-reorder.md","filePath":"adr/0229-press-and-move-project-title-reorder.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "adr/0229-press-and-move-project-title-reorder.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="adr-0229-press-and-move-project-title-reorder" tabindex="-1">ADR 0229: Press-and-move project title reorder <a class="header-anchor" href="#adr-0229-press-and-move-project-title-reorder" aria-label="Permalink to &quot;ADR 0229: Press-and-move project title reorder&quot;">​</a></h1><ul><li>Status: Accepted</li><li>Date: 2026-09-11</li><li>Amends: <a href="./0228-long-press-project-title-reorder">ADR 0228</a></li><li>Related: <a href="./../spec/08-meta/decisions-log">D403</a> · <a href="./../spec/04-ux/08-component-spec">Component spec</a> · E2E-253</li></ul><h2 id="context" tabindex="-1">Context <a class="header-anchor" href="#context" aria-label="Permalink to &quot;Context&quot;">​</a></h2><p>ADR 0228 removed the reorder grip and armed a drag after a 400ms still press. That delay is a mobile long-press pattern. On a desktop sidebar it makes reorder slower than ChatGPT, Claude, and similar product lists, where grabbing a row and moving it starts the drag immediately.</p><h2 id="decision" tabindex="-1">Decision <a class="header-anchor" href="#decision" aria-label="Permalink to &quot;Decision&quot;">​</a></h2><p>The project title remains the reorder control, with no grip. Pointer disambiguation is movement, not time:</p><ul><li>Mouse and pen: an 8px move while pressed arms the drag. A click with no qualifying movement still selects the project and toggles collapse.</li><li>Touch presses do not start a reorder, so a one-finger pan can scroll the list. Keyboard ArrowUp/ArrowDown on the focused title remains available.</li><li>While dragging, an accent insertion line on the target group shows before/after placement from the pointer&#39;s vertical midpoint. Escape cancels. Persistence and pin/archive buckets are unchanged.</li></ul><h2 id="consequences" tabindex="-1">Consequences <a class="header-anchor" href="#consequences" aria-label="Permalink to &quot;Consequences&quot;">​</a></h2><ul><li>Desktop reorder matches common sidebar lists: press, move, drop.</li><li>Touch scrolling is not stolen by an accidental 8px pan on a title.</li><li>The 400ms still-press contract in ADR 0228 is replaced.</li></ul><h2 id="references" tabindex="-1">References <a class="header-anchor" href="#references" aria-label="Permalink to &quot;References&quot;">​</a></h2><ul><li><code>apps/desktop/src/components/Sidebar.tsx</code></li><li><code>apps/desktop/src/lib/sidebar-project-reorder.ts</code></li><li><code>apps/desktop/test/sidebar-project-reorder.test.mjs</code></li></ul></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("adr/0229-press-and-move-project-title-reorder.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _0229PressAndMoveProjectTitleReorder = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _0229PressAndMoveProjectTitleReorder as default
};
