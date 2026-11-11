import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"ADR 0185: Korean shell locale","description":"","frontmatter":{},"headers":[],"relativePath":"adr/0185-korean-shell-locale.md","filePath":"adr/0185-korean-shell-locale.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "adr/0185-korean-shell-locale.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="adr-0185-korean-shell-locale" tabindex="-1">ADR 0185: Korean shell locale <a class="header-anchor" href="#adr-0185-korean-shell-locale" aria-label="Permalink to &quot;ADR 0185: Korean shell locale&quot;">​</a></h1><ul><li>Status: Accepted</li><li>Date: 2026-09-09</li><li>Decision owners: PI-Desktop desktop/i18n maintainers</li><li>Related: ADR 0160, ADR 0182, ADR 0183, D349, E2E-091</li></ul><h2 id="context" tabindex="-1">Context <a class="header-anchor" href="#context" aria-label="Permalink to &quot;Context&quot;">​</a></h2><p>The searchable locale registry already supports complete shell catalogs, but Korean users were still limited to English or another shipped locale. Korean is a distinct language and should not be represented by an English fallback or by a Chinese catalog with a Korean label.</p><h2 id="decision" tabindex="-1">Decision <a class="header-anchor" href="#decision" aria-label="Permalink to &quot;Decision&quot;">​</a></h2><ol><li>Ship a complete Korean shell catalog at <code>packages/i18n/src/locales/ko</code>, with native name <code>한국어</code> and English name <code>Korean</code>.</li><li>Resolve <code>ko</code> and regional <code>ko-*</code> OS locale tags to the Korean catalog. Add <code>ko</code> to persisted <code>AppSettings.language</code> and to the Electron Chromium locale allowlist.</li><li>Ship a Korean product changelog catalog with the same stable version and highlight-count set as English so release notes follow the active shell locale.</li><li>Preserve the existing English-first catalog contract: identical keys, interpolation variables, searchable picker behavior, and English fallback for plugin-contributed labels. No host protocol, storage schema, or IPC version changes.</li></ol><h2 id="consequences" tabindex="-1">Consequences <a class="header-anchor" href="#consequences" aria-label="Permalink to &quot;Consequences&quot;">​</a></h2><ul><li>Korean users can select Korean or use Auto detection without reloading the renderer.</li><li>Catalog and changelog parity tests cover Korean alongside the existing shipped locales.</li><li>Regional Korean variants intentionally share one base catalog; spelling differences can be added later without changing the picker contract.</li></ul><h2 id="alternatives" tabindex="-1">Alternatives <a class="header-anchor" href="#alternatives" aria-label="Permalink to &quot;Alternatives&quot;">​</a></h2><ul><li>Register <code>ko</code> while falling back to English: rejected because the picker would advertise an untranslated shell.</li><li>Add region-specific Korean catalogs immediately: rejected because the current locale contract uses one maintainable catalog per language.</li></ul></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("adr/0185-korean-shell-locale.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _0185KoreanShellLocale = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _0185KoreanShellLocale as default
};
