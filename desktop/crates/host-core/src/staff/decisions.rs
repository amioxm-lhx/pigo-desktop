//! What a middle manager decided, and the orders it handed out (ADR 0254).
//!
//! A manager is an ordinary employee -- same credential, its own workflow, its
//! own schedule, its own run log -- with two extra powers: it reads a board,
//! and it may hand a job to a worker. Both powers are declared in its own
//! workflow graph, so what a manager may do is data on its row rather than a
//! privilege written into the code (which is why renaming one changes nothing).
//!
//! The bounds are narrow on purpose. Dispatching is the one place where a
//! resident agent creates work for another resident agent, and an unattended
//! loop that can feed itself is the failure mode to design against:
//!
//! - a manager may only hand orders to the workers its own graph names,
//! - only the verbs its own graph declares,
//! - at most \`maxDirectivesPerDay\` (five by default) per local day, and
//! - an order that outlives its deadline is expired rather than run late.
//!
//! An order is queued, never run inline: the scheduler picks it up and starts
//! the worker's own run, so "the manager asked for it" and "the worker did it"
//! stay two facts that can be compared afterwards.

use super::*;
use serde_json::json;

/// What a board node can be told to do. Closed for the same reason node kinds
/// are: an unknown action would compile into a step nobody implements.
pub const BOARD_ACTIONS: [&str; 4] = ["read", "judge", "dispatch", "report"];

/// The jobs a manager may hand to a worker. Closed for the same reason: an
/// unknown verb is an order whose meaning only the manager knew.
pub const DIRECTIVE_VERBS: [&str; 7] = [
    "probe",    // 去站点巡一圈，把看到的带回来
    "analyze",  // 读原材料，出结论或选题
    "produce",  // 写稿、写话术、出脚本
    "outreach", // 投放、留言、私信、发邮件
    "reply",    // 接住来问的人
    "followup", // 跟着有意向的人走到成交
    "service",  // 交付、回访、要反馈
];

/// How many orders one manager may hand out in one local day before the host
/// says no. This is the only brake on a manager: the owner does not approve
/// each errand (ADR 0254), so the ceiling is what keeps a looping manager
/// bounded.
pub const DEFAULT_MAX_DIRECTIVES_PER_DAY: i64 = 5;
/// How long an order stays live when the manager did not say.
const DEFAULT_DEADLINE_HOURS: i64 = 6;
const MAX_DEADLINE_HOURS: i64 = 48;
const MIN_DEADLINE_HOURS: i64 = 1;
const MAX_TASK_CHARS: usize = 2_000;
const MAX_TITLE_CHARS: usize = 240;
const MAX_VERDICT_CHARS: usize = 2_000;
const MAX_NOTE_CHARS: usize = 4_000;
const MAX_INPUTS: usize = 20;
const MAX_EVIDENCE: usize = 40;
const DEFAULT_DECISION_LIMIT: i64 = 100;
const MAX_DECISION_LIMIT: i64 = 500;

/// One thing a manager concluded, with the rows it concluded it from.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub id: String,
    pub employee_id: String,
    pub employee_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub board: String,
    pub title: String,
    pub verdict: String,
    /// What it was looking at, in its own words: the board rows behind the call.
    pub inputs: Vec<String>,
    pub evidence: Vec<Evidence>,
    /// The orders this judgement handed out, as they were written.
    pub dispatched: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion_id: Option<String>,
    /// What was refused, and why: "没派出去" is a result, not a silence.
    pub note: String,
    pub created_at: String,
}

const DECISION_SELECT: &str = "SELECT id, employee_id, employee_name, run_id, board, title, verdict,
            inputs_json, evidence_json, dispatched_json, suggestion_id, note, created_at
     FROM staff_decisions";

fn list_of_text(raw: Option<&Value>, limit: usize) -> Vec<String> {
    raw.and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .take(limit)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn decision_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Decision> {
    let inputs: String = row.get(7)?;
    let evidence: String = row.get(8)?;
    let dispatched: String = row.get(9)?;
    Ok(Decision {
        id: row.get(0)?,
        employee_id: row.get(1)?,
        employee_name: row.get(2)?,
        run_id: row.get(3)?,
        board: row.get(4)?,
        title: row.get(5)?,
        verdict: row.get(6)?,
        inputs: serde_json::from_str::<Value>(&inputs)
            .ok()
            .map(|value| list_of_text(Some(&value), MAX_INPUTS))
            .unwrap_or_default(),
        evidence: serde_json::from_str(&evidence).unwrap_or_default(),
        dispatched: serde_json::from_str(&dispatched).unwrap_or_default(),
        suggestion_id: row.get(10)?,
        note: row.get(11)?,
        created_at: ms_to_ts(row.get(12)?),
    })
}

/// What one manager's graph lets it hand out.
///
/// Read back from the pinned version every time rather than stored on the
/// employee: the graph is the version of record, and an owner who edits it
/// should see the change take effect on the next run.
struct DispatchScope {
    targets: Vec<String>,
    verbs: Vec<String>,
}

fn dispatch_scope(db: &Database, employee: &Employee) -> Result<DispatchScope> {
    let mut scope = DispatchScope {
        targets: Vec::new(),
        verbs: Vec::new(),
    };
    let (Some(workflow_id), Some(version)) = (employee.workflow_id.as_deref(), employee.workflow_version)
    else {
        return Ok(scope);
    };
    let Some((graph, _)) = super::workflow::get_graph(db, workflow_id, version)? else {
        return Ok(scope);
    };
    for config in graph.configs_of("board") {
        for name in list_of_text(config.get("targets"), MAX_INPUTS) {
            if !scope.targets.contains(&name) {
                scope.targets.push(name);
            }
        }
        for verb in list_of_text(config.get("verbs"), DIRECTIVE_VERBS.len()) {
            if !scope.verbs.contains(&verb) {
                scope.verbs.push(verb);
            }
        }
    }
    Ok(scope)
}

fn directive_quota(employee: &Employee) -> i64 {
    employee
        .quota
        .get("maxDirectivesPerDay")
        .and_then(Value::as_i64)
        .unwrap_or(DEFAULT_MAX_DIRECTIVES_PER_DAY)
        .clamp(1, 50)
}

fn bounded(label: &str, value: String, max: usize) -> Result<String> {
    if value.chars().count() > max {
        bail!("LIMIT_EXCEEDED: {label} must be at most {max} characters");
    }
    Ok(value)
}

/// The workers a manager's order could go to, and the verbs it could use,
/// without spending anything: what the console shows an owner who wants to know
/// what their manager is allowed to do before it does it.
pub fn dispatch_scope_of(db: &Database, employee_id: &str) -> Result<Value> {
    let employee = get_employee(db, employee_id)?
        .ok_or_else(|| anyhow!("NOT_FOUND: employee {employee_id} does not exist"))?;
    let scope = dispatch_scope(db, &employee)?;
    let spent = directives_today(db, &employee.id)?;
    Ok(json!({
        "employeeId": employee.id,
        "board": employee.board,
        "targets": scope.targets,
        "verbs": scope.verbs,
        "spentToday": spent,
        "maxPerDay": directive_quota(&employee),
    }))
}

fn directives_today(db: &Database, employee_id: &str) -> Result<i64> {
    let (day_start, _) = day_bounds(&local_day(now_ms()))?;
    let spent: i64 = db.conn().query_row(
        "SELECT COUNT(*) FROM staff_directives WHERE issuer_employee_id = ?1 AND created_at >= ?2",
        params![employee_id, day_start],
        |row| row.get(0),
    )?;
    Ok(spent)
}

fn resolve_target(db: &Database, input: &Value, issuer: &Employee) -> Result<Employee> {
    let target = match (
        optional_str(input, "employeeId"),
        optional_str(input, "employeeName"),
        optional_str(input, "to"),
    ) {
        (Some(id), _, _) => get_employee(db, &id)?,
        (None, Some(name), _) | (None, None, Some(name)) => list_employees(db)?
            .into_iter()
            .find(|employee| employee.name == name),
        (None, None, None) => bail!("INVALID_ARGUMENT: employeeName is required"),
    };
    let target = target.ok_or_else(|| {
        anyhow!(
            "NOT_FOUND: {} 想派的活没有对应的人（{}）",
            issuer.name,
            optional_str(input, "employeeName")
                .or_else(|| optional_str(input, "to"))
                .unwrap_or_else(|| "未点名".to_string())
        )
    })?;
    Ok(target)
}

/// Queue one order from a manager to a worker.
///
/// Every refusal is a sentence the owner can act on -- the target is paused,
/// the blueprint does not cover this worker, today's ceiling is reached --
/// because a silently dropped order is indistinguishable from a manager that
/// decided to do nothing.
pub fn assign_directive(db: &Database, input: &Value) -> Result<Directive> {
    let issuer = match optional_str(input, "issuerEmployeeId") {
        Some(id) => get_employee(db, &id)?
            .ok_or_else(|| anyhow!("NOT_FOUND: employee {id} does not exist"))?,
        None => {
            let run_id = optional_str(input, "reasonRunId").ok_or_else(|| {
                anyhow!("INVALID_ARGUMENT: issuerEmployeeId or reasonRunId is required")
            })?;
            let run = get_run(db, &run_id)?
                .ok_or_else(|| anyhow!("NOT_FOUND: run {run_id} does not exist"))?;
            get_employee(db, &run.employee_id)?
                .ok_or_else(|| anyhow!("NOT_FOUND: employee {} does not exist", run.employee_id))?
        }
    };
    if issuer.layer != "manager" {
        bail!(
            "PERMISSION_DENIED: {} 不是中层，只有中层才会派活",
            issuer.name
        );
    }
    let verb = required_str(input, "verb")?;
    if !DIRECTIVE_VERBS.contains(&verb.as_str()) {
        bail!(
            "INVALID_ARGUMENT: unknown directive verb {verb} (expected {})",
            DIRECTIVE_VERBS.join(", ")
        );
    }
    let task = bounded("the order", required_str(input, "task")?, MAX_TASK_CHARS)?;
    let target = resolve_target(db, input, &issuer)?;
    if target.id == issuer.id {
        bail!("INVALID_ARGUMENT: {} 不能给自己派活", issuer.name);
    }
    if target.layer != "worker" {
        bail!("PERMISSION_DENIED: {} 不是基层员工，派活只能派给基层", target.name);
    }
    if target.status != "running" {
        bail!(
            "CONFLICT: {} 现在是 {} 状态，派给它的活不会被执行",
            target.name,
            status_word(&target.status)
        );
    }
    let scope = dispatch_scope(db, &issuer)?;
    if !scope.targets.iter().any(|name| name == &target.name) {
        bail!(
            "PERMISSION_DENIED: {} 的蓝图里没有「派活给 {}」这条授权",
            issuer.name,
            target.name
        );
    }
    if !scope.verbs.contains(&verb) {
        bail!(
            "PERMISSION_DENIED: {} 的蓝图里没有 {verb} 这类活的授权",
            issuer.name
        );
    }
    let max_per_day = directive_quota(&issuer);
    let spent = directives_today(db, &issuer.id)?;
    if spent >= max_per_day {
        bail!(
            "LIMIT_EXCEEDED: {} 今天已经派了 {spent} 件活（上限 {max_per_day} 件）",
            issuer.name
        );
    }
    let hours = input
        .get("deadlineHours")
        .and_then(Value::as_i64)
        .unwrap_or(DEFAULT_DEADLINE_HOURS)
        .clamp(MIN_DEADLINE_HOURS, MAX_DEADLINE_HOURS);
    let now = now_ms();
    let id = Uuid::new_v4().to_string();
    let payload = json!({
        "task": task,
        "board": issuer.board,
        "issuer": issuer.name,
        "issuerEmployeeId": issuer.id,
        "deadlineHours": hours,
    });
    db.conn().execute(
        "INSERT INTO staff_directives
           (id, suggestion_id, employee_id, employee_name, issuer_employee_id, reason_run_id,
            board, verb, payload_json, status, result_note, deadline_at, priority, created_at,
            updated_at)
         VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'queued', '', ?9, ?10, ?11, ?11)",
        params![
            id,
            target.id,
            target.name,
            issuer.id,
            optional_str(input, "reasonRunId"),
            issuer.board,
            verb,
            payload.to_string(),
            now + hours * 60 * 60 * 1_000,
            input.get("priority").and_then(Value::as_i64).unwrap_or(0),
            now,
        ],
    )?;
    get_directive(db, &id)?.ok_or_else(|| anyhow!("NOT_FOUND: directive {id} vanished after write"))
}

/// Mark an order as picked up by the run that is carrying it out.
pub fn claim_directive(db: &Database, directive_id: &str, run_id: &str) -> Result<bool> {
    let updated = db.conn().execute(
        "UPDATE staff_directives
            SET status = 'running', claimed_run_id = ?2, updated_at = ?3
          WHERE id = ?1 AND status = 'queued'",
        params![directive_id, run_id, now_ms()],
    )?;
    Ok(updated > 0)
}

/// The order a run is carrying, as the worker's prompt needs to read it.
pub fn directive_brief(db: &Database, directive_id: &str) -> Result<Option<Value>> {
    let Some(directive) = get_directive(db, directive_id)? else {
        return Ok(None);
    };
    Ok(Some(json!({
        "id": directive.id,
        "verb": directive.verb,
        "board": directive.board,
        "issuer": directive.payload.get("issuer").and_then(Value::as_str).unwrap_or_default(),
        "task": directive.payload.get("task").and_then(Value::as_str).unwrap_or_default(),
    })))
}

/// Record one judgement, and hand out whatever it decided to hand out.
///
/// The card is written first and the orders after, so a manager whose dispatch
/// was refused still shows the owner what it concluded and why it could not
/// act -- the refusal is the note, not a missing row.
pub fn record_decision(db: &Database, input: &Value) -> Result<Decision> {
    let employee_id = required_str(input, "employeeId")?;
    let employee = get_employee(db, &employee_id)?
        .ok_or_else(|| anyhow!("NOT_FOUND: employee {employee_id} does not exist"))?;
    if employee.layer != "manager" {
        bail!(
            "PERMISSION_DENIED: {} 不是中层，中层才会下判断",
            employee.name
        );
    }
    let run_id = optional_str(input, "runId");
    let board = optional_str(input, "board")
        .unwrap_or_else(|| employee.board.clone());
    let title = bounded("the decision title", required_str(input, "title")?, MAX_TITLE_CHARS)?;
    let verdict = optional_str(input, "verdict")
        .map(|text| bounded("the verdict", text, MAX_VERDICT_CHARS))
        .transpose()?
        .unwrap_or_default();
    let inputs = list_of_text(input.get("inputs"), MAX_INPUTS);
    let mut evidence: Vec<Evidence> = Vec::new();
    if let Some(values) = input.get("evidence").and_then(Value::as_array) {
        for value in values.iter().take(MAX_EVIDENCE) {
            if let Ok(cited) = serde_json::from_value::<Evidence>(value.clone()) {
                evidence.push(cited);
            }
        }
    }
    let id = Uuid::new_v4().to_string();
    let now = now_ms();
    db.conn().execute(
        "INSERT INTO staff_decisions
           (id, employee_id, employee_name, run_id, board, title, verdict, inputs_json,
            evidence_json, dispatched_json, suggestion_id, note, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, '[]', ?10, '', ?11)",
        params![
            id,
            employee.id,
            employee.name,
            run_id,
            board,
            title,
            verdict,
            serde_json::to_string(&inputs)?,
            serde_json::to_string(&evidence)?,
            optional_str(input, "suggestionId"),
            now,
        ],
    )?;

    let mut dispatched = Vec::new();
    let mut refused = Vec::new();
    let orders = input
        .get("dispatched")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for order in orders.iter().take(MAX_INPUTS) {
        match assign_directive(db, &order_with_issuer(order, &employee, run_id.as_deref())) {
            Ok(directive) => dispatched.push(serde_json::to_value(&directive)?),
            Err(error) => refused.push(error.to_string()),
        }
    }
    let note = bounded(
        "the refusal note",
        refused.join(" / "),
        MAX_NOTE_CHARS,
    )?;
    db.conn().execute(
        "UPDATE staff_decisions SET dispatched_json = ?2, note = ?3 WHERE id = ?1",
        params![id, serde_json::to_string(&dispatched)?, note],
    )?;
    get_decision(db, &id)?.ok_or_else(|| anyhow!("NOT_FOUND: decision {id} vanished after write"))
}

/// One order as the model wrote it, with the issuer filled in from the run that
/// made the call rather than from the model's own recollection.
fn order_with_issuer(order: &Value, employee: &Employee, run_id: Option<&str>) -> Value {
    let mut params = serde_json::Map::new();
    params.insert("issuerEmployeeId".to_string(), json!(employee.id));
    if let Some(run_id) = run_id {
        params.insert("reasonRunId".to_string(), json!(run_id));
    }
    for (target, source) in [
        ("employeeName", "to"),
        ("employeeName", "employeeName"),
        ("employeeId", "employeeId"),
        ("verb", "verb"),
        ("task", "task"),
        ("task", "instruction"),
        ("priority", "priority"),
        ("deadlineHours", "deadlineHours"),
    ] {
        if params.contains_key(target) {
            continue;
        }
        if let Some(value) = order.get(source) {
            if !value.is_null() {
                params.insert(target.to_string(), value.clone());
            }
        }
    }
    Value::Object(params)
}

/// One line of board material, cut to what a prompt can carry.
fn clip(text: &str, max: usize) -> String {
    let value = text.trim().replace(char::is_whitespace, " ");
    if value.chars().count() <= max {
        return value;
    }
    value.chars().take(max).collect::<String>() + "…"
}

/// What a manager reads when its workflow says "read the board".
///
/// Deterministic on purpose. The counts a manager decides from are the same
/// rows the owner is looking at, so "why did it say that" is answered by the
/// board itself and not by the model's recollection of it. The manager's own
/// outstanding orders are part of what it reads: a manager that cannot see what
/// it already handed out is a manager that hands the same errand out twice.
pub fn board_brief(db: &Database, input: &Value) -> Result<Value> {
    let board = optional_str(input, "board").unwrap_or_else(|| "leads".to_string());
    if !SUGGESTION_BOARDS.contains(&board.as_str()) {
        bail!(
            "INVALID_ARGUMENT: unknown board {board} (expected {})",
            SUGGESTION_BOARDS.join(", ")
        );
    }
    let limit = input
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(12)
        .clamp(1, 40);
    let mut notes: Vec<String> = Vec::new();
    let mut summary: Vec<(String, i64)> = Vec::new();

    for (stage, count) in stage_counts(db, &board)? {
        summary.push((stage, count));
    }
    if board == "contents" {
        notes.push(format!(
            "- [看板] 内容分状态：{}",
            summary
                .iter()
                .map(|(label, count)| format!("{label} {count}"))
                .collect::<Vec<_>>()
                .join(" / ")
        ));
        notes.push("- [看板] 这些是「我们打算说什么」，还没发生的事：已发布的那几条才有真实反馈。".to_string());
        for row in content_rows(db, limit)? {
            notes.push(row);
        }
        for row in topic_rows(db)? {
            notes.push(row);
        }
    } else {
        notes.push(format!(
            "- [看板] 线索分列：{}",
            summary
                .iter()
                .map(|(label, count)| format!("{label} {count}"))
                .collect::<Vec<_>>()
                .join(" / ")
        ));
        for row in lead_rows(db, limit)? {
            notes.push(row);
        }
        let stale: i64 = db.conn().query_row(
            "SELECT COUNT(*) FROM staff_leads
              WHERE stage = 'new' AND created_at <= ?1",
            params![now_ms() - 86_400_000],
            |row| row.get(0),
        )?;
        if stale > 0 {
            notes.push(format!(
                "- [看板] 有 {stale} 条线索进来看了一整天还没被碰过"
            ));
        }
    }

    if let Some(employee_id) = optional_str(input, "employeeId") {
        let mine = list_directives(
            db,
            &json!({ "issuerEmployeeId": employee_id, "limit": 8 }),
        )?;
        for directive in mine {
            let task = directive
                .payload
                .get("task")
                .and_then(Value::as_str)
                .unwrap_or_default();
            notes.push(format!(
                "- [我派的活][{}] {} ← {}",
                directive.status,
                clip(task, 120),
                directive.employee_name
            ));
        }
        if !notes.is_empty() {
            notes.push(format!(
                "- [配额] 今天还能派 {} 件活",
                directive_quota(
                    &get_employee(db, &employee_id)?
                        .ok_or_else(|| anyhow!("NOT_FOUND: employee {employee_id} does not exist"))?
                ) - directives_today(db, &employee_id)?
            ));
        }
    }

    let open = list_suggestions(
        db,
        &json!({ "board": board, "status": "open", "limit": 5 }),
    )?;
    for suggestion in open {
        notes.push(format!(
            "- [等你定] {}（{}）",
            clip(&suggestion.title, 120),
            clip(&suggestion.action, 80)
        ));
    }

    Ok(json!({
        "board": board,
        "notes": notes,
        "summary": summary
            .into_iter()
            .map(|(label, count)| json!({ "label": label, "count": count }))
            .collect::<Vec<_>>(),
    }))
}

/// The column counts of one board, read from the same tables it draws.
fn stage_counts(db: &Database, board: &str) -> Result<Vec<(String, i64)>> {
    let (table, column) = if board == "contents" {
        ("staff_contents", "status")
    } else {
        ("staff_leads", "stage")
    };
    let mut stmt = db.conn().prepare(&format!(
        "SELECT {column}, COUNT(*) FROM {table} GROUP BY {column} ORDER BY COUNT(*) DESC"
    ))?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn lead_rows(db: &Database, limit: i64) -> Result<Vec<String>> {
    let mut stmt = db.conn().prepare(
        "SELECT title, site, stage, author, score, quote FROM staff_leads
          WHERE stage NOT IN ('lost', 'dropped')
          ORDER BY updated_at DESC, created_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        let title: String = row.get(0)?;
        let site: String = row.get(1)?;
        let stage: String = row.get(2)?;
        let author: String = row.get(3)?;
        let score: i64 = row.get(4)?;
        let quote: String = row.get(5)?;
        Ok(format!(
            "- [线索][{}·{}分] {}（{}）{} 原话：{}",
            stage,
            score,
            title,
            site,
            author,
            quote
        ))
    })?;
    Ok(rows
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|row| clip(&row, 400))
        .collect())
}

fn content_rows(db: &Database, limit: i64) -> Result<Vec<String>> {
    let mut stmt = db.conn().prepare(
        "SELECT title, channel, topic, status, views, replies, variant FROM staff_contents
          WHERE status <> 'retired'
          ORDER BY updated_at DESC, created_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        let title: String = row.get(0)?;
        let _channel: String = row.get(1)?;
        let topic: String = row.get(2)?;
        let status: String = row.get(3)?;
        let views: i64 = row.get(4)?;
        let replies: i64 = row.get(5)?;
        let variant: String = row.get(6)?;
        Ok(format!(
            "- [内容][{}]{}{} ← {}｜浏览 {views} 回复 {replies}",
            status,
            title,
            if variant.is_empty() {
                String::new()
            } else {
                format!("（{variant} 版）")
            },
            topic
        ))
    })?;
    Ok(rows
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|row| clip(&row, 300))
        .collect())
}

/// Which topics actually got read, from the counts the owner entered by hand.
fn topic_rows(db: &Database) -> Result<Vec<String>> {
    let since = super::local_day(now_ms() - 3 * 86_400_000);
    let mut stmt = db.conn().prepare(
        "SELECT label, COALESCE(SUM(views), 0), COALESCE(SUM(replies), 0), COALESCE(SUM(leads), 0)
           FROM staff_metrics
          WHERE dimension = 'topic' AND day >= ?1
          GROUP BY label
          ORDER BY SUM(views) DESC, SUM(replies) DESC
          LIMIT 5",
    )?;
    let rows = stmt.query_map(params![since], |row| {
        let label: String = row.get(0)?;
        let views: i64 = row.get(1)?;
        let replies: i64 = row.get(2)?;
        let leads: i64 = row.get(3)?;
        Ok(format!(
            "- [走势] 近三天 {label}：浏览 {views}／回复 {replies}／线索 {leads}"
        ))
    })?;
    let mut out: Vec<String> = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    if out.is_empty() {
        out.push("- [走势] 近三天没有任何话题数据：老板还没有把浏览/回复数录进来。".to_string());
    }
    Ok(out)
}

pub fn get_decision(db: &Database, id: &str) -> Result<Option<Decision>> {
    Ok(db
        .conn()
        .prepare_cached(&format!("{DECISION_SELECT} WHERE id = ?1"))?
        .query_row(params![id], decision_from_row)
        .optional()?)
}

/// The judgements one board shows, newest first. A worker's own runs are not
/// what this answers: a decision is a manager's output, and the console draws
/// it above the cards that output eventually produced.
pub fn list_decisions(db: &Database, filter: &Value) -> Result<Vec<Decision>> {
    let limit = match filter.get("limit") {
        None | Some(Value::Null) => DEFAULT_DECISION_LIMIT,
        Some(value) => value
            .as_i64()
            .ok_or_else(|| anyhow!("INVALID_ARGUMENT: limit must be an integer"))?
            .clamp(1, MAX_DECISION_LIMIT),
    };
    let board = optional_str(filter, "board");
    let employee_id = optional_str(filter, "employeeId");
    let mut stmt = db.conn().prepare_cached(&format!(
        "{DECISION_SELECT}
         WHERE (?1 IS NULL OR board = ?1)
           AND (?2 IS NULL OR employee_id = ?2)
         ORDER BY created_at DESC, id
         LIMIT ?3"
    ))?;
    let rows = stmt.query_map(params![board, employee_id, limit], decision_from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn status_word(status: &str) -> &str {
    match status {
        "paused" => "暂停",
        "stopped" => "停用",
        _ => status,
    }
}
