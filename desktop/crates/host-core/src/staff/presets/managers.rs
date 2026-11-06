/**
 * The two middle managers (ADR 0254).
 *
 * The workers do the errands; these two read what the workers brought back and
 * decide what happens next. A manager is an ordinary employee in every other
 * way -- its own key, its own model, its own workflow, its own run log, its own
 * memory -- and the two powers that make it a manager are declared in the graph
 * below: a \`board\` node, and the workers and verbs that node is allowed to
 * hand work to.
 *
 * Neither of them talks to the outside world. A manager does not post, does not
 * comment, does not email: it reads, judges, and dispatches, and the dispatching
 * is recorded as an order with a deadline. That separation is what makes the
 * console readable -- what the owner sees above the cards is the thinking, and
 * the cards below are what the thinking caused.
 */

use super::super::*;
use super::{local_offset_minutes, Preset};
use serde_json::json;

/// The workers the lead manager may hand errands to, and the kinds of errand.
///
/// Declared once and interpolated into the graph, so the graph the host
/// validates and the graph the console draws can never disagree about the
/// manager's authority.
const LEAD_TARGETS: [&str; 4] = ["笔杆子", "地推员", "接线员", "跟单员"];
const LEAD_VERBS: [&str; 4] = ["produce", "outreach", "reply", "followup"];

/// The people who can act on what the editor decides.
const CONTENT_TARGETS: [&str; 2] = ["笔杆子", "地推员"];
const CONTENT_VERBS: [&str; 2] = ["produce", "outreach"];

/// 09:15 and 16:45: after the morning's sweeps have landed, and before the
/// afternoon's conversations start.
const LEAD_TIMES: [&str; 2] = ["09:15", "16:45"];
/// 20:45: after the analyst has read the day's replies, before the writer's
/// own run at 21:30.
const CONTENT_TIMES: [&str; 1] = ["20:45"];

pub(super) fn lead_manager() -> Preset {
    Preset {
        id: "leadManager",
        name: "线索主管",
        legacy_names: &[],
        layer: "manager",
        board: "leads",
        role: "中层：读线索看板与一线原话，给线索分级，把活派下去，只把大机会报给老板",
        workflow_name: "线索主管",
        workflow_description: "读看板做判断，把线索分成该动的和该等的，派活并跟踪回音",
        channels: &[],
        schedule: json!({ "timesOfDay": LEAD_TIMES, "utcOffsetMinutes": local_offset_minutes() }),
        quota: json!({ "maxRunsPerDay": 2, "maxDirectivesPerDay": 5 }),
        graph: lead_manager_graph(),
    }
}

pub(super) fn content_editor() -> Preset {
    Preset {
        id: "contentEditor",
        name: "内容主编",
        legacy_names: &[],
        layer: "manager",
        board: "contents",
        role: "中层：读选题与走势，定今天主攻什么、用什么角度、投在哪，派给笔杆子和地推员",
        workflow_name: "内容主编",
        workflow_description: "按真实反馈定今天的题目与配比，派稿派投放，盯回音",
        channels: &[],
        schedule: json!({ "timesOfDay": CONTENT_TIMES, "utcOffsetMinutes": local_offset_minutes() }),
        quota: json!({ "maxRunsPerDay": 1, "maxDirectivesPerDay": 5 }),
        graph: content_editor_graph(),
    }
}

/// Read the board, make the call, escalate only what money needs, report.
fn lead_manager_graph() -> Value {
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
                "id": "glance",
                "kind": "board",
                "title": "读线索看板",
                "instruction": "上一步会把这轮看板的摘要（分列数量、每条线索的原话、我们派出去还没回音的活、老板还没定的建议）放在 [看板] 开头的行里。先读一遍，不要自己去找数据。",
                "config": { "action": "read", "board": "leads" },
                "position": { "x": 280, "y": 32 },
            },
            {
                "id": "weigh",
                "kind": "board",
                "title": "下判断",
                "instruction": "做今天这一轮判断：哪几条线索值得先动、动的时候用哪个角度、派给谁去动。判断要落到行动上，每条派出去的活都要写清派给谁、做什么、为什么是现在。已经派过还没回音的活不要重复派；线索不够好的就在结论里说清为什么先不动。老板不需要知道你排了序，他需要知道你先动哪一条。",
                "config": {
                    "action": "judge",
                    "board": "leads",
                    "targets": LEAD_TARGETS,
                    "verbs": LEAD_VERBS,
                },
                "position": { "x": 520, "y": 32 },
            },
            {
                "id": "brief",
                "kind": "board",
                "title": "上报要老板定的",
                "instruction": "只把老板必须亲自定的事写进建议箱：涉及钱（报价、折扣、预算）、涉及对外承诺、或者要改变对外的说法时才写。没有就明确说这一轮没有要塞给老板的事，不要为了显得在干活而写。",
                "config": { "action": "report", "board": "leads" },
                "position": { "x": 40, "y": 120 },
            },
            {
                "id": "report",
                "kind": "notify",
                "title": "汇总",
                "instruction": "用三句话汇报这一轮：看了什么、先动哪一条、派了几件活给谁。老板只读这三句。",
                "config": { "channel": "desktop" },
                "position": { "x": 280, "y": 120 },
            },
        ],
        "edges": [
            { "from": "beat", "to": "glance" },
            { "from": "glance", "to": "weigh" },
            { "from": "weigh", "to": "brief" },
            { "from": "brief", "to": "report" },
        ],
    })
}

/// Read what the day produced, decide the day's topic, hand out the work.
fn content_editor_graph() -> Value {
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
                "id": "glance",
                "kind": "board",
                "title": "读内容看板",
                "instruction": "上一步会把这轮看板的摘要（内容分状态、近三天哪个题目真的被读、我们派出去还没回音的活）放在 [看板] 开头的行里。先读一遍，不要自己去找数据。",
                "config": { "action": "read", "board": "contents" },
                "position": { "x": 280, "y": 32 },
            },
            {
                "id": "weigh",
                "kind": "board",
                "title": "定今天的题目",
                "instruction": "定今天主攻什么：题目、角度、投在哪个位置、准备几个版本做 A/B。选题必须能指回一句真实原话或一组真实数字，指不回去的题目就换一个。如果近三天没有任何效果数据，就说明这一点，并只派「先拿到真实反馈」这一件事，不要凭感觉定题目。已派出去还没回音的活不要重复派。",
                "config": {
                    "action": "judge",
                    "board": "contents",
                    "targets": CONTENT_TARGETS,
                    "verbs": CONTENT_VERBS,
                },
                "position": { "x": 520, "y": 32 },
            },
            {
                "id": "brief",
                "kind": "board",
                "title": "上报要老板定的",
                "instruction": "只把老板必须亲自定的事写进建议箱：涉及钱、涉及对外承诺、或者要改变对外的说法时才写。没有就明确说没有。",
                "config": { "action": "report", "board": "contents" },
                "position": { "x": 40, "y": 120 },
            },
            {
                "id": "report",
                "kind": "notify",
                "title": "汇总",
                "instruction": "用三句话汇报这一轮：哪个题目被证明有效、今天主攻什么、派了几件活给谁。",
                "config": { "channel": "desktop" },
                "position": { "x": 280, "y": 120 },
            },
        ],
        "edges": [
            { "from": "beat", "to": "glance" },
            { "from": "glance", "to": "weigh" },
            { "from": "weigh", "to": "brief" },
            { "from": "brief", "to": "report" },
        ],
    })
}

/// The two managers, in the order the console offers them.
pub(super) fn blueprints() -> Vec<Preset> {
    vec![lead_manager(), content_editor()]
}
