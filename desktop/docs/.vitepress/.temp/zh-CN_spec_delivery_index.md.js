import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"交付与验收","description":"PI-Desktop 的里程碑、验收、E2E、变更和发布流程。","frontmatter":{"title":"交付与验收","description":"PI-Desktop 的里程碑、验收、E2E、变更和发布流程。"},"headers":[],"relativePath":"zh-CN/spec/delivery/index.md","filePath":"zh-CN/spec/delivery/index.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/delivery/index.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="交付与验收" tabindex="-1">交付与验收 <a class="header-anchor" href="#交付与验收" aria-label="Permalink to &quot;交付与验收&quot;">​</a></h1><p>这一主题把“实现完成”与“可以交付”连接起来，覆盖里程碑、验收标准、E2E、变更清单和发布手册。</p><h2 id="阅读顺序" tabindex="-1">阅读顺序 <a class="header-anchor" href="#阅读顺序" aria-label="Permalink to &quot;阅读顺序&quot;">​</a></h2><ol><li><a href="/zh-CN/spec/06-delivery/01-mvp-milestones">MVP 里程碑</a></li><li><a href="/zh-CN/spec/06-delivery/02-acceptance-criteria">验收标准</a></li><li><a href="/zh-CN/spec/06-delivery/03-ai-development-workflow">AI 开发工作流</a></li><li><a href="/zh-CN/spec/06-delivery/04-e2e-test-plan">E2E 测试计划</a></li><li><a href="/zh-CN/spec/06-delivery/05-change-checklist">变更清单</a></li><li><a href="/zh-CN/spec/06-delivery/06-release-runbook">发布手册</a></li></ol><h2 id="交付闭环" tabindex="-1">交付闭环 <a class="header-anchor" href="#交付闭环" aria-label="Permalink to &quot;交付闭环&quot;">​</a></h2><div class="language-text vp-adaptive-theme line-numbers-mode"><button title="Copy Code" class="copy"></button><span class="lang">text</span><pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span>spec → implementation → targeted validation → E2E scenario → release record</span></span></code></pre><div class="line-numbers-wrapper" aria-hidden="true"><span class="line-number">1</span><br></div></div><p>用户可见或协议可见的行为变化，必须在 E2E 测试计划中留下可执行场景。</p></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/delivery/index.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const index = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  index as default
};
