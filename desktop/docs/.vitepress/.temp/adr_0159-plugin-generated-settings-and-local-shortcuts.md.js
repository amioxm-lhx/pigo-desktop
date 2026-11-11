import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"ADR 0159: Generated plugin settings and plugin-local shortcuts","description":"","frontmatter":{},"headers":[],"relativePath":"adr/0159-plugin-generated-settings-and-local-shortcuts.md","filePath":"adr/0159-plugin-generated-settings-and-local-shortcuts.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "adr/0159-plugin-generated-settings-and-local-shortcuts.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="adr-0159-generated-plugin-settings-and-plugin-local-shortcuts" tabindex="-1">ADR 0159: Generated plugin settings and plugin-local shortcuts <a class="header-anchor" href="#adr-0159-generated-plugin-settings-and-plugin-local-shortcuts" aria-label="Permalink to &quot;ADR 0159: Generated plugin settings and plugin-local shortcuts&quot;">​</a></h1><ul><li>Status: Accepted</li><li>Date: 2026-08-13</li><li>Decision owners: PI-Desktop desktop/plugin maintainers</li></ul><h2 id="context" tabindex="-1">Context <a class="header-anchor" href="#context" aria-label="Permalink to &quot;Context&quot;">​</a></h2><p>Plugins already declared settings and could read/write their private settings, but users had no standard way to edit those values. The next known setting is a shortcut, and registering plugin shortcuts globally would expand the collision and lifecycle surface before the plugin settings contract is stable.</p><h2 id="decision" tabindex="-1">Decision <a class="header-anchor" href="#decision" aria-label="Permalink to &quot;Decision&quot;">​</a></h2><ol><li><code>contributes.settings</code> is the source of truth for the generated settings UI. Supported types are <code>string</code>, <code>number</code>, <code>boolean</code>, <code>select</code>, <code>json</code>, and <code>shortcut</code>.</li><li>A shortcut setting declares a plugin command through <code>command</code> and has the fixed <code>scope: &quot;plugin&quot;</code>. The user may edit it in the installed plugin page.</li><li>Plugin shortcuts are handled by the renderer only while the PI-Desktop window is focused. The host re-checks the plugin activation scope before executing the command. They are not Electron/global shortcuts.</li><li>The host validates values, persists them in the plugin-private settings file, and sends <code>plugin:settingsChanged</code> to the plugin process.</li><li>Secret settings remain rejected until a dedicated secure storage contract is designed.</li></ol><h2 id="consequences" tabindex="-1">Consequences <a class="header-anchor" href="#consequences" aria-label="Permalink to &quot;Consequences&quot;">​</a></h2><ul><li>Plugin authors can ship a usable settings surface without bundling a custom configuration page.</li><li>The existing application shortcut map remains authoritative for app-global behavior and plugin shortcuts cannot replace it.</li><li>A future global plugin shortcut feature will require a separate decision for collision handling, OS registration, and disabled/background plugin state.</li></ul></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("adr/0159-plugin-generated-settings-and-local-shortcuts.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _0159PluginGeneratedSettingsAndLocalShortcuts = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _0159PluginGeneratedSettingsAndLocalShortcuts as default
};
