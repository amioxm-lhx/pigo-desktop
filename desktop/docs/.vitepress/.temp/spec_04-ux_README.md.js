import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"UX","description":"","frontmatter":{},"headers":[],"relativePath":"spec/04-ux/README.md","filePath":"spec/04-ux/README.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "spec/04-ux/README.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="ux" tabindex="-1">UX <a class="header-anchor" href="#ux" aria-label="Permalink to &quot;UX&quot;">​</a></h1><table tabindex="0"><thead><tr><th>Doc</th><th>Description</th></tr></thead><tbody><tr><td><a href="./01-ui-ia">01-ui-ia.md</a></td><td>UI information architecture</td></tr><tr><td><a href="./02-i18n-english-first">02-i18n-english-first.md</a></td><td>English-first i18n policy</td></tr><tr><td><a href="./03-permission-ux">03-permission-ux.md</a></td><td>Permission card UX</td></tr><tr><td><a href="./04-builtin-commands">04-builtin-commands.md</a></td><td>Builtin command palette catalog</td></tr><tr><td><a href="./05-onboarding">05-onboarding.md</a></td><td>First-run onboarding</td></tr><tr><td><a href="./06-settings-ia">06-settings-ia.md</a></td><td>Settings IA</td></tr><tr><td><a href="./07-ui-design-system">07-ui-design-system.md</a></td><td>UI design system (tokens, typography, motion, density)</td></tr><tr><td><a href="./08-component-spec">08-component-spec.md</a></td><td>Key component specs (shell, chat, tools, permissions)</td></tr><tr><td><a href="./09-interaction-patterns">09-interaction-patterns.md</a></td><td>Interaction patterns (keyboard, streaming, abort, collapse)</td></tr><tr><td><a href="./10-workbuddy-benchmark-ux">10-workbuddy-benchmark-ux.md</a></td><td>WorkBuddy benchmark walkthrough + adopt/adapt/reject proposals</td></tr><tr><td><a href="./11-asktool-question-card">11-asktool-question-card.md</a></td><td>Inline multi-question card</td></tr><tr><td><a href="./12-prompt-enhancement">12-prompt-enhancement.md</a></td><td>Composer one-shot prompt enhancement</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("spec/04-ux/README.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const README = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  README as default
};
