import { ssrRenderAttrs, ssrRenderAttr, ssrRenderStyle } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const _imports_0 = "/assets/home-light.BlprWBgp.webp";
const _imports_1 = "/assets/home-dark.bJxJtjG3.webp";
const _imports_2 = "/assets/dark-home.B0o_UQBr.webp";
const _imports_3 = "/assets/minimap.BLjct-8t.webp";
const _imports_4 = "/assets/minimap-hover.BT-1tng1.webp";
const _imports_5 = "/assets/model-menu.CTAVPG2i.webp";
const _imports_6 = "/assets/composer-slash.CQqPjHVL.webp";
const _imports_7 = "/assets/composer-at.CDo5UrNI.webp";
const _imports_8 = "/assets/panel-review.dqU1_Fb4.webp";
const _imports_9 = "/assets/panel-browser.Mt51FzZX.webp";
const _imports_10 = "/assets/panel-files.J7ylbScM.webp";
const _imports_11 = "/assets/panel-menu.CaReLU0Z.webp";
const _imports_12 = "/assets/pulls-live.ClRtAz0U.webp";
const _imports_13 = "/assets/dark-pulls.BlZvRf0R.webp";
const _imports_14 = "/assets/project-archive-live.CIS00irU.webp";
const _imports_15 = "/assets/dark-project-archive.efARBM_A.webp";
const _imports_16 = "/assets/scheduled-live.DghmMCm8.webp";
const _imports_17 = "/assets/notifications-light.Byl5ok7U.webp";
const _imports_18 = "/assets/notifications-dark.Bp2c-zzw.webp";
const _imports_19 = "/assets/notifications-narrow.CMINiPIY.webp";
const _imports_20 = "/assets/toasts-light.CoORVm6X.webp";
const _imports_21 = "/assets/toasts-dark.qbTOS-E5.webp";
const _imports_22 = "/assets/search.nQfLzoR8.webp";
const _imports_23 = "/assets/search-query.mnkfC5oa.webp";
const _imports_24 = "/assets/search-settings.lXqtqvC6.webp";
const _imports_25 = "/assets/search-pages.wDaGMEmj.webp";
const _imports_26 = "/assets/search-anchor.DPNlqMW8.webp";
const _imports_27 = "/assets/search-dark.CtFlgRKj.webp";
const _imports_28 = "/assets/plugins-live.CFkD8sJZ.webp";
const _imports_29 = "/assets/plugins-market.CvUzcPgS.webp";
const _imports_30 = "/assets/plugins-menu.BGRm34lo.webp";
const _imports_31 = "/assets/plugins-row-menu.CJjfE6wV.webp";
const _imports_32 = "/assets/plugins-template.BMfSHgDl.webp";
const _imports_33 = "/assets/extensions-mcp.DBHk2DNm.webp";
const _imports_34 = "/assets/extensions-mcp.DBHk2DNm.webp";
const _imports_35 = "/assets/extensions-mcp.DBHk2DNm.webp";
const _imports_36 = "/assets/extensions-mcp.DBHk2DNm.webp";
const _imports_37 = "/assets/extensions-mcp.DBHk2DNm.webp";
const _imports_38 = "/assets/extensions-mcp.DBHk2DNm.webp";
const _imports_39 = "/assets/extensions-mcp.DBHk2DNm.webp";
const _imports_40 = "/assets/extensions-subagents-dark.C2OgTG0X.webp";
const _imports_41 = "/assets/extensions-mcp-dark.YfmgdJ-r.webp";
const _imports_42 = "/assets/settings-live.CZ-0vU0Q.webp";
const _imports_43 = "/assets/dark-settings.D9pr-PeK.webp";
const _imports_44 = "/assets/settings-models.BzRb-XfE.webp";
const _imports_45 = "/assets/settings-extensions.IDwew-QF.webp";
const _imports_46 = "/assets/settings-extensions-custom.-s4-Ao1b.webp";
const __pageData = JSON.parse('{"title":"界面截图","description":"PI-Desktop 的每个界面，全部取自运行中的应用。","frontmatter":{"title":"界面截图","description":"PI-Desktop 的每个界面，全部取自运行中的应用。"},"headers":[],"relativePath":"zh-CN/guide/screenshots.md","filePath":"zh-CN/guide/screenshots.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/guide/screenshots.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="界面截图" tabindex="-1">界面截图 <a class="header-anchor" href="#界面截图" aria-label="Permalink to &quot;界面截图&quot;">​</a></h1><p>下面每一张都来自支撑 <a href="/zh-CN/spec/06-delivery/04-e2e-test-plan">E2E 测试计划</a> 的截图装置：应用以 <code>PI_DESKTOP_CAPTURE=1</code> 在一个临时数据目录上启动，自行走过每个 界面并写出 PNG，再由 <code>scripts/publish-screenshots.py</code> 转换成本页的图片。因此这些 截图展示的是实际发布的界面，而不是设计稿——包括全新安装时看到的空态。</p><p>会话标题和对话内容来自截图装置的样例数据。 <a href="/guide/screenshots">English version</a> 是同样的界面配英文界面语言。</p><h2 id="主页与会话" tabindex="-1">主页与会话 <a class="header-anchor" href="#主页与会话" aria-label="Permalink to &quot;主页与会话&quot;">​</a></h2><p>主页是全新安装打开后的第一个界面：标题区、输入框，以及按项目分组会话的侧边栏。</p><p><img${ssrRenderAttr("src", _imports_0)} alt="浅色主题下的 PI-Desktop 主页"></p><p><img${ssrRenderAttr("src", _imports_1)} alt="深色主题下的 PI-Desktop 主页"></p><p><img${ssrRenderAttr("src", _imports_2)} alt="深色主题下的对话页"></p><p>对话流式写入正文，右侧是缩略导航条；鼠标划过导航条时标记会放大，并预览光标下的 那条消息。</p><p><img${ssrRenderAttr("src", _imports_3)} alt="带缩略导航条的对话"></p><p><img${ssrRenderAttr("src", _imports_4)} alt="光标下放大的缩略导航条"></p><p>Composer 的模型 × 推理芯片切换当前会话使用的模型。在输入框里，<code>/</code> 打开命令菜单，<code>@</code> 打开文件引用菜单。</p><p><img${ssrRenderAttr("src", _imports_5)} alt="Composer 中的模型和推理菜单"></p><p><img${ssrRenderAttr("src", _imports_6)} alt="输入框的斜杠命令菜单"></p><p><img${ssrRenderAttr("src", _imports_7)} alt="输入框的 @ 文件引用菜单"></p><h2 id="工作面板" tabindex="-1">工作面板 <a class="header-anchor" href="#工作面板" aria-label="Permalink to &quot;工作面板&quot;">​</a></h2><p>当智能体产出工作产物时，工作面板在对话旁边打开。下面几张是尚未打开工作区时的面板 状态，也就是一次新对话的起点。</p><p><img${ssrRenderAttr("src", _imports_8)} alt="审阅面板"></p><p><img${ssrRenderAttr("src", _imports_9)} alt="浏览器预览面板"></p><p><img${ssrRenderAttr("src", _imports_10)} alt="文件浏览面板"></p><p><img${ssrRenderAttr("src", _imports_11)} alt="工作面板切换菜单"></p><h2 id="目的地页" tabindex="-1">目的地页 <a class="header-anchor" href="#目的地页" aria-label="Permalink to &quot;目的地页&quot;">​</a></h2><p>合并请求、项目归档和定时任务都是从侧边栏进入的整页目的地。</p><p><img${ssrRenderAttr("src", _imports_12)} alt="合并请求页"></p><p><img${ssrRenderAttr("src", _imports_13)} alt="深色主题下的合并请求页"></p><p><img${ssrRenderAttr("src", _imports_14)} alt="项目归档"></p><p><img${ssrRenderAttr("src", _imports_15)} alt="深色主题下的项目归档"></p><p><img${ssrRenderAttr("src", _imports_16)} alt="定时任务"></p><h2 id="通知与提示" tabindex="-1">通知与提示 <a class="header-anchor" href="#通知与提示" aria-label="Permalink to &quot;通知与提示&quot;">​</a></h2><p>通知收件箱持久保留已完成的工作、权限请求和更新提醒；Toast 负责其中即时提示的部分。</p><p><img${ssrRenderAttr("src", _imports_17)} alt="浅色主题下的通知收件箱"></p><p><img${ssrRenderAttr("src", _imports_18)} alt="深色主题下的通知收件箱"></p><p><img${ssrRenderAttr("src", _imports_19)} alt="窄窗口下的通知浮层"></p><p><img${ssrRenderAttr("src", _imports_20)} alt="浅色主题下的成功、警告与错误提示"></p><p><img${ssrRenderAttr("src", _imports_21)} alt="深色主题下的成功、警告与错误提示"></p><h2 id="全局搜索" tabindex="-1">全局搜索 <a class="header-anchor" href="#全局搜索" aria-label="Permalink to &quot;全局搜索&quot;">​</a></h2><p><code>⌘K</code> 用一个弹窗同时搜索会话、页面、设置项和命令。选中设置项会跳到对应标签页并 高亮闪烁那一行。</p><p><img${ssrRenderAttr("src", _imports_22)} alt="显示最近会话的全局搜索"></p><p><img${ssrRenderAttr("src", _imports_23)} alt="匹配会话的全局搜索"></p><p><img${ssrRenderAttr("src", _imports_24)} alt="匹配设置项的全局搜索"></p><p><img${ssrRenderAttr("src", _imports_25)} alt="匹配目的地页的全局搜索"></p><p><img${ssrRenderAttr("src", _imports_26)} alt="从搜索跳转到的设置项"></p><p><img${ssrRenderAttr("src", _imports_27)} alt="深色主题下的全局搜索"></p><h2 id="插件" tabindex="-1">插件 <a class="header-anchor" href="#插件" aria-label="Permalink to &quot;插件&quot;">​</a></h2><p>已安装插件、插件市场和打包流程都在插件页上。</p><p><img${ssrRenderAttr("src", _imports_28)} alt="已安装的插件"></p><p><img${ssrRenderAttr("src", _imports_29)} alt="插件市场"></p><p><img${ssrRenderAttr("src", _imports_30)} alt="插件页菜单"></p><p><img${ssrRenderAttr("src", _imports_31)} alt="单个插件的行菜单"></p><p><img${ssrRenderAttr("src", _imports_32)} alt="新建插件模板弹窗"></p><h2 id="扩展" tabindex="-1">扩展 <a class="header-anchor" href="#扩展" aria-label="Permalink to &quot;扩展&quot;">​</a></h2><p>MCP 服务、技能和子智能体独立于插件管理，各自可以全局启用或按项目启用。</p><p><img${ssrRenderAttr("src", _imports_33)} alt="MCP 服务"></p><p><img${ssrRenderAttr("src", _imports_34)} alt="生效范围选择器"></p><p><img${ssrRenderAttr("src", _imports_35)} alt="MCP 服务编辑器"></p><p><img${ssrRenderAttr("src", _imports_36)} alt="技能"></p><p><img${ssrRenderAttr("src", _imports_37)} alt="子智能体"></p><p><img${ssrRenderAttr("src", _imports_38)} alt="插件提供的子智能体"></p><p><img${ssrRenderAttr("src", _imports_39)} alt="子智能体编辑器"></p><p><img${ssrRenderAttr("src", _imports_40)} alt="深色主题下的子智能体"></p><p><img${ssrRenderAttr("src", _imports_41)} alt="深色主题下的 MCP 服务"></p><h2 id="设置" tabindex="-1">设置 <a class="header-anchor" href="#设置" aria-label="Permalink to &quot;设置&quot;">​</a></h2><p>设置是一个整页目的地，左侧是可搜索的标签栏。</p><p><img${ssrRenderAttr("src", _imports_42)} alt="基础——语言、主题与外观"></p><p><img${ssrRenderAttr("src", _imports_43)} alt="深色主题下的基础设置"></p><p><img${ssrRenderAttr("src", _imports_44)} alt="模型配置中的默认模型与 AI 服务"></p><p><img${ssrRenderAttr("src", _imports_45)} alt="扩展市场中的插件目录源选择"></p><p><img${ssrRenderAttr("src", _imports_46)} alt="扩展市场中的自定义插件目录地址"></p><h2 id="如何重新生成" tabindex="-1">如何重新生成 <a class="header-anchor" href="#如何重新生成" aria-label="Permalink to &quot;如何重新生成&quot;">​</a></h2><p>先构建渲染层，确认 <code>target/debug/pi-desktop-host-core</code> 存在，创建 <code>/tmp/codex-screens</code>，然后按语言各跑一遍并发布：</p><div class="language-bash vp-adaptive-theme line-numbers-mode"><button title="Copy Code" class="copy"></button><span class="lang">bash</span><pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span style="${ssrRenderStyle({ "--shiki-light": "#6F42C1", "--shiki-dark": "#B392F0" })}">pnpm</span><span style="${ssrRenderStyle({ "--shiki-light": "#005CC5", "--shiki-dark": "#79B8FF" })}"> --filter</span><span style="${ssrRenderStyle({ "--shiki-light": "#032F62", "--shiki-dark": "#9ECBFF" })}"> @pi-desktop/desktop</span><span style="${ssrRenderStyle({ "--shiki-light": "#032F62", "--shiki-dark": "#9ECBFF" })}"> build</span></span>
<span class="line"><span style="${ssrRenderStyle({ "--shiki-light": "#6F42C1", "--shiki-dark": "#B392F0" })}">mkdir</span><span style="${ssrRenderStyle({ "--shiki-light": "#005CC5", "--shiki-dark": "#79B8FF" })}"> -p</span><span style="${ssrRenderStyle({ "--shiki-light": "#032F62", "--shiki-dark": "#9ECBFF" })}"> /tmp/codex-screens</span></span>
<span class="line"></span>
<span class="line"><span style="${ssrRenderStyle({ "--shiki-light": "#6A737D", "--shiki-dark": "#6A737D" })}"># 英文一遍；中文一遍在命令末尾加 --lang=zh-CN。</span></span>
<span class="line"><span style="${ssrRenderStyle({ "--shiki-light": "#005CC5", "--shiki-dark": "#79B8FF" })}">cd</span><span style="${ssrRenderStyle({ "--shiki-light": "#032F62", "--shiki-dark": "#9ECBFF" })}"> apps/desktop</span><span style="${ssrRenderStyle({ "--shiki-light": "#24292E", "--shiki-dark": "#E1E4E8" })}"> &amp;&amp; PI_DESKTOP_CAPTURE</span><span style="${ssrRenderStyle({ "--shiki-light": "#D73A49", "--shiki-dark": "#F97583" })}">=</span><span style="${ssrRenderStyle({ "--shiki-light": "#032F62", "--shiki-dark": "#9ECBFF" })}">1</span><span style="${ssrRenderStyle({ "--shiki-light": "#24292E", "--shiki-dark": "#E1E4E8" })}"> PI_DESKTOP_DATA_DIR</span><span style="${ssrRenderStyle({ "--shiki-light": "#D73A49", "--shiki-dark": "#F97583" })}">=</span><span style="${ssrRenderStyle({ "--shiki-light": "#24292E", "--shiki-dark": "#E1E4E8" })}">$(</span><span style="${ssrRenderStyle({ "--shiki-light": "#6F42C1", "--shiki-dark": "#B392F0" })}">mktemp</span><span style="${ssrRenderStyle({ "--shiki-light": "#005CC5", "--shiki-dark": "#79B8FF" })}"> -d</span><span style="${ssrRenderStyle({ "--shiki-light": "#24292E", "--shiki-dark": "#E1E4E8" })}">) </span><span style="${ssrRenderStyle({ "--shiki-light": "#6F42C1", "--shiki-dark": "#B392F0" })}">\\</span></span>
<span class="line"><span style="${ssrRenderStyle({ "--shiki-light": "#032F62", "--shiki-dark": "#9ECBFF" })}">  ELECTRON_RENDERER_URL=</span><span style="${ssrRenderStyle({ "--shiki-light": "#032F62", "--shiki-dark": "#9ECBFF" })}"> ./node_modules/.bin/electron</span><span style="${ssrRenderStyle({ "--shiki-light": "#032F62", "--shiki-dark": "#9ECBFF" })}"> .</span></span>
<span class="line"></span>
<span class="line"><span style="${ssrRenderStyle({ "--shiki-light": "#6F42C1", "--shiki-dark": "#B392F0" })}">python3</span><span style="${ssrRenderStyle({ "--shiki-light": "#032F62", "--shiki-dark": "#9ECBFF" })}"> scripts/publish-screenshots.py</span><span style="${ssrRenderStyle({ "--shiki-light": "#005CC5", "--shiki-dark": "#79B8FF" })}"> --source</span><span style="${ssrRenderStyle({ "--shiki-light": "#032F62", "--shiki-dark": "#9ECBFF" })}"> /tmp/codex-screens</span><span style="${ssrRenderStyle({ "--shiki-light": "#005CC5", "--shiki-dark": "#79B8FF" })}"> --locale</span><span style="${ssrRenderStyle({ "--shiki-light": "#032F62", "--shiki-dark": "#9ECBFF" })}"> zh</span></span></code></pre><div class="line-numbers-wrapper" aria-hidden="true"><span class="line-number">1</span><br><span class="line-number">2</span><br><span class="line-number">3</span><br><span class="line-number">4</span><br><span class="line-number">5</span><br><span class="line-number">6</span><br><span class="line-number">7</span><br><span class="line-number">8</span><br></div></div><p>最后一张写完后，装置会在标准输出打印 <code>CAPTURE_DONE</code>。</p></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/guide/screenshots.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const screenshots = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  screenshots as default
};
