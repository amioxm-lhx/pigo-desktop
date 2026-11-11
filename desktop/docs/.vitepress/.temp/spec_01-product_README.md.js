import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"Product & Scope","description":"","frontmatter":{},"headers":[],"relativePath":"spec/01-product/README.md","filePath":"spec/01-product/README.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "spec/01-product/README.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="product-scope" tabindex="-1">Product &amp; Scope <a class="header-anchor" href="#product-scope" aria-label="Permalink to &quot;Product &amp; Scope&quot;">​</a></h1><table tabindex="0"><thead><tr><th>Doc</th><th>Description</th></tr></thead><tbody><tr><td><a href="./00-overview">00-overview.md</a></td><td>Product overview</td></tr><tr><td><a href="./01-product-scope">01-product-scope.md</a></td><td>In/out of scope</td></tr><tr><td><a href="./02-non-goals">02-non-goals.md</a></td><td>Explicit non-goals</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("spec/01-product/README.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const README = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  README as default
};
