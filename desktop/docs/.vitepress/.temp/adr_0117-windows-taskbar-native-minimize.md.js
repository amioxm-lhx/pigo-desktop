import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"ADR 0117: Preserve the Windows taskbar entry for native minimize","description":"","frontmatter":{},"headers":[],"relativePath":"adr/0117-windows-taskbar-native-minimize.md","filePath":"adr/0117-windows-taskbar-native-minimize.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "adr/0117-windows-taskbar-native-minimize.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="adr-0117-preserve-the-windows-taskbar-entry-for-native-minimize" tabindex="-1">ADR 0117: Preserve the Windows taskbar entry for native minimize <a class="header-anchor" href="#adr-0117-preserve-the-windows-taskbar-entry-for-native-minimize" aria-label="Permalink to &quot;ADR 0117: Preserve the Windows taskbar entry for native minimize&quot;">​</a></h1><ul><li>Status: Accepted (amended by ADR 0123)</li><li>Date: 2026-08-24</li><li>Deciders: PI-Desktop core</li><li>Related: D216, D252, D256, E2E-124, ADR 0078, ADR 0123</li></ul><h2 id="context" tabindex="-1">Context <a class="header-anchor" href="#context" aria-label="Permalink to &quot;Context&quot;">​</a></h2><p>The tray-resident minimize decision in D216 hides the main window from the operating system&#39;s window list. On Windows, clicking the taskbar button of the focused window is a native minimize/restore toggle. Electron reports the minimize half of that toggle through the main window&#39;s <code>minimize</code> event. The current handler then calls <code>window.hide()</code>, which removes the taskbar entry and leaves only the tray icon as a restore path.</p><h2 id="decision" tabindex="-1">Decision <a class="header-anchor" href="#decision" aria-label="Permalink to &quot;Decision&quot;">​</a></h2><ol><li>On Windows, the main window&#39;s native <code>minimize</code> event is not converted to <code>window.hide()</code>. The OS completes the minimize, so the main window remains represented by its taskbar entry.</li><li>The renderer-drawn Windows minimize button and the Windows native-menu minimize action use the same native minimize transition. Linux uses that transition for its renderer and native-menu minimize actions as well. The macOS native minimize event remains tray-resident.</li><li>The existing restore path continues to restore and focus the same window. Clicking the taskbar entry while the window is merely covered therefore keeps its existing bring-to-front behavior; no new IPC, storage, or host protocol contract is needed.</li></ol><h2 id="consequences" tabindex="-1">Consequences <a class="header-anchor" href="#consequences" aria-label="Permalink to &quot;Consequences&quot;">​</a></h2><ul><li>Windows has one native taskbar minimize model for both the OS taskbar toggle and the explicit in-app minimize action.</li><li>Windows/Linux taskbar-minimized windows remain reachable from the taskbar; close-to-tray windows remain reachable from the resident tray icon.</li><li>Background work and window bounds persistence are unchanged.</li></ul></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("adr/0117-windows-taskbar-native-minimize.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _0117WindowsTaskbarNativeMinimize = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _0117WindowsTaskbarNativeMinimize as default
};
