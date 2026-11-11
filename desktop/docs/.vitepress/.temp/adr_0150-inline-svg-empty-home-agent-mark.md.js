import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"ADR 0150: Inline SVG empty-home agent mark","description":"","frontmatter":{},"headers":[],"relativePath":"adr/0150-inline-svg-empty-home-agent-mark.md","filePath":"adr/0150-inline-svg-empty-home-agent-mark.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "adr/0150-inline-svg-empty-home-agent-mark.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="adr-0150-inline-svg-empty-home-agent-mark" tabindex="-1">ADR 0150: Inline SVG empty-home agent mark <a class="header-anchor" href="#adr-0150-inline-svg-empty-home-agent-mark" aria-label="Permalink to &quot;ADR 0150: Inline SVG empty-home agent mark&quot;">​</a></h1><ul><li>Status: Superseded by ADR 0152</li><li>Date: 2026-09-04</li><li>Deciders: PI-Desktop core</li><li>Related: D291, E2E-046, E2E-099, US-UI-17</li></ul><h2 id="context" tabindex="-1">Context <a class="header-anchor" href="#context" aria-label="Permalink to &quot;Context&quot;">​</a></h2><p>The empty-home hero used a raster sprite atlas with randomized pose groups, discrete frame changes, and a hover shortcut that accelerated playback. The result was visually loud in the central conversation area and added asset, timer, and reduced-motion state to a decorative mark.</p><h2 id="decision" tabindex="-1">Decision <a class="header-anchor" href="#decision" aria-label="Permalink to &quot;Decision&quot;">​</a></h2><p>Replace the sprite with a 100px inline SVG agent mark. A thin orbit and signal point rotate slowly around a compact core; the core breathes gently and the eyes blink occasionally. The animation is deterministic, pointer hover does not change its cadence, and <code>prefers-reduced-motion: reduce</code> freezes the mark. The SVG remains decorative with <code>aria-hidden=&quot;true&quot;</code> and preserves the existing 100px layout slot.</p><h2 id="consequences" tabindex="-1">Consequences <a class="header-anchor" href="#consequences" aria-label="Permalink to &quot;Consequences&quot;">​</a></h2><ul><li>Empty-home branding is quieter and remains readable across light and dark themes through semantic color tokens.</li><li>The component has no raster atlas, random selection, timers, or hover state.</li><li>CSS owns the motion and can disable it without changing the rendered SVG.</li><li>The old sprite asset can remain available for historical source context but is no longer part of the renderer path.</li></ul><h2 id="alternatives-rejected" tabindex="-1">Alternatives rejected <a class="header-anchor" href="#alternatives-rejected" aria-label="Permalink to &quot;Alternatives rejected&quot;">​</a></h2><ul><li>Keep the existing atlas and tune pause durations: this would retain the visually noisy pixel-art treatment and interaction-specific timer state.</li><li>Generate another raster mascot: this would add asset maintenance while preserving the same theme and reduced-motion limitations.</li></ul></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("adr/0150-inline-svg-empty-home-agent-mark.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _0150InlineSvgEmptyHomeAgentMark = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _0150InlineSvgEmptyHomeAgentMark as default
};
