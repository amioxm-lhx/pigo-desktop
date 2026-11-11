import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"11. asktool问题卡","description":"","frontmatter":{},"headers":[],"relativePath":"zh-CN/spec/04-ux/11-asktool-question-card.md","filePath":"zh-CN/spec/04-ux/11-asktool-question-card.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/04-ux/11-asktool-question-card.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="_11-asktool问题卡" tabindex="-1">11. asktool问题卡 <a class="header-anchor" href="#_11-asktool问题卡" aria-label="Permalink to &quot;11. asktool问题卡&quot;">​</a></h1><blockquote><p><strong>翻译说明：</strong> 本页是与 <a href="/spec/04-ux/11-asktool-question-card">英文源规格</a> 一一对应的机器辅助翻译。代码、协议字段和标识符保持原文；如翻译与英文源事实有歧义，以英文版本为准。</p></blockquote><p>Asktool 卡是内联输入框批准表面，而不是许可 对话框。它与 Plan 和 Goal 批准卡安装在同一停靠区域， 紧邻输入框输入的上方，因此暂停的问题仍然可用 主动决策点，而不是进入成绩单历史。</p><p>它使用现有的消息宽度、边框、背景和按钮标记，以便暂停的问题在 视觉上仍然是对话的一部分。问题文本比聊天正文大半档 （<code>--text-base-plus</code>，15 px），因此它读起来是卡片的主要焦点， 同时仍贴近周围成绩单的字号。</p><p>标题标识提示并显示进度。可点击的小指示器 编码已应答、未应答、跳过和当前状态，而不与 问题文本。选择控件使用单选语义来进行单选和 多选的复选框语义。每个问题都包含一个可见的自定义 输入选择。操作行包含 Skip 和 Next/Submit，其中 Decline 全部为 标题中安静的次要动作。</p><p>该卡没有倒计时或到期副本。在窄屏幕上，选项仍然存在 全角和动作可以共享行；问题文本和自定义输入可能 自然包裹，无需剪裁。</p><h2 id="版式层级" tabindex="-1">版式层级 <a class="header-anchor" href="#版式层级" aria-label="Permalink to &quot;版式层级&quot;">​</a></h2><p>该卡使用清晰的四级字号阶梯，以避免过多相近尺寸带来的视觉噪音：</p><table tabindex="0"><thead><tr><th>元素</th><th>标记</th><th>尺寸</th><th>字重</th><th>备注</th></tr></thead><tbody><tr><td>卡片标题</td><td><code>--text-xs</code></td><td>11 px</td><td>medium</td><td>全大写，宽字距</td></tr><tr><td>问题编号</td><td><code>--text-xs</code></td><td>11 px</td><td>medium</td><td>宽字距，眉标角色</td></tr><tr><td>进度</td><td><code>--text-2xs</code></td><td>10.5 px</td><td>—</td><td>淡色，最不显眼</td></tr><tr><td>问题文本</td><td><code>--text-base-plus</code></td><td>15 px</td><td>medium</td><td>主要焦点</td></tr><tr><td>选项文本</td><td><code>--text-sm-plus</code></td><td>12.5 px</td><td>—</td><td>比问题低一档</td></tr><tr><td>选项标记</td><td><code>--text-xs-plus</code></td><td>11.5 px</td><td>—</td><td>与选项文本成比例</td></tr><tr><td>自定义输入文本</td><td><code>--text-sm-plus</code></td><td>12.5 px</td><td>—</td><td>与选项文本一致</td></tr><tr><td>拒绝按钮</td><td><code>--text-xs</code></td><td>11 px</td><td>—</td><td>安静的次要动作</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/04-ux/11-asktool-question-card.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _11AsktoolQuestionCard = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _11AsktoolQuestionCard as default
};
