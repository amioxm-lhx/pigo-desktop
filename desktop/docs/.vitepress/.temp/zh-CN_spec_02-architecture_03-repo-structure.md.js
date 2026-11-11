import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"03. 仓库结构","description":"","frontmatter":{},"headers":[],"relativePath":"zh-CN/spec/02-architecture/03-repo-structure.md","filePath":"zh-CN/spec/02-architecture/03-repo-structure.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/02-architecture/03-repo-structure.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="_03-仓库结构" tabindex="-1">03. 仓库结构 <a class="header-anchor" href="#_03-仓库结构" aria-label="Permalink to &quot;03. 仓库结构&quot;">​</a></h1><blockquote><p><strong>翻译说明：</strong> 本页是与 <a href="/spec/02-architecture/03-repo-structure">英文源规格</a> 一一对应的机器辅助翻译。代码、协议字段和标识符保持原文；如翻译与英文源事实有歧义，以英文版本为准。</p></blockquote><h2 id="_1-工作区布局" tabindex="-1">1. 工作区布局 <a class="header-anchor" href="#_1-工作区布局" aria-label="Permalink to &quot;1. 工作区布局&quot;">​</a></h2><p>一个仓库里有两个工作区：pnpm 管理全部 JavaScript 包（<code>apps/*</code>、<code>packages/*</code>、<code>docs</code>），Cargo 管理 Rust crate。根目录 <code>package.json</code> 的脚本同时驱动两者。</p><div class="language-text vp-adaptive-theme line-numbers-mode"><button title="Copy Code" class="copy"></button><span class="lang">text</span><pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span>PI-Desktop/</span></span>
<span class="line"><span>├── apps/</span></span>
<span class="line"><span>│ └── desktop/                # Electron 产品外壳</span></span>
<span class="line"><span>│   ├── electron/</span></span>
<span class="line"><span>│   │ ├── main/               # 主进程，每个关注点一个模块</span></span>
<span class="line"><span>│   │ ├── preload/            # 渲染器与插件面板的 preload</span></span>
<span class="line"><span>│   │ └── shared/             # main 与 preload 共用的代码</span></span>
<span class="line"><span>│   ├── src/                  # React 渲染器</span></span>
<span class="line"><span>│   │ ├── components/         # UI；含 settings/、workpanel/、plugins/、extensions/</span></span>
<span class="line"><span>│   │ ├── hooks/              # React hooks</span></span>
<span class="line"><span>│   │ ├── lib/                # 与框架无关的渲染器逻辑和 IPC 客户端</span></span>
<span class="line"><span>│   │ ├── pages/              # 路由目的页</span></span>
<span class="line"><span>│   │ ├── stores/             # zustand 应用 store</span></span>
<span class="line"><span>│   │ ├── styles/             # 按界面拆分的 CSS；tokens.css 是设计系统</span></span>
<span class="line"><span>│   │ └── assets/             # 字体与品牌素材</span></span>
<span class="line"><span>│   ├── test/                 # node --test 套件（*.test.mjs）与 helpers/</span></span>
<span class="line"><span>│   ├── resources/            # 打包 extraResources：skills/、plugins/、models.dev/</span></span>
<span class="line"><span>│   ├── build/                # electron-builder 用的图标与 macOS entitlements</span></span>
<span class="line"><span>│   ├── index.html</span></span>
<span class="line"><span>│   ├── electron.vite.config.ts</span></span>
<span class="line"><span>│   └── package.json          # 同时承载 electron-builder 配置</span></span>
<span class="line"><span>├── crates/</span></span>
<span class="line"><span>│ └── host-core/              # Rust 特权宿主（二进制 pi-desktop-host-core）</span></span>
<span class="line"><span>│   ├── Cargo.toml</span></span>
<span class="line"><span>│   └── src/                  # rpc/、tools/，其余每个领域一个模块</span></span>
<span class="line"><span>├── packages/</span></span>
<span class="line"><span>│ ├── shared/                 # IPC/协议契约、错误码、更新日志</span></span>
<span class="line"><span>│ ├── i18n/                   # en 与 zh-CN 目录及 locale 辅助函数</span></span>
<span class="line"><span>│ ├── agent-runtime/          # pi sidecar 与运行时包装（打包进应用）</span></span>
<span class="line"><span>│ ├── plugin-sdk/             # 插件作者类型与校验器</span></span>
<span class="line"><span>│ └── plugin-devkit/          # pi-plugin CLI：scaffold、check、pack、publish</span></span>
<span class="line"><span>├── examples/</span></span>
<span class="line"><span>│ ├── plugins/                # hello 与 roundtable 示例插件</span></span>
<span class="line"><span>│ └── fixtures/sample-project # E2E 场景使用的工作区 fixture</span></span>
<span class="line"><span>├── docs/                     # VitePress 站点与英文源事实</span></span>
<span class="line"><span>│ ├── spec/                   # 编号的规格领域（见 spec/README.md）</span></span>
<span class="line"><span>│ ├── adr/                    # 架构决策记录</span></span>
<span class="line"><span>│ ├── project/                # 看板、审计、实施计划</span></span>
<span class="line"><span>│ ├── guide/                  # 面向用户的快速指南</span></span>
<span class="line"><span>│ ├── zh-CN/                  # spec/ 与 guide/ 的逐路径中文镜像</span></span>
<span class="line"><span>│ ├── image/                  # 仓库 README 内嵌的图片</span></span>
<span class="line"><span>│ ├── public/                 # 文档站提供的静态资源</span></span>
<span class="line"><span>│ ├── scripts/                # 仅文档使用的检查（check-locales.mjs）</span></span>
<span class="line"><span>│ └── .vitepress/             # 站点配置与主题</span></span>
<span class="line"><span>├── scripts/                  # 仓库自动化（见 scripts/README.md）</span></span>
<span class="line"><span>├── .github/                  # CI 与发布工作流、issue 模板</span></span>
<span class="line"><span>├── AGENTS.md                 # AI 编码代理的强制规则</span></span>
<span class="line"><span>├── package.json              # 根脚本、pnpm 工作区</span></span>
<span class="line"><span>├── pnpm-workspace.yaml</span></span>
<span class="line"><span>├── Cargo.toml                # Rust 工作区</span></span>
<span class="line"><span>└── README.md · README.zh-CN.md</span></span></code></pre><div class="line-numbers-wrapper" aria-hidden="true"><span class="line-number">1</span><br><span class="line-number">2</span><br><span class="line-number">3</span><br><span class="line-number">4</span><br><span class="line-number">5</span><br><span class="line-number">6</span><br><span class="line-number">7</span><br><span class="line-number">8</span><br><span class="line-number">9</span><br><span class="line-number">10</span><br><span class="line-number">11</span><br><span class="line-number">12</span><br><span class="line-number">13</span><br><span class="line-number">14</span><br><span class="line-number">15</span><br><span class="line-number">16</span><br><span class="line-number">17</span><br><span class="line-number">18</span><br><span class="line-number">19</span><br><span class="line-number">20</span><br><span class="line-number">21</span><br><span class="line-number">22</span><br><span class="line-number">23</span><br><span class="line-number">24</span><br><span class="line-number">25</span><br><span class="line-number">26</span><br><span class="line-number">27</span><br><span class="line-number">28</span><br><span class="line-number">29</span><br><span class="line-number">30</span><br><span class="line-number">31</span><br><span class="line-number">32</span><br><span class="line-number">33</span><br><span class="line-number">34</span><br><span class="line-number">35</span><br><span class="line-number">36</span><br><span class="line-number">37</span><br><span class="line-number">38</span><br><span class="line-number">39</span><br><span class="line-number">40</span><br><span class="line-number">41</span><br><span class="line-number">42</span><br><span class="line-number">43</span><br><span class="line-number">44</span><br><span class="line-number">45</span><br><span class="line-number">46</span><br><span class="line-number">47</span><br><span class="line-number">48</span><br><span class="line-number">49</span><br><span class="line-number">50</span><br><span class="line-number">51</span><br></div></div><h2 id="_2-包职责" tabindex="-1">2. 包职责 <a class="header-anchor" href="#_2-包职责" aria-label="Permalink to &quot;2. 包职责&quot;">​</a></h2><h3 id="apps-desktop" tabindex="-1"><code>apps/desktop</code> <a class="header-anchor" href="#apps-desktop" aria-label="Permalink to &quot;\`apps/desktop\`&quot;">​</a></h3><p>产品入口：</p><ul><li>Electron 生命周期、窗口、托盘、应用菜单</li><li>渲染器与主进程之间的 IPC 面</li><li>host-core 与 sidecar 进程监管</li><li>插件运行时、面板与视图</li><li>打包配置</li></ul><h3 id="crates-host-core" tabindex="-1"><code>crates/host-core</code> <a class="header-anchor" href="#crates-host-core" aria-label="Permalink to &quot;\`crates/host-core\`&quot;">​</a></h3><p>Rust 宿主服务：</p><ul><li>工具执行</li><li>权限网关</li><li>插件宿主服务</li><li>持久化（SQLite、转录、工件、密钥）</li><li>审计日志</li></ul><h3 id="packages-agent-runtime" tabindex="-1"><code>packages/agent-runtime</code> <a class="header-anchor" href="#packages-agent-runtime" aria-label="Permalink to &quot;\`packages/agent-runtime\`&quot;">​</a></h3><p>Node 对 pi 的包装：</p><ul><li>模型引导</li><li>代理回合控制</li><li>事件标准化</li><li>宿主工具桥客户端</li></ul><h3 id="packages-shared" tabindex="-1"><code>packages/shared</code> <a class="header-anchor" href="#packages-shared" aria-label="Permalink to &quot;\`packages/shared\`&quot;">​</a></h3><p>跨边界契约：</p><ul><li>IPC 通道名称</li><li>DTO 类型</li><li>错误码</li><li>协议版本</li><li>应用内展示的更新日志条目</li></ul><h3 id="packages-i18n" tabindex="-1"><code>packages/i18n</code> <a class="header-anchor" href="#packages-i18n" aria-label="Permalink to &quot;\`packages/i18n\`&quot;">​</a></h3><ul><li>英文源目录与 zh-CN 目录</li><li>locale 解析辅助函数</li><li>消息 ID 约定</li></ul><h3 id="packages-plugin-sdk" tabindex="-1"><code>packages/plugin-sdk</code> <a class="header-anchor" href="#packages-plugin-sdk" aria-label="Permalink to &quot;\`packages/plugin-sdk\`&quot;">​</a></h3><ul><li>清单类型</li><li>宿主 API 类型</li><li>校验器</li></ul><h3 id="packages-plugin-devkit" tabindex="-1"><code>packages/plugin-devkit</code> <a class="header-anchor" href="#packages-plugin-devkit" aria-label="Permalink to &quot;\`packages/plugin-devkit\`&quot;">​</a></h3><ul><li>插件作者与市场发布流程使用的 <code>pi-plugin</code> CLI</li><li>模板脚手架、清单检查、打包、发布</li></ul><h2 id="_3-运行时数据-不在-git-中" tabindex="-1">3. 运行时数据（不在 git 中） <a class="header-anchor" href="#_3-运行时数据-不在-git-中" aria-label="Permalink to &quot;3. 运行时数据（不在 git 中）&quot;">​</a></h2><p><code>PI_DESKTOP_DATA_DIR</code> 可覆盖默认位置。</p><div class="language-text vp-adaptive-theme line-numbers-mode"><button title="Copy Code" class="copy"></button><span class="lang">text</span><pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span>~/.pi-desktop/</span></span>
<span class="line"><span> ├── pi.sqlite               # single DB, host-core owned (03-runtime/04, D086)</span></span>
<span class="line"><span> ├── sessions/               # per-session transcript files (D119)</span></span>
<span class="line"><span> ├── artifacts/              # plan and goal checkpoint artifacts</span></span>
<span class="line"><span> ├── attachments/            # content-addressed prompt attachment blobs (main)</span></span>
<span class="line"><span> ├── scratch/&lt;sessionId&gt;/    # session-scoped temporary files</span></span>
<span class="line"><span> ├── secrets/</span></span>
<span class="line"><span> ├── logs/</span></span>
<span class="line"><span> │    ├── app/&lt;category&gt;.log</span></span>
<span class="line"><span> │    ├── host/&lt;category&gt;.log</span></span>
<span class="line"><span> │    └── agent/&lt;category&gt;.log</span></span>
<span class="line"><span> ├── cache/</span></span>
<span class="line"><span> ├── plugins/</span></span>
<span class="line"><span> │    ├── installed/</span></span>
<span class="line"><span> │    ├── data/</span></span>
<span class="line"><span> │    ├── logs/</span></span>
<span class="line"><span> │    ├── cache/</span></span>
<span class="line"><span> │    ├── market/             # catalog and downloaded packages</span></span>
<span class="line"><span> │    └── registry.json</span></span>
<span class="line"><span> ├── window-state.json       # last main-window bounds (main)</span></span>
<span class="line"><span> └── close-behavior.json     # persisted close-to-tray choice (main)</span></span></code></pre><div class="line-numbers-wrapper" aria-hidden="true"><span class="line-number">1</span><br><span class="line-number">2</span><br><span class="line-number">3</span><br><span class="line-number">4</span><br><span class="line-number">5</span><br><span class="line-number">6</span><br><span class="line-number">7</span><br><span class="line-number">8</span><br><span class="line-number">9</span><br><span class="line-number">10</span><br><span class="line-number">11</span><br><span class="line-number">12</span><br><span class="line-number">13</span><br><span class="line-number">14</span><br><span class="line-number">15</span><br><span class="line-number">16</span><br><span class="line-number">17</span><br><span class="line-number">18</span><br><span class="line-number">19</span><br><span class="line-number">20</span><br><span class="line-number">21</span><br></div></div><h2 id="_4-命名约定" tabindex="-1">4. 命名约定 <a class="header-anchor" href="#_4-命名约定" aria-label="Permalink to &quot;4. 命名约定&quot;">​</a></h2><table tabindex="0"><thead><tr><th>对象</th><th>约定</th></tr></thead><tbody><tr><td>JS 包</td><td><code>@pi-desktop/*</code></td></tr><tr><td>Rust crate</td><td><code>pi-desktop-host-core</code>（或 <code>host-core</code>）</td></tr><tr><td>IPC 通道</td><td><code>pi-desktop/&lt;domain&gt;/&lt;action&gt;</code></td></tr><tr><td>i18n 键</td><td><code>domain.section.key</code></td></tr><tr><td>插件 ID</td><td>反向域名风格</td></tr><tr><td>主进程模块</td><td><code>electron/main/</code> 下每个关注点一个文件；<code>index.ts</code> 负责接线</td></tr><tr><td>渲染器测试</td><td><code>apps/desktop/test/&lt;subject&gt;.test.mjs</code>，不放在源码旁</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/02-architecture/03-repo-structure.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _03RepoStructure = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _03RepoStructure as default
};
