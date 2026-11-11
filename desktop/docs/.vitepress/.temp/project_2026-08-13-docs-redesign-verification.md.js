import { ssrRenderAttrs, ssrRenderAttr } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const _imports_0 = "/screenshots/docs-home-desktop.png";
const _imports_1 = "/screenshots/docs-home-mobile-zh.png";
const _imports_2 = "/screenshots/docs-spec-zh-desktop.png";
const __pageData = JSON.parse('{"title":"Documentation redesign visual verification","description":"Desktop and mobile rendering baselines for the bilingual VitePress documentation system.","frontmatter":{"title":"Documentation redesign visual verification","description":"Desktop and mobile rendering baselines for the bilingual VitePress documentation system."},"headers":[],"relativePath":"project/2026-08-13-docs-redesign-verification.md","filePath":"project/2026-08-13-docs-redesign-verification.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "project/2026-08-13-docs-redesign-verification.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="documentation-redesign-visual-verification" tabindex="-1">Documentation redesign visual verification <a class="header-anchor" href="#documentation-redesign-visual-verification" aria-label="Permalink to &quot;Documentation redesign visual verification&quot;">​</a></h1><p>These browser-rendered captures record the responsive baseline introduced by the August 2026 documentation redesign. They are evidence for E2E-125, not a replacement for rebuilding and checking the current site.</p><h2 id="desktop-landing-page" tabindex="-1">Desktop landing page <a class="header-anchor" href="#desktop-landing-page" aria-label="Permalink to &quot;Desktop landing page&quot;">​</a></h2><p>The 1440×900 capture verifies the centered hero, compact navigation, system visual, feature row, and the beginning of the intent-based content map.</p><p><img${ssrRenderAttr("src", _imports_0)} alt="PI-Desktop documentation landing page at 1440 by 900"></p><h2 id="mobile-chinese-landing-page" tabindex="-1">Mobile Chinese landing page <a class="header-anchor" href="#mobile-chinese-landing-page" aria-label="Permalink to &quot;Mobile Chinese landing page&quot;">​</a></h2><p>The 390×844 capture verifies that the translated hero leads the reading order, the system visual follows the primary actions, and the page has no horizontal overflow.</p><p><img${ssrRenderAttr("src", _imports_1)} alt="PI-Desktop Chinese documentation landing page at 390 by 844"></p><h2 id="chinese-specification-page" tabindex="-1">Chinese specification page <a class="header-anchor" href="#chinese-specification-page" aria-label="Permalink to &quot;Chinese specification page&quot;">​</a></h2><p>The desktop specification capture verifies the generated Chinese sidebar, bounded reading column, source notice, and deep outline for a long runtime contract.</p><p><img${ssrRenderAttr("src", _imports_2)} alt="PI-Desktop Chinese specification page at 1440 by 900"></p><h2 id="verification-contract" tabindex="-1">Verification contract <a class="header-anchor" href="#verification-contract" aria-label="Permalink to &quot;Verification contract&quot;">​</a></h2><ul><li>Viewports: 1440×900 desktop and 390×844 mobile.</li><li>Locales: English and Simplified Chinese.</li><li>Appearance: light and dark mode are checked during the browser run; the committed captures use light mode for legibility in repository viewers.</li><li>Overflow: the document root must match the viewport width; wide tables and code blocks may scroll only inside their own containers.</li></ul></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("project/2026-08-13-docs-redesign-verification.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _20260813DocsRedesignVerification = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _20260813DocsRedesignVerification as default
};
