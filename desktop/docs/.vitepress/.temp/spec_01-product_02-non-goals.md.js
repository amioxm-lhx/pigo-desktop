import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"02. Non-Goals","description":"","frontmatter":{},"headers":[],"relativePath":"spec/01-product/02-non-goals.md","filePath":"spec/01-product/02-non-goals.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "spec/01-product/02-non-goals.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="_02-non-goals" tabindex="-1">02. Non-Goals <a class="header-anchor" href="#_02-non-goals" aria-label="Permalink to &quot;02. Non-Goals&quot;">​</a></h1><h2 id="_1-mvp-will-not-include" tabindex="-1">1. MVP will not include <a class="header-anchor" href="#_1-mvp-will-not-include" aria-label="Permalink to &quot;1. MVP will not include&quot;">​</a></h2><ol><li>Remote Gateway / WebUI remote control (the local loopback MCP control plane is an opt-in desktop-side exception, not a remote gateway)</li><li>Cloud accounts and multi-device sync</li><li>Full IDE (LSP, debugger, multi-file editor workspace)</li><li>A from-scratch agent loop replacing pi</li><li>Full plugin marketplace at day one</li><li>Mobile clients</li><li>Multi-user auth systems</li><li>Billing/subscription modules</li><li>Computer Use browser takeover</li><li>Unconfirmed full-disk high privilege mode</li><li>Non-English as the primary source language</li><li>A second planner Agent, planner service, planner model, or separate permission profile for Plan</li><li>Treating Plan as a strict read-only security sandbox or auto-approving a scheduled Plan run</li></ol><h2 id="_2-not-optimized-yet" tabindex="-1">2. Not optimized yet <a class="header-anchor" href="#_2-not-optimized-yet" aria-label="Permalink to &quot;2. Not optimized yet&quot;">​</a></h2><ul><li>Minimal package size extremes</li><li>Complex animation systems</li><li>Complete multi-locale coverage at launch</li><li>Pixel-perfect multi-platform polish</li><li>Massive history search performance</li></ul><h2 id="_3-not-success-criteria" tabindex="-1">3. Not success criteria <a class="header-anchor" href="#_3-not-success-criteria" aria-label="Permalink to &quot;3. Not success criteria&quot;">​</a></h2><ul><li>Clone every ChatGPT Desktop feature</li><li>Clone WorkBuddy enterprise integrations</li><li>Clone LiveAgent gateway stack</li></ul><h2 id="_4-architecture-options-deferred-rejected-for-mvp" tabindex="-1">4. Architecture options deferred/rejected for MVP <a class="header-anchor" href="#_4-architecture-options-deferred-rejected-for-mvp" aria-label="Permalink to &quot;4. Architecture options deferred/rejected for MVP&quot;">​</a></h2><table tabindex="0"><thead><tr><th>Option</th><th>Why</th></tr></thead><tbody><tr><td>Tauri shell</td><td>Electron route is frozen</td></tr><tr><td>Renderer-side agent loop</td><td>Security and lifecycle risk</td></tr><tr><td>Remote-first design</td><td>Conflicts with local-first MVP</td></tr><tr><td>Marketplace before local plugin runtime</td><td>Premature expansion</td></tr><tr><td>Rewrite pi agent engine in Rust immediately</td><td>Too costly; keep pi engine</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("spec/01-product/02-non-goals.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _02NonGoals = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _02NonGoals as default
};
