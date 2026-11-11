import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"规格地图","description":"与英文规格一一对应的 PI-Desktop 产品、架构、运行时、体验、安全、交付、插件和决策入口。","frontmatter":{"title":"规格地图","description":"与英文规格一一对应的 PI-Desktop 产品、架构、运行时、体验、安全、交付、插件和决策入口。"},"headers":[],"relativePath":"zh-CN/spec/index.md","filePath":"zh-CN/spec/index.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/index.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="规格地图" tabindex="-1">规格地图 <a class="header-anchor" href="#规格地图" aria-label="Permalink to &quot;规格地图&quot;">​</a></h1><p>规格地图把完整技术库整理成八个主题域。每份英文规格都在相同相对路径下提供中文正文；协议字段、代码标识符与路径保持原文，并在页首链接回英文源事实。</p><h2 id="八个主题域" tabindex="-1">八个主题域 <a class="header-anchor" href="#八个主题域" aria-label="Permalink to &quot;八个主题域&quot;">​</a></h2><table tabindex="0"><thead><tr><th>主题</th><th>中文入口</th><th>你会在这里找到</th></tr></thead><tbody><tr><td>产品</td><td><a href="./product/">产品与范围</a></td><td>定位、交付范围、操作模式和非目标</td></tr><tr><td>架构</td><td><a href="./architecture/">架构与工程</a></td><td>进程边界、技术栈、仓库结构和数据所有权</td></tr><tr><td>运行时</td><td><a href="./runtime/">运行时核心</a></td><td>IPC、Agent、工具、Rust host、RPC 和模型</td></tr><tr><td>体验</td><td><a href="./ux/">用户体验</a></td><td>信息架构、国际化、设计系统和交互</td></tr><tr><td>安全</td><td><a href="./security/">安全</a></td><td>权限、路径、插件和进程隔离</td></tr><tr><td>交付</td><td><a href="./delivery/">交付与验收</a></td><td>里程碑、验收、E2E、变更和发布</td></tr><tr><td>插件</td><td><a href="./plugins/">插件系统</a></td><td>包格式、API、权限、生命周期和开发体验</td></tr><tr><td>决策</td><td><a href="./meta/">决策与元数据</a></td><td>ADR、决策日志和开放问题</td></tr></tbody></table><h2 id="推荐阅读顺序" tabindex="-1">推荐阅读顺序 <a class="header-anchor" href="#推荐阅读顺序" aria-label="Permalink to &quot;推荐阅读顺序&quot;">​</a></h2><ol><li><a href="/zh-CN/guide/">快速开始</a></li><li><a href="/zh-CN/spec/00-baseline">冻结基线</a></li><li><a href="/zh-CN/spec/01-product/README">产品与范围</a></li><li><a href="/zh-CN/spec/02-architecture/README">架构与工程</a></li><li><a href="/zh-CN/spec/03-runtime/README">运行时核心</a></li><li><a href="/zh-CN/spec/07-plugins/README">插件系统</a></li><li><a href="/zh-CN/spec/06-delivery/README">交付与验收</a></li></ol><h2 id="如何保持同步" tabindex="-1">如何保持同步 <a class="header-anchor" href="#如何保持同步" aria-label="Permalink to &quot;如何保持同步&quot;">​</a></h2><p>中文目录和英文目录使用相同的八个主题域、文件路径与阅读顺序。技术正文发生 变化时，同一逻辑变更必须更新对应中文页、相关 ADR 和 E2E 测试计划。翻译页 显式指向英文源事实，避免协议字段或代码标识符产生两套定义。</p></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/index.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const index = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  index as default
};
