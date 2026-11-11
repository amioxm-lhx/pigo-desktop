import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"输入框提示词增强","description":"","frontmatter":{},"headers":[],"relativePath":"zh-CN/spec/04-ux/12-prompt-enhancement.md","filePath":"zh-CN/spec/04-ux/12-prompt-enhancement.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/04-ux/12-prompt-enhancement.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="输入框提示词增强" tabindex="-1">输入框提示词增强 <a class="header-anchor" href="#输入框提示词增强" aria-label="Permalink to &quot;输入框提示词增强&quot;">​</a></h1><blockquote><p><strong>翻译说明：</strong> 本页是与 <a href="/spec/04-ux/12-prompt-enhancement">英文源规格</a> 一一对应的机器辅助翻译。代码、协议字段和标识符保持原文；如翻译与英文源事实有歧义，以英文版本为准。</p></blockquote><h2 id="_1-范围" tabindex="-1">1. 范围 <a class="header-anchor" href="#_1-范围" aria-label="Permalink to &quot;1. 范围&quot;">​</a></h2><p>提示词增强能力为非空草稿提供一次性的 <code>Enhance prompt</code> 请求。输入框把它渲染成 一个独立的 Sparkles 操作，位于「模型 × 推理」组合选择器与单一的 Stop/Send 提交 位之间。调用时，该请求仅用输入框当前显示的模型重写草稿文本，文件引用块保持不变。</p><p>这是 v1 的工具型操作，而不是一次 agent 回合：它不追加消息、不读取会话历史、 不运行工具，也不持久化任何转录行。</p><h2 id="_2-可用性与交互" tabindex="-1">2. 可用性与交互 <a class="header-anchor" href="#_2-可用性与交互" aria-label="Permalink to &quot;2. 可用性与交互&quot;">​</a></h2><p>Sparkles 操作仅在下列条件全部成立时启用：</p><ul><li>草稿经 trim 后非空；</li><li>生效的显示提供商/模型已启用且已认证，判定使用与 Send 相同的就绪谓词；以及</li><li>trim 后的草稿不以 <code>/</code> 开头。</li></ul><p>请求进行期间，该操作被禁用，并显示共享的 <code>.tool-spinner</code> 与本地化的 <code>Enhancing…</code> 标签。发送仍然允许。输入框从当前显示的模型选择器发送 <code>providerId</code>、<code>modelId</code> 与 <code>thinkingLevel</code>；主进程校验这些值，并在快照缺失或 过期时依次回退到会话、草稿、全局与提供商默认值。</p><p>成功时，trim 后的结果替换文本，光标移到末尾，并出现单个 <code>Undo enhancement</code> 操作用于还原增强前的原始文本。任何用户编辑、发送或输入框会话切换都会清除该撤销 操作。不存在多级历史、快捷键或取消操作。</p><h2 id="_3-请求与提供商边界" tabindex="-1">3. 请求与提供商边界 <a class="header-anchor" href="#_3-请求与提供商边界" aria-label="Permalink to &quot;3. 请求与提供商边界&quot;">​</a></h2><p>渲染层通过列入允许清单的 <code>pi-desktop/prompt/enhance</code> invoke 通道发起请求。 Electron 主进程使用与 agent 回合相同的运行时启动解析器解析生效的提供商/模型， 仅在主进程读取 API 凭据，并调用 agent-runtime 的一次性补全辅助函数。厂商 OAuth 提供商通过既有的、由主进程持有的解析器获得短时效的 <code>ModelAuth</code>；任何密钥或 刷新令牌都不会进入渲染层。</p><p>补全上下文恰好包含：</p><ol><li>来自 <code>packages/agent-runtime/src/prompt-templates.ts</code> 的静态 <code>PROMPT_ENHANCEMENT_SYSTEM_PROMPT</code>；以及</li><li>一条用户消息 <code>Draft:\\n&lt;draft&gt;</code>。</li></ol><p>不包含任何先前对话、工具、附件或可配置模板。所选思考等级会传给 pi-ai， 提供商初始化的重试沿用既有的有界重试控制器。模型输出按纯文本消费并 trim。 输出为空或仅含空白字符时，记为 <code>PROMPT_ENHANCEMENT_EMPTY</code> 失败。</p><h2 id="_4-失败与竞态处理" tabindex="-1">4. 失败与竞态处理 <a class="header-anchor" href="#_4-失败与竞态处理" aria-label="Permalink to &quot;4. 失败与竞态处理&quot;">​</a></h2><p>失败时保留当前草稿，并渲染一条可关闭的输入框错误条，其中包含归类后的错误消息 与错误码。既有的提供商错误码如 <code>PROVIDER_UNAUTHORIZED</code>、<code>NETWORK_ERROR</code> 与 <code>TIMEOUT</code> 会被复用。</p><p>渲染层在发起请求时捕获草稿键与一个编辑代号。若响应到达前草稿发生变化、被发送 或清空，或用户切换了会话，则响应被丢弃，不能覆盖更新后的草稿。文件引用块不参与 重写，也不会因成功或失败而被移除。</p></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/04-ux/12-prompt-enhancement.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _12PromptEnhancement = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _12PromptEnhancement as default
};
