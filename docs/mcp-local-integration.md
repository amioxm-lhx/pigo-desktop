# 本地 MCP 服务接入：实现现状核查

> 目标读者：要判断"这个仓库的 MCP 到底做到哪一步了"的开发者。
> 本文是**代码事实核查**，每条结论都带 `文件:行号`。核查基准提交 `95add8a` 的 MCP 代码（`internal/mcp`、`cmd/pigo-mcp-bridge`、`mcp.rs` 自那以后未再改动）。核查时 HEAD 为 `e1d9dfa`。

---

## 0. 结论

**MCP 已经在两条技术栈上全部落地，不再是规划状态。**

| 位置 | 状态 | 规模 |
|---|---|---|
| pigo CLI 原生客户端 `internal/mcp/` | **已实现并接线** | 2683 行 / 12 文件 |
| 插件桥 `cmd/pigo-mcp-bridge/` | **已实现**（方案 A，保留） | 1408 行 / 8 文件 |
| 桌面端 `mcp.rs` + `mcp_servers.rs` | **已实现，原 3 个阻塞 gap 已修** | 1533 + 668 行 |
| 副作用确认门 `SideEffectDeclarer` | **已补齐** | 见 §3 |

关键提交：`0afe07a feat(mcp): native MCP client for CLI and desktop runtime gap fixes`。

> 本文早前版本的结论（"pigo CLI 完全没有 MCP 客户端"、"确认门是硬编码工具名会漏掉 MCP"）**已被上游代码证伪并修复**。下面是逐项复核。

---

## 1. pigo CLI 原生客户端：`internal/mcp/`

### 1.1 文件与职责

| 文件 | 行数 | 职责 |
|---|---|---|
| `client.go` | 324 | 握手、`tools/list`（分页）、`tools/call`、`resources/list`、`resources/read` |
| `config.go` | 271 | `mcp.toml` 解析、server 定义、**env 白名单** |
| `registry.go` | 235 | 多 server 聚合、分层合并、信任门、连接生命周期 |
| `schema.go` | 100 | JSON Schema 净化（注册前） |
| `tool.go` | 156 | `*mcp.Tool` 实现 `agentcore.AgentTool`、命名空间、结果映射 |
| `transport_http.go` | 326 | Streamable HTTP + SSE 响应解析 |
| 测试 6 个 | 1271 | 覆盖上述全部模块 |

### 1.2 协议覆盖

| 能力 | 实现 | 证据 |
|---|---|---|
| 协议版本 | `2024-11-05`（与桌面端对齐） | `client.go:27` |
| stdio 传输 | 复用 `internal/jsonrpc` | `client.go:114` `Dial` |
| HTTP 传输 | Streamable HTTP + SSE | `transport_http.go:51` `dialHTTP` |
| `tools/list` **分页** | 跟随 `nextCursor`，上限 8 页 | `client.go:186` `listTools` + `:36-38` `maxListPages` |
| `resources/list` | **已实现**（分页） | `client.go:253` `ListResources` |
| `resources/read` | **已实现** | `client.go:283` `ReadResource` |
| `prompts/list` / `prompts/get` | **未实现** | `internal/mcp/*.go` 中 `prompts/` 出现 0 次 |
| 工具命名 | `mcp_<server>_<tool>` | `tool.go:136` `Namespace`、`:141` `safeToken` |

### 1.3 安全边界（均已落地）

| 措施 | 证据 |
|---|---|
| **env 白名单**（不继承父进程，防止泄漏 provider key） | `config.go:210` `envAllowlist` + `:227` `BuildEnv`；白名单为 `PATH/HOME/TMPDIR/TEMP/TMP/LANG/LC_ALL/SystemRoot/WINDIR/ComSpec/USERPROFILE` |
| **项目级配置 fail-closed**（未信任目录不加载 `.pigo/mcp.toml`） | `registry.go:125` `Resolve(..., trusted bool, ...)`；接线处 `run.go:186-187` 传 `Trusted(cwd)` |
| **HTTP 明文限制**（`http://` 仅 loopback 或 `allow_insecure`） | `transport_http.go:73` `checkEndpointSecurity`、`:94` `isLoopbackHost` |
| **命令行路径穿越拒绝** | `config.go:101` `containsTraversal` |
| **Windows `npx` → `npx.cmd` → `cmd /c`** | `client.go:306-307` `resolveCommand`（`exec.LookPath` + 扩展名判定） |
| **Schema 净化**（剥 `$schema`、拒外部 `$ref`、非 object 丢弃） | `schema.go:34` `SanitizeSchema`、`:89` `externalRef` |
| **每 server 工具数上限 64** | `client.go:41` `MaxToolsPerServer`（超出截断并告警） |
| **SSE 单行上限 16MB** | `transport_http.go:32` `sseLineLimit` |

### 1.4 接线点（全部完成）

| 接缝 | 证据 |
|---|---|
| `Env` 增加 `MCP *mcp.Manager` | `run.go:69` |
| 连接发生在 `ValidateToolPolicy` **之前** | `run.go:186-191`（注释 `:177-184` 明确说明顺序理由） |
| 生命周期 `defer Close()` | `main.go:505-506` |
| `FileConfig` 增加 `[mcp]` 表 | `config.go:78`、`:146` `MCPConfig` |
| `--no-mcp` | `main.go:256` |
| `--mcp-config` | `main.go:257` |
| 分层解析 | `main.go:608` `mcpSettings()` |

---

## 2. 插件桥 `cmd/pigo-mcp-bridge/`

方案 A 仍在仓库中，未被原生实现移除：

- `main.go` 头部注释明确标注 "docs/mcp-local-integration.md §2, 方案 A"；
- 定位为**插件**，走插件 JSON-RPC 协议，内部再作为 MCP 客户端；
- 8 个文件 / 1408 行，含 `config.go`（env 白名单）、`mcpclient.go`、`serve.go` 与 4 个测试文件。

与原生客户端的分工：桥覆盖 stdio，适合不想升级 pigo 的用户；原生客户端覆盖 stdio + HTTP 且能做信任门与策略过滤。**两者共用同一份 `mcp.toml` 形状**（`mcp.Server`）。

---

## 3. 副作用确认门：已修复

早前版本指出的缺口（CLI 用硬编码 `SideEffectTools = {bash, write, edit}` 判定，导致 MCP/插件工具绕过确认门）**已经补上**：

| 改动 | 证据 |
|---|---|
| 新增可选接口 `SideEffectDeclarer` | `internal/agentcore/tool.go:43-51` |
| 确认门改为"先问声明、再回落名字表" | `internal/trust/interactive.go:48-53` `IsSideEffect` |
| MCP 工具声明有副作用 | `internal/mcp/tool.go:58` `HasSideEffects() bool { return true }` |
| 插件工具同样声明 | `internal/plugin/plugin.go:182` |
| 单测覆盖"声明优先于名字" | `internal/trust/interactive_test.go:279` `TestIsSideEffect` |

这意味着**任何新接入的工具都无法再靠"改个名字"绕过确认门**，这是本轮修复里最有价值的一条。

---

## 4. 桌面端：原 6 个 gap 的复核

`mcp.rs` 从 1206 行增至 1533 行。逐项复核：

| # | 原 gap | 现状 | 证据 |
|---|---|---|---|
| 1 | 同步读 stdout，server→client 请求被丢、乱序响应无解 | **已修复**：独立读循环 + pending map（oneshot） | `mcp.rs:62` `StdioPending`、`:469` `spawn_stdio_reader`、`:558` 插入 pending、`:867-878` reader task |
| 2 | `tools/list` 无分页 | **已修复**：`nextCursor` + `MAX_LIST_PAGES = 8` | `mcp.rs:26`、`:965` `next_cursor`、`:1046-1080` |
| 3 | Windows 不解析 PATHEXT，`npx` 失败 | **已修复**：`resolve_windows_command` + `locate_on_path_in` | `mcp.rs:973-1039`，测试 `:1300-1329` |
| 4 | `Arc<Mutex<Connection>>` 单连接串行化 | **未改**（设计如此） | `mcp.rs:41` |
| 5 | 无 `resources` / `prompts` | **仍未支持** | `mcp.rs` 无 `resources/list` 实现 |
| 6 | 用户自建 server 继承全部父进程 env | **仍未改**：`env_clear()` 仅在 `pluginId` 存在时执行 | `mcp.rs:424-425` |

第 4 条不是缺陷：单连接串行化是刻意的（一个 server 一个子进程，协议本身要求请求-响应配对），并发收益在 server 侧而非客户端。

第 5、6 条是**真实剩余缺口**，且 CLI 与桌面端在此出现能力倒挂：
- CLI 有 `resources/list` + `resources/read`，桌面端没有；
- CLI 对所有 server 强制 env 白名单，桌面端只对插件声明的 server 做。

---

## 5. 验证结果（实跑）

```
go test ./internal/mcp/...          ok  github.com/smallnest/pigo/internal/mcp        0.768s
go test ./cmd/pigo-mcp-bridge/...   ok  github.com/smallnest/pigo/cmd/pigo-mcp-bridge 0.385s
go test ./internal/cli/run/ -run MCP
    --- PASS: TestSetupEnvMCPDisabledByFlags
    --- PASS: TestSetupEnvMCPUntrustedProjectLayerSkipped
```

测试锚点齐全：`client_test.go`（握手/分页/超时）、`config_test.go`（env 白名单/路径穿越）、`schema_test.go`（净化）、`registry_test.go`（分层合并 + 信任门 + `HasSideEffects` 断言 `:95`）、`transport_http_test.go`（HTTP/SSE/明文边界）、`smoke_exec_test.go`（端到端真子进程）。

---

## 6. 剩余工作

按价值排序：

1. **桌面端 `resources` 支持**（gap 5）。CLI 已实现可作为移植参照（`internal/mcp/client.go:253-300`）。当前桌面端 MCP 只能调工具，读不了资源。
2. **桌面端 env 白名单统一**（gap 6）。把 `mcp.rs:424` 的 `if let Some(plugin_id)` 条件去掉，对所有 server 一律 `env_clear()` + 白名单。CLI 的 `config.go:210-222` 是现成的白名单定义。
3. **`prompts/*` 协议族**（两侧都缺）。优先级最低——prompts 是给 UI 做命令面板用的，agent 循环用不到。
4. **行为对齐复核**：CLI 与桌面端现在各写了一套 MCP 客户端（Go / Rust），协议版本已对齐 `2024-11-05`，但工具上限、超时、分页上限需逐项对表，避免同一份 `mcp.toml` 在两端表现不一致。

---

## 7. 一句话总结

**接入已完成，不用再规划**：CLI 原生客户端 2683 行 + 插件桥 1408 行 + 桌面端双文件 2201 行，确认门缺口已补，实跑测试全绿。真正剩下的只有桌面端的 `resources` 支持与 env 白名单统一。
