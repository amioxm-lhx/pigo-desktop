# 延伸能力速览

> **主线坐标｜主线之外的新大陆**：前 10 章把 pigo 的"内脏"——循环、Provider、工具、压缩、会话、信任、子 Agent 与可扩展生态——从 `cmd/pigo` 一路追到了 `internal/cli`。但 v0.4–v0.5 又在主干之外长出了一圈新能力：全屏 TUI、Hooks、持久记忆与 `/dream`、OpenAI Responses 协议、公共 SDK、远程控制，以及 `/btw` `/goal` 两种交互模式。它们代码都已落地、只是正文还盖不到。本章给每一块做一页速览——只点清"它是什么、落在哪个包、关键入口怎么调用"，不展开实现，留作你按图索骥的索引。

前 10 章是"拆牛"，这一章是"牛身上还长着的几块新肉"。每块都给了经核实的源码位置，方便你直接 `grep` 进去。

## 全屏 TUI：`internal/cli/tui`

REPL 是"一行一行"的终端界面；TUI 则是基于 [bubbletea](https://github.com/charmbracelet/bubbletea) 的**全屏**界面，把同一份装配好的运行环境（Provider、工具、会话、信任、技能、插件）整个搬进一个面板化的视图里。入口是：

```go
// internal/cli/tui/run.go
func Run(opts Options) error
```

`Options`（`internal/cli/tui/options.go`）**逐字段镜像** `repl.Options`（`internal/cli/repl/interactive.go`），所以 `cmd/pigo` 的 `dispatch` 能把同一份装配结果同时喂给 REPL 和 TUI 两条路径，下游的会话/实时/斜杠/信任接线可以原样移植——这是它和 REPL 共享入口接缝的关键设计。包内按职责拆得很细：`model.go`+`session.go` 是 bubbletea 的 Model/会话骨架，`bridge.go` 把 Agent 事件流接到 UI，`input.go`/`slash.go`/`markdown.go` 管输入与渲染，`statusbar.go`/`toolcard.go`/`subagentpanel.go`/`transcript.go` 是各信息面板，`remotecontrol.go` 把下面要讲的远程控制也接进了 TUI，`banner.go`/`spinner.go`/`selection.go`/`gitinfo.go`/`clipimage.go` 是细节点缀。它目前仍以 `Run` 为骨架节点，多数 `Options` 字段已就位锁定契约、但还没被全部消费——属于"结构已立、体验在补"的半成型模块。

## Hooks 系统：`internal/hooks` 与 `internal/cli/run` 的接线层

Hooks 让你在 Agent 运行的关键节点上挂外部命令（Shell/任意可执行文件），在 Claude Code 那套契约上做了对齐：pigo 把事件写进 hook 的 stdin，读回 stdout 决定放行还是拦截。它定义了 **9 个 hook 事件点**：

| 事件点 | 触发时机 | 典型用途 |
|---|---|---|
| `PreToolUse` | 工具调用执行**前** | 拦截危险命令、改写入参 |
| `PostToolUse` | 工具调用执行**后** | 记录、后处理、审计 |
| `UserPromptSubmit` | 用户提交提示词时 | 过滤/改写提示词 |
| `Stop` | 顶层运行结束前 | 收尾检查、禁止过早收工 |
| `SubagentStop` | 子 Agent 结束前 | 子任务收尾检查 |
| `SessionStart` | 会话启动时（新开/恢复） | 环境准备、欢迎语 |
| `SessionEnd` | 会话结束时 | 清理、上报 |
| `PreCompact` | 压缩发生前（手动/自动） | 压缩前钩子 |
| `Notification` | 需要通知时 | 推送、提醒 |

契约（`internal/hooks/protocol.go`）：pigo 向 hook 的 stdin 写入 `HookInput`（只含可观测、非机密的字段——**绝不带 API key**），hook 退出码 `0`=放行、`2`=拦截（stderr 为理由）；退出 `0` 时若 stdout 是一段合法 JSON 则解析为 `HookOutput`，可声明 `decision: block|approve`、注入 `additionalContext`、或（仅 `PreToolUse`）用 `updatedInput` **改写工具入参**。`Dispatcher`（`internal/hooks/dispatch.go`）负责匹配、按序执行、合并结果，遵循 **fail-open**：某个 hook 跑挂了只告警跳过、不阻断 Agent；而 `PreToolUse` 一旦被拦截就**短路**后续 hook，避免"被拦下的调用又被改写"。

Hook 通过 `config.toml` 的 `[hooks]` 表（或配置层）声明为 `HookSet`，再由 `internal/cli/run` 的接线层挂进主循环：`hooks_install.go` 的 `InstallSeams` 把它编进 `BeforeToolCall`/`AfterToolCall`（`preToolCallHook`/`postToolCallHook`）、`hooks_prompt.go` 的 `DispatchUserPromptSubmit`、`hooks_stop.go` 的 `stopHook`/`InstallSubagentStop`、`hooks_session.go` 的 `DispatchSessionStart`。`nil` 的 dispatcher 是合法的无操作，所以"没配 hook"时零成本。

## 持久记忆与 `/dream`：`internal/memory` + `internal/dream`

pigo 有两类"记得住"的东西：会话内的压缩记忆（第 6 章）和**跨会话的持久记忆**。持久记忆的根目录是 `$PIGO_HOME/memory`，默认回退到 `~/.pigo/memory`（`internal/cli/run/run.go` 的 `MemoryDir()`）——一个**全局共享**的存储，便于跨项目的"全局"记忆。`internal/memory` 包负责它的落地与检索：`paths.go` 把文件解析成带作用域的条目（`ScopeGlobal`/`ScopeProject`/`ScopeCC` 等），`store.go` 管读写，`search.go`/`ftsquery.go` 提供检索，`reconcile.go` 做跨目录对账。REPL 与 TUI 里的 `/memory` 命令都会调 `memstatus.RunMemory`（`internal/cli/memstatus`）打印一份带配色的多段报告（开关状态、条目数、占用等）。

`/dream`（第 4.1 节 US-007）是在持久记忆之上做的**整合蒸馏**：把散落的记忆条目去重、合并、裁剪、再让模型"蒸馏"成更浓缩的笔记。命令实现在 `internal/cli/repl/dream_repl.go`（`runDream`），启动时还有一个后台触发（`dream_startup.go`）。它支持 `--dry-run` 只预览不落盘；同一时刻用 `memoryRoot/global/dream/dream.lock` 加锁防并发。`internal/dream` 包是真正的引擎：`consolidator.go`（`Consolidate`/`distill`）跑 LLM 蒸馏、`plan.go` 产出确定性的"整合计划"、`report.go` 生成结构化变更摘要、`apply.go` 落地、`prompt.go`/`config.go` 管提示词与配置。记忆按项目作用域归属，所以一次 `/dream` 只蒸馏属于当前项目的那部分。

## OpenAI Responses 协议：`internal/provider/responses.go`

第 4 章讲了 Chat Completions 与 Messages 两套协议；v0.5 又补了第三套——**OpenAI Responses API**。它不是走 `/chat/completions`，而是走 `/responses`，用 `response` 对象承载多轮、工具调用与推理摘要。pigo 用 `ProtocolOpenAIResponses`（字符串值 `"openai/resp_api"`）标识，在 `internal/provider/resolve.go` 里：

```go
case ProtocolOpenAIResponses:
    // Responses 驱动没有内建默认端点，必须显式 --base-url
    if baseURL == "" {
        return nil, "", fmt.Errorf("--protocol openai/resp_api requires --base-url")
    }
    return NewOpenAIResponsesProvider("openai", baseURL, ...), "openai", nil
```

注意：**`resp_api` 协议没有内建公共端点，必须带 `--base-url`**（不像 anthropic 路径默认指向公共 API）。驱动实现是 `responsesDriver`（`internal/provider/responses.go`），它的 `pump` 逐事件消费 SSE：`ResponseTextDeltaEvent`（正文增量）、`ResponseReasoningSummaryTextDeltaEvent`（推理摘要增量）、`ResponseOutputItemDoneEvent`（工具调用完成）、`ResponseCompletedEvent`/`ResponseFailedEvent`/`ResponseErrorEvent`（终态）。`buildResponsesParams` 把 `CompletionRequest` 的消息映射成 Responses 的 `Input` 列表，`buildResponsesTools` 把 Agent 工具编译成函数工具，并额外支持图片输入与推理（reasoning）字段——所以走 Responses 协议的模型能拿到和原生 OpenAI 一致的"边想边说"体验。

## 公共 SDK 包：`agent/`

前 10 章都在讲"pigo 这个命令行程序"；`agent/` 则是把**同一套运行能力打包成一个可被你自己的 Go 程序嵌入的 SDK**，让外部代码用几行就能驱动一个 pigo Agent。它的设计铁律是"出口类型全是 Go 原语（string / []string / bool / func）"，绝不把 `internal/*` 的类型漏到你代码里，pigo 内部怎么演进都不会破坏你。

```go
sess, err := agent.New(
    agent.WithModel("claude-opus-4-8"),
    agent.WithAPIKey(os.Getenv("ANTHROPIC_API_KEY")),
)
if err != nil { log.Fatal(err) }
defer sess.Close()
reply, err := sess.Prompt(ctx, "Say hello in one word.")
```

`Session` 的句柄很薄：`Prompt` / `Stream`（流式）/ `Reset`（重置对话）/ `ToolNames` / `Model` / `Provider` / `Close`（`internal/agent/agent.go`）。能力用 `Option` 旋钮装配（`internal/agent/options.go`）：`WithModel`/`WithBaseURL`/`WithProtocol`/`WithProvider`/`WithAPIKey` 决定"用谁、说哪套协议、拿哪个密钥"；`WithSystemPrompt`/`WithAppendSystemPrompt` 注入系统提示；`WithThinkingLevel` 设推理档位。**安全默认值要特别当心**：一个 SDK Session 默认就带着 pigo 的全套内置工具、且**自动执行、不弹逐次确认**（等价于命令行 `--approve`）——所以它能读、改、删工作目录下的文件并跑 shell。要收敛就上 `WithTools`（白名单）、`WithDisallowedTools`（黑名单，总是优先）、`WithoutTools`（纯文本补全）。`WithSkills`/`WithMemory` 默认**关闭**，保证嵌入式会话是密闭的——不读不写机器上共享的 pigo 状态，除非你显式打开。

## 远程控制：`internal/remotecontrol`

远程控制让 pigo 在局域网里起一个**网页控制台**：你在手机/浏览器上打开一个带配对码（QR）的页面，就能远程给 Agent 发提示、对确认弹窗点"允许/拒绝"。REPL 里的 `/remote-control` 命令（经 `internal/cli/repl/remotecontrol.go` 的 `runRemoteControl`）拉起这个服务。核心在 `internal/remotecontrol`：

```go
// server.go
type Config struct { /* Addr, TokenSecret, ... */ }
func NewServer(cfg Config, handler Handler) *Server
func (s *Server) SetHandler(h Handler)

// bridge.go
func NewBridge(sink Sink) *Bridge   // Bridge 实现 Handler 接口
```

`Handler` 接口只有两个方法：`OnInput(text string)`（远端发来的提示）和 `OnDecide(id string, approve, always bool)`（对确认弹窗的答复）；`Bridge` 把 REPL 的信任闸门接到这个接口上。HTTP 路由有 `/healthz`、`/pair`（配对）、`/ws`（WebSocket 长连）、`/`（`internal/remotecontrol/server.go`）。配对用 `token.go` 的令牌 + `qr.go` 生成的二维码，地址探测在 `lanaddr.go`。TUI 也内置了 `remotecontrol.go` 走同一套机制——所以无论走 REPL 还是 TUI，远程控制台都能挂上。

## `/btw` 与 `/goal` 两种交互模式：`internal/cli/btw` + `internal/cli/goal`

REPL 里除了 `/compact`、斜杠命令，还有两个"元模式"命令，它们都被**拦截在 REPL 主循环里**（而不是走普通的斜杠 `Action` 闭包），因为它们要跑 Agent 流、读实时上下文、改共享状态——纯 `string→string` 的 Action 做不到。

**`/btw`**（side thread，对应 pi 的 pi-btw）：抛出一个**临时侧线程**，问模型一个不污染主对话的小问题。隔离契约是它存在的全部意义——侧线程跑在**主对话的一份拷贝**上，问答只追加到那份拷贝，**绝不**写进 `host.AgentCtx().Messages`，也不落盘；关掉侧线程、切会话、重启 pigo 都将其丢弃。带参数时开新侧线程并进入追问循环（`btw>` 提示符区别于主 `pigo(model)>`）；**裸 `/btw`** 则重开本进程最近一次侧线程、回放其问答历史（`internal/cli/btw/btw.go` 的 `RunBtw`）。

**`/goal`**（autonomous mode，对应 pi-goal）：给一个高层目标，pigo 就**自主接力**跑——从循环的 follow-up 接缝处一轮轮续跑，直到模型自己宣布 `goal_complete`、报真死路 `goal_blocked`，或被安全护栏拦下。护栏有两个硬上限（`internal/cli/goal/goal.go`）：`goalMaxAutomaticTurns = 25`（单次 `/goal` 最多自主续跑 25 轮）和 `goalMaxNoProgress = 3`（连续 3 轮无工具调用就暂停），防止模型空转。`/goal` 还支持 `pause`/`resume`/`clear` 子命令；目标状态只活在会话内存的 `GoalState`（`host.Goal()`）里，重启不持久化。

> **小结｜七块新大陆一张图**：TUI 是 REPL 的全屏替身；Hooks 是把外部命令挂进 9 个运行节点；记忆 + `/dream` 让 Agent 跨会话"记事并定期整理"；Responses 是第三套 Provider 协议；`agent/` 把整台机器变成可嵌入的 Go SDK；远程控制给 Agent 加了局域网网页遥控器；`/btw` `/goal` 则是两类"元交互"。它们都已在 `internal/*`（与顶层 `agent/`）落地，正文前 10 章的拆解方法——顺着一份装配结果、看它怎么被挂进循环——对这里每一块同样适用。
