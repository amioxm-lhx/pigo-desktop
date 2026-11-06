//! The staff.* RPC surface.
//!
//! Kept out of rpc/mod.rs so that module stays a dispatcher, and kept next to
//! the domain it exposes so a new staff call is one edit in one place.
//!
//! Two things must never cross this boundary: an API key, and a claim that a run
//! is the user.

use super::*;
use serde_json::json;

fn id(params: &Value) -> Result<String> {
    required_str(params, "id")
}

fn not_found(what: &str, id: &str) -> anyhow::Error {
    anyhow!("NOT_FOUND: {what} {id} does not exist")
}

/// Guard the author identity of a memory write.
///
/// The renderer and the user act on their own behalf; a run is identified by a
/// runId. Accepting sourceKind = "user" from a run would let a run author a
/// constraint, which is the one thing the memory rules forbid outright.
fn memory_input(params: &Value) -> Result<MemoryWriteInput> {
    let input = MemoryWriteInput::from_json(params)?;
    if input.run_id.is_some() && input.source_kind == "user" {
        bail!("PERMISSION_DENIED: a run may not claim the user as its source");
    }
    Ok(input)
}

/// The author of a call that does not carry a sourceKind: a run identifies
/// itself with a run id, everyone else is the user.
fn author(params: &Value) -> &'static str {
    if params.get("runId").and_then(Value::as_str).is_some() {
        "run"
    } else {
        "user"
    }
}

pub(crate) fn handle(ctx: &StaffContext<'_>, method: &str, params: &Value) -> Result<Value> {
    let db = ctx.db;
    let secrets = ctx.secrets;
    match method {
        // ---- built-in employees -------------------------------------------
        "staff.presets.list" => list_presets(db),
        "staff.presets.install" => install_presets(db, secrets, params),

        // ---- the boards the middle managers read ---------------------------
        //
        // These are the owner's surfaces first: a board row is read, moved and
        // decided by the person looking at it. A run is allowed to file a card
        // or a draft -- that is how the boards fill up -- but the two doors
        // that mean "I decided" are checked below and are the owner's alone.
        "staff.leads.list" => Ok(json!({
            "leads": list_leads(db, params)?,
            "counts": lead_stage_counts(db)?,
        })),
        "staff.lead.save" => Ok(json!({ "lead": upsert_lead(db, params)? })),
        "staff.lead.move" => {
            let lead_id = required_str(params, "id")?;
            let lead = move_lead(db, params)?
                .ok_or_else(|| not_found("lead", &lead_id))?;
            Ok(json!({ "lead": lead }))
        },
        "staff.contents.list" => Ok(json!({
            "contents": list_contents(db, params)?,
            "counts": content_status_counts(db)?,
        })),
        "staff.content.save" => Ok(json!({ "content": upsert_content(db, params)? })),
        "staff.content.status" => {
            let content_id = required_str(params, "id")?;
            let content = set_content_status(db, params)?
                .ok_or_else(|| not_found("content", &content_id))?;
            Ok(json!({ "content": content }))
        },
        // The return leg: which published pieces to look at, and what the page
        // said when somebody looked. Ingestion is a collector's call, never a
        // model's -- a reply count is a fact about the world, not a judgement.
        "staff.watch.list" => Ok(json!({
            "targets": watch_targets(db, params)?,
            "summary": watch_summary(db, params)?,
        })),
        "staff.watch.ingest" => ingest_watch(db, params),
        "staff.metrics.list" => Ok(json!({ "metrics": list_metrics(db, params)? })),
        "staff.metric.save" => Ok(json!({ "metric": upsert_metric(db, params)? })),
        "staff.report.summary" => report_summary(db, params),
        "staff.advice.list" => Ok(json!({ "suggestions": list_suggestions(db, params)? })),
        "staff.advice.derive" => Ok(json!({ "raised": derive_suggestions(db, params)? })),
        "staff.advice.create" => Ok(json!({ "suggestion": create_suggestion(db, params)? })),
        "staff.advice.decide" => {
            let suggestion_id = required_str(params, "id")?;
            let suggestion = decide_suggestion(db, params)?
                .ok_or_else(|| not_found("suggestion", &suggestion_id))?;
            Ok(json!({ "suggestion": suggestion }))
        },
        "staff.directives.list" => Ok(json!({ "directives": list_directives(db, params)? })),
        // A manager's own output. The console draws these above the cards they
        // caused, which is the difference between "a board that filled up" and
        // "a manager that decided something".
        "staff.decisions.list" => Ok(json!({ "decisions": list_decisions(db, params)? })),
        // Written by a manager's run, and by nothing else: an owner approves
        // proposals, a manager makes calls.
        "staff.decision.record" => Ok(json!({ "decision": record_decision(db, params)? })),
        // Handing work to another employee. The issuer is taken from the run
        // that made the call, never from the request body alone.
        "staff.directive.assign" => Ok(json!({ "directive": assign_directive(db, params)? })),
        "staff.dispatch.scope" => {
            let employee_id = required_str(params, "employeeId")?;
            dispatch_scope_of(db, &employee_id)
        },
        // What a board node reads. Deterministic: the numbers a manager decides
        // from are the rows the owner is looking at.
        "staff.board.read" => board_brief(db, params),
        "staff.directive.update" => {
            let directive_id = required_str(params, "id")?;
            let directive = update_directive(db, params)?
                .ok_or_else(|| not_found("directive", &directive_id))?;
            Ok(json!({ "directive": directive }))
        },

        // ---- the data ledger ----------------------------------------------
        // One call for both, so a rule's "would take" and its "did take" can
        // never disagree -- the page draws them side by side.
        "staff.data.stats" => data_stats(db, params),
        "staff.data.prune" => prune(db, params),

        // ---- credential profiles ------------------------------------------
        "staff.profiles.list" => Ok(json!({ "profiles": list_profiles(db, secrets)? })),
        "staff.profile.create" => Ok(json!({
            "profile": create_profile(db, secrets, params)?
        })),
        "staff.profile.update" => {
            let profile_id = id(params)?;
            let profile = update_profile(db, secrets, &profile_id, params)?
                .ok_or_else(|| not_found("credential profile", &profile_id))?;
            Ok(json!({ "profile": profile }))
        }
        "staff.profile.setSecret" => {
            let profile_id = id(params)?;
            let value = required_str(params, "secretValue")?;
            let profile = set_profile_secret(db, secrets, &profile_id, &value)?
                .ok_or_else(|| not_found("credential profile", &profile_id))?;
            Ok(json!({ "profile": profile }))
        }
        "staff.profile.delete" => {
            let profile_id = id(params)?;
            Ok(json!({ "deleted": delete_profile(db, secrets, &profile_id)? }))
        }

        // ---- workflows ----------------------------------------------------
        "staff.workflow.list" => Ok(json!({ "workflows": list_workflows(db)? })),
        "staff.workflow.get" => {
            let workflow_id = id(params)?;
            let workflow = get_workflow(db, &workflow_id)?
                .ok_or_else(|| not_found("workflow", &workflow_id))?;
            let version = match params.get("version").and_then(Value::as_i64) {
                Some(version) => version,
                None => workflow.latest_version,
            };
            let graph = get_graph(db, &workflow_id, version)?.map(|(_, raw)| {
                serde_json::from_str::<Value>(&raw)
                    .unwrap_or_else(|_| json!({ "nodes": [], "edges": [] }))
            });
            Ok(json!({ "workflow": workflow, "version": version, "graph": graph }))
        }
        "staff.workflow.create" => Ok(json!({ "workflow": create_workflow(db, params)? })),
        "staff.workflow.update" => {
            let workflow_id = id(params)?;
            let workflow = update_workflow(db, &workflow_id, params)?
                .ok_or_else(|| not_found("workflow", &workflow_id))?;
            Ok(json!({ "workflow": workflow }))
        }
        "staff.workflow.versions" => {
            let workflow_id = id(params)?;
            if get_workflow(db, &workflow_id)?.is_none() {
                return Err(not_found("workflow", &workflow_id));
            }
            Ok(json!({ "versions": list_versions(db, &workflow_id)? }))
        }
        "staff.workflow.delete" => {
            let workflow_id = id(params)?;
            Ok(json!({ "deleted": delete_workflow(db, &workflow_id)? }))
        }
        // The editor asks before it saves, so a half-drawn graph never reaches
        // storage and never reaches a run.
        "staff.workflow.validate" => {
            let graph = params.get("graph").cloned().unwrap_or(Value::Null);
            Ok(match parse_graph(&graph) {
                Ok(graph) => json!({
                    "valid": true,
                    "nodeCount": graph.node_count(),
                    "steps": graph.compile().map(|steps| steps.len()).unwrap_or(0)
                }),
                Err(error) => json!({ "valid": false, "error": error.to_string() }),
            })
        }

        // ---- employees ----------------------------------------------------
        "staff.employee.list" => Ok(json!({ "employees": list_employees(db)? })),
        "staff.employee.get" => {
            let employee_id = id(params)?;
            let employee = get_employee(db, &employee_id)?
                .ok_or_else(|| not_found("employee", &employee_id))?;
            Ok(json!({ "employee": employee }))
        }
        "staff.employee.create" => Ok(json!({ "employee": create_employee(db, params)? })),
        "staff.employee.update" => {
            let employee_id = id(params)?;
            let employee = update_employee(db, &employee_id, params)?
                .ok_or_else(|| not_found("employee", &employee_id))?;
            Ok(json!({ "employee": employee }))
        }
        "staff.employee.delete" => {
            let employee_id = id(params)?;
            Ok(json!({ "deleted": delete_employee(db, &employee_id)? }))
        }
        "staff.employee.runs" => {
            let employee_id = optional_str(params, "employeeId");
            let limit = params.get("limit").and_then(Value::as_i64).unwrap_or(50);
            Ok(json!({ "runs": list_runs(db, employee_id.as_deref(), limit)? }))
        }
        "staff.employee.refreshSchedule" => {
            let employee_id = id(params)?;
            let next = refresh_schedule(db, &employee_id)?
                .ok_or_else(|| not_found("employee", &employee_id))?;
            Ok(json!({ "nextRunAt": next }))
        }

        // ---- runs ---------------------------------------------------------
        "staff.run.start" => {
            let employee_id = required_str(params, "employeeId")?;
            let trigger = params
                .get("trigger")
                .and_then(Value::as_str)
                .unwrap_or("manual");
            let session_id = optional_str(params, "sessionId");
            let started = start_run(ctx, &employee_id, trigger, session_id.as_deref())?;
            let employee = get_employee(db, &employee_id)?
                .ok_or_else(|| not_found("employee", &employee_id))?;
            Ok(json!({
                "run": started.run,
                "steps": started.steps,
                "employee": employee
            }))
        }
        "staff.run.finish" => {
            let run_id = required_str(params, "runId")?;
            let status = params
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("succeeded");
            let error_code = optional_str(params, "errorCode");
            let ok = schedule::finish_run(db, &run_id, status, error_code.as_deref())?;
            if !ok {
                return Err(not_found("running employee run", &run_id));
            }
            Ok(json!({ "finished": true }))
        }
        "staff.run.step" => {
            let run_id = required_str(params, "runId")?;
            let step = params
                .get("step")
                .and_then(Value::as_i64)
                .ok_or_else(|| anyhow!("INVALID_ARGUMENT: step is required"))?;
            let result = optional_str(params, "result");
            let report = optional_str(params, "report");
            let ok = set_run_step(db, &run_id, step, result.as_deref(), report.as_deref())?;
            if !ok {
                return Err(not_found("running employee run", &run_id));
            }
            Ok(json!({ "step": step }))
        }

        // ---- run binding ---------------------------------------------------
        // The one call that hands key material to the caller, and the reason it
        // is safe: no IPC channel in the Electron main process registers it, so
        // only the in-process resident driver can reach it (spec 20 section 7).
        "staff.run.binding" => {
            let run_id = required_str(params, "runId")?;
            let binding = resolve_binding_for_run(db, secrets, &run_id)?;
            Ok(json!({
                "employeeId": binding.employee_id,
                "credentialProfileId": binding.credential_profile_id,
                "providerId": binding.provider_id,
                "modelId": binding.model_id,
                "thinkingLevel": binding.thinking_level,
                "baseUrl": binding.base_url,
                "apiKey": binding.api_key,
            }))
        }

        // ---- scheduler ----------------------------------------------------
        // Claiming is one call on purpose: it advances next_run_at and opens the
        // session in the same step, so a resident driver cannot double-start a
        // slot by asking twice.
        "staff.scheduler.claim" => {
            let limit = params.get("limit").and_then(Value::as_i64).unwrap_or(8);
            Ok(serde_json::to_value(claim_due(ctx, limit)?)?)
        }

        // ---- memory -------------------------------------------------------
        "staff.memory.write" => Ok(json!({ "item": write_memory(db, memory_input(params)?)? })),
        "staff.memory.publish" => Ok(json!({ "item": publish_memory(db, memory_input(params)?)? })),
        "staff.memory.query" => {
            let filter = MemoryFilter::from_json(params)?;
            Ok(json!({ "items": query_memory(db, &filter)? }))
        }
        "staff.memory.supersede" => {
            let old_id = id(params)?;
            Ok(json!({ "item": supersede_memory(db, &old_id, memory_input(params)?)? }))
        }
        "staff.memory.retire" => {
            let item_id = id(params)?;
            let retired = retire_memory(db, &item_id, author(params))?;
            if !retired {
                return Err(not_found("active memory item", &item_id));
            }
            Ok(json!({ "retired": true }))
        }
        other => bail!("INVALID_ARGUMENT: unknown staff method {other}"),
    }
}
