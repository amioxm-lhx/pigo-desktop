import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"架构与工程","description":"PI-Desktop 的桌面壳、进程边界、存储所有权和工程结构。","frontmatter":{"title":"架构与工程","description":"PI-Desktop 的桌面壳、进程边界、存储所有权和工程结构。"},"headers":[],"relativePath":"zh-CN/spec/architecture/index.md","filePath":"zh-CN/spec/architecture/index.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/architecture/index.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="架构与工程" tabindex="-1">架构与工程 <a class="header-anchor" href="#架构与工程" aria-label="Permalink to &quot;架构与工程&quot;">​</a></h1><p>这一主题说明桌面客户端如何拆分，以及每个进程和数据边界由谁负责。中文页面提供同步导览；完整契约见对应英文规格。</p><h2 id="阅读顺序" tabindex="-1">阅读顺序 <a class="header-anchor" href="#阅读顺序" aria-label="Permalink to &quot;阅读顺序&quot;">​</a></h2><ol><li><a href="/zh-CN/spec/02-architecture/01-architecture">系统架构</a></li><li><a href="/zh-CN/spec/02-architecture/02-tech-stack">应用技术栈</a></li><li><a href="/zh-CN/spec/02-architecture/03-repo-structure">仓库结构</a></li><li><a href="/zh-CN/spec/02-architecture/04-documentation-site">文档站</a></li></ol><h2 id="系统地图" tabindex="-1">系统地图 <a class="header-anchor" href="#系统地图" aria-label="Permalink to &quot;系统地图&quot;">​</a></h2><div class="language-text vp-adaptive-theme line-numbers-mode"><button title="Copy Code" class="copy"></button><span class="lang">text</span><pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span>Renderer UI → Electron orchestration → Rust host core</span></span>
<span class="line"><span>     ↓                  ↓                    ↓</span></span>
<span class="line"><span> transcript       pi Node sidecar       SQLite + processes</span></span></code></pre><div class="line-numbers-wrapper" aria-hidden="true"><span class="line-number">1</span><br><span class="line-number">2</span><br><span class="line-number">3</span><br></div></div><p>Renderer 负责呈现，Electron 负责桌面能力协调，pi sidecar 负责代理循环，Rust host 负责特权进程、文件系统、RPC 和持久化边界。</p></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/architecture/index.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const index = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  index as default
};
