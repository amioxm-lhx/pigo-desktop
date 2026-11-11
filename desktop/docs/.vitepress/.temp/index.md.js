import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"PI-Desktop","titleTemplate":"Documentation for a local-first AI coding agent","description":"","frontmatter":{"layout":"home","title":"PI-Desktop","titleTemplate":"Documentation for a local-first AI coding agent","hero":{"name":"PI-Desktop","text":"Read the product as a system.","tagline":"A calm, inspectable desktop workspace for building with AI. Follow the boundaries, understand the contracts, and make changes with context.","actions":[{"theme":"brand","text":"Start with context","link":"/guide/"},{"theme":"alt","text":"Open the spec map","link":"/spec/README"}]},"features":[{"title":"Locate the boundary","details":"Start from product intent, then move through architecture, runtime, UX, security, delivery, and plugins."},{"title":"Read the contract","details":"Follow the Rust host core, pi sidecar, NDJSON RPC, storage ownership, and provider model system."},{"title":"Change with evidence","details":"Connect the relevant spec, ADR, implementation, E2E scenario, and release checklist."}]},"headers":[],"relativePath":"index.md","filePath":"index.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "index.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("index.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const index = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  index as default
};
