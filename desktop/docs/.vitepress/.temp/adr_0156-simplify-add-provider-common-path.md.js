import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"ADR 0156: Simplify the Add-Provider Common Path","description":"","frontmatter":{},"headers":[],"relativePath":"adr/0156-simplify-add-provider-common-path.md","filePath":"adr/0156-simplify-add-provider-common-path.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "adr/0156-simplify-add-provider-common-path.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="adr-0156-simplify-the-add-provider-common-path" tabindex="-1">ADR 0156: Simplify the Add-Provider Common Path <a class="header-anchor" href="#adr-0156-simplify-the-add-provider-common-path" aria-label="Permalink to &quot;ADR 0156: Simplify the Add-Provider Common Path&quot;">​</a></h1><ul><li>Status: Accepted</li><li>Date: 2026-09-05</li><li>Deciders: PI-Desktop core</li><li>Updates ADR 0116 and ADR 0155</li></ul><h2 id="context" tabindex="-1">Context <a class="header-anchor" href="#context" aria-label="Permalink to &quot;Context&quot;">​</a></h2><p>The add-provider dialog stacked Service, Name, Base URL, API key, and API format before the model panes. Named Zhipu / Z.AI endpoints already know their name, URL, and wire format, so those extra fields made the first-run path look like a generic gateway form.</p><h2 id="decision" tabindex="-1">Decision <a class="header-anchor" href="#decision" aria-label="Permalink to &quot;Decision&quot;">​</a></h2><p>A new dialog starts with only <strong>Service</strong>. After a named endpoint is chosen (Zhipu / Z.AI API or Coding Plan, OpenCode Go), the common path is Service + API key, plus a one-line host summary. Custom endpoint then shows Name, Base URL, and API key. Name (named rows) and API format (custom rows) stay behind Advanced. OpenCode Go is a Service option, not an API-format option.</p><p>No stepper, vendor-card grid, or extra <code>apiStyle</code> values. Persistence, catalog matching, and Completions flags from ADR 0155 are unchanged.</p><h2 id="consequences" tabindex="-1">Consequences <a class="header-anchor" href="#consequences" aria-label="Permalink to &quot;Consequences&quot;">​</a></h2><ul><li>Adding a known service is pick + paste + choose models.</li><li>Custom OpenAI-compatible gateways keep the previous Name / URL / key contract, with API format still available in Advanced.</li><li>OpenCode Go is discoverable next to other named services.</li></ul></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("adr/0156-simplify-add-provider-common-path.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _0156SimplifyAddProviderCommonPath = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _0156SimplifyAddProviderCommonPath as default
};
