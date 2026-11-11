import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"ADR 0082: Localized and page-adaptive plugin panel chrome","description":"","frontmatter":{},"headers":[],"relativePath":"adr/0082-localized-plugin-panel-chrome.md","filePath":"adr/0082-localized-plugin-panel-chrome.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "adr/0082-localized-plugin-panel-chrome.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="adr-0082-localized-and-page-adaptive-plugin-panel-chrome" tabindex="-1">ADR 0082: Localized and page-adaptive plugin panel chrome <a class="header-anchor" href="#adr-0082-localized-and-page-adaptive-plugin-panel-chrome" aria-label="Permalink to &quot;ADR 0082: Localized and page-adaptive plugin panel chrome&quot;">​</a></h1><h2 id="status" tabindex="-1">Status <a class="header-anchor" href="#status" aria-label="Permalink to &quot;Status&quot;">​</a></h2><p>Accepted</p><h2 id="context" tabindex="-1">Context <a class="header-anchor" href="#context" aria-label="Permalink to &quot;Context&quot;">​</a></h2><p>Plugin panels are hosted in their own Electron windows. Their titlebar was given a single string from the manifest and the preload chrome defaulted to a dark surface. That split the panel from the active PI-Desktop language and made light plugin pages look like they had an unrelated black header.</p><h2 id="decision" tabindex="-1">Decision <a class="header-anchor" href="#decision" aria-label="Permalink to &quot;Decision&quot;">​</a></h2><ol><li><code>ui.title</code> accepts either the existing string form or a localized object containing both <code>en</code> and <code>zh-CN</code> strings. The host resolves the value using the active PI-Desktop UI locale and falls back to the other supplied label, then the manifest name.</li><li>The host passes the active light/dark theme to the panel window. The preload samples the loaded document&#39;s computed body/document background and foreground colors for the titlebar, using the host theme when the page is transparent.</li><li>The titlebar remains in the closed preload-owned Shadow DOM. Plugin CSS can choose the page surface but cannot reach or restyle the host controls.</li><li>Existing string manifests and existing panel dimensions remain compatible.</li></ol><h2 id="consequences" tabindex="-1">Consequences <a class="header-anchor" href="#consequences" aria-label="Permalink to &quot;Consequences&quot;">​</a></h2><ul><li>Plugin authors can ship one manifest with English and Simplified Chinese panel titles.</li><li>Panel chrome remains visually coherent with custom plugin pages and the host&#39;s explicit theme preference.</li><li>A panel whose colors change after first paint is not continuously inspected; plugins should set their page surface before <code>DOMContentLoaded</code>.</li></ul></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("adr/0082-localized-plugin-panel-chrome.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _0082LocalizedPluginPanelChrome = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _0082LocalizedPluginPanelChrome as default
};
