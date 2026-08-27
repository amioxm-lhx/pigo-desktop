# PIGO 项目全量分析报告

> 分析日期：2026-08-19
> 分析对象：`github.com/smallnest/pigo`（pi AI Agent 的 Go 重实现）
> 当前代码版本：**v0.5.4**（git tag 最新）
> 书稿（book/）最后更新：2026-07-28（约对应 v0.3.7 时代）

---

## 一、项目定位与概况

pigo 是用 Go 完整复刻 [pi](https://pi.dev) 的命令行编码智能体（AI coding agent），核心能力是「读懂需求 → 读写文件 / 执行命令 / 检索代码 / 抓取网页 → 借助 LLM 完成闭环」。项目具有两个运行面：

| 维度 | 说明 |
|------|------|
| **运行模式** | 无头（headless）一次性执行（`-p`，适合脚本/CI）+ 交互式 REPL + 全屏 TUI（bubbletea v2） |
| **LLM 接入** | 38 个内置 Provider（OpenAI / Anthropic / Gemini / DeepSeek / 国内厂商等），OpenAI Chat Completions、Anthropic Messages、OpenAI Responses 三种协议 |
| **工具能力** | 15 个内置工具（文件读写 / 搜索 / bash / 网络 / 记忆 / 子 Agent 委派） |
| **工程特性** | 会话持久化（JSONL 树）、上下文自动压缩、项目信任、Hooks、Skills、插件、包管理、自更新、持久记忆（memory）、记忆整合（dream）、远程控制 |
| **技术栈** | Go 1.27+、spf13/pflag、bubbletea v2 + lipgloss v2、modernc.org/sqlite、openai-go、jsonschema v6、cobra 无（纯手写 CLI） |
| **SDK 化** | `agent/` 公共包可嵌入其他 Go 程序（issue #554） |

**版本演进节奏**：v0.1.0（2026-07-20）→ v0.5.4（当前），约一个月内迭代 20+ 个版本，功能持续快速叠加。这正是文档漂移的根源。

---

## 二、代码架构全景

### 2.1 分层架构

```
┌─────────────────────────────────────────────────────────┐
│  agent/         公共 SDK 包（Session/Prompt/Stream）      │
├─────────────────────────────────────────────────────────┤
│  cmd/pigo/      纯入口：flag 解析 + config 覆盖 + dispatch │
├─────────────────────────────────────────────────────────┤
│  internal/cli/  运行装配(run) · REPL · TUI · headless    │
│                 · status · btw · goal · config · pkgcmd │
├─────────────────────────────────────────────────────────┤
│  internal/runtime/  两层 Agent 循环 · 系统提示 · 子代理    │
│                     · 技能 · 斜杠命令 · 遥测 · checkpoint │
├─────────────────────────────────────────────────────────┤
│  internal/provider/ Provider 注册表 · 三种协议 · SSE      │
│                     · 鉴权 · 模型目录                     │
│  internal/agenttool/ 工具注册表 · 执行器 · 15 个工具       │
│  internal/hooks/     Hook 系统（9 个 hook 点）            │
├─────────────────────────────────────────────────────────┤
│  支撑模块：session(会话树) · compaction(压缩) · trust(信任)│
│           memory(持久记忆) · dream(记忆整合)              │
│           remotecontrol(远程控制) · selfupdate(自更新)    │
│           plugin · pkgmgr · pihost · jsonrpc · clipboard │
├─────────────────────────────────────────────────────────┤
│  internal/agentcore/  核心类型（Message/Tool/Event），零依赖 │
└─────────────────────────────────────────────────────────┘
```

### 2.2 模块规模（internal/ 下 .go 文件数）

| 模块 | 文件数 | 职责 |
|------|-------|------|
| agenttool | 35 | 工具实现与注册（含后台 bash、重试、文件快照、websearch 后端） |
| runtime | 40 | Agent 循环、提示词、配置、子代理、技能、遥测 |
| cli/tui | 38 | 全屏 TUI（bubbletea v2，最大子目录） |
| cli/repl | 22 | 行式 REPL（行编辑、rewind、远程控制、dream） |
| dream | 22 | /dream 记忆整合（plan/distill/consolidate/apply/schedule） |
| cli/run | 19 | 非交互运行装配 |
| provider | 32 | LLM 协议层与 Provider 注册表 |
| memory | 12 | 持久记忆（FTS 检索、reconcile、SQLite 存储） |
| remotecontrol | 12 | 手机/浏览器远程控制 REPL |
| hooks | 12 | 用户可扩展生命周期 Hook（shell 命令驱动） |
| agentcore | 12 | 核心数据类型 |
| 其他 | ~50 | session / compaction / trust / plugin / pkgmgr / selfupdate / jsonrpc / pihost / clipboard / builtinskills / cli 各子包 |

### 2.3 核心机制要点

1. **两层 Agent 循环**（`internal/runtime/loop.go`）
   - 内层：流式回复 → 停止原因分派 → 批量执行工具 → 回填，直到某轮无工具调用；
   - 外层：内层收敛后查 `GetFollowUpMessages`，有后续消息则重跑内层；
   - 6 个扩展钩子（GetFollowUpMessages / GetSteeringMessages / PrepareNextTurn / ShouldStopAfterTurn / OnStop / Reminders）+ `finish()` 唯一出口。

2. **Provider 注册表**（`internal/provider/registry.go`）
   - 38 个 Provider 以纯数据声明（`ProviderSpec`：Name / EnvVars / DefaultBaseURL / Protocol / AuthScheme），零运行时开销；
   - 模型名前缀启发式推断（claude-* → anthropic、qwen-* → dashscope 等）；
   - 协议含 OpenAI Chat Completions、Anthropic Messages、**OpenAI Responses**（v0.5.x 新增）。

3. **工具系统**（`internal/agenttool/`）
   - 注册即编译 JSON Schema（坏 Schema 启动期拦截）；参数校验失败转为模型可读的字段级错误；
   - 执行器五道闸门（信任 / 副作用确认 / 重试 / 结果裁剪 / 取消），批量执行支持串行/并发；
   - 当前 15 个工具：read / write / edit / grep / find / ls / bash / bash_output / kill_bash / todo / webfetch / websearch / memory_search / task（+goal 模式专用 goal_complete / goal_blocked）。

4. **会话与压缩**：JSONL 会话树（id/parentId，v3 schema，支持 fork/clone/export/import）；token 计数 → 找切点 → 摘要 → RebuildContext，`/compact` 手动 + 自动触发，checkpoint 持久化实现「无限上下文」。

5. **安全**：项目信任三态（Trusted/Untrusted/Undecided）持久化；工具级准入（--allowed-tools / --disallowed-tools，黑名单优先、子 Agent 继承、--approve 不可绕过）；hook payload 不含凭证。

6. **v0.4–v0.5 新增大块**：Hooks（9 点）、TUI、/btw 侧线程、/goal 自主模式、/dream 记忆整合、持久记忆 memory、远程控制、自更新、OpenAI Responses driver、公共 SDK（agent/）。

---

## 三、文档同步状况审计（核心结论）

**结论：项目存在三类文档，同步程度差异极大。书稿（book/）严重滞后，CHANGELOG 有版本缺口，README/docs 网站相对同步。**

### 3.1 版本差距总览

| 文档 | 覆盖到 | 代码当前 | 差距 |
|------|--------|---------|------|
| book/ 书稿（15 个 .md） | ~v0.3.7（07-28） | v0.5.4 | **7 个版本、约 30 天功能增量未同步** |
| CHANGELOG.md | v0.4.3 + Unreleased | v0.5.4 | **v0.4.4、v0.5.0~v0.5.4 共 6 个版本无记录** |
| README.md | 基本同步（含 SDK/hooks/工具准入/自更新） | v0.5.4 | 少量 flag 细节待核对 |
| docs/ 网站（index.html + web/） | 宣传 TUI/hooks/memory | v0.5.4 | 相对同步，细节需抽查 |

### 3.2 书稿（book/）滞后清单 —— 逐项核实

#### ① 引用的源码文件大量失效（结构性过时，最严重）

书稿多章以 `cmd/pigo/*.go` 为锚点，但 **0.3.7 已把 CLI 逻辑全部重构进 `internal/cli/*`**，下述文件当前**均不存在**：

| 书稿引用 | 章节 | 现状 |
|---------|------|------|
| `cmd/pigo/run.go` | 第 1 章 | 缺失 → `internal/cli/run/run.go` |
| `cmd/pigo/repl.go` | 第 1 章 | 缺失 → `internal/cli/repl/` |
| `cmd/pigo/interactive.go` | 第 1 章 | 缺失 → `internal/cli/repl/`、`cli/tui/` |
| `cmd/pigo/baseurl.go` | 第 1 章 | 缺失 → `internal/provider/resolve.go` |
| `cmd/pigo/color.go` | 第 1 章 | 缺失 → `internal/cli/ui/` |
| `cmd/pigo/trust.go` | 第 8 章 | 缺失 → `internal/trust/` + `internal/cli/run` |
| `cmd/pigo/subagent_rpc.go` | 第 9 章 | 缺失 → `internal/cli/headless/` |
| `cmd/pigo/pkgcmd.go` | 第 10 章 | 缺失 → `internal/cli/pkgcmd/` |

#### ② 符号名漂移

| 书稿符号 | 当前等价物 |
|---------|-----------|
| `newRunConfig`（reading-guide ②） | `internal/cli/run.NewConfig` / `SetupEnv` |
| `resolveProvider`（reading-guide ②、第 4 章） | `internal/provider.ResolveProvider` |
| `runPackageCommand`（第 1 章） | `internal/cli/pkgcmd.Run` |
| `printProviderHelp`（第 1 章） | `internal/cli` provider 帮助 |
| `runSubAgentRPC`（第 1 章） | `internal/cli/headless.RunSubAgentRPC` |

> 仍准确的符号：`dispatch`（cmd/pigo/main.go:315）、`cliOptions`、`StartRun`（internal/runtime/loop.go:135）、`BeforeToolCall`、`GetFollowUpMessages`、`finish()`。**两层循环骨架没有漂移，这是书稿最有价值、最不需要动的部分。**

#### ③ 工具清单过时

| 书稿第 5 章 | 代码现状 |
|------------|---------|
| 「七类工具」：read/write/edit/grep/find/ls/bash/webfetch/todo | **15 个**：书稿覆盖之外新增 `bash_output`、`kill_bash`、`websearch`、`memory_search`、`task` |
| bash：流式输出、超时、取消 | 新增 `run_in_background` 后台执行 + `bash_output`/`kill_bash` 控制 |
| 「TodoTool 是唯一有状态的工具」 | 已不成立：另有 BashJobStore（后台任务）、FileSnapshotRecorder（/rewind）、memory store |
| 搜索三件套 grep/find/ls | 仍准确（ls 仍在）✓ |

#### ④ 事件类型数量过时

- 书稿第 2 章：**10 种事件类型**；
- 代码实际：**14 种** —— 新增 `CompactionStartEvent`、`SubAgentProgressEvent`（v0.4.3）、`TelemetryEvent`（v0.3.0 即引入，书稿未计）。

#### ⑤ 全新功能模块完全未覆盖（书稿零提及）

| 功能 | 代码位置 | 引入版本 |
|------|---------|---------|
| 全屏 TUI | internal/cli/tui（38 文件） | v0.4.0 |
| Hooks 系统（9 个 hook 点） | internal/hooks | v0.4.1 |
| 远程控制（手机/浏览器配对） | internal/remotecontrol | v0.4.2 |
| /btw 侧线程、/goal 自主模式 | cli/btw、cli/goal | v0.3.4 / v0.3.5 |
| 持久记忆 + 无限上下文 checkpoint | internal/memory | v0.5.x |
| /dream 记忆整合 | internal/dream（22 文件） | v0.5.x |
| OpenAI Responses API driver | internal/provider/responses.go | v0.5.x |
| 公共 SDK 包 | agent/ | v0.5.x（#554） |
| 自更新 | internal/selfupdate | Unreleased |
| /rewind、/think、/status、/session | cli/repl、cli/status | v0.4.x |

#### ⑥ 其他细节

- thinking 级别：书稿描述 7 级中的部分，代码已新增 **max**（高于 xhigh）；
- Provider 数量：书稿第 4 章时代的清单已过时（现 38 个，新增 novita、opencode、kimi-coding、xiaomi 系列、azure-openai-responses 等）；
- 书稿未提 `--cwd`、`--allowed-tools`/`--disallowed-tools`（工具级准入）、`--prompt-template`、`--thinking-level` 等 flag；
- book/outline.md 的「对应源码包」列与现状已脱节。

### 3.3 CHANGELOG.md 缺口

| 问题 | 说明 |
|------|------|
| 版本缺失 | git tag 已到 v0.5.4，但 CHANGELOG 正文最后是 v0.4.3（2026-07-31） |
| 缺失版本 | **v0.4.4、v0.5.0、v0.5.1、v0.5.2、v0.5.3、v0.5.4** |
| Unreleased 部分覆盖 | 已记 allowed-tools、self-update，但 dream/memory/Responses/SDK 等大特性未完整入档 |

### 3.4 README / docs 相对同步（无需大动）

- README 已覆盖：工具级准入、websearch、task 工具、SDK 嵌入、hooks、自更新、模型名前缀推断表、内置 Provider 大表（含 novita）——与代码基本一致；
- docs/ 网站宣传 TUI/hooks/memory，方向正确；
- 待核对项：README 命令行参数表是否含 `--dream`、`--cwd`、`--thinking-level`、`--subagent-rpc`；docs 站点细节（如 issue 页数量停留在 #0338 附近，实际 issue 已到 #556+）。

---

## 四、文档更新建议（按优先级）

### P0 —— 立即修（错误信息会误导读者）

1. **修复书稿 8 处失效文件路径**：第 1/8/9/10 章的「关键文件」清单从 `cmd/pigo/*.go` 改为 `internal/cli/*` 实际位置（见 3.2① 对照表）。
2. **补齐 CHANGELOG 6 个缺失版本**：v0.4.4、v0.5.0~v0.5.4 的特性可依据 git log 与 task 文档（tasks/）整理。

### P1 —— 应补（功能已存在但书稿完全没有）

3. **书稿新增章节或附录**：TUI（第 4 章之后的运行面）、Hooks（可与第 8 章安全合并）、memory + dream（可作第 6 章压缩的延伸章「无限上下文」）、remotecontrol、OpenAI Responses 协议（第 4 章补一节）、SDK（可放后记前）。
4. **更新第 5 章工具清单**：「七类工具」→ 15 个工具，补充 bash 后台执行、websearch、memory_search、task 的说明；修正「唯一有状态工具」的表述。
5. **更新第 2 章事件类型**：10 种 → 14 种。

### P2 —— 顺手改

6. **符号名勘误**：`newRunConfig` → `NewConfig/SetupEnv`、`resolveProvider` → `ResolveProvider`（reading-guide 与第 1、4 章）。
7. **outline.md 同步**：更新「对应源码包」列与各章「关键文件」清单。
8. **README flag 表核对**：补 `--dream`、`--cwd`、`--thinking-level` 等。

> 提示：书稿中最稳固、最值得保留的部分是第 2 章（agentcore 契约）、第 3 章（两层循环）、第 6 章（压缩机制）的核心概念——这些抽象层没有随功能叠加而漂移，修订时以「补新不拆旧」为原则最省力。

---

## 五、附录：关键文件索引

| 关注点 | 文件 |
|--------|------|
| CLI 入口 | `cmd/pigo/main.go` |
| 运行装配 | `internal/cli/run/run.go` |
| 两层循环 | `internal/runtime/loop.go`、`stream_response.go` |
| 系统提示组装 | `internal/runtime/prompt.go` |
| 分层配置 | `internal/runtime/config.go` |
| Provider 注册表 | `internal/provider/registry.go` |
| Provider 解析 | `internal/provider/resolve.go` |
| 协议解码 | `internal/provider/openai.go`、`anthropic.go`、`responses.go` |
| 工具注册/执行 | `internal/agenttool/registry.go`、`tool_executor.go`、`batch_executor.go` |
| 会话持久化 | `internal/session/session.go`、`export.go` |
| 上下文压缩 | `internal/compaction/` |
| 项目信任 | `internal/trust/` |
| Hooks | `internal/hooks/` |
| 持久记忆 | `internal/memory/` |
| 记忆整合 | `internal/dream/` |
| 远程控制 | `internal/remotecontrol/` |
| 子 Agent | `internal/runtime/subagent.go`、`task.go`、`internal/cli/headless/` |
| 包管理 | `internal/pkgmgr/`、`internal/cli/pkgcmd/` |
| SDK | `agent/agent.go`、`agent/options.go` |
| 书稿 | `book/outline.md`（大纲）、`book/reading-guide.md`（主线） |
