import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"用户体验","description":"","frontmatter":{},"headers":[],"relativePath":"zh-CN/spec/04-ux/README.md","filePath":"zh-CN/spec/04-ux/README.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/04-ux/README.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="用户体验" tabindex="-1">用户体验 <a class="header-anchor" href="#用户体验" aria-label="Permalink to &quot;用户体验&quot;">​</a></h1><blockquote><p><strong>翻译说明：</strong> 本页是与 <a href="/spec/04-ux/README">英文源规格</a> 一一对应的机器辅助翻译。代码、协议字段和标识符保持原文；如翻译与英文源事实有歧义，以英文版本为准。</p></blockquote><table tabindex="0"><thead><tr><th>医生</th><th>描述</th></tr></thead><tbody><tr><td><a href="/zh-CN/spec/04-ux/01-ui-ia">01-ui-ia.md</a></td><td>UI信息架构</td></tr><tr><td><a href="/zh-CN/spec/04-ux/02-i18n-english-first">02-i18n-english-first.md</a></td><td>英语优先的国际化政策</td></tr><tr><td><a href="/zh-CN/spec/04-ux/03-permission-ux">03-permission-ux.md</a></td><td>权限卡用户体验</td></tr><tr><td><a href="/zh-CN/spec/04-ux/04-builtin-commands">04-builtin-commands.md</a></td><td>内置命令面板目录</td></tr><tr><td><a href="/zh-CN/spec/04-ux/05-onboarding">05-onboarding.md</a></td><td>首次运行新手引导</td></tr><tr><td><a href="/zh-CN/spec/04-ux/06-settings-ia">06-settings-ia.md</a></td><td>设置 IA</td></tr><tr><td><a href="/zh-CN/spec/04-ux/07-ui-design-system">07-ui-design-system.md</a></td><td>UI设计系统（符号、版式、动作、密度）</td></tr><tr><td><a href="/zh-CN/spec/04-ux/08-component-spec">08-component-spec.md</a></td><td>关键组件规格（shell、聊天、工具、权限）</td></tr><tr><td><a href="/zh-CN/spec/04-ux/09-interaction-patterns">09-交互模式.md</a></td><td>交互模式（键盘、流式传输、中止、折叠）</td></tr><tr><td><a href="/zh-CN/spec/04-ux/10-workbuddy-benchmark-ux">10-workbuddy-benchmark-ux.md</a></td><td>WorkBuddy 基准演练 + adopt/adapt/reject 提案</td></tr><tr><td><a href="/zh-CN/spec/04-ux/11-asktool-question-card">11-asktool-question-card.md</a></td><td>内联多问题卡</td></tr><tr><td><a href="/zh-CN/spec/04-ux/12-prompt-enhancement">12-prompt-enhancement.md</a></td><td>输入框一次性提示词增强</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/04-ux/README.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const README = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  README as default
};
