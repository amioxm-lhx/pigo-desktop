import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"17. asktool 互动问题","description":"","frontmatter":{},"headers":[],"relativePath":"zh-CN/spec/03-runtime/17-asktool-questions.md","filePath":"zh-CN/spec/03-runtime/17-asktool-questions.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/03-runtime/17-asktool-questions.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="_17-asktool-互动问题" tabindex="-1">17. asktool 互动问题 <a class="header-anchor" href="#_17-asktool-互动问题" aria-label="Permalink to &quot;17. asktool 互动问题&quot;">​</a></h1><blockquote><p><strong>翻译说明：</strong> 本页是与 <a href="/spec/03-runtime/17-asktool-questions">英文源规格</a> 一一对应的机器辅助翻译。代码、协议字段和标识符保持原文；如翻译与英文源事实有歧义，以英文版本为准。</p></blockquote><h2 id="_1-目的" tabindex="-1">1. 目的 <a class="header-anchor" href="#_1-目的" aria-label="Permalink to &quot;1. 目的&quot;">​</a></h2><p><code>asktool</code> 让模型暂停回合并询问用户一个或多个有界的 问题。它在 Agent、Plan 和 Goal 模式下可用，并且独立于 权限审批：不授权操作，无有效性 截止日期。</p><h2 id="_2-请求形状" tabindex="-1">2. 请求形状 <a class="header-anchor" href="#_2-请求形状" aria-label="Permalink to &quot;2. 请求形状&quot;">​</a></h2><p>该工具接受非空 <code>questions</code> 数组。每个问题包含：</p><ul><li><code>question</code>：提示文字；</li><li><code>options</code>：一个或多个可选答案标签；</li><li><code>multiSelect</code>：可选；如果为 true，则允许多个可选答案。</li></ul><p>桌面卡总是添加一个额外的 <code>Enter another answer</code> 选项以及文本 场。该模型不需要向工具添加特殊的自由文本选择 论据。</p><h2 id="_3-卡片交互" tabindex="-1">3. 卡片交互 <a class="header-anchor" href="#_3-卡片交互" aria-label="Permalink to &quot;3. 卡片交互&quot;">​</a></h2><p>一次仅显示一个问题。为每个呈现一个小指示器 问题并使用三种状态：未回答、已回答和已跳过。选择一个 指标重新审视了这个问题。 <code>Next</code> 当存在答案时记录答案； 否则记录一个跳过。 <code>Skip</code> 显式记录跳过并前进。 <code>Decline all</code> 解决了跳过的每个问题。</p><p>没有计时器、倒计时或自动过期。该卡仍处于待处理状态 直到用户提交或回合停止。如果回合停止时 卡已打开，运行时会解决跳过的所有剩余问题。</p><h2 id="_4-工具输出" tabindex="-1">4. 工具输出 <a class="header-anchor" href="#_4-工具输出" aria-label="Permalink to &quot;4. 工具输出&quot;">​</a></h2><p>响应是返回到模型并持续存在的正常工具结果 工具行。对于每个问题，内容序列化为：</p><div class="language-text vp-adaptive-theme line-numbers-mode"><button title="Copy Code" class="copy"></button><span class="lang">text</span><pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span>question text：answer 1、answer 2</span></span></code></pre><div class="line-numbers-wrapper" aria-hidden="true"><span class="line-number">1</span><br></div></div><p>多个问题由 <code>\\n---\\n</code> 分隔。跳过或未答复 Question 保留问题文本并使用空占位符：</p><div class="language-text vp-adaptive-theme line-numbers-mode"><button title="Copy Code" class="copy"></button><span class="lang">text</span><pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span>question text：</span></span></code></pre><div class="line-numbers-wrapper" aria-hidden="true"><span class="line-number">1</span><br></div></div><p>这种格式是确定性的，保留问题顺序，并进行多项选择 无需将仅渲染器状态对象暴露给 模型。</p></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/03-runtime/17-asktool-questions.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _17AsktoolQuestions = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _17AsktoolQuestions as default
};
