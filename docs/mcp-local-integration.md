# 本地 MCP 服务接入分析

> 目标读者：要在这个仓库里落地"接入本地 MCP 服务"的开发者。
> 结论先行，后面给证据（文件:行号）与可执行的改动清单。

---

## 0. 先说结论

这个仓库其实是**两条技术栈**：Go 版 `pigo` CLI（根目录 `cmd/` `internal/`）和桌面端 `desktop/`（Tauri 2 + Rust `host-core` + React）。**两者对 MCP 的支持完全不对等**：

| 目标 | 推荐路径 | 代码量 | 说明 |
|---|---|---|---|
| 桌面端用本地 MCP | 写 `.agents/servers/<id>.json`，或插件 `contributes.mcpServers` | **0 行** | 已经是完成态能力 |
| pigo CLI 用本地 MCP | 方案 A：写一个「pigo 插件 → MCP」的桥接器 | ~300 行，**零核心改动** | 1~2 天可用，但只覆盖 stdio |
| pigo CLI 用本地 MCP | 方案 B：`internal/mcp` 原生客户端 | ~1200 行 + 测试 | 推荐终态，覆盖 stdio + HTTP |

关键事实：**pigo 核心目前完全没有 MCP 客户端**。全仓 `internal/` 下 `mcp` 只出现在 3 处注释里（`internal/jsonrpc/transport.go:110`、`internal/cli/run/toolpolicy.go:104`、`internal/jsonrpc/message.go:2`），没有任何实现文件。`README.md:707` 出现的 `pigo install npm:pi-mcp-adapter` 只是在演示包管理器语法，不是内建能力。

真正的接入点已经具备：`internal/jsonrpc` 是一个成熟的子进程 JSON-RPC 2.0 客户端（带 pending map、16MB 行上限、5s 优雅关闭），`internal/agenttool.ToolRegistry` 已经能做 JSON Schema 编译与字段级校验。**MCP 就是 JSON-RPC 2.0 + 三个方法**，所以缺的只是"协议翻译"，不是"传输能力"。

### 0.1 必要性判断：要补，但不是"最高优先级"

**结论：有必要补。近期先在 Windows 上补一个能用的最小闭环（方案 A），原生实现（方案 B）作为终态；但它不该插队到 harness 能力矩阵的 P0 前面。**

支持补的四条证据，全部来自这个仓库自己，不是外部推测：

1. **这是仓库已经承诺、但从未交付的能力**：
   - `tasks/prd-pigo-agent.md:28` 把 MCP 写进对标路线图；`:290-299` 是"阶段五 / US-025 MCP 客户端支持"，验收标准写明 stdio 传输、工具注册进 Registry、走三段式执行路径、要有最小 MCP server 的集成测试。
   - `tasks/prd-pigo-parity-features.md:10` 说得更直接：MCP 客户端是"pigo 为兼容 Claude Code 开放能力而新增"；`:156-175`（US-014/US-015）给了逐条验收标准；`:270` 是产品级验收标准"**MCP server 工具在 pigo 中可被发现与调用**"。
   - `CHANGELOG.md` 里 MCP 零条目 —— 承诺在，交付不在。
2. **两条技术栈的能力已经漂移**。桌面端 668 行存储层（`mcp_servers.rs`）+ 1206 行运行时（`mcp.rs`）+ 4 篇 ADR；CLI 端 0 行。同一个产品，"桌面能用 MCP、命令行不能用"是明确的体验裂缝。
3. **"靠插件生态兜住 MCP"这条路在 Windows 上是断的**。`internal/pkgmgr/distribute.go:40` 直接返回 `extension install is not supported on windows yet`，也就是说 `pigo install npm:pi-mcp-adapter` 这条官方门路在 Windows 上装不上。而本仓库的开发环境就是 Windows。
4. **接入成本比看上去低**。`internal/jsonrpc` 已经是成熟的子进程 JSON-RPC 2.0 客户端（pending map `transport.go:139-149`、16MB 行上限 `:111`、`failAll`）；插件系统已经把"发现 → 注册 → 三段式执行 → 崩溃隔离"整条链跑通。MCP 是同一形状的第二个实例，缺的是协议翻译，不是基础设施。

### 0.2 真正的代价不在"写客户端"，在一个安全前置项

**这是本轮勘察最重要的发现，也是原方案 B 漏掉的一点。**

CLI 的副作用确认门是**硬编码的三个工具名**，不是按风险等级判定：

- `internal/trust/interactive.go:36-40`：`SideEffectTools = {bash, write, edit}`；
- `:215`：`BeforeToolCall` 第一句就是 `if !SideEffectTools[call.Name] { return nil }`；
- `internal/agentcore/tool.go:32-41`：`AgentTool` 接口只有 `Name/Description/Schema/ExecutionMode/Execute`，**没有任何"风险等级"或"需要确认"的扩展点**。

推论两条，都很要紧：

1. **MCP 工具不会自动受确认门约束。**"风险标 `medium` 就触发确认"在 CLI 侧不成立 —— 全仓除 thinking level 外没有任何地方消费 `"medium"`。
2. **今天已有的插件工具同样绕过确认门。** 这不是 MCP 引入的新问题，但补 MCP 会把它从"少量受信插件"放大到"任意第三方 MCP server"。

所以方案 B 必须多出**第 5 个改动点**：把 `SideEffectTools` 从硬编码表改成"工具可声明副作用"，并让 MCP 工具默认声明为有副作用。这是安全前置项，不能排到 P2 之后 —— 详见 §3.2 第 5 项与 §3.7 第 3 条。

### 0.3 什么情况下可以不做

- 目标用户以 Linux/macOS 为主，且开发环境不是 Windows —— 插件桥（方案 A，300 行、只覆盖 stdio）就够，不必做原生客户端；
- 更在意 agent 本身变强 —— `docs/harness-capability-matrix.md:28` 把上下文管理标为 **P0**，`:34` 的安全护栏标为 **P2**。MCP 属于"生态兼容"，提升的是可接入的工具数量，不是 agent 的推理/执行质量，排在它们之后是合理的。

**一句话：该补，但要以"先修确认门、再上 MCP"的顺序补。**

---

## 1. 现状盘点

### 1.1 pigo CLI（Go）：有插件系统，无 MCP

插件协议（`internal/plugin/manifest.go:7-22` 的包注释已经写明）：

| 方法 | 方向 | 载荷 |
|---|---|---|
| `initialize` | 请求 | → `Manifest{name, version, tools[], commands[], events[]}` |
| `tools/call` | 请求 | `{name, arguments}` → `{content, isError}` |
| `commands/call` | 请求 | `{name, arguments}` → `{prompt, notifications}` |
| `event` | 通知 | pigo → 插件，生命周期事件 |
| `shutdown` | 通知 | 关闭前尽力投递 |

发现规则非常简单粗暴（`internal/plugin/manager.go:29-60`）：扫描目录里**每一个可执行文件**，逐个拉起、握手、聚合工具。目录来自 `$PIGO_HOME/plugins` 或 `~/.pigo/plugins`（`internal/cli/run/run.go:495-505`）。加载是容错的：单个插件失败只打日志跳过。

工具注册链路：`SetupEnv`（`internal/cli/run/run.go:77`）→ `plugin.Discover`（:156）→ 追加到 `tools` → `ValidateToolPolicy`（:170）→ `ApplyToolPolicy`（:173）→ `ToolRegistry`（:329）。

**注意一个坑**：`ToolRegistry`（`run.go:329-335`）注册失败是**静默忽略**的（`_ = reg.Register(t)`）。如果 MCP server 返回的 schema 让 `compileSchema` 失败（`internal/agenttool/registry.go:150`），那个工具会无声消失，只有打开 `--debug` 才可能察觉。做 MCP 桥时这点必须改成显式告警。

### 1.2 桌面端（Rust + Tauri）：MCP 已完整实现

| 层 | 文件 | 职责 |
|---|---|---|
| 存储/校验 | `desktop/crates/host-core/src/mcp_servers.rs`（668 行） | `.agents/servers/<id>.json` 读写、global/project 两级、id/env/header/url 校验、最多 128 个 |
| 客户端 | `desktop/apps/desktop-tauri/src-tauri/src/mcp.rs`（1206 行） | stdio + Streamable HTTP、懒连接、`initialize`/`tools/list`/`tools/call`、工具命名空间 |
| 权限门 | `desktop/crates/host-core/src/tools/mod.rs:962` | `is_desktop_dispatched()`：`mcp_` 前缀与 `plugin_` 同等待遇，不自己执行，转发到 `plugins.execute` |
| sidecar 透传 | `desktop/sidecar/internal/runtime/tools.go:87,132` | 把 `mcp_*` 工具当 host 工具代理回桌面 |
| 市场/导入/编辑器 | `desktop/packages/shared/src/mcp-{catalog,catalog-builtin,import,registry}.ts` + `apps/desktop-tauri/src/components/**` | 模板市场、粘贴导入、编辑器表单 |

命名规则（`mcp.rs:819-837`）：

```
mcp_<safe(server_id)>_<safe(tool_name)>     // 非 [A-Za-z0-9_] 一律换成 _
```

风险固定为 `medium`、`planSafeActions: []`（`mcp.rs:135-136`），依据 ADR 0038 决策 5：第三方自报的风险等级不能作为门控依据。这个原则在 CLI 侧也必须照抄。

### 1.3 协议差异对齐表（这是做桥的依据）

| 维度 | pigo plugin 协议 | MCP |
|---|---|---|
| 握手 | `initialize` → `Manifest` | `initialize` → `{protocolVersion, capabilities, serverInfo}`，随后客户端发 `notifications/initialized` |
| 版本协商 | 无 | 有（桌面端钉 `2024-11-05`，`mcp.rs:26`） |
| 工具发现 | Manifest 内嵌 `tools[]` | `tools/list`，**可分页**（cursor） |
| 调用 | `tools/call {name, arguments}` | `tools/call {name, arguments}` — **同形** |
| 结果 | `{content: string, isError}` | `{content: [{type,text|data,mimeType}], isError, structuredContent}` |
| 传输 | stdio NDJSON | stdio NDJSON / Streamable HTTP(+SSE) |
| 额外能力 | 无 | resources、prompts、sampling、elicitation、progress、list_changed |

`tools/call` 参数形状完全一致，`initialize` 和工具列表形状不同。**翻译层只需要处理这两处**，其余是映射与裁剪。

---

## 2. 方案 A：零核心改动（插件桥，最快可用）

### 2.1 原理

`manager.go:29` 的发现规则是"目录内任意可执行文件即插件"。所以把 **"pigo 插件协议 ↔ MCP 协议"** 的翻译做成一个可执行文件，塞进 `~/.pigo/plugins/`，pigo 就会把它当普通插件加载，而它在内部去连真正的本地 MCP server。

```
pigo  ──stdio JSON-RPC(plugin)──▶  pigo-mcp-bridge  ──stdio JSON-RPC(MCP)──▶  本地 MCP server
                                  （读 ~/.pigo/mcp.toml，
                                    为每个 server 起子进程）
```

### 2.2 实现骨架（Go，约 300 行）

```go
// cmd/pigo-mcp-bridge/main.go
func main() {
    cfg := loadConfig("~/.pigo/mcp.toml")   // 也可用 PIGO_MCP_CONFIG 覆盖
    servers := connectAll(cfg)              // 每台 lazy：握手失败只记日志，不阻塞

    // 1) 应答握手：把各 server 的 tools 聚合成 Manifest.Tools
    //    名字统一加 mcp_<server>_<tool> 前缀，避免与内建工具撞名
    // 2) tools/call 转发到对应 server 的 tools/call，做结果映射
    // 3) shutdown 时关闭所有子进程
    serve(map[string]Handler{
        "initialize":  func(json.RawMessage) any { return buildManifest(servers) },
        "tools/call":  func(p json.RawMessage) any { return forward(p) },
        "shutdown":    func(json.RawMessage) any { closeAll(servers); return nil },
    })
}
```

要点：

- 直接复用 `internal/jsonrpc`（`plugin.Load` 的对侧就是它，`internal/plugin/plugin.go:40-68` 可当参考）。
- MCP 结果 `content[].text` 需要拼成一个字符串，因为插件协议的 `CallResult.Content` 是 `string`（`manifest.go:71-74`）。多块内容用分隔拼接；`image` 块降级成 `[image: mimeType, N bytes]` 文本（插件协议无法回传二进制）。
- `isError` 直接透传到 `CallResult.IsError`。
- 工具数 >64 时要截断并打日志，否则会淹没上下文。
- **env 必须显式构造白名单**（不能省）：`jsonrpc.Config.Env` 为 nil 时子进程继承整个父进程环境（`transport.go:60-62`）。桥里要自己拼 `PATH` + `TEMP`/`TMPDIR` + 配置里声明的 `env`，绝不能把 `OPENAI_API_KEY` 一类密钥带给第三方 server。
- 失败一律降级为工具错误返回，不要让桥进程自己 panic —— 桥一挂，pigo 看到的就是"所有 MCP 工具消失"。

### 2.3 配置

`~/.pigo/mcp.toml`：

```toml
[[server]]
id = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "D:/notes"]
env = { NODE_NO_WARNINGS = "1" }   # 显式声明才传给子进程；未声明的父进程变量一律不继承

[[server]]
id = "sqlite"
command = "uvx"
args = ["mcp-server-sqlite", "--db", "D:/data/app.db"]
```

安装：

```powershell
go build -o "$env:USERPROFILE\.pigo\plugins\pigo-mcp-bridge.exe" ./cmd/pigo-mcp-bridge
```

### 2.4 优点与硬限制

优点：不改任何核心代码，不引入回归风险；天然获得插件的事件订阅、故障隔离（一个 server 挂了只让那个工具报错，`plugin.go:181-195`）；`--no-tools` 自动关掉。

限制（必须提前告知使用者）：

1. **只覆盖 stdio**。远程 HTTP MCP 需要桥自己实现 Streamable HTTP，工作量翻倍——那时不如直接做方案 B。
2. **启动即全量握手**。`Discover` 没有懒加载（`manager.go:41-58`），pigo 每次启动都会拉起所有桥接的 MCP server。`npx` 冷启动 1~3 秒，会让 CLI 启动明显变慢。
3. **Windows 上 `npx` 需要 `.cmd`**。`jsonrpc.NewClient` 用 `exec.Command`（`transport.go:76`），Windows 不解析 PATHEXT，`npx` 不可执行；要写 `npx.cmd` 或 `cmd /c npx`。这是本地 MCP 在 Windows 上最常见的失败点。
4. **环境继承**。`jsonrpc.Config.Env` 为 nil 时子进程继承父进程环境（`transport.go:60-62`）。这意味着桥会把 `OPENAI_API_KEY` 之类泄漏给第三方 MCP server。**必须在桥里显式构造白名单 env**。
5. **schema 不合法会静默丢工具**（见 §1.1 的坑）。
6. **确认门不认它**。插件工具的名字不在 `SideEffectTools` 表里（`internal/trust/interactive.go:36-40`），所以桥接出来的 MCP 工具在未信任目录下**不会触发逐次确认**，`--approve` 与否行为一样。这是方案 A 的真实安全代价，不是"以后再说"的细节。

---

## 3. 方案 B：原生接入 pigo 核心（推荐终态）

### 3.1 模块切分

新增 `internal/mcp/`，每一层都能在现有代码里找到参照物：

| 新文件 | 职责 | 参照物 |
|---|---|---|
| `client.go` | 握手（`initialize` + `notifications/initialized`）、`tools/list`、`tools/call`、`resources/list` + `resources/read`（最小实现，US-015 验收项） | `internal/plugin/plugin.go` |
| `tool.go` | `*mcp.Tool` 实现 `agentcore.AgentTool` | `pluginTool`（`plugin.go:150-196`） |
| `transport_http.go` | Streamable HTTP + SSE 响应解析 | 无（`internal/provider/transport.go` 可参考 HTTP 习惯） |
| `registry.go` | 多 server 聚合、命名空间、每 server 工具上限、去重 | `internal/plugin/manager.go` |
| `config.go` | `mcp.toml` 解析、env 展开、信任门、路径校验 | `internal/cli/config/config.go` |

`agentcore.AgentTool` 的契约很小（`internal/agentcore/tool.go:32-41`）：`Name/Description/Schema/ExecutionMode/Execute`。MCP 工具实现它没有障碍。

### 3.2 与现有接缝的五处改动（精确到函数）

| # | 位置 | 改动 | 为什么在这里 |
|---|---|---|---|
| 1 | `internal/cli/config/config.go` `FileConfig`(:30) | 增一个 `MCP` 字段（TOML 表 `[mcp]`） | 优先级只有三层：`command-line flag > config.toml > built-in default`（`config.go:7`），照此叠加即可。注意 `config.go` 本身刻意**不**做 overlay（`config.go:12-15`：只负责解码，叠加逻辑在 `cmd/pigo`），所以字段加在这里、接线写在那里 |
| 2 | `internal/cli/run/run.go` `SetupEnv`，紧跟 `plugin.Discover`（:156-162）之后 | `if !noTools { tools = append(tools, mcpTools...) }` | **必须在 `ValidateToolPolicy`(:170) 之前**。该处注释已经解释了这个顺序约束：工具名只有运行时才存在 |
| 3 | `Env` 结构（`run.go:43-44` 已有 `Plugins *plugin.Manager`） | 增 `MCP *mcp.Manager`，run 结束时 `Close()` | 生命周期对齐，避免子进程泄漏 |
| 4 | `cmd/pigo/main.go` | 增 `--mcp-config <path>`、`--no-mcp`，风格对齐 `--no-skills`/`--no-tools` | CLI 可控性；也是 CI 里关掉 MCP 的开关 |
| 5 | `internal/trust/interactive.go` `SideEffectTools`(:36-40) + `BeforeToolCall`(:215) | 从"硬编码三个名字"改为"工具可声明副作用"；MCP 工具默认声明为有副作用 | **安全前置项，不是可选项。** 见下 |

第 5 项的理由，需要展开说清楚：

现在的确认门第一句是 `if !SideEffectTools[call.Name] { return nil }`（`internal/trust/interactive.go:215`），而 `SideEffectTools` 只有 `bash`/`write`/`edit` 三个键（`:36-40`）。`agentcore.AgentTool` 接口（`internal/agentcore/tool.go:32-41`）也没有任何"风险等级"或"需要确认"的方法。所以：

- **改名换姓的新工具天然不受门控。** 这意味着今天已加载的插件工具就绕过了确认门（`pluginTool.Name()` 直接返回 `t.spec.Name`，`plugin.go:158`），这是既有问题；
- **接 MCP 会把它放大。** 从"用户自己写的插件"扩到"从网上随便下的第三方 MCP server"，而 MCP server 拿到的正是 agent 的完整工具上下文。

最小改法（**P1 第一天就要做**，不要留到 P2）：给 `AgentTool` 加一个可选接口而非改主接口，避免动所有既有实现：

```go
// SideEffectDeclarer 是可选接口：声明自己有副作用的工具在未信任目录下必须确认。
// 不实现它的工具沿用 SideEffectTools 名字表（保持向后兼容）。
type SideEffectDeclarer interface{ HasSideEffects() bool }
```

`BeforeToolCall` 判定的顺序改为：先看 `HasSideEffects()`（若实现），再回落到名字表。MCP 工具与插件工具都实现它并返回 `true`。

`toolpolicy.go` **不需要改**：`ValidateToolPolicy` 已经接受运行时才存在的工具名（`run.go:163-169` 的注释说明得很清楚：`--allowed-tools` 的校验必须发生在注册层之前，否则会误拒合法名字），MCP 工具名会一并被 Allow/Deny 过滤。

### 3.3 工具命名

与桌面端保持一致（`mcp.rs:819`），让两条栈的产品行为统一：

```
mcp_<safe(serverId)>_<safe(toolName)>
safe(s) = s 中非 [A-Za-z0-9_] 的字符替换为 '_'，空则 "unnamed"
```

冲突策略：同名先到先得，后者告警丢弃（不要像 `ToolRegistry.Register` 那样静默）。

**与 PRD 的显式偏离**：`tasks/prd-pigo-parity-features.md` 原文（US-014 验收 与 Success Metrics）要求 Claude Code 风格的双下划线命名 `mcp__server__tool`。本文选择单下划线 `mcp_<server>_<tool>`，理由：桌面端已以此命名发布（`mcp.rs:819-837`），改名即对已发布软件的破坏性变更；CLI 与桌面端统一的价值高于与 Claude Code 字面一致（工具名只是内部注册名，不构成线上的协议兼容面）。PRD 两处已于 2026-11-11 同步修订为桌面端命名。

### 3.4 Schema 净化（必做）

MCP 的 `inputSchema` 是 JSON Schema 子集，但不是全部能被 `santhosh-tekuri/jsonschema/v6` 离线编译通过。注册前必须：

1. 空 / `null` → `{"type":"object","properties":{}}`（对齐 `plugin.go:165-170` 的降级策略：那里退化成 `{"type":"object"}`，三者都该被 `compileSchema` 接受）
2. 移除 `$schema`：不同 server 声明的 draft 可能与本库默认不一致。
3. 拒绝任何指向外部的 `$ref`（`http(s)://`、`file://`）：既是离线要求，也是 SSRF 面。原地失败并告警，不要让 `compileSchema` 抛错后被静默吞掉。
4. `"additionalProperties"` 缺失时**不要**补 `false`——第三方 server 常在参数上放宽，收紧会误伤。
5. 顶层必须是 object；不是则丢弃并告警。

### 3.5 结果映射

| MCP content block | `agentcore` 映射 |
|---|---|
| `{type:"text", text}` | `NewTextContent(text)`，多块按顺序拼接 |
| `{type:"image", data, mimeType}` | `NewImageContent(data, mimeType)`；模型不支持图片时降级为文本占位（照 `desktop/sidecar/internal/runtime/tools.go:305-319` 的 `supportsImages` 判断） |
| `{type:"resource", resource:{text/blob}}` | 文本折叠为 `NewTextContent`，blob 折叠为 `[binary resource …]` |
| `structuredContent` | 放 `AgentToolResult.Details` |
| `isError: true` | `Details = {"isError": true}`（对齐 `plugin.go:192-194`），**不要**返回 Go error |

执行模式：`ToolExecutionSequential`。理由与 `plugin.go:172-176` 相同——跨进程调用、副作用未知。

### 3.6 超时与预算（与桌面端对齐，避免行为漂移）

| 项 | 建议值 | 备注 |
|---|---|---|
| `initialize` | 10s | 与 `plugin.go:22` 一致（桌面端是 20s，`mcp.rs:21`；CLI 侧取短值更符合交互体感） |
| `tools/call` | 60s | 桌面端 60s（`mcp.rs:22`）；pigo 侧受 `Bash` 类似的工具超时机制约束 |
| 单行上限 | 16MB | `jsonrpc` 已经是这个值（`transport.go:111`） |
| 每 server 工具数 | 64 | 桌面端 `MAX_TOOLS_PER_SERVER`（`mcp.rs:23`），超出截断并告警 |
| `tools/list` 分页 | 最多 8 页 | ADR 0038 承诺值 |
| server 总数 | 128（每层）/ 单插件 8 | 桌面端 `MAX_SERVERS`（`mcp_servers.rs:12`，global 与 project 各自独立计数，`mcp_servers.rs:447`）/ `MAX_PLUGIN_SERVERS`（`mcp.rs:25`，限制**单个插件**能声明几个 server）；另有 `MAX_ARGS 64`/`MAX_ENV_ENTRIES 64`/`MAX_HEADERS 32`/`MAX_VALUE_BYTES 4096`（`mcp_servers.rs:13-16`）值得照抄 |

**协议版本**：`initialize` 里声明 `2025-06-18`，并接受服务端回落到 `2025-03-26`/`2024-11-05`。桌面端钉死在 `2024-11-05`（`mcp.rs:26`），如果做统一，建议以桌面端为准先对齐 `2024-11-05`，减少兼容面。

### 3.7 安全模型（评审重点，直接照抄现有原则）

1. **项目级配置 fail-closed**。`<project>/mcp.toml` 只有在目录被信任（`run.Trusted(cwd)`，`run.go:604`）时才加载。这与 hooks 的 `ResolveHookSet`（`run.go:570-581`）完全同构——克隆一个仓库不该等于执行任意命令。
2. **不继承父进程 env**。只传 `PATH`、`TEMP`/`TMPDIR`、locale 变量，加上配置里显式声明的 `env`。依据：桌面端插件桥就是这么做的（ADR 0038 决策 6）；`jsonrpc.Config.Env == nil` 的默认继承行为（`transport.go:60-62`）在这里是危险的，必须显式填。
3. **风险一律 `medium`，但这不足以触发确认门 —— 必须配合 §3.2 第 5 项。** 桌面端的风险分级（`mcp.rs:135-136` 固定 `medium`、`planSafeActions: []`）是它自己的模型；CLI 侧的确认门只看 `internal/trust/interactive.go:36-40` 那三个硬编码名字，**不看风险等级**。所以只把 MCP 工具标成 `medium` 是不够的，`HasSideEffects()` 那条可选接口是前置条件。
4. **远程 HTTP**。默认只允许 `https://`；`http://` 仅当 host 是 loopback（127.0.0.1/::1/localhost）或显式配置 `allow_insecure = true` 才放行。参照 ADR 0142（`desktop/docs/adr/0142-allow-non-loopback-http-mcp.md`）的取舍，但 CLI 侧默认更严。
5. **命令校验**。`command` 必须是 PATH 上的裸名或配置目录内的相对路径，禁止 `..`（对齐 `desktop/packages/plugin-sdk/src/mcp-config.ts` 的 `checkRelativePath`）。
6. **审计**。server id / tool name / 耗时进 telemetry（`internal/runtime/telemetry.go` 已有 turn/tool 统计）。

### 3.8 推荐配置格式

```toml
# ~/.config/pigo/config.toml  （全局层）
[mcp]
enabled            = true
init_timeout       = "10s"
call_timeout       = "60s"
max_tools_per_server = 64

[[mcp.servers]]
id        = "filesystem"
transport = "stdio"
command   = "npx"
args      = ["-y", "@modelcontextprotocol/server-filesystem", "D:/notes"]
env       = { NODE_NO_WARNINGS = "1" }
enabled   = true

[[mcp.servers]]
id        = "local-sqlite"
transport = "http"
url       = "http://127.0.0.1:8765/mcp"   # loopback http 允许
enabled   = true
```

`<project>/.pigo/mcp.toml` 用同样形状，只在目录被信任时合并。桌面端的项目级判定可作参照：`CapabilityLevel::Project` 且未给 projectPath 时直接报错（`mcp_servers.rs:160-162`）。

### 3.9 其他运行时交互

- `--no-tools`：跳过 MCP 发现（与插件同处一个 `if !noTools` 分支）。
- 子 Agent：`BuiltinToolsExcept`（`run.go:295`）会把工具集复制给子 agent。MCP 工具默认**继承**（它们已经被策略过滤过），但要在文档里说清——子 agent 不应绕过父 agent 的工具边界。
- `goal` 模式：`run.go:275-282` 单独建 registry，MCP 工具需要显式传入，否则 `/goal` 下看不到。

---

## 4. 桌面端：已可用，但有 6 个会挡住"本地"场景的 gap

如果要接入的目标是桌面端，先把这些修掉，否则"本地 MCP"会以各种古怪方式失败。

| # | 问题 | 证据 | 影响 |
|---|---|---|---|
| 1 | **非目标 id 的消息被"丢弃"** | `mcp.rs:536-554`：`request()` 里写 stdin 后同步循环读 stdout，id 不等于本次请求就 `continue` | 因为是在同一个 `loop` 里丢弃，**不会**造成缓冲区堆积（这点别写进 issue）。真正的故障是三个：① 服务端→客户端的请求（`sampling/createMessage`、`roots/list`、`elicitation/create`）被当成"不是我的回包"丢掉，服务端只能等到自己的超时；② `notifications/cancelled`/`progress` 全部忽略，`CALL_TIMEOUT 60s` 到点后调用方返回超时，但子进程里的活儿还在跑，下一次 `tools/call` 会排在它后面；③ 丢弃时**没有任何日志**，排查只能靠抓子进程 stderr——而 stderr 已经是 `Stdio::null()`（`mcp.rs:405`）。修法：独立读循环 + pending map + server→client 请求处理器。注意 `internal/jsonrpc` 的 `readLoop`（`transport.go:107-136`）在 119-127 行做的是**同样的事**（解析失败的跳过、`ID == nil` 的跳过），只能抄 pending map/deliver 的结构（139-149），**不能整段照抄** |
| 2 | **`tools/list` 无分页** | `mcp.rs:323` 的 `list_tools` 只发一次请求；全仓（`mcp.rs` + `mcp_servers.rs`）搜不到 `cursor`/`nextCursor` | 工具数超过单页上限的 server 被静默截断（`MAX_TOOLS_PER_SERVER 64` 之上还有服务端自己的分页），用户看不到完整工具集 |
| 3 | **Windows 无法启动 `npx`/`uvx`** | `mcp.rs` 用 `tokio::process::Command::new(command)` | Windows 不解析 PATHEXT，`npx` 实为 `npx.cmd`。这是本地 MCP 在 Windows 上最高频的失败点，需显式走 `cmd /c` 或解析 `.cmd`/`.bat`/`.exe` |
| 4 | **单连接串行化** | `connections` 是 `Mutex<HashMap<String, Arc<Mutex<Connection>>>>`（`mcp.rs:38`） | 同一 server 的并发工具调用排队，模型一次发多个 MCP 调用时线性等待。这层锁是**设计使然**：`request()` 取 `&mut self`，请求与响应靠同一条 stdout 顺序配对，要去掉锁就得先改成读写分离 |
| 5 | **无 `resources`/`prompts`** | 全仓无 `resources/list` | 知识库型本地 server（filesystem/sqlite）只能暴露工具面，能力浪费 |
| 6 | **用户自建 server 继承全部父进程环境** | `mcp.rs:410` 的 `env_clear()` 只在 `record.pluginId` 存在时执行 | 插件声明的 server 会清空环境（ADR 0038 决策 6），但**用户自己在 MCP 编辑器里加的 server 不会**——`OPENAI_API_KEY`、`GITHUB_TOKEN` 等直接传给第三方本地二进制。这是刻意的取舍（`tools/mod.rs:959-961`：用户亲手输入命令，至少和信任插件清单一样慎重），但风险不对称且易误判，建议统一为白名单 + 显式 `env` |
前 3 条影响"能不能用"，4/5 影响"好不好用"，第 6 条是安全边界。补充：`MCPServers` 的 `Connection` 枚举（`mcp.rs:51-54`）**stdio 与 HTTP 都已有**，所以"本地 HTTP MCP"不缺传输。

---

## 5. 分期与工作量

| 阶段 | 内容 | 文件 | 预估 | 产出 |
|---|---|---|---|---|
| P0.5 | **安全前置**：`HasSideEffects()` 可选接口，`BeforeToolCall` 消费它；`pluginTool` 实现并返回 `true` | `internal/agentcore/tool.go` + `internal/trust/interactive.go` + `internal/plugin/plugin.go` | 0.5~1 天 | 任何新工具都不再能绕过确认门（含插件与桥接器） |
| P0 | 方案 A 桥接器 + `.pigo/mcp.toml` | `cmd/pigo-mcp-bridge/` 3~4 个文件 | 1~2 天 | CLI 立即能用本地 stdio MCP |
| P1 | 方案 B：`internal/mcp` stdio 闭环（含 `resources/list` + `resources/read` 最小实现，关闭 US-015 验收） | `client.go`/`tool.go`/`registry.go`/`config.go` + `run.go`/`config.go` 接线 | 3~5 天 | `--mcp-config` 可用，命令行可控 |
| P2 | HTTP 传输 + 信任门 + 审计 | `transport_http.go` + trust 集成 + telemetry | 2~3 天 | 远程/本地 HTTP server 覆盖 |
| P3 | 桌面端 gap 1/2/3 | `desktop/apps/desktop-tauri/src-tauri/src/mcp.rs` | 1~2 天 | 本地 server 真正可用且稳定 |

建议顺序：**P0.5 最先做**（半天到一天，一次性堵住确认门缺口，让后面所有 MCP 工作都在安全前提下落地）→ **P0 先发**（解燃眉之急、验证需求）→ **P3 并行**（桌面端已有用户，卡点最痛）→ **P1/P2 作为终态**。P0 与 P1 的 `mcp.toml` 形状保持一致，升级时用户配置不用改。

P0.5 之所以必须排在 P0 之前，而不是"P1 之前"就行：方案 A 的桥接器同样以插件工具身份运行，名字同样不在 `SideEffectTools` 里（§2.4 第 6 条）。如果先发 P0 再补 P0.5，中间那段时间用户就是在无确认门的情况下跑第三方 MCP server —— 而 P0.5 只需要 0.5~1 天，没有理由冒这个窗口。

§3.7 的第 1、2 条（信任门、env 白名单）**也不能等到 P2**：它们同样是安全边界，一旦 MCP 工具被接进主循环，用户就会在无隔离的前提下跑第三方二进制。这两条随各自阶段（P0 的桥要自己做 env 白名单；P1 要做项目级配置的信任门判定）落地，HTTP 传输与审计才留在 P2。

---

## 6. 测试策略

**CLI 侧**（沿用现有 `_test.go` 就近原则）：

- 造一个最小 echo MCP server 作为 `testdata/mcp_echo/main.go`：`initialize` 回固定 manifest，`tools/list` 回 2 个工具，`tools/call` 回固定文本 + 一个 `isError` 分支。既能当 e2e fixture，也能当协议回归基线。
- 单测锚点：
  - `safe_token` 命名规则与冲突告警；
  - schema 净化（`$schema` 剥离、外部 `$ref` 拒绝、非 object 顶层丢弃）；
  - **env 隔离**：断言子进程 env 里不存在 `OPENAI_API_KEY`（这是最容易被回归破坏的安全断言）；
  - 未信任目录下项目级 `mcp.toml` 不加载；
  - `--no-tools` / `--no-mcp` 完全不发起子进程。
- 集成：`pigo --mcp-config fixture.toml -p "列出可用工具"` 走 headless，断言 `--output-format stream-json` 里出现 `tools/list` 派生的工具名。

**桌面端**：已有 `desktop/apps/desktop-tauri/test/mcp-market-panel.test.mjs` 的 `.mjs` 测试范式，gap 1/2/3 的修复各自加一个用例（乱序通知、分页 cursor、Windows 命令解析）。

---

## 7. 一句话建议

**先修确认门、再做方案 A 止血，同时按方案 B 的接口定形。** 冻结不变的约定是：`mcp.toml` 的形状、`mcp_<server>_<tool>` 命名、env 白名单、每 server 64 工具上限——这几条定下就不要动，桥接器与原生客户端共用，后续替换对用户无感。`medium` 风险要**同时**配 `HasSideEffects()`，否则它在 CLI 侧只是个没人读的字段。桌面端则优先修 gap 1 和 3，那是"本地 MCP 不可用"的直接原因。
