import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"03. Repo Structure","description":"","frontmatter":{},"headers":[],"relativePath":"spec/02-architecture/03-repo-structure.md","filePath":"spec/02-architecture/03-repo-structure.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "spec/02-architecture/03-repo-structure.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="_03-repo-structure" tabindex="-1">03. Repo Structure <a class="header-anchor" href="#_03-repo-structure" aria-label="Permalink to &quot;03. Repo Structure&quot;">​</a></h1><h2 id="_1-workspace-layout" tabindex="-1">1. Workspace layout <a class="header-anchor" href="#_1-workspace-layout" aria-label="Permalink to &quot;1. Workspace layout&quot;">​</a></h2><p>Two workspaces share one repository: pnpm owns every JavaScript package (<code>apps/*</code>, <code>packages/*</code>, <code>docs</code>), Cargo owns the Rust crate. Root <code>package.json</code> scripts fan out to both.</p><div class="language-text vp-adaptive-theme line-numbers-mode"><button title="Copy Code" class="copy"></button><span class="lang">text</span><pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span>PI-Desktop/</span></span>
<span class="line"><span>├── apps/</span></span>
<span class="line"><span>│ └── desktop/                # Electron product shell</span></span>
<span class="line"><span>│   ├── electron/</span></span>
<span class="line"><span>│   │ ├── main/               # main process, one module per concern</span></span>
<span class="line"><span>│   │ ├── preload/            # renderer and plugin-panel preloads</span></span>
<span class="line"><span>│   │ └── shared/             # code both main and preload import</span></span>
<span class="line"><span>│   ├── src/                  # React renderer</span></span>
<span class="line"><span>│   │ ├── components/         # UI; settings/, workpanel/, plugins/, extensions/</span></span>
<span class="line"><span>│   │ ├── hooks/              # React hooks</span></span>
<span class="line"><span>│   │ ├── lib/                # framework-free renderer logic and the IPC client</span></span>
<span class="line"><span>│   │ ├── pages/              # routed destinations</span></span>
<span class="line"><span>│   │ ├── stores/             # zustand app store</span></span>
<span class="line"><span>│   │ ├── styles/             # CSS split by surface; tokens.css is the design system</span></span>
<span class="line"><span>│   │ └── assets/             # fonts and brand art</span></span>
<span class="line"><span>│   ├── test/                 # node --test suites (*.test.mjs) and helpers/</span></span>
<span class="line"><span>│   ├── resources/            # packaged extraResources: skills/, plugins/, models.dev/</span></span>
<span class="line"><span>│   ├── build/                # icons and macOS entitlements for electron-builder</span></span>
<span class="line"><span>│   ├── index.html</span></span>
<span class="line"><span>│   ├── electron.vite.config.ts</span></span>
<span class="line"><span>│   └── package.json          # also holds the electron-builder config</span></span>
<span class="line"><span>├── crates/</span></span>
<span class="line"><span>│ └── host-core/              # Rust privileged host (binary pi-desktop-host-core)</span></span>
<span class="line"><span>│   ├── Cargo.toml</span></span>
<span class="line"><span>│   └── src/                  # rpc/, tools/, plus one module per domain</span></span>
<span class="line"><span>├── packages/</span></span>
<span class="line"><span>│ ├── shared/                 # IPC/protocol contracts, error codes, changelog</span></span>
<span class="line"><span>│ ├── i18n/                   # shipped UI catalogs plus locale helpers</span></span>
<span class="line"><span>│ ├── agent-runtime/          # pi sidecar and runtime wrapper (bundled into the app)</span></span>
<span class="line"><span>│ ├── plugin-sdk/             # plugin author types and validators</span></span>
<span class="line"><span>│ └── plugin-devkit/          # pi-plugin CLI: scaffold, check, pack, publish</span></span>
<span class="line"><span>├── examples/</span></span>
<span class="line"><span>│ ├── plugins/                # hello and roundtable sample plugins</span></span>
<span class="line"><span>│ └── fixtures/sample-project # workspace fixture for E2E scenarios</span></span>
<span class="line"><span>├── docs/                     # VitePress site and the English source of truth</span></span>
<span class="line"><span>│ ├── spec/                   # numbered specification domains (see spec/README.md)</span></span>
<span class="line"><span>│ ├── adr/                    # architecture decision records</span></span>
<span class="line"><span>│ ├── project/                # board, audits, implementation plans</span></span>
<span class="line"><span>│ ├── guide/                  # user-facing quick guide</span></span>
<span class="line"><span>│ ├── zh-CN/                  # path-for-path Chinese mirror of spec/ and guide/</span></span>
<span class="line"><span>│ ├── image/                  # images embedded by the repository READMEs</span></span>
<span class="line"><span>│ ├── public/                 # static assets served by the docs site</span></span>
<span class="line"><span>│ ├── scripts/                # docs-only checks (check-locales.mjs)</span></span>
<span class="line"><span>│ └── .vitepress/             # site config and theme</span></span>
<span class="line"><span>├── scripts/                  # repository automation (see scripts/README.md)</span></span>
<span class="line"><span>├── .github/                  # CI and release workflows, issue templates</span></span>
<span class="line"><span>├── AGENTS.md                 # mandatory rules for AI coding agents</span></span>
<span class="line"><span>├── package.json              # root scripts, pnpm workspace</span></span>
<span class="line"><span>├── pnpm-workspace.yaml</span></span>
<span class="line"><span>├── Cargo.toml                # Rust workspace</span></span>
<span class="line"><span>└── README.md · README.zh-CN.md</span></span></code></pre><div class="line-numbers-wrapper" aria-hidden="true"><span class="line-number">1</span><br><span class="line-number">2</span><br><span class="line-number">3</span><br><span class="line-number">4</span><br><span class="line-number">5</span><br><span class="line-number">6</span><br><span class="line-number">7</span><br><span class="line-number">8</span><br><span class="line-number">9</span><br><span class="line-number">10</span><br><span class="line-number">11</span><br><span class="line-number">12</span><br><span class="line-number">13</span><br><span class="line-number">14</span><br><span class="line-number">15</span><br><span class="line-number">16</span><br><span class="line-number">17</span><br><span class="line-number">18</span><br><span class="line-number">19</span><br><span class="line-number">20</span><br><span class="line-number">21</span><br><span class="line-number">22</span><br><span class="line-number">23</span><br><span class="line-number">24</span><br><span class="line-number">25</span><br><span class="line-number">26</span><br><span class="line-number">27</span><br><span class="line-number">28</span><br><span class="line-number">29</span><br><span class="line-number">30</span><br><span class="line-number">31</span><br><span class="line-number">32</span><br><span class="line-number">33</span><br><span class="line-number">34</span><br><span class="line-number">35</span><br><span class="line-number">36</span><br><span class="line-number">37</span><br><span class="line-number">38</span><br><span class="line-number">39</span><br><span class="line-number">40</span><br><span class="line-number">41</span><br><span class="line-number">42</span><br><span class="line-number">43</span><br><span class="line-number">44</span><br><span class="line-number">45</span><br><span class="line-number">46</span><br><span class="line-number">47</span><br><span class="line-number">48</span><br><span class="line-number">49</span><br><span class="line-number">50</span><br><span class="line-number">51</span><br></div></div><h2 id="split-domain-facades" tabindex="-1">Split-domain facades <a class="header-anchor" href="#split-domain-facades" aria-label="Permalink to &quot;Split-domain facades&quot;">​</a></h2><p>Large entry points remain compatibility facades while their implementation is owned by domain modules. Electron main wires <code>ipc/</code>, <code>runtime/</code>, <code>bootstrap/</code>, and <code>services/</code>; renderer page entry points delegate to <code>features/app</code>, <code>features/plugins</code>, and <code>features/settings</code>; and host-core facades delegate to the <code>plugins/</code>, <code>db/</code>, <code>providers/</code>, and <code>plans/</code> submodules. The shared <code>types.ts</code> entry point re-exports the domain files under <code>shared/src/types/</code>.</p><p>The facade paths preserve existing imports and public contracts. New logic belongs in the domain module that owns its state or process boundary.</p><p>Source budgets are reported and enforced by <a href="./../../architecture/README"><code>scripts/check-architecture.mjs</code></a>. Its allowlist records only existing extraction debt with a reason.</p><h2 id="_2-package-responsibilities" tabindex="-1">2. Package responsibilities <a class="header-anchor" href="#_2-package-responsibilities" aria-label="Permalink to &quot;2. Package responsibilities&quot;">​</a></h2><h3 id="apps-desktop" tabindex="-1"><code>apps/desktop</code> <a class="header-anchor" href="#apps-desktop" aria-label="Permalink to &quot;\`apps/desktop\`&quot;">​</a></h3><p>Product entry:</p><ul><li>Electron lifecycle, windows, tray, application menu</li><li>IPC surface between renderer and main</li><li>host-core and sidecar process supervision</li><li>plugin runtime, panels, and views</li><li>packaging configuration</li></ul><h3 id="crates-host-core" tabindex="-1"><code>crates/host-core</code> <a class="header-anchor" href="#crates-host-core" aria-label="Permalink to &quot;\`crates/host-core\`&quot;">​</a></h3><p>Rust host services:</p><ul><li>tools execution</li><li>permission gateway</li><li>plugin host services</li><li>persistence (SQLite, transcripts, artifacts, secrets)</li><li>audit logging</li></ul><h3 id="packages-agent-runtime" tabindex="-1"><code>packages/agent-runtime</code> <a class="header-anchor" href="#packages-agent-runtime" aria-label="Permalink to &quot;\`packages/agent-runtime\`&quot;">​</a></h3><p>Node wrapper over pi:</p><ul><li>model bootstrap</li><li>agent turn control</li><li>event normalization</li><li>host tool bridge client</li></ul><h3 id="packages-shared" tabindex="-1"><code>packages/shared</code> <a class="header-anchor" href="#packages-shared" aria-label="Permalink to &quot;\`packages/shared\`&quot;">​</a></h3><p>Cross-boundary contracts:</p><ul><li>IPC channel names</li><li>DTO types, split by domain under <code>src/types/</code> and re-exported from <code>types.ts</code></li><li>error codes</li><li>protocol versioning</li><li>changelog entries surfaced in the app</li></ul><h3 id="packages-i18n" tabindex="-1"><code>packages/i18n</code> <a class="header-anchor" href="#packages-i18n" aria-label="Permalink to &quot;\`packages/i18n\`&quot;">​</a></h3><ul><li>English source catalog and shipped translated catalogs</li><li>locale registry and resolution helpers</li><li>message ID conventions</li></ul><h3 id="packages-plugin-sdk" tabindex="-1"><code>packages/plugin-sdk</code> <a class="header-anchor" href="#packages-plugin-sdk" aria-label="Permalink to &quot;\`packages/plugin-sdk\`&quot;">​</a></h3><ul><li>manifest types</li><li>host API types</li><li>validators</li></ul><h3 id="packages-plugin-devkit" tabindex="-1"><code>packages/plugin-devkit</code> <a class="header-anchor" href="#packages-plugin-devkit" aria-label="Permalink to &quot;\`packages/plugin-devkit\`&quot;">​</a></h3><ul><li><code>pi-plugin</code> CLI used by plugin authors and the marketplace publish flow</li><li>template scaffolding, manifest check, package, publish</li></ul><h2 id="_3-runtime-data-not-in-git" tabindex="-1">3. Runtime data (not in git) <a class="header-anchor" href="#_3-runtime-data-not-in-git" aria-label="Permalink to &quot;3. Runtime data (not in git)&quot;">​</a></h2><p><code>PI_DESKTOP_DATA_DIR</code> overrides the default location.</p><div class="language-text vp-adaptive-theme line-numbers-mode"><button title="Copy Code" class="copy"></button><span class="lang">text</span><pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span>~/.pi-desktop/</span></span>
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
<span class="line"><span> └── close-behavior.json     # persisted close-to-tray choice (main)</span></span></code></pre><div class="line-numbers-wrapper" aria-hidden="true"><span class="line-number">1</span><br><span class="line-number">2</span><br><span class="line-number">3</span><br><span class="line-number">4</span><br><span class="line-number">5</span><br><span class="line-number">6</span><br><span class="line-number">7</span><br><span class="line-number">8</span><br><span class="line-number">9</span><br><span class="line-number">10</span><br><span class="line-number">11</span><br><span class="line-number">12</span><br><span class="line-number">13</span><br><span class="line-number">14</span><br><span class="line-number">15</span><br><span class="line-number">16</span><br><span class="line-number">17</span><br><span class="line-number">18</span><br><span class="line-number">19</span><br><span class="line-number">20</span><br><span class="line-number">21</span><br></div></div><h2 id="_4-naming-conventions" tabindex="-1">4. Naming conventions <a class="header-anchor" href="#_4-naming-conventions" aria-label="Permalink to &quot;4. Naming conventions&quot;">​</a></h2><table tabindex="0"><thead><tr><th>Object</th><th>Convention</th></tr></thead><tbody><tr><td>JS packages</td><td><code>@pi-desktop/*</code></td></tr><tr><td>Rust crate</td><td><code>pi-desktop-host-core</code> (or <code>host-core</code>)</td></tr><tr><td>IPC channels</td><td><code>pi-desktop/&lt;domain&gt;/&lt;action&gt;</code></td></tr><tr><td>i18n keys</td><td><code>domain.section.key</code></td></tr><tr><td>Plugin IDs</td><td>reverse-domain style</td></tr><tr><td>Main-process modules</td><td>one file per concern under <code>electron/main/</code>; <code>index.ts</code> wires them</td></tr><tr><td>Renderer tests</td><td><code>apps/desktop/test/&lt;subject&gt;.test.mjs</code>, never beside the source</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("spec/02-architecture/03-repo-structure.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const _03RepoStructure = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  _03RepoStructure as default
};
