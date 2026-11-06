# 20. 员工（Staff Employees）

- 决策：ADR 0252
- 存储 schema：v17
- 归属：Rust host-core
- 英文源规格：[英文源规格](/spec/03-runtime/20-staff-employees)

英文页面是规范源。本页保留表名、列名、方法名、错误码和代码结构，便于
中文读者检索；实现必须以英文规范中的完整定义为准。

## 1. 目标

让一组长期存在的员工按各自的排期无人值守地运行，每个员工有自己的凭据
档案（credential profile）和模型绑定，并通过带来源信息的记忆记录彼此共享
所得，而不是共享 transcript。

## 2. 归属

| 关注点 | 归属 |
|---|---|
| 员工 / 档案 / 运行 / 记忆行 | Rust host-core（SQLite） |
| 凭据材料 | `SecretStore`（见 `14-secrets-storage.md`） |
| 排期执行 | host-core 只做批量领取；常驻的 Electron main 驱动把每次领取变成一个会话（§12） |
| agent 执行 | agent sidecar，一次运行一个会话 |
| 员工列表、工作流编辑器、记忆审阅 | renderer |

renderer 永远看不到凭据值，也不直接写记忆。

## 3. 数据模型

```sql
CREATE TABLE credential_profiles (
  id            TEXT PRIMARY KEY,
  label         TEXT NOT NULL,
  provider_id   TEXT NOT NULL REFERENCES providers(id) ON DELETE RESTRICT,
  secret_ref    TEXT NOT NULL,
  base_url      TEXT,
  config_json   TEXT NOT NULL DEFAULT '{}',
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL
);

CREATE TABLE employees (
  id                    TEXT PRIMARY KEY,
  name                  TEXT NOT NULL,
  role                  TEXT NOT NULL DEFAULT '',
  credential_profile_id TEXT REFERENCES credential_profiles(id) ON DELETE RESTRICT,
  provider_id           TEXT,
  model_id              TEXT,
  thinking_level        TEXT,
  workflow_id           TEXT,
  workflow_version      TEXT,
  status                TEXT NOT NULL DEFAULT 'paused',   -- paused | running | stopped
  quota_json            TEXT NOT NULL DEFAULT '{}',
  created_at            INTEGER NOT NULL,
  updated_at            INTEGER NOT NULL
);

CREATE TABLE employee_runs (
  id                    TEXT PRIMARY KEY,
  employee_id           TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
  session_id            TEXT REFERENCES sessions(id) ON DELETE SET NULL,
  credential_profile_id TEXT,
  provider_id           TEXT,
  model_id              TEXT,
  trigger               TEXT NOT NULL DEFAULT 'schedule',  -- schedule | manual | event
  status                TEXT NOT NULL DEFAULT 'running',
  error_code            TEXT,
  started_at            INTEGER NOT NULL,
  ended_at              INTEGER
);

CREATE TABLE memory_items (
  id           TEXT PRIMARY KEY,
  scope        TEXT NOT NULL,                     -- employee | shared
  owner_id     TEXT,                              -- employee id when scope = employee
  kind         TEXT NOT NULL,                     -- fact | preference | playbook | constraint | signal
  key          TEXT NOT NULL DEFAULT '',          -- contention / dedupe key
  text         TEXT NOT NULL,
  source_kind  TEXT NOT NULL,                     -- user | interaction | run | url
  source_ref   TEXT NOT NULL DEFAULT '',
  confidence   REAL NOT NULL DEFAULT 0.5,
  status       TEXT NOT NULL DEFAULT 'active',    -- active | superseded | retired
  supersedes_id TEXT,
  employee_id  TEXT,
  run_id       TEXT,
  created_at   INTEGER NOT NULL,
  expires_at   INTEGER
);
```

凭据是引用而不是内联：`profile -> secret_ref -> SecretStore`。`secrets_meta`
记录 `owner_kind = 'credential_profile'` 与 `owner_id = <profile id>`。

最后一张表的物理列名是 `memory_key` 和 `content`；`key` 和 `text` 是线上
（wire）名称，这样 SQL 不必使用看起来像关键字的名称，而响应仍然读作
`key` / `text`。

## 4. 凭据解析

一次运行按以下顺序解析凭据，失败即关闭（fail closed）：

1. `employee.credential_profile_id` -> 档案行。
2. 档案的 `secret_ref` -> `secrets.getForRuntime`（有审计、临时性）。
3. 档案的 `base_url` 在设置时覆盖 provider 的 base URL。
4. 缺少密钥 -> `PROVIDER_SECRET_MISSING`；该次运行不启动。

没有档案的员工回退到 provider 自己的密钥，因此已有的单账号配置无需改动。

## 5. 记忆语义

### 5.1 类别与写入权限

| kind | 含义 | 谁可以写入 |
|---|---|---|
| `fact` | 可观察且有来源 | 任何运行，但必须提供 `source_ref` |
| `preference` | 用户偏好 | 任何运行 |
| `playbook` | 被验证有效的打法 | 只能由记忆闸门写入 |
| `signal` | 一个观察到的兴趣信号 | 任何运行 |
| `constraint` | 限制或禁令 | 只能是用户 |

host 会拒绝由员工写入的 `constraint`。员工不得放宽自己的限制；无人值守
运行让这条从策略变成硬边界。同一条边界也覆盖删除：除非调用能表明自己是
用户，否则拒绝把 `constraint` 置为 retired，因为删掉一条限制就是放宽它。

`playbook` 只能由闸门写入，因此即使在自己的作用域里，运行也不能自行产生
一条 playbook。

### 5.2 作用域与晋升

- 运行直接写入 `scope = 'employee'`，归属自己的 employee id；完全不能写
  `scope = 'shared'`。
- 发布到 `scope = 'shared'` 必须通过工作流的记忆闸门。闸门是确定性代码，
  按顺序判断三件事：
  1. 非 `user` 作者必须提供 `source_ref`；
  2. 文本已经存在的条目视为重复（比较前先去除首尾空白、转小写、折叠空白），
     予以拒绝；
  3. 一条活跃的 `constraint` 拥有它的非空 `key`：其他任何 kind 都不得发布
     在该 key 之下。矛盾在语义上无法判定，因此这条规则是结构性的而非解释性的。
- `source_kind = 'user'` 可以绕过闸门。这正是运行范围的写入不得自称用户的原因：
  当请求带有 `runId` 时，`staff.*` 会拒绝 `sourceKind = "user"`。
- 检索只返回 `status = 'active'` 且未超过 `expires_at` 的条目，先按置信度、
  再按时间排序，并受调用方给定的上限约束。
- 具有相同非空 `key` 而 `text` 不同的两条活跃条目会同时返回，并都标记为
  `contested`。上面的闸门是故意放它们进来的：它拒绝的是同一条主张出现两次，
  而不是两个并存的分歧。

### 5.3 仅追加

更新永不修改原行。替换会插入新行，原行转为 `status = 'superseded'`，其
`supersedes_id` 指向新行，因此任何答案都能追溯到产生它的那次运行。

## 6. 生命周期

| 状态 | 含义 |
|---|---|
| `paused` | 已存在但未排期；新建员工的默认状态 |
| `running` | 可被排期 |
| `stopped` | 被闸门或用户停止；需要显式重新启动 |

`employee_runs` 沿用 `task_runs` 的约定：一次运行在结束、失败或 host 重启
之前都是 `running`。启动时把残留的 `running` 运行标记为 `aborted`，与现有
的启动维护一致。

## 7. RPC 面

`staff.*` 由 host-core 的 `rpc/mod.rs` 分发到 `staff::handle`；整个面对外
只有这一处入口，因此新增一个方法只需改一个地方。

| 方法 | 用途 |
|---|---|
| `staff.profiles.list` / `staff.profile.create` / `staff.profile.update` / `staff.profile.delete` | 凭据档案 |
| `staff.profile.setSecret` | 把档案的 API key 写入 `SecretStore`；唯一接受密钥材料的调用 |
| `staff.employee.list` / `staff.employee.get` / `staff.employee.create` / `staff.employee.update` / `staff.employee.delete` | 员工名册 |
| `staff.employee.runs` | 某个员工或全部员工的近期运行 |
| `staff.memory.write` / `staff.memory.publish` | 私有写入，以及经闸门晋升到共享 |
| `staff.memory.query` / `staff.memory.supersede` / `staff.memory.retire` | 读取与仅追加替换；带 `runId` 的调用身份是运行，不带的身份是用户 |
| `staff.workflow.list` / `staff.workflow.get` / `staff.workflow.create` / `staff.workflow.update` / `staff.workflow.versions` / `staff.workflow.delete` | 带版本的工作流图；`create` 以及携带 `graph` 的 `update` 会写入一个新的不可变版本 |
| `staff.workflow.validate` | 解析并编译一张候选图，但不保存 |
| `staff.employee.refreshSchedule` | 只重算某个员工的 `next_run_at`，不动它的其他列 |
| `staff.run.start` / `staff.run.finish` / `staff.run.step` | 开启一次运行、以状态收尾、推进其步骤计数 |
| `staff.run.binding` | 解析某次运行冻结的凭据、provider、模型、思考等级与端点；唯一携带密钥材料的响应 |
| `staff.scheduler.claim` | 一次调用同时推进到期的排期并开启其会话；返回 `{ claims, skipped }` |
| `staff.presets.list` | 全新控制台可直接雇佣的内置蓝图，各带它将安装的工作流图与排期 |
| `staff.presets.install` | 按名称雇佣给定蓝图，共用同一个凭据档案；以员工名去重，可重复调用 |

凭据值是只写的：没有任何响应携带它，档案只暴露 `hasSecret` 和 `secretRef`。
为一次运行解析密钥通过 `resolve_run_binding` 在进程内完成，其结果既不是
`Serialize` 也不是 `Debug`。`staff.run.binding` 是唯一的例外，§12 说明了
它为什么可以在不把密钥交给 renderer 的前提下存在。

## 8. 失败模式

| 情况 | 行为 |
|---|---|
| 缺少密钥 | 运行被拒绝，错误码 `PROVIDER_SECRET_MISSING` |
| 档案仍被引用时删除 | 由 `ON DELETE RESTRICT` 拒绝 |
| provider 仍被档案引用时删除 | 拒绝 |
| 未知的 kind 或 scope | 拒绝写入，不插入任何行 |
| 员工写入 constraint | 拒绝写入 |
| `fact` 缺少 `source_ref` | 拒绝写入，除非由闸门晋升 |

## 9. 验收标准

七项均由 `crates/host-core/src/staff/tests.rs` 覆盖。

- [x] 同一个应用里两个员工可以跑在不同的凭据档案和模型上，且每次运行都记录
      是哪个档案付费
- [x] 没有任何 RPC 响应、日志行或员工行包含密钥材料
- [x] 员工无法写入 `constraint`
- [x] 共享条目在没有来源引用时无法写入
- [x] 替换条目会保留原行并建立双向链接
- [x] 检索永不返回 superseded、retired 或已过期的条目
- [x] v16 数据库迁移到 v17 时不改动任何既有行

## 10. 工作流图契约

工作流是带版本的图。`staff.workflow.create` 写入版本 1；
`staff.workflow.update` 在携带 `graph` 时写入下一个版本，不携带时只修改
name、description 和 status。版本不可变，运行在被领取时就锁定
`(workflow_id, workflow_version)`，因此编辑图永远不会改变正在运行或历史运行
已经做过的事。

编辑器的绘制与保存是同一张图。`position` 是随版本一起走的视图状态，重新
打开旧版本会恢复它当时的形状。唯一的例外是仍然停在最初蓝图那一排上的图：
那一排是机器摆的而不是业主摆的，因此打开时会重新布局，落到「自动布局」按钮
产出的分带上。唯一的例外是仍然停在最初蓝图那一排上的图：
那一排是机器摆的而不是业主摆的，因此打开时会重新布局，落到「自动布局」按钮
产出的分带上。

| 规则 | 由谁强制 | 编辑器行为 |
|---|---|---|
| 恰好一个 `trigger` | host | 第二个 trigger 在画出来之前就被拒绝 |
| 每个节点都从 trigger 可达 | host | 校验时报告 |
| 无环 | host | 校验时报告 |
| 各 kind 的必填 config | host | 渲染的正是该 kind 的字段 |
| 节点 id 只能是字母数字、`-` 或 `_` | host | 生成的 id 已经满足 |
| 最多 200 个节点、600 条边 | host | 调色板不再提供节点 |

封闭的 kind 集合是：

```text
trigger | probe | analyze | produce | agent | memory | condition | handoff | humanGate | notify
```

`staff.workflow.validate` 运行同一套解析器与编译器但不保存，返回
`{ valid, error?, nodeCount?, steps? }`，因此编辑器能在用户确认之前解释拒绝原因。

`probe` 节点只巡一个封闭的渠道集合：`juejin`、`zhihu`、`bilibili`、
`wechatMp`、`wechatChannels`，以 `sources` 数组携带。命名其他渠道的图在解析时即被
拒绝；而仍用单个 `channel` 字符串的旧图继续可编译，因为解析器把它读成只含一项的列表。

`memory` 节点只能命名四种操作之一，校验器接受的、编译器产出的都是这一份清单：

```text
read | write | publish | retire
```

`read` 由驱动自己作答：它去读记忆库，把读到的内容交给下一步，不花模型轮次。
一轮里可以有多个 read 节点，读到的内容会累加：既要读话术、又要读该去哪儿投放的员工，
两样都会拿到手上。若用后一次覆盖前一次，等于把材料悄悄砍掉一半——种子里的地推员正是
每轮都读到了话术却报「投放 0 条、未提供候选位置」，直到第二次读不再冲掉第一次。
`write` 与 `publish` 把该步自身的结果记下来，分别写入员工作用域与共享作用域。
解析时仍接受 `query` 作为 `read` 的旧写法并编译成 `read`：驱动一度只认这个词，而
控制台写的是 `read`，于是命名 `read` 的图白花一轮模型调用，还把那一轮的正文当成
信号记了下来（运行 6e6e5672）。`retire` 目前没有驱动实现，因此 retire 节点会跑
一轮模型调用且不记录任何内容。

## 11. 排期语义

| 字段 | 含义 |
|---|---|
| `everyMinutes` | 固定间隔网格，5..1440；锚点是员工的 `created_at`，因此重启不会让网格漂移 |
| `timesOfDay` | `HH:MM` 锚点，最多 24 个，会排序去重；与 `everyMinutes` 互斥 |
| `jitterMinutes` | 由员工 id 与槽位派生出的确定性偏移，0..1440，使两个同节奏的员工不会同时触发 |
| `utcOffsetMinutes` | 读取锚点所用的时区 |
| `maxRunsPerDay` | 每日上限，默认 96 |

```json
{ "everyMinutes": 30, "jitterMinutes": 5, "utcOffsetMinutes": 480, "quota": { "maxRunsPerDay": 48 } }
```

失败的运行会按 5 分钟到 6 小时指数退避，而不是在自己的网格上重试；需要排期的
员工必须锁定工作流版本：一次悄悄跟随最新修订的无人值守运行，会执行没有人批准过的东西。

## 12. 常驻

host-core 保持无头。它只负责批量、状态与恢复，从不打开会话。常驻驱动位于
桌面壳（Tauri 移植里的 `staff_driver.rs`），因为会话是在那里启动的；它按定时器
轮询，不依赖任何窗口是否打开。壳通过 `staff.run.binding` 解析每一步的回合：
运行自己的密钥与端点会覆盖 provider 行，因此凭据档案可以不同于它点名的行。

站点采集仍归站点采集器所有。在该界面移植之前，probe 步骤拿到的是规范里的
空输入行（`- (nothing: no memory item and no collected page reached this step)`），
运行如实记录"这一轮什么都没采到"，而不是自己去抓取。

`staff.scheduler.claim` 刻意做成一次调用：它在同一个事务里为每个到期员工推进
`next_run_at` 并开启会话，因此两个轮询者不可能启动同一个槽位。它返回
`{ claims, skipped }`，被跳过的员工会带上把它挡在这一批之外的原因。

凭据通过 `staff.run.binding` 交给该驱动，它解析运行的档案、provider、模型、
思考等级、Base URL 与密钥。它之所以是 RPC，是因为驱动与 host-core 是两个进程；
它刻意不在 renderer 的 IPC 白名单里：桌面桥接是逐条枚举 `staff.*` 通道的，
所以名单里没有它，本身就是强制手段。

每一步的结果是从它的回复里读出来的，而不是猜出来的：驱动取**以 `STEP RESULT:`
开头**的最后一行。这两条都承重。某一步可能在解释工作流时引用这个标记，而"全文
第一个匹配"曾把这种句子的后半截当成信号发布进了共享记忆（运行 6e6e5672）。

每一步的 prompt 还会写明这一步拿到了什么，拿不到时也照写：空的记忆读取、以及
什么都没采到的探针，都会写成 `- (nothing: no memory item and no collected page
reached this step)`。该步被要求只依据这些材料、不依赖这台机器上的其他东西，并被告知
"说明缺什么然后停下就是一个完整的答复"。没有这条边界，一个没拿到输入的步骤只剩下
"继续做"的许可：运行 ac6038f1 的第 2 步既没有记忆条目也没有采集页，于是它去读不相干
会话留下的 scratch 文件——花了九分半钟、148k 输出 token，产出员工自己从未采集过的
"证据"。

已结束步骤的那一行结论也会传给它之后的步骤，写在 `What this run has already done:`
下面。没有它，图就只是几个陌生人排在一起：本该记录"团队学到什么"的那个节点，除了
自己的叙述无可记录；收尾的那一步也说不出这轮到底做了什么。只读的 `memory` 节点不
向这张清单贡献内容，因为驱动不花模型轮次就把它答完了。

会记录的节点要被要求交另一种结尾。它回复的最后一行就是那条记忆，会被后来的人原样读
回去，因此它被要求把那一行写成结论本身，而不是关于这一步的说明。之前对所有步骤统一要
"实际发生了什么"，收上来的就正是这个：侦察员存的是"发布了共享信号一条 —— 本轮无可用
商机……"，选题员存的是一个文件名加"未读机上任何文件"，而它们下一轮都被当作"团队已知"
交回给模型。这段措辞放在 prompt 生成器里而不是每张图里，因此业主以后自己拖出来的节点也
受同一个约定约束。


## 13. 运营界面

渲染层是运营台，不是第二个控制面：它提供的每个动作都走驱动所用的同一批
`staff.*` RPC，它自己不持有数据库里没有的状态。

入口是侧边栏的 **我的应用 → AI 团队** 一行，它位于侧边栏底部区域而不是命令面板
后面，因此常驻状态不需要搜索就能看到。该行带实时徽标：正在运行的运行数，或一个
圆点，颜色在驱动健康时为绿、出现排期错误后为琥珀、驱动未常驻时为红。

默认页签围绕无人值守系统必须回答的第二个问题构建：现在有没有东西在等主人？
它读取的正是运行日志所用的同一份运行历史，而已经恢复正常的员工会被刻意略过，因为
一个不断追问已解决问题控制台，就是一个没人会看的控制台。

页面每五秒轮询 `staff.scheduler.status`，侧边栏徽标每二十秒轮询，两者同时订阅
`pi-desktop/staff/event/changed` 事件，这就是无人值守的运行不需要刷新任何东西
也能被看见的原因。

| 页签 | 做什么 |
|---|---|
| 总览 | 默认页签：今日运行与成功率，然后是所有需要人来的事——调度器故障、未按排期运行的员工，以及最新一次运行失败或停在闸门上的员工 |
| 员工 | 名册、凭据档案、provider、模型、思考等级、工作流与锁定版本、排期、配额 |
| 工作流 | 节点图、各 kind 参数、YAML、校验、版本历史 |
| 记忆 | 带过滤的浏览、写入、晋升为共享、退役、contested 标记 |
| 密钥与模型 | 凭据档案与只写密钥 |
| 运行日志 | 每个员工的运行、结果、错误，以及跳进会话 |

有一项检查端到端地把边界守住，它刻意不在单元测试套件里，因为它需要一个真实的
host 进程：

```text
node apps/desktop/test/manual/staff-wire-smoke.mjs
```

它用 NDJSON 驱动真实 host，运行在临时数据目录上，会等待一个排期槽位到期，并在
除 `staff.run.binding` 之外的任何回复携带密钥材料时失败。

## 14. 预设员工

空名册不是一个起点，因此全新控制台直接提供内置名册。头两个是整条漏斗赖以立住的一对，其余五个把一个选题一路带到成交：

| 预设 id | 名称 | 职责 |
|---|---|---|
| `scout` | 侦察员 | 外勤：巡目标社区，留下与产品信号相符的内容，把原始线索交给选题员 |
| `interestAnalyst` | 选题员 | 内勤：读共享结论，判断哪些帖子露出真实兴趣，再把结论写回共享记忆供写手使用 |
| `writer` | 笔杆子 | 内勤：把选题写成软文、短帖、视频脚本，每次交两三个开头让业主挑 |
| `poster` | 地推员 | 外勤：把定稿的话术带到当初提问的地方，每次投放一版做 A/B，并记下每条投给了谁 |
| `receptionist` | 接线员 | 内勤：接住来问的人，问出只有他本人能回答的问题，给出意向分级，把值得跟的作为建议交出去 |
| `closer` | 跟单员 | 内勤：把已经有意向的对话推进到试用、报价、合同与打款，每一步的卡点都留下记录 |
| `aftersales` | 售后员 | 内勤：交付之后接着跟，收集客户的原话，挖复购与转介绍 |

名称面向产品，保持中文；id 才是稳定键。整条链是通过看板交接的，而不是员工之间互相
调用：写手不会给地推员发消息，两个人读的是同一批行。这正是无人值守的流水线还能查得清
的原因——投放出了问题，写下「该写什么」的那一行，和写下「将由谁来说」的那一行，都还在。

一次运行只归档，不做决定。`produce` 步骤的配置里写了
`artifact: leads | content | outreach | advice` 时，它的提示词末尾会带上一段契约，要求
输出一个围栏 JSON 块，驱动解析出来的东西就是它归档的内容：线索落在 `new` 阶段，草稿
无论请求的是什么状态都落在 `draft` 或 `review`，建议则落在指定看板上并附上这次运行
自己的引用。解析不出来的回复什么都不归档，只记一次解析失败——半解析出来的卡片，等于
替一个真实的人说了他从没说过的话。

`staff.presets.list` 返回每个蓝图的名册条目——id、名称、职责、工作流名与描述、渠道、
排期、配额——以及同名员工是否已经雇用。它**不返回图**：安装写下的就是这个名册条目，
图来自蓝图自己。`staff.presets.install` 接收预设 id 与一个凭据档案，
在一个事务里完成雇佣：

- 员工名已存在的会被跳过、绝不重复，因此每次启动都能安全调用
- 两个员工都解析到业主自己的凭据档案；密钥在档案里，员工行永远没有
- 解析不到密钥时仍写入名册条目，但保持暂停，未配置的应用不会启动付不起的运行
- 蓝图改了图不会被重新应用到已经雇用的员工：运行复现的是被钉住的那个工作流版本。
  把既有员工换到新图只有一条路：带 `adoptBlueprint` 调用 `install`，它把本版本自带的
  那份蓝图**追加**成一个新版本，而不是就地改掉——员工原本在跑的那张图之后仍然读得到。
  `staff.presets.list` 会为每个蓝图报出 `blueprintChanged`，控制台据此提示，业主不需要
  自己手动 diff 两张图。

两个蓝图之间是靠共享记忆连起来的，而不是互相调用：侦察员的 `file` 节点把它
「判商机」判出来的需求信号发布出去，选题员的第一步就用同样的作用域、同样的
类别把它读回来。在侦察员有这个节点之前，它发现的东西从不离开自己那一次运行——
选题员读的是一个从来没有员工写过的库，而里面唯一一条还是它自己 publish 写进去的。
负责「存下判断」的节点还必须说清楚要压缩什么：`publish` 节点记录的是它自己那一步的
结论，没有 instruction 它就只能拿自己的叙述来交差，而第一轮存下来的正是这个东西。

侦察员每天巡两轮，选题员在它的第二次巡社区之后每天跑一次。两者都带
`maxRunsPerDay` 上限，卡住的工作流烧不掉业主的配额。

侦察员的巡探步骤只列出已经有采集器的站点——今天是知乎，其余五个渠道在等各自的
适配器。这份清单来自站点注册表，步骤也围绕它来写：巡探把驱动读到的内容交给员工，
所以列上一个没人能读的站点，只会把员工支去手工抓取，而站点回给它的正是当初挡住第
一轮的那堵登录墙。职责没有变，覆盖范围是一个站点一个站点长出来的。

名册是按名字雇的，所以改过名或删过员工的业主，那个决定会被保留：安装一个名字——
或者它曾经用过的名字——已经在名册上的蓝图时，是跳过，而不是再雇一个。

桌面端在设置了 `PI_STAFF_BOOTSTRAP_API_KEY`、`PI_STAFF_BOOTSTRAP_BASE_URL`、`PI_STAFF_BOOTSTRAP_MODEL_ID` 时从它们读取，
否则读取数据目录下的 `staff-bootstrap.json`。该文件每次安装只读一次，内容既不进 renderer 也不进日志。

## 15. 站点会话

巡检要读的站点把内容放在登录之后，因此控制台为每个站点保存一份登录态，员工
通过它来读取。

这道门是实测出来的，不是假设的。2026-09-20 用一个全新的 Chromium 配置访问
`https://www.zhihu.com/hot`：

- 请求落在 `/signin?next=%2Fhot`，拿到的是登录墙而不是列表；
- 匿名会话被下发了 `_xsrf`、`__zse_ck`、`_zap`、`d_c0`、`SESSIONID`、
  `JOID` 以及百度统计计数，这些都不能解锁内容；
- 真正登录后才有的 `z_c0` 不存在；
- JSON 接口在页面自己的源上就失败：
  `/api/v3/feed/topstory/hot-lists/total` 返回 `401`，
  `/api/v4/questions/<id>/answers` 返回 `403`，因为两者都要求只有站点自己的
  前端才能算出的 `x-zse-96` 签名。

因此采集器不伪造该签名、不破解验证码、不做指纹伪装。它在站点自己的分区里加载
页面并读取渲染结果——失败会很明显（登录墙、空列表），而不是悄悄返回空。

规则：

- 每个站点一个 Chromium 分区：`persist:pi-staff-site-<channel>`。Cookie 就是会话，
  它留在 Chromium 的存储里，不进入 `SecretStore`，也不进日志。
- 登录会在独立窗口打开站点自己的页面。业主用该站点的 App 扫码，因此密码不会
  输入到本应用；一旦会话拿到登录 Cookie，窗口会自行关闭。
- **手动**运行遇到有采集器但没登录态的站点时，会打开该站点的登录窗口并等待：
  此刻业主就在控制台前、刚刚要求了一次巡探，而那个窗口是这个进程唯一能读到站点的
  方式，否则这一轮只会空手而归。定时运行永远不弹窗：09:30 没有人可以扫码，站点会
  被记为跳过。
- 只有当该 Cookie 存在且未过期时才算已登录。退出登录会清空该分区的 cookie、
  localStorage、IndexedDB、cache storage 与 service worker。
- 站点窗口的 user agent 去掉 Chromium 的 `Electron/<version>` 标识，因为这个标识
  描述的客户端是站点没有规则的；除此之外不改动请求。
- 每次运行默认只打开站点的一个页面：`maxUrls` 默认 1，上限 20。
- 页面在业主看不见的窗口里读取，且只对该窗口关闭 Chromium 的后台节流：被节流的
  页面会停止执行站点自己的脚本，而一个从未渲染出来的列表会被误报成空列表。
  除此之外不改动请求。
- 列表页只有标题。要判断商机，需要的材料是问题本身的正文、带赞同数与日期的
  回答，以及回答下的评论，而这些都在下一层。因此一次巡检还会跟踪列表页自己带
  的链接：默认 3 个，上限 5 个。
- 只有指向本站、协议为 `https`、且路径是该渠道详情读取器认识的那种页面的链接
  （知乎是 `/question/`）才会被跟踪，因此一个指向站外的链接不可能被放进该站点的
  登录态会话里打开。目标保持页面自身的顺序，而不是重新排序——判断什么值得读是
  商机判别步骤的职责；同一个问题从两个列表出现时也只算一个页面。
- 详情页按读者的方式读取。采集器做有限次滚动（折叠在下方的回答是随着滚动才渲染
  出来的），并按回答自己的评论数按钮打开站点自带的评论面板——那个数字本身就是
  一个按钮，按下之前评论并不在文档里。每次只打开、读取、关闭一个回答的评论，面板
  不会比它所属的回答活得更久。不调用任何接口，也不伪造签名。
- 读不出来的详情页、或撞上登录墙的详情页会被排除在这次巡检之外，并写进应用日
  志。它所属的列表页已经在步骤里了，一个打不开的问题不该把一次成功的巡检变成
  一次失败。
- 一个提示词最多承载 24,000 字符（覆盖本轮读到的全部页面），按页面读取顺序消耗，
  单个页面上限 6,000。超出上限的页面会在步骤里被计数，而不是悄悄丢掉。
- 已配置但未登录的站点，会在步骤里被记为跳过。无人值守的运行绝不能让「我们没
  看到」读成「那里什么都没有」。
- 巡探步骤会被明确告知：读取驱动采集回来的内容，且它自己没有对等的能力去抓。凡是
  并非站点自家前端的请求，站点都会用同一堵登录墙回应，手工抓取只会拿到比驱动已经
  掌握的更少的东西，而且这一次尝试会白白消耗掉这个回合。
- 尚未接入采集器的站点是产品的缺口，而不是这次巡检的发现，因此不进入步骤：控制台
  已经把它标成未接入，每个未接入渠道每次运行都重复一行，只会占用模型上下文。
- 站点设置存放在数据目录的 `staff-sites.json`：采集入口（仅 http(s)）、搜索
  关键词清单与每次页数上限。登录态不写在那里；Cookie 才是唯一事实来源。

Tauri 壳在 `staff_sites.rs` 中实现这一节：登录窗是打开站点自己页面的
`WebviewWindow`；采集器是隐藏窗口，通过壳拦截的 `pigo-collect://` 导航把结果
交回，因此远程页面拿不到 IPC 权限。macOS 上无法表达按站点划分的 partition：
WKWebView 让同一进程的所有 webview 共用一个 Cookie 存储。登录态仍从 Cookie
读取，但存储是共享的而不是每站点独立的；退出登录只删除该站点的 Cookie，不清空
整个存储，因为清空整个存储会一并丢掉 renderer 自己的偏好。

控制台把它呈现为「目标网站」页签：每个渠道一行显示状态、扫码登录按钮、采集入口、
页数上限，以及一个会读取一个页面及其链接页的「立即试采集」按钮，让业主在把排期运行托付给它
之前先看清登录态到底能读到什么。

`apps/desktop/test/manual/staff-site-zhihu-live.cjs` 会在 Electron 里加载同一批模块
对真实站点运行。不指定 profile 时它验证的是门禁：未登录的会话会被指名拒绝，
`/hot` 会落到登录页且 cookie 里没有 `z_c0`，登录页确实渲染出可扫的码。用
`PI_STAFF_SITE_USER_DATA` 指向控制台自己的 profile（需先关闭应用，因为 Chromium
持有该 profile 的锁）时，它验证的是扫过码的会话真的能取回条目。

### 回音巡检

壳里的 `staff_watch.rs` 是返回链路。它每十五分钟一次、以及在业主点「立刻巡检」时，
向 host-core 询问哪些已投放的稿子带地址（`staff.watch.list`），在同一个隐藏采集器
里逐个打开这些页面，用正文指纹找到我们那一篇，再读回复数和它下面的评论；每一次读取
都通过 `staff.watch.ingest` 写回。host-core 负责写稿子的数量、当天的话题趋势，以及
每个陌生回复一张卡片。

页面里没有指纹、或者因为这台机器没有登录会话而打不开，都报告为「没读到」——绝不
记成 0，也绝不读成「没人回应」。同一条回复无论巡检跑多少次都只建一张卡，因为卡片以
站点、作者和原话三者作为键。

## 16. 看板

员工产出的是行，看板是业主读这些行、并做决定的地方。控制台旁边随「我的应用」一起出厂
四块看板：

| 看板 | 读什么 | 它承载的决定 |
|---|---|---|
| 商机池 | `staff_leads` | 哪些卡片值得回，每张归谁 |
| 内容工厂 | `staff_contents` | 哪个版本定稿，已经投放了什么 |
| 战报 | `staff_metrics` 加全部看板 | 哪个话题在起量，数字全部从行里数出来 |
| 弹药库 | 共享记忆与凭据档案 | 每个员工开工前读到的口径 |

看板是同一套 `staff.*` RPC 之上的页面——不是第二个数据库，不是第二个窗口，也不属于
某一个员工。切分是按决定来的：`笔杆子` 归档的草稿出现在内容工厂，`地推员` 从同一张
表里读它们。

### 两扇门

归档和决定是两次调用，第二次是业主的：

- 带 `runId` 的 `staff.lead.save` 保留卡片原有的阶段，而一次运行新归档的卡片落在
  `new`。带 `runId` 的 `staff.content.save` 永远产不出高于 `draft`/`review` 的状态。
  一个能批准自己写的东西的员工，会让「已定稿」变成「已写完」。
- `staff.lead.move` 是列与列之间唯一的通道，`staff.content.status` 是通往 `approved`
  唯一的通道，两者都无法从一次运行里触达。移动卡片会打下 `contacted_at`、`replied_at`
  或 `won_at`，战报数的就是这些。

### 建议

每块看板都带同一条导轨：一条建议、它依据的引用、它的成本，和两个按钮。
`staff.advice.derive` 读行，把看板正等着的事提出来——十二小时没人动过的卡片、回了话
却没人接的人、等着定稿的草稿、把其他话题甩开的那一个、有回应却没变成卡片的、放了太久
的共享话术。每条建议只要还开着，就按 `(board, title)` 幂等，所以控制台每次渲染都可以
问一遍。

推导而不是生成是刻意的：问模型今天该干什么，它永远答得出来，而那个答案值多少，全看它
背后有多少证据。想提建议的一次运行，走 `staff.advice.create` 写下它自己的引用。

一条建议是带着它将要变成的工单一起提出来的，而这份工单由看板来写，因为只有看板知道后续
该是谁的活：没人接的卡片交给 `接线员`，正在起量的话题交给 `笔杆子`，等着定稿的稿子谁也
不交——因为只有业主才有权定稿。采纳就是把这份工单发出去；业主可以改派，没被改写的部分
原样保留。

`staff.advice.decide` 是从建议到行动唯一的转换。驳回会留下引用和备注——「我们决定了
不做」是看板能告诉下一次分析的最有用的东西。采纳会写下一条 `staff_directives` 记录，
指明员工、动作和载荷；指令通过 `staff.directive.update` 回执，用同一句话把这条建议结掉。

### 效果数据

`staff_metrics` 是按 `(day, dimension, label)` 为键的每日快照，按本地日切分，维度是
`topic`、`channel` 或 `content`。同一个计数器的第二次读数会覆盖而不是累加，因为把同
一个计数器的两次读数加起来会翻倍。战报上没有任何东西是以总数形式存下来的：每一个数字
都是已经存在的行的计数，因此战报不可能和它所总结的看板对不上。

### 边界

`humanGate` 仍然是硬停：跑到它就带着 `HUMAN_GATE` 结束，而不是挂起等待。看板给了业主
在一次运行之后做决定的地方，这是流水线先需要的东西；让运行可暂停、可恢复是调度器的
改动，不在这里声称。
