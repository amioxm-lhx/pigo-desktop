import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"Project Tracking","description":"","frontmatter":{},"headers":[],"relativePath":"project/README.md","filePath":"project/README.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "project/README.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="project-tracking" tabindex="-1">Project Tracking <a class="header-anchor" href="#project-tracking" aria-label="Permalink to &quot;Project Tracking&quot;">​</a></h1><ul><li>Historical project board (archived; last refreshed 2026-08-11 for the 0.5.x line): <a href="./BOARD"><code>BOARD.md</code></a></li><li>Documentation/code alignment audit: <a href="./2026-07-30-docs-code-audit">2026-07-30 audit</a></li><li>Plan implementation plan: <a href="./plan-mode-implementation-plan"><code>plan-mode-implementation-plan.md</code></a></li><li>GitHub Issues + Milestones: repository Issues page</li><li>GitHub Projects: create after adding the <code>project</code> token scope</li></ul></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("project/README.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const README = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  README as default
};
