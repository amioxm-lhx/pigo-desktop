import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"02. 非目标","description":"","frontmatter":{},"headers":[],"relativePath":"zh-CN/spec/01-product/02-non-goals.md","filePath":"zh-CN/spec/01-product/02-non-goals.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/01-product/02-non-goals.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="_02-非目标" tabindex="-1">02. 非目标 <a class="header-anchor" href="#_02-非目标" aria-label="Permalink to &quot;02. 非目标&quot;">​</a></h1><blockquote><p><strong>翻译说明：</strong> 本页是与 <a href="/spec/01-product/02-non-goals">英文源规格</a> 一一对应的机器辅助翻译。代码、协议字段和标识符保持原文；如翻译与英文源事实有歧义，以英文版本为准。</p></blockquote><h2 id="_1-mvp-将不包括" tabindex="-1">1. MVP 将不包括 <a class="header-anchor" href="#_1-mvp-将不包括" aria-label="Permalink to &quot;1. MVP 将不包括&quot;">​</a></h2><p>1.远程 Gateway / WebUI 远程控制（本地回环 MCP 控制是默认关闭的桌面侧例外， 不是远程网关） 2. 云账号与多设备同步 3.完整的IDE（LSP、调试器、多文件编辑器工作区） 4. 一个从头开始的代理循环替换 pi 5. 第一天的完整插件市场 6. 移动客户端 7. 多用户认证系统 8.Billing/subscription 模块 9.计算机使用浏览器接管 10、未确认的全盘高权限模式 11. 非英语作为主要源语言 12. 第二个规划器 Agent、规划器服务、规划器模型或单独的 Plan 的权限配置文件 13. 将 Plan 视为严格的只读安全沙箱或自动批准 计划的 Plan 运行</p><h2 id="_2-尚未优化" tabindex="-1">2. 尚未优化 <a class="header-anchor" href="#_2-尚未优化" aria-label="Permalink to &quot;2. 尚未优化&quot;">​</a></h2><ul><li>最小封装尺寸极限</li><li>复杂的动画系统</li><li>发布时完成多区域覆盖</li><li>像素完美的多平台打磨</li><li>海量历史搜索性能</li></ul><h2 id="_3-不是成功标准" tabindex="-1">3. 不是成功标准 <a class="header-anchor" href="#_3-不是成功标准" aria-label="Permalink to &quot;3. 不是成功标准&quot;">​</a></h2><ul><li>克隆每个 ChatGPT 桌面功能</li><li>克隆 WorkBuddy 企业集成</li><li>克隆 LiveAgent 网关堆栈</li></ul><h2 id="_4-mvp-的架构选项-deferred-rejected" tabindex="-1">4. MVP 的架构选项 deferred/rejected <a class="header-anchor" href="#_4-mvp-的架构选项-deferred-rejected" aria-label="Permalink to &quot;4. MVP 的架构选项 deferred/rejected&quot;">​</a></h2><table tabindex="0"><thead><tr><th>选项</th><th>为什么</th></tr></thead><tbody><tr><td>金牛座贝壳</td><td>Electron 路线被冻结</td></tr><tr><td>渲染器端代理循环</td><td>安全和生命周期风险</td></tr><tr><td>远程优先设计</td><td>与本地优先 MVP 冲突</td></tr><tr><td>本地插件运行时之前的市场</td><td>过早扩张</td></tr><tr><td>立即重写 Rust 中的 pi 代理引擎</td><td>成本太高；保留 pi 引擎</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/01-product/02-non-goals.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _02NonGoals = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _02NonGoals as default
};
