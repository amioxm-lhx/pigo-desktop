import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"用户体验","description":"PI-Desktop 的信息架构、国际化、设计系统和交互模式。","frontmatter":{"title":"用户体验","description":"PI-Desktop 的信息架构、国际化、设计系统和交互模式。"},"headers":[],"relativePath":"zh-CN/spec/ux/index.md","filePath":"zh-CN/spec/ux/index.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/ux/index.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="用户体验" tabindex="-1">用户体验 <a class="header-anchor" href="#用户体验" aria-label="Permalink to &quot;用户体验&quot;">​</a></h1><p>这一主题把界面结构和运行时行为放在同一张图里阅读。中文导览与英文 UX 目录保持相同的入口顺序。</p><h2 id="阅读顺序" tabindex="-1">阅读顺序 <a class="header-anchor" href="#阅读顺序" aria-label="Permalink to &quot;阅读顺序&quot;">​</a></h2><ol><li><a href="/zh-CN/spec/04-ux/01-ui-ia">信息架构</a></li><li><a href="/zh-CN/spec/04-ux/02-i18n-english-first">国际化策略</a></li><li><a href="/zh-CN/spec/04-ux/07-ui-design-system">UI 设计系统</a></li><li><a href="/zh-CN/spec/04-ux/08-component-spec">组件规格</a></li><li><a href="/zh-CN/spec/04-ux/09-interaction-patterns">交互模式</a></li></ol><h2 id="体验原则" tabindex="-1">体验原则 <a class="header-anchor" href="#体验原则" aria-label="Permalink to &quot;体验原则&quot;">​</a></h2><ul><li>让当前项目、会话、运行状态和权限状态保持可见。</li><li>用清晰的状态反馈替代装饰性动效。</li><li>中文入口与英文技术正文保持一一对应，避免出现“有导航、无页面”的孤立入口。</li></ul></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/ux/index.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const index = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  index as default
};
