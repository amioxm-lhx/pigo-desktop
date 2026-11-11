import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"05. Security","description":"","frontmatter":{},"headers":[],"relativePath":"spec/05-security/README.md","filePath":"spec/05-security/README.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "spec/05-security/README.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="_05-security" tabindex="-1">05. Security <a class="header-anchor" href="#_05-security" aria-label="Permalink to &quot;05. Security&quot;">​</a></h1><blockquote><p>Directory: <code>docs/spec/05-security</code></p></blockquote><table tabindex="0"><thead><tr><th>Doc</th><th>Description</th></tr></thead><tbody><tr><td><a href="./01-security">01-security.md</a></td><td>Security baseline</td></tr><tr><td><a href="./02-remote-control-security">02-remote-control-security.md</a></td><td>Remote Agent Control security</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("spec/05-security/README.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const README = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  README as default
};
