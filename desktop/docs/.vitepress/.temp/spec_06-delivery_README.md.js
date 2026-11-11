import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"Delivery & Acceptance","description":"","frontmatter":{},"headers":[],"relativePath":"spec/06-delivery/README.md","filePath":"spec/06-delivery/README.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "spec/06-delivery/README.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="delivery-acceptance" tabindex="-1">Delivery &amp; Acceptance <a class="header-anchor" href="#delivery-acceptance" aria-label="Permalink to &quot;Delivery &amp; Acceptance&quot;">​</a></h1><blockquote><p>Directory: <code>docs/spec/06-delivery</code></p></blockquote><table tabindex="0"><thead><tr><th>Doc</th><th>Description</th></tr></thead><tbody><tr><td><a href="./01-mvp-milestones">01-mvp-milestones.md</a></td><td>Milestones</td></tr><tr><td><a href="./02-acceptance-criteria">02-acceptance-criteria.md</a></td><td>Acceptance criteria</td></tr><tr><td><a href="./03-ai-development-workflow">03-ai-development-workflow.md</a></td><td>AI/human development workflow rules</td></tr><tr><td><a href="./04-e2e-test-plan">04-e2e-test-plan.md</a></td><td>E2E test documentation &amp; MVP scenario catalog</td></tr><tr><td><a href="./05-change-checklist">05-change-checklist.md</a></td><td>Practical checklist before finishing work</td></tr><tr><td><a href="./06-release-runbook">06-release-runbook.md</a></td><td>Desktop release lanes, packaging, and mandatory shipped-locale changelog gate (D164/D345)</td></tr><tr><td><a href="./07-remote-control-rollout">07-remote-control-rollout.md</a></td><td>Remote Agent Control rollout and acceptance</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("spec/06-delivery/README.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const README = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  README as default
};
