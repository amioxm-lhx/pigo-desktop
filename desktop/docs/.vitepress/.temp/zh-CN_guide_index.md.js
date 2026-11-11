import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"快速开始","description":"PI-Desktop 产品和文档结构的中文导览。","frontmatter":{"title":"快速开始","description":"PI-Desktop 产品和文档结构的中文导览。"},"headers":[],"relativePath":"zh-CN/guide/index.md","filePath":"zh-CN/guide/index.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/guide/index.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="快速开始" tabindex="-1">快速开始 <a class="header-anchor" href="#快速开始" aria-label="Permalink to &quot;快速开始&quot;">​</a></h1><p>PI-Desktop 是一个本地优先的 AI 编程代理桌面客户端。它让工作区、宿主进程、代理运行时和模型配置保持可见、可检查，同时让日常编码保持直接。</p><h2 id="从哪里开始" tabindex="-1">从哪里开始 <a class="header-anchor" href="#从哪里开始" aria-label="Permalink to &quot;从哪里开始&quot;">​</a></h2><table tabindex="0"><thead><tr><th>你想了解…</th><th>从这里开始</th></tr></thead><tbody><tr><td>应用界面长什么样</td><td><a href="/zh-CN/guide/screenshots">界面截图</a></td></tr><tr><td>当前交付了什么</td><td><a href="/zh-CN/spec/01-product/01-product-scope">产品范围</a></td></tr><tr><td>系统如何协作</td><td><a href="/zh-CN/spec/02-architecture/01-architecture">系统架构</a></td></tr><tr><td>协议和存储边界</td><td><a href="/zh-CN/spec/03-runtime/01-ipc-protocol">运行时规格</a></td></tr><tr><td>如何开发插件</td><td><a href="/zh-CN/plugin-development">插件开发</a></td></tr><tr><td>为什么做出某个决策</td><td><a href="/zh-CN/adr/">ADR 索引</a></td></tr><tr><td>如何验证行为变化</td><td><a href="/zh-CN/spec/06-delivery/04-e2e-test-plan">E2E 测试计划</a></td></tr></tbody></table><p>这张表与英文快速开始页保持相同顺序；如果你需要完整英文技术正文， 可以直接打开 <a href="/guide/">English guide</a>。</p><h2 id="系统心智模型" tabindex="-1">系统心智模型 <a class="header-anchor" href="#系统心智模型" aria-label="Permalink to &quot;系统心智模型&quot;">​</a></h2><div class="language-text vp-adaptive-theme line-numbers-mode"><button title="Copy Code" class="copy"></button><span class="lang">text</span><pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span>Renderer UI  →  Electron orchestration  →  Rust host core</span></span>
<span class="line"><span>      ↓                    ↓                       ↓</span></span>
<span class="line"><span>  transcript          pi Node sidecar          SQLite + processes</span></span></code></pre><div class="line-numbers-wrapper" aria-hidden="true"><span class="line-number">1</span><br><span class="line-number">2</span><br><span class="line-number">3</span><br></div></div><p>Renderer 负责呈现；Electron main 协调桌面能力：窗口生命周期、IPC 路由、进程监管、 更新客户端，以及插件、MCP 桥接和可选的回环 MCP 控制服务；Rust host 负责工具执行与 工作区沙箱、权限网关、插件宿主服务、RPC 与持久化；pi sidecar 负责代理循环和模型工作。</p><h2 id="文档语言说明" tabindex="-1">文档语言说明 <a class="header-anchor" href="#文档语言说明" aria-label="Permalink to &quot;文档语言说明&quot;">​</a></h2><p>中文入口提供与英文一致的产品导览、主题地图和完整规格正文。每个中文规格页都 保留英文源页面链接；代码、协议字段和标识符不翻译。英文仍是最终源事实，但你 不需要为了阅读完整内容不断跳回英文站点。</p><p>切换到 <a href="/guide/">English</a> 查看完整的英文快速开始文档，或使用顶部搜索直接查找协议方法、错误码和决策编号。</p></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/guide/index.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const index = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  index as default
};
