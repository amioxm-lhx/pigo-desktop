import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"Plugin System","description":"","frontmatter":{},"headers":[],"relativePath":"spec/07-plugins/README.md","filePath":"spec/07-plugins/README.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "spec/07-plugins/README.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="plugin-system" tabindex="-1">Plugin System <a class="header-anchor" href="#plugin-system" aria-label="Permalink to &quot;Plugin System&quot;">​</a></h1><blockquote><p>Directory: <code>docs/spec/07-plugins</code></p></blockquote><p>Plugin authors should start with the task-oriented <a href="./../../plugin-development">zero-to-one development guide</a>. The documents in this directory define the normative contracts and implementation boundaries.</p><table tabindex="0"><thead><tr><th>Document</th><th>Description</th></tr></thead><tbody><tr><td><a href="./01-plugin-system">01-plugin-system.md</a></td><td>Plugin system overview</td></tr><tr><td><a href="./02-plugin-manifest-schema">02-plugin-manifest-schema.md</a></td><td>Manifest schema</td></tr><tr><td><a href="./03-plugin-api">03-plugin-api.md</a></td><td>Plugin API</td></tr><tr><td><a href="./04-plugin-security">04-plugin-security.md</a></td><td>Plugin security</td></tr><tr><td><a href="./05-plugin-lifecycle">05-plugin-lifecycle.md</a></td><td>Lifecycle</td></tr><tr><td><a href="./06-plugin-packaging">06-plugin-packaging.md</a></td><td>Packaging &amp; installation</td></tr><tr><td><a href="./07-plugin-marketplace">07-plugin-marketplace.md</a></td><td>Plugin marketplace</td></tr><tr><td><a href="./08-plugin-signing-updates">08-plugin-signing-updates.md</a></td><td>Signing &amp; updates</td></tr><tr><td><a href="./09-plugin-command-palette">09-plugin-command-palette.md</a></td><td>Command palette</td></tr><tr><td><a href="./10-plugin-devex">10-plugin-devex.md</a></td><td>Developer experience</td></tr><tr><td><a href="./11-plugin-storage-isolation">11-plugin-storage-isolation.md</a></td><td>Storage isolation</td></tr><tr><td><a href="./12-plugin-ipc-and-host-services">12-plugin-ipc-and-host-services.md</a></td><td>Host services &amp; IPC</td></tr><tr><td><a href="./13-plugin-permissions-matrix">13-plugin-permissions-matrix.md</a></td><td>Permissions matrix</td></tr><tr><td><a href="./14-plugin-roadmap">14-plugin-roadmap.md</a></td><td>Plugin roadmap</td></tr><tr><td><a href="./15-plugin-center">15-plugin-center.md</a></td><td>Plugin center (publishing side)</td></tr><tr><td><a href="./16-trusted-extensions">16-trusted-extensions.md</a></td><td>Trusted extensions (in-sidecar extension surface)</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("spec/07-plugins/README.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const README = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  README as default
};
