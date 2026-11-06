/**
 * The two employees the business starts with.
 *
 * The scout goes out and looks for demand; the analyst reads what came back
 * and turns it into the next thing to say. Everything else in the roster
 * consumes what this pair produces, which is why they ship first.
 */

use super::super::*;
use super::{local_offset_minutes, Preset};
use serde_json::json;

/// The sites a sweep reads.
///
/// Only the sites that have a collector are named. A sweep hands the employee
/// what the driver read, so naming a site nobody can read would send the
/// employee off to fetch it by hand -- and the site answers that with the very
/// login wall that stopped the first round. The list grows with the site
/// registry in `apps/desktop-tauri/src-tauri/src/staff_sites.rs`, which is why
/// it is one entry long today: juejin, bilibili, wechatMp and wechatChannels
/// still need their own adapters.
const SCOUT_SITES: [&str; 1] = ["zhihu"];

/// 09:30 and 14:30, the two quiet windows when a new post is still being read.
const SCOUT_TIMES: [&str; 2] = ["09:30", "14:30"];
/// 20:30: after the day's replies have landed, before the next probe goes out.
const ANALYST_TIMES: [&str; 1] = ["20:30"];

pub(super) fn scout() -> Preset {
    Preset {
        id: "scout",
        name: "侦察员",
        legacy_names: &["侦察探针"],
        layer: "worker",
        board: "",
        role: "外勤：按关键问题清单巡站，找出自己就是决策人、正在问「一个人怎么把生意做起来」的人",
        workflow_name: "侦察员",
        workflow_description: "巡关键问题，挑出会买的人，起草一条能换来主页访问的留言",
        channels: &SCOUT_SITES,
        schedule: json!({ "timesOfDay": SCOUT_TIMES, "utcOffsetMinutes": local_offset_minutes() }),
        quota: json!({ "maxRunsPerDay": 2 }),
        graph: scout_graph(),
    }
}

pub(super) fn interest_analyst() -> Preset {
    Preset {
        id: "interestAnalyst",
        name: "选题员",
        legacy_names: &["兴趣分析"],
        layer: "worker",
        board: "",
        role: "内勤：把一线原话按「会不会买」分成三类，产出待回答的问题清单",
        workflow_name: "选题员",
        workflow_description: "从互动原话里分出会买的人，产出待回答的问题清单和对应的证据",
        channels: &[],
        schedule: json!({ "timesOfDay": ANALYST_TIMES, "utcOffsetMinutes": local_offset_minutes() }),
        quota: json!({ "maxRunsPerDay": 1 }),
        graph: analyst_graph(),
    }
}

/// Walk the sources, judge what is worth answering, draft the message, report.
fn scout_graph() -> Value {
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
                "id": "scan",
                "kind": "probe",
                "title": "巡社区",
                "instruction": "驱动已经把这一轮按关键问题清单读到的页面放在 [collected: ...] 里（这是登录态下真实读到的页面，不是你抓来的）。只依据这些内容挑人：谁正在问一个「一个人怎么把生意做起来」的问题——怎么获客、怎么找早期用户、一个人怎么同时做内容和交付、怎么让 AI 替自己干那些重复的活。我们要找的是自己就是决策人、而且已经在公开平台上说话的人，不是替老板发愁的人。逐条写出候选，并写明这个人的身份线索来自哪一句原话。某个站点如果被标成 skipped，就在汇报里写明是哪个站点没读到，不要改用其他方式去取。不要在这个步骤里发言。",
                "config": { "sources": SCOUT_SITES, "window": "24h" },
                "position": { "x": 280, "y": 32 },
            },
            {
                "id": "triage",
                "kind": "analyze",
                "title": "判商机",
                "instruction": "逐条过筛，只有同时满足这三条的才留下：\n1. 身份：他自己就是决策人——一人公司、独立开发、独立顾问、小团队主理人。出现「我们老板」「我们公司」「领导要求」这类第三人称的人，一律不留。\n2. 意图：他自己的原话里有一个第一人称的获客或提效动作——「我每周要写」「我的客户问我」「我一个人做不过来」。只有情绪、只有吐槽、没有动作的不留。\n3. 可答性：我们手上有能回答这条问题的真实经验或真实数字。答不出来的不留。\n留下的每条写清楚：他是谁（用什么身份）、他要什么（引一句他自己的原话）、我们凭什么答得出来。三条缺任何一条就丢掉，宁可这一轮一条都不留。",
                "config": { "focus": "会买的人：自己就是决策人，原话里有第一人称的获客动作，而且我们答得出来", "artifact": "leads" },
                "position": { "x": 520, "y": 32 },
            },
            {
                "id": "file",
                "kind": "memory",
                "title": "存线索",
                "instruction": "把上一步留下的问题压成一条信号：写清这一轮真的看到谁在问什么——他的身份、他自己的那句原话、我们要答的那个问题。写成下次能直接引用的一句话，不要复述步骤，也不要写我们打算怎么回复。",
                "config": { "operation": "publish", "scope": "shared", "kind": "signal" },
                "position": { "x": 40, "y": 120 },
            },
            {
                "id": "draft",
                "kind": "produce",
                "title": "起留言",
                "instruction": "为每条留下的问题写一条留言。这条留言唯一的目标是让对方点开我们的主页：不是成交，也不是把话说完。三段，一共不超过四句。第一句用他自己的词接住他的处境；第二句给一条他今天就能动手的具体做法；第三句说清我是干什么的，用一句会让人想点开主页看看的话；最后问一个只有他本人能回答的问题。别人的评论区里不提产品名、不放外链。不要客套话。",
                "config": { "artifact": "content", "channel": "reply", "limit": "5" },
                "position": { "x": 280, "y": 120 },
            },
            {
                "id": "report",
                "kind": "notify",
                "title": "汇总",
                "instruction": "用三句话汇报这一轮：巡了哪些渠道、留下几条、最值得跟的是哪一条。",
                "config": { "channel": "desktop" },
                "position": { "x": 520, "y": 120 },
            },
        ],
        "edges": [
            { "from": "beat", "to": "scan" },
            { "from": "scan", "to": "triage" },
            { "from": "triage", "to": "file" },
            { "from": "file", "to": "draft" },
            { "from": "draft", "to": "report" },
        ],
    })
}

/// Read what the team already knows, mine the replies, publish what was learned.
fn analyst_graph() -> Value {
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
                "config": { "operation": "read", "scope": "shared", "kind": "signal" },
                "position": { "x": 280, "y": 32 },
            },
            {
                "id": "mine",
                "kind": "analyze",
                "title": "读原话",
                "instruction": "从侦察员带回来的互动原话和留言回应里，把说话的人分成三类：正在自己做这门生意的人（会买）、被迫承受这件事的人（不会买）、只是围观的人。分别写出三类人各自用的词，以及他们拒绝过的方案。能指回一句原话的才写，并且区分「他说的」和「我们猜的」。",
                "config": { "focus": "谁在替自己的生意说话，而不是在替老板发愁" },
                "position": { "x": 520, "y": 32 },
            },
            {
                "id": "topics",
                "kind": "produce",
                "title": "出选题",
                "instruction": "产出本轮的待回答的问题清单：每条写明是谁在问（身份）、原话里那个具体问题、我们手上能拿出来的对应证据（真实数字或亲身经历），以及答完之后希望对方做什么。只收第一类人的问题；这一轮第一类人一个都没有，就写「本轮没有可成交的问题」，不要拿第二类、第三类的问题凑数。",
                "config": { "artifact": "待回答的问题清单" },
                "position": { "x": 40, "y": 120 },
            },
            {
                "id": "publish",
                "kind": "memory",
                "title": "沉淀信号",
                "instruction": "把上一步的选题压成一条信号：这一轮从互动原话里到底看出了什么，写成下次能直接引用的一句话结论，并带上可用边界。",
                "config": { "operation": "publish", "scope": "shared", "kind": "signal" },
                "position": { "x": 280, "y": 120 },
            },
            {
                "id": "report",
                "kind": "notify",
                "title": "汇总",
                "config": { "channel": "desktop" },
                "position": { "x": 520, "y": 120 },
            },
        ],
        "edges": [
            { "from": "beat", "to": "recall" },
            { "from": "recall", "to": "mine" },
            { "from": "mine", "to": "topics" },
            { "from": "topics", "to": "publish" },
            { "from": "publish", "to": "report" },
        ],
    })
}


/// The founding pair, in the order the console offers them.
pub(super) fn blueprints() -> Vec<Preset> {
    vec![scout(), interest_analyst()]
}
