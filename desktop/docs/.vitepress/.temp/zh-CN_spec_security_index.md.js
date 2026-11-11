import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"安全","description":"PI-Desktop 的权限、路径、插件和进程隔离边界。","frontmatter":{"title":"安全","description":"PI-Desktop 的权限、路径、插件和进程隔离边界。"},"headers":[],"relativePath":"zh-CN/spec/security/index.md","filePath":"zh-CN/spec/security/index.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/security/index.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="安全" tabindex="-1">安全 <a class="header-anchor" href="#安全" aria-label="Permalink to &quot;安全&quot;">​</a></h1><p>安全规格描述哪些操作需要授权、哪些路径可以访问，以及插件和宿主进程如何隔离。</p><h2 id="关键入口" tabindex="-1">关键入口 <a class="header-anchor" href="#关键入口" aria-label="Permalink to &quot;关键入口&quot;">​</a></h2><ul><li><a href="/zh-CN/spec/05-security/01-security">安全模型</a></li><li><a href="/zh-CN/spec/03-runtime/03-tools-and-permissions">工具与权限</a></li><li><a href="/zh-CN/spec/07-plugins/04-plugin-security">插件安全</a></li><li><a href="/zh-CN/spec/03-runtime/06-host-rpc-protocol">Host RPC 与资源隔离</a></li></ul><h2 id="维护规则" tabindex="-1">维护规则 <a class="header-anchor" href="#维护规则" aria-label="Permalink to &quot;维护规则&quot;">​</a></h2><p>任何权限、进程、路径、插件能力或 secret 存储变化，都需要同步更新英文规格、ADR 和 E2E 测试计划。</p></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/security/index.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const index = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  index as default
};
