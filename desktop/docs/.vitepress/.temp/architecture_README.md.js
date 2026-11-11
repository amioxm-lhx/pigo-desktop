import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"Architecture budgets","description":"","frontmatter":{},"headers":[],"relativePath":"architecture/README.md","filePath":"architecture/README.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "architecture/README.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="architecture-budgets" tabindex="-1">Architecture budgets <a class="header-anchor" href="#architecture-budgets" aria-label="Permalink to &quot;Architecture budgets&quot;">​</a></h1><p>The scripts/check-architecture.mjs checker reports the source-file count, total physical line count, largest source files, and the &gt;500, &gt;1000, and &gt;2000 LOC threshold counts for tracked source under apps/, packages/, crates/, and scripts/. Generated output, dependency trees, and build directories are excluded.</p><p>The checker enforces these limits:</p><ul><li>apps/desktop/electron/main/index.ts is at most 1500 LOC.</li><li>apps/desktop/src/stores/app-store.ts is at most 1000 LOC.</li><li>A newly added ordinary TypeScript or TSX source file is at most 800 LOC, unless it has a reasoned entry under typescript in allowlist.json.</li><li>Every Rust source file above 1000 LOC has a reasoned entry under rust in allowlist.json.</li></ul><p>The Rust entries currently document legacy modules that were outside the follow-up split scope. New Rust domain files are expected to remain below the limit. An allowlist entry records an explicit debt boundary; it does not raise the general limit for future files.</p></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("architecture/README.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const README = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  README as default
};
