import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"ADR 0167: Agent-chosen Bash timeout","description":"","frontmatter":{},"headers":[],"relativePath":"adr/0167-agent-chosen-bash-timeout.md","filePath":"adr/0167-agent-chosen-bash-timeout.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "adr/0167-agent-chosen-bash-timeout.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="adr-0167-agent-chosen-bash-timeout" tabindex="-1">ADR 0167: Agent-chosen Bash timeout <a class="header-anchor" href="#adr-0167-agent-chosen-bash-timeout" aria-label="Permalink to &quot;ADR 0167: Agent-chosen Bash timeout&quot;">​</a></h1><ul><li>Status: Accepted for implementation</li><li>Date: 2026-09-06</li><li>Deciders: PI-Desktop core</li><li>Related: D329, D190, D273, ADR 0054, GitHub issue #44</li></ul><h2 id="context" tabindex="-1">Context <a class="header-anchor" href="#context" aria-label="Permalink to &quot;Context&quot;">​</a></h2><p>ADR 0054 / D190 required a 60-second default Bash timeout and rejected any override outside 1–300 seconds. That 5-minute ceiling killed legitimate builds, tests, and audits, and <code>timeout: 600</code> / <code>1800</code> failed immediately with <code>INVALID_ARGUMENT</code>.</p><p>D273 already widened the schema so millisecond habits validate, then clamped them to the same 300-second ceiling. The agent could ask for 30 minutes and still be killed at five.</p><p>A timeout remains mandatory: every spawn needs a finite deadline, and timeout/abort still kills the process tree. The frozen failure mode is an unbounded hang, not the integer 300.</p><h2 id="decision" tabindex="-1">Decision <a class="header-anchor" href="#decision" aria-label="Permalink to &quot;Decision&quot;">​</a></h2><ol><li>Missing <code>timeout</code> still means exactly 60 seconds.</li><li>An explicit override is 1 through 21,600 seconds (6 hours). That is a host safety bound so RPC and the process timer stay finite, not a product cap the agent is expected to hit.</li><li>A value above 21,600 is read as milliseconds (D273), converted, and clamped to 21,600 seconds. In-range values, including 600 and 1800, are seconds.</li><li>Out-of-range values still fail validation and never spawn.</li><li>Timeout and user abort still terminate the complete process tree.</li></ol><h2 id="consequences" tabindex="-1">Consequences <a class="header-anchor" href="#consequences" aria-label="Permalink to &quot;Consequences&quot;">​</a></h2><ul><li><code>timeout: 600</code> / <code>1800</code> / <code>1800000</code> honour 10 and 30 minutes.</li><li>A command with no timeout still dies at 60 seconds.</li><li>Host-core <code>MAX_BASH_TIMEOUT_MS</code> stays in lockstep with the runtime.</li></ul></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("adr/0167-agent-chosen-bash-timeout.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _0167AgentChosenBashTimeout = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _0167AgentChosenBashTimeout as default
};
