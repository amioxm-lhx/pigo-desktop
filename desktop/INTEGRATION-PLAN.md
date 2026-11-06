# Pi Desktop × pigo 完全对接计划

> 生成日期：2026-11-02
> 桌面源码：`gitee.com/amioxm/pi-agent-go`，commit `12edfca`（origin/master，最新）
> 目标：用 pigo 作为 Pi Desktop（Tauri）的 Go agent sidecar，完成协议、功能与打包对接。

## 0. 结论摘要

- 桌面源码已复制到 `pigo/desktop`，**旧 sidecar 未复制**，保留本仓库已有的 `desktop/sidecar`（pigo 实现）。
- `node scripts/build-agent.mjs` 已用当前 pigo sidecar 构建成功：`desktop/sidecar/build/pi-go-desktop-agent`，21.2 MB，路径与 `tauri.conf.json` 约定一致。
- 基础闭环已经可用：`sidecar.configure/health`、`agent.prompt`、宿主工具代理、流式事件、历史回灌、compaction 回放、运行中历史刷新。
- **但还未“完全对接”**。Tauri sidecar 核心有 9 个方法，当前实现 7 个，缺 `agent.steer`、`asktool.resolve`；`agent.stop` 返回形状不对。
- 侧车侧还缺 3 类事件（`planning_state`、`asktool_request`、`status`）、5 个核心工具（4 个 plan/goal + asktool）、图片附件、plan/goal 模式、asktool、运行期 compaction、错误分类/重试/activity。
- 桌面壳本身是 **Tauri phase-1**：`apps/desktop-tauri` 明确还有 16 个 `not_implemented` 通道，以及 OAuth、插件宿主、MCP、models.dev 目录、子代理目录、队列/RACP、浏览器面板、导入器、标题总结等 phase-2 缺口。**这些不是 sidecar 能单独补齐的**。
- 按代码面估算：参考实现（pi-go `sidecar/internal/runtime`）约 8.6k 行非测试代码，当前 pigo sidecar 约 3.0k 行；完整侧车功能还差约 5–7k 行等价工作，另加桌面壳 phase-2 与 pigo provider 层改造。

## 1. 源码落地与目录

### 1.1 下载来源

- 仓库：`https://gitee.com/amioxm/pi-agent-go.git`
- 分支：`origin/master`
- commit：`12edfca0b3a7fd0fcd42215f087455a54d8949f3`（2026-11-02）
- 本地 HEAD 与 `origin/master` 一致，已 `git fetch` 确认。

### 1.2 复制方式

用 `git archive HEAD:pi-go-desktop` 导出**已提交的最新源码**，再 rsync 到 `pigo/desktop`：

- 保留：`apps/`、`packages/`、`crates/`、`docs/`、`scripts/`、`examples/`、`Cargo.*`、`package.json`、`pnpm-*`、`PORTING-PI-GO.md`、README、AGENTS.md、LICENSE。
- 排除：`sidecar/`（避免覆盖本仓库的 pigo sidecar）、`apps/desktop-tauri/PI-Agent-Go-Desktop.exe`（32 MB 构建产物）。
- 结果：约 59 MB 源码。
- 许可证：桌面目录整体为 **LGPL-3.0-or-later**（`desktop/LICENSE`）；pigo 根目录是 MIT。sidecar 代码在 `desktop/sidecar` 下，分发组合需要按 LGPL 处理，报告末尾列为待决策项。

### 1.3 实际架构（重要）

桌面已经不是 Electron，当前是 **Tauri**：

```
React renderer (apps/desktop-tauri/src)
        │ tauri invoke / events
Tauri Rust shell (apps/desktop-tauri/src-tauri)
        ├── host-core (Rust, 持久化/权限/工具/会话)
        └── pi-go-desktop-agent (Go sidecar, agent 循环)
```

- `packages/agent-runtime` 只剩辅助模块（provider binding、headers、retry、prompt 模板等），**不再包含 Node runtime / Node sidecar**。
- `PORTING-PI-GO.md` 里仍有 “Electron main / agent-runtime sidecar” 的旧描述，不能作为当前集成依据；以 `apps/desktop-tauri/README.md`、`src-tauri` 源码和 `packages/shared` 类型为准。
- 后端打包布局：macOS 通过 `bundle.macOS.files` 放到
  `<App>.app/Contents/Resources/bin`；Windows 通过 `tauri.windows.conf.json`
  的 `bundle.resources` 放到 `pigo-desktop.exe` 旁的 `bin/`；`boot.rs` 依次探测
  `Resources/bin`、`bin/`、主可执行文件同目录。macOS 签名前会先用同一 identity
  对两个源二进制 `codesign --options runtime --timestamp`，再由 Tauri 打包并封签。

## 2. 当前 sidecar 已实现

| 能力 | 状态 | 位置 |
|---|---|---|
| NDJSON JSON-RPC 双向传输、host.proxy、错误码 | 完成 | `desktop/sidecar/internal/jsonrpc`、`hostproxy` |
| `sidecar.configure/health` | 完成 | `internal/server` |
| `agent.prompt` / `getStatus` / `disposeSession` / `abort` / `stop` / `steer` / `executeApprovedPlan` | 完成 | `internal/server` |
| pigo 通用 OpenAI / Anthropic / Responses provider | 完成 | `internal/runtime/provider.go` |
| 宿主工具 Read/Write/Edit/Bash/Glob/Grep 代理 | 完成 | `internal/runtime/tools.go` |
| 流式事件：agent/turn/message/tool/error/compaction | 完成 | `internal/runtime/events.go` |
| 历史回灌、当前 prompt 去重、失败/子代理行过滤 | 完成 | `internal/runtime/history.go` |
| compaction checkpoint + retained tail 回放 | 完成 | 同上 |
| 运行期 threshold 自动压缩 + `session.appendCompaction` | 完成 | `internal/runtime/compaction.go` |
| 运行中 host 历史变更检测/刷新、truncate/refresh | 完成 | 同上 |
| 精确提示词（身份、协作、搜索、编辑、shell、scratch、mode、项目指令、memory） | 完成基础版 | `internal/runtime/prompt.go` |
| 图片附件、plan/goal、asktool | 完成 | `internal/runtime` |
| 子代理（`Task`/`TaskWait`/`TaskList`/`TaskStop`、事件投影、usage） | 完成 | `internal/runtime/subagent*.go` |
| 扩展/插件命令与工具、`plugin.complete`、`mcp_*` 工具注入 | 完成 | `internal/runtime`、`plugin_host.js` |
| overflow/手动 compaction、错误分类/重试/恢复 | 完成 | `internal/runtime`（见第 11 节） |

## 3. 契约审计（Tauri 实际使用 vs 当前实现）

> 本节是 2026-11-02 的审计快照。其中标 ❌ 的多数项目已在第 11 节完成；当前真实剩余项以第 12 节为准。

### 3.1 Sidecar 方法

Tauri 核心实际调用 9 个 sidecar 方法：

| 方法 | 当前 | 差距 |
|---|---|---|
| `sidecar.configure` | ✅ | — |
| `sidecar.health` | ✅ | — |
| `agent.prompt` | ✅ | 缺 `attachments`；`projectInstructions` 的 `source` 未渲染；`pluginSkills` 未进入 prompt |
| `agent.steer` | ❌ | 未注册方法；需要把新输入注入运行中的 turn，并处理 `message`/attachments |
| `agent.abort` | ✅ | 未校验 `turnId`；未 best-effort 调用 `plans.abort` |
| `agent.stop` | ⚠️ | 返回 `{ok:true}`，桌面期望 `{requested:boolean}`；无 runtime 时应返回 `requested:false` |
| `agent.getStatus` | ⚠️ | 缺 `planningState`、`pendingPlanId`、`activity`、`pendingToolConfirmations` 语义 |
| `agent.disposeSession` | ✅ | — |
| `asktool.resolve` | ❌ | 未注册方法，也没有 asktool 工具/事件 |

`agent.compact`、`agent.executeApprovedPlan`、`agent.testRuntimeIdentity`、`agent.steeringContext` 在当前 Tauri 路由里未使用或 `not_implemented`，属于可选/后续项；`agent.extension`、`agent.registerTool`、`agent.complete` 属于插件权限面，Tauri phase 2。

### 3.2 Agent 事件

共享类型共 17 类。当前 sidecar 发出 13 类：

- ✅ `agent_start` / `agent_end`
- ✅ `turn_start` / `turn_end`
- ✅ `message_start` / `message_update` / `message_end`
- ✅ `tool_start` / `tool_update` / `tool_end`
- ✅ `error`
- ✅ `compaction_start` / `compaction_end`（但运行期 compaction 未启用）

缺失的 sidecar-origin 事件：

- ❌ `planning_state`（plan/goal 模式）
- ❌ `asktool_request`
- ❌ `status`（activity / retry / compacting / waiting-subagents）

`tool_permission_request` 在 Tauri 由 **host-core 的 `permissions.request` 通知**转成 UI 事件，不是 sidecar 责任；当前行为可接受。

另有一个形状问题：`message_update` 的 delta 帧当前携带完整 `content`/`thinking`，而桌面 `applyMessageUpdate` 只从 `previous` + delta 合成；虽然通常不会重复，但与参考实现 `streamingIdentity`（delta 帧清空增长内容）不一致，会增大帧体积，属于 P1 对齐项。

### 3.3 宿主工具

host-core `tools.list` 提供 6 个工具：

- ✅ `Read` / `Write` / `Edit` / `Bash` / `Glob` / `Grep`

sidecar 需要自带的工具：

- ✅ `EnterPlanMode`、`EnterGoalMode`、`SubmitPlan`、`SubmitGoal`、`asktool`、`Task`、`TaskWait`、`TaskList`、`TaskStop`、`BrowserPreview`（壳内打开工作区 HTML 到默认浏览器）。
- ❌ `Skill`、`Plugin*` 在 Tauri host.proxy 仍被显式拒绝（Skill 已由 sidecar 自带实现）。

### 3.4 Prompt / 启动参数

Tauri `launch.rs` 发给 sidecar 的字段：

- ✅ `sessionId`、`mode`、`thinkingLevel`、`commandShell`、`scratchDir`、`projectPath`、`projectMemory`、`provider`、`modelConfig`。
- ⚠️ `projectInstructions` 的 entry 使用 `source` 字段（不是 `path`）；当前 prompt 只读 `path`，标题会退化为 "Project instructions"。
- ❌ `attachmentsDir` 未使用。
- ❌ `pluginSkills` 目录未进入 prompt（Tauri 只传 catalog，正文读取是 phase 2）。
- ❌ `supportsVision`、`attachments`（path + image base64）未使用。
- ⚠️ Tauri 当前 `thinkingLevel="off"`、`supportsReasoning=false`、generic 128k model config；sidecar 能接受，但不是完整 models.dev 能力。

### 3.5 Provider 与 OAuth

- ✅ 通用 `openai` / `anthropic` / `responses` 驱动可用；Tauri 的 API-key provider 可以走通。
- ❌ `provider.resolveAuth` **不在 Tauri 的 `HOST_PROXY_ALLOWED` 16 项里**。当前 sidecar 在 `authKind=oauth` 时会调用它，结果会被 host.proxy 拒绝；OAuth 供应商在 Tauri phase 1 本来也是 `PROVIDER_NOT_BOUND`。
- ❌ 自定义 headers：Tauri launch 不传 headers；完整 Electron/phase-2 需要 pigo provider 层支持 headers。
- ❌ models.dev 元数据/thinking level：Tauri phase-1 使用 generic model config，shell 侧待 phase 2；不是 sidecar 单独能补的。

### 3.6 附件

Tauri `agent_prompt` 会构造两份数据：

- 持久化 `MessageAttachment`（`ref`，无字节）；
- sidecar `attachments`（`path` + 图片 `data` base64）。

当前 sidecar 完全不接受 `attachments`，所以：

- ❌ 当前轮图片不会进入模型。
- ❌ 历史图片无法恢复；host 只持久化 metadata，不持久化字节。完整恢复需要 host 提供附件字节读取 RPC，或 workspace/attachments 目录重新读取。
- 文件附件在参考实现中也只作为 metadata，不进入模型；当前可以按同样语义只处理图片。

### 3.7 Plan / Goal / AskTool

- ❌ plan/goal 模式没有 `planning_state`、Enter/Submit 工具、host `plans.enter/submit/pending/abort` 调用。
- ❌ asktool 没有工具、pending request 队列、`asktool_request` 事件、`asktool.resolve` handler。
- Tauri 已有 `plans.resolve`、`plans.changed`、`asktool.resolve` 通道；缺的是 sidecar 内部能力。

### 3.8 Compaction / 长会话

- ✅ 历史 checkpoint 回放 + retained tail 已实现。
- ✅ 运行期 threshold compaction 已启用：pigo loop 触发压缩，sidecar 在 `OnCompaction` 中把 checkpoint 写回 `session.appendCompaction`，`compaction_end` 带 `mark`/`firstKeptMessageId`，下一次 runtime 重建可直接回放。
- ⚠️ checkpoint 写回要求宿主 transcript 与 live context 对齐；Tauri 目前不持久化 tool 行（见 3.4），因此含工具调用的历史无法锚定边界时会跳过 `session.appendCompaction`，压缩只在当前 runtime 生效。shell 补上工具行持久化后可完整写回。
- ❌ overflow 触发（provider 报上下文超限后压缩重试）和手动 `agent.compact` 仍未实现；Tauri 的 `agent.compact` 仍是 `not_implemented`。

### 3.9 错误、重试、状态

- ❌ 当前 provider 错误统一为 `PROVIDER_ERROR`；没有参考实现的错误分类、retriable、providerStatus、重试/恢复。
- ❌ 没有 `status` 事件和 activity（starting / waiting-model / compacting / retrying / waiting-subagents）。
- ❌ `agent.getStatus` 只返回 running/turn/model/pending 计数，缺 planning/activity。

### 3.10 持久化 / turn 生命周期

- Tauri shell 在 `agent.prompt` 前自行 `session.beginTurn`、写入 user 消息、发 user `message_start/end`；sidecar 不需要重复。
- Tauri `events.rs` 在 `message_end` 持久化 user/assistant；`agent_end`/`error` 触发 `session.endTurn` 和 revision 保存。当前 sidecar 的事件顺序基本匹配。
- ⚠️ 工具行在 Tauri 当前只存在于 renderer live state；sidecar 的 `tool_start/tool_end` 没有对应的持久化 `UiMessage`。这是 shell/桌面侧的问题，但会影响刷新后的工具历史。

## 4. “还差多少”量化

### 4.1 数量

| 维度 | 参考实现 | 当前 | 差距 |
|---|---:|---:|---:|
| Sidecar 核心方法 | 9（Tauri 实际使用） | 7（其中 1 个返回形状错误） | 2 个方法 + 1 个形状 |
| Agent 事件类型 | 17 | 13（compaction 条件性） | 3 个 sidecar 事件 + 1 个 host-origin（可接受） |
| 宿主工具 | 6 | 6 | 0 |
| Sidecar 自带核心工具 | 5 | 0 | 5 |
| Prompt 输入块 | 全部 | 大部分 | attachments / pluginSkills / source / supportsVision |
| Provider 协议 | OpenAI/Anthropic/Responses | 3 | 0（phase 1） |
| Provider 完整能力 | headers/OAuth/models.dev/thinking | 基础 | phase 2 |
| 运行期 compaction | 自动+手动 | 0 | 1 套 |
| 错误重试/恢复/activity | 完整 | 0 | 1 套 |
| 历史/刷新/checkpoint 回放 | 完整 | 完整 | 0 |
| 子代理/扩展/插件/MCP | 完整（phase 2 启用） | 0 | phase 2 |

### 4.2 代码面

- 参考旧 sidecar：**11,346 行非测试代码**；其中 `internal/runtime` 8,584 行。
- 当前 pigo sidecar：**3,023 行非测试代码**（含 transport/protocol/server/runtime）。
- 直接缺口约 **5–7k 行等价实现**，主要集中在 `subagent.go`（2,662 行）、`compaction.go`（792）、`events.go`（617）、`recovery/retry/errors`（约 1.2k）、`mode/plan/asktool`（约 900）。
- 另有桌面壳 phase 2（Tauri 16 个 `not_implemented` 通道）和 pigo provider headers 改造，不计入 sidecar 行数。

### 4.3 就绪度（按功能面，不给日历工期）

- **基础聊天 + 宿主工具 + 历史/刷新**：高，已可跑单轮/多轮、工具调用、abort/stop（除 stop 返回形状）。
- **完整 Tauri phase-1 聊天体验**：中等；缺 steer、attachments、status/activity、错误重试、plan/goal、asktool、运行期 compaction。
- **完整桌面（含 phase 2）**：较低；仍需 OAuth、插件宿主、MCP、models.dev、子代理目录、队列/RACP、浏览器/插件视图、导入器、更新、staff 等，且多数依赖 Tauri 壳先落地。

## 5. 功能清单与优先级

> 本清单是 2026-11-02 的初始优先级，仅作历史参考；完成情况见第 11 节，剩余项见第 12 节。

### P0 — 对接阻塞（先做）

1. `agent.stop` 返回 `{requested:boolean}`；无 runtime 时 `requested:false`。
2. `agent.abort` 支持 `turnId` 校验；best-effort `plans.abort`。
3. `agent.steer`：接收 `expectedTurnId`、content、attachments、`message`，注入运行中 turn；无 active turn 返回 `TURN_NOT_FOUND`。
4. `PromptParams` 增加 `attachments`：图片 base64 → `agentcore.ImageContent`；文件只保留 metadata 语义。
5. `projectInstructions` 支持 `source` 字段渲染。
6. 对齐 host.proxy allowlist：移除 `provider.resolveAuth`/`project.instructions.resolve`（Tauri 不允许），保留 tools/session.get，后续加 plans/appendCompaction。
7. `message_update` delta 帧按 `streamingIdentity` 清空增长 content/thinking，减少 payload 并贴近参考语义。
8. 状态字段：`pendingToolConfirmations`、`activity`（至少 starting/waiting-model）、`planningState`/`pendingPlanId` 占位。

### P1 — 完整 Tauri phase-1 交互

9. `asktool`：工具 schema、pending request 队列、`asktool_request` 事件、`asktool.resolve` handler、abort 时清空。
10. plan/goal 模式：EnterPlanMode/EnterGoalMode/SubmitPlan/SubmitGoal、`planning_state`、host `plans.enter/submit/pending/abort`、模式切换。
11. `status` 事件：starting / waiting-model / compacting / retrying / waiting-subagents。
12. 错误分类：provider status/code、retriable、recovery；对齐 Tauri UI 的 `error`/`activity`。
13. Prompt 增强：pluginSkills catalog、mode 完整文案、attachmentsDir；按需保持 pigo 自有文案还是逐字对齐（许可证决策）。
14. 运行期 compaction：阈值/溢出触发、`compaction_start/end`、`session.appendCompaction`、上下文重建；手动 `agent.compact` 可随 shell 一起解禁。

### P2 — 桌面 phase-2（依赖 Tauri 壳）

15. 子代理：`Task` + `TaskWait`/`TaskList`/`TaskStop` + definitions + provider/model 解析 + usage 汇总已完成；逐 token 子代理事件行待做。
16. 扩展/插件/MCP：插件元数据宿主（views/themes/services/settings）、Node 插件运行时（命令/agent 工具/运行时主题/settings/toast/notify，每插件一个 node 进程）、`pluginTools` 下发与 `plugins.execute` 回填、插件面板与停靠视图（loopback iframe + 注入 `window.pluginBridge`）、面板桥接（`plugin_bridge.rs`：fs scope、net 允许列表、session 域、themes/app/plugin/ui/models/project/shell）、`BrowserPreview`（内嵌标签页 + live reload）已完成；clipboard/browser/desktop/agent.complete、Node 进程侧同名的 fs/net/session API、常驻服务、MCP 待做。插件运行时依赖系统 Node 18+（`PI_DESKTOP_NODE_BIN` 可覆盖），缺失时其余功能正常。
17. Provider：OAuth vendor accounts、`provider.resolveAuth`、自定义 headers、models.dev 元数据、thinking levels。
18. 队列/RACP/审批、outbox、导入器、标题总结/prompt 增强、自动更新、staff/浏览器面板。
19. 桌面打包：Tauri 资源、macOS DMG、跨平台 sidecar 构建、签名/更新。

## 6. 排期（按依赖阶段，不估算日历工期）

> 排期按依赖顺序和验收门槛组织，不给具体天数；可并行的工作单独标注。

| 阶段 | 内容 | 依赖 | 验收 |
|---|---|---|---|
| **M0 源码与构建** | 已复制最新源码、sidecar 模块、build-agent 脚本、二进制路径 | 无 | `node scripts/build-agent.mjs` 产出 `sidecar/build/pi-go-desktop-agent`；`sidecar.configure/health` 通 |
| **M1 契约对齐** | stop 返回、steer、attachments、project source、host.proxy allowlist、delta 帧、status 占位 | M0 | Tauri prompt/steer/stop/abort 行为与共享类型一致；图片 prompt 可用 |
| **M2 交互能力** | asktool、plan/goal、planning_state、prompt skills/增强 | M1 | 计划审批、asktool 卡片、模式切换端到端通过 |
| **M3 可靠性/压缩** | 错误分类/重试/activity、自动 compaction、appendCompaction、turnId/dispose | M1 | 长会话可压缩；provider 429/超时可重试；UI activity 正确 |
| **M4 桌面 phase-2** | 子代理、插件/MCP、浏览器、队列/RACP、outbox/导入器等 | M2/M3 + Tauri 壳 phase-2 | 与 Electron 原功能对齐 |
| **M5 Provider 生态** | OAuth、headers、models.dev、thinking、自定义 provider | M3 + Tauri 壳 + pigo provider 层改造 | vendor 账号与自定义 endpoint 可用 |
| **M6 打包与验收** | macOS/Windows 构建、E2E、许可证、发布 | M1–M5 | 安装包可启动；E2E 矩阵通过 |

可并行轨道：

- **Sidecar 轨道**：M1 → M2 → M3 → M4。
- **Tauri 壳轨道**：16 个 `not_implemented` 通道、OAuth/插件/MCP/队列等 phase-2。
- **pigo provider 轨道**：headers、OAuth/模型目录适配。
- **E2E 轨道**：先在 M1 建立 sidecar 协议 smoke，再扩展到 Tauri CDP。

## 7. 验收矩阵

必须覆盖：

1. 启动：`PI_DESKTOP_AGENT_BIN=desktop/sidecar/build/pi-go-desktop-agent`，Tauri 应用启动，`sidecar.health` 正常。
2. 单轮：prompt → 流式 text → message_end → agent_end → host 持久化。
3. 工具：Read/Write/Edit/Bash/Glob/Grep，权限请求由 host-core 发起，结果回填。
4. 多轮/重启：历史回灌、refresh、fork/rewind。
5. Stop/Abort：`{requested:true}`、turnId 校验、turn 结束状态。
6. Steer：运行中追加消息，事件顺序正确。
7. 附件：图片进入模型；历史 metadata 可读（字节限制待 host 支持）。
8. Plan/Goal：SubmitPlan/SubmitGoal、planning_state、审批、执行。
9. AskTool：问题卡片、resolve、跳过/停止。
10. 长会话：threshold/overflow compaction、checkpoint 回放。
11. 错误：401/429/超时/取消，错误码、retriable、activity。
12. 打包：macOS `.app`/`.dmg` 和 Windows 安装包，sidecar 资源存在。

## 8. 风险与待决策

1. **Tauri 不是 Electron**：`PORTING-PI-GO.md`、旧文档中的 Electron/Node sidecar 描述已过时；所有验收以 `apps/desktop-tauri/README.md`、`src-tauri` 和 `packages/shared` 为准。
2. **许可证**：`desktop/` 是 LGPL-3.0-or-later，pigo 根目录是 MIT；sidecar 位于 LGPL 目录下。发布组合需要明确 LGPL 边界，不能用 MIT 覆盖桌面壳。
3. **Provider 层改造**：自定义 headers 需要 pigo `provider.Model/StreamConfig` 支持 headers 并透传到 OpenAI/Anthropic/Responses driver；sidecar 单方面无法解决。
4. **图片历史**：host 不持久化字节；不增加 host RPC/持久化，只能恢复 metadata，无法重放历史图片。
5. **Tauri phase-2 依赖**：插件、MCP、OAuth、models.dev、子代理目录、队列/RACP、浏览器面板等多数不是 sidecar 能单独完成的，必须与桌面壳同批推进。
6. **严格 serde**：host-core 对 `UiMessage`/`createdAt` 等字段严格；sidecar 事件形状必须以 `packages/shared/src/types/messages.ts` 为准，任何新增字段都应先确认 host 的 serde 行为。
7. **版本同步**：桌面源码从 Gitee `12edfca` 复制，后续需固定 submodule/同步脚本；sidecar 使用 `replace ../..` 指向 pigo 根目录。

## 9. 立即行动清单

1. 修 `agent.stop` 返回形状与 `agent.abort` turnId 校验。
2. 实现 `agent.steer`（含 attachments/message）。
3. 补 `PromptParams.attachments` 与图片 content block。
4. 修 `projectInstructions.source` 渲染。
5. 移除/隔离 `provider.resolveAuth` 调用，明确 OAuth phase 2。
6. 对齐 `message_update` delta 帧（streamingIdentity）。
7. 补 `asktool` 与 `agent.steer` 的协议级测试。
8. 在 Tauri 壳里以 `PI_DESKTOP_AGENT_BIN` 跑一轮真实 prompt，记录 E2E 基线。
9. 建立 `desktop/E2E.md` 测试矩阵（或并入本文件第 7 节）。
10. 与桌面壳同步 phase-2 优先级：OAuth、插件/MCP、子代理、models.dev。

## 10. 附录：关键路径

- 桌面壳：`apps/desktop-tauri/src-tauri/src/{router,events,launch,boot,proxy}.rs`
- 共享契约：`packages/shared/src/types/{agent,messages,sessions,plans,permissions}.ts`
- host-core RPC：`crates/host-core/src/rpc/mod.rs`
- 参考 sidecar：`/Users/apple/workspace/golang/pi-agent-go/pi-go-desktop/sidecar/internal/runtime/`
- 当前 sidecar：`/Users/apple/workspace/golang/pigo/desktop/sidecar/`

## 11. 实施进度（2026-11-02）

### 已完成

- [x] M0：最新桌面源码复制、旧 sidecar 隔离、pigo sidecar 构建路径验证。
- [x] `agent.stop` 返回 `{requested:boolean}`；无 runtime 时 `requested:false`。
- [x] `agent.abort` 支持 `turnId` 校验，并 best-effort 调用 `plans.abort`。
- [x] 注册并实现 `agent.steer`：校验 `expectedTurnId`、注入 steering 队列、发 `message_end`（`steering:true`）。
- [x] `PromptParams.attachments`：图片 base64 → `agentcore.ImageContent`；文件附件保持 metadata 语义。
- [x] `projectInstructions` 支持 `source` 字段；仍兼容旧 `path`。
- [x] `message_update` delta 帧按 `streamingIdentity` 清空增长 content/thinking。
- [x] 移除生产代码中的 `provider.resolveAuth` 调用；OAuth 明确返回 `PROVIDER_NOT_BOUND`，等待 phase 2。
- [x] host.proxy allowlist 对齐 Tauri 16 项。
- [x] `agent.getStatus` 增加 `planningState`/`pendingPlanId` 占位字段。
- [x] `asktool` 工具、`asktool_request` 事件、`asktool.resolve`；等待/取消/非法参数/单选校验，见 `internal/runtime/asktool.go`。
- [x] plan/goal 四个工具：EnterPlanMode/EnterGoalMode/SubmitPlan/SubmitGoal；host `plans.enter/submit`、`planning_state`（`planning`/`awaiting_approval`）、mode 边界校验，见 `internal/runtime/mode.go`。
- [x] `agent.getStatus` 的 `planningState`/`pendingPlanId` 已接真实值；Write/Edit 在 plan/goal 模式下由 beforeToolCall 阻断。
- [x] 审批执行入口：`agent.executeApprovedPlan` + `GetFollowUpMessages`/internal instruction；Tauri 增加 `agent/executeApprovedPlan` 路由、`plans.claimExecution`/`finishExecution`、renderer API 调用。
- [x] `agent.getStatus` 的 `activity` 已接真实值：starting / waiting-model / compacting；变更时发 `status` 事件。
- [x] 补 stop/steer/asktool/plan-goal/execute-plan/activity/attachments/source/delta 测试；`go vet`、`go test -race`、`build-agent` 通过。
- [x] 覆盖率：runtime 75.9%，server 61.5%，jsonrpc 73.6%。
- [x] 运行期 threshold compaction：pigo `RunConfig.OnCompaction` 同步持久化 `session.appendCompaction`；`compaction_end` 带 mark/error；重启按 checkpoint 回放；`go test -race` 覆盖成功、写回失败、回放与边界不匹配。
- [x] Tauri 构建修复：补齐 `apps/desktop-tauri/build/{icon.png,icns,ico}`（并加入 `.gitignore` 例外）；修复 `plans` 执行生命周期里 `MutexGuard` 跨 `await` 导致的 `!Send`。`cargo check --workspace` 与 `cargo build -p pi-desktop-tauri` 通过。
- [x] TS 类型检查：`pnpm install --frozen-lockfile` + `pnpm typecheck` 全 workspace 通过；替换 `Array.findLast`（ES2022 不支持）为尾部倒序扫描。
- [x] 本机环境：Trae 沙箱会向 build script 注入 `.sboxlib` 并触发 SIGILL，`cargo`/`cargo test` 需在普通终端或 `lldb --batch ... -- cargo ...` 下运行；代码本身已无构建错误。
- [x] Rust 测试：`host-core` 459 passed / 0 failed；`pi-desktop-tauri` 测试目标 0 tests。
- [x] `envelope.rs` 清理：删除未使用的 `INVALID_ARGUMENT`/`NOT_FOUND` 常量与 `err_details`/`envelope_from_any`；`cargo check --workspace` 零警告，rustfmt 通过。

- [x] P0 错误分类/重试/恢复：provider 状态码与网络/超时/上下文超限分类，request-setup 退避重试（429 专属预算），`retrying` activity（attempt/delay/error），失败行携带真实 errorCode/retriable。
- [x] P0 overflow + 手动 compaction：`CONTEXT_TOO_LARGE` 触发一次 inline 压缩并重试；sidecar `agent.compact` + Tauri 路由；`compaction_start/end` 带 manual/overflow、mark/error/willRetry。
- [x] P0 Tauri 工具行持久化：`tool_start` 缓存调用，`tool_end` 写 `session.appendMessage`（role=tool、args/result/status/duration），abort/agent_end 清理；刷新后工具历史保留，compaction 边界可对齐。
- [x] P0 pluginSkills + `Skill` 工具：prompt 渲染技能目录，`Skill` 按 id 从宿主解析的路径读取正文（256 KiB 上限）；launch 附带 skill path。
- [x] P1 队列：改用 Host-owned `session.queuePush/List/Remove/Prioritize`（renderer 补齐 principal/inputHash/permissionMode），`agent_end/error` 后自动启动队首，`queueChanged` 事件刷新。
- [x] P1 标题总结 + prompt 增强：sidecar `session.summarizeTitle` / `prompt.enhance` 一次性 provider 调用，Tauri 路由接入。
- [x] P1 OAuth vendor accounts：`oauth.rs` 实现 7 个 vendor（浏览器回调 + 设备码 + 粘贴码），每次登录创建独立 provider 行并把凭据写入 `secret:provider:<id>:oauth`；vendor 列表/账号删除/应答/取消接入 `router.rs`；launch 解析短期 access token；sidecar 通过 `host.proxy provider.resolveAuth` 按请求刷新，refresh token 不进入 sidecar。
- [x] P1 models.dev：`catalog.rs` 读取 `apps/desktop-tauri/resources/models.dev/api.json`（发布包内 `Resources/models.dev/api.json`），设置页刷新抓取 `https://models.dev/api.json` 且只更新进程内快照；模型列表按 vendorKey/URL 装饰，launch 使用真实 context/maxTokens/reasoning/thinking 元数据；`provider/modelCatalogStatus` 与 `refreshModelCatalog` 已接线。
- [x] P1 导入器：`importer.rs`（Claude Code / Codex / OpenCode / Pi 会话扫描与 `session.import` 转换，扫描缓存只在主进程）与 `model_config_import.rs`（Claude Code / Codex / OpenCode / Pi / CC Switch，端点+凭据幂等跳过、首个导入补默认 provider）；写入始终经 host `providers.create` / `session.import`。
- [x] Provider 自定义 headers 透传：pigo `StreamConfig/LoopConfig` 增加 `Headers`，OpenAI Chat / Anthropic / Responses driver 均附加；sidecar 从 provider 行透传（OAuth 账号的 Copilot integration headers、Anthropic OAuth beta header 依赖此路径）。
- [x] P2 子代理（catalog + Task）：`subagents.rs` 移植 5 个内置定义与共享解析/合并规则（用户文档来自 host `agents.active` 并直接读取原文件，用户覆盖内置、上限 16、诊断不阻塞）；`pi-desktop/subagent/catalog` 返回 `{subagents,diagnostics,projectPath}`；launch 解析每个 `provider/model` pin（含 OAuth access token 与 models.dev 元数据，解析失败不进 map）并随 prompt 传入 `subagents/subagentProviders/subagentModelKeys`，OAuth 绑定表按会话允许主行与 pin 行。
- [x] P2 子代理（sidecar `Task` 生命周期）：`Task` 改为后台委派（ADR 0089），返回 `delegationId`/`status: running` 后父级继续工作；`TaskWait`（all/any、minCompleted、600s 默认/900s 上限、超时非失败）、`TaskList`、`TaskStop`（停止后报 `stopped`）；会话级 registry（100 条上限）、并发上限 10、子代理活动回写 `waiting-subagents`（agent/lastTool/lastPhase）、`Task` 卡片在结算后经 `message_end` 用同一 toolCallId 刷新；父级 idle 时自动等待本轮委派并把未读报告作为 follow-up 注入（steering 优先，Stop/abort 中止委派）。
- [x] P2 子代理实时行 + 结算持久化：`internal/runtime/subagent_events.go` 把委派自身的 message/tool 事件按 `parentToolCallId`/`agentName` 投影到父事件流（含 delta 流式文本与工具行）；Tauri 壳把子代理行连同归属一并写入 host 转录。`Task` 结算 `message_end` 复用 `tool_end` 的持久化行 id，host-core 对 tool 行做原地刷新，重载后卡片保持终态。
- [x] P2 BrowserPreview：sidecar 自带本地工具（每模式可用、`file_path` 别名、workspace-relative 词法校验），经 `host.proxy tools.execute` 交给 Tauri 壳；壳按来源 session 的 workspace 解析、拒绝绝对路径/`..`/盘符/符号链接逃逸、仅允许现有 `.html`/`.htm`。`preview.rs` 启动 loopback HTTP 服务，renderer 的 work-panel 浏览器标签页用 iframe 加载该 URL，HTML 响应注入 live-reload 轮询脚本（页面或同目录资源 mtime 变化即刷新）；服务不可用时回退到系统默认浏览器。
- [x] P2 子代理 usage：子代理每个 turn 的 input/output token 汇总，经 `turn_end.subagentUsage` 上报渲染层。
- [x] P2 插件元数据宿主（用户选定方案）：`plugins.rs` 从 host-core 注册表与各插件 `manifest.json` 派生 `plugin/views`（本地化标题、entry 存在性、scope/enabled 过滤）、`plugin/themes`（CSS 读取 + 尺寸上限 + 严格清洗：禁 @import/标记/脚本表达式/非 data: url）、`plugin/services`（初版声明项一律 `stopped`；N3 已替换为真实监督）、`plugin/settings/get|set`（声明键过滤，值持久化到 `data_dir/plugin-settings.json`，当前无插件运行时消费）；`plugin/reload` 为无操作元数据 reload。
- [x] P2 插件运行时公共 API：commands/UI、tool 注册与执行、`agentExtensions`、`fs.*`、`net.fetch`、`session.*`、`models.list`、`agent.complete`、`clipboard.*`、`shell.openExternal`、`project.create` 均经 `bridge.invoke` 复用面板桥的权限、fs scope、net allowlist 与 session 归属校验；`pi.agent.complete` 走 sidecar `plugin.complete` 一次性调用。
- [x] P2 插件运行时剩余（2026-11-05，N3）：resident service 启停监督与崩溃重启、插件自带 MCP server 加载、`plugin-asset://` 主题资源、pi 扩展 loop hook 均已落地；browser guest/CDP 与 `desktop.control` 因 Tauri 壳没有对应 guest/操作目录，继续返回明确 `UNSUPPORTED`（归入 N4）。

### P2 已落地

- [x] `extensions.command.run` / `extensions.ui/respond`：插件 Node 进程执行命令并桥接 confirm/select/input。
- [x] P2 插件 JS 运行时：commands/tools/agentExtensions、panel 与 work-panel view、插件 Node 公共 API 已完成；resident service 监督、插件 MCP、`plugin-asset://` 已在 N3 补齐。
- [x] OAuth 供应商模型可用性：`oauth.rs` 在登录与 token 刷新时读取账户级目录（Copilot `GET /models` 按 picker/policy/tool_calls 过滤，individual 账号回退 `policy.state=enabled`；Radius `GET /v1/config` 网关目录），ids 持久化在 `:oauth` 凭据的 `availableModelIds` 中；`providers.listModels` 对 OAuth 行优先返回该账户目录（live 读取后回写，失败回退 models.dev），`provider.create` 用它填充模型绑定与默认模型。仍未做 Copilot 的 policy 模型自动启用（`POST /models/{id}/policy`）与多 wire-API 的每模型 `apiStyle`。
- [x] 会话导入的 OpenCode 存储布局：JSON 目录与 SQLite 数据库两种形态均已覆盖（后者见 `importer_opencode_sqlite.rs`，表结构探测 + 副本读取）。

### 剩余缺口与建议完成顺序（2026-11-04 复核）

复核口径：渲染层共 276 个 invoke 通道；8 个显式 phase-2（浏览器面板与自动更新）。`browser/navigate|action|getState` 已无 UI 调用，因此实际待办归并为 11 项任务；本轮已完成 10 项，余 1 项。

| 顺序 | 任务 | 缺口 | 影响 |
|---|---|---|---|
| 1 ✅ | 站点管理与采集 | `staff/sites/list` / `signIn` / `signOut` / `update` / `collect`；probe 步骤接入采集器 | 站点面板可用；AI 团队巡检读到知乎列表页与详情页（其余五渠道仍显示未接入） |
| 2 ✅ | 发布后回复监控 | `staff/watch/status` / `run` 常驻轮询 + 页面回读 | 已投放稿子的回复数、话题趋势与陌生回复自动进商机池 |
| 3 ✅ | 选股神器后端 | `stocks/status`、`apiKey/set`、`overview`、`plan`、`watch/list`、`watch/add`、`watch/remove` 共 7 个通道 | 页面可用：同花顺真实行情、情绪相位、四策略评分、买卖计划与自选股告警 |
| 4 ✅ | 用户 MCP 运行时 | `mcp/test`、`mcp/import`；launch 注入 `mcp_*` 工具 | MCP 设置“测试连接”可用，MCP 工具进入模型工具表 |
| 5 ✅ | MCP / Skill 市场 | `mcp/market/search`、`skill/market/search`、`skill/market/fetch` | `McpMarketPanel` / `SkillMarketPanel` 可搜索、预览并安装 |
| 6 ✅ | pi 扩展命令运行时 | `extensions/commands/run` / `ui/respond`；`trustedExtensions` 随 launch 下发 | 导入的 pi 扩展可注册斜杠命令并弹出 confirm/select/input；工具注册/执行已接通；agent 扩展 `on(...)` 的 loop 事件已在 N3 接通 |
| 7 ✅ | 克隆 Git 项目 | `project/clone` | 首页项目切换器里的克隆入口可用 |
| 8 ✅ | 网络代理测试 | `network/testProxy` | 设置里“测试代理”按钮可用 |
| 9 ✅ | Pull Requests 面板 | `pulls/list` | PR 页经 `gh pr list` 列出真实 PR |
| 10 ✅ | OAuth 供应商模型目录 | 登录/刷新读取账户级目录（Copilot `/models`、Radius `/v1/config`）并持久化到 OAuth 凭据；`listModels` 对 OAuth 行优先使用该目录 | 订阅账号只看到自己可用的模型 |
| 11 ✅ | OpenCode SQLite 导入 | 同时读取 JSON 存储与同目录 SQLite 数据库 | SQLite 版 OpenCode 的历史可导入 |

排序理由：1→2 属同一条站点采集链路，先解锁常驻驱动与“我的应用”站点能力；3 是独立整页缺口，可单独交付；4→5 同属 MCP 能力，先运行时、后市场；6→9 是较小且可并行的入口修复；10→11 属兼容性边角，最后处理。

进展（2026-11-05）：第 1、2、3、4、5、6、7、8、9、11 项已完成，均有单元测试或手工链路验证（`stocks.rs`、`staff_sites.rs`、`staff_watch.rs`、`plugin_host.rs`/`plugin_host.js`、`mcp.rs`、`market.rs`、`git_clone.rs`、`proxy_test.rs`、`pulls.rs`、`importer_opencode_sqlite.rs`；sidecar 的 `buildPluginTools` 同步接受 `mcp_` 命名空间）。站点采集与回音巡检的登录窗、真实页面读取仍需业主扫码后手工验证；macOS 上 WKWebView 共用 Cookie 存储，无法做每站点 partition，已在规格 §15 记录。pi 扩展在 Tauri 版由每个插件的 Node 进程加载，命令、UI 桥与工具执行可用；agent 扩展 `on(...)` 的 loop 事件因 Go sidecar 尚未暴露 hook，只记录诊断。插件 Node API 的 fs/net/session/models/agent.complete/clipboard/shell 已通过 `bridge.invoke` 复用面板桥实现；agent 扩展 loop hook 已在 N3 接通；browser/CDP 与 desktop.control 仍无壳内等价能力。选股引擎按手册重建：真实 hithink-finance 数据、150ms 外呼间隔与分级缓存、四策略评分、买卖计划、自选股；仍需一个真实 API Key 做线上冒烟。11 项清单已全部完成；OAuth 账户目录仍缺 Copilot policy 自动启用。

剩余未完成项已按当前代码重新核对，归并到第 12 节；OAuth 供应商模型目录已完成。

进展（2026-11-05，N2 首批）：`staff_sites.rs` 已接入 `juejin` 与 `v2ex`：登录 URL、搜索 URL、默认关键词、认证 Cookie（`sessionid`/`sessionid_ss`、`A2`/`PB3_SESSION`）、详情 URL 规则，以及独立 DOM 采集脚本（`staff_site_scripts.rs`，list/detail 模板 + channel config；知乎保留原脚本）。回音巡检脚本同步扩到 `.topic_content`/`.article-content`/`.reply_content`，点赞与回复计数兼容 V2EX/掘金。单元测试 23 项通过（19 staff_sites + 4 脚本），`cargo test -p pigo-desktop` 115 passed。

进展（2026-11-05，N2 第二批）：`bilibili` 采集器已接入：登录 `passport.bilibili.com/login`、搜索 `search.bilibili.com/all?keyword=...`、Cookie `SESSDATA`、详情 `/video/BV...` 与 `/video/av...`；列表脚本读取 `bili-video-card`，详情脚本读取标题与简介（评论区在 open shadow root 内，首版不抓评论）。`cargo test -p pigo-desktop staff_site` 23 passed。

进展（2026-11-05，N2c 视频号初测）：视频号助手页面在普通桌面 Chromium 中可正常加载（`/platform` → `/login.html`，标题“视频号助手”），登录入口是微信 OAuth 扫码 iframe，未发现微信客户端专属封锁。自动化实例因窄视口/二维码 iframe 渲染失败未完成扫码，登录后的后台 DOM 与登录态存储（cookie 还是 localStorage）仍未验证；下一步在 Tauri 登录窗内复验。

进展（2026-11-05，N2 第三批）：`wechatMp`（公众号）采集器已接入：登录 `https://mp.weixin.qq.com/`、Cookie `slave_sid`/`slave_user`、无关键词搜索，必须把 MP 后台“已发表内容”列表页设置为 seed URL；列表脚本读取 `.appmsg_item`/`.appmsg_title`，详情脚本读取 `#activity-name` 与 `#js_content`；文章 URL 同时支持 `/s/<slug>` 与 `/s?__biz=...&mid=...&idx=...&sn=...` 两种 canonical 形式，评论不抓。回音巡检脚本同步识别 `#js_content`、阅读量与“在看”。真实登录后需确认后台实际 cookie 名仍是 `slave_sid`；如已改名，只补 `auth_cookies` 即可。`cargo test -p pigo-desktop` 115 passed。

进展（2026-11-05，N2 第四批 视频号）：复验在 WKWebView 内扫码完成。登录后 Cookie 为 `sessionid` + `wxuin`（域 `channels.weixin.qq.com`，持久、secure），因此登录判定直接用 cookie，不需要页面信号。后台是 wujie 微前端，内容在 `wujie-app.shadowRoot`；内容管理路由 `/platform/post/list`，列表容器 `.post-list` → `.post-list-body`。该测试账号没有已发表视频（`post-list-body` 为空、“你还没有发表过视频”），所以真实行 DOM 未能确认。已按列表-only 实现：`wechatChannels` 固定打开 `/platform/post/list`，脚本遍历 open shadow root 读取 `.post-list-body`，行标题用泛化选择器；不做详情页与评论。`cargo test -p pigo-desktop` 115 passed。

进展（2026-11-05，N2 收尾 Copilot）：`fetch_copilot_model_ids` 现在把 picker 可见但 `policy=disabled` 的模型 `POST /models/{id}/policy {"state":"enabled"}` 后重读目录；单个失败只记日志并保留原目录。新增 `ModelBinding.apiStyle`（shared 类型 / host-core 持久化 / launch / 子代理 / plugin.complete），模型级 wire API 优先于 provider 级；设置页每个模型新增 API 格式选择，只暴露 Go sidecar 支持的 `chat_completions`/`responses`/`anthropic_messages`。`host-core` 460 passed、`pigo-desktop` 117 passed、`pnpm typecheck` 通过。

进展（2026-11-05，并行轨道 E）：asktool E2E 已补齐：`asktool_request` → `asktool.resolve` → 工具结果进入下一轮模型请求；答案数量不匹配时请求保持 pending；未知 requestId 返回 `ASKTOOL_NOT_FOUND`。`pnpm test:e2e` 21 passed / 1 skipped。

进展（2026-11-05，N3 第一步 `plugin-asset://`）：`main.rs` 注册异步 `plugin-asset` scheme；`plugins.rs` 只服务 `contributes.themes[].assets` 声明过的图片/字体（扩展名白名单、4 MB 上限、canonical 路径必须落在插件根内），并要求插件启用且持有 `ui.theme`。主题 CSS 的 `url()` 被重写到 scheme 地址，未声明引用继续被清洗；Windows 走 `http://plugin-asset.localhost`。单元测试覆盖声明重写、路径穿越/绝对路径拒绝、两种 scheme 形态。

进展（2026-11-05，N3 第二步 resident service）：`plugin_host.js` 新增 `services.start/stop`：声明来自 manifest，handler 来自 `pi.services.register`；`start` 幂等、5s 预算、单个失败只标记该服务；`unload` 先停服务再 `onUnload`。`plugin_host.rs` 保存 `(pluginId, serviceId)` 状态并在 `load` 成功后启动、`unload` 前清理；`plugin/services` 合并实时 `state/restarts/message`。进程崩溃时按 ADR 0040 重启整个插件（1/2/4/8/16s、最多 5 次、60s 健康清零）；`autoRestart:false` 或未声明服务不重启；`clear_failures`/手工 reload 取消待执行重启。测试覆盖启动→停止顺序、未注册服务报 `failed`、退避序列与上限。

进展（2026-11-05，N3 第三步插件 MCP）：`mcp.rs` 在每次 prompt 发现用户 MCP 工具后追加插件 `contributes.mcpServers`：权限按 transport 校验、HTTP 主机必须在 `manifest.net.domains` 内、stdio 相对命令必须落在插件根内；`env`/`headers` 的 `{setting}` 只读插件设置，缺失即跳过该 server。stdio 子进程清空宿主环境，只保留 PATH/临时目录/locale 与 `PI_PLUGIN_ID`，并以插件根为 cwd。工具名 `mcp_plugin_<pluginId>_<serverId>_<tool>` 经既有 `mcp_*` 路径进入 host-core 权限与风险门；插件卸载/崩溃时删除其工具并关闭连接。测试覆盖权限/allowlist/相对命令/引用解析/按插件清理。

进展（2026-11-05，N4 第一批 浏览器 guest）：`tauri` 启用 `unstable` 多 webview（升级到 2.12.5），新增 `browser.rs`：懒创建 `work-browser` 子 webview、隐藏启动、只允许 http(s)/about:blank，工作区 `.html` 经 loopback preview 服务加载；`setBounds` 把 hole 与 chrome 视图矩形、窗口客户区依次求交，无可见区域即隐藏；历史记录支持前进/后退/刷新/停止并推送 `browser/event/state`。渲染层新增 `BrowserHostTab`（地址栏、导航按钮、外部打开、错误行），`PluginViewTab` 的 `pi.browser/browser` 分支改用它，`openUrlInWorkPanel` 对 http(s) 链接总是打开面板而非系统浏览器。插件 `pi.browser.*` 全部经 `bridge.invoke` 并要求 `browser.cdp`；snapshot/click/fill/evaluate/console 为脚本求值，CDP 仅放行 `Runtime.evaluate`/`Accessibility.getFullAXTree`，`screenshot` 返回 `UNSUPPORTED`（该限制已在 N5 修复：macOS 视口截图改为可用，见第 14 节）。新增 ADR 0255，并同步 IPC/插件 API 规格。验证：`pigo-desktop` 133 passed、`pnpm typecheck` 通过、`pnpm test:e2e` 21 passed / 1 skipped、`PI_E2E_APP_BIN` GUI 冒烟 22 passed、`cargo check` 通过。注：本机 Trae 沙箱会向 build script 进程注入 `/private/tmp/.sboxlib_*`，导致 `cargo test` 的 build script SIGSEGV；需用 `lldb --batch -o "process handle SIGUSR1 -s false -p true" -o run -o quit -- <toolchain>/bin/cargo test ...` 绕过执行（`cargo check` 不受影响）。

进展（2026-11-05，N4 第二批 desktop.control）：新增 `desktop_control.rs`：目录覆盖 app/project/session/agent/plans/workspace/fs 22 项与 6 个 plugin-only 协作项；`listOperations` 不暴露通道名，`invoke` 要求 `desktop.control`、递归剥离 secret 参数、按 risk 分级；dangerous 需插件 `confirm:true` + native 用户对话框（中英按系统语言，仅显示目录 id/描述/参数预览），拒绝即 `PERMISSION_DENIED`。普通操作经 `router::route` 复用渲染层通道路由（用装箱 `RouteFuture` + 独立任务绕开 `load_row`↔`staff_driver` 的 Send 循环类型错误），协作项直接调 host-core 并注入 `pluginId`/`sourceSessionId`；`session/open|create` 与 `agent/prompt` 成功后发 `session/event/changed`，每次调用写 plugin.audit 日志。`pigo-desktop` 137 passed；`cargo check` 零警告。见规格 07-plugins/03 的 Tauri port note。

进展（2026-11-05，N3 第四步扩展 loop hook）：`events.rs` 把 sidecar 的 `agent_start/agent_end/turn_start/turn_end/message_*/tool_*` 映射为 ExtensionAPI 事件（`tool_*` → `tool_execution_*`）并通过 `PluginHost::emit_extension_event` 推给带 `agentExtensions` 的插件；子代理事件不重放。`plugin_host.js` 新增父→子 `extension.event` 帧处理，先补发该会话的 `session_start`，再在 AsyncLocalStorage 会话上下文中调用 handler；CJS 直接导出函数也可作为扩展工厂。测试覆盖 `turn_start` 送达且 `session_start` 先到。`cargo test -p pigo-desktop` 129 passed，`pnpm typecheck` 通过，`pnpm test:e2e` 21 passed / 1 skipped。

## 12. 剩余任务清单（2026-11-05 复核）

复核方式：逐项对照 `desktop/apps/desktop-tauri/src-tauri/src`、`desktop/package.json`、`desktop/scripts` 与计划第 11 节。以下条目均未完成，且不重复已勾选的历史项。

### A. 功能缺口

- [x] **`plugin-asset://` 主题资源（2026-11-05）**：注册异步 scheme（macOS/Linux `plugin-asset://localhost/<pluginId>/<path>`，Windows `http://plugin-asset.localhost/...`）；只服务该插件 `contributes.themes[].assets` 声明过、且位于插件根内的图片/字体（4 MB 上限），要求 `ui.theme` 权限；主题 CSS 的 `url()` 被重写到该 scheme，未声明的资源仍被清洗掉。
- [x] **插件常驻服务监督（2026-11-05）**：`load` 成功后按 manifest `contributes.services`（最多 4 个、需 `background.service`）调用子进程 `services.start`（5s 预算），`unload` 前先 `services.stop` 再 `onUnload`；`plugin/services` 读取真实 `starting/running/stopped/failed`、重启次数与失败原因。
- [x] **插件服务崩溃重启（2026-11-05）**：进程异常退出时按 ADR 0040 重启整个插件（1/2/4/8/16s 退避、最多 5 次、存活 60s 清零）；`autoRestart: false` 或未声明服务不重启；手工 enable/disable/reload 取消待执行的重启。
- [x] **插件自带 MCP server 加载（2026-11-05）**：`contributes.mcpServers` 经权限（`mcp.server.local`/`mcp.server.remote`）与 `manifest.net.domains` 校验后进入宿主 MCP 工具表，工具名空间为 `mcp_plugin_<pluginId>_<serverId>_<tool>`；`env`/`headers` 只从插件自身设置解析 `{setting}`；stdio 子进程清空宿主环境（仅 PATH/临时目录/locale + `PI_PLUGIN_ID`），插件卸载或崩溃时删除工具并关闭连接。
- [x] **pi 扩展 loop hook（2026-11-05）**：新增父→子单向 `extension.event` 帧；壳把 sidecar 的 `agent_start/agent_end/turn_start/turn_end/message_*/tool_*` 映射为 ExtensionAPI 事件（`tool_*` → `tool_execution_*`）并推给带扩展的插件；`session_start` 先于该会话首个 loop 事件；子代理事件不重放给父会话。
- [x] **浏览器 guest 与 `pi.browser.*`（2026-11-05，N4 第一批）**：`browser.rs` 用 Tauri `unstable` 多 webview API 创建 host-owned `work-browser` 子 webview；`browser/navigate|action|getState|setBounds|setVisible|close` 全部接通，边界按 chrome 视图矩形与窗口取交集后夹紧；状态经 `browser/event/state` 推送。渲染层 `BrowserHostTab` 提供地址栏/前进后退/刷新/停止/外部打开，`openUrlInWorkPanel` 不再回退系统浏览器。插件 `pi.browser.*` 经 `bridge.invoke` 实现并统一要求 `browser.cdp`；snapshot/click/fill/evaluate/console 走脚本求值，CDP 白名单仅 `Runtime.evaluate`/`Accessibility.getFullAXTree`/`Page.captureScreenshot`，其余 `PERMISSION_DENIED`；`screenshot` 与 `Page.captureScreenshot` 经 WKWebView `takeSnapshot` 返回 PNG base64（仅可视视口；`fullPage` 与非 macOS 平台仍返回 `UNSUPPORTED`）。见 ADR 0255。
- [x] **协作投递循环（2026-11-05）**：新增 `collaboration.rs`，把 `spawn|send` 写入的 queued 投递转为 Host 队列条目（以 `collaboration:<messageId>` 幂等），目标空闲时立即启动 turn，忙碌时留在 `turn_queue` 由既有 `queue::run_next` 在 turn 结束后启动；投递失败调用 `session.collaboration.fail` 并排空 pending 回调。turn 正常/错误/中止结束、以及 prompt 启动失败时调用 `session.collaboration.settle`，把完成回执投递回来源会话。内容原样传递，满足 host-core 的 provenance 校验。
- [x] **`desktop.control`（2026-11-05，N4 第二批）**：新增 `desktop_control.rs` 审查目录（app/project/session/agent/plans/workspace/fs 共 22 项 + 6 个 plugin-only `session/collaboration/*`），`pi.desktop.listOperations` 只返回 `{id, description, risk}`，不暴露通道名；`pi.desktop.invoke` 要求 `desktop.control`，递归剥离 secret 形状参数，`read/write/dangerous` 分级；dangerous 先要求 `confirm:true`，再弹 native 对话框（系统语言，只显示目录 id/描述/参数预览），拒绝或关闭返回 `PERMISSION_DENIED`；普通操作复用 `router::route`（经 `RouteFuture` 装箱与独立任务规避 `load_row`/`staff_driver` 的 Send 循环类型错误），协作操作直接走 host-core 并注入 `pluginId`/`sourceSessionId`；成功写操作发 `session/event/changed`，每次调用按 plugin/operation/risk/结果记 audit 日志。

### B. 打包与发布

- [x] **macOS 签名链路（2026-11-05）**：打包布局保持 `bundle.macOS.files`，签名前用 Developer ID 对两个源二进制执行 `codesign --options runtime --timestamp`，再由 Tauri 打包封签（避免 `externalBin` 导致 `cargo check` 依赖构建产物）；`build-macos.sh` 构建后校验后端存在、`codesign --verify`、公证凭据下的 `stapler validate`/`spctl`，并使用 Tauri 的 `APPLE_PASSWORD`（兼容旧 `APPLE_APP_SPECIFIC_PASSWORD`）。
- [ ] **macOS 发布验收**：未签名 lane 已产出 `.app`/`.dmg` 并通过打包启动冒烟；仍需真实 Developer ID 证书跑一次签名/公证，确认 Gatekeeper 通过。
- [x] **DMG 背景资源（2026-11-05）**：补齐 `apps/desktop-tauri/build/dmg-background.png`（720×500），此前缺失导致 `bundle_dmg.sh` 失败。
- [x] **Windows 打包 lane（2026-11-05）**：新增 `scripts/build-windows.ps1` 与 `pnpm package:win`；`tauri.windows.conf.json` 把两个后端放到 `bin/`，构建脚本校验后端产物与安装包。
- [ ] **Windows 实包验收**：需在 Windows 主机执行 `pnpm package:win`，确认安装包可启动。
- [x] **staff driver 随包运行（代码链路）**：真实启动 E2E 现已断言 `staff resident driver polling` 与 `staff watch resident loop` 日志；打包应用可用 `PI_E2E_APP_BIN=<app>/Contents/MacOS/pigo-desktop PI_E2E_PACKAGED=1` 复验。

### C. 验证

- [x] **E2E 骨架（N0，2026-11-05）**：`pnpm test:e2e` 可跑；覆盖 sidecar 握手、单轮事件序列、工具经 `host.proxy` 回环、stop/dispose 契约，以及 Tauri 启动契约（打包路径、boot 顺序、allowlist）。
- [x] **E2E 矩阵（steer/附件/plan-goal，2026-11-05）**：steer 注入运行中 turn 并进入下一轮模型请求；无 active turn 返回 `TURN_NOT_FOUND`；图片附件变成 `image_url`、文件附件不进入模型；`SubmitPlan`/`SubmitGoal` → `awaiting_approval`、`plans.submit`、状态回读；`EnterPlanMode` 切换模式并在 Plan 模式阻断 Write；`executeApprovedPlan` 以内部 Agent turn 执行已批准计划。
- [x] **E2E asktool（2026-11-05）**：问题卡片事件、答案回填、非法答案数量保持 pending、未知请求拒绝。
- [ ] **E2E 矩阵剩余**：多轮/重启与 fork、长会话压缩、错误分类/重试、协作投递、打包验收仍未自动化。
- [x] **N5 全量回归（2026-11-05）**：host-core 460、`pigo-desktop` 145、sidecar `go test ./...`、`pnpm typecheck`/`docs:build`、E2E 22 全部通过；`check:release-docs` 通过。明细见第 14 节。
- [ ] **旧 Electron 单元测试**：`apps/desktop-tauri/test/*.test.mjs` 仍有 136 个失败，全部指向已删除的 `electron/main` 与 `resources/plugins`；属于移植前遗留，Tauri 门禁以 `test/e2e` 与各 workspace 包为准，待清理或迁移。
- [ ] **市场目录预检**：`check:marketplace` 依赖远端 catalog，`demo.workspace-notes/summary@0.2.3` 仍声明 `fs.*.workspace` 旧权限并会被安装时降权；需在插件仓库侧发布修复版本。
- [ ] **站点采集手工验证**：知乎、V2EX、掘金、B 站、公众号、视频号均需真实账号验证；视频号还需要一个**有已发表视频**的账号才能确认列表行 DOM 与 URL；macOS WKWebView 共用 Cookie 存储，无法按站点 partition。
- [ ] **选股线上冒烟**：需要真实 hithink-finance API Key。

## 13. 新的完成安排（2026-11-05 复核）

排序原则：外部依赖的发布验收不阻塞产品工作；先补用户可见缺口，再补插件生态；E2E 矩阵作为并行轨道随功能增长。

| 阶段 | 任务 | 依赖 | 完成判据 |
|---|---|---|---|
| **N0 回归基线 ✅** | `pnpm test:e2e`：sidecar 协议 smoke（握手/单轮/工具回环/stop/steer/附件/plan-goal/asktool）+ Tauri 启动契约；`PI_E2E_APP_BIN` 可跑真实启动冒烟 | 无 | 已通过：21 passed / 1 skipped（默认）；GUI 冒烟 1 passed |
| **N1 发布链路（代码）✅** | macOS 后端预签名 + 公证/校验；Windows 打包 lane；staff driver 启动断言 | N0 | `cargo check`、真实启动冒烟、打包契约测试通过 |
| **N1b 发布验收（外部依赖，可并行/阻塞）** | 用真实 Developer ID 跑 `pnpm package:mac`；在 Windows 主机跑 `pnpm package:win` | 证书 / Windows 主机 | 已产出并冒烟通过未签名 `.app`/`.dmg`；签名后 Gatekeeper 通过、Windows 安装包可启动 |
| **N2 用户可见缺口 ✅** | ✅ 六个渠道全部接入（视频号为 list-only）；✅ Copilot policy 自动启用与每模型 `apiStyle` | N0；站点需业主扫码验证 | 代码与测试通过；视频号需有已发表视频的账号校准行选择器 |
| **N3 插件生态 ✅** | ✅ `plugin-asset://` scheme → ✅ resident service 监督（含崩溃重启）→ ✅ 插件自带 MCP 加载 → ✅ pi 扩展 loop hook | N0 | 主题资源可加载；服务可启停并如实上报；插件 MCP 工具进入工具表；扩展 `on(...)` 收到真实 loop 事件。`pigo-desktop` 129 passed；E2E 回归通过 |
| **N4 潜伏能力 ✅** | ✅ 浏览器 guest/CDP 与 `pi.browser.*`（含 macOS 视口截图）；✅ `desktop.control` 审查目录与 `pi.desktop.*`；✅ 协作投递循环 | N0 | 浏览器：真实 webview 页面可打开、可夹紧、可截图、插件可调用；桌面控制：目录可列举、dangerous 需 `confirm` + native 同意、调用复用通道路由；协作：queued 投递自动启动目标 turn，完成后回执来源会话 |
| **N5 验收收尾（自动部分完成）** | 全量回归、发布文档、许可证边界确认、站点/选股手工冒烟记录 | N1b–N4 | 自动回归与发布文档预检通过、许可证随包；打包验收与手工冒烟仍依赖 N1b/账号，见第 14 节 |

并行轨道 E（E2E 矩阵补齐）：✅ asktool；下一步 多轮/重启与 fork → 长会话压缩 → 错误分类/重试 → 协作投递 → 打包验收。每个功能阶段新增的能力必须同步补一条 E2E，不单独攒到最后。

N2 子步骤顺序：✅ B 站 → ✅ 公众号 → ✅ 视频号（list-only）→ ✅ Copilot 边角。公众号与视频号都属于“创作者后台”模式：只采自己账号的已发表内容，不做全站关键词搜索；公众号必须配置 MP 已发表列表作为 seed URL，视频号固定打开 `/platform/post/list` 并读取 wujie shadow root。视频号登录态已确认为 `sessionid` + `wxuin`；剩余风险是列表行 DOM 未在真实已发表视频下校准。

N0 运行方式：

- `pnpm test:e2e`：sidecar 协议 smoke + 启动契约，GUI 冒烟默认跳过。
- `PI_E2E_APP_BIN=target/debug/pigo-desktop pnpm test:e2e`：附带真实 Tauri 启动冒烟（会短暂打开窗口）。
- `PI_DESKTOP_AGENT_BIN=<path>` 或 `PI_E2E_SKIP_BUILD=1`：复用已构建 sidecar，跳过 `go build`。
- 测试位于 `apps/desktop-tauri/test/e2e/`，与仍引用已删除 Electron 目录的旧 `test/*.test.mjs` 隔离。

N1 运行方式：

- `pnpm package:mac`：构建 host/sidecar、按需签名后端、生成 `.app`/`.dmg`，并校验 `Contents/Resources/bin` 下的两个后端。
- 签名/公证：设置 `APPLE_SIGNING_IDENTITY` 与 `APPLE_ID`/`APPLE_PASSWORD`/`APPLE_TEAM_ID`（或 `APPLE_API_*`）；Tauri 负责公证与 staple，脚本随后执行 `codesign --verify`、`stapler validate`、`spctl`。
- `pnpm package:win`：在 Windows 主机构建安装包，并校验后端产物与安装包均已生成。
- 打包后启动冒烟：`PI_E2E_APP_BIN=<app>/Contents/MacOS/pigo-desktop PI_E2E_PACKAGED=1 pnpm test:e2e`，验证打包布局（`Resources/bin`）、sidecar configure 与 staff driver 启动。

可并行轨道：N1b（凭据/Windows 主机就绪即跑）与 N2 可并行；并行轨道 E 随功能推进。N3/N4 已完成，N5 的自动回归、发布文档与许可证边界已完成，剩余打包验收与手工冒烟见第 14 节。

## 14. N5 验收记录（2026-11-05）

### 自动回归

| 检查 | 命令 | 结果 |
|---|---|---|
| host-core | `cargo test -p host-core` | 460 passed / 0 failed |
| Tauri 壳 | `cargo test -p pigo-desktop` | 145 passed / 0 failed（含 OpenCode 路由头 3 条、浏览器截图 2 条、协作投递 3 条） |
| sidecar | `go test ./...`（`desktop/sidecar`） | 全部 ok |
| TS/构建 | `pnpm typecheck`、`pnpm docs:build` | 通过 |
| E2E | `PI_E2E_APP_BIN=target/debug/pigo-desktop pnpm test:e2e` | 22 passed / 0 skipped（含真实 Tauri 启动） |
| workspace 包 | `pnpm -r test` | `packages/*` 与 `docs` 全绿 |
| 发布文档 | `pnpm check:release-docs` | 通过（0.14.9 / 0.14.x） |
| 市场目录 | `pnpm check:marketplace` | **失败**：远端 catalog 的 `demo.workspace-notes/summary@0.2.3` 仍用 `fs.read.workspace` 等旧权限 |
| 旧桌面单测 | `node --test test/*.test.mjs`（`apps/desktop-tauri`） | 920 pass / 136 fail，全部因 `electron/main` 与 `resources/plugins` 已删除 |

### 本轮修复

1. **发布预检路径**：`check-release-docs.mjs` 与中英 release runbook 从 Electron 时代的 `apps/desktop/resources/models.dev/api.json` 改为实际打包的 `apps/desktop-tauri/resources/models.dev/api.json`，预检恢复通过。
2. **i18n 渲染键扫描**：`packages/i18n/test/renderer-keys.test.mjs` 指向 `apps/desktop-tauri/src`，24 tests 恢复通过。
3. **OpenCode 路由头回归**：会话与子代理流改由 sidecar 承担后，`x-opencode-session` / `x-opencode-client` / `User-Agent` 没有调用方；现在由 `launch.rs` 在启动参数中对主 provider 与每个 subagent provider 注入（显式非空调用方值优先），一次性补全仍由 `agent-runtime` 负责。3 条 Rust 单测覆盖命中条件、非 OpenCode 不变与空值替换。
4. **许可证随包**：`tauri.conf.json` 的 bundle resources 增加 `licenses/LGPL-3.0-or-later.txt`（`desktop/LICENSE`）与 `licenses/MIT.txt`（pigo 根 `LICENSE`），E2E 启动契约新增断言。边界仍为：`desktop/` 整体 LGPL-3.0-or-later，仓库其余部分 MIT。
5. **浏览器截图（2026-11-05）**：`browser.screenshot` 与 CDP `Page.captureScreenshot` 不再无条件 `UNSUPPORTED`；macOS 经 WKWebView `takeSnapshot:withConfiguration:completionHandler:` 获取视口快照，转 PNG 后以 base64 返回，主线程调度与 10s 超时由 `capture_png` 处理。`fullPage` 明确拒绝（不返回裁剪图），非 macOS 仍返回 `UNSUPPORTED`；新增 2 条 Rust 测试，插件 API 与 ADR 0255 同步。
6. **协作投递循环（2026-11-05）**：`spawn|send` 成功后排空 queued 投递给目标会话（幂等队列条目 + 立即或 turn 结束后启动），turn 结束/中止/启动失败时 settle 并投递完成回执；失败投递走 `session.collaboration.fail`。新增 3 条 Rust 测试；`pigo-desktop` 145 passed，`cargo check` 零警告。
7. **macOS 未签名打包（2026-11-05）**：产出 `pigo-desktop.app`（73 MiB，x64）与 `pigo-desktop_0.14.9_x64.dmg`（41 MiB）；包内含 host-core、Go sidecar、models.dev 快照与 LGPL/MIT 许可证。`PI_E2E_APP_BIN=<app>/Contents/MacOS/pigo-desktop PI_E2E_PACKAGED=1 pnpm test:e2e` 22 passed（真实启动，`staff resident driver polling` 已断言）。DMG 背景资源补齐后 `bundle_dmg.sh` 通过；签名/公证仍待 N1b。

### 仍阻塞（不计入“已验收”）

- **N1b**：macOS Developer ID 签名/公证与 Windows 实包未执行；未签名 macOS 包与 `PI_E2E_PACKAGED=1` 启动冒烟已完成。
- **手工冒烟**：六站点采集、视频号列表行 DOM、选股 API Key 均需真实账号。
- **旧 Electron 单测**：136 个失败为移植前遗留（引用已删除目录），需单独清理或迁移，不能作为 Tauri 门禁。
- **市场目录**：预检失败需插件仓库发布修复版本。
- **已知功能缺口**：浏览器截图仅 macOS 可视视口（`fullPage` 与其他平台 `UNSUPPORTED`）；协作投递尚无自动化 E2E（实现与单测已通过）。



