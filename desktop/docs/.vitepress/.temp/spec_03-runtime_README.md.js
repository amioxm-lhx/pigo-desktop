import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"Runtime Core","description":"","frontmatter":{},"headers":[],"relativePath":"spec/03-runtime/README.md","filePath":"spec/03-runtime/README.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "spec/03-runtime/README.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="runtime-core" tabindex="-1">Runtime Core <a class="header-anchor" href="#runtime-core" aria-label="Permalink to &quot;Runtime Core&quot;">​</a></h1><table tabindex="0"><thead><tr><th>Doc</th><th>Description</th></tr></thead><tbody><tr><td><a href="./01-ipc-protocol">01-ipc-protocol.md</a></td><td>Electron IPC protocol</td></tr><tr><td><a href="./02-agent-runtime">02-agent-runtime.md</a></td><td>pi agent runtime</td></tr><tr><td><a href="./03-tools-and-permissions">03-tools-and-permissions.md</a></td><td>Tools &amp; permissions</td></tr><tr><td><a href="./04-data-storage">04-data-storage.md</a></td><td>Local storage model</td></tr><tr><td><a href="./05-host-core-rust">05-host-core-rust.md</a></td><td>Rust host core</td></tr><tr><td><a href="./06-host-rpc-protocol">06-host-rpc-protocol.md</a></td><td>Host JSON-RPC protocol</td></tr><tr><td><a href="./07-process-model">07-process-model.md</a></td><td>Process topology &amp; lifecycle</td></tr><tr><td><a href="./08-error-codes">08-error-codes.md</a></td><td>Shared error code registry</td></tr><tr><td><a href="./09-logging-and-observability">09-logging-and-observability.md</a></td><td>Logging &amp; audit</td></tr><tr><td><a href="./10-session-state-machine">10-session-state-machine.md</a></td><td>Session/turn state machine</td></tr><tr><td><a href="./11-provider-model-system">11-provider-model-system.md</a></td><td>Universal provider &amp; model coverage</td></tr><tr><td><a href="./12-provider-config-schema">12-provider-config-schema.md</a></td><td>Provider config / SQL / host methods</td></tr><tr><td><a href="./13-model-catalog-and-selection">13-model-catalog-and-selection.md</a></td><td>Model catalog, picker, selection</td></tr><tr><td><a href="./14-secrets-storage">14-secrets-storage.md</a></td><td>Secrets storage &amp; redaction</td></tr><tr><td><a href="./15-workspace-ignore-rules">15-workspace-ignore-rules.md</a></td><td>Workspace ignore &amp; denylist</td></tr><tr><td><a href="./16-tool-result-limits">16-tool-result-limits.md</a></td><td>Tool result size limits</td></tr><tr><td><a href="./17-asktool-questions">17-asktool-questions.md</a></td><td>Interactive multi-question tool</td></tr><tr><td><a href="./18-line-anchored-edit-contract">18-line-anchored-edit-contract.md</a></td><td>Line-anchored Edit contract</td></tr><tr><td><a href="./19-remote-agent-control-protocol">19-remote-agent-control-protocol.md</a></td><td>Remote Agent Control Protocol</td></tr><tr><td><a href="./20-staff-employees">20-staff-employees.md</a></td><td>Staff employees, credentials, shared memory</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("spec/03-runtime/README.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const README = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  README as default
};
