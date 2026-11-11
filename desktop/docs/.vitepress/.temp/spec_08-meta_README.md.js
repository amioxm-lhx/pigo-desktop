import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"Meta","description":"","frontmatter":{},"headers":[],"relativePath":"spec/08-meta/README.md","filePath":"spec/08-meta/README.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "spec/08-meta/README.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="meta" tabindex="-1">Meta <a class="header-anchor" href="#meta" aria-label="Permalink to &quot;Meta&quot;">​</a></h1><table tabindex="0"><thead><tr><th>Doc</th><th>Description</th></tr></thead><tbody><tr><td><a href="./decisions-log">decisions-log.md</a></td><td>Frozen detail decisions</td></tr><tr><td><a href="./open-questions">open-questions.md</a></td><td>Remaining non-blocking questions</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("spec/08-meta/README.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const README = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  README as default
};
