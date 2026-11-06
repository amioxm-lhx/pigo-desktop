# pigo-desktop · Tauri Shell

`apps/desktop-tauri` **就是桌面应用本体**（2026-10-06 起 Electron 已完全移除）：
渲染层（`src/`，React 19）+ Tauri 2 壳（`src-tauri/`）在同一目录，仓库里不再有
Electron 代码。被移除的 Electron 实现备份在
`D:\projectackup\pi-go-desktop-electron-20261006\`（含未提交改动的完整补丁）。

```
┌─────────────────────────────┐
│ renderer (React, 不变)        │  window.piDesktop.invoke/on  ← 同一套 275 通道
└──────────────┬──────────────┘
               │ tauri invoke("pi_desktop_invoke") / tauri events
┌──────────────▼──────────────┐
│ Tauri Rust core (本目录)      │  ← 原 Electron main 的职责（已删除）
│  · 通道路由 (257 通道)         │
│  · host-core / Go sidecar    │  stdio NDJSON JSON-RPC（协议不变）
│    进程监督 + 崩溃重启          │
│  · 事件扇出 + 消息持久化        │
│  · 窗口/对话框/通知/打开路径     │
└──────┬───────────────┬──────┘
       │ stdio         │ stdio
┌──────▼─────┐  ┌──────▼─────────────┐
│ host-core  │  │ pi-go-desktop-agent│   （两个后端进程，未改动）
│ (Rust,持久) │  │ (Go, agent 引擎)    │
└────────────┘  └────────────────────┘
```

## 为什么渲染层可以不动

Electron 的 preload 只暴露一个通道式 bridge（`window.piDesktop`，6 个成员），
`src/tauri-bridge.ts` 在检测到 `__TAURI_INTERNALS__` 时安装同形 API：

| preload 成员 | Tauri shim 实现 |
|---|---|
| `invoke(channel, ...args)` | `invoke("pi_desktop_invoke", {channel, args})`，信封在 Rust 侧组装，永不 reject |
| `on(channel, cb)` | 每 channel 一条 tauri `listen`，同步反注册（Electron 语义） |
| `channels` | 直接 import `@pi-desktop/shared` 的 `IPC` 常量表 |
| `platform` | navigator 推断（首帧前同步） |
| `locale` | `navigator.language` |
| `getDroppedFilePath(file)` | Tauri drag-drop 事件的 paths 按文件名匹配（近似） |

另外 Electron 的 `-webkit-app-region: drag` 在 WebView2 无效——窗口拖拽由
shim 后续补（阶段 2，`startDragging` mousedown 探测），当前用系统边框调整大小，
标题栏拖动暂缺。

## 本地 Tauri 源码

`tauri` / `tauri-utils` 通过 workspace 根 `[patch.crates-io]` 指向
`D:/project/myopensource/有价值5/tauri`（Tauri 2.12.1 checkout），官方插件
（dialog / opener / notification / single-instance）从 crates.io 引入并被同一
patch 归一到本地源码，避免双份 tauri 类型。

## 开发 / 构建

```bash
# 后端二进制（一次性，产物在 workspace target/ 与 sidecar/build/）
cargo build --release -p host-core
go build -o sidecar/build/pi-go-desktop-agent.exe ./sidecar   # 已有产物可跳过

# 渲染层 → apps/desktop-tauri/dist
pnpm --filter @pi-desktop/desktop build

# 运行（桌面壳直接加载 dist；无 CLI 依赖）
# 插件命令/工具/主题需要系统 Node.js 18+；缺失时其余功能正常，插件进程不启动。
# 可用 PI_DESKTOP_NODE_BIN 指定 node 路径。
cargo run -p pigo-desktop

# 开发热载：另一个终端跑 vite，再 cargo run（debug 默认连 devUrl:5173）
pnpm --filter @pi-desktop/desktop dev

# 发布构建（加载打包的前端而非 devUrl）
cargo build --release -p pigo-desktop --features custom-protocol

# macOS 安装包（.app + .dmg；只能在 macOS 上运行）
pnpm package:mac
```

`pnpm package:mac` 会依次构建 host-core、Go sidecar 与渲染层，然后调用
`tauri build` 打包。产物在 `target/release/bundle/` 下。默认产出**未签名**
的包（可设 `APPLE_SIGNING_IDENTITY` 走签名路径），`MAC_ARCH` 必须与本机
架构一致。该脚本等价于 `scripts/build-macos.sh`，用法见其文件头注释。

数据目录与 Electron 完全一致（`~/.pi-agent-go-desktop`），已有的 sqlite、
providers、sessions 全部直接可用。环境变量 `PI_DESKTOP_HOST_BIN` /
`PI_DESKTOP_AGENT_BIN` / `PI_DESKTOP_DATA_DIR` 语义不变。

## 验证轮修复（2026-10-06）

- **`project/get` 通道补齐**：首次启动报 "lands in Tauri phase 2" 是路由表
  漏项（不是真缺口），已按 Electron 语义补上（`workspace.get` + `.git/HEAD`
  读分支）
- **托盘常驻**（D216 对齐）：启动即创建，左键恢复窗口，右键 Open/Quit，
  文案按系统语言 zh/en
- **关闭行为**（ask/tray/quit，Electron 默认 ask）：首次关闭弹原生对话框
  （关闭到托盘/退出）；选托盘则隐藏窗口、托盘常驻。暂不持久化到设置
  （每次启动回到 ask，Electron 会记住选择）
- **右键菜单**：WebView2 默认菜单已抑制（Electron 无右键菜单，对齐）
- **composer 附件**：pickFiles/pickPhotos/importFiles/pasteFiles 均已移植（token
  两段式与 Electron 一致；pasteFiles 的字节经 shim 编码为 `__bytes_b64` 落盘）
- **`agentQueueList`** 当时返回空 entries 而非报错；后续已接入真实队列
  （push/list/remove/prioritize），此处仅保留当时的排障记录
- **聊天主链路打通**（CDP 实测一轮完整 turn）：用户消息此前缺
  `UiMessage.createdAt`（host 严格 serde 拒收，表现为"发消息没反应"）已修；
  `vendorKey`/`projectPath`/`projectMemory` 为空时不再传 null；
  `agentSteer`（流式中追加消息）已补；图片附件按 MessageAttachment（ref，入库）
  与 PromptAttachment（path+base64，仅 sidecar）两种契约形状拆分
- **实测结论**：prompt → beginTurn → 落库 → sidecar → 供应商调用 → 事件流 →
  错误回传 → 持久化全部到位；当时唯一残留的失败是存储的 DeepSeek API key
  本身 401（key 尾号与供应商回显一致，证明传参正确），换有效 key 即通

## 形状契约纪律（三轮验证的教训）

host-core 是严格 serde，渲染层对成功响应不设防。Rust handler 必须逐字对照
`packages/shared/src/types/*` 与 Electron ipc 源码：

- **参数**：渲染层多处传裸字符串 id（sessionDelete/providersDelete/
  providersTest/scheduledDelete/plugin enable·disable·uninstall/subagent
  read·remove/agent getStatus/projectSet/projectOpenFolder），路由层统一包装
  成 `{id}` / `{sessionId}` / `{path}`
- **返回**：不能想当然——`projectPickFolders` 要 `{folders, canceled}`、
  `project/get` 要 `{workspace:{…branch}}`、`projectOpen` 要
  `{workspace, canceled}`、`staff/scheduler/status` 缺 `inFlight` 数组会直接
  崩溃侧边栏、`instructions/get` 要 `{global, project?}`、
  `closeBehaviorGet` 要 `{behavior, supported}`、`notificationShowNative`
  要 `{shown}`
- 改任何 handler 前先查 `apps/desktop-tauri/src/lib/api.ts` 的类型声明

## 阶段 1 已实现

- **257 个 invoke 通道**：约 150 个纯转发 host-core（staff 44 / skills /
  subagents / market / scheduled / secrets / notification / project-group…）、
  4 个转发 sidecar、60+ 个 Rust 本地/包装实现（session 增删改查、settings、
  providers 密钥注入、指令文件、窗口控制、菜单动作、fs 面板、workspace diff、
  更新状态降级、对话框选择等）
- **agent turn 主链路**：`agentPrompt` = settings/session 读取 → provider/
  model/密钥解析 → 项目指令链（AGENTS.md + 全局 + 分组上下文）→
  `session.beginTurn` → 用户消息持久化 + 即时回显 → sidecar `agent.prompt`；
  流式事件原样转发渲染层，`message_end` 落库（`session.appendMessage`），
  `agent_end`/`error` 驱动 `session.endTurn` + `saveActiveRevision`
- **事件扇出**：host `permissions.request` → 渲染层审批事件；
  `plans.changed` → 渲染层；其余转发 sidecar；`plugins.execute` 交给 Node
  插件运行时执行并回 `plugins.resolveExecution`
- **反向代理**：sidecar `host.proxy` 白名单 16 个方法直转 host-core；`BrowserPreview`
  由壳内处理（校验会话工作区、loopback 预览服务 + work-panel iframe + live reload），
  `Skill`/`Plugin*` 仍显式拒绝
- **进程监督**：HOST_OVERLOADED 退避重试（50/100/200/400ms）、崩溃重启
  （500→4000ms 退避、120s 窗口超 3 次放弃）、hostStatus 事件、schema 不匹配
  fatal
- **原生面**：无框窗口（渲染层自绘控制条）、最小化/最大化/关闭、系统对话框
  （打开项目/选择文件夹/保存）、系统通知、打开路径/文件夹揭示/外链、devtools
- **Provider 生态**：OAuth vendor accounts（登录/账号/刷新，`provider.resolveAuth`
  由壳内解析，sidecar 不持有 refresh token）、models.dev 目录（bundled + 刷新 +
  模型装饰）、自定义 headers、thinking levels
- **导入与增强**：claude-code/codex/opencode 会话导入、模型配置导入、
  `prompt/enhance`、`session/summarizeTitle`

## 阶段 2 剩余差距（按影响排序）

| 缺口 | Electron 侧 | 说明 |
|---|---|---|
| 标题栏拖拽 | `-webkit-app-region` | shim 加 mousedown→startDragging |
| 插件宿主 | `plugin-runtime.ts`（utilityProcess） | Node 宿主已移植（`plugin_host.rs` + 内嵌 `plugin_host.js`，每插件一个 node 进程）：命令、agent 工具、themes、settings、toast/notify/workspace.get；面板与停靠视图已接线（`plugin_surface.rs` loopback iframe + 注入 `window.pluginBridge`）；加载失败不自动禁用（D017），插件 JS 与 Electron 一样以 Node 进程权限运行 |
| 插件面板桥接 | `plugin-panel-host.ts` 通道表 | `plugin_bridge.rs` 在 Rust 侧统一鉴权：`app.*`、`plugin.*`、`ui.*`、`themes.*`、`fs.*`（manifest `fs` scope + 受保护路径，越界直接拒绝、无调用时授权弹窗）、`net.fetch`（`net.domains` 允许列表、禁用重定向）、`session.*`（import/list/get/listMessages/rename/delete/getLlmContext，经 host-core `plugin.session.*`）、`models.list`、`project.create`、`shell.openExternal`；`clipboard.*`/`browser.*`/`desktop.*`/`agent.complete` 仍回 UNSUPPORTED；Node 进程侧同名 `pi.fs/net/session` API 仍待接 |
| 插件面板形态 | `WebContentsView` 独立窗口 | Tauri 无原生叠加层：面板改为渲染层内模态窗口（宿主绘制标题栏），视图改为工作面板内 iframe；尺寸/位置由渲染层负责，`setBounds/setVisible` 仅保持契约兼容 |
| MCP 运行时 | `user-mcp` / `mcp-control` | `pluginTools` 现下发插件工具；用户 MCP 工具仍未进对话 |
| 内嵌浏览器 / 插件视图 | WebContentsView ×3 | `BrowserPreview` 与插件视图都走壳内 loopback + renderer iframe（插件视图由 `plugin_surface.rs` 提供并注入 `window.pluginBridge`）；原生 WebContentsView 叠加层不再需要 |
| 子代理 | `loadSubagentDefinitions` | `Task`/`TaskWait`/`TaskList`/`TaskStop` 已接线（后台委派、并发 10、自动 resume 注入报告、`turn_end.subagentUsage`）；子代理实时行已接线（消息/工具事件带 `parentToolCallId`/`agentName`，随 host 转录持久化，结算行原地刷新） |
| RACP | `agent-host-bridge.ts` | 队列 push/list/remove/prioritize 已接线；RACP 协议与剩余审批联动待做 |
| 持久化 outbox | `persistence-outbox.ts` | 崩溃窗口内可能丢最新一两条消息（DB 写已直连） |
| 股票引擎 / 站点浏览器 | `stocks-engine` / partition session | |
| 插件脚手架 / 扩展导入 | `plugin-devkit` / `plugin-import.ts` | 已移植：`plugin_scaffold.rs`（四个官方模板，空目录校验、取消即不动）与 `extension_import.rs`（复制到 `plugins/imported`、生成 manifest、排除凭据/node_modules、仅 registry 依赖的两步 npm 安装且禁止生命周期脚本）。偏差：skills 目录只取 `SKILL.md` 或直接 `.md`，不再递归扫描子目录；`agentExtensions` 的实际运行仍等 sidecar 的 trusted-extension 运行时 |
| — | — | staff 驻留驱动已移植（`staff_driver.rs`：30s 轮询 `staff.scheduler.claim` → 在运行会话里逐步跑模型回合 → `staff.run.step`/`finish`，记忆读写与 `produce` 归档；`employees/runNow` 走同一执行器）。probe 采集仍等站点采集器：没有采集器时按规范交空输入行 |
| 自动更新 | electron-updater | 换 tauri-plugin-updater |
| 剪贴板历史 / 原生菜单 | Electron API | 托盘+关闭询问已对齐；剪贴板历史（内存记录）待移植 |
| — | — | pasteFiles 已移植（shim 把 ArrayBuffer 编码为 `__bytes_b64`，Rust 解码落盘） |
| 系统/插件 skill 正文 | 本地 `Skill` 工具 | 已接线：sidecar 按 id 从宿主解析的路径读取正文 |

## 结构

```
src-tauri/src/
├── main.rs      组合根（插件、状态、setup、invoke_handler）
├── rpc.rs       NDJSON JSON-RPC 客户端 + 崩溃监督（host/sidecar 共用）
├── boot.rs      二进制解析 + 握手 + 启动时序
├── events.rs    通知扇出 + agent 事件持久化 + host.proxy 反向代理
├── router.rs    pi_desktop_invoke：257 通道路由 + agent turn + fs/窗口
├── staff_driver.rs staff 驻留驱动（claim → 步骤回合 → 记忆/归档 → 结算）
├── proxy.rs     纯转发通道表（host 131 条 / sidecar 4 条 / 白名单 16 条）
├── plugins.rs   插件元数据面（views/themes/services/settings + CSS 清洗）
├── plugin_host.rs Node 插件运行时（子进程、命令/工具注册表、运行时主题）
├── plugin_host.js 内嵌 Node 宿主脚本（NDJSON 协议 + `pi` API 子集）
├── plugin_surface.rs 插件面板/视图 loopback 服务（token 作用域 + bridge 注入）
├── plugin_bridge.rs 面板桥接通道（权限矩阵、fs scope、net 允许列表、session 域）
├── launch.rs    resolveAgentRuntimeLaunch 移植（缩减版）
├── fsops.rs     fs 面板 / diff / index（移植 fs-panel.ts）
├── native.rs    窗口/对话框/通知/opener/devtools
├── envelope.rs  Result 信封（对齐 shared/errors.ts）
└── state.rs     共享状态
```
