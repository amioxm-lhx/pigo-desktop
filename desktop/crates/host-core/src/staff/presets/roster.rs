/**
 * The rest of the org chart, in the order the work reaches them.
 *
 * The founding pair finds demand and names the topics. These five carry a
 * topic the rest of the way: someone writes it, someone posts it, someone
 * answers the people who reply, someone closes them, and someone looks after
 * them afterwards. Each is a blueprint rather than a running employee -- the
 * console offers them, and the owner decides who is actually on payroll.
 *
 * The chain hands off through the boards rather than through each other. A
 * writer does not message the poster; both read the same rows. That is what
 * makes an unattended pipeline debuggable: when a post is wrong, the row that
 * said what to write and the row that says who will say it are both there.
 */

use super::super::*;
use super::{local_offset_minutes, Preset};
use serde_json::json;

/// 21:30: after the analyst has named the topics for tomorrow.
const WRITER_TIMES: [&str; 1] = ["21:30"];

/// The two windows when a new post is still being read by its own community.
const POSTER_TIMES: [&str; 2] = ["10:00", "15:00"];

/// Three passes over the inbox: midday, end of afternoon, and after dinner.
const RECEPTIONIST_TIMES: [&str; 3] = ["11:30", "16:30", "21:00"];

/// Two passes over open conversations, before the day's two quiet windows.
const CLOSER_TIMES: [&str; 2] = ["09:40", "14:40"];

/// 19:30: after the day's deliveries, before the writer's topic run.
const AFTERSALES_TIMES: [&str; 1] = ["19:30"];

/// The five, in the order the work reaches them.
pub(super) fn blueprints() -> Vec<Preset> {
    vec![
        writer(),
        poster(),
        receptionist(),
        closer(),
        aftersales(),
    ]
}

fn writer() -> Preset {
    Preset {
        id: "writer",
        name: "笔杆子",
        legacy_names: &[],
        layer: "worker",
        board: "",
        role: "内勤：按选题写软文、视频脚本与留言话术，每次交多版备选",
        workflow_name: "笔杆子",
        workflow_description: "读当天选题，写出可直接投放的软文与留言话术，多版备选",
        channels: &[],
        schedule: json!({ "timesOfDay": WRITER_TIMES, "utcOffsetMinutes": local_offset_minutes() }),
        quota: json!({ "maxRunsPerDay": 2 }),
        graph: writer_graph(),
    }
}

fn poster() -> Preset {
    Preset {
        id: "poster",
        name: "地推员",
        legacy_names: &[],
        layer: "worker",
        board: "",
        role: "外勤：为定稿话术备好每个位置的现场版，按站点习性改口吻，并记录这批要投给谁",
        workflow_name: "地推员",
        workflow_description: "把定稿话术备成每个位置的现场版，一次一版做 A/B，等人按一下发布",
        channels: &[],
        schedule: json!({ "timesOfDay": POSTER_TIMES, "utcOffsetMinutes": local_offset_minutes() }),
        quota: json!({ "maxRunsPerDay": 4 }),
        graph: poster_graph(),
    }
}

fn receptionist() -> Preset {
    Preset {
        id: "receptionist",
        name: "接线员",
        legacy_names: &[],
        layer: "worker",
        board: "",
        role: "内勤：接住每个来问的人，问清场景与卡点，给出意向分级",
        workflow_name: "接线员",
        workflow_description: "承接咨询对话，问出预算与卡点，把值得跟的人交出去",
        channels: &[],
        schedule: json!({ "timesOfDay": RECEPTIONIST_TIMES, "utcOffsetMinutes": local_offset_minutes() }),
        quota: json!({ "maxRunsPerDay": 3 }),
        graph: receptionist_graph(),
    }
}

fn closer() -> Preset {
    Preset {
        id: "closer",
        name: "跟单员",
        legacy_names: &[],
        layer: "worker",
        board: "",
        role: "内勤：只跟有意向的人，推进试用、报价、合同与打款，记下每个卡点",
        workflow_name: "跟单员",
        workflow_description: "把有意向的对话推进到成交，每一步的卡点都留成记录",
        channels: &[],
        schedule: json!({ "timesOfDay": CLOSER_TIMES, "utcOffsetMinutes": local_offset_minutes() }),
        quota: json!({ "maxRunsPerDay": 4 }),
        graph: closer_graph(),
    }
}

fn aftersales() -> Preset {
    Preset {
        id: "aftersales",
        name: "售后员",
        legacy_names: &[],
        layer: "worker",
        board: "",
        role: "内勤：交付后回访，收集真实反馈，挖复购与转介绍",
        workflow_name: "售后员",
        workflow_description: "交付之后接着跟，把客户的原话变成下一轮可用的口碑",
        channels: &[],
        schedule: json!({ "timesOfDay": AFTERSALES_TIMES, "utcOffsetMinutes": local_offset_minutes() }),
        quota: json!({ "maxRunsPerDay": 2 }),
        graph: aftersales_graph(),
    }
}

/// Read the day's topics, write the pieces, leave them for approval.
fn writer_graph() -> Value {
    json!({
        "nodes": [
            {
                "id": "beat",
                "kind": "trigger",
                "title": "排期",
                "config": { "mode": "schedule" },
                "position": { "x": 40, "y": 32 },
            },
            {
                "id": "recall",
                "kind": "memory",
                "title": "读选题",
                "config": { "operation": "read", "scope": "shared", "kind": "signal" },
                "position": { "x": 280, "y": 32 },
            },
            {
                "id": "pick",
                "kind": "analyze",
                "title": "定角度",
                "instruction": "从读到的信号里挑出今天要写的题目，最多三个。每个题目写清楚：写给谁、要回答他哪一个问题、引用哪一句原话作为依据、以及为什么现在写。挑不出有原话支撑的题目就少写，不要凑数。",
                "config": { "focus": "有原话支撑的题目，而不是我们想说的卖点" },
                "position": { "x": 520, "y": 32 },
            },
            {
                "id": "draft",
                "kind": "produce",
                "title": "起稿",
                "instruction": "为每个题目写稿：一条社区留言、一篇短文、或一段视频口播稿，按渠道选形态。每篇不超过 300 字，开头第一句必须直接对上他要解决的问题，不许写「在当今时代」这类铺陈。同一题目写 2 到 3 个不同开头的版本，作为 A/B 备选。",
                "config": { "artifact": "content", "variants": "3" },
                "position": { "x": 40, "y": 120 },
            },
            {
                "id": "report",
                "kind": "notify",
                "title": "交稿",
                "instruction": "用三句话汇报：写了几篇、分别是什么题目、哪一篇你自己最看好。",
                "config": { "channel": "desktop" },
                "position": { "x": 280, "y": 120 },
            },
        ],
        "edges": [
            { "from": "beat", "to": "recall" },
            { "from": "recall", "to": "pick" },
            { "from": "pick", "to": "draft" },
            { "from": "draft", "to": "report" },
        ],
    })
}

/// Take the approved wording out to the rooms where the question was asked.
fn poster_graph() -> Value {
    json!({
        "nodes": [
            {
                "id": "beat",
                "kind": "trigger",
                "title": "排期",
                "config": { "mode": "schedule" },
                "position": { "x": 40, "y": 32 },
            },
            {
                "id": "recall",
                "kind": "memory",
                "title": "读话术",
                "config": { "operation": "read", "scope": "shared", "kind": "playbook" },
                "position": { "x": 280, "y": 32 },
            },
            {
                "id": "where",
                "kind": "memory",
                "title": "读位置",
                "instruction": "只读同事从站点上带回来的位置：他们问的问题、原话和网址都在这里。没有网址的位置不许往下走。",
                "config": { "operation": "read", "scope": "shared", "kind": "signal" },
                "position": { "x": 520, "y": 32 },
            },
            {
                "id": "target",
                "kind": "analyze",
                "title": "选位置",
                "instruction": "为今天的话术挑出要去的位置：哪条帖子、哪个问题、谁的视频下面。只挑和这句话术问的是同一个问题的地方。一次最多五个，按「他大概率会回」排序，并说明每条为什么值得去。位置只能来自上面读到的原话和网址，不许自己编一个。",
                "config": { "focus": "问题对得上的位置，而不是流量大的位置" },
                "position": { "x": 40, "y": 120 },
            },
            {
                "id": "send",
                "kind": "produce",
                "title": "投放",
                "instruction": "为上面每个位置改写出这条留言的现场版：保留本体，换掉称呼和开头，让它读起来是回帖而不是广告。每条不超过三句，不发链接，不提产品名。leadUrl 必须填上面那条位置的真实网址。\n\n你写的是一条待发布的话，不是一条已发布的话。我们没有能力把任何东西发到站外，发布要由人来按一下，所以每一条都必须是可以直接粘进该页面输入框的成稿，不要写「建议如何回复」这种还要人再加工的东西。",
                "config": { "artifact": "outreach", "limit": "5" },
                "position": { "x": 280, "y": 120 },
            },
            {
                "id": "log",
                "kind": "memory",
                "title": "记投放",
                "instruction": "把这一轮备好了什么记下来：哪条文案、针对哪个位置、用的哪一版。写成下次能直接引用的清单，不要复述过程。写「已备好」，不要写「已投放」——投出去这件事还没有发生。",
                "config": { "operation": "publish", "scope": "shared", "kind": "fact" },
                "position": { "x": 520, "y": 120 },
            },
            {
                "id": "report",
                "kind": "notify",
                "title": "汇报",
                "instruction": "用三句话汇报：备好了几个位置、分别用了哪一版、哪个位置最可能有人回。第一句必须写明「本轮备好 N 条待发布，实际发布 0 条」，不要把备好说成投出。",
                "config": { "channel": "desktop" },
                "position": { "x": 40, "y": 208 },
            },
        ],
        "edges": [
            { "from": "beat", "to": "recall" },
            { "from": "recall", "to": "where" },
            { "from": "where", "to": "target" },
            { "from": "target", "to": "send" },
            { "from": "send", "to": "log" },
            { "from": "log", "to": "report" },
        ],
    })
}

/// Answer whoever came in, and say who is worth handing on.
fn receptionist_graph() -> Value {
    json!({
        "nodes": [
            {
                "id": "beat",
                "kind": "trigger",
                "title": "排期",
                "config": { "mode": "schedule" },
                "position": { "x": 40, "y": 32 },
            },
            {
                "id": "recall",
                "kind": "memory",
                "title": "读已知",
                "config": { "operation": "read", "scope": "shared", "kind": "constraint" },
                "position": { "x": 280, "y": 32 },
            },
            {
                "id": "triage",
                "kind": "analyze",
                "title": "分诊",
                "instruction": "把这一轮的每一段对话按意向分级：A 是他已经说出自己要做的事和时间；B 是他在问方案但还没说场景；C 是路过看一眼。分级只依据他说过的话，并逐条写出那句原话。",
                "config": { "focus": "他自己说出来的下一步，而不是我们的判断" },
                "position": { "x": 520, "y": 32 },
            },
            {
                "id": "answer",
                "kind": "produce",
                "title": "回话",
                "instruction": "为 A 和 B 各写一条回复：先用一句话确认他说的问题，再问一个只有他本人能回答的问题（用什么系统、谁在用、什么时候要）。不推销，不问联系方式。",
                "config": { "artifact": "content", "channel": "reply" },
                "position": { "x": 40, "y": 120 },
            },
            {
                "id": "hand_over",
                "kind": "produce",
                "title": "交单",
                "instruction": "把 A 级对话整理成待交接建议：每个人一段，写清楚他是谁、说过什么、卡在哪、建议交给谁跟。落成结构化数据，交给老板决定。",
                "config": { "artifact": "advice", "board": "leads" },
                "position": { "x": 280, "y": 120 },
            },
            {
                "id": "report",
                "kind": "notify",
                "title": "汇报",
                "instruction": "用三句话汇报：接了几段对话、几个 A 级、最值得跟的是谁。",
                "config": { "channel": "desktop" },
                "position": { "x": 520, "y": 120 },
            },
        ],
        "edges": [
            { "from": "beat", "to": "recall" },
            { "from": "recall", "to": "triage" },
            { "from": "triage", "to": "answer" },
            { "from": "answer", "to": "hand_over" },
            { "from": "hand_over", "to": "report" },
        ],
    })
}

/// Carry the conversations that are already warm to money.
fn closer_graph() -> Value {
    json!({
        "nodes": [
            {
                "id": "beat",
                "kind": "trigger",
                "title": "排期",
                "config": { "mode": "schedule" },
                "position": { "x": 40, "y": 32 },
            },
            {
                "id": "recall",
                "kind": "memory",
                "title": "读进展",
                "config": { "operation": "read", "scope": "shared", "kind": "fact" },
                "position": { "x": 280, "y": 32 },
            },
            {
                "id": "stage",
                "kind": "analyze",
                "title": "看卡点",
                "instruction": "对每个在谈的人判断：现在卡在哪一步——还没看到东西、看到了但没试、试了但没算过账、算过但没走流程。逐条写出判断依据的那句话，判断不出来就写不确定并说明缺什么。",
                "config": { "focus": "现在真正挡着他的那一步" },
                "position": { "x": 520, "y": 32 },
            },
            {
                "id": "push",
                "kind": "produce",
                "title": "拟推进",
                "instruction": "为每个卡点写一条推进话术：一句话解决这个卡点，附一个具体的下一步（约 15 分钟演示、给他一份算好的账、把试用期说清楚）。一条只解决一个卡点，不要罗列。",
                "config": { "artifact": "content", "channel": "direct" },
                "position": { "x": 40, "y": 120 },
            },
            {
                "id": "log",
                "kind": "memory",
                "title": "记进展",
                "instruction": "把这一轮每段对话推进到哪一步、下一步约了什么记下来，写成下次开工能直接接着做的清单。",
                "config": { "operation": "publish", "scope": "shared", "kind": "fact" },
                "position": { "x": 280, "y": 120 },
            },
            {
                "id": "report",
                "kind": "notify",
                "title": "汇报",
                "instruction": "用三句话汇报：在跟几个人、各自卡在哪、这周最可能成的是谁。",
                "config": { "channel": "desktop" },
                "position": { "x": 520, "y": 120 },
            },
        ],
        "edges": [
            { "from": "beat", "to": "recall" },
            { "from": "recall", "to": "stage" },
            { "from": "stage", "to": "push" },
            { "from": "push", "to": "log" },
            { "from": "log", "to": "report" },
        ],
    })
}

/// Look after the people who already paid.
fn aftersales_graph() -> Value {
    json!({
        "nodes": [
            {
                "id": "beat",
                "kind": "trigger",
                "title": "排期",
                "config": { "mode": "schedule" },
                "position": { "x": 40, "y": 32 },
            },
            {
                "id": "recall",
                "kind": "memory",
                "title": "读交付",
                "config": { "operation": "read", "scope": "shared", "kind": "fact" },
                "position": { "x": 280, "y": 32 },
            },
            {
                "id": "check",
                "kind": "analyze",
                "title": "挑回访",
                "instruction": "挑出今天该回访的人：刚交付的、交付一周没动静的、上次反馈过问题的。逐条写清楚上次说到哪儿、这次该问什么，问不出来的不要排进来。",
                "config": { "focus": "上一次对话留下的开口" },
                "position": { "x": 520, "y": 32 },
            },
            {
                "id": "ask",
                "kind": "produce",
                "title": "写回访",
                "instruction": "为每个人写一条回访消息：不要问「用得怎么样」，要问一个具体的、他能用一句话回的问题。发现他在用的场景变了，就把那句原话记下来。",
                "config": { "artifact": "content", "channel": "direct" },
                "position": { "x": 40, "y": 120 },
            },
            {
                "id": "voice",
                "kind": "memory",
                "title": "收口碑",
                "instruction": "把客户自己说出来的、可以公开展示的原话记下来，一句一条，注明是谁在什么场景说的。没有原话就写没有，不要替他总结。",
                "config": { "operation": "publish", "scope": "shared", "kind": "fact" },
                "position": { "x": 280, "y": 120 },
            },
            {
                "id": "report",
                "kind": "notify",
                "title": "汇报",
                "instruction": "用三句话汇报：回访了几个人、收到什么原话、有没有人提到别人也需要。",
                "config": { "channel": "desktop" },
                "position": { "x": 520, "y": 120 },
            },
        ],
        "edges": [
            { "from": "beat", "to": "recall" },
            { "from": "recall", "to": "check" },
            { "from": "check", "to": "ask" },
            { "from": "ask", "to": "voice" },
            { "from": "voice", "to": "report" },
        ],
    })
}
