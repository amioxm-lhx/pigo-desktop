use super::*;
use serde_json::json;

/// A new employee starts paused. Scheduling an unattended worker is an explicit
/// act, never a side effect of creating one.
pub const EMPLOYEE_STATUSES: [&str; 3] = ["paused", "running", "stopped"];

/// What an employee is for. A worker does the errand it was given; a manager
/// reads a board, decides, and hands errands to workers. The distinction is a
/// column rather than a name the code recognises, so an owner who renames
/// 线索主管 keeps everything 线索主管 was allowed to do -- and a worker can never
/// be handed the board node, which is what dispatching is.
pub const EMPLOYEE_LAYERS: [&str; 2] = ["worker", "manager"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Employee {
    pub id: String,
    pub name: String,
    pub role: String,
    /// `worker` or `manager`.
    pub layer: String,
    /// The board a manager owns. Empty for a worker.
    pub board: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_version: Option<i64>,
    pub status: String,
    /// How often this employee runs. An empty object means manual only.
    pub schedule: Value,
    pub quota: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_run_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmployeeRun {
    pub id: String,
    pub employee_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    pub trigger: String,
    /// The directive this run is carrying out, when a manager dispatched it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive_id: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_version: Option<i64>,
    pub step_count: i64,
    pub current_step: i64,
    /// The pinned steps' titles in execution order, so a run log can name what
    /// each result belongs to without re-reading the workflow version.
    pub step_titles: Vec<String>,
    /// What each finished step reported, keyed by its one-based step number.
    /// An object rather than an array so a step that died leaves no gap.
    pub results: Value,
    /// Each finished step's full reply, keyed like `results`: the collected
    /// material the one-line verdict summarizes.
    pub reports: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
}

/// What a run needs in order to start: which model, and which key pays.
///
/// Deliberately neither Serialize nor Debug. The API key is inside this struct,
/// and the one property that keeps credentials out of replies, logs, and panic
/// payloads is that the value has no path to either trait.
pub struct RunBinding {
    pub employee_id: String,
    pub credential_profile_id: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub thinking_level: Option<String>,
    /// A profile may point at a different endpoint than the provider row; the
    /// driver needs it alongside the key so both travel together.
    pub base_url: Option<String>,
    pub api_key: Option<String>,
}

const EMPLOYEE_SELECT: &str =
    "SELECT id, name, role, credential_profile_id, provider_id, model_id, thinking_level,
            workflow_id, workflow_version, status, schedule_json, quota_json, next_run_at,
            last_run_at, created_at, updated_at, layer, board
     FROM employees";

const RUN_SELECT: &str =
    "SELECT id, employee_id, session_id, credential_profile_id, provider_id, model_id,
            trigger, status, workflow_id, workflow_version, current_step, error_code,
            started_at, ended_at, steps_json, results_json, reports_json, directive_id
     FROM employee_runs";

fn employee_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Employee> {
    let schedule_json: String = row.get(10)?;
    let quota_json: String = row.get(11)?;
    Ok(Employee {
        id: row.get(0)?,
        name: row.get(1)?,
        role: row.get(2)?,
        credential_profile_id: row.get(3)?,
        provider_id: row.get(4)?,
        model_id: row.get(5)?,
        thinking_level: row.get(6)?,
        workflow_id: row.get(7)?,
        workflow_version: row.get(8)?,
        status: row.get(9)?,
        schedule: parse_object(&schedule_json),
        quota: parse_object(&quota_json),
        next_run_at: row.get::<_, Option<i64>>(12)?.map(ms_to_ts),
        last_run_at: row.get::<_, Option<i64>>(13)?.map(ms_to_ts),
        layer: row.get(16)?,
        board: row.get(17)?,
        created_at: ms_to_ts(row.get(14)?),
        updated_at: ms_to_ts(row.get(15)?),
    })
}

/// A malformed stored object reads as empty rather than failing the whole list:
/// one bad row must not make the roster unreadable.
fn parse_object(raw: &str) -> Value {
    serde_json::from_str(raw)
        .ok()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

/// The directive a run carries, read straight off the row for the scheduler.
pub fn run_directive_id(db: &Database, run_id: &str) -> Result<Option<String>> {
    Ok(db
        .conn()
        .prepare_cached("SELECT directive_id FROM employee_runs WHERE id = ?1")?
        .query_row(params![run_id], |row| row.get(0))
        .optional()?
        .flatten())
}

fn run_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EmployeeRun> {
    let steps_json: String = row.get(14)?;
    let parsed_steps = serde_json::from_str::<Value>(&steps_json).ok();
    let step_count = parsed_steps
        .as_ref()
        .and_then(|value| value.as_array().map(|steps| steps.len() as i64))
        .unwrap_or(0);
    let step_titles = parsed_steps
        .as_ref()
        .and_then(|value| value.as_array())
        .map(|steps| {
            steps
                .iter()
                .filter_map(|step| {
                    step.get("title")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    let results_json: String = row.get(15)?;
    let reports_json: String = row.get(16)?;
    Ok(EmployeeRun {
        id: row.get(0)?,
        employee_id: row.get(1)?,
        session_id: row.get(2)?,
        credential_profile_id: row.get(3)?,
        provider_id: row.get(4)?,
        model_id: row.get(5)?,
        trigger: row.get(6)?,
        directive_id: row.get(17)?,
        status: row.get(7)?,
        workflow_id: row.get(8)?,
        workflow_version: row.get(9)?,
        step_count,
        current_step: row.get(10)?,
        step_titles,
        results: parse_object(&results_json),
        reports: parse_object(&reports_json),
        error_code: row.get(11)?,
        started_at: ms_to_ts(row.get(12)?),
        ended_at: row.get::<_, Option<i64>>(13)?.map(ms_to_ts),
    })
}

fn normalize_layer(value: Option<&Value>) -> Result<String> {
    match value.and_then(Value::as_str) {
        None => Ok("worker".to_string()),
        Some(layer) if EMPLOYEE_LAYERS.contains(&layer) => Ok(layer.to_string()),
        Some(other) => bail!("INVALID_ARGUMENT: unknown employee layer {other}"),
    }
}

/// A manager owns exactly one of the boards the console draws, and nobody else
/// owns one. A manager pointed at a board that does not exist would dispatch
/// against a page nobody can open, which is why the check is here and not in
/// the console.
fn validate_board_layer(layer: &str, board: &str) -> Result<()> {
    if layer == "manager" {
        if !SUGGESTION_BOARDS.contains(&board) {
            bail!(
                "INVALID_ARGUMENT: a manager must own a board ({})",
                SUGGESTION_BOARDS.join(", ")
            );
        }
        return Ok(());
    }
    if !board.is_empty() {
        bail!("INVALID_ARGUMENT: only a manager owns a board");
    }
    Ok(())
}

/// Reading a board and dispatching from it is a manager's work. A graph that
/// does it is refused at the moment it is bound to a worker, so the refusal
/// names the employee rather than surfacing hours later as a run that quietly
/// handed work out.
fn validate_board_nodes(
    db: &Database,
    layer: &str,
    workflow_id: Option<&str>,
    workflow_version: Option<i64>,
) -> Result<()> {
    if layer == "manager" {
        return Ok(());
    }
    let (Some(workflow_id), Some(version)) = (workflow_id, workflow_version) else {
        return Ok(());
    };
    let Some((graph, _)) = super::workflow::get_graph(db, workflow_id, version)? else {
        return Ok(());
    };
    if graph.has_kind("board") {
        bail!(
            "INVALID_ARGUMENT: workflow {workflow_id} reads a board, which only a manager may run"
        );
    }
    Ok(())
}

fn normalize_status(value: Option<&Value>) -> Result<String> {
    match value.and_then(Value::as_str) {
        None => Ok("paused".to_string()),
        Some(status) if EMPLOYEE_STATUSES.contains(&status) => Ok(status.to_string()),
        Some(other) => bail!("INVALID_ARGUMENT: unknown employee status {other}"),
    }
}

/// Nullable text that distinguishes "absent" from "explicitly cleared".
fn nullable_field(params: &Value, key: &str) -> Option<Option<String>> {
    match params.get(key) {
        None => None,
        Some(Value::Null) => Some(None),
        Some(_) => Some(optional_str(params, key)),
    }
}

fn optional_i64(params: &Value, key: &str) -> Result<Option<i64>> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_i64()
            .map(Some)
            .ok_or_else(|| anyhow!("INVALID_ARGUMENT: {key} must be an integer")),
    }
}

/// A JSON object field, or None when the caller did not send one.
fn object_field(params: &Value, key: &str) -> Result<Option<Value>> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) if value.is_object() => Ok(Some(value.clone())),
        Some(_) => bail!("INVALID_ARGUMENT: {key} must be an object"),
    }
}

/// A run is only reproducible if it knows which workflow revision it follows,
/// so a pin is required exactly when an employee is meant to run unattended.
fn validate_pin(
    db: &Database,
    workflow_id: Option<&str>,
    workflow_version: Option<i64>,
) -> Result<()> {
    match (workflow_id, workflow_version) {
        (None, None) => Ok(()),
        (Some(id), Some(version)) => {
            if super::workflow::get_workflow(db, id)?.is_none() {
                bail!("INVALID_ARGUMENT: workflow {id} does not exist");
            }
            if super::workflow::get_graph(db, id, version)?.is_none() {
                bail!("INVALID_ARGUMENT: workflow {id} has no version {version}");
            }
            Ok(())
        }
        _ => bail!("INVALID_ARGUMENT: workflowId and workflowVersion are set together"),
    }
}

fn require_pin_for_schedule(schedule: Option<&Value>, pinned: bool) -> Result<()> {
    let scheduled = schedule
        .map(|value| {
            !value
                .as_object()
                .map(|object| object.is_empty())
                .unwrap_or(true)
        })
        .unwrap_or(false);
    if scheduled && !pinned {
        bail!("INVALID_ARGUMENT: a scheduled employee must pin a workflow version");
    }
    Ok(())
}

pub fn create_employee(db: &Database, params_json: &Value) -> Result<Employee> {
    let name = required_str(params_json, "name")?;
    let status = normalize_status(params_json.get("status"))?;
    let layer = normalize_layer(params_json.get("layer"))?;
    let board = optional_str(params_json, "board").unwrap_or_default();
    validate_board_layer(&layer, &board)?;
    let workflow_id = optional_str(params_json, "workflowId");
    let workflow_version = optional_i64(params_json, "workflowVersion")?;
    validate_pin(db, workflow_id.as_deref(), workflow_version)?;
    validate_board_nodes(db, &layer, workflow_id.as_deref(), workflow_version)?;
    let schedule = object_field(params_json, "schedule")?;
    require_pin_for_schedule(schedule.as_ref(), workflow_id.is_some())?;
    let schedule = match schedule {
        Some(value) => {
            super::schedule::Schedule::parse(&value)?;
            value
        }
        None => json!({}),
    };
    let quota = object_field(params_json, "quota")?.unwrap_or_else(|| json!({}));
    let id = Uuid::new_v4().to_string();
    let now = now_ms();

    db.conn().execute(
        "INSERT INTO employees
           (id, name, role, credential_profile_id, provider_id, model_id, thinking_level,
            workflow_id, workflow_version, schedule_json, quota_json, layer, board, status,
            created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)",
        params![
            id,
            name,
            optional_str(params_json, "role").unwrap_or_default(),
            optional_str(params_json, "credentialProfileId"),
            optional_str(params_json, "providerId"),
            optional_str(params_json, "modelId"),
            optional_str(params_json, "thinkingLevel"),
            workflow_id,
            workflow_version,
            schedule.to_string(),
            quota.to_string(),
            layer,
            board,
            status,
            now,
        ],
    )?;

    let employee =
        get_employee(db, &id)?.ok_or_else(|| anyhow!("employee {id} disappeared after insert"))?;
    reschedule_if_running(db, &employee)?;
    get_employee(db, &id)?.ok_or_else(|| anyhow!("employee {id} disappeared after insert"))
}

pub fn list_employees(db: &Database) -> Result<Vec<Employee>> {
    let mut stmt = db
        .conn()
        .prepare_cached(&format!("{EMPLOYEE_SELECT} ORDER BY created_at ASC"))?;
    let rows = stmt.query_map([], employee_from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_employee(db: &Database, id: &str) -> Result<Option<Employee>> {
    Ok(db
        .conn()
        .prepare_cached(&format!("{EMPLOYEE_SELECT} WHERE id = ?1"))?
        .query_row(params![id], employee_from_row)
        .optional()?)
}

/// A running employee owns a pending slot; anything else owns none.
fn reschedule_if_running(db: &Database, employee: &Employee) -> Result<()> {
    if employee.status == "running" {
        super::schedule::reschedule(db, &employee.id, now_ms())?;
    } else {
        db.conn().execute(
            "UPDATE employees SET next_run_at = NULL WHERE id = ?1",
            params![employee.id],
        )?;
    }
    Ok(())
}

pub fn update_employee(db: &Database, id: &str, params_json: &Value) -> Result<Option<Employee>> {
    let Some(current) = get_employee(db, id)? else {
        return Ok(None);
    };
    let status = match params_json.get("status") {
        Some(value) => normalize_status(Some(value))?,
        None => current.status.clone(),
    };
    let layer = match params_json.get("layer") {
        Some(value) => normalize_layer(Some(value))?,
        None => current.layer.clone(),
    };
    let board = optional_str(params_json, "board").unwrap_or_else(|| current.board.clone());
    validate_board_layer(&layer, &board)?;
    let workflow_id =
        nullable_field(params_json, "workflowId").unwrap_or(current.workflow_id.clone());
    let workflow_version = match params_json.get("workflowVersion") {
        None => current.workflow_version,
        Some(Value::Null) => None,
        Some(value) => value
            .as_i64()
            .ok_or_else(|| anyhow!("INVALID_ARGUMENT: workflowVersion must be an integer"))?
            .into(),
    };
    validate_pin(db, workflow_id.as_deref(), workflow_version)?;
    validate_board_nodes(db, &layer, workflow_id.as_deref(), workflow_version)?;
    let schedule = match object_field(params_json, "schedule")? {
        Some(value) => {
            super::schedule::Schedule::parse(&value)?;
            value
        }
        None => current.schedule.clone(),
    };
    require_pin_for_schedule(Some(&schedule), workflow_id.is_some())?;
    let quota = object_field(params_json, "quota")?.unwrap_or(current.quota);

    db.conn().execute(
        "UPDATE employees SET
           name = ?2, role = ?3, credential_profile_id = ?4, provider_id = ?5, model_id = ?6,
           thinking_level = ?7, workflow_id = ?8, workflow_version = ?9, schedule_json = ?10,
           quota_json = ?11, layer = ?12, board = ?13, status = ?14, updated_at = ?15
         WHERE id = ?1",
        params![
            id,
            optional_str(params_json, "name").unwrap_or(current.name),
            optional_str(params_json, "role").unwrap_or(current.role),
            nullable_field(params_json, "credentialProfileId")
                .unwrap_or(current.credential_profile_id),
            nullable_field(params_json, "providerId").unwrap_or(current.provider_id),
            nullable_field(params_json, "modelId").unwrap_or(current.model_id),
            nullable_field(params_json, "thinkingLevel").unwrap_or(current.thinking_level),
            workflow_id,
            workflow_version,
            schedule.to_string(),
            quota.to_string(),
            layer,
            board,
            status,
            now_ms(),
        ],
    )?;

    let employee = get_employee(db, id)?;
    if let Some(employee) = &employee {
        reschedule_if_running(db, employee)?;
    }
    get_employee(db, id)
}

/// Ask the scheduler for a fresh slot without changing anything else. Used when
/// the user hits "run soon" on a paused-but-scheduled employee.
pub fn refresh_schedule(db: &Database, id: &str) -> Result<Option<String>> {
    let Some(employee) = get_employee(db, id)? else {
        return Ok(None);
    };
    let next = super::schedule::reschedule(db, &employee.id, now_ms())?;
    Ok(next.map(ms_to_ts))
}

/// Runs cascade with their employee, but memory deliberately does not: what an
/// employee learned stays readable after the employee is gone.
pub fn delete_employee(db: &Database, id: &str) -> Result<bool> {
    let deleted = db
        .conn()
        .execute("DELETE FROM employees WHERE id = ?1", params![id])?;
    Ok(deleted > 0)
}

/// Resolve model plus credentials for a run, failing closed.
///
/// A profile is authoritative when set. Without one the employee falls back to
/// the provider's own secret, so an existing single-account setup keeps working.
pub fn resolve_run_binding(
    db: &Database,
    secrets: &SecretStore,
    employee_id: &str,
) -> Result<RunBinding> {
    let employee = get_employee(db, employee_id)?
        .ok_or_else(|| anyhow!("NOT_FOUND: employee {employee_id} does not exist"))?;

    if employee.status == "stopped" {
        bail!(
            "CONFLICT: employee {} is stopped; restart it before running",
            employee.name
        );
    }

    let resolved = resolve_secret_for_binding(
        db,
        secrets,
        employee.credential_profile_id.as_deref(),
        employee.provider_id.as_deref(),
    )?;
    if resolved.0.is_none() {
        bail!(
            "PROVIDER_SECRET_MISSING: employee {} has no credential profile or provider key",
            employee.name
        );
    }

    Ok(RunBinding {
        employee_id: employee.id,
        credential_profile_id: employee.credential_profile_id,
        provider_id: employee.provider_id,
        model_id: employee.model_id,
        thinking_level: employee.thinking_level,
        base_url: resolved.1,
        api_key: resolved.0,
    })
}

/// The credential half of a binding: the key, and the endpoint that goes with it.
///
/// Returns `(apiKey, baseUrl)`. A profile is authoritative when set, and it is
/// the only source of a base-url override.
fn resolve_secret_for_binding(
    db: &Database,
    secrets: &SecretStore,
    credential_profile_id: Option<&str>,
    provider_id: Option<&str>,
) -> Result<(Option<String>, Option<String>)> {
    match credential_profile_id {
        Some(profile_id) => {
            let base_url: Option<String> = db
                .conn()
                .prepare_cached("SELECT base_url FROM credential_profiles WHERE id = ?1")?
                .query_row(params![profile_id], |row| row.get(0))
                .optional()?
                .flatten();
            let key = crate::staff::resolve_profile_secret(db, secrets, profile_id)?;
            Ok((Some(key), base_url))
        }
        None => match provider_id {
            Some(provider_id) => Ok((
                crate::providers::get_secret_for_provider(db, secrets, provider_id)?,
                None,
            )),
            None => Ok((None, None)),
        },
    }
}

/// Resolve the binding a run was started with, from the run row itself.
///
/// The employee row is not the authority here: a run must be paid for by the
/// account it was started with even if the employee has since been repointed at
/// another profile. Thinking level is the one exception -- it is a run parameter,
/// not a credential, and it is read live from the employee.
pub fn resolve_binding_for_run(
    db: &Database,
    secrets: &SecretStore,
    run_id: &str,
) -> Result<RunBinding> {
    let run =
        get_run(db, run_id)?.ok_or_else(|| anyhow!("NOT_FOUND: run {run_id} does not exist"))?;
    let resolved = resolve_secret_for_binding(
        db,
        secrets,
        run.credential_profile_id.as_deref(),
        run.provider_id.as_deref(),
    )?;
    if resolved.0.is_none() {
        bail!("PROVIDER_SECRET_MISSING: run {run_id} has no credential profile or provider key");
    }
    let thinking_level = get_employee(db, &run.employee_id)?.and_then(|e| e.thinking_level);

    Ok(RunBinding {
        employee_id: run.employee_id,
        credential_profile_id: run.credential_profile_id,
        provider_id: run.provider_id,
        model_id: run.model_id,
        thinking_level,
        base_url: resolved.1,
        api_key: resolved.0,
    })
}

/// Open a run. The binding and the compiled workflow are copied onto the row, so
/// a later edit to the employee cannot rewrite the history of what ran.
pub fn begin_run(
    db: &Database,
    employee_id: &str,
    trigger: Option<&str>,
    session_id: Option<&str>,
) -> Result<EmployeeRun> {
    begin_run_for(db, employee_id, trigger, session_id, None)
}

/// Open a run that is carrying out a manager's order. The directive travels
/// onto the row so the order and its execution can never be told apart later.
pub fn begin_run_for(
    db: &Database,
    employee_id: &str,
    trigger: Option<&str>,
    session_id: Option<&str>,
    directive_id: Option<&str>,
) -> Result<EmployeeRun> {
    let employee = get_employee(db, employee_id)?
        .ok_or_else(|| anyhow!("NOT_FOUND: employee {employee_id} does not exist"))?;
    let trigger = match trigger {
        Some(value @ ("schedule" | "manual" | "event" | "directive")) => value,
        Some(other) => bail!("INVALID_ARGUMENT: unknown run trigger {other}"),
        None => "schedule",
    };
    let workflow_id = employee.workflow_id.clone().ok_or_else(|| {
        anyhow!(
            "CONFLICT: employee {} has no workflow bound; bind one before running",
            employee.name
        )
    })?;
    let version = employee.workflow_version.ok_or_else(|| {
        anyhow!(
            "CONFLICT: employee {} has no pinned workflow version",
            employee.name
        )
    })?;
    let (graph, _) = super::workflow::get_graph(db, &workflow_id, version)?
        .ok_or_else(|| anyhow!("CONFLICT: workflow {workflow_id} has no version {version}"))?;
    // The gate is checked here rather than only when the employee was saved:
    // a workflow version adopted after the fact is exactly how a worker would
    // otherwise end up holding the board node.
    if employee.layer != "manager" && graph.has_kind("board") {
        bail!(
            "PERMISSION_DENIED: employee {} is not a manager, and this workflow reads a board",
            employee.name
        );
    }
    let steps = graph.compile()?;
    let steps_json = serde_json::to_string(&steps)?;

    let id = Uuid::new_v4().to_string();
    db.conn().execute(
        "INSERT INTO employee_runs
           (id, employee_id, session_id, credential_profile_id, provider_id, model_id,
            trigger, status, workflow_id, workflow_version, steps_json, current_step, started_at,
            directive_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'running', ?8, ?9, ?10, 0, ?11, ?12)",
        params![
            id,
            employee.id,
            session_id,
            employee.credential_profile_id,
            employee.provider_id,
            employee.model_id,
            trigger,
            workflow_id,
            version,
            steps_json,
            now_ms(),
            directive_id,
        ],
    )?;

    get_run(db, &id)?.ok_or_else(|| anyhow!("employee run {id} disappeared after insert"))
}

pub fn finish_run(
    db: &Database,
    run_id: &str,
    status: &str,
    error_code: Option<&str>,
) -> Result<bool> {
    if !matches!(status, "succeeded" | "failed" | "aborted") {
        bail!("INVALID_ARGUMENT: unknown run status {status}");
    }
    let updated = db.conn().execute(
        "UPDATE employee_runs SET status = ?2, error_code = ?3, ended_at = ?4
         WHERE id = ?1 AND status = 'running'",
        params![run_id, status, error_code, now_ms()],
    )?;
    Ok(updated > 0)
}

/// Record how far a run got, and what the finished step reported. A run that
/// died at step 3 should say so, and the run log is the only report an
/// unattended run ever gives, so the step's own line and its full reply are
/// stored with the progress mark.
pub fn set_run_step(
    db: &Database,
    run_id: &str,
    step: i64,
    result: Option<&str>,
    report: Option<&str>,
) -> Result<bool> {
    if step < 0 {
        bail!("INVALID_ARGUMENT: step must not be negative");
    }
    if result.is_none() && report.is_none() {
        let updated = db.conn().execute(
            "UPDATE employee_runs SET current_step = ?2 WHERE id = ?1 AND status = 'running'",
            params![run_id, step],
        )?;
        return Ok(updated > 0);
    }
    // Read-modify-write on one row the driver alone advances, so the merges
    // cannot interleave with another writer.
    let row: Option<(String, String)> = db
        .conn()
        .query_row(
            "SELECT results_json, reports_json FROM employee_runs WHERE id = ?1 AND status = 'running'",
            params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((results_raw, reports_raw)) = row else {
        return Ok(false);
    };
    let mut results = parse_object(&results_raw);
    let mut reports = parse_object(&reports_raw);
    if let Some(result) = result {
        if let Value::Object(entries) = &mut results {
            entries.insert(step.to_string(), Value::String(result.to_string()));
        }
    }
    if let Some(report) = report {
        if let Value::Object(entries) = &mut reports {
            entries.insert(step.to_string(), Value::String(report.to_string()));
        }
    }
    let updated = db.conn().execute(
        "UPDATE employee_runs SET current_step = ?2, results_json = ?3, reports_json = ?4
         WHERE id = ?1 AND status = 'running'",
        params![run_id, step, results.to_string(), reports.to_string()],
    )?;
    Ok(updated > 0)
}

pub fn get_run(db: &Database, run_id: &str) -> Result<Option<EmployeeRun>> {
    Ok(db
        .conn()
        .prepare_cached(&format!("{RUN_SELECT} WHERE id = ?1"))?
        .query_row(params![run_id], run_from_row)
        .optional()?)
}

pub fn list_runs(db: &Database, employee_id: Option<&str>, limit: i64) -> Result<Vec<EmployeeRun>> {
    let limit = limit.clamp(1, 500);
    let mut stmt = db.conn().prepare_cached(&format!(
        "{RUN_SELECT} WHERE (?1 IS NULL OR employee_id = ?1) ORDER BY started_at DESC, id ASC LIMIT ?2"
    ))?;
    let rows = stmt.query_map(params![employee_id, limit], run_from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
