import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"交付及验收","description":"","frontmatter":{},"headers":[],"relativePath":"zh-CN/spec/06-delivery/README.md","filePath":"zh-CN/spec/06-delivery/README.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/06-delivery/README.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="交付及验收" tabindex="-1">交付及验收 <a class="header-anchor" href="#交付及验收" aria-label="Permalink to &quot;交付及验收&quot;">​</a></h1><blockquote><p><strong>翻译说明：</strong> 本页是与 <a href="/spec/06-delivery/README">英文源规格</a> 一一对应的机器辅助翻译。代码、协议字段和标识符保持原文；如翻译与英文源事实有歧义，以英文版本为准。</p></blockquote><blockquote><p>目录：<code>docs/spec/06-delivery</code></p></blockquote><table tabindex="0"><thead><tr><th>医生</th><th>描述</th></tr></thead><tbody><tr><td><a href="/zh-CN/spec/06-delivery/01-mvp-milestones">01-mvp-milestones.md</a></td><td>里程碑</td></tr><tr><td><a href="/zh-CN/spec/06-delivery/02-acceptance-criteria">02-接受-criteria.md</a></td><td>验收标准</td></tr><tr><td><a href="/zh-CN/spec/06-delivery/03-ai-development-workflow">03-ai-development-workflow.md</a></td><td>AI/human开发工作流程规则</td></tr><tr><td><a href="/zh-CN/spec/06-delivery/04-e2e-test-plan">04-e2e-test-plan.md</a></td><td>E2E 测试文档和 MVP 场景目录</td></tr><tr><td><a href="/zh-CN/spec/06-delivery/05-change-checklist">05-change-checklist.md</a></td><td>完成工作前的实用清单</td></tr><tr><td><a href="/zh-CN/spec/06-delivery/06-release-runbook">06-release-runbook.md</a></td><td>桌面发布通道、打包和强制已发货语言变更日志门 (D164/D345)</td></tr><tr><td><a href="/zh-CN/spec/06-delivery/07-remote-control-rollout">07-remote-control-rollout.md</a></td><td>远程 Agent 控制交付与验收</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/06-delivery/README.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const README = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  README as default
};
