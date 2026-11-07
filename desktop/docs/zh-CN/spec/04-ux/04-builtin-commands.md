# 04. 内置命令

> **翻译说明：** 本页与[英文源规格](/spec/04-ux/04-builtin-commands)一一对应。代码、协议字段和标识符保持原文；如翻译与英文源事实有歧义，以英文版本为准。

## 1. 目标

定义无需插件即可使用的第一方命令。它们也会出现在输入框的 `/` 菜单中。

快捷键：**Cmd/Ctrl + Shift + P**（D014）

## 2. 命令 ID 约定

```text
builtin.<domain>.<action>
```

## 3. 核心内置目录

Rust 的 `BUILTIN_ALIASES` 表是唯一事实来源，恰好包含以下命令。插件命令会动态
扩展可搜索的命令列表；应用导航、项目管理、设置、插件管理和诊断不属于内置命令契约。

| id | 标题 | 关键词 | 类别 | 风险 | 行为 |
|---|---|---|---|---|---|
| `builtin.session.new` | 新建任务 | new、chat、task | Session | 低 | 复用当前分组最新空会话，否则立即露出空首页并创建持久空会话，然后聚焦输入框 |
| `builtin.agent.btw` | 顺便一问 | btw、aside、旁路提问 | Session | 低 | 基于当前会话历史快照执行一次旁路问答，答案不写入对话记录 |
| `builtin.agent.compact` | 压缩对话上下文 | compact、context、tokens | Session | 低 | 为当前空闲会话创建模型上下文检查点 |
| `builtin.session.copy` | 复制最近回复 | copy、clipboard | Session | 低 | 将最近一条助手回复以纯文本写入系统剪贴板 |
| `builtin.session.info` | 会话信息 | session、info、tokens | Session | 低 | 在输入框浮层打开只读会话报告 |
| `builtin.app.status` | 运行时状态 | status、runtime、environment | App | 低 | 在输入框浮层打开只读运行时与环境报告 |
| `builtin.mode.agent` | 切换到 Agent | mode、agent | Session | 低 | 将空闲会话模式设为 Agent |
| `builtin.mode.plan` | 切换到 Plan | mode、plan、planning | Session | 低 | 将空闲会话模式设为 Plan |
| `builtin.mode.goal` | 切换到 Goal | mode、goal、objective、autonomous | Session | 低 | 将空闲会话模式设为 Goal |
| `builtin.session.export` | 导出会话（pigo JSONL） | export、jsonl、backup | Session | 低 | 通过原生保存对话框，把当前会话写成 pigo 兼容 JSONL |
| `builtin.agent.rebuild` | 重建对话上下文 | rebuild、reload、context | Session | 低 | 丢弃活动运行时，从 host 会话记录重建模型上下文，不开启新回合 |
| `builtin.agent.goal` | 设置或查看目标 | goal、objective、status | Session | 低 | 启动、查看或清除目标；token 预算与暂停/恢复仅 REPL 支持 |
| `builtin.session.tree` | 对话节点（tree） | tree、nodes、branch、variants | Session | 低 | 打开编号节点列表；`/tree <n>` 从节点 n 分叉出独立会话 |
| `builtin.session.rewind` | 还原点（rewind） | rewind、restore、files、rollback | Session | 中 | 打开文件编辑还原点；`/rewind <n>` 回滚已记录的文件并把对话分支到该回合之前 |
| `builtin.memory.report` | 持久记忆报告 | memory、remember、recall | Session | 低 | 打开持久记忆索引的只读报告，按 scope 列出条目数 |
| `builtin.agent.dream` | 记忆整体化（dream） | dream、memory、consolidate | Session | 中 | 用当前会话的模型跑一次记忆整体化；`/dream dry-run` 只分析不写入 |

`/session` 与 `/status` 只读取渲染器已有状态：不发起 host RPC，也不写入会话。
展示的是 store 中已有的数值，不是为报告重新估算的结果。`/memory` 是例外——记忆
索引属于 sidecar，卡片会发起一次只读的 `memory.report` 调用并渲染其结果。

## 4. 可见性与执行规则

- 上表 ID 构成完整的第一方 registry。移除的 ID 不会出现在命令面板结果中，
  也不再有渲染器 dispatch case；插件命令仍可独立发现。
- `New Task` 使用当前项目或临时分组。若该组最新会话为空则选中复用；否则
  在第一帧露出空首页并创建持久空会话。同一分组内该操作是幂等的。
- `Compact Conversation Context` 在当前会话空闲时可用；活动回合或检查点期间
  继续遵循既有的忙碌与压缩契约。
- 模式命令使用与 Composer Agent/Plan/Goal 芯片相同的活动会话配置路径，立即更新
  空闲会话。没有活动会话时，它们更新下一个会话使用的持久化默认值；正在运行的会话
  或待审批会话不会被修改。
- `SubmitPlan` 和 `SubmitGoal` 是模型工具，不是命令面板命令。没有 Chat 模式或
  request-changes 别名。
- 原有的应用、项目、设置、插件和日志操作（仍适用时）通过各自的专用界面提供，
  但不属于命令面板或输入框的内置命名空间。

## 5. 执行结果

命令返回：

```ts
type CommandExecutionResult =
  | { ok: true; navigation?: string; message?: string }
  | { ok: false; error: AppError }
```

## 6. 验收

1. 内置 registry 恰好包含上表列出的唯一且带前缀的 ID，ID 与斜杠别名均无重复。
2. 命令面板搜索可匹配每条命令的标题和关键词。
3. 模式切换命令立即更新空闲会话模式；Plan、Goal 和 Agent 使用同一个 pi Agent。
4. Compact 在空闲时可用，在活动回合/检查点期间返回 `AGENT_BUSY`。
5. 被移除的 ID 以及遗留的 `newChat`、`openProject`、`openSettings` dispatch 别名，
   不会出现在 registry 或渲染器 switch 中。

## 7. Composer 斜杠别名（D123、ADR 0024、ADR 0106）

内置命令通过短别名显示在输入框 `/` 菜单中。别名定义在 Rust 的
`BUILTIN_ALIASES` 表（`src-tauri/src/composer.rs`）中。每行同时声明 `argMode`
（`none` / `optional` / `required`），并作为 `ComposerCommand.argMode` 下发给
渲染器；`dispatchComposerCommand` 依据该元数据分派，而不是匹配命令 id。因此
新增一条无参内置命令只需增加一个表项和一个 palette case，无需改动输入框提交钩子。

| 别名 | 命令面板 ID | argMode |
|---|---|---|
| `/new` | `builtin.session.new` | none |
| `/btw` | `builtin.agent.btw` | required |
| `/compact` | `builtin.agent.compact` | none |
| `/copy` | `builtin.session.copy` | none |
| `/session` | `builtin.session.info` | none |
| `/status` | `builtin.app.status` | none |
| `/agent-mode` | `builtin.mode.agent` | optional |
| `/plan-mode` | `builtin.mode.plan` | optional |
| `/goal-mode` | `builtin.mode.goal` | optional |
| `/export` | `builtin.session.export` | none |
| `/rebuild` | `builtin.agent.rebuild` | none |
| `/goal` | `builtin.agent.goal` | optional |
| `/tree` | `builtin.session.tree` | optional |
| `/rewind` | `builtin.session.rewind` | optional |
| `/memory` | `builtin.memory.report` | none |
| `/dream` | `builtin.agent.dream` | optional |

### 会话导出 / 重建 / 目标

- `/export` 向 host-core 取完整会话详情，写出 pigo 的 `SessionHeader` +
  `entryWire` JSONL（schema v3、线性 `parentId` 链、只保留可读的
  `user`/`assistant` 行）。产物既能被本桌面端 `importer::parse_pi` 导入，也能被
  CLI 续接。取消保存对话框按非错误路径提示。
- `/rebuild` 是 sidecar 侧操作：host 会话记录才是事实来源，所以该命令销毁活动
  `AgentContext` 并通过 `Manager.Rebuild` 重建（压缩检查点、工具目录、provider
  绑定）。活动回合期间返回 `AGENT_BUSY`；没有活动运行时无需处理，下一条提示词
  会自动新建。
- `/goal <目标>` 切到 Goal 模式并发送目标；`/goal status` 报告当前模式与规划阶段；
  `/goal clear` 回到 Agent 模式。桌面端运行时**只执行一轮已批准的目标**，没有
  REPL 的自主循环，因此 `pause`、`resume`、`--tokens` 会明确提示不支持，而不是
  伪装成可用。

### 持久记忆 /memory 与记忆整体化 /dream

`/memory` 打开一张只读卡片，报告 pigo 记忆库的状态：是否启用、解析出的根目录
（`$PIGO_HOME/memory`，否则 `~/.pigo/memory`）、SQLite 索引路径、按 scope 的条目
数，以及计数前执行 reconcile 的增量。sidecar 会先 reconcile（与 CLI 一致），所以
条目数描述的是磁盘上的 Markdown 文件，而不是过期索引。索引打不开时错误**写在卡片
里**，而不是让请求失败——"索引打不开"本身就是用户需要的诊断。

与 CLI 的 `/memory` 不同，这张卡片没有 checkpoint 段：桌面端没有 CLI 那套自动
checkpoint 流程，上下文数字来自渲染器自己的压缩记录与最近一次用量上报。

`/dream` 跑一次记忆整体化，并回报 CLI 的变更计数（merged / deduped / paths
cleaned / pruned / distilled / bytes / files）。三条边界是刻意保留的：

- **进程内执行，不另起子进程。** CLI 是 spawn `pigo --dream`；桌面端没有这个二进制
  可 spawn，所以 sidecar 直接跑 `dream.Runner`，用单飞闩锁挡住并发。同一时刻的第二次
  请求返回 `DREAM_BUSY`，而不是悄悄退化成磁盘锁的 "skipped" 空转。dream 仍然自建模型
  客户端，且从不触碰活动会话的 `AgentContext`。
- **用会话自己的模型。** 走会话的 launch binding（provider、model、凭证），而不是
  另配一套。
- **桌面会话就是蒸馏来源。** 对话在 host-core 的 SQLite 里，所以 sidecar 把
  `session.list` / `session.get` 适配成 runner 的 `SessionSource`；没有这层适配，
  整体化只能看到 pigo 的文件会话库（桌面端从不写入它），`distilled` 会永远是 0。
  子代理行与工具结果行不进入蒸馏文本。

`/dream dry-run`（也接受 `--dry-run`、`-n`）只分析并报告，不写文件、不更新 dream
状态，与 CLI 参数一致。命令需要活动会话与带凭证的 provider，两种情况都按 toast 提示，
而不是返回一份空报告。

### 对话节点 /tree 与还原点 /rewind

`/tree` 分两层，因为桌面端存储的是两种不同的东西：

**分叉谱系（会话层，真实树）**。一个分支就是一个真实会话，所以谱系属于元数据而非
消息结构。schema v22 在被分叉的会话上记录 `parent_session_id` 与
`forked_from_message_id`，`session.fork` 用它在内存中已经知道的源会话与切点写入这
两列。`/tree` 渲染当前会话所属的那棵树 —— 它的祖先链加上挂在上面的每个分支
—— 缩进、根在前、当前会话带「当前」标记、记录的父会话已删除时带「源会话已删除」
标记 —— 点击某一行即切换到该分支的会话。无关会话留在侧栏，这个面板是分支视图，
不是第二个会话列表。v22 之前创建的
会话与所有根会话都没有父节点，一律按根渲染；列可为空，不需要回填历史数据。

**transcript 节点（消息层，线性）**。会话记录仍是 host 持有的线性 JSONL，message
没有父节点指针（ADR 0023），所以谱系下方的节点列表是当前会话的顺序编号，不是另一
张分支图：

- `/tree` 读取完整 `session.get` transcript，打开编号节点列表（角色、单行摘要、
  工具名、subagent 嵌套）。携带 regenerate 变体族（`revisionRootId` /
  `revisionCount` / `activeRevision`）的用户回合提供变体选择器，底层走
  `session.activateRevision`。
- `/tree <n>`（或「从此处分支」按钮）以 `throughMessageId` = 节点 n 调用
  `session.fork` 并激活副本。这是桌面端对 REPL「切换 active leaf」的等价实现：
  原会话不被修改。与 REPL 不同的是分支是一个独立会话，因此子会话记录自己的来源，
  而不是共用同一棵树。
- `/rewind` 按从旧到新列出还原点。一个还原点即某个产生了至少一条消息级 review
  快照（ADR 0043）的用户回合，也就是带 `details.review` 的 `Write` / `Edit`
  工具结果 —— 与 REPL `FileSnapshotRecorder` 记录的文件变更同源。
- `/rewind <n>` 会把第 n 个点及其后所有 active 变更**由新到旧**逐条回滚
  （`review.rollback` 在文件 post-tool hash 不匹配时拒绝覆盖），再把对话分叉到该
  回合之前的那条消息。冲突、`unavailable` 与已回滚的条目都计为跳过，绝不计入
  已恢复。首个回合没有前置节点，因此只恢复文件、对话保持不变。
- **范围边界**：Bash/shell 副作用、scratch 写入与没有结构化 `path` 的工具不在
  快照范围内，不会被恢复；也没有会话级文件 checkpoint 或 undo stack。这与
  REPL rewind v1 的范围一致，面板中会明确写出。

`required` 别名不带参数时由本地消费并给出提示，不会作为普通提示词落到主对话。
`/btw <question>` 打开旁路线程；`/copy`、`/session`、`/status` 不接受参数。

别名与模板和插件命令名称共享一个命名空间；冲突时优先使用内置别名，其次是项目模板、
用户模板和插件命令。选择别名会插入 `/alias `；单独发送 `/new` 或 `/compact` 时，
会在本地执行，不会创建空提示。Agent/Plan/Goal 别名支持附带提示词：
`/agent-mode <prompt>`、`/plan-mode <prompt>` 或 `/goal-mode <prompt>` 会切换空闲会话
（或下一个会话默认值），并通过正常提示路径发送 `<prompt>`。不带正文的模式别名仍然
只执行本地切换。提示正文仍是可见的用户回合；dispatch 失败不会清除输入框草稿。
旧的内置别名不再解析，除非由其他命令来源提供，否则会作为普通未知斜杠文本处理。

## 8. 输入框中的 Skill 项

当前激活的内置、插件和用户 Skill 也会显示在输入框的 `/` 菜单中。它们使用精确的
Skill ID 作为斜杠名称，显示 Skill 名称和描述，并单独归入 **技能** 分组，排在扩展
命令之后。该分组始终位于最后；Skill 不会覆盖同名命令或模板。

选择 Skill 会插入 `/<skill-id> `。发送 `/<skill-id>`（可附带提示正文）时，输入的命令
会以可见的消息芯片保留，同时要求模型先用经过校验的 ID 调用现有的 `Skill` 工具，再
回答请求。只有当前项目已激活的 Skill 会被列出或接受，因此项目范围和插件激活状态
仍会在发送时强制校验。如果 Skill 已不再激活，文本会按普通未知斜杠提示处理。

## 9. 顿号打开斜杠菜单（D405）

中文输入法会用顿号「、」（U+3001）代替 ASCII 的 `/`，否则用户必须在书写中途切换
输入法才能唤出菜单。当输入框为空时，第 1 个字符提交的「、」会在触发检测之前被改写
为 `/`，随后按上文相同的插入、过滤与发送行为打开普通斜杠菜单。

只有空草稿的第 1 个字符会被改写。出现在草稿其他位置的「、」属于普通标点，始终不会
被改动；该别名对 `@` 文件菜单没有影响。
