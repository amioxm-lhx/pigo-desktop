import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"05. 新手引导","description":"","frontmatter":{},"headers":[],"relativePath":"zh-CN/spec/04-ux/05-onboarding.md","filePath":"zh-CN/spec/04-ux/05-onboarding.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/04-ux/05-onboarding.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="_05-新手引导" tabindex="-1">05. 新手引导 <a class="header-anchor" href="#_05-新手引导" aria-label="Permalink to &quot;05. 新手引导&quot;">​</a></h1><blockquote><p><strong>翻译说明：</strong> 本页是与 <a href="/spec/04-ux/05-onboarding">英文源规格</a> 一一对应的机器辅助翻译。代码、协议字段和标识符保持原文；如翻译与英文源事实有歧义，以英文版本为准。</p></blockquote><h2 id="_1-mvp-的决策" tabindex="-1">1. MVP 的决策 <a class="header-anchor" href="#_1-mvp-的决策" aria-label="Permalink to &quot;1. MVP 的决策&quot;">​</a></h2><p>使用<strong>内联首次运行清单</strong>，而不是多页模式向导。</p><p>理由：</p><ul><li>更快地获得第一个值</li><li>更少的阻塞</li><li>更容易使用 skip/return</li></ul><h2 id="_2-首次运行检测" tabindex="-1">2. 首次运行检测 <a class="header-anchor" href="#_2-首次运行检测" aria-label="Permalink to &quot;2. 首次运行检测&quot;">​</a></h2><p>当满足以下任一条件时显示检查表：</p><ol><li>没有配置提供商 2.默认提供商没有秘密存在</li><li>尚无会话存在</li></ol><p>保留解雇状态，但不完整的关键步骤可能会重新显示为横幅。</p><h2 id="_3-检查清单步骤" tabindex="-1">3. 检查清单步骤 <a class="header-anchor" href="#_3-检查清单步骤" aria-label="Permalink to &quot;3. 检查清单步骤&quot;">​</a></h2><ol><li><strong>添加提供商</strong></li><li><strong>保存 API 密钥</strong></li><li><strong>打开项目文件夹</strong></li><li><strong>发送您的第一个提示</strong></li><li><em>（可选）</em> 加载开发插件</li></ol><h2 id="_4-安置" tabindex="-1">4. 安置 <a class="header-anchor" href="#_4-安置" aria-label="Permalink to &quot;4. 安置&quot;">​</a></h2><ul><li>显示在主聊天空白状态</li><li>提供商和关键项目深层链接至“设置”→ Agent</li><li>可选插件项打开应用程序外壳的插件目的地</li><li>项目和提示项调用其相关的应用程序操作</li><li>核心步骤完成后清单会崩溃</li></ul><h2 id="_5-复制提示音" tabindex="-1">5. 复制提示音 <a class="header-anchor" href="#_5-复制提示音" aria-label="Permalink to &quot;5. 复制提示音&quot;">​</a></h2><p>英文源码，简洁，行动导向。</p><p>示例：</p><ul><li>“添加模型提供商”</li><li>“保存您的 API 密钥”</li><li>“打开项目以启用本地工具”</li></ul><h2 id="_6-非目标" tabindex="-1">6. 非目标 <a class="header-anchor" href="#_6-非目标" aria-label="Permalink to &quot;6. 非目标&quot;">​</a></h2><ul><li>帐户注册</li><li>云同步设置</li><li>长产品巡演覆盖</li><li>为回访用户提供强制教程</li></ul><h2 id="_7-验收" tabindex="-1">7. 验收 <a class="header-anchor" href="#_7-验收" aria-label="Permalink to &quot;7. 验收&quot;">​</a></h2><ol><li>新鲜的个人资料显示清单</li><li>完成provider+key+prompt删除关键的空状态拦截器</li><li>用户可以在不中断应用程序使用的情况下关闭可选部分</li></ol></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/04-ux/05-onboarding.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _05Onboarding = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _05Onboarding as default
};
