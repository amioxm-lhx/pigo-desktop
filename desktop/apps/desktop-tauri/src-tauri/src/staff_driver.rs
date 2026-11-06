//! Resident staff driver (spec 03-runtime/20 §12).
//!
//! host-core stays headless: `staff.scheduler.claim` advances each due
//! employee's slot, opens the run's session and returns the pinned workflow
//! steps. This module owns the execution half — the timer that drives that
//! claim, the model turns that run each step inside the run's own session, the
//! step ledger, artifact filing, and settling the run — because a turn needs
//! the sidecar and the session host-core opened.
//!
//! Failure rules that matter to an unattended run:
//! - A step's outcome is the *last* line that starts with `STEP RESULT:`; the
//!   first match anywhere was stored as a run's signal once and published the
//!   tail of a sentence that merely quoted the marker (run 6e6e5672).
//! - A `humanGate` is a hard stop: the run ends with `HUMAN_GATE` rather than
//!   parking (spec §16 Bounds).
//! - An artifact that does not parse files nothing and is logged; a half-parsed
//!   card would be a claim about a real person that nobody made.
//! - A failed step settles the run as `failed` with the step's error code; the
//!   schedule already moved, so the next slot is the retry.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::rpc::RpcError;
use crate::state::AppState;

/// Poll cadence for due employees. Claiming is idempotent (host-core advances
/// `next_run_at` in the same transaction), so this is a floor, not a promise.
pub const POLL_MS: u64 = 30_000;
const CLAIM_LIMIT: i64 = 8;
const MAX_CONCURRENT_RUNS: usize = 4;
const STEP_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const NO_MATERIAL: &str = "- (nothing: no memory item and no collected page reached this step)";
const PROBE_EMPTY: &str = "\n[collected: 本轮没有读到任何页面：这台机器还没有该站点的采集器，不要改用其他方式自己去抓取。]\n";
const PROBE_COLLECTED: &str =
    "\n[collected: 本轮由驱动读取的页面在下面的材料里；没有页面就直说缺什么，不要自己再抓取。]\n";

pub struct Resident {
    running: AtomicBool,
    claimed_total: AtomicU64,
    last_tick_at: Mutex<Option<String>>,
    last_error: Mutex<Option<String>>,
    in_flight: Mutex<Vec<Value>>,
    /// turnId → watcher for a staff step's turn. The dispatcher fans agent
    /// events into these after persisting them, so the driver can await a turn
    /// without owning the event stream.
    waiters: Mutex<HashMap<String, UnboundedSender<Value>>>,
    slots: tokio::sync::Semaphore,
}

impl Resident {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            running: AtomicBool::new(false),
            claimed_total: AtomicU64::new(0),
            last_tick_at: Mutex::new(None),
            last_error: Mutex::new(None),
            in_flight: Mutex::new(Vec::new()),
            waiters: Mutex::new(HashMap::new()),
            slots: tokio::sync::Semaphore::new(MAX_CONCURRENT_RUNS),
        })
    }

    /// `StaffSchedulerStatus` for the console header.
    pub fn status(&self) -> Value {
        let mut status = json!({
            "resident": self.running.load(Ordering::Relaxed),
            "pollMs": POLL_MS,
            "claimedTotal": self.claimed_total.load(Ordering::Relaxed),
            "inFlight": self.in_flight.lock().unwrap().clone(),
        });
        if let Some(at) = self.last_tick_at.lock().unwrap().clone() {
            status["lastTickAt"] = json!(at);
        }
        if let Some(error) = self.last_error.lock().unwrap().clone() {
            status["lastError"] = json!(error);
        }
        status
    }

    fn begin_run(&self, entry: Value) {
        self.in_flight.lock().unwrap().push(entry);
    }

    fn set_step(&self, run_id: &str, step: i64) {
        let mut ledger = self.in_flight.lock().unwrap();
        if let Some(entry) = ledger
            .iter_mut()
            .find(|row| row.get("runId").and_then(Value::as_str) == Some(run_id))
        {
            entry["step"] = json!(step);
        }
    }

    fn end_run(&self, run_id: &str) {
        self.in_flight
            .lock()
            .unwrap()
            .retain(|row| row.get("runId").and_then(Value::as_str) != Some(run_id));
    }

    fn watch(&self, turn_id: &str) -> UnboundedReceiver<Value> {
        let (tx, rx) = unbounded_channel();
        self.waiters.lock().unwrap().insert(turn_id.to_string(), tx);
        rx
    }

    fn release(&self, turn_id: &str) {
        self.waiters.lock().unwrap().remove(turn_id);
    }

    /// Hand one agent event to the step turn that is waiting for it.
    pub fn notify_turn(&self, envelope: &Value) {
        let Some(turn_id) = envelope.get("turnId").and_then(Value::as_str) else {
            return;
        };
        let waiters = self.waiters.lock().unwrap();
        if let Some(tx) = waiters.get(turn_id) {
            let _ = tx.send(envelope.clone());
        }
    }
}

/// Start the resident loop. Called once boot has the host and sidecar
/// handshaked; a claim against a dead host would only fill the error slot.
pub fn start(app: AppHandle, state: Arc<AppState>) {
    state.staff.running.store(true, Ordering::Relaxed);
    log::info!("staff resident driver polling every {POLL_MS}ms");
    tauri::async_runtime::spawn(async move {
        loop {
            tick(&app, &state).await;
            tokio::time::sleep(Duration::from_millis(POLL_MS)).await;
        }
    });
}

async fn tick(app: &AppHandle, state: &Arc<AppState>) {
    *state.staff.last_tick_at.lock().unwrap() = Some(now_iso());
    let claim = state
        .host
        .call("staff.scheduler.claim", json!({ "limit": CLAIM_LIMIT }))
        .await;
    let batch = match claim {
        Ok(batch) => {
            *state.staff.last_error.lock().unwrap() = None;
            batch
        }
        Err(error) => {
            *state.staff.last_error.lock().unwrap() = Some(error.to_string());
            return;
        }
    };
    for skipped in batch
        .get("skipped")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        log::warn!(
            "staff claim skipped {}: {}",
            text(&skipped, "employeeId"),
            text(&skipped, "error")
        );
    }
    let claims = batch
        .get("claims")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if claims.is_empty() {
        return;
    }
    state
        .staff
        .claimed_total
        .fetch_add(claims.len() as u64, Ordering::Relaxed);
    for claim in claims {
        let app = app.clone();
        let state = state.clone();
        tauri::async_runtime::spawn(async move {
            let Ok(_permit) = state.staff.slots.acquire().await else {
                return;
            };
            run_claim(&app, &state, claim).await;
        });
    }
}

/// `staff/employees/runNow`: same executor as a scheduled slot, started with
/// the manual trigger so the run log says who asked for it.
pub async fn run_now(app: &AppHandle, state: &Arc<AppState>, employee_id: &str) -> Value {
    let started = state
        .host
        .call(
            "staff.run.start",
            json!({ "employeeId": employee_id, "trigger": "manual" }),
        )
        .await;
    let started = match started {
        Ok(started) => started,
        Err(error) => {
            return crate::envelope::envelope_from_rpc("host", &error);
        }
    };
    let run = started.get("run").cloned().unwrap_or(Value::Null);
    let employee = started.get("employee").cloned().unwrap_or(Value::Null);
    let claim = json!({
        "runId": run.get("id").cloned().unwrap_or(Value::Null),
        "employeeId": employee_id,
        "employeeName": text(&employee, "name"),
        "board": text(&employee, "board"),
        "sessionId": run.get("sessionId").cloned().unwrap_or(Value::Null),
        "providerId": run.get("providerId").cloned().unwrap_or(Value::Null),
        "modelId": run.get("modelId").cloned().unwrap_or(Value::Null),
        "workflowId": run.get("workflowId").cloned().unwrap_or(Value::Null),
        "workflowVersion": run.get("workflowVersion").cloned().unwrap_or(Value::Null),
        "trigger": "manual",
        "steps": started.get("steps").cloned().unwrap_or(json!([])),
        "startedAt": run.get("startedAt").cloned().unwrap_or(Value::Null),
    });
    let app_handle = app.clone();
    let state_handle = state.clone();
    let run_id = text(&run, "id");
    tauri::async_runtime::spawn(async move {
        let Ok(_permit) = state_handle.staff.slots.acquire().await else {
            return;
        };
        run_claim(&app_handle, &state_handle, claim).await;
    });
    crate::envelope::ok(json!({
        "run": run,
        "sessionId": started.pointer("/run/sessionId").cloned().unwrap_or(Value::Null),
        "runId": run_id,
    }))
}

/// Execute one claimed slot. Every exit path settles the run row, so a run is
/// never left `running` while no executor owns it.
async fn run_claim(app: &AppHandle, state: &Arc<AppState>, claim: Value) {
    let run_id = text(&claim, "runId");
    if run_id.is_empty() {
        return;
    }
    let steps = claim
        .get("steps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    state.staff.begin_run(json!({
        "runId": run_id,
        "employeeId": text(&claim, "employeeId"),
        "employeeName": text(&claim, "employeeName"),
        "sessionId": text(&claim, "sessionId"),
        "step": 0,
        "stepCount": steps.len() as i64,
    }));
    emit_changed(app);
    let (status, error_code) = execute(&app, state, &claim, &run_id, &steps).await;
    finish_run(state, &run_id, status, error_code.as_deref()).await;
    state.staff.end_run(&run_id);
    emit_changed(app);
}

async fn execute(
    app: &AppHandle,
    state: &Arc<AppState>,
    claim: &Value,
    run_id: &str,
    steps: &[Value],
) -> (&'static str, Option<String>) {
    let session_id = text(claim, "sessionId");
    let employee_id = text(claim, "employeeId");
    if session_id.is_empty() {
        return ("failed", Some("SESSION_MISSING".to_string()));
    }

    let binding = match state
        .host
        .call("staff.run.binding", json!({ "runId": run_id }))
        .await
    {
        Ok(binding) => binding,
        Err(error) => return ("failed", Some(rpc_code(&error))),
    };
    let session = match state
        .host
        .call(
            "session.get",
            json!({ "id": session_id, "messageLimit": 1 }),
        )
        .await
    {
        Ok(session) => session.get("session").cloned().unwrap_or(Value::Null),
        Err(error) => return ("failed", Some(rpc_code(&error))),
    };
    let settings = state
        .host
        .call("settings.get", json!({}))
        .await
        .unwrap_or_else(|_| json!({}));
    // The run's own binding wins over the provider row: a credential profile
    // may point at a different endpoint and key than the row it names.
    let overrides = json!({
        "providerId": binding.get("providerId").cloned().unwrap_or(Value::Null),
        "modelId": binding.get("modelId").cloned().unwrap_or(Value::Null),
        "thinkingLevel": binding.get("thinkingLevel").cloned().unwrap_or(Value::Null),
        "baseUrl": binding.get("baseUrl").cloned().unwrap_or(Value::Null),
        "apiKey": binding.get("apiKey").cloned().unwrap_or(Value::Null),
    });
    let launch = match crate::launch::resolve_launch(state, &session, &settings, &overrides).await {
        Ok(launch) => launch,
        Err(envelope) => return ("failed", Some(envelope_code(&envelope))),
    };

    let mut done: Vec<String> = Vec::new();
    let mut material: Vec<String> = Vec::new();
    let total = steps.len();

    for (index, step) in steps.iter().enumerate() {
        let step_no = (index + 1) as i64;
        state.staff.set_step(run_id, step_no);
        emit_changed(app);

        let kind = text(step, "kind");
        let title = text(step, "title");
        let memory = step.get("memory").filter(|value| !value.is_null()).cloned();

        if kind == "humanGate" {
            let reason = text(step, "gate");
            record_step(
                state,
                run_id,
                step_no,
                &format!("等待人工确认：{reason}"),
                "",
            )
            .await;
            return ("failed", Some("HUMAN_GATE".to_string()));
        }

        // A read is answered by the store, not by a paid turn (spec §12).
        if kind == "memory" && memory_operation(&memory).as_str() == "read" {
            let items = read_memory(state, &employee_id, &memory).await;
            let listing = if items.is_empty() {
                NO_MATERIAL.to_string()
            } else {
                items.join("\n")
            };
            material.push(listing.clone());
            record_step(
                state,
                run_id,
                step_no,
                &format!("读到 {} 条记忆", items.len()),
                &listing,
            )
            .await;
            done.push(format!("{step_no}. {title}：读到 {} 条记忆", items.len()));
            continue;
        }

        let artifact = artifact_of(step);
        // A probe step reads through the site collector rather than the model
        // fetching by hand: the site answers anything but its own front end
        // with the same login wall (spec §15).
        let probe_line = if kind == "probe" {
            let sources = probe_sources(step);
            let manual = text(claim, "trigger") == "manual";
            let notes = crate::staff_sites::collect_for_step(app, state, &sources, manual).await;
            if notes.is_empty() {
                None
            } else {
                material.extend(notes);
                Some(PROBE_COLLECTED)
            }
        } else {
            None
        };
        let prompt = build_prompt(
            claim,
            step,
            step_no,
            total,
            &material,
            &done,
            artifact.as_deref(),
            probe_line,
        );
        let reply = match run_turn(state, &launch, &session_id, &prompt).await {
            Ok(reply) => reply,
            Err(error) => {
                let code = error.code.clone();
                record_step(state, run_id, step_no, &error.message, "").await;
                return ("failed", Some(code));
            }
        };
        let line = closing_line(&reply);
        let line = if line.is_empty() {
            "（这一步没有返回文本）".to_string()
        } else {
            line
        };
        record_step(state, run_id, step_no, &line, &reply).await;
        done.push(format!("{step_no}. {title}：{line}"));

        if let Some(artifact) = artifact {
            file_artifact(state, claim, run_id, &artifact, &reply).await;
        }
        if memory.is_some() && matches!(memory_operation(&memory).as_str(), "write" | "publish") {
            write_memory(state, claim, run_id, &memory, &line).await;
        }
        if kind == "handoff" {
            hand_off(state, run_id, step, &line).await;
        }
    }

    ("succeeded", None)
}

struct TurnError {
    code: String,
    message: String,
}

fn turn_error(code: &str, message: &str) -> TurnError {
    TurnError {
        code: code.to_string(),
        message: if message.is_empty() {
            code.to_string()
        } else {
            message.to_string()
        },
    }
}

/// Run one step as a turn in the run's own session and wait for its reply.
async fn run_turn(
    state: &Arc<AppState>,
    launch: &crate::launch::Launch,
    session_id: &str,
    content: &str,
) -> Result<String, TurnError> {
    let begin = state
        .host
        .call(
            "session.beginTurn",
            json!({
                "sessionId": session_id,
                "providerId": launch.provider_id,
                "modelId": launch.model_id,
            }),
        )
        .await
        .map_err(|error| turn_error(&rpc_code(&error), &error.to_string()))?;
    let turn_id = begin
        .get("turnId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if turn_id.is_empty() {
        return Err(turn_error("AGENT_ERROR", "host did not open a turn"));
    }
    let user_message_id = uuid::Uuid::new_v4().to_string();
    let user_message = json!({
        "id": user_message_id,
        "role": "user",
        "content": content,
        "createdAt": now_iso(),
    });
    if let Err(error) = state
        .host
        .call(
            "session.appendMessage",
            json!({
                "sessionId": session_id,
                "message": user_message,
                "turnId": turn_id,
            }),
        )
        .await
    {
        return Err(turn_error(&rpc_code(&error), &error.to_string()));
    }

    let mut prompt = launch.sidecar_params.clone();
    prompt["turnId"] = json!(turn_id);
    prompt["content"] = json!(content);
    prompt["attachments"] = json!([]);
    prompt["userMessageId"] = json!(user_message_id);

    // Register before the call: the sidecar streams into the dispatcher, and a
    // turn that finishes immediately must not race the watcher into existence.
    let mut events = state.staff.watch(&turn_id);
    let accepted = state.sidecar.call("agent.prompt", prompt).await;
    if let Err(error) = accepted {
        state.staff.release(&turn_id);
        let _ = state
            .host
            .call(
                "session.endTurn",
                json!({ "sessionId": session_id, "turnId": turn_id, "status": "error" }),
            )
            .await;
        return Err(turn_error(&rpc_code(&error), &error.to_string()));
    }
    let reply = collect_turn(&mut events).await;
    state.staff.release(&turn_id);
    match reply {
        Ok(text) => Ok(text),
        Err(error) => {
            let _ = state
                .sidecar
                .call(
                    "agent.abort",
                    json!({ "sessionId": session_id, "turnId": turn_id }),
                )
                .await;
            Err(error)
        }
    }
}

/// Wait for `agent_end`, keeping the last assistant message as the reply.
async fn collect_turn(events: &mut UnboundedReceiver<Value>) -> Result<String, TurnError> {
    let mut reply = String::new();
    loop {
        let next = tokio::time::timeout(STEP_TIMEOUT, events.recv()).await;
        let envelope = match next {
            Ok(Some(envelope)) => envelope,
            Ok(None) => return Err(turn_error("AGENT_ERROR", "turn channel closed")),
            Err(_) => return Err(turn_error("STEP_TIMEOUT", "步骤超时")),
        };
        let event = envelope.get("event").cloned().unwrap_or(Value::Null);
        // A delegate's rows belong to its Task card, never to this reply.
        let is_delegate = envelope
            .get("parentToolCallId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .is_some();
        if is_delegate {
            continue;
        }
        match event.get("type").and_then(Value::as_str) {
            Some("message_end") => {
                let message = event.get("message").cloned().unwrap_or(Value::Null);
                if message.get("role").and_then(Value::as_str) == Some("assistant") {
                    if let Some(content) = message.get("content").and_then(Value::as_str) {
                        reply = content.to_string();
                    }
                }
            }
            Some("agent_end") => return Ok(reply),
            Some("error") => {
                let error = event.get("error").cloned().unwrap_or(Value::Null);
                return Err(turn_error(
                    error
                        .get("code")
                        .and_then(Value::as_str)
                        .unwrap_or("AGENT_ERROR"),
                    error.get("message").and_then(Value::as_str).unwrap_or(""),
                ));
            }
            _ => {}
        }
    }
}

async fn record_step(state: &Arc<AppState>, run_id: &str, step: i64, result: &str, report: &str) {
    let result = truncate(result, 4_000);
    let mut params = json!({ "runId": run_id, "step": step, "result": result });
    if !report.is_empty() {
        params["report"] = json!(report);
    }
    if let Err(error) = state.host.call("staff.run.step", params).await {
        log::warn!("staff run {run_id} step {step} not recorded: {error}");
    }
}

async fn finish_run(state: &Arc<AppState>, run_id: &str, status: &str, error_code: Option<&str>) {
    let mut params = json!({ "runId": run_id, "status": status });
    if let Some(code) = error_code {
        params["errorCode"] = json!(code);
    }
    if let Err(error) = state.host.call("staff.run.finish", params).await {
        log::warn!("staff run {run_id} not settled: {error}");
    }
}

/// The prompt a step is handed. host-core owns ordering and gating; wording
/// lives here (spec §12), including the closing contract the reply is read by.
fn build_prompt(
    claim: &Value,
    step: &Value,
    step_no: i64,
    total: usize,
    material: &[String],
    done: &[String],
    artifact: Option<&str>,
    probe_line: Option<&str>,
) -> String {
    let title = text(step, "title");
    let instruction = text(step, "instruction");
    let config = step.get("config").cloned().unwrap_or(json!({}));
    let mut prompt = String::new();
    prompt.push_str(&format!(
        "你是「{}」，正在执行自己工作流的第 {step_no}/{total} 步：{title}。\n",
        text(claim, "employeeName")
    ));
    if let Some(directive) = claim.get("directive").filter(|value| !value.is_null()) {
        let task = directive.get("task").and_then(Value::as_str).unwrap_or("");
        if !task.is_empty() {
            prompt.push_str(&format!("这一轮在替上级执行派单：{task}\n"));
        }
    }
    if !instruction.is_empty() {
        prompt.push_str(&format!("\n{instruction}\n"));
    }
    if config
        .as_object()
        .map(|map| !map.is_empty())
        .unwrap_or(false)
    {
        prompt.push_str(&format!(
            "\n这一步的参数：\n```json\n{}\n```\n",
            serde_json::to_string_pretty(&config).unwrap_or_default()
        ));
    }
    let kind = text(step, "kind");
    if kind == "probe" {
        prompt.push_str(probe_line.unwrap_or(PROBE_EMPTY));
    }
    prompt.push_str("\n驱动交给这一步的材料：\n");
    if material.is_empty() {
        prompt.push_str(&format!("{NO_MATERIAL}\n"));
    } else {
        for item in material {
            prompt.push_str(item);
            prompt.push('\n');
        }
    }
    if !done.is_empty() {
        prompt.push_str("\n这一轮已经做完的事：\n");
        for item in done {
            prompt.push_str(&format!("- {item}\n"));
        }
    }
    prompt.push_str(
        "\n只依据上面交给你的材料和已经做完的事，不要翻这台机器上其他文件，也不要自己去找别的证据。\
         材料不够就直说缺什么并停下，这也是一个完整的回答。\n",
    );
    if let Some(artifact) = artifact {
        prompt.push_str(&artifact_contract(artifact));
    }
    prompt.push_str(&closing_contract(step));
    prompt
}

/// The channels a probe names. `sources` is the current spelling; `channel`
/// is the older one-element form the parser still accepts.
fn probe_sources(step: &Value) -> Vec<String> {
    let config = step.get("config");
    if let Some(rows) = config
        .and_then(|config| config.get("sources"))
        .and_then(Value::as_array)
    {
        let list: Vec<String> = rows
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        if !list.is_empty() {
            return list;
        }
    }
    config
        .and_then(|config| config.get("channel"))
        .and_then(Value::as_str)
        .map(|channel| vec![channel.to_string()])
        .unwrap_or_default()
}

/// `produce` (and analyze/produce-style nodes) may name the row they file.
fn artifact_of(step: &Value) -> Option<String> {
    let artifact = step.get("config")?.get("artifact")?.as_str()?.trim();
    matches!(artifact, "leads" | "content" | "outreach" | "advice").then(|| artifact.to_string())
}

fn artifact_contract(artifact: &str) -> String {
    let shape = match artifact {
        "leads" => {
            r#"{"items":[{"title":"","site":"","url":"","author":"","quote":"","topic":"","angle":"","score":0,"tags":[]}]}"#
        }
        "content" | "outreach" => {
            r#"{"items":[{"title":"","body":"","channel":"","topic":"","leadId":""}]}"#
        }
        _ => {
            r#"{"board":"leads|contents|report|arsenal","title":"","body":"","action":"","impact":"","costNote":""}"#
        }
    };
    format!(
        "\n最后用一个 ```json 代码块给出这一步要归档的数据，形状如下：\n{shape}\n\
         没有可归档的内容就写 {{\"items\":[]}}，不要编造。代码块之后再用最后一行写 STEP RESULT。\n"
    )
}

fn closing_contract(step: &Value) -> String {
    let recording = step
        .get("memory")
        .and_then(|memory| memory.get("operation"))
        .and_then(Value::as_str)
        .map(|operation| matches!(operation, "write" | "publish"))
        .unwrap_or(false);
    if recording {
        "\n最后一行就是这条要记下来的内容本身（不是对步骤的说明），格式必须是：\n\
         STEP RESULT: <一句话结论>\n"
            .to_string()
    } else {
        "\n最后一行必须是：\nSTEP RESULT: <一句话，说明这一步实际产出了什么；什么都没产出也照实写>\n"
            .to_string()
    }
}

/// The reply's closing line: last line *starting* with the marker (spec §12).
fn closing_line(reply: &str) -> String {
    let marker = "STEP RESULT:";
    let found = reply
        .lines()
        .filter_map(|line| line.trim().strip_prefix(marker))
        .last()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string);
    if let Some(line) = found {
        return line;
    }
    reply
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("```"))
        .unwrap_or("")
        .to_string()
}

fn memory_operation(memory: &Option<Value>) -> String {
    memory
        .as_ref()
        .and_then(|plan| plan.get("operation"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn memory_kind(memory: &Option<Value>) -> String {
    memory
        .as_ref()
        .and_then(|plan| plan.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("signal")
        .to_string()
}

/// A `read` is answered by the store: shared items plus the employee's own.
async fn read_memory(
    state: &Arc<AppState>,
    employee_id: &str,
    memory: &Option<Value>,
) -> Vec<String> {
    let mut filter = json!({ "ownerId": employee_id, "limit": 8 });
    if let Some(limit) = memory
        .as_ref()
        .and_then(|plan| plan.get("limit"))
        .and_then(Value::as_i64)
    {
        filter["limit"] = json!(limit);
    }
    if let Some(scope) = memory
        .as_ref()
        .and_then(|plan| plan.get("scope"))
        .and_then(Value::as_str)
    {
        filter["scope"] = json!(scope);
    }
    if let Some(kind) = memory
        .as_ref()
        .and_then(|plan| plan.get("kind"))
        .and_then(Value::as_str)
    {
        filter["kind"] = json!(kind);
    }
    let Ok(result) = state.host.call("staff.memory.query", filter).await else {
        return Vec::new();
    };
    result
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let text = item.get("text").and_then(Value::as_str)?;
                    let kind = item.get("kind").and_then(Value::as_str).unwrap_or("fact");
                    let key = item.get("key").and_then(Value::as_str).unwrap_or("");
                    Some(if key.is_empty() {
                        format!("- [{kind}] {text}")
                    } else {
                        format!("- [{kind}] {key}：{text}")
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `write` records to the employee's own scope, `publish` to shared (spec §5).
async fn write_memory(
    state: &Arc<AppState>,
    claim: &Value,
    run_id: &str,
    memory: &Option<Value>,
    text_line: &str,
) {
    if text_line.trim().is_empty() {
        return;
    }
    let operation = memory_operation(memory);
    let scope = memory
        .as_ref()
        .and_then(|plan| plan.get("scope"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            if operation == "publish" {
                "shared".to_string()
            } else {
                "employee".to_string()
            }
        });
    let mut params = json!({
        "scope": scope,
        "kind": memory_kind(memory),
        "text": truncate(text_line, 400),
        "sourceKind": "run",
        "sourceRef": run_id,
        "employeeId": text(claim, "employeeId"),
        "runId": run_id,
    });
    if scope == "employee" {
        params["ownerId"] = json!(text(claim, "employeeId"));
    }
    let method = if operation == "publish" {
        "staff.memory.publish"
    } else {
        "staff.memory.write"
    };
    if let Err(error) = state.host.call(method, params).await {
        log::warn!("staff run {run_id} could not record memory: {error}");
    }
}

/// File the row a step's fenced JSON block asks for. A parse miss files
/// nothing: a half-parsed card would be a claim about a real person.
async fn file_artifact(
    state: &Arc<AppState>,
    claim: &Value,
    run_id: &str,
    artifact: &str,
    reply: &str,
) {
    let Some(parsed) = fenced_json(reply) else {
        log::warn!("staff run {run_id}: {artifact} step reply carried no JSON block");
        return;
    };
    let entries: Vec<Value> = match parsed.get("items").and_then(Value::as_array) {
        Some(items) => items.clone(),
        None if parsed.is_object() => vec![parsed.clone()],
        None => Vec::new(),
    };
    let employee_id = text(claim, "employeeId");
    for entry in entries {
        if !entry.is_object() {
            continue;
        }
        let mut row = entry;
        row["employeeId"] = json!(employee_id);
        row["runId"] = json!(run_id);
        let call = match artifact {
            "leads" => {
                if text(&row, "title").is_empty() {
                    continue;
                }
                row["stage"] = json!("new");
                Some(("staff.lead.save", row))
            }
            "content" | "outreach" => {
                if text(&row, "title").is_empty() {
                    continue;
                }
                // A run files drafts and never approves its own work.
                row["status"] = json!("draft");
                if artifact == "outreach" && text(&row, "channel").is_empty() {
                    row["channel"] = json!("outreach");
                }
                Some(("staff.content.save", row))
            }
            _ => {
                if text(&row, "title").is_empty() {
                    continue;
                }
                let board = row
                    .get("board")
                    .and_then(Value::as_str)
                    .filter(|board| matches!(*board, "leads" | "contents" | "report" | "arsenal"))
                    .unwrap_or("report");
                row["board"] = json!(board);
                Some(("staff.advice.create", row))
            }
        };
        let Some((method, params)) = call else {
            continue;
        };
        if let Err(error) = state.host.call(method, params).await {
            log::warn!("staff run {run_id} could not file {artifact}: {error}");
        }
    }
}

/// Hand work to another employee. Only a manager may dispatch, and only within
/// its own blueprint scope; a refused handoff is logged, not fatal.
async fn hand_off(state: &Arc<AppState>, run_id: &str, step: &Value, line: &str) {
    let target = text(step, "handoffEmployeeId");
    if target.is_empty() {
        return;
    }
    let params = json!({
        "employeeId": target,
        "reasonRunId": run_id,
        "verb": "answer",
        "task": truncate(line, 1_000),
    });
    if let Err(error) = state.host.call("staff.directive.assign", params).await {
        log::warn!("staff run {run_id} handoff to {target} refused: {error}");
    }
}

fn fenced_json(reply: &str) -> Option<Value> {
    let re = regex::Regex::new(r"(?s)```(?:json)?\s*(.*?)```").ok()?;
    let last = re.captures_iter(reply).last()?;
    let raw = last.get(1)?.as_str().trim();
    serde_json::from_str::<Value>(raw).ok()
}

fn emit_changed(app: &AppHandle) {
    let _ = app.emit("pi-desktop/staff/event/changed", json!({ "at": now_iso() }));
}

fn rpc_code(error: &RpcError) -> String {
    if let RpcError::Remote { data, .. } = error {
        if let Some(code) = data
            .pointer("/error/code")
            .or_else(|| data.get("code"))
            .and_then(Value::as_str)
        {
            return code.to_string();
        }
    }
    "AGENT_ERROR".to_string()
}

fn envelope_code(envelope: &Value) -> String {
    envelope
        .pointer("/error/code")
        .and_then(Value::as_str)
        .unwrap_or("AGENT_ERROR")
        .to_string()
}

fn truncate(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    trimmed.chars().take(limit).collect::<String>()
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_line_takes_the_last_marker_line() {
        let reply = "我在整理。\n这里引用一下 STEP RESULT: 不是结论\n真正结论\nSTEP RESULT: 本轮留下 2 条候选\n```\n尾注\n";
        assert_eq!(closing_line(reply), "本轮留下 2 条候选");
    }

    #[test]
    fn closing_line_falls_back_to_the_last_prose_line() {
        let reply = "第一段。\n\n最后一段。\n";
        assert_eq!(closing_line(reply), "最后一段。");
    }

    #[test]
    fn fenced_json_reads_the_last_block() {
        let reply = "说明\n```json\n{\"items\":[{\"title\":\"a\"}]}\n```\n收尾";
        let parsed = fenced_json(reply).expect("block");
        assert_eq!(parsed["items"][0]["title"], "a");
    }

    #[test]
    fn probe_sources_take_the_list_and_the_legacy_channel() {
        let step = json!({ "config": { "sources": ["zhihu", "juejin"] } });
        assert_eq!(probe_sources(&step), vec!["zhihu", "juejin"]);
        let legacy = json!({ "config": { "channel": "zhihu" } });
        assert_eq!(probe_sources(&legacy), vec!["zhihu"]);
        assert!(probe_sources(&json!({ "config": {} })).is_empty());
    }

    #[test]
    fn artifact_names_outside_the_closed_set_file_nothing() {
        let step = json!({ "config": { "artifact": "待回答的问题清单" } });
        assert!(artifact_of(&step).is_none());
        let step = json!({ "config": { "artifact": "leads" } });
        assert_eq!(artifact_of(&step).as_deref(), Some("leads"));
    }
}
