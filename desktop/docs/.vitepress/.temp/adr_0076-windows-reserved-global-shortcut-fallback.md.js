import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"ADR 0076: Capture the Windows-reserved plugin launcher chord in host-core","description":"","frontmatter":{},"headers":[],"relativePath":"adr/0076-windows-reserved-global-shortcut-fallback.md","filePath":"adr/0076-windows-reserved-global-shortcut-fallback.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "adr/0076-windows-reserved-global-shortcut-fallback.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="adr-0076-capture-the-windows-reserved-plugin-launcher-chord-in-host-core" tabindex="-1">ADR 0076: Capture the Windows-reserved plugin launcher chord in host-core <a class="header-anchor" href="#adr-0076-capture-the-windows-reserved-plugin-launcher-chord-in-host-core" aria-label="Permalink to &quot;ADR 0076: Capture the Windows-reserved plugin launcher chord in host-core&quot;">​</a></h1><ul><li>Status: Accepted</li><li>Date: 2026-08-12</li><li>Deciders: PI-Desktop core</li><li>Related: D211 · ADR 0072 · E2E-120</li></ul><h2 id="context" tabindex="-1">Context <a class="header-anchor" href="#context" aria-label="Permalink to &quot;Context&quot;">​</a></h2><p>Windows reserves <code>Alt+Space</code> for the active window system menu. Electron&#39;s <code>globalShortcut</code> may therefore reject the plugin launcher&#39;s default binding, and a renderer <code>before-input-event</code> fallback only works while PI-Desktop is focused.</p><h2 id="decision" tabindex="-1">Decision <a class="header-anchor" href="#decision" aria-label="Permalink to &quot;Decision&quot;">​</a></h2><p>Host-core installs a narrow Windows <code>WH_KEYBOARD_LL</code> hook for the default <code>Alt+Space</code> binding. It consumes the matching keydown, emits the existing JSON-RPC notification transport with <code>keyboard.shortcut</code>, and lets Electron toggle the plugin launcher. Electron sends the additive <code>keyboard.setGlobalShortcut</code> host method whenever the effective binding changes; the hook is enabled only when the effective Windows binding is <code>Alt+Space</code>. Other platforms and custom bindings retain Electron&#39;s normal global shortcut path.</p><p>The hook does not inspect or persist text, and it does not expose a new renderer or plugin capability. If hook installation fails, the focused-window fallback remains available and the failure is logged.</p><h2 id="consequences" tabindex="-1">Consequences <a class="header-anchor" href="#consequences" aria-label="Permalink to &quot;Consequences&quot;">​</a></h2><ul><li>The default Windows launcher chord works while another application is foregrounded and does not open that application&#39;s system menu.</li><li>The host binary owns a small platform-specific input integration, while Electron remains responsible for window creation and plugin-panel access.</li><li>Protocol version and storage schema remain unchanged because the method and notification are additive and Electron is the only consumer.</li></ul></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("adr/0076-windows-reserved-global-shortcut-fallback.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _0076WindowsReservedGlobalShortcutFallback = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _0076WindowsReservedGlobalShortcutFallback as default
};
