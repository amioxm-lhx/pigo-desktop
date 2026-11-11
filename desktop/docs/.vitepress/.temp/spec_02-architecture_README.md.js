import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"Architecture & Engineering","description":"","frontmatter":{},"headers":[],"relativePath":"spec/02-architecture/README.md","filePath":"spec/02-architecture/README.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "spec/02-architecture/README.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="architecture-engineering" tabindex="-1">Architecture &amp; Engineering <a class="header-anchor" href="#architecture-engineering" aria-label="Permalink to &quot;Architecture &amp; Engineering&quot;">​</a></h1><table tabindex="0"><thead><tr><th>Doc</th><th>Description</th></tr></thead><tbody><tr><td><a href="./01-architecture">01-architecture.md</a></td><td>System architecture</td></tr><tr><td><a href="./02-tech-stack">02-tech-stack.md</a></td><td>Tech stack</td></tr><tr><td><a href="./03-repo-structure">03-repo-structure.md</a></td><td>Repository structure</td></tr><tr><td><a href="./04-documentation-site">04-documentation-site.md</a></td><td>Documentation site</td></tr><tr><td><a href="./05-remote-agent-control">05-remote-agent-control.md</a></td><td>Remote Agent Host and Gateway target architecture</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("spec/02-architecture/README.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const README = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  README as default
};
