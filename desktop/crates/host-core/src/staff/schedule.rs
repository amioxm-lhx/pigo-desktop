use super::*;
use serde_json::json;

/// A schedule shorter than this is a runaway, not a plan.
pub const MIN_EVERY_MINUTES: i64 = 5;
pub const MAX_EVERY_MINUTES: i64 = 7 * 24 * 60;
/// Safety net for an unattended employee: even with no explicit quota, one
/// employee cannot start more than this many runs in a rolling day.
pub const DEFAULT_MAX_RUNS_PER_DAY: i64 = 96;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const BACKOFF_BASE_MS: i64 = 5 * 60 * 1000;
const BACKOFF_MAX_MS: i64 = 6 * 60 * 60 * 1000;

/// When an employee runs. Stored on the employee as schedule_json.
#[derive(Debug, Clone, PartialEq)]
pub struct Schedule {
    pub every_minutes: Option<i64>,
    pub jitter_minutes: i64,
    /// Minutes from local midnight, ascending.
    pub times_of_day: Vec<i64>,
    pub utc_offset_minutes: i64,
}

fn minute_of_day(raw: &str) -> Result<i64> {
    let (hours, minutes) = raw
        .split_once(':')
        .ok_or_else(|| anyhow!("INVALID_ARGUMENT: time of day {raw} must be HH:MM"))?;
    let hours: i64 = hours
        .parse()
        .map_err(|_| anyhow!("INVALID_ARGUMENT: time of day {raw} must be HH:MM"))?;
    let minutes: i64 = minutes
        .parse()
        .map_err(|_| anyhow!("INVALID_ARGUMENT: time of day {raw} must be HH:MM"))?;
    if !(0..24).contains(&hours) || !(0..60).contains(&minutes) {
        bail!("INVALID_ARGUMENT: time of day {raw} is out of range");
    }
    Ok(hours * 60 + minutes)
}

impl Schedule {
    /// Parse schedule_json. An empty object means "never scheduled", which is
    /// how a manually triggered employee is expressed.
    pub fn parse(value: &Value) -> Result<Option<Schedule>> {
        if value.is_null() {
            return Ok(None);
        }
        let Some(object) = value.as_object() else {
            bail!("INVALID_ARGUMENT: schedule must be an object");
        };
        let every_minutes = match object.get("everyMinutes") {
            None | Some(Value::Null) => None,
            Some(raw) => {
                let minutes = raw
                    .as_i64()
                    .ok_or_else(|| anyhow!("INVALID_ARGUMENT: everyMinutes must be an integer"))?;
                if !(MIN_EVERY_MINUTES..=MAX_EVERY_MINUTES).contains(&minutes) {
                    bail!(
                        "INVALID_ARGUMENT: everyMinutes must be {MIN_EVERY_MINUTES}..={MAX_EVERY_MINUTES}"
                    );
                }
                Some(minutes)
            }
        };
        let jitter_minutes = match object.get("jitterMinutes") {
            None | Some(Value::Null) => 0,
            Some(raw) => {
                let jitter = raw
                    .as_i64()
                    .ok_or_else(|| anyhow!("INVALID_ARGUMENT: jitterMinutes must be an integer"))?;
                if !(0..=1440).contains(&jitter) {
                    bail!("INVALID_ARGUMENT: jitterMinutes must be 0..=1440");
                }
                jitter
            }
        };
        let utc_offset_minutes = match object.get("utcOffsetMinutes") {
            None | Some(Value::Null) => 0,
            Some(raw) => {
                let offset = raw.as_i64().ok_or_else(|| {
                    anyhow!("INVALID_ARGUMENT: utcOffsetMinutes must be an integer")
                })?;
                if !(-14 * 60..=14 * 60).contains(&offset) {
                    bail!("INVALID_ARGUMENT: utcOffsetMinutes is out of range");
                }
                offset
            }
        };
        let mut times_of_day = Vec::new();
        if let Some(raw) = object.get("timesOfDay") {
            let list = raw
                .as_array()
                .ok_or_else(|| anyhow!("INVALID_ARGUMENT: timesOfDay must be an array"))?;
            if list.len() > 24 {
                bail!("LIMIT_EXCEEDED: timesOfDay holds at most 24 entries");
            }
            for entry in list {
                let text = entry.as_str().ok_or_else(|| {
                    anyhow!("INVALID_ARGUMENT: timesOfDay entries are HH:MM strings")
                })?;
                times_of_day.push(minute_of_day(text)?);
            }
            times_of_day.sort_unstable();
            times_of_day.dedup();
        }
        if every_minutes.is_none() && times_of_day.is_empty() {
            return Ok(None);
        }
        if every_minutes.is_some() && !times_of_day.is_empty() {
            bail!("INVALID_ARGUMENT: use either everyMinutes or timesOfDay, not both");
        }
        Ok(Some(Schedule {
            every_minutes,
            jitter_minutes,
            times_of_day,
            utc_offset_minutes,
        }))
    }

    pub fn is_empty(&self) -> bool {
        self.every_minutes.is_none() && self.times_of_day.is_empty()
    }

    /// The first run time strictly after now.
    ///
    /// The anchor is the employee creation time, which keeps an interval
    /// schedule on a fixed grid: restarts on the same day cannot drift the run
    /// into a different slot, so "every 6 hours" means the same 6 hours every
    /// day instead of "6 hours after whenever the app happened to start".
    pub fn next_after(&self, now: i64, anchor: i64, employee_id: &str) -> Option<i64> {
        if self.is_empty() {
            return None;
        }
        if !self.times_of_day.is_empty() {
            let local_now = now + self.utc_offset_minutes * 60_000;
            let local_day_start = local_now.div_euclid(DAY_MS) * DAY_MS;
            for day in 0..=1 {
                let day_start = local_day_start + day * DAY_MS;
                for minute in &self.times_of_day {
                    let candidate = day_start + minute * 60_000 - self.utc_offset_minutes * 60_000;
                    if candidate > now {
                        return Some(candidate);
                    }
                }
            }
            return None;
        }
        let period = self.every_minutes? * 60_000;
        let anchor = anchor.min(now);
        let slot = (now - anchor).div_euclid(period) + 1;
        let base = anchor + slot * period;
        if self.jitter_minutes == 0 {
            return Some(base);
        }
        // Jitter is derived from the employee id and the slot, never from a
        // random source: a restart must not reschedule the same run.
        let span = self.jitter_minutes * 60_000;
        Some(base + (fnv1a(employee_id, slot) % span as u64) as i64)
    }

    /// The interval used to space runs out for quota and backoff purposes.
    fn nominal_period_ms(&self) -> i64 {
        match self.every_minutes {
            Some(minutes) => minutes * 60_000,
            None => DAY_MS,
        }
    }
}

fn fnv1a(seed: &str, slot: i64) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.as_bytes().iter().chain(slot.to_le_bytes().iter()) {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub struct StartedRun {
    pub run: EmployeeRun,
    pub steps: Vec<WorkflowStep>,
}

/// Start one run for an employee: bind credentials, open a session, snapshot the
/// workflow version, and record the run.
///
/// Every failure past the session is cleaned up, so a refused run never leaves a
/// session behind for the user to find and wonder about.
pub fn start_run(
    ctx: &StaffContext<'_>,
    employee_id: &str,
    trigger: &str,
    session_id: Option<&str>,
) -> Result<StartedRun> {
    start_run_for(ctx, employee_id, trigger, session_id, None)
}

/// Start one run that is carrying out a manager's order. The directive rides
/// with the run from here on, so the order and its execution cannot be told
/// apart later.
pub fn start_run_for(
    ctx: &StaffContext<'_>,
    employee_id: &str,
    trigger: &str,
    session_id: Option<&str>,
    directive_id: Option<&str>,
) -> Result<StartedRun> {
    let binding = resolve_run_binding(ctx.db, ctx.secrets, employee_id)?;
    let employee = get_employee(ctx.db, employee_id)?
        .ok_or_else(|| anyhow!("employee {employee_id} does not exist"))?;

    let owned_session = session_id.is_none();
    let session_id = match session_id {
        Some(id) => id.to_string(),
        None => {
            let title = format!("{} - {}", employee.name, trigger);
            // An unattended run has nobody to answer a permission card, so it
            // opens in auto: a card would leave the run hanging forever and the
            // schedule would keep opening new ones. Plan/Goal's hard deny list
            // is untouched -- runs use agent mode.
            crate::sessions::create_session_with_options(
                ctx.db,
                crate::sessions::SessionCreateOptions {
                    title: Some(title),
                    mode: Some("agent".into()),
                    provider_id: employee.provider_id.clone().or(binding.provider_id.clone()),
                    model_id: employee.model_id.clone().or(binding.model_id.clone()),
                    project_path: ctx.workspace.map(str::to_string),
                    thinking_level: binding.thinking_level.clone(),
                    permission_mode: Some("auto".into()),
                },
            )?
            .id
        }
    };

    match begin_run_for(
        ctx.db,
        employee_id,
        Some(trigger),
        Some(&session_id),
        directive_id,
    ) {
        Ok(run) => {
            let steps = steps_for_run(ctx.db, &run)?;
            Ok(StartedRun { run, steps })
        }
        Err(error) => {
            if owned_session {
                let _ = crate::sessions::delete_session(ctx.db, &session_id);
            }
            Err(error)
        }
    }
}

/// Read back the compiled steps a run was started with.
pub fn steps_for_run(db: &Database, run: &EmployeeRun) -> Result<Vec<WorkflowStep>> {
    let raw: Option<String> = db
        .conn()
        .prepare_cached("SELECT steps_json FROM employee_runs WHERE id = ?1")?
        .query_row(params![run.id], |row| row.get(0))
        .optional()?;
    let raw = raw.unwrap_or_else(|| "[]".to_string());
    serde_json::from_str(&raw).context("parse stored run steps")
}

/// Close a run and keep the schedule honest about what happened.
///
/// A failure backs the employee off exponentially. Unattended means nobody
/// notices a broken key at 3am, so the schedule has to stop hammering.
pub fn finish_run(
    db: &Database,
    run_id: &str,
    status: &str,
    error_code: Option<&str>,
) -> Result<bool> {
    let run: Option<(String, Option<String>)> = db
        .conn()
        .prepare_cached("SELECT employee_id, directive_id FROM employee_runs WHERE id = ?1")?
        .query_row(params![run_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .optional()?;
    let Some((employee_id, directive_id)) = run else {
        return Ok(false);
    };
    if !super::finish_run(db, run_id, status, error_code)? {
        return Ok(false);
    }

    let now = now_ms();
    db.conn().execute(
        "UPDATE employees SET last_run_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![employee_id, now],
    )?;

    if let Some(directive_id) = directive_id {
        close_directive(db, run_id, &directive_id, status, error_code)?;
        // The slot the employee was holding is untouched: an order is extra
        // work, not a substitute for the sweep it was already going to make.
        return Ok(true);
    }

    let Some(schedule) = employee_schedule(db, &employee_id)? else {
        return Ok(true);
    };
    let anchor = employee_anchor(db, &employee_id)?;
    let next = if status == "failed" {
        Some(now + back_off_ms(db, &employee_id)?)
    } else {
        schedule.next_after(now, anchor, &employee_id)
    };
    db.conn().execute(
        "UPDATE employees SET next_run_at = ?2, updated_at = ?3 WHERE id = ?1",
        params![employee_id, next, now],
    )?;
    Ok(true)
}

/// What became of a manager's order, said in the worker's own words.
///
/// The last line of the run's last step is the closest thing to a report the
/// order will ever get, so it is what the manager's console shows against the
/// errand -- "the worker did it" without saying what it found is not an answer.
fn close_directive(
    db: &Database,
    run_id: &str,
    directive_id: &str,
    status: &str,
    error_code: Option<&str>,
) -> Result<()> {
    let done = status == "succeeded";
    let note = if done {
        let results: String = db
            .conn()
            .prepare_cached("SELECT results_json FROM employee_runs WHERE id = ?1")?
            .query_row(params![run_id], |row| row.get(0))
            .optional()?
            .unwrap_or_else(|| "{}".to_string());
        let answer = serde_json::from_str::<Value>(&results)
            .ok()
            .and_then(|value| {
                value.as_object().and_then(|entries| {
                    entries
                        .iter()
                        .filter_map(|(step, text)| {
                            let index = step.parse::<i64>().ok()?;
                            Some((index, text.as_str()?.to_string()))
                        })
                        .max_by_key(|(index, _)| *index)
                        .map(|(_, text)| text)
                })
            })
            .unwrap_or_default();
        if answer.is_empty() {
            "已完成（员工没有留下结论）".to_string()
        } else {
            format!("已完成：{answer}")
        }
    } else {
        format!("没做完：{}", error_code.unwrap_or(status))
    };
    super::update_directive(
        db,
        &json!({
            "id": directive_id,
            "status": if done { "done" } else { "failed" },
            "resultNote": note,
        }),
    )?;
    Ok(())
}

fn employee_anchor(db: &Database, employee_id: &str) -> Result<i64> {
    let anchor: Option<i64> = db
        .conn()
        .prepare_cached("SELECT created_at FROM employees WHERE id = ?1")?
        .query_row(params![employee_id], |row| row.get(0))
        .optional()?;
    Ok(anchor.unwrap_or_else(now_ms))
}

fn consecutive_failures(db: &Database, employee_id: &str) -> Result<i64> {
    let mut stmt = db.conn().prepare_cached(
        "SELECT status FROM employee_runs WHERE employee_id = ?1 ORDER BY started_at DESC LIMIT 20",
    )?;
    let rows = stmt.query_map(params![employee_id], |row| row.get::<_, String>(0))?;
    let mut failures = 0;
    for row in rows {
        match row?.as_str() {
            "failed" => failures += 1,
            _ => break,
        }
    }
    Ok(failures)
}

fn back_off_ms(db: &Database, employee_id: &str) -> Result<i64> {
    let failures = consecutive_failures(db, employee_id)?;
    let shift = failures.min(16) as u32;
    Ok((BACKOFF_BASE_MS * 2i64.pow(shift)).min(BACKOFF_MAX_MS))
}

pub fn employee_schedule(db: &Database, employee_id: &str) -> Result<Option<Schedule>> {
    let raw: Option<String> = db
        .conn()
        .prepare_cached("SELECT schedule_json FROM employees WHERE id = ?1")?
        .query_row(params![employee_id], |row| row.get(0))
        .optional()?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
    Schedule::parse(&value)
}

/// Recompute next_run_at for a running employee.
pub fn reschedule(db: &Database, employee_id: &str, now: i64) -> Result<Option<i64>> {
    let Some(schedule) = employee_schedule(db, employee_id)? else {
        db.conn().execute(
            "UPDATE employees SET next_run_at = NULL WHERE id = ?1",
            params![employee_id],
        )?;
        return Ok(None);
    };
    let anchor = employee_anchor(db, employee_id)?;
    let next = schedule.next_after(now, anchor, employee_id);
    db.conn().execute(
        "UPDATE employees SET next_run_at = ?2, updated_at = ?3 WHERE id = ?1",
        params![employee_id, next, now],
    )?;
    Ok(next)
}

fn runs_in_window(db: &Database, employee_id: &str, since: i64) -> Result<i64> {
    Ok(db.conn().query_row(
        "SELECT COUNT(*) FROM employee_runs WHERE employee_id = ?1 AND started_at >= ?2",
        params![employee_id, since],
        |row| row.get(0),
    )?)
}

fn max_runs_per_day(db: &Database, employee_id: &str) -> Result<i64> {
    let raw: Option<String> = db
        .conn()
        .prepare_cached("SELECT quota_json FROM employees WHERE id = ?1")?
        .query_row(params![employee_id], |row| row.get(0))
        .optional()?;
    let value: Value = raw
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| json!({}));
    let limit = value
        .get("maxRunsPerDay")
        .and_then(Value::as_i64)
        .unwrap_or(DEFAULT_MAX_RUNS_PER_DAY);
    if limit < 1 {
        bail!("INVALID_ARGUMENT: maxRunsPerDay must be at least 1");
    }
    Ok(limit)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
/// One employee the scheduler owes a run to, and why.
struct DueEmployee {
    employee_id: String,
    /// The queued order this claim exists to carry out, when one exists. A
    /// manager's errand outranks the slot it landed on: the slot can wait, an
    /// order that missed its window cannot.
    directive_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimedRun {
    pub run_id: String,
    pub employee_id: String,
    pub employee_name: String,
    /// The board this employee owns, so a board node that did not name one
    /// reads the manager's own instead of guessing.
    pub board: String,
    pub session_id: String,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub workflow_id: String,
    pub workflow_version: i64,
    /// What opened the run. The collector reads it to decide whether it may
    /// put a site's sign-in page on screen: a manual run has the owner sitting
    /// in front of it, a scheduled one has nobody to scan a code.
    pub trigger: String,
    pub steps: Vec<WorkflowStep>,
    pub started_at: String,
    /// The order this run is carrying out, as its prompt needs to read it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedEmployee {
    pub employee_id: String,
    pub error: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimBatch {
    pub claims: Vec<ClaimedRun>,
    pub skipped: Vec<SkippedEmployee>,
}

/// Claim every employee that is due, and move each one to its next slot.
///
/// Claiming advances the schedule before the run finishes, so a crash mid-run
/// cannot restart the same slot over and over.
pub fn claim_due(ctx: &StaffContext<'_>, limit: i64) -> Result<ClaimBatch> {
    let now = now_ms();
    let limit = limit.clamp(1, 32);
    // An order whose window has closed is closed out before the scan runs, so
    // it is never picked up late and the owner is told it never happened.
    expire_directives(ctx.db, now)?;
    let due = due_employees(ctx.db, now, limit)?;
    let mut claims = Vec::new();
    let mut skipped = Vec::new();
    for due in due {
        let employee_id = due.employee_id;
        match claim_one(ctx, &employee_id, now, due.directive_id.as_deref()) {
            Ok(Some(claim)) => claims.push(claim),
            Ok(None) => {}
            Err(error) => {
                let message = error.to_string();
                let backoff = back_off_ms(ctx.db, &employee_id).unwrap_or(BACKOFF_BASE_MS);
                let _ = ctx.db.conn().execute(
                    "UPDATE employees SET next_run_at = ?2, updated_at = ?3 WHERE id = ?1",
                    params![employee_id, now + backoff, now],
                );
                // An order that cannot start is closed rather than left queued:
                // queued means the next tick tries again, and a broken order
                // would then be retried every thirty seconds forever.
                if let Some(directive_id) = &due.directive_id {
                    let _ = super::update_directive(
                        ctx.db,
                        &json!({
                            "id": directive_id,
                            "status": "failed",
                            "resultNote": format!("没派出去：{message}"),
                        }),
                    );
                }
                skipped.push(SkippedEmployee {
                    employee_id,
                    error: message,
                });
            }
        }
    }
    Ok(ClaimBatch { claims, skipped })
}

/// Orders past their deadline are closed where an owner will see them.
///
/// A stale errand is worse than a refused one: running it a day late puts a
/// comment on a thread that has moved on, and the owner never learns that the
/// manager's plan went stale.
fn expire_directives(db: &Database, now: i64) -> Result<usize> {
    let expired = db.conn().execute(
        "UPDATE staff_directives
            SET status = 'failed', result_note = '没派出去：超过期限，已经过期', ended_at = ?1,
                updated_at = ?1
          WHERE status = 'queued' AND deadline_at IS NOT NULL AND deadline_at <= ?1",
        params![now],
    )?;
    Ok(expired)
}

fn claim_one(
    ctx: &StaffContext<'_>,
    employee_id: &str,
    now: i64,
    directive_id: Option<&str>,
) -> Result<Option<ClaimedRun>> {
    // An order runs whatever the schedule says: the employee is on payroll and
    // someone above it asked for this, which is exactly the temporary task the
    // shape of the roster is for.
    if let Some(directive_id) = directive_id {
        let started = start_run_for(ctx, employee_id, "directive", None, Some(directive_id))?;
        super::claim_directive(ctx.db, directive_id, &started.run.id)?;
        ctx.db.conn().execute(
            "UPDATE employees SET last_run_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![employee_id, now],
        )?;
        let brief = super::directive_brief(ctx.db, directive_id)?;
        return Ok(Some(claimed_run(ctx, employee_id, started, brief)?));
    }
    let Some(schedule) = employee_schedule(ctx.db, employee_id)? else {
        // Not scheduled any more; drop the pending stamp instead of spinning.
        ctx.db.conn().execute(
            "UPDATE employees SET next_run_at = NULL WHERE id = ?1",
            params![employee_id],
        )?;
        return Ok(None);
    };

    let limit = max_runs_per_day(ctx.db, employee_id)?;
    if runs_in_window(ctx.db, employee_id, now - DAY_MS)? >= limit {
        let next = now + schedule.nominal_period_ms().max(BACKOFF_BASE_MS);
        ctx.db.conn().execute(
            "UPDATE employees SET next_run_at = ?2, updated_at = ?3 WHERE id = ?1",
            params![employee_id, next, now],
        )?;
        return Ok(None);
    }

    let started = start_run(ctx, employee_id, "schedule", None)?;
    let anchor = employee_anchor(ctx.db, employee_id)?;
    let next = schedule.next_after(now, anchor, employee_id);
    ctx.db.conn().execute(
        "UPDATE employees SET next_run_at = ?2, last_run_at = ?3, updated_at = ?3 WHERE id = ?1",
        params![employee_id, next, now],
    )?;

    Ok(Some(claimed_run(ctx, employee_id, started, None)?))
}

/// The claim the driver reads, built the same way for every trigger.
fn claimed_run(
    ctx: &StaffContext<'_>,
    employee_id: &str,
    started: StartedRun,
    directive: Option<Value>,
) -> Result<ClaimedRun> {
    let employee = get_employee(ctx.db, employee_id)?
        .ok_or_else(|| anyhow!("employee {employee_id} does not exist"))?;
    Ok(ClaimedRun {
        run_id: started.run.id,
        employee_id: employee_id.to_string(),
        employee_name: employee.name.clone(),
        board: employee.board,
        session_id: started.run.session_id.unwrap_or_default(),
        provider_id: started.run.provider_id,
        model_id: started.run.model_id,
        workflow_id: started.run.workflow_id.unwrap_or_default(),
        workflow_version: started.run.workflow_version.unwrap_or_default(),
        trigger: started.run.trigger,
        steps: started.steps,
        started_at: started.run.started_at,
        directive,
    })
}

/// Who the scheduler owes a run to: the employees holding a live order, and
/// the ones whose slot has come round.
///
/// The order comes first in the ordering because it is the one with a clock on
/// it, and because a worker that is busy with an errand should finish it before
/// its next scheduled sweep starts.
fn due_employees(db: &Database, now: i64, limit: i64) -> Result<Vec<DueEmployee>> {
    let mut stmt = db.conn().prepare_cached(
        "SELECT employee_id, directive_id FROM (
            SELECT e.id AS employee_id,
                   (SELECT d.id FROM staff_directives d
                     WHERE d.employee_id = e.id AND d.status = 'queued'
                       AND (d.deadline_at IS NULL OR d.deadline_at > ?1)
                     ORDER BY d.priority DESC, d.created_at ASC
                     LIMIT 1) AS directive_id,
                   e.next_run_at AS next_run_at,
                   e.created_at AS created_at
              FROM employees e
             WHERE e.status = 'running'
         )
         WHERE directive_id IS NOT NULL
            OR (next_run_at IS NOT NULL AND next_run_at <= ?1)
         ORDER BY (directive_id IS NULL) ASC, next_run_at ASC, created_at ASC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![now, limit], |row| {
        Ok(DueEmployee {
            employee_id: row.get(0)?,
            directive_id: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Boot maintenance: a run that was live when the process died can never finish,
/// so it is closed out; and a running employee gets a real next slot.
pub fn recover(db: &Database) -> Result<i64> {
    let now = now_ms();
    let aborted = db.conn().execute(
        "UPDATE employee_runs SET status = 'aborted', error_code = 'HOST_RESTART', ended_at = ?1
         WHERE status = 'running'",
        params![now],
    )?;
    let mut stmt = db.conn().prepare_cached(
        "SELECT id FROM employees WHERE status = 'running' AND next_run_at IS NULL",
    )?;
    let employees = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for employee_id in employees {
        reschedule(db, &employee_id, now)?;
    }
    Ok(aborted as i64)
}
