import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"ADR 0212: Remove diagnostic timing log streams","description":"","frontmatter":{},"headers":[],"relativePath":"adr/0212-remove-diagnostic-timing-log-streams.md","filePath":"adr/0212-remove-diagnostic-timing-log-streams.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "adr/0212-remove-diagnostic-timing-log-streams.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="adr-0212-remove-diagnostic-timing-log-streams" tabindex="-1">ADR 0212: Remove diagnostic timing log streams <a class="header-anchor" href="#adr-0212-remove-diagnostic-timing-log-streams" aria-label="Permalink to &quot;ADR 0212: Remove diagnostic timing log streams&quot;">​</a></h1><ul><li>Status: Accepted</li><li>Date: 2026-09-10</li><li>Amends: <a href="./0046-categorized-process-logs">ADR 0046</a> · D183</li><li>Related: <a href="./../spec/03-runtime/09-logging-and-observability">Logging and observability</a></li></ul><h2 id="context" tabindex="-1">Context <a class="header-anchor" href="#context" aria-label="Permalink to &quot;Context&quot;">​</a></h2><p>D137/D183 added per-tool, per-model, boot-phase, and updater timing output to help diagnose a slow run. The investigation is complete, and those records now create noise in local log directories during normal use.</p><h2 id="decision" tabindex="-1">Decision <a class="header-anchor" href="#decision" aria-label="Permalink to &quot;Decision&quot;">​</a></h2><ol><li>Stop emitting sidecar <code>[timing]</code> lines, host <code>tool timing</code> lines, boot-phase timing records, updater timing records, and renderer bootstrap timing output.</li><li>Remove the dedicated <code>timing</code> log category and the <code>PI_DESKTOP_TIMING</code> suppression environment variable. The updater&#39;s functional timeout remains.</li><li>Keep key lifecycle, state-change, permission, tool, plugin, provider, persistence, updater, and error records. Keep structured audit fields and UI/protocol duration metadata that are consumed by product features or security forensics.</li><li>Do not delete or migrate timing files already present in a user&#39;s local data directory. They are historical records.</li></ol><h2 id="consequences" tabindex="-1">Consequences <a class="header-anchor" href="#consequences" aria-label="Permalink to &quot;Consequences&quot;">​</a></h2><ul><li>Normal launches, turns, retries, and tool calls produce fewer log records and no longer create dedicated timing files.</li><li>Failures remain diagnosable through stable lifecycle records, error codes, tool/permission audit rows, and the visible transcript.</li><li>Timing-specific troubleshooting scenarios and documentation are retired.</li><li>Existing local timing files may remain until the user removes them.</li></ul></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("adr/0212-remove-diagnostic-timing-log-streams.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _0212RemoveDiagnosticTimingLogStreams = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _0212RemoveDiagnosticTimingLogStreams as default
};
