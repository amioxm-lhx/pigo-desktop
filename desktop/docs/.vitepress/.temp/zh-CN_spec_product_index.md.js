import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"产品与范围","description":"PI-Desktop 的产品定位、交付范围和明确的非目标。","frontmatter":{"title":"产品与范围","description":"PI-Desktop 的产品定位、交付范围和明确的非目标。"},"headers":[],"relativePath":"zh-CN/spec/product/index.md","filePath":"zh-CN/spec/product/index.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/product/index.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="产品与范围" tabindex="-1">产品与范围 <a class="header-anchor" href="#产品与范围" aria-label="Permalink to &quot;产品与范围&quot;">​</a></h1><p>这一主题回答“PI-Desktop 要解决什么问题，以及当前明确不做什么”。它与英文的 Product &amp; Scope 目录保持同一结构；技术契约和原始决策继续以英文页面为源事实。</p><h2 id="阅读顺序" tabindex="-1">阅读顺序 <a class="header-anchor" href="#阅读顺序" aria-label="Permalink to &quot;阅读顺序&quot;">​</a></h2><ol><li><a href="/zh-CN/spec/01-product/00-overview">产品概览</a></li><li><a href="/zh-CN/spec/01-product/01-product-scope">产品范围</a></li><li><a href="/zh-CN/spec/01-product/02-non-goals">非目标</a></li></ol><h2 id="核心判断" tabindex="-1">核心判断 <a class="header-anchor" href="#核心判断" aria-label="Permalink to &quot;核心判断&quot;">​</a></h2><ul><li>PI-Desktop 是本地优先的 AI 编程代理桌面客户端。</li><li>工作区、代理运行时、权限边界和模型配置需要可见、可检查。</li><li>日常编码体验优先于远程控制、云端网关和无法审计的自动化。</li></ul><p>需要逐条实现约束时，继续阅读 <a href="/zh-CN/spec/01-product/README">完整产品规格</a>。</p></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/product/index.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const index = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  index as default
};
