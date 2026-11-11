import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"PI-Desktop 文档","titleTemplate":"本地优先 AI 编程代理的文档系统","description":"","frontmatter":{"layout":"home","title":"PI-Desktop 文档","titleTemplate":"本地优先 AI 编程代理的文档系统","hero":{"name":"PI-Desktop","text":"把产品当作一套系统来阅读。","tagline":"一个克制、可检查的桌面工作区。沿着边界理解契约，在完整上下文中修改和验证代码。","actions":[{"theme":"brand","text":"先建立上下文","link":"/zh-CN/guide/"},{"theme":"alt","text":"打开规格地图","link":"/zh-CN/spec/README"}]},"features":[{"title":"先找到边界","details":"从产品意图出发，依次进入架构、运行时、体验、安全、交付和插件主题。"},{"title":"再读懂契约","details":"沿着 Rust host、pi sidecar、NDJSON RPC、存储所有权和模型系统深入。"},{"title":"带着证据修改","details":"把规格、ADR、实现、E2E 场景和发布清单连接成一条闭环。"}]},"headers":[],"relativePath":"zh-CN/index.md","filePath":"zh-CN/index.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/index.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/index.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const index = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  index as default
};
