import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"决策与元数据","description":"PI-Desktop 的决策日志、开放问题和文档维护入口。","frontmatter":{"title":"决策与元数据","description":"PI-Desktop 的决策日志、开放问题和文档维护入口。"},"headers":[],"relativePath":"zh-CN/spec/meta/index.md","filePath":"zh-CN/spec/meta/index.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/meta/index.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="决策与元数据" tabindex="-1">决策与元数据 <a class="header-anchor" href="#决策与元数据" aria-label="Permalink to &quot;决策与元数据&quot;">​</a></h1><p>这一主题记录不能被静默改变的约束、开放问题和文档站自身的维护规则。</p><h2 id="关键入口" tabindex="-1">关键入口 <a class="header-anchor" href="#关键入口" aria-label="Permalink to &quot;关键入口&quot;">​</a></h2><ul><li><a href="/zh-CN/spec/08-meta/decisions-log">决策日志</a></li><li><a href="/zh-CN/spec/08-meta/open-questions">开放问题</a></li><li><a href="/zh-CN/adr/">ADR 索引</a></li><li><a href="/zh-CN/spec/README">完整规格索引</a></li></ul><p>中文入口翻译阅读路径和主题摘要；决策编号、协议字段和原始英文记录保持与仓库源事实同步。</p></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/meta/index.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const index = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  index as default
};
