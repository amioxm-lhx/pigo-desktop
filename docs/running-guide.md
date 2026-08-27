# pigo 运行说明

本文档覆盖 pigo 从环境准备、构建、密钥配置到全部运行模式的完整使用说明。
快速入门见仓库根目录 [README.md](../README.md)，本文为详细版。

---

## 1. 环境要求

| 项目 | 要求 |
|------|------|
| Go 运行时 | **1.27+**（go.mod 声明 `go 1.27rc1`） |
| 操作系统 | macOS / Linux（Windows 请从 Releases 下载 zip） |
| API Key | 至少一个 Provider 的密钥（本地 Ollama 除外） |

### 1.1 GOROOT 注意事项

若使用 `g`、`goenv` 等 Go 版本管理器，注意：**显式导出的 `GOROOT` 必须与
实际使用的 toolchain 版本一致**，否则编译报
`compile: version "goX" does not match go tool version "goY"`。

- 项目 go.mod 要求 `1.27rc1`，`GOTOOLCHAIN=auto` 会自动下载切换工具链；
  若 `GOROOT` 仍指向旧版本目录就会冲突。
- 解决：用版本管理器切到匹配版本（如 `g install 1.27rc1 && g use 1.27rc1`），
  或干脆不导出 `GOROOT`，让 go 命令自行推导。
- 仓库提供的 `start.sh` 内置了版本不匹配检测，会自动在脚本内取消冲突的 GOROOT。

---

## 2. 构建

```bash
# 方式一：仓库自带的启动脚本（推荐，自动加载 .env、自动构建）
./start.sh --version

# 方式二：直接 go build（生成 ./pigo）
go build -o ./pigo ./cmd/pigo

# 方式三：安装到 $GOPATH/bin
go install ./cmd/pigo

# 方式四：不构建直接运行
go run ./cmd/pigo -p "1+1=?"
```

`start.sh` 支持的环境变量：

| 变量 | 说明 |
|------|------|
| `PIGO_BIN` | 二进制输出路径，默认 `./pigo` |
| `PIGO_NO_BUILD=1` | 跳过构建，直接运行已有二进制 |

其他安装途径：一键脚本 `curl -fsSL .../install.sh | sh`（从 GitHub Releases
下载预编译二进制），详见 README「安装与构建」章节。

构建后验证：

```bash
./pigo --version
# pigo dev (commit none, built unknown)   ← 源码构建显示 dev；正式发布版由 goreleaser 注入版本号
```

---

## 3. 密钥配置

Provider 的 API Key 从**环境变量**读取（变量名见第 6 节表格），三种配置途径：

### 3.1 仓库 `.env` 文件（配合 start.sh）

仓库根目录的 `.env`（已被 gitignore）由 `start.sh` 启动时自动加载：

```bash
OPENROUTER_API_KEY=sk-or-...
DEEPSEEK_API_KEY=sk-...
```

### 3.2 Shell 环境变量

```bash
export OPENROUTER_API_KEY=sk-or-...
./pigo
```

### 3.3 命令行参数 / 配置文件

- `-k/--api-key` 直接传入（优先级最高）
- `~/.config/pigo/config.toml` 的 `api_key` 字段（见第 11 节）

优先级：**CLI 参数 > config.toml > 环境变量 > 内置默认**。

---

## 4. 运行模式

不带 `-p` 且 stdout 是终端时进入交互界面；**默认是全屏 TUI**，
`--no-tui`（或 stdout 非终端）退化为行式 REPL。

### 4.1 交互式 TUI / REPL

```bash
./pigo                # 全屏 TUI（默认）
./pigo --no-tui       # 行式 REPL
./pigo -m deepseek-chat   # 指定模型进入交互界面
```

### 4.2 无头模式（headless / print）

适合脚本与管道，运行完打印最终回答并以退出码反映成败（0 成功 / 1 失败）：

```bash
./pigo -p "阅读 README 并总结"
./pigo "阅读 README 并总结"                      # 位置参数等价于 -p
./pigo -p "..." -o stream-json                  # 逐行输出 JSON 事件流
./pigo -p "..." -o stream-json | jq .           # 可组合进管道
```

`--output-format/-o`：`text`（默认，仅最终文本）| `stream-json`（行分隔 JSON 事件）。

### 4.3 会话管理

交互会话以 JSONL 持久化在 `~/.pigo/sessions/`：

```bash
./pigo -l                 # 列出所有会话
./pigo -c                 # 继续最近一次会话（--continue）
./pigo -r <session-id>    # 恢复指定会话（--resume）
```

### 4.4 包管理子命令

```bash
./pigo install <pkg>      # 安装扩展包（plugin / prompt / skill）
./pigo list               # 列出已安装包
./pigo update [<pkg>]     # 更新
./pigo uninstall <pkg>    # 卸载
```

安装布局：plugins → `$PIGO_HOME/plugins`，prompts → `$PIGO_HOME/commands`，
skills → `~/.agents/skills`（可用 `PIGO_SKILLS_DIR` 覆盖）。

---

## 5. 命令行参数一览

| 参数 | 说明 |
|------|------|
| `-p, --print <prompt>` | 无头模式运行 prompt |
| `-m, --model <id>` | 模型 id，默认 `openrouter/free`；知名模型名自动推断 Provider（见 6.2） |
| `--provider <name>` | 按名称选内置 Provider（用其默认端点/协议/密钥变量） |
| `-u, --base-url <url>` | 覆盖 Provider 端点（如本地 Ollama） |
| `-k, --api-key <key>` | API Key（覆盖 env/config） |
| `-P, --protocol <p>` | 强制线协议：`openai` \| `anthropic` |
| `-o, --output-format` | `text` \| `stream-json` |
| `-n, --no-tools` | 禁用内置文件/Shell 工具 |
| `--allowed-tools <names>` | 仅允许这些工具（可重复、逗号分隔、不区分大小写） |
| `--disallowed-tools <names>` | 禁用这些工具；与 allowed 冲突时 disallowed 优先 |
| `-a, --approve` | 信任当前目录：跳过首次信任提示，副作用工具免逐次确认 |
| `-C, --cwd <dir>` | 以指定目录为工作目录（文件访问/信任/项目配置均按它解析） |
| `-l, --list-sessions` | 列出会话后退出 |
| `-r, --resume <id>` | 恢复指定会话 |
| `-c, --continue` | 继续最近一次会话 |
| `--no-tui` | 用行式 REPL 代替全屏 TUI |
| `--no-skills` | 禁用 skill 发现 |
| `--no-prompt-templates` | 禁用提示词模板发现（内置斜杠命令不受影响） |
| `--system-prompt <text>` | 替换默认系统提示词 |
| `--append-system-prompt <text\|file>` | 追加系统提示词，可重复；值为已存在文件则读文件 |
| `--prompt-template <file\|dir>` | 加载提示词模板（非递归），可重复 |
| `--thinking-level <lv>` | 推理强度：`off\|minimal\|low\|medium\|high\|xhigh\|max`，默认 medium |
| `-v, --version` | 打印版本信息 |
| `--subagent-rpc` | 内部：进程隔离子代理 JSON-RPC 服务（stdio） |
| `--dream` / `--dream-dry-run` | 内部：记忆整理（consolidation）流程 |

---

## 6. 模型与 Provider

### 6.1 默认行为

默认模型 `openrouter/free`（OpenRouter 免费层），需 `OPENROUTER_API_KEY`。

### 6.2 模型名自动推断 Provider

只给 `-m` 时，知名模型名前缀自动选中对应 Provider：

| 前缀 | Provider | 前缀 | Provider |
|------|----------|------|----------|
| `claude-` | anthropic | `kimi-` / `moonshot-` | moonshotai |
| `gpt-` / `o1-` / `o3-` / `o4-` | openai | `qwen-` | dashscope |
| `gemini-` | google | `ernie-` | qianfan |
| `deepseek-` | deepseek | `doubao-` | volcengine |
| `glm-` | zai | `grok-` | xai |
| `mistral-` / `codestral-` / `devstral-` | mistral | `hunyuan-` | hunyuan |
| `minimax-` | minimax | `mimo-` | xiaomi |

歧义家族（`llama-*`、`qwq-*`、`gemma-*`、`mixtral-*`）不推断，回落到默认
OpenRouter。含 `/` 的路由式 id（如 `openai/gpt-4o`）走 OpenRouter 路由。

### 6.3 内置 Provider 清单

完整清单随时可用 `./pigo --help` 查看（"Supported --provider names" 段），
常用项：

| Provider | 密钥环境变量 | 默认端点 |
|----------|--------------|----------|
| openrouter | `OPENROUTER_API_KEY` | https://openrouter.ai/api/v1 |
| anthropic | `ANTHROPIC_OAUTH_TOKEN` / `ANTHROPIC_API_KEY` / `CLAUDE_API_KEY` | https://api.anthropic.com/v1 |
| openai | `OPENAI_API_KEY` | https://api.openai.com/v1 |
| deepseek | `DEEPSEEK_API_KEY` | https://api.deepseek.com |
| google | `GEMINI_API_KEY` / `GOOGLE_API_KEY` | https://generativelanguage.googleapis.com/v1beta |
| zai-coding-cn | `ZAI_CODING_CN_API_KEY` | https://open.bigmodel.cn/api/coding/paas/v4 |
| moonshotai-cn | `MOONSHOT_API_KEY` | https://api.moonshot.cn/v1 |
| minimax-cn | `MINIMAX_CN_API_KEY` | https://api.minimaxi.com/anthropic |
| qianfan | `QIANFAN_API_KEY` | https://qianfan.baidubce.com/v2 |
| volcengine | `ARK_API_KEY` / `VOLCENGINE_API_KEY` | https://ark.cn-beijing.volces.com/api/v3 |
| dashscope | `DASHSCOPE_API_KEY` | https://dashscope.aliyuncs.com/compatible-mode/v1 |
| hunyuan | `HUNYUAN_API_KEY` | https://api.hunyuan.cloud.tencent.com/v1 |

特殊端点（Azure OpenAI / AWS Bedrock / Google Vertex / Cloudflare）除密钥外
还需额外环境变量（如 `AZURE_OPENAI_BASE_URL`、`AWS_REGION`、
`CLOUDFLARE_ACCOUNT_ID`），见仓库根目录 `.env` 模板中的注释。

任意 Provider 都可用通用约定 `<PROVIDER名大写>_BASE_URL` 覆盖端点，
例如 `DEEPSEEK_BASE_URL`、`ZAI_CODING_CN_BASE_URL`。

### 6.4 本地 Ollama（无需密钥）

```bash
./pigo -m ollama/qwen2.5-coder:7b            # 本地默认 http://localhost:11434/v1
./pigo -m qwen2.5-coder:7b -u http://192.168.1.10:11434/v1   # 远程 Ollama
```

模型 id 带 `ollama/` 前缀、或 base URL 含 `11434` 端口时自动识别为 Ollama。

### 6.5 常用组合示例

```bash
./pigo -m deepseek-chat                                  # DeepSeek-V3
./pigo -m deepseek-reasoner                              # DeepSeek-R1
./pigo --provider deepseek -m deepseek-chat              # 显式指定 provider（等价）
./pigo -m claude-opus-4-8                                # 自动选中 Anthropic
./pigo -u https://my.gateway/v1 -P openai -k sk-... -m my-model  # 任意 OpenAI 兼容端点
```

---

## 7. 交互界面斜杠命令

REPL / TUI 内输入 `/` 触发命令（skills 与提示词模板也会注册为 `/名称` 命令）：

| 命令 | 说明 |
|------|------|
| `/exit` `/quit` | 退出 |
| `/status` | 查看当前模型 / Provider / token 用量等状态 |
| `/session` | 会话信息 |
| `/compact` | 手动压缩上下文 |
| `/rebuild` | 重建上下文 |
| `/memory` | 查看/管理持久记忆 |
| `/dream` | 触发记忆整理（consolidation） |
| `/goal` | 自主目标模式 |
| `/btw` | 侧线程（side thread）插话 |
| `/clone` `/fork` | 分叉当前会话 |
| `/tree` | 查看会话树 |
| `/rewind` | 回退到早前节点 |
| `/export` `/import` | 导出 / 导入会话 |
| `/copy` | 复制最近回答 |
| `/remote-control` | TUI 远程控制开关 |

---

## 8. 信任机制与内置工具

内置副作用工具：`read` / `write` / `edit` / `bash` / `webfetch` 等。
首次在某目录启动会触发**信任提示**；授权状态记录在
`$PIGO_HOME/trust.json`（默认 `~/.pigo/trust.json`）。

- `-a/--approve`：本次运行信任启动目录，副作用工具免逐次确认
- `--no-tools`：完全禁用内置工具
- `--allowed-tools` / `--disallowed-tools`：细粒度准入控制

---

## 9. Skills 与提示词模板

| 来源 | 位置 | 禁用开关 |
|------|------|----------|
| Skills | `~/.agents/skills/`（`PIGO_SKILLS_DIR` 可覆盖） | `--no-skills` |
| 用户提示词模板 | `~/.pigo/commands/`、`~/.pigo/prompts/` | `--no-prompt-templates` |
| 项目提示词模板 | `.pigo/prompts/` | 同上 |
| config.toml `prompts` | 配置中指定的路径 | 同上 |
| CLI 临时模板 | `--prompt-template <file|dir>` | 同上 |

模板支持 `/name` 调用、`$1` 位置参数与 `${1:-default}` 默认值语法。

---

## 10. PIGO_* 环境变量

| 变量 | 说明 |
|------|------|
| `PIGO_MODEL` | 默认模型 id（等价 `-m`） |
| `PIGO_PROVIDER` | 默认 Provider 名（等价 `--provider`） |
| `PIGO_THINKING_LEVEL` | 默认推理强度（CLI `--thinking-level` 优先） |
| `PIGO_TOOL_EXECUTION_MODE` | 工具执行模式 |
| `PIGO_HOME` | 数据目录，默认 `~/.pigo`（trust/sessions/packages 都在其下） |
| `PIGO_SKILLS_DIR` | skills 目录，默认 `~/.agents/skills` |
| `PIGO_STREAM_IDLE_TIMEOUT` | 流式响应空闲超时（如 `60s`） |

---

## 11. config.toml 配置文件

路径：`$XDG_CONFIG_HOME/pigo/config.toml`，默认 `~/.config/pigo/config.toml`。
文件缺失不报错（用默认值）；格式错误会警告但不中断。
优先级：**CLI 参数 > config.toml > 内置默认**。

```toml
model          = "deepseek-chat"
provider       = "deepseek"
# base_url     = "https://api.deepseek.com"
# api_key      = "sk-..."          # 不建议明文入库，推荐用环境变量
# protocol     = "openai"
thinking_level = "medium"
output_format  = "text"
approve        = false
no_tools       = false
no_skills      = false
# system_prompt = "你是一个……"
allowed_tools    = []               # 空 = 不限制
disallowed_tools = []               # 与 allowed 冲突时优先生效
prompts          = []               # 提示词模板路径数组

[memory]       # 持久记忆 / 无限上下文
[checkpoint]   # 检查点
[compaction]   # 上下文压缩
[dream]        # /dream 记忆整理
# enabled = true          # 缺省视为 true；显式 false 关闭自动触发
# interval_days = 7
# recent_sessions = 20
```

---

## 12. 常见问题

**Q: 编译报 `compile: version "goX" does not match go tool version "goY"`**
GOROOT 与 toolchain 版本不匹配。见 1.1 节：切换版本管理器到 go.mod 要求的
版本，或取消 `export GOROOT`。

**Q: IDE 里提示找不到依赖，但命令行 `go build` 正常**
IDE 的 gopls 继承了旧的 GOROOT 环境。修复环境后需重启语言服务器 / 重载窗口。

**Q: 报 `missing API key` / Provider 鉴权失败**
确认对应密钥环境变量已设置且非空（空值视为未设置）。`./pigo --help` 可查每个
Provider 认哪个变量；仓库 `.env` 模板列出了全部变量名。

**Q: 非终端环境（CI / 管道）报 `no prompt`**
无头环境必须提供 prompt：`./pigo -p "..."`；不带 prompt 且 stdout 非终端是
有意报错（无可运行内容也无可交互界面）。

**Q: 想看当前生效的模型 / Provider / 用量**
交互界面内输入 `/status`。
