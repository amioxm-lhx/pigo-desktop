import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"插件系统","description":"PI-Desktop 的插件包、API、权限、生命周期和开发体验。","frontmatter":{"title":"插件系统","description":"PI-Desktop 的插件包、API、权限、生命周期和开发体验。"},"headers":[],"relativePath":"zh-CN/spec/plugins/index.md","filePath":"zh-CN/spec/plugins/index.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/plugins/index.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="插件系统" tabindex="-1">插件系统 <a class="header-anchor" href="#插件系统" aria-label="Permalink to &quot;插件系统&quot;">​</a></h1><p>插件系统允许用户在本地安装和运行扩展，同时保留明确的包格式、API、权限和生命周期边界。</p><h2 id="阅读顺序" tabindex="-1">阅读顺序 <a class="header-anchor" href="#阅读顺序" aria-label="Permalink to &quot;阅读顺序&quot;">​</a></h2><ol><li><a href="/zh-CN/spec/07-plugins/01-plugin-system">插件系统</a></li><li><a href="/zh-CN/plugin-development">插件开发</a></li><li><a href="/zh-CN/spec/07-plugins/02-plugin-manifest-schema">Manifest schema</a></li><li><a href="/zh-CN/spec/07-plugins/03-plugin-api">插件 API</a></li><li><a href="/zh-CN/spec/07-plugins/04-plugin-security">插件安全</a></li><li><a href="/zh-CN/spec/07-plugins/05-plugin-lifecycle">插件生命周期</a></li><li><a href="/zh-CN/spec/07-plugins/10-plugin-devex">开发体验</a></li></ol><h2 id="当前边界" tabindex="-1">当前边界 <a class="header-anchor" href="#当前边界" aria-label="Permalink to &quot;当前边界&quot;">​</a></h2><ul><li>第一阶段面向本地、用户安装的插件。</li><li>包格式是 <code>.piplug</code>，信任起点是 sha256 校验。</li><li>Marketplace 协议可以先定义，市场实现按路线图推进。</li></ul></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/plugins/index.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const index = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  index as default
};
