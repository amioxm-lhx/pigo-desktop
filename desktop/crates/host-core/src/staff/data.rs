//! The data ledger: how much each station's output weighs, and what a cleanup
//! would take.
//!
//! Written for the board rather than for the disk. The AI team's own tables run
//! to about a megabyte here while the conversation transcript beside them is
//! more than ten times that, so a stage is worth clearing when it drowns the
//! board it is read on -- never on a timer, and never by a rule the owner has
//! not picked.
//!
//! Two things a rule may not touch. It may not take a row something downstream
//! points at: a draft a proposal cites, a card a draft answers. And it may not
//! take the only record of something that happened in the world: a published
//! piece keeps its address and its counters forever, because that address is
//! what the return leg reads back.
//!
//! The numbers are measured, never estimated. `LENGTH(CAST(x AS BLOB))` on
//! SQLite returns real bytes, where `LENGTH(x)` returns characters and would
//! undercount Chinese by a third.

use super::*;
use serde_json::json;

/// How long each kind of row is worth keeping, in days.
///
/// A published piece has no entry here on purpose: its address is the thing the
/// watch reads back, so there is no age at which it stops being useful.
pub const RETENTION_LEADS_DAYS: i64 = 30;
pub const RETENTION_CONTENTS_DAYS: i64 = 30;
pub const RETENTION_DISPATCH_DAYS: i64 = 90;
pub const RETENTION_METRICS_DAYS: i64 = 180;

/// Runs keep their row; what a sweep takes is the bulky per-step report of runs
/// long past. Thirty is the window in which "why did it post that" still gets
/// asked.
pub const RETENTION_RUNS_KEPT: i64 = 30;

const DAY_MS: i64 = 86_400_000;

/// The rules a cleanup may run, in the order the page offers them.
pub const PRUNE_RULES: [&str; 6] = [
    "contents-retired",
    "leads-dead",
    "dispatch-answered",
    "metrics-aged",
    "runs-reports",
    "memory-expired",
];

/// The stages the ledger reports, in the order the work travels.
const LEDGER_STAGES: [&str; 8] = [
    "scan", "leads", "contents", "publish", "metrics", "dispatch", "runs", "memory",
];

/// The text columns of a table, as one byte expression.
fn bytes_of(columns: &[&str]) -> String {
    columns
        .iter()
        .map(|column| format!("LENGTH(CAST(COALESCE({column}, '') AS BLOB))"))
        .collect::<Vec<_>>()
        .join(" + ")
}

/// Binds for a predicate shaped `... created_at < ?n`, where the placeholders
/// before the last one are the cited run ids the rule must leave alone.
type Binds<'a> = &'a [&'a dyn rusqlite::ToSql];

/// Row count and real byte size of the rows a predicate selects.
///
/// Measured with the same binds the delete will use, so "would take 12" and
/// "took 12" cannot drift apart: a rule that reported one number and deleted
/// another would be a bug the owner could not see.
fn weigh(
    db: &Database,
    table: &str,
    columns: &[&str],
    predicate: &str,
    binds: Binds<'_>,
) -> Result<(i64, i64)> {
    let sql = format!(
        "SELECT COUNT(*), COALESCE(SUM({}), 0) FROM {table} WHERE {predicate}",
        bytes_of(columns)
    );
    let mut stmt = db.conn().prepare(&sql)?;
    let answer = stmt.query_row(binds, |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(answer)
}

/// Every run id a proposal still cites as its evidence.
///
/// A cleanup may not take the row a claim rests on: that citation is the only
/// reason anybody would ever open the draft again, and the console draws it.
fn cited_run_ids(db: &Database) -> Result<Vec<String>> {
    let mut stmt = db
        .conn()
        .prepare("SELECT evidence_json FROM staff_suggestions")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut ids = std::collections::BTreeSet::new();
    for row in rows {
        let parsed: Value =
            serde_json::from_str(&row?).unwrap_or_else(|_| Value::Array(Vec::new()));
        for entry in parsed.as_array().into_iter().flatten() {
            if let Some(run_id) = entry.get("runId").and_then(Value::as_str) {
                if !run_id.is_empty() {
                    ids.insert(run_id.to_string());
                }
            }
        }
    }
    Ok(ids.into_iter().collect())
}

/// `IN (?, ?, ...)`, or a clause that matches everything when the list is empty.
///
/// Built from placeholders rather than interpolated: a run id is a value, and
/// the one time it stops being treated as one is the one time this deletes the
/// wrong rows.
fn not_in(column: &str, values: &[String]) -> String {
    if values.is_empty() {
        return format!("({column} IS NULL OR 1)");
    }
    let list = values.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    format!("({column} IS NULL OR {column} NOT IN ({list}))")
}

/// The cited ids as binds, followed by the cutoff the predicate ends with.
fn binds_for<'a>(cited: &'a [String], cutoff: &'a i64) -> Vec<&'a dyn rusqlite::ToSql> {
    let mut binds: Vec<&dyn rusqlite::ToSql> = cited.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
    binds.push(cutoff);
    binds
}

/// The predicate a dead card has to satisfy, and the ids it must leave alone.
fn dead_lead_predicate(db: &Database) -> Result<(String, Vec<String>)> {
    let cited = cited_run_ids(db)?;
    let clause = format!(
        "stage IN ('dropped', 'lost')
         AND id NOT IN (SELECT lead_id FROM staff_contents WHERE lead_id IS NOT NULL)
         AND {} AND created_at < ?{}",
        not_in("run_id", &cited),
        cited.len() + 1
    );
    Ok((clause, cited))
}

/// The predicate a retired draft has to satisfy, and the ids it must leave alone.
fn retired_content_predicate(db: &Database) -> Result<(String, Vec<String>)> {
    let cited = cited_run_ids(db)?;
    const LEADLESS: &str = "(lead_id IS NULL OR lead_id = '')";
    let clause = format!(
        "status = 'retired' AND {LEADLESS} AND {} AND created_at < ?{}",
        not_in("run_id", &cited),
        cited.len() + 1
    );
    Ok((clause, cited))
}

/// Old runs that still carry the bulky part of their log.
fn stale_run_reports(db: &Database, keep: i64) -> Result<(i64, i64)> {
    let sql = "SELECT COUNT(*), COALESCE(SUM(LENGTH(CAST(reports_json AS BLOB))), 0)
               FROM (
                 SELECT reports_json,
                        ROW_NUMBER() OVER (
                          PARTITION BY employee_id ORDER BY started_at DESC
                        ) AS age
                 FROM employee_runs
               )
               WHERE age > ?1 AND reports_json <> '{}'";
    let mut stmt = db.conn().prepare(sql)?;
    let answer = stmt.query_row(params![keep], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(answer)
}

/// Memory whose own expiry has passed but that nothing has retired yet.
fn expired_memory(db: &Database) -> Result<(i64, i64)> {
    let mut stmt = db.conn().prepare(
        "SELECT COUNT(*), COALESCE(SUM(LENGTH(CAST(COALESCE(memory_key, '') AS BLOB))
                                       + LENGTH(CAST(COALESCE(content, '') AS BLOB))
                                       + LENGTH(CAST(COALESCE(source_ref, '') AS BLOB))), 0)
         FROM memory_items
         WHERE status = 'active' AND expires_at IS NOT NULL AND expires_at < ?1",
    )?;
    let answer = stmt.query_row(params![now_ms()], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(answer)
}

/// One stage's line.
///
/// `rows`/`bytes` are what the stage is still working with. `parked` is what it
/// wrote, still keeps on disk, and nothing is using any more -- a retired draft
/// is a row on disk like any other, so a ledger that counted only the live ones
/// would print "0" over a table full of text. `cleanable` is always a subset of
/// the parked rows: what has aged past its own retention window.
fn stage_json(
    id: &str,
    rows: i64,
    bytes: i64,
    parked_rows: i64,
    parked_bytes: i64,
    cleanable: i64,
    cleanable_bytes: i64,
) -> Value {
    json!({
        "id": id,
        "rows": rows,
        "bytes": bytes,
        "parkedRows": parked_rows,
        "parkedBytes": parked_bytes,
        "cleanable": cleanable,
        "cleanableBytes": cleanable_bytes,
    })
}

/// What every station holds, what a cleanup would take, and how big the file is.
///
/// One call rather than two, because the page draws the two answers side by
/// side and a rule whose "would take" disagreed with its "did take" would be a
/// bug the owner could not see.
pub fn data_stats(db: &Database, params: &Value) -> Result<Value> {
    let now = now_ms();
    let days = |key: &str, fallback: i64| -> Result<i64> {
        match params.get(key) {
            None | Some(Value::Null) => Ok(fallback),
            Some(value) => Ok(value
                .as_i64()
                .ok_or_else(|| anyhow!("INVALID_ARGUMENT: {key} must be an integer"))?
                .clamp(1, 3_650)),
        }
    };
    let lead_days = days("leadDays", RETENTION_LEADS_DAYS)?;
    let content_days = days("contentDays", RETENTION_CONTENTS_DAYS)?;
    let dispatch_days = days("dispatchDays", RETENTION_DISPATCH_DAYS)?;
    let metric_days = days("metricDays", RETENTION_METRICS_DAYS)?;
    let runs_kept = days("runsKept", RETENTION_RUNS_KEPT)?;

    let lead_cutoff = now - lead_days * DAY_MS;
    let content_cutoff = now - content_days * DAY_MS;
    let dispatch_cutoff = now - dispatch_days * DAY_MS;
    let metric_cutoff = now - metric_days * DAY_MS;

    const LEAD_COLUMNS: [&str; 9] = [
        "title", "site", "url", "author", "quote", "topic", "angle", "tags_json", "note",
    ];
    const CONTENT_COLUMNS: [&str; 6] =
        ["title", "channel", "topic", "body", "variant", "published_url"];
    const SUGGESTION_COLUMNS: [&str; 7] = [
        "title", "body", "action", "impact", "cost_note", "evidence_json", "result_note",
    ];
    const DIRECTIVE_COLUMNS: [&str; 5] =
        ["employee_name", "board", "verb", "payload_json", "result_note"];
    const METRIC_COLUMNS: [&str; 3] = ["day", "dimension", "label"];
    const RUN_COLUMNS: [&str; 3] = ["steps_json", "results_json", "reports_json"];
    const MEMORY_COLUMNS: [&str; 3] = ["memory_key", "content", "source_ref"];

    let mut stages: Vec<Value> = Vec::new();

    // 巡站 keeps nothing at all: the pages a sweep reads are part of its prompt
    // and are gone when the run ends. A zero here would read as a fault, and it
    // is the opposite -- not keeping them is what keeps this file small.
    stages.push(stage_json("scan", 0, 0, 0, 0, 0, 0));

    // ---- cards -------------------------------------------------------------
    let (rows, bytes) = weigh(db, "staff_leads", &LEAD_COLUMNS, "1", &[])?;
    let (dead_clause, dead_cited) = dead_lead_predicate(db)?;
    let dead_binds = binds_for(&dead_cited, &lead_cutoff);
    let (cleanable, cleanable_bytes) =
        weigh(db, "staff_leads", &LEAD_COLUMNS, &dead_clause, &dead_binds)?;
    stages.push(stage_json("leads", rows, bytes, 0, 0, cleanable, cleanable_bytes));

    // ---- drafts, and the pieces that left the building ---------------------
    let (rows, bytes) = weigh(
        db,
        "staff_contents",
        &CONTENT_COLUMNS,
        "status <> 'retired' AND status <> 'published'",
        &[],
    )?;
    let (parked_rows, parked_bytes) = weigh(
        db,
        "staff_contents",
        &CONTENT_COLUMNS,
        "status = 'retired'",
        &[],
    )?;
    let (retired_clause, retired_cited) = retired_content_predicate(db)?;
    let retired_binds = binds_for(&retired_cited, &content_cutoff);
    let (cleanable, cleanable_bytes) = weigh(
        db,
        "staff_contents",
        &CONTENT_COLUMNS,
        &retired_clause,
        &retired_binds,
    )?;
    stages.push(stage_json(
        "contents",
        rows,
        bytes,
        parked_rows,
        parked_bytes,
        cleanable,
        cleanable_bytes,
    ));

    let (rows, bytes) = weigh(
        db,
        "staff_contents",
        &CONTENT_COLUMNS,
        "status = 'published'",
        &[],
    )?;
    stages.push(stage_json("publish", rows, bytes, 0, 0, 0, 0));

    // ---- site numbers ------------------------------------------------------
    let (rows, bytes) = weigh(db, "staff_metrics", &METRIC_COLUMNS, "1", &[])?;
    let (cleanable, cleanable_bytes) = weigh(
        db,
        "staff_metrics",
        &METRIC_COLUMNS,
        "created_at < ?1",
        &[&metric_cutoff],
    )?;
    stages.push(stage_json("metrics", rows, bytes, 0, 0, cleanable, cleanable_bytes));

    // ---- what the middle managers said, and what came of it ----------------
    let (a_rows, a_bytes) = weigh(db, "staff_suggestions", &SUGGESTION_COLUMNS, "1", &[])?;
    let (d_rows, d_bytes) = weigh(db, "staff_directives", &DIRECTIVE_COLUMNS, "1", &[])?;
    let (a_take, a_take_bytes) = weigh(
        db,
        "staff_suggestions",
        &SUGGESTION_COLUMNS,
        "status <> 'open' AND created_at < ?1",
        &[&dispatch_cutoff],
    )?;
    let (d_take, d_take_bytes) = weigh(
        db,
        "staff_directives",
        &DIRECTIVE_COLUMNS,
        "status NOT IN ('queued', 'running') AND created_at < ?1",
        &[&dispatch_cutoff],
    )?;
    stages.push(stage_json(
        "dispatch",
        a_rows + d_rows,
        a_bytes + d_bytes,
        0,
        0,
        a_take + d_take,
        a_take_bytes + d_take_bytes,
    ));

    // ---- run logs ----------------------------------------------------------
    let (rows, bytes) = weigh(db, "employee_runs", &RUN_COLUMNS, "1", &[])?;
    let (cleanable, cleanable_bytes) = stale_run_reports(db, runs_kept)?;
    stages.push(stage_json("runs", rows, bytes, 0, 0, cleanable, cleanable_bytes));

    // ---- shared memory -----------------------------------------------------
    let (live_rows, live_bytes) = weigh(
        db,
        "memory_items",
        &MEMORY_COLUMNS,
        "status = 'active'",
        &[],
    )?;
    let (parked_rows, parked_bytes) = weigh(
        db,
        "memory_items",
        &MEMORY_COLUMNS,
        "status <> 'active'",
        &[],
    )?;
    let (cleanable, cleanable_bytes) = expired_memory(db)?;
    stages.push(stage_json(
        "memory",
        live_rows,
        live_bytes,
        parked_rows,
        parked_bytes,
        cleanable,
        cleanable_bytes,
    ));

    debug_assert_eq!(stages.len(), LEDGER_STAGES.len());

    // The file itself, so the page can say what share of it is this team.
    let page_count: i64 = db.conn().query_row("PRAGMA page_count", [], |row| row.get(0))?;
    let page_size: i64 = db.conn().query_row("PRAGMA page_size", [], |row| row.get(0))?;

    Ok(json!({
        "stages": stages,
        "database": { "bytes": page_count * page_size },
        "retention": {
            "leads": lead_days,
            "contents": content_days,
            "dispatch": dispatch_days,
            "metrics": metric_days,
            "runsKept": runs_kept,
            "runs": RETENTION_RUNS_KEPT,
        },
    }))
}

/// Take the rows a rule has been asked for, and say what was taken.
///
/// `dryRun` measures without deleting, which is what the page's first pass
/// does: the owner picks rules off a list that already says what each one
/// costs, so confirming is never a leap.
pub fn prune(db: &Database, params: &Value) -> Result<Value> {
    let dry_run = params
        .get("dryRun")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let requested: Vec<String> = match params.get("rules") {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => bail!("INVALID_ARGUMENT: rules is required"),
    };
    if requested.is_empty() {
        bail!("INVALID_ARGUMENT: rules is required");
    }
    for rule in &requested {
        if !PRUNE_RULES.contains(&rule.as_str()) {
            bail!("INVALID_ARGUMENT: unknown prune rule {rule}");
        }
    }

    const LEAD_COLUMNS: [&str; 9] = [
        "title", "site", "url", "author", "quote", "topic", "angle", "tags_json", "note",
    ];
    const CONTENT_COLUMNS: [&str; 6] =
        ["title", "channel", "topic", "body", "variant", "published_url"];
    const SUGGESTION_COLUMNS: [&str; 7] = [
        "title", "body", "action", "impact", "cost_note", "evidence_json", "result_note",
    ];
    const DIRECTIVE_COLUMNS: [&str; 5] =
        ["employee_name", "board", "verb", "payload_json", "result_note"];
    const METRIC_COLUMNS: [&str; 3] = ["day", "dimension", "label"];

    let now = now_ms();
    let mut taken = Vec::new();

    for rule in &requested {
        let outcome = match rule.as_str() {
            "contents-retired" => {
                let cutoff = now - RETENTION_CONTENTS_DAYS * DAY_MS;
                let (clause, cited) = retired_content_predicate(db)?;
                let binds = binds_for(&cited, &cutoff);
                let measured = weigh(db, "staff_contents", &CONTENT_COLUMNS, &clause, &binds)?;
                if !dry_run {
                    db.conn()
                        .execute(&format!("DELETE FROM staff_contents WHERE {clause}"), &binds[..])?;
                }
                json!({ "rule": rule, "rows": measured.0, "bytes": measured.1 })
            }
            "leads-dead" => {
                let cutoff = now - RETENTION_LEADS_DAYS * DAY_MS;
                let (clause, cited) = dead_lead_predicate(db)?;
                let binds = binds_for(&cited, &cutoff);
                let measured = weigh(db, "staff_leads", &LEAD_COLUMNS, &clause, &binds)?;
                if !dry_run {
                    db.conn()
                        .execute(&format!("DELETE FROM staff_leads WHERE {clause}"), &binds[..])?;
                }
                json!({ "rule": rule, "rows": measured.0, "bytes": measured.1 })
            }
            "dispatch-answered" => {
                let cutoff = now - RETENTION_DISPATCH_DAYS * DAY_MS;
                let binds: Binds<'_> = &[&cutoff];
                let mut rows = 0;
                let mut bytes = 0;
                for (table, columns, predicate) in [
                    (
                        "staff_suggestions",
                        &SUGGESTION_COLUMNS[..],
                        "status <> 'open' AND created_at < ?1",
                    ),
                    (
                        "staff_directives",
                        &DIRECTIVE_COLUMNS[..],
                        "status NOT IN ('queued', 'running') AND created_at < ?1",
                    ),
                ] {
                    let measured = weigh(db, table, columns, predicate, binds)?;
                    rows += measured.0;
                    bytes += measured.1;
                    if !dry_run {
                        db.conn()
                            .execute(&format!("DELETE FROM {table} WHERE {predicate}"), binds)?;
                    }
                }
                json!({ "rule": rule, "rows": rows, "bytes": bytes })
            }
            "metrics-aged" => {
                let cutoff = now - RETENTION_METRICS_DAYS * DAY_MS;
                let binds: Binds<'_> = &[&cutoff];
                let measured =
                    weigh(db, "staff_metrics", &METRIC_COLUMNS, "created_at < ?1", binds)?;
                if !dry_run {
                    db.conn()
                        .execute("DELETE FROM staff_metrics WHERE created_at < ?1", binds)?;
                }
                json!({ "rule": rule, "rows": measured.0, "bytes": measured.1 })
            }
            "runs-reports" => {
                // The row stays and keeps the one-line result of every step,
                // which is what the boards cite. What goes is `reports`, the
                // full reply of each step: most of the bytes, read by nobody
                // after a month.
                let measured = stale_run_reports(db, RETENTION_RUNS_KEPT)?;
                if !dry_run {
                    db.conn().execute(
                        "UPDATE employee_runs SET reports_json = '{}'
                         WHERE id IN (
                           SELECT id FROM (
                             SELECT id, ROW_NUMBER() OVER (
                               PARTITION BY employee_id ORDER BY started_at DESC
                             ) AS age
                             FROM employee_runs
                           )
                           WHERE age > ?1
                         )",
                        params![RETENTION_RUNS_KEPT],
                    )?;
                }
                json!({ "rule": rule, "rows": measured.0, "bytes": measured.1 })
            }
            "memory-expired" => {
                let measured = expired_memory(db)?;
                if !dry_run {
                    db.conn().execute(
                        "UPDATE memory_items SET status = 'retired'
                         WHERE status = 'active' AND expires_at IS NOT NULL AND expires_at < ?1",
                        params![now],
                    )?;
                }
                json!({ "rule": rule, "rows": measured.0, "bytes": measured.1 })
            }
            other => bail!("INVALID_ARGUMENT: unknown prune rule {other}"),
        };
        taken.push(outcome);
    }

    Ok(json!({ "dryRun": dry_run, "taken": taken }))
}