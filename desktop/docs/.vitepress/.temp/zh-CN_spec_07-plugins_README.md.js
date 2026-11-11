import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"插件系统","description":"","frontmatter":{},"headers":[],"relativePath":"zh-CN/spec/07-plugins/README.md","filePath":"zh-CN/spec/07-plugins/README.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/07-plugins/README.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="插件系统" tabindex="-1">插件系统 <a class="header-anchor" href="#插件系统" aria-label="Permalink to &quot;插件系统&quot;">​</a></h1><blockquote><p><strong>翻译说明：</strong> 本页是与 <a href="/spec/07-plugins/README">英文源规格</a> 一一对应的机器辅助翻译。代码、协议字段和标识符保持原文；如翻译与英文源事实有歧义，以英文版本为准。</p></blockquote><blockquote><p>目录：<code>docs/spec/07-plugins</code></p></blockquote><p>插件作者应该从面向任务开始 【从零到一的开发指南】(../../plugin-development.md)。文档在 该目录定义了规范性合同和实施边界。</p><table tabindex="0"><thead><tr><th>文件</th><th>描述</th></tr></thead><tbody><tr><td><a href="/zh-CN/spec/07-plugins/01-plugin-system">01-plugin-system.md</a></td><td>插件系统概述</td></tr><tr><td><a href="/zh-CN/spec/07-plugins/02-plugin-manifest-schema">02-plugin-manifest-schema.md</a></td><td>Manifest Schema</td></tr><tr><td><a href="/zh-CN/spec/07-plugins/03-plugin-api">03-plugin-api.md</a></td><td>插件 API</td></tr><tr><td><a href="/zh-CN/spec/07-plugins/04-plugin-security">04-plugin-security.md</a></td><td>插件安全</td></tr><tr><td><a href="/zh-CN/spec/07-plugins/05-plugin-lifecycle">05-plugin-lifecycle.md</a></td><td>生命周期</td></tr><tr><td><a href="/zh-CN/spec/07-plugins/06-plugin-packaging">06-plugin-packaging.md</a></td><td>包装及安装</td></tr><tr><td><a href="/zh-CN/spec/07-plugins/07-plugin-marketplace">07-plugin-marketplace.md</a></td><td>插件市场</td></tr><tr><td><a href="/zh-CN/spec/07-plugins/08-plugin-signing-updates">08-plugin-signing-updates.md</a></td><td>签名和更新</td></tr><tr><td><a href="/zh-CN/spec/07-plugins/09-plugin-command-palette">09-plugin-command-palette.md</a></td><td>命令面板</td></tr><tr><td><a href="/zh-CN/spec/07-plugins/10-plugin-devex">10-plugin-devex.md</a></td><td>开发者经验</td></tr><tr><td><a href="/zh-CN/spec/07-plugins/11-plugin-storage-isolation">11-plugin-storage-isolation.md</a></td><td>存储隔离</td></tr><tr><td><a href="/zh-CN/spec/07-plugins/12-plugin-ipc-and-host-services">12-plugin-ipc-and-host-services.md</a></td><td>主机服务和 IPC</td></tr><tr><td><a href="/zh-CN/spec/07-plugins/13-plugin-permissions-matrix">13-plugin-permissions-matrix.md</a></td><td>权限矩阵</td></tr><tr><td><a href="/zh-CN/spec/07-plugins/14-plugin-roadmap">14-plugin-roadmap.md</a></td><td>插件路线图</td></tr><tr><td><a href="/zh-CN/spec/07-plugins/15-plugin-center">15-plugin-center.md</a></td><td>插件中心（发布侧）</td></tr><tr><td><a href="/zh-CN/spec/07-plugins/16-trusted-extensions">16-trusted-extensions.md</a></td><td>受信任扩展（sidecar 内扩展面）</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/07-plugins/README.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const README = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  README as default
};
