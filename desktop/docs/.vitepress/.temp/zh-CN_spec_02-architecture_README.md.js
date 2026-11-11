import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"架构与工程","description":"","frontmatter":{},"headers":[],"relativePath":"zh-CN/spec/02-architecture/README.md","filePath":"zh-CN/spec/02-architecture/README.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/02-architecture/README.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="架构与工程" tabindex="-1">架构与工程 <a class="header-anchor" href="#架构与工程" aria-label="Permalink to &quot;架构与工程&quot;">​</a></h1><blockquote><p><strong>翻译说明：</strong> 本页是与 <a href="/spec/02-architecture/README">英文源规格</a> 一一对应的机器辅助翻译。代码、协议字段和标识符保持原文；如翻译与英文源事实有歧义，以英文版本为准。</p></blockquote><table tabindex="0"><thead><tr><th>医生</th><th>描述</th></tr></thead><tbody><tr><td><a href="/zh-CN/spec/02-architecture/01-architecture">01-architecture.md</a></td><td>系统架构</td></tr><tr><td><a href="/zh-CN/spec/02-architecture/02-tech-stack">02-tech-stack.md</a></td><td>技术栈</td></tr><tr><td><a href="/zh-CN/spec/02-architecture/03-repo-structure">03-repo-struct.md</a></td><td>存储库结构</td></tr><tr><td><a href="/zh-CN/spec/02-architecture/04-documentation-site">04-documentation-site.md</a></td><td>文档站点</td></tr><tr><td><a href="/zh-CN/spec/02-architecture/05-remote-agent-control">05-remote-agent-control.md</a></td><td>远程 Agent Host 和 Gateway 目标架构</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/02-architecture/README.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const README = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  README as default
};
