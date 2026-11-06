use super::*;
use serde_json::json;
use std::collections::HashMap;

/// Which board wrote a suggestion. The set is closed because the console draws
/// one inbox per board and an unknown board would be advice nobody is shown.
pub const SUGGESTION_BOARDS: [&str; 4] = ["leads", "contents", "report", "arsenal"];

/// `open` is waiting on the owner. An agent may write a suggestion, but only
/// the owner accepting one turns it into work.
pub const SUGGESTION_STATUSES: [&str; 4] = ["open", "accepted", "rejected", "done"];

/// A directive's life: queued when approved, running while an employee has it,
/// done or failed when it reports back.
pub const DIRECTIVE_STATUSES: [&str; 4] = ["queued", "running", "done", "failed"];

const DEFAULT_SUGGESTION_LIMIT: i64 = 100;
const MAX_SUGGESTION_LIMIT: i64 = 500;
const DEFAULT_DIRECTIVE_LIMIT: i64 = 200;
const MAX_DIRECTIVE_LIMIT: i64 = 1_000;
const MAX_SUGGESTION_TEXT_CHARS: usize = 8_192;
const MAX_EVIDENCE: usize = 40;

/// One citation a suggestion rests on.
///
/// This is what makes a claim checkable: an employee, a run, and the step
/// inside it, so the owner can open the very reply the sentence came from
/// instead of taking the summary's word for it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub employee_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub employee_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<i64>,
    /// What this citation is about, in the board's own words.
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
}

/// What the owner approved, and what came of it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Directive {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub employee_id: Option<String>,
    pub employee_name: String,
    /// The middle manager that handed this out, when one did. Null for a
    /// directive the owner approved, which is the older and still-valid path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer_employee_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_run_id: Option<String>,
    /// After this moment the errand is stale and stays undone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_at: Option<String>,
    pub priority: i64,
    /// The run that picked it up, so an order names its own execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimed_run_id: Option<String>,
    pub board: String,
    pub verb: String,
    pub payload: Value,
    pub status: String,
    pub result_note: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
}

/// A middle manager's proposal, with the citations it rests on.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub id: String,
    pub board: String,
    pub title: String,
    pub body: String,
    /// The next action being proposed, in one line.
    pub action: String,
    /// Who or what this would change, and how much.
    pub impact: String,
    /// What it costs to do, so a suggestion without a price is visibly bare.
    pub cost_note: String,
    pub evidence: Vec<Evidence>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<Directive>,
    pub result_note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

const SUGGESTION_SELECT: &str = "SELECT id, board, title, body, action, impact, cost_note,
            evidence_json, status, directive_json, result_note, decided_at, created_at, updated_at
     FROM staff_suggestions";

const DIRECTIVE_SELECT: &str = "SELECT id, suggestion_id, employee_id, employee_name, board, verb,
            payload_json, status, result_note, created_at, updated_at, ended_at,
            issuer_employee_id, reason_run_id, deadline_at, priority, claimed_run_id
     FROM staff_directives";

fn directive_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Directive> {
    let payload: String = row.get(6)?;
    Ok(Directive {
        id: row.get(0)?,
        suggestion_id: row.get(1)?,
        employee_id: row.get(2)?,
        employee_name: row.get(3)?,
        board: row.get(4)?,
        verb: row.get(5)?,
        payload: serde_json::from_str(&payload).unwrap_or_else(|_| json!({})),
        status: row.get(7)?,
        result_note: row.get(8)?,
        created_at: ms_to_ts(row.get(9)?),
        updated_at: ms_to_ts(row.get(10)?),
        ended_at: row.get::<_, Option<i64>>(11)?.map(ms_to_ts),
        issuer_employee_id: row.get(12)?,
        reason_run_id: row.get(13)?,
        deadline_at: row.get::<_, Option<i64>>(14)?.map(ms_to_ts),
        priority: row.get(15)?,
        claimed_run_id: row.get(16)?,
    })
}

fn suggestion_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Suggestion> {
    let evidence: String = row.get(7)?;
    let directive: String = row.get(9)?;
    Ok(Suggestion {
        id: row.get(0)?,
        board: row.get(1)?,
        title: row.get(2)?,
        body: row.get(3)?,
        action: row.get(4)?,
        impact: row.get(5)?,
        cost_note: row.get(6)?,
        evidence: serde_json::from_str(&evidence).unwrap_or_default(),
        status: row.get(8)?,
        directive: serde_json::from_str::<Value>(&directive)
            .ok()
            .filter(|value| value.is_object() && value.as_object().is_some_and(|map| !map.is_empty()))
            .and_then(|value| serde_json::from_value(value).ok()),
        result_note: row.get(10)?,
        decided_at: row.get::<_, Option<i64>>(11)?.map(ms_to_ts),
        created_at: ms_to_ts(row.get(12)?),
        updated_at: ms_to_ts(row.get(13)?),
    })
}

fn board_of(input: &Value) -> Result<String> {
    let board = required_str(input, "board")?;
    if !SUGGESTION_BOARDS.contains(&board.as_str()) {
        bail!("INVALID_ARGUMENT: unknown suggestion board {board}");
    }
    Ok(board)
}

fn evidence_list(input: &Value) -> Result<Vec<Evidence>> {
    match input.get("evidence") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(value) => {
            let items = value
                .as_array()
                .ok_or_else(|| anyhow!("INVALID_ARGUMENT: evidence must be a list"))?;
            if items.len() > MAX_EVIDENCE {
                bail!("LIMIT_EXCEEDED: a suggestion cites at most {MAX_EVIDENCE} items");
            }
            let mut evidence = Vec::with_capacity(items.len());
            for item in items {
                let mut parsed: Evidence = serde_json::from_value(item.clone())
                    .context("INVALID_ARGUMENT: evidence entry is not a citation")?;
                if parsed.label.trim().is_empty() {
                    bail!("INVALID_ARGUMENT: an evidence entry needs a label");
                }
                parsed.label = parsed.label.chars().take(240).collect();
                parsed.quote = parsed
                    .quote
                    .map(|quote| quote.chars().take(2_000).collect::<String>());
                evidence.push(parsed);
            }
            Ok(evidence)
        }
    }
}

fn bound(text: String) -> String {
    if text.chars().count() <= MAX_SUGGESTION_TEXT_CHARS {
        return text;
    }
    text.chars().take(MAX_SUGGESTION_TEXT_CHARS).collect()
}

/// The suggestions a board shows, newest first.
pub fn list_suggestions(db: &Database, filter: &Value) -> Result<Vec<Suggestion>> {
    let limit = match filter.get("limit") {
        None | Some(Value::Null) => DEFAULT_SUGGESTION_LIMIT,
        Some(value) => value
            .as_i64()
            .ok_or_else(|| anyhow!("INVALID_ARGUMENT: limit must be an integer"))?
            .clamp(1, MAX_SUGGESTION_LIMIT),
    };
    let board = optional_str(filter, "board");
    let status = optional_str(filter, "status");
    let sql = format!(
        "{SUGGESTION_SELECT}
         WHERE (?1 IS NULL OR board = ?1)
           AND (?2 IS NULL OR status = ?2)
         ORDER BY created_at DESC, id
         LIMIT ?3"
    );
    let mut stmt = db.conn().prepare(&sql)?;
    let rows = stmt.query_map(params![board, status, limit], suggestion_from_row)?;
    let mut suggestions = Vec::new();
    for row in rows {
        suggestions.push(row?);
    }

    // The directive is attached from the table rather than only from
    // `directive_json`, so a suggestion and its directive can never disagree
    // about what was actually handed to whom.
    let mut stmt = db.conn().prepare(&format!(
        "{DIRECTIVE_SELECT} WHERE suggestion_id IS NOT NULL ORDER BY created_at DESC, id"
    ))?;
    let rows = stmt.query_map([], directive_from_row)?;
    let mut latest: HashMap<String, Directive> = HashMap::new();
    for row in rows {
        let directive = row?;
        if let Some(suggestion_id) = directive.suggestion_id.clone() {
            latest.entry(suggestion_id).or_insert(directive);
        }
    }
    for suggestion in &mut suggestions {
        if let Some(directive) = latest.get(&suggestion.id) {
            suggestion.directive = Some(directive.clone());
        }
    }
    Ok(suggestions)
}

/// Write a proposal. Any board, or a run on a board's behalf, may do this.
pub fn create_suggestion(db: &Database, input: &Value) -> Result<Suggestion> {
    let board = board_of(input)?;
    let title = required_str(input, "title")?;
    let evidence = evidence_list(input)?;
    // The work order this proposal would become, if accepted. Stored from birth
    // rather than invented at decision time, because the board that read the
    // rows is the one that knows whose job the follow-up is: a draft awaiting
    // approval is the owner's to approve, and re-triaging missed replies is the
    // receptionist's to run.
    let plan = input
        .get("directive")
        .cloned()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    let suggestion_id = Uuid::new_v4().to_string();
    let now = now_ms();
    let text = |key: &str| -> String {
        optional_str(input, key)
            .map(bound)
            .unwrap_or_default()
    };
    db.conn().execute(
        "INSERT INTO staff_suggestions
           (id, board, title, body, action, impact, cost_note, evidence_json, status,
            directive_json, result_note, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'open', ?9, '', ?10, ?10)",
        params![
            suggestion_id,
            board,
            bound(title),
            text("body"),
            text("action"),
            text("impact"),
            text("costNote"),
            serde_json::to_string(&evidence)?,
            plan.to_string(),
            now,
        ],
    )?;
    get_suggestion(db, &suggestion_id)?
        .ok_or_else(|| anyhow!("NOT_FOUND: suggestion {suggestion_id} vanished after write"))
}

pub fn get_suggestion(db: &Database, id: &str) -> Result<Option<Suggestion>> {
    let mut stmt = db
        .conn()
        .prepare(&format!("{SUGGESTION_SELECT} WHERE id = ?1"))?;
    let mut suggestion = match stmt.query_row(params![id], suggestion_from_row).optional()? {
        Some(suggestion) => suggestion,
        None => return Ok(None),
    };
    let mut stmt = db.conn().prepare(&format!(
        "{DIRECTIVE_SELECT} WHERE suggestion_id = ?1 ORDER BY created_at DESC, id LIMIT 1"
    ))?;
    suggestion.directive = stmt
        .query_row(params![id], directive_from_row)
        .optional()?;
    Ok(Some(suggestion))
}

/// The owner's answer, and, when it is yes, the directive it becomes.
///
/// Rejecting is final on the row but not destructive: the suggestion stays
/// with its citations, because "we decided not to" is the most useful thing a
/// board can tell the next analysis.
pub fn decide_suggestion(db: &Database, input: &Value) -> Result<Option<Suggestion>> {
    let suggestion_id = required_str(input, "id")?;
    let Some(current) = get_suggestion(db, &suggestion_id)? else {
        return Ok(None);
    };
    if current.status != "open" {
        bail!(
            "INVALID_ARGUMENT: suggestion {suggestion_id} was already decided ({})",
            current.status
        );
    }
    let decision = required_str(input, "decision")?;
    if decision != "accepted" && decision != "rejected" {
        bail!("INVALID_ARGUMENT: decision must be accepted or rejected");
    }
    let now = now_ms();
    let mut directive = Value::Object(serde_json::Map::new());
    if decision == "accepted" {
        let plan = merge_plan(stored_plan(db, &suggestion_id)?, input);
        let created = create_directive(db, &current, &plan)?;
        directive = serde_json::to_value(&created)?;
    }
    db.conn().execute(
        "UPDATE staff_suggestions SET status = ?2, directive_json = ?3, result_note = ?4,
           decided_at = ?5, updated_at = ?5
         WHERE id = ?1",
        params![
            suggestion_id,
            decision,
            directive.to_string(),
            optional_str(input, "note").unwrap_or_default(),
            now,
        ],
    )?;
    get_suggestion(db, &suggestion_id)
}

/// The work order a proposal was raised with.
fn stored_plan(db: &Database, id: &str) -> Result<Value> {
    let raw: Option<String> = db
        .conn()
        .prepare_cached("SELECT directive_json FROM staff_suggestions WHERE id = ?1")?
        .query_row(params![id], |row| row.get(0))
        .optional()?;
    Ok(raw
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({})))
}

/// The stored plan with the caller's overrides laid over it.
///
/// Merged rather than replaced: the owner answering a proposal may want to send
/// it to someone other than the board's default, and should not have to restate
/// the verb to do it.
fn merge_plan(mut plan: Value, input: &Value) -> Value {
    let overrides = input
        .get("directive")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(base) = plan.as_object_mut() {
        for (key, value) in &overrides {
            base.insert(key.clone(), value.clone());
        }
    }
    plan
}

/// Turn an accepted suggestion into one piece of work with a named owner.
fn create_directive(db: &Database, suggestion: &Suggestion, plan: &Value) -> Result<Directive> {
    let directive_id = Uuid::new_v4().to_string();
    let now = now_ms();
    // A plan that mentions a worker is taken at its word, including when the
    // word is "nobody": approving a draft is the owner's own act, and wording
    // that only the person reading the card can carry out must not be handed to
    // whichever employee happened to file the evidence.
    let names_a_worker = plan.get("employeeId").is_some() || plan.get("employeeName").is_some();
    let (employee_id, employee_name) = if names_a_worker {
        (
            plan.get("employeeId")
                .and_then(Value::as_str)
                .map(str::to_string),
            plan.get("employeeName")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        )
    } else {
        let first = suggestion.evidence.first();
        (
            first.and_then(|e| e.employee_id.clone()),
            first
                .and_then(|e| e.employee_name.clone())
                .unwrap_or_default(),
        )
    };
    db.conn().execute(
        "INSERT INTO staff_directives
           (id, suggestion_id, employee_id, employee_name, board, verb, payload_json, status,
            result_note, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'queued', '', ?8, ?8)",
        params![
            directive_id,
            suggestion.id,
            employee_id,
            employee_name,
            suggestion.board,
            optional_str(plan, "verb").unwrap_or_else(|| suggestion.action.clone()),
            plan.get("payload")
                .cloned()
                .unwrap_or_else(|| json!({}))
                .to_string(),
            now,
        ],
    )?;
    get_directive(db, &directive_id)?
        .ok_or_else(|| anyhow!("NOT_FOUND: directive {directive_id} vanished after write"))
}

pub fn get_directive(db: &Database, id: &str) -> Result<Option<Directive>> {
    let mut stmt = db
        .conn()
        .prepare(&format!("{DIRECTIVE_SELECT} WHERE id = ?1"))?;
    Ok(stmt.query_row(params![id], directive_from_row).optional()?)
}

pub fn list_directives(db: &Database, filter: &Value) -> Result<Vec<Directive>> {
    let limit = match filter.get("limit") {
        None | Some(Value::Null) => DEFAULT_DIRECTIVE_LIMIT,
        Some(value) => value
            .as_i64()
            .ok_or_else(|| anyhow!("INVALID_ARGUMENT: limit must be an integer"))?
            .clamp(1, MAX_DIRECTIVE_LIMIT),
    };
    let status = optional_str(filter, "status");
    let employee_id = optional_str(filter, "employeeId");
    let board = optional_str(filter, "board");
    let issuer = optional_str(filter, "issuerEmployeeId");
    let sql = format!(
        "{DIRECTIVE_SELECT}
         WHERE (?1 IS NULL OR status = ?1)
           AND (?2 IS NULL OR employee_id = ?2)
           AND (?3 IS NULL OR board = ?3)
           AND (?4 IS NULL OR issuer_employee_id = ?4)
         ORDER BY created_at DESC, id
         LIMIT ?5"
    );
    let mut stmt = db.conn().prepare(&sql)?;
    let rows = stmt.query_map(params![status, employee_id, board, issuer, limit], directive_from_row)?;
    let mut directives = Vec::new();
    for row in rows {
        directives.push(row?);
    }
    Ok(directives)
}

/// What became of a directive. The employee that ran it reports here, and the
/// suggestion it came from is closed with the same sentence.
pub fn update_directive(db: &Database, input: &Value) -> Result<Option<Directive>> {
    let directive_id = required_str(input, "id")?;
    let Some(current) = get_directive(db, &directive_id)? else {
        return Ok(None);
    };
    let status = match optional_str(input, "status") {
        Some(status) => {
            if !DIRECTIVE_STATUSES.contains(&status.as_str()) {
                bail!("INVALID_ARGUMENT: unknown directive status {status}");
            }
            status
        }
        None => current.status.clone(),
    };
    let result_note = optional_str(input, "resultNote").unwrap_or(current.result_note);
    let now = now_ms();
    let ended: Option<i64> = if status == "done" || status == "failed" {
        // A directive that reports twice keeps the moment it first finished:
        // the later note is a correction, not a second completion.
        match current.ended_at.as_deref() {
            Some(text) => Some(ts_to_ms(text)),
            None => Some(now),
        }
    } else {
        current.ended_at.as_deref().map(ts_to_ms)
    };
    db.conn().execute(
        "UPDATE staff_directives SET status = ?2, result_note = ?3, ended_at = ?4, updated_at = ?5
         WHERE id = ?1",
        params![directive_id, status, result_note, ended, now],
    )?;
    if ended.is_some() {
        if let Some(suggestion_id) = &current.suggestion_id {
            db.conn().execute(
                "UPDATE staff_suggestions SET status = 'done', result_note = ?2, updated_at = ?3
                 WHERE id = ?1 AND status = 'accepted'",
                params![suggestion_id, result_note, now],
            )?;
        }
    }
    get_directive(db, &directive_id)
}
