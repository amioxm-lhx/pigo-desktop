import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"运行时核心","description":"PI-Desktop 的 IPC、代理循环、工具权限、Rust host 和模型系统。","frontmatter":{"title":"运行时核心","description":"PI-Desktop 的 IPC、代理循环、工具权限、Rust host 和模型系统。"},"headers":[],"relativePath":"zh-CN/spec/runtime/index.md","filePath":"zh-CN/spec/runtime/index.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/runtime/index.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="运行时核心" tabindex="-1">运行时核心 <a class="header-anchor" href="#运行时核心" aria-label="Permalink to &quot;运行时核心&quot;">​</a></h1><p>这一主题回答“一次请求如何穿过客户端，以及哪些组件可以改变状态”。页面分组与英文 Runtime Core 保持同步。</p><h2 id="阅读顺序" tabindex="-1">阅读顺序 <a class="header-anchor" href="#阅读顺序" aria-label="Permalink to &quot;阅读顺序&quot;">​</a></h2><ol><li><a href="/zh-CN/spec/03-runtime/01-ipc-protocol">IPC 协议</a></li><li><a href="/zh-CN/spec/03-runtime/02-agent-runtime">Agent 运行时</a></li><li><a href="/zh-CN/spec/03-runtime/03-tools-and-permissions">工具与权限</a></li><li><a href="/zh-CN/spec/03-runtime/05-host-core-rust">Rust host core</a></li><li><a href="/zh-CN/spec/03-runtime/06-host-rpc-protocol">Host RPC 协议</a></li><li><a href="/zh-CN/spec/03-runtime/11-provider-model-system">Provider 与模型系统</a></li></ol><h2 id="关注边界" tabindex="-1">关注边界 <a class="header-anchor" href="#关注边界" aria-label="Permalink to &quot;关注边界&quot;">​</a></h2><ul><li>Renderer 只通过 preload IPC 访问桌面能力。</li><li>pi sidecar 持有代理循环和 provider-facing model work。</li><li>Rust host 独占 SQLite，并负责进程、文件系统、RPC 和错误边界。</li><li>所有协议或状态变化都应同步更新 E2E 场景。</li></ul></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/runtime/index.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const index = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  index as default
};
