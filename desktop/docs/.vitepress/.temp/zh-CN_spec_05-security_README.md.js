import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"05. 安全","description":"","frontmatter":{},"headers":[],"relativePath":"zh-CN/spec/05-security/README.md","filePath":"zh-CN/spec/05-security/README.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/05-security/README.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="_05-安全" tabindex="-1">05. 安全 <a class="header-anchor" href="#_05-安全" aria-label="Permalink to &quot;05. 安全&quot;">​</a></h1><blockquote><p><strong>翻译说明：</strong> 本页是与 <a href="/spec/05-security/README">英文源规格</a> 一一对应的机器辅助翻译。代码、协议字段和标识符保持原文；如翻译与英文源事实有歧义，以英文版本为准。</p></blockquote><blockquote><p>目录：<code>docs/spec/05-security</code></p></blockquote><table tabindex="0"><thead><tr><th>医生</th><th>描述</th></tr></thead><tbody><tr><td><a href="/zh-CN/spec/05-security/01-security">01-security.md</a></td><td>安全基线</td></tr><tr><td><a href="/zh-CN/spec/05-security/02-remote-control-security">02-remote-control-security.md</a></td><td>远程 Agent 控制安全</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/05-security/README.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const README = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  README as default
};
