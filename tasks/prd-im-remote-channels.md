# PRD: IM 远程控制通道（飞书 / 微信）

> Status: Draft | Author: 红鑫 | Created: 2026-11-12
> Related: `tasks/prd-remote-control.md`、`tasks/spec-remote-control.md`、`internal/remotecontrol`

## 1. 结论先行

三句话概括这份规划的核心判断：

1. **飞书是第一优先级，可以做到「零公网、零加解密、零运维」**。企业自建应用 + WebSocket 长连接模式，只需出站访问公网能力，不需要公网 IP、域名、内网穿透，也不用处理签名验签和 AES 解密。官方 Go SDK（`github.com/larksuite/oapi-sdk-go/v3`）已内置心跳与断线重连。
2. **微信个人号没有官方 API，是「可用但脆弱」的附加通道，不能作为主链路**。2025-10-28 `wxauto` 归档停更，微信 4.0 从 Qt Widgets 迁到 Qt Quick 后 UIAutomation 控件树基本失效（4.1.12+ 完全失效）；`WeChatFerry` 只 hook 3.9.12.x。所有可行路径都违反微信个人号使用规范，存在封号风险与「版本一升级即失效」问题。因此本期把它设计成**可替换的桥接进程**，而不是编进主程序的硬依赖。
3. **通道层与运行载体解耦**。新增 `internal/im` 包，Channel 只依赖 `internal/serve` 已有的 `Service` / `SessionRuntime`（`Snapshot` / `Subscribe` / `Prompt` / `Steer` / `Abort`），CLI 与桌面端共用一套，不重复实现 Agent 循环。

## 2. 背景

现有 `/remote-control` 解决了「同一局域网里用手机浏览器接管 CLI 会话」的问题，但它有三个结构性限制：

| 限制 | 现状 | 后果 |
|------|------|------|
| 必须同网段 | 监听 LAN IP + 二维码配对 | 出门在外用不了 |
| 单客户端 | `Server.client` 单连接 | 多人协作看不了 |
| 载体绑定 | `internal/cli/repl`、`internal/cli/tui` 各接一套 | 桌面端（Tauri + sidecar）完全用不到 |

IM 通道从根上绕开这三条：消息走平台服务器中转，天然跨网；一个机器人可以服务多个聊天；只要把 Channel 抽出来，CLI 和桌面端都只是「谁来托管这个常驻进程」的差别。

## 3. 用户故事

- **US-001** 作为开发者，我在通勤地铁上用手机飞书给机器人发一句「把 build 失败的原因查一下」，5 分钟后拿到结论，不需要开电脑热点。
- **US-002** 作为开发者，我在飞书里收到一条交互卡片：「即将执行 `bash: rm -rf ./dist`，允许吗？」点「允许」后继续，点「拒绝」后 Agent 收到拒绝结果并自行调整。
- **US-003** 作为开发者，每个飞书会话（单聊或群）绑定一个独立的 pigo 会话，互不串台；我用 `/new` 开新会话，用 `/resume <id>` 回到历史会话。
- **US-004** 作为个人用户，我在微信「文件传输助手」里发指令给电脑上的 pigo，电脑把结果发回同一个会话。只接受我自己发出的消息，别人的一律丢弃。
- **US-005** 作为桌面端用户，我在 Settings 里填完飞书 App ID / Secret，点「连接」，看到在线状态；关掉窗口后进程仍在跑（可配置）。
- **US-006** 作为运维者，`pigo im serve --channel feishu` 可以在服务器上裸跑，日志里不出现任何 token / secret 明文。

## 4. 平台能力矩阵

这张表决定了后续所有渲染与交互设计，先钉死：

| 维度 | 飞书（自建应用 + 长连接） | 企业微信（智能机器人 + 长连接） | 微信个人号（桥接） |
|------|--------------------------|--------------------------------|-------------------|
| 公网 IP | 不需要 | 不需要 | 不需要 |
| 消息加解密 | SDK 内置（明文投递） | SDK 内置 | 不涉及 |
| 事件延迟 | 实时推送 | 实时推送 | 轮询，500ms~3s |
| 流式更新同一条消息 | ✅ 支持 PATCH 更新卡片 | ✅ 支持主动推送流式内容 | ❌ 只能追加新消息 |
| 交互按钮 | ✅ 卡片按钮 + 回调 | ✅ 模板卡片 | ❌ 只能文本 y/N |
| 单条消息长度 | 长文本 OK，超长需分片 | 有上限，需分片 | 有上限，需分片 |
| 速率限制 | 有，需节流合并 | 有 | 发送慢（OCR 路径 ~10s/条），风险高 |
| 群聊 @ 触发 | ✅ | ✅ | ❌ 本期不做群 |
| 凭据 | App ID + App Secret | BotID + Secret | 无（依赖本机微信登录态） |
| 合规性 | 官方支持 | 官方支持 | ⚠️ 违反个人号使用规范 |

**本期范围：飞书（P0）+ 微信个人号（P2，桥接）。企业微信按同一套 Adapter 接口留位，不实现。**

## 5. 架构设计

### 5.1 分层

```
┌─────────────────────────── pigo 进程 ───────────────────────────────┐
│                                                                      │
│  internal/im                                                        │
│  ┌────────────────┐   ┌──────────────────┐   ┌───────────────────┐  │
│  │  Adapter 实现   │──▶│                  │   │  BindingStore     │  │
│  │  ├ feishu      │   │   Router         │──▶│  chat → session   │  │
│  │  ├ wechat      │◀──│  (命令解析/绑定)  │   │  (持久化 JSON)     │  │
│  │  └ (wecom 留位) │   │                  │   └───────────────────┘  │
│  └────────────────┘   └────────┬─────────┘                          │
│         ▲                      │                                     │
│         │ Inbound / Outbound    │ Prompt / Steer / Abort             │
│         │                      ▼                                     │
│         │            ┌──────────────────────┐                        │
│         │            │ internal/serve       │  ← 已有，无需改动        │
│         │            │  Service             │                        │
│         │            │  └ SessionRuntime    │                        │
│         │            │      Snapshot        │                        │
│         │            │      Subscribe ──────┼──▶ AgentEvent 流        │
│         │            │      Prompt/Steer    │                        │
│         │            └──────────────────────┘                        │
│         │                      │                                     │
│         └──────────────────────┴── 审批回调（工具确认 / 卡片按钮）      │
└──────────────────────────────────────────────────────────────────────┘
         ▲                                        ▲
         │ 出站 WS 长连接（无需公网入站）             │ 本机 IPC
         │                                        │
   飞书开放平台                              微信桥接进程 (stdio NDJSON)
```

关键点：**`internal/serve` 一行不改**。它就是为「多个前端消费同一会话」设计的（注释里已明说 `repl/tui/headless/remotecontrol/and later a web frontend`），IM 通道正是下一个。

### 5.2 核心接口

```go
// internal/im/channel.go
package im

// Adapter 是一个 IM 平台的收发实现。每个平台一个实现，
// 生命周期由 im.Serve 管理。
type Adapter interface {
    // ID 返回通道标识："feishu" / "wechat-personal" / "wecom"
    ID() string

    // Run 建立通道并把入站消息投递到 in，阻塞直到 ctx 取消。
    // 必须做到：收到平台事件后立刻 ack（飞书要求 3s 内返回），
    // 真正的处理放到 Router 的 worker 里，绝不能在这条协程里跑 Agent。
    Run(ctx context.Context, in chan<- Inbound) error

    // Send 发送一条出站消息，返回平台消息 id（用于后续原地更新）。
    Send(ctx context.Context, chat ChatRef, msg Outbound) (string, error)

    // Update 尝试原地更新已发送消息。不支持的平台返回 ErrUpdateUnsupported，
    // 由 Router 降级为「追加新消息」。
    Update(ctx context.Context, chat ChatRef, msgID string, msg Outbound) error
}

type Inbound struct {
    Adapter string
    Chat    ChatRef   // 会话定位
    Sender  UserRef   // 发送者（用于白名单裁决）
    MsgID   string    // 平台消息 id（幂等去重用）
    Text    string
    Kind    Kind      // KindText | KindCommand | KindAction
    Action  string    // 卡片按钮回传的 value
    At      time.Time
}

type Outbound struct {
    Text    string
    Buttons []Button  // 非空则渲染为交互卡片
    Final   bool      // 最终答复（区别于中间进度）
}

var ErrUpdateUnsupported = errors.New("im: adapter does not support in-place update")
```

```go
// internal/im/router.go
// Router 把一个 IM 会话映射到一个 pigo 会话，并驱动它。
type Router struct {
    svc      serve.Service
    store    *BindingStore      // chat → session 持久化
    cfgs     ChatPolicy         // 白名单 / 工具策略 / 节流
    inflight sync.Map           // chatID → *runState（防并发同会话双跑）
}
```

### 5.3 会话绑定模型

一条 IM 会话（`ChatRef.ID`）绑定一个 pigo `session.ID`，持久化在 `$PIGO_HOME/im/bindings.json`：

```json
{
  "version": 1,
  "bindings": {
    "feishu:p2p:oc_abc123": {
      "session_id": "20261112-093012-a1b2",
      "cwd": "/Users/hx/proj/foo",
      "model": "claude-opus-4-8",
      "created_at": "2026-11-12T09:30:12Z",
      "last_active": "2026-11-12T10:02:44Z"
    }
  }
}
```

设计要点：

- **懒创建**：第一条消息才 `CreateSession`，绑定在建会话那一刻写入；空会话不落盘（沿用 `serve.Service` 现有语义）。
- **一 Chat 一 Run**：同一 chat 上一次 run 未结束时，新消息默认走 `Steer`（注入当前轮次），而不是并列开第二个 run。`inflight` map 做互斥。
- **cwd 隔离**：绑定记录里带 `cwd`。IM 来的请求默认落在绑定时指定的工作目录，**不允许**从聊天内容里任意切换目录（否则等于把整台机器暴露给「谁能发消息」这件事）。
- **群聊策略**：群聊必须 `@机器人` 才触发；群聊默认绑定到「群专属只读会话」（`no_tools` 或仅 read 工具），需要写权限时用 `/mode write` 显式提权，提权动作可审计。

### 5.4 控制命令

在 IM 里以 `/` 开头，Router 拦截后不进入模型：

| 命令 | 作用 | 备注 |
|------|------|------|
| `/new` | 结束当前绑定，下一条消息开新会话 | 保留旧会话可 `/resume` |
| `/resume <id>` | 绑定到历史会话 | id 从 `/sessions` 拿 |
| `/sessions` | 列出最近 10 个会话 | 带模型、时间、cwd 尾段 |
| `/stop` | `Abort()` 当前 run | |
| `/status` | 阶段、模型、已跑时长、token 用量 | |
| `/model <id>` | `SetModel` | 白名单内可选 |
| `/mode read\|write` | 切换工具边界 | 群聊提权入口 |
| `/help` | 命令列表 | |

非命令消息 = prompt。

### 5.5 事件渲染策略（决定体验的核心）

这是 IM 通道最容易做砸的地方。Agent 一次 run 可能产出几十个 `AgentEvent`，直接逐个发消息会刷屏、触发限流、并且在微信上会显著抬高风控概率。

统一策略：**一条「进度消息」+ 一条「结果消息」**。

```
入站 prompt
   │
   ├─▶ 立即回一条占位：「收到，正在处理…」（<1s，先把用户安抚住）
   │
   ├─▶ 订阅 SessionRuntime.Subscribe()
   │      ├─ 文本增量 → 累积到 buffer，按 throttle 节流
   │      ├─ 工具调用开始 → 更新进度行：「▸ read internal/serve/runtime.go」
   │      ├─ 工具调用结束 → 更新进度行：「✓ read (128 lines)」
   │      └─ 需要确认 → 发审批卡片/文本，等待决策
   │
   └─▶ Run 结束
          ├─ Update 最后一条消息为完整答案（飞书：PATCH 卡片）
          └─ 不支持 Update 的平台：发一条 Final 消息
```

各平台落地差异：

| 平台 | 占位消息 | 中间进度 | 最终答案 | 节流 |
|------|---------|---------|---------|------|
| 飞书 | 卡片（config 元素） | PATCH 同一条卡片 | PATCH 为完整 markdown | 2s，且至少变化 64 字符才发 |
| 微信个人号 | 纯文本 | **不发** | 单独一条消息，超长分片 | 只发 Final |
| 企业微信（留位） | 模板卡片 | 流式推送 | 同上 | 同上 |

微信个人号「不发中间进度」是有意的：OCR/UIA 路径单条发送 ~10s，发 5 条进度会拖到 1 分钟，且高频发送是最容易触发风控的行为。宁可让用户多等，也不要刷屏。折中方案：超过 60s 未完成时补发一条「仍在运行（已 X 秒）」，仅此一条。

### 5.6 审批流

复用 `serve.SessionRuntime` 之外的确认机制。当前 CLI 的确认走 `remotecontrol.Bridge.Confirm`，IM 侧需要一个平行的 `Approver`：

```go
// internal/im/approve.go
type Approver interface {
    // Ask 发审批请求并阻塞等待决策。ctx 超时视为拒绝（fail-closed）。
    Ask(ctx context.Context, chat ChatRef, req ApprovalReq) (Decision, error)
}
```

| 平台 | 呈现 | 决策回传 | 超时 |
|------|------|---------|------|
| 飞书 | 交互卡片，两个按钮「允许」「允许并记住本会话」+ 一个「拒绝」 | 卡片回调 `card.action.trigger`（**新版卡片回调，长连接支持；旧版消息卡片回传交互不支持长连接**） | `[im] approval_timeout`，默认 10m，超时即拒绝 |
| 微信个人号 | 文本：「⚠️ 需要执行 bash: `rm -rf ./dist`\n回复 `y` 允许，`n` 拒绝（5 分钟内有效）」 | 下一条文本消息命中 `y/n` 即消费 | 5m |

安全默认：**IM 通道下审批默认 fail-closed**。本地终端可以「等人回来再点」，IM 场景下无人应答必须当作拒绝，否则等同于把确认机制废掉。

### 5.7 安全模型

| 风险 | 对策 |
|------|------|
| 谁能控制这台机器 | 飞书：`allowed_users`（open_id / 邮箱）白名单，非白名单消息直接丢弃并记一行 log，不回复（避免探测）。空白名单 = 拒绝所有。 |
| 群聊被他人利用 | 群聊必须 @机器人 + 群在 `allowed_groups` 白名单 + 默认只读模式 |
| 凭据泄漏 | App Secret / Bot Secret 只从 `$PIGO_HOME/.credentials.yaml`（0600）或环境变量读；`config.toml` 只存 `credential = "feishu-main"` 引用名，沿用 issue #568 已有的凭据引用机制。日志里做脱敏。 |
| 任意目录操作 | 绑定固定 cwd，不支持从消息切换；`/mode read` 时工具白名单收敛到 read/grep/find |
| 脏数据回灌 | 平台消息 id 幂等去重（微信弱网会重推同一 MsgId，飞书事件也会重推） |
| 敏感内容外发 | IM 消息走平台服务器，等同于把会话内容交给第三方。首次连接时打印明确提示，`[im] egress_warning = true` 可关。 |

## 6. 微信个人号专项

### 6.1 技术路线对比

现状（2026-11）实测信息：

| 路线 | 收消息 | 发消息 | 版本约束 | 检测面 | 判断 |
|------|-------|-------|---------|--------|------|
| `WeChatFerry` (hook DLL) | ✅ 实时回调 | ✅ 快 | 锁 3.9.12.x，不支持 4.x | 高（进程注入） | 功能最全，但必须冻结微信版本、关自动更新 |
| `wechatauto-replica` (UIA + OCR 兜底) | ⚠️ 轮询 + OCR 投票 | ⚠️ 慢，实测 10~13s/条 | 微信 4.x | 低（不动进程） | 4.x 下唯一还在维护的开源路径 |
| 本地数据库解密读取 + Accessibility 发送 | ✅ 纯本地文件，检测面极低 | ✅ | 3.9 / 4.x 均可 | 极低 | macOS 最务实组合 |
| 协议逆向（iPad / Web 协议） | ✅ | ✅ | — | 极高（服务端可识别） | **不采用**。封号风险最高，且可能触及法律风险 |

### 6.2 采用「桥接进程 + NDJSON 协议」

主程序**不内联**任何平台实现，只定义协议，平台实现作为独立子进程外挂：

```
pigo  ──stdin/stdout NDJSON──▶  pigo-im-wechat-bridge（可替换）
                                  ├─ 用户选 1：wechatauto-replica (4.x UIA)
                                  ├─ 用户选 2：WeChatFerry (3.9.12 hook)
                                  └─ 用户选 3：自研 db 解密 + UIA
```

理由有三条，都很实际：

1. **合规边界清晰**。pigo 仓库本体只包含一个进程间协议和 HTTP 客户端，不含 hook、不含协议逆向、不含任何绕过微信安全机制的实现。是否启用、用什么实现，由使用者自行决定并承担后果。
2. **版本失效只换桥接**。微信一升级，UIA 树一变，只需换掉子进程；主程序的 Router、绑定、审批、渲染全部不受影响。
3. **语言不受限**。现有成熟实现全是 Python，用 NDJSON 桥接就不用把它们翻译成 Go。

协议（与 LSP 同构，行分隔 JSON）：

```jsonc
// bridge → pigo（入站）
{"jsonrpc":"2.0","method":"im.recv","params":{
  "chat":{"id":"文件传输助手","kind":"p2p"},
  "sender":{"id":"wxid_self","name":"红鑫"},
  "msgId":"8812:1700000000:hash",
  "text":"查一下昨天那个构建失败的日志",
  "at":"2026-11-12T09:30:12+08:00"
}}

// pigo → bridge（出站）
{"jsonrpc":"2.0","id":7,"method":"im.send","params":{
  "chat":{"id":"文件传输助手","kind":"p2p"},
  "text":"构建失败在 internal/serve/runtime.go:214 …",
  "final":true
}}
// ← {"jsonrpc":"2.0","id":7,"result":{"msgId":"..."}}
```

### 6.3 风险与收敛策略

| 风险 | 严重度 | 对策 |
|------|-------|------|
| 账号被封 / 功能受限 | 高 | **只与自己的会话通信**：仅监听「文件传输助手」或用户自建的单人会话，绝不向他人发送、绝不群发、绝不加好友。腾讯打击的是批量注册/高频群发/协议模拟登录，此用法风险显著低于公开报告的营销场景，但**不为零**，需使用者知情 |
| 版本升级后通道失效 | 高 | 冻结版本 + 关闭自动更新；`/im status` 显示桥接进程健康度与最近一次成功收发时间 |
| 发送慢、体验差 | 中 | 只发 Final；超过 60s 补一条心跳 |
| 窗口必须可见（UIA 路径） | 中 | 文档写明；提供「最小化到托盘」说明 |
| 消息顺序错乱（轮询） | 中 | 用 `msgId` + 服务端时间戳排序，幂等去重 |
| macOS 无 hook 方案 | 中 | macOS 走「db 解密收 + Accessibility 发」组合；本项目桌面端双平台，需分别提供桥接 |

### 6.4 本期不做

- 个人微信号的群聊控制
- 向他人主动发消息（含通知推送）
- 图片/文件/语音消息（v1 只做纯文本，`[image]` 之类的占位提示）

## 7. 配置

`config.toml` 新增 `[im]` 表，遵循现有「命令行 flag > config.toml > 内置默认」的链：

```toml
[im]
enabled = false
# 工具确认策略：confirm（默认，走审批流）| allow（直接放行，仅建议单人自用）
tool_policy = "confirm"
approval_timeout = "10m"
# 首次连接时提示「消息会经过平台服务器」
egress_warning = true

[im.feishu]
enabled = true
credential = "feishu-main"      # 引用 $PIGO_HOME/.credentials.yaml 里的名字
# 或者直接写（不推荐，会被日志脱敏但仍落盘明文）
# app_id = "cli_xxx"
# app_secret = "xxx"
allowed_users = ["ou_xxxxxxxx", "hx@example.com"]
allow_groups = false
group_require_mention = true
progress_throttle = "2s"

[im.wechat_personal]
enabled = false
transport = "bridge"
# 桥接进程命令；pigo 通过 stdio NDJSON 与它通信
bridge_command = ["python", "-m", "pigowx.bridge"]
# 只接受来自这些会话的消息（必须包含自己）
self_chats = ["文件传输助手"]
heartbeat_after = "60s"
```

`.credentials.yaml`（沿用既有格式）：

```yaml
feishu-main:
  app_id: cli_a1b2c3d4
  app_secret: xxxxxxxx
```

## 8. CLI 与桌面端表面

### 8.1 CLI

```bash
# 常驻前台运行，多个通道并行
pigo im serve
pigo im serve --channel feishu
pigo im serve --channel feishu,wechat-personal

# 诊断
pigo im status          # 各通道连接状态、最近消息时间、绑定数
pigo im bindings        # 列出 chat → session 绑定
pigo im test feishu     # 发一条自检消息给自己（验证凭据与权限）
```

REPL/TUI 内新增 `/im` 命令，与 `/remote-control` 平级：

```
/im              查看状态
/im stop         停止所有通道
/im start feishu 单独启动
```

复用 `/remote-control` 的既有做法：在 `repl.go` / `tui/model.go` 的 input loop 里**先于 registry 解析前置拦截**（`line == "/im" || strings.HasPrefix(line, "/im ")`），因为 `/im` 需要读取 live 的通道状态，不能做成无状态的静态命令。状态展示类子命令（`/im status` 的格式化输出）再走 `prompts.RegisterLiveCommands` 注册到 `runtime.SlashRegistry`，与 `/model`、`/thinking` 同层。

### 8.2 桌面端

Tauri 侧只需要三件事，不需要新增 Agent 逻辑：

1. **Settings 页**：`RemoteChannelsPage`，飞书填 App ID/Secret → 点「连接」→ 显示 `已连接 / 重连中 / 凭据无效`。写配置走已有 config 写入通道。
2. **sidecar 托管**：在 `desktop/sidecar/internal/server/server.go` 新增三个 RPC 方法，转发到 `internal/im`：
   - `im.start` / `im.stop` / `im.status`
   - 事件通过已有的 `emitter` 推 channel 状态给前端
3. **常驻策略**：默认「随 App 启停」；`[im] keep_running_on_close = true` 时，窗口关闭后 sidecar 保持存活（Tauri 需要在 `on_window_event` 里跳过 sidecar kill）。

注意：sidecar 目前不走 `internal/serve`（它有自己的 `internal/runtime`，因为工具需要 host-proxy 回 Rust 侧执行）。**因此 IM 通道在桌面端有两种可选接法**，需要 Phase 2 定夺：

| 接法 | 做法 | 优点 | 缺点 |
|------|------|------|------|
| A. 独立进程 | 桌面端另起 `pigo im serve` 子进程，与 sidecar 并列 | `internal/im` 只依赖 serve，零适配 | 两套会话存储/配置需对齐；IM 会话不出现在桌面端列表 |
| B. sidecar 内嵌 | 在 sidecar 里实现 `serve.Service` 适配（把 sidecar runtime 包成 SessionRuntime） | IM 会话与桌面端会话同源，能在 UI 里看到 | 需要一层适配器，工作量约 +3~5 天 |

**推荐 A 先落地，B 作为后续统一方向。** 理由：A 能在 Phase 1 之后立刻可用，且不阻塞飞书主链路上线；B 的真正价值是「IM 里发的消息，桌面端能看到同一个会话」，属于体验优化而非必需能力。

## 9. 分阶段落地

| 阶段 | 内容 | 交付物 | 预估 |
|------|------|--------|------|
| **P0** | `internal/im` 骨架：`Adapter` / `Router` / `BindingStore` / `Approver` 接口 + 一个内存假 Adapter | 包 + 表驱动测试（含并发同 chat、幂等去重、超时拒绝） | 2~3 天 |
| **P1** | 飞书 Adapter：长连接、卡片渲染、PATCH 更新、`card.action.trigger` 审批回调、白名单 | 可用通道 + 真机验证记录 | 4~5 天 |
| **P2** | CLI `pigo im serve` + `pigo im status` + REPL `/im`；桌面端 Settings 页 + sidecar RPC（接法 A） | 双端可启动、可观测 | 3~4 天 |
| **P3** | 微信个人号桥接：NDJSON 协议 + 参考实现（Windows：UIA/OCR 路径）+ `self_chats` 收敛 | 协议文档 + 桥接骨架 + 一份平台接入说明 | 4~6 天 |
| **P4**（可选） | 企业微信 Adapter；桌面端接法 B | — | 视需求 |

P1 完成即可交付真实价值，P3 是「锦上添花但脆弱」的部分，允许延后。

## 10. 验收标准

- [ ] `pigo im serve --channel feishu` 在无公网 IP 的机器上建立长连接，日志出现 `connected`，无 secret 明文
- [ ] 手机飞书单聊发 prompt，1s 内收到占位卡，run 结束后同一张卡片被更新为完整答案
- [ ] 非白名单用户发消息：无回复，日志一行 `drop unauthorized sender=…`
- [ ] 高危工具调用触发审批卡片，10 分钟不点按拒绝处理（Agent 收到拒绝结果继续跑，不挂死）
- [ ] `session_host = desktop` 下高危工具同样在飞书弹出审批卡片；点「允许 / 本会话允许 / 拒绝」分别落到 host 的 `allow-once / allow-session / deny`，且 PC 端残留卡片同步消失
- [ ] 同一 chat 连发两条消息，第二条走 Steer 而非并行第二个 run
- [ ] 平台重推同一 MsgID 两次，只处理一次
- [ ] 微信通道：非 `self_chats` 会话的消息被丢弃；`y/n` 审批可用；桥接进程崩溃后 30s 内自动重启并恢复
- [ ] 全部单元测试 `go test ./internal/im/...` 绿；`gofmt` / `go vet` 干净

## 11. 待定问题

1. **桌面端接法 A/B** 需要在 P2 前定。若目标是「IM 与桌面端共享会话列表」，B 就必须做。
2. **飞书事件 3 秒窗口**：SDK 的 handler 里必须只做「解析 + 入队」就返回。需要确认 Router 的队列在极端堆积时的丢弃策略（建议按 chat 限深，超限回一条「排队的会话太多，稍后再试」）。
3. **微信桥接的默认实现**选哪个？建议不内置任何实现，只提供协议与文档，由用户自行接。这一点需要在开工前和你的判断对齐——内置实现会把仓库拖进合规灰区。
4. **凭据是否进 `.credentials.yaml`**：沿用 `credential = "name"` 引用是当前最优，但要确认 `internal/cli/config` 的凭据解析在 sidecar 场景下也能读到同一路径（`$PIGO_HOME`）。

---

## 12. 实现状态（2026-11-12）

按用户决定，**本期只做飞书**（微信个人号暂缓），队列丢弃策略按「按 chat 限深 + 超限回一条」实现。

### 12.1 已交付

| 文件 | 内容 |
|------|------|
| `internal/im/types.go` | `ChatRef`/`UserRef`/`Inbound`/`Outbound`/`Button`/`ApprovalReq`/`Decision`，含 `Tone` 语义色 |
| `internal/im/adapter.go` | `Adapter`（Run/Send/Update/Close）+ `Approver`（Ask）两个接口，`ErrUpdateUnsupported` |
| `internal/im/queue.go` | 按 chat 限深的 FIFO 车道，`DefaultQueueDepth = 3`，溢出回调 + panic 隔离 |
| `internal/im/router.go` | 绑定/命令/事件渲染/标签页节流/超时中止，`RejectReply` 默认「排队太多，稍后再试。」 |
| `internal/im/binding.go` | `$PIGO_HOME/im/bindings.json`，临时文件 + rename 原子写 |
| `internal/im/commands.go` | `/new /resume /sessions /stop /status /model /help` + 卡片按钮 → 命令映射 |
| `internal/im/approve.go` | `ApprovalRouter`（confirm/allow/deny），`WrapRunConfig` 把工具确认接进 `Batch.BeforeToolCall` |
| `internal/im/feishu/adapter.go` | 长连接、白名单、消息解析、按钮回调、卡片 PATCH、SDK 日志降噪 |
| `internal/im/feishu/card.go` | 卡片 JSON（`update_multi: true`）、语义色、审批三按钮、9000 rune 截断 |
| `internal/cli/imcmd/imcmd.go` | `pigo im serve` / `pigo im bindings`，配置与 flag 合并、启动前校验 |
| `internal/cli/config/config.go` | `[im]` / `[im.feishu]` 表 |
| `cmd/pigo/main.go` | `im` 子命令前置分发（与 `evals`/`pkgcmd` 同层） |
| `internal/im/host.go` / `servehost.go` | `SessionHost`/`AgentSession`/`EventStream` 抽象 + 本地适配 |
| `internal/im/desktop/*` | 远端实现：descriptor 发现、`agent/status` 轮询合成事件 |
| `apps/desktop-tauri/src-tauri/src/control_server.rs` | 本机回环控制端点（8 个 op、每次启动换 token） |
| `apps/desktop-tauri/src-tauri/src/remote_channel.rs` | 通道设置、`pigo im serve` 子进程托管、日志环形缓冲、端点探测 |
| `apps/desktop-tauri/src/components/settings/RemoteChannelsPage.tsx` | Settings →「远程控制」页 |

依赖只增加了 3 个：`larksuite/oapi-sdk-go/v3` + `gogo/protobuf` + `gorilla/websocket`（后两个是 SDK 的）。

### 12.2 与规划的差异

1. **`internal/serve` 确实一行未改** —— 规划中的这条承诺成立。
2. **桌面端接入（P2）改走方案 C**（见 §12.3），不是规划里推荐的 A 或 B。`pigo im serve --session-host desktop` 现已可驱动桌面会话。

   2026-11-12 补齐：Settings →「远程控制」页（US-005）已实现，见 §12.5。
3. **所有出站消息统一发交互卡片**，而不是「文本 + 卡片混用」。原因：飞书 PATCH 只能更新交互卡片，混用会导致进度消息无法变成最终答案。这一点规划里没写死，是实现时定下的。
4. **进度更新只在 `ToolExecutionStart/End` 与 `MessageEnd` 触发**，不吃流式 delta。`MessageEnd` 拿到的是完整消息文本，比拼装增量稳，且天然限流。
5. **`tool_policy = confirm` 的落地方式**是 `ApprovalRouter.WrapRunConfig` 包住宿主的 `RunConfigFunc`，在 `Batch.BeforeToolCall` 上挂一层。宿主自己的 trust 判断先跑，它已经决定的一律尊重——IM 不该覆盖「这个目录已信任」这类知识。这只适用于 `session_host = cli`：桌面宿主的 RunConfig 在 Rust 侧组装，Go 进程拿不到这个钩子，所以它改走「轮询 host 待确认队列」的审批桥（见 §12.7），三种 policy 在两种宿主下语义一致。

### 12.3 桌面端接入：改用方案 C（控制面端点）

规划里的 A（独立子进程）与 B（sidecar 内嵌 SessionRuntime）都不成立或代价过高，实际采用了第三条：

| | A 独立进程 | B sidecar 内嵌 | **C 控制面端点（已实现）** |
|---|---|---|---|
| IM 会话与桌面端同源 | ❌ | ✅ | ✅ |
| 需要重建 PromptParams | 否 | 是（provider / 插件工具 / 项目上下文全在前端组装） | 否（Rust 侧 `agent_prompt` 只要 `sessionId` + `content`） |
| 改动面 | 打包子进程 + Settings 页 | sidecar runtime 适配层 | Rust 一个端点 + Go 一个后端 |

**实现**：`desktop/apps/desktop-tauri/src-tauri/src/control_server.rs` 在 127.0.0.1 随机端口起极简 HTTP 服务，每次启动生成 250 bit token 写入 `<data_dir>/control.json`，`POST /invoke {op, args}` 转发到 `router::route`。白名单只有 8 个：`session/create|list|get|configure`、`agent/prompt|status|stop|steer`；`desktop_control` 里的 `dangerous` 操作（删除会话、改设置、装插件）一律 `NOT_ALLOWED`——远端调用者背后没有人可以点确认框。

**Go 侧**：`internal/im` 不再依赖 `serve` 具体类型，改为 `SessionHost` / `AgentSession` 接口（`host.go`）。`ServeHost`（`servehost.go`）是本地实现；`internal/im/desktop` 是远端实现，用 `agent/status` 轮询 + 会话尾部文本合成 `ToolExecutionStartEvent` / `MessageEndEvent` / `AgentEndEvent` 喂给 Router——桌面端的事件流只在 shell 内部广播，外部订阅不到，轮询是这里唯一诚实的选择。

**已知取舍**：
1. 进度卡比桌面端画面滞后约 1~2s，且拿不到工具名（`agent/status` 只给 activity phase）。
2. ~~工具确认由桌面端窗口负责，IM 卡片不参与~~ → **已解决（见 §12.7）**：host-core 的 `permissions.pending` / `permissions.resolve` 通过控制端点暴露给通道，飞书按待确认队列弹卡片，PC 窗口和飞书卡片是同一个问题的两个视图，谁先答谁生效。规划里的「审批卡片」现在两种 `session_host` 都支持。
3. 桌面端必须保持运行；App 退出后 `control.json` 被删除，`pigo im serve` 会明确报「找不到控制端点」。

### 12.4 队列策略的最终形态

```go
// queue.go
type Queue struct {
    depth   int                       // 每个 chat 的等待上限（不含正在跑的那条）
    handler HandlerFunc               // 每个 chat 一条 goroutine，串行
    onFull  func(Inbound, int)        // 溢出回调，同步执行
}
```

- `depth` 默认 3，可由 `[im.feishu] queue_depth` 覆盖。
- 溢出时 `Submit` 返回 `RejectedFull`，`onFull` 交给 Router。
- Router 的 `handleRejected` 做**按 chat 冷却**（默认 10s）：第一次溢出回一条「排队太多，稍后再试。」，冷却期内其余静默丢弃并记日志。理由写在代码注释里——一次发八条得到八条同样的拒绝，读起来像故障。
- 回复走独立 goroutine，不阻塞 adapter 的读循环（飞书 3 秒窗口）。
- 命令不走队列：`/stop` 必须在队列满时也能用，那正是用户最需要它的时候。

### 12.5 桌面端 Settings 页（US-005，已实现）

规划设想的接法 A 是「sidecar 加 `im.start/stop/status` RPC」，实际接在 shell 侧，因为通道进程不属于 agent loop：

| 层 | 实现 |
|---|---|
| Rust `remote_channel.rs` | 设置持久化（`<data_dir>/im-channel.json`）、渲染 `im-channel.toml`、解析 `pigo`、启停子进程、抓 stdout+stderr 环形日志、探测控制端点、App 退出时杀掉子进程 |
| router | `im/channel/get\|save\|clearSecret\|start\|stop\|pickBinary` 六个 channel |
| 前端 | `RemoteChannelsPage`（Settings → 远程控制）、`lib/remote-channel.ts`（纯逻辑，10 个单测） |

**关键取舍**

1. **密钥不出 shell**：`get` 只返回 `hasSecret`，输入框留空表示「保持不变」，清除需要显式按钮。设置文件与生成的 TOML 都按 0600 落盘（Windows 继承用户目录 ACL）。
2. **二进制解析顺序**：设置里指定的路径 → `pigo-desktop.exe` 同目录 → `PATH`。第二种让便携部署零配置，也是本机部署时把 `pigo.exe` 放在 App 旁边的理由。
3. **启动前置校验**：`session_host = desktop` 时先探控制端点，端点在才 spawn —— 否则用户会看到一个「启动成功但每条消息都失败」的通道。
4. **日志是唯一的诊断面**：app_id 无效、权限没开、token 过期都只在这里出现，所以页面把最后 40 行常驻展示，不折叠。
5. **App 退出即杀子进程**：通道依赖本 App 的会话，留下孤儿只会对每条消息报连接错误。

### 12.6 下一步

1. `pigo im test`：发一条自检卡片，验证凭据、权限与卡片渲染。
2. ~~桌面端审批卡片：把 IM 审批接到 renderer 的 asktool 流~~ → **已完成（2026-11-12，见 §12.7）**。注意实际接的不是 renderer 的 asktool 流：工具权限确认在 host-core（`permissions.request`），与 sidecar 的 `asktool_request` 是两套机制，接错会漏掉 Edit/Bash 这类真正的写操作确认。
3. 微信个人号桥接（规划 §6），仍是「只给协议 + 文档、不内置实现」。

### 12.7 桌面端审批卡片（2026-11-12 补齐）

**问题**：`session_host = desktop` 下，工具确认只由 host-core → shell → renderer 的窗口卡片承担，`pigo im serve` 进程订阅不到这个广播，飞书里一片安静，只能回 PC 点按钮。窗口最小化时确认直接挂到 120s 超时。

**根因**（不是配置问题）：控制端点的白名单里没有待确认队列的读/写 op；`imcmd.runServe` 在 desktop 分支刻意不构造 `ApprovalRouter`；`ApprovalRouter.WrapRunConfig` 只作用于本进程的 `serve.RunConfigFunc`，而桌面会话的 RunConfig 由 Rust 侧组装。

**接法**：host-core 原本就有 `permissions.pending`（D374/D375，为「晚连接的客户端补读已弹出的确认」而设计）和 `permissions.resolve`，所以不需要新增 host 能力，只需把它们暴露给外部驱动器：

| 层 | 改动 |
|---|---|
| Rust `control_server.rs` | 白名单新增 `permissions/pending` → `pi-desktop/tool/pendingPermissions`、`permissions/resolve` → `pi-desktop/tool/resolvePermission`。只按 host 签发的 `requestId` 回答已有问题，不是授权接口；settings / plugins / deletes 仍一律 `NOT_ALLOWED` |
| Rust `router.rs` | 新增 pending 通道；`resolvePermission` 成功后 emit `tool_permission_resolved`，让窗口里那张已经被回答的卡片消失 |
| 前端 `events-slice.ts` | 在 activeSession 门禁之前按 `requestId` 全局移除 pending 卡片（后台会话的卡片同样会变陈旧） |
| Go `internal/im/desktop/permissions.go` | `Client` 实现 `im.PermissionGate`：`Pending` 解析 camelCase wire（含 `remainingMs`、args 预览），`Resolve` 把 host 的 `NOT_FOUND` 归一成 `im.ErrPermissionGone` |
| Go `internal/im/permissions.go` + `approval_poll.go` | `PermissionPoller`：一轮 run 期间按 700ms 轮询该 session 的待确认队列，按 `requestId` 去重，飞书卡片超时取 `min(host remainingMs, approval_timeout)`，剩不到 8s 就不再发卡片 |
| Go `router.go` / `approve.go` / `imcmd.go` | 只有宿主实现 `PermissionGate` 时才启动轮询；chat 与 approver 来自绑定表而非请求本身；desktop 分支也构造 `ApprovalRouter`（但不调 `WrapRunConfig`） |

**语义与安全**：
- 三种 `tool_policy` 在两种宿主下一致：`confirm` 弹卡片、`allow` 自动 `allow-once`、`deny` 自动拒绝。
- 卡片三个按钮一一映射：`允许 → allow-once`、`本会话允许 → allow-session`、`拒绝 → deny`；审批出错、超时、ctx 取消一律显式写回 `deny`（fail-closed），host 自带 120s 超时是最后一道保险。
- 竞态：PC 端先点 → host 队列移除该请求 → 轮询器 cancel 飞书卡片并改写为「已处理」；飞书先点 → shell emit 事件清掉窗口卡片。`NOT_FOUND` 视为「已在别处处理」，不报错。
- 降级：只升级 `pigo.exe` 而桌面 App 仍是旧版时，`permissions/pending` 会回 `NOT_ALLOWED`；客户端把它归一成 `ErrPermissionUnsupported`，轮询器打一行「需重建桌面 App」后停止，不刷日志、不发无效卡片。
- 轮询只覆盖 **IM 发起的这一轮 run**，且按 sessionId 过滤；PC 端自己发起的对话不会被推送到飞书。

**验证**：`go test ./internal/im/... ./internal/cli/imcmd/...` 全绿（含轮询器策略/去重/撤回/超时的表驱动测试与控制端点 wire 测试），`go vet` 干净，前端 `tsc --noEmit` 通过。Rust 侧 `cargo test -p pigo-desktop control_server` 需要在装好 VS Build Tools（MSVC `link.exe`）或 MinGW `dlltool.exe` 的机器上跑；本机两个都缺，未做编译验证。
