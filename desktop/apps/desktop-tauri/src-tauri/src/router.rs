//! `pi_desktop_invoke` — the single Tauri command every renderer channel
//! flows through. Mirrors the Electron `ipc/register.ts` dispatch: pure
//! proxy rows first (proxy.rs), then local/wrapped handlers, and an explicit
//! phase-2 marker for channels the Electron main implements that this shell
//! does not carry yet.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::envelope::{self, err, not_implemented, ok};
use crate::events::now_ms;
use crate::fsops;
use crate::launch;
use crate::native;
use crate::plugins;
use crate::proxy::{PROXY_HOST, PROXY_SIDECAR};
use crate::rpc::{envelope_result, RpcError};
use crate::state::AppState;

/// Plugin-registry mutations: forward to host-core, then reconcile the Node
/// plugin runtime with the new set. The Node processes are the only views of
/// `plugins.list` that must be updated before the next renderer refresh.
const PLUGIN_ROW_CHANNELS: &[(&str, &str)] = &[
    ("pi-desktop/plugin/enable", "plugins.enable"),
    ("pi-desktop/plugin/disable", "plugins.disable"),
    ("pi-desktop/plugin/uninstall", "plugins.uninstall"),
    // `plugin/loadDev` is deliberately absent: it is a wrapped channel (the
    // shell asks for the folder, host-core never receives an empty path). A
    // row here would forward the renderer's empty params and answer
    // "path required" before the picker arm below can run.
    ("pi-desktop/plugin/setScope", "plugins.setScope"),
    ("pi-desktop/plugin/setAutoUpdate", "plugins.setAutoUpdate"),
    (
        "pi-desktop/plugin/installFromPath",
        "plugins.installFromPath",
    ),
    (
        "pi-desktop/plugin/installFromPackage",
        "plugins.installFromPackage",
    ),
];

#[tauri::command]
pub async fn pi_desktop_invoke(
    app: AppHandle,
    state: State<'_, std::sync::Arc<AppState>>,
    channel: String,
    args: Vec<Value>,
) -> Result<Value, Value> {
    let state = state.inner().clone();
    Ok(route(app, &state, &channel, &args).await)
}

fn params_of(args: &[Value]) -> Value {
    args.first().cloned().unwrap_or(json!({}))
}

fn browser_result(result: Result<Value, crate::browser::BrowserError>) -> Value {
    match result {
        Ok(value) => ok(value),
        Err(error) => err(&error.code, error.message),
    }
}

fn arg_str<'a>(params: &'a Value, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        if let Some(v) = params.get(*key).and_then(|v| v.as_str()) {
            return Some(v);
        }
    }
    None
}

/// Site channels arrive as a bare string (`signIn`) or as `{ channel }`
/// inside a merged patch (`update`).
fn site_channel(params: &Value) -> String {
    params
        .as_str()
        .or_else(|| params.get("channel").and_then(Value::as_str))
        .unwrap_or("")
        .to_string()
}

fn site_result(result: Result<Value, crate::staff_sites::SiteError>) -> Value {
    match result {
        Ok(value) => ok(value),
        Err(error) => err(error.code, error.message),
    }
}

pub async fn route(
    app: AppHandle,
    state: &std::sync::Arc<AppState>,
    channel: &str,
    args: &[Value],
) -> Value {
    let mut params = params_of(args);

    // `session/configure` is a two-argument channel: the renderer passes the
    // session id and the configuration object separately, while host-core
    // expects one `{ id, mode, ... }` record. Without this merge the bare string
    // reached host-core as its whole payload and answered "id required".
    if channel == "pi-desktop/session/configure" {
        if let (Some(id), Some(config)) = (args.first().and_then(Value::as_str), args.get(1)) {
            let mut merged = config.clone();
            if let Some(object) = merged.as_object_mut() {
                object.insert("id".into(), json!(id));
                params = merged;
            }
        }
    }

    // `staff/sites/update` is a two-argument channel: the renderer passes the
    // channel name and the patch separately.
    if channel == "pi-desktop/staff/sites/update" {
        if let (Some(name), Some(patch)) = (args.first().and_then(Value::as_str), args.get(1)) {
            let mut merged = patch.clone();
            if let Some(object) = merged.as_object_mut() {
                object.insert("channel".into(), json!(name));
                params = merged;
            }
        }
    }

    // Electron ipc handlers often took a bare string id and wrapped it into
    // `{id}` before calling the backend; the renderer still sends the bare
    // string. Normalize for those channels (serde peers reject a bare string
    // where an object is expected).
    if params.is_string() {
        let s = params.as_str().unwrap_or("").to_string();
        params = match channel {
            "pi-desktop/agent/getStatus" => json!({ "sessionId": s }),
            "pi-desktop/project/set" | "pi-desktop/project/openFolder" => json!({ "path": s }),
            "pi-desktop/providers/oauth/start" => json!({ "vendorId": s }),
            "pi-desktop/providers/oauth/cancel" => json!({ "loginId": s }),
            "pi-desktop/providers/oauth/delete" => json!({ "providerId": s }),
            "pi-desktop/session/delete"
            | "pi-desktop/providers/delete"
            | "pi-desktop/providers/testConnection"
            | "pi-desktop/scheduled/delete"
            | "pi-desktop/market/getDetail"
            | "pi-desktop/plugin/enable"
            | "pi-desktop/plugin/disable"
            | "pi-desktop/plugin/uninstall"
            | "pi-desktop/plugin/reload"
            | "pi-desktop/plugin/openPanel"
            | "pi-desktop/subagent/read"
            | "pi-desktop/subagent/remove" => json!({ "id": s }),
            _ => params,
        };
    }

    // Remote-control channel (Feishu). One entry point rather than six match
    // arms: the supervision of the `pigo im serve` child belongs in one place.
    if channel.starts_with("pi-desktop/im/channel/") {
        if let Some(result) = crate::remote_channel::handle(channel, state, &params) {
            return result;
        }
    }

    // Plugin registry mutations need a runtime reconcile after host-core
    // commits; they are otherwise the same pure proxy shape.
    if let Some(method) = PLUGIN_ROW_CHANNELS
        .iter()
        .find_map(|(name, method)| (*name == channel).then_some(*method))
    {
        let result = state.host.call(method, params.clone()).await;
        if result.is_ok() {
            state.plugins.clear_failures();
            let project = workspace_root(state).await;
            crate::plugin_host::sync(state, project.as_deref()).await;
        }
        return envelope_result("host", result);
    }

    // 1. Pure proxy rows.
    if let Some((_, method)) = PROXY_HOST.iter().find(|(c, _)| *c == channel) {
        let method = (*method).to_string();
        let host = state.host.clone();
        let result = host.call(&method, params).await;
        return envelope_result("host", result);
    }
    if let Some((_, method)) = PROXY_SIDECAR.iter().find(|(c, _)| *c == channel) {
        let method = (*method).to_string();
        let sidecar = state.sidecar.clone();
        let result = sidecar.call(&method, params).await;
        return envelope_result("sidecar", result);
    }

    // 2. Local and wrapped handlers.
    match channel {
        // ── app ────────────────────────────────────────────────────────────
        "pi-desktop/app/getVersion" => ok(launch::app_version(state).await),
        "pi-desktop/app/dismissOnboarding" => {
            let merged = match state.host.call("settings.get", json!({})).await {
                Ok(mut settings) => {
                    settings["onboardingDismissed"] = json!(true);
                    settings
                }
                Err(e) => return envelope::envelope_from_rpc("host", &e),
            };
            envelope_result("host", state.host.call("settings.set", merged).await)
        }
        "pi-desktop/app/systemFonts" => ok(json!([])),
        // The pigo binary the remote channel will run. Picked here rather than
        // in the page because only the shell can open a native dialog.
        "pi-desktop/im/channel/pickBinary" => {
            #[cfg(windows)]
            let extensions: &[&str] = &["exe"];
            #[cfg(not(windows))]
            let extensions: &[&str] = &[];
            match native::pick_file(&app, "pigo", extensions) {
                Some(path) => ok(json!({ "canceled": false, "path": path.to_string_lossy() })),
                None => ok(json!({ "canceled": true })),
            }
        }

        // ── updates (electron-updater is phase 2) ─────────────────────────
        "pi-desktop/updates/getState" => ok(json!({
            "mode": "disabled",
            "status": "idle",
            "currentVersion": crate::state::APP_VERSION,
        })),
        "pi-desktop/updates/check"
        | "pi-desktop/updates/download"
        | "pi-desktop/updates/install" => ok(
            json!({ "mode": "disabled", "status": "idle", "currentVersion": crate::state::APP_VERSION }),
        ),
        "pi-desktop/updates/openReleases" => {
            match native::open_url(&app, "https://gitee.com/harvey_danny/pi-agent-go/releases") {
                Ok(()) => ok(json!({ "ok": true })),
                Err(e) => err("INTERNAL", e),
            }
        }

        // ── notifications ─────────────────────────────────────────────────
        "pi-desktop/notification/setViewingSession" => {
            *state.viewing_session.lock().unwrap() = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .map(String::from);
            ok(json!({}))
        }
        "pi-desktop/notification/showNative" => {
            let title = params
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("pigo-desktop");
            let body = params.get("body").and_then(|v| v.as_str()).unwrap_or("");
            native::show_notification(&app, title, body);
            ok(json!({ "shown": true }))
        }

        // ── agent turn lifecycle ──────────────────────────────────────────
        "pi-desktop/agent/prompt" => agent_prompt(&app, state, &params).await,
        "pi-desktop/agent/btw" => agent_btw(state, &params).await,
        "pi-desktop/agent/dream" => agent_dream(state, &params).await,
        "pi-desktop/agent/executeApprovedPlan" => {
            agent_execute_approved_plan(&app, state, &params).await
        }
        "pi-desktop/agent/steer" => {
            // Reduced Electron flow: skip the steeringContext pre-check, take
            // the turn id from our own active-turns map.
            let session_id = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let Some(content) = params.get("content").and_then(|v| v.as_str()) else {
                return err("INVALID_ARGUMENT", "content required");
            };
            let expected_turn_id = params
                .get("expectedTurnId")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| state.active_turns.lock().unwrap().get(&session_id).cloned());
            let Some(expected_turn_id) = expected_turn_id else {
                return err("TURN_NOT_FOUND", "no active turn to steer");
            };
            let steer_params = json!({
                "sessionId": session_id,
                "expectedTurnId": expected_turn_id,
                "content": content,
                "attachments": params.get("attachments").cloned().unwrap_or(json!([])),
                "message": {
                    "id": uuid::Uuid::new_v4().to_string(),
                    "role": "user",
                    "content": content,
                    "createdAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                    "steering": true,
                },
            });
            envelope_result(
                "sidecar",
                state.sidecar.call("agent.steer", steer_params).await,
            )
        }
        "pi-desktop/agent/abort" => {
            let session_id = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            state
                .active_tools
                .lock()
                .unwrap()
                .retain(|(pending_session, _), _| pending_session != session_id);
            state.plugins.abort_extension_prompts(session_id);
            let turn_id = state.active_turns.lock().unwrap().remove(session_id);
            let mut abort_params = params.clone();
            if abort_params.get("turnId").is_none() {
                abort_params["turnId"] = turn_id.clone().map(|t| json!(t)).unwrap_or(Value::Null);
            }
            let sidecar_result = state.sidecar.call("agent.abort", abort_params).await;
            // End the durable turn even if the sidecar is already gone.
            if let Some(turn_id) = turn_id {
                // Drop the mutex guard before awaiting: holding a std MutexGuard
                // across an await makes the command future non-Send.
                let execution_id = state.active_executions.lock().unwrap().remove(&turn_id);
                if let Some(execution_id) = execution_id {
                    let _ = state
                        .host
                        .call(
                            "plans.finishExecution",
                            json!({ "executionId": execution_id, "status": "interrupted", "errorCode": "TURN_ABORTED" }),
                        )
                        .await;
                }
                let _ = state
                    .host
                    .call(
                        "session.endTurn",
                        json!({ "sessionId": session_id, "turnId": turn_id, "status": "aborted" }),
                    )
                    .await;
            }
            envelope_result("sidecar", sidecar_result)
        }
        "pi-desktop/agent/compact" => agent_compact(state, &params).await,
        "pi-desktop/prompt/enhance" => prompt_enhance(state, &params).await,
        "pi-desktop/session/summarizeTitle" => session_summarize_title(state, &params).await,
        "pi-desktop/agent/queue/push" => {
            let Some(session_id) = params.get("sessionId").and_then(|v| v.as_str()) else {
                return err("INVALID_ARGUMENT", "sessionId required");
            };
            crate::queue::push(&app, state, &params, session_id).await
        }
        "pi-desktop/agent/queue/list" => crate::queue::list(state, &params).await,
        "pi-desktop/agent/queue/remove" => {
            let Some(id) = params.get("turnId").and_then(|v| v.as_str()) else {
                return err("INVALID_ARGUMENT", "turnId required");
            };
            crate::queue::remove(&app, state, &params, id).await
        }
        "pi-desktop/agent/queue/prioritize" => {
            let Some(id) = params.get("turnId").and_then(|v| v.as_str()) else {
                return err("INVALID_ARGUMENT", "turnId required");
            };
            crate::queue::prioritize(&app, state, &params, id).await
        }
        "pi-desktop/tool/pendingPermissions" => {
            // The host owns the open permission queue (D374/D375). A client
            // that attached after `permissions.request` was emitted — the
            // remote-control channel is exactly such a client — reads it here.
            envelope_result("host", state.host.call("permissions.pending", params).await)
        }
        "pi-desktop/tool/resolvePermission" => {
            let result = envelope_result(
                "host",
                state.host.call("permissions.resolve", params.clone()).await,
            );
            // A decision can come from the window or from a remote card. Whichever
            // wins, the other view must drop its now-stale prompt: the host has
            // already consumed the request and a second click would fail with
            // NOT_FOUND. The renderer matches by requestId across all sessions,
            // so a missing sessionId in the payload is harmless.
            if result.get("ok").and_then(Value::as_bool) == Some(true) {
                if let Some(request_id) = params.get("requestId").and_then(Value::as_str) {
                    let _ = app.emit(
                        "pi-desktop/agent/event/message",
                        json!({
                            "sessionId": params.get("sessionId").cloned().unwrap_or(Value::Null),
                            "ts": now_ms(),
                            "event": { "type": "tool_permission_resolved", "requestId": request_id },
                        }),
                    );
                }
            }
            result
        }
        "pi-desktop/plans/resolve" => {
            envelope_result("host", state.host.call("plans.resolve", params).await)
        }

        // ── sessions (host + sidecar dispose bookkeeping) ─────────────────
        "pi-desktop/session/list"
        | "pi-desktop/session/create"
        | "pi-desktop/session/get"
        | "pi-desktop/session/open"
        | "pi-desktop/session/fork"
        | "pi-desktop/session/configure" => envelope_result(
            "host",
            state.host.call(&host_session_method(channel), params).await,
        ),
        "pi-desktop/session/delete" => {
            let session_id = params
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let result = state
                .host
                .call("session.delete", json!({ "id": session_id }))
                .await;
            if result.is_ok() {
                let _ = state
                    .sidecar
                    .call("agent.disposeSession", json!({ "sessionId": session_id }))
                    .await;
                state.active_turns.lock().unwrap().remove(&session_id);
            }
            state.plugins.abort_extension_prompts(&session_id);
            envelope_result("host", result)
        }
        "pi-desktop/session/moveProject"
        | "pi-desktop/session/replaceMessages"
        | "pi-desktop/session/activateRevision" => {
            let session_id = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let method = host_session_method(channel);
            let result = state.host.call(&method, params.clone()).await;
            if result.is_ok() {
                let _ = state
                    .sidecar
                    .call("agent.disposeSession", json!({ "sessionId": session_id }))
                    .await;
            }
            envelope_result("host", result)
        }
        "pi-desktop/session/openScratchPath" => {
            let scratch = envelope_result(
                "host",
                state
                    .host
                    .call("session.getScratchPath", params.clone())
                    .await,
            );
            let path = scratch
                .get("data")
                .and_then(|d| d.get("path"))
                .and_then(|p| p.as_str())
                .map(String::from);
            let Some(path) = path else { return scratch };
            match native::open_path(&app, &path) {
                Ok(()) => ok(json!({ "ok": true })),
                Err(e) => err("INTERNAL", e),
            }
        }
        "pi-desktop/session/importScan" => crate::importer::scan(state).await,
        "pi-desktop/session/importRun" => {
            let items = params.as_array().cloned().unwrap_or_default();
            crate::importer::run(state, &items).await
        }
        "pi-desktop/modelConfig/importScan" => crate::model_config_import::scan(state).await,
        "pi-desktop/modelConfig/importRun" => {
            let items = params.as_array().cloned().unwrap_or_default();
            crate::model_config_import::run(state, &items).await
        }

        // ── settings ──────────────────────────────────────────────────────
        "pi-desktop/settings/get" => {
            envelope_result("host", state.host.call("settings.get", params).await)
        }
        "pi-desktop/settings/set" => {
            let result = state.host.call("settings.set", params.clone()).await;
            if result.is_ok() {
                // Re-configure the sidecar like the Electron main does.
                let _ = configure_sidecar(state).await;
            }
            envelope_result("host", result)
        }
        "pi-desktop/network/testProxy" => {
            let params = params.clone();
            tauri::async_runtime::spawn_blocking(move || crate::proxy_test::run(&params))
                .await
                .unwrap_or_else(|error| err("INTERNAL", error.to_string()))
        }
        "pi-desktop/commandShell/list" => {
            let result = state.host.call("commandShells.list", params).await;
            match &result {
                Ok(catalog) => {
                    let available = catalog
                        .get("effective")
                        .and_then(|e| e.get("available"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if available {
                        envelope_result("host", result)
                    } else {
                        err("SHELL_NOT_FOUND", "No available command shell")
                    }
                }
                Err(_) => envelope_result("host", result),
            }
        }

        // ── instructions ──────────────────────────────────────────────────
        "pi-desktop/agent/instructions/get" => {
            // Contract: `{global: AgentInstructionFile, project?: …}`.
            let project_path = arg_str(&params, &["projectPath"]).map(String::from);
            let global_path = instruction_path(state, "global", &params).1;
            let global_content = fsops::read_if_exists(&global_path);
            let global_exists = global_content.is_some();
            let global = json!({
                "scope": "global",
                "path": global_path.to_string_lossy(),
                "content": global_content.unwrap_or_default(),
                "exists": global_exists,
            });
            let mut payload = json!({ "global": global });
            if let Some(project_path) = project_path {
                let project_file = PathBuf::from(&project_path).join("AGENTS.md");
                let project_content = fsops::read_if_exists(&project_file);
                let project_exists = project_content.is_some();
                payload["project"] = json!({
                    "scope": "project",
                    "path": project_file.to_string_lossy(),
                    "content": project_content.unwrap_or_default(),
                    "exists": project_exists,
                });
            }
            ok(payload)
        }
        "pi-desktop/agent/instructions/save" => {
            let scope = params
                .get("scope")
                .and_then(|v| v.as_str())
                .unwrap_or("global");
            let content = params.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let (scope, path) = instruction_path(state, scope, &params);
            match fsops::write_file(&path, content) {
                Ok(()) => ok(json!({
                    "file": {
                        "scope": scope,
                        "path": path.to_string_lossy(),
                        "content": content,
                        "exists": true,
                    }
                })),
                Err(e) => err("INTERNAL", e),
            }
        }

        // ── providers ─────────────────────────────────────────────────────
        "pi-desktop/providers/list" => {
            let rows = state
                .host
                .call("providers.list", json!({ "includeDisabled": true }))
                .await;
            match rows {
                Ok(mut payload) => {
                    // Inject secrets so the settings page can show which rows
                    // carry keys (the Electron main does the same).
                    if let Some(providers) =
                        payload.get_mut("providers").and_then(|p| p.as_array_mut())
                    {
                        for row in providers.iter_mut() {
                            let id = row.get("id").and_then(|v| v.as_str()).unwrap_or("");
                            if let Ok(secret) = state
                                .host
                                .call("providers.getSecret", json!({ "id": id }))
                                .await
                            {
                                if let Some(value) = secret.get("value").and_then(|v| v.as_str()) {
                                    row["apiKey"] = json!(value);
                                }
                            }
                        }
                    }
                    ok(payload)
                }
                Err(e) => envelope::envelope_from_rpc("host", &e),
            }
        }
        "pi-desktop/providers/listModels" => crate::models::list_models(state, &params).await,
        "pi-desktop/providers/refreshModelCatalog" => {
            let catalog = state.catalog.clone();
            let refreshed = tauri::async_runtime::spawn_blocking(move || catalog.refresh())
                .await
                .unwrap_or(false);
            ok(json!({
                "refreshed": refreshed,
                "status": state.catalog.status(),
            }))
        }
        "pi-desktop/providers/modelCatalogStatus" => {
            ok(json!({ "status": state.catalog.status() }))
        }
        "pi-desktop/providers/oauth/vendors" => ok(state.oauth.vendors(state).await),
        "pi-desktop/providers/oauth/start" => {
            let Some(vendor_id) = params.get("vendorId").and_then(|v| v.as_str()) else {
                return err("INVALID_ARGUMENT", "vendorId required");
            };
            match state
                .oauth
                .start(state.clone(), app.clone(), vendor_id)
                .await
            {
                Ok(login_id) => ok(json!({ "loginId": login_id })),
                Err(message) => err("INVALID_ARGUMENT", message),
            }
        }
        "pi-desktop/providers/oauth/respond" => {
            let (Some(login_id), Some(prompt_id)) = (
                params.get("loginId").and_then(|v| v.as_str()),
                params.get("promptId").and_then(|v| v.as_str()),
            ) else {
                return err("INVALID_ARGUMENT", "loginId and promptId required");
            };
            let value = params
                .get("value")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            ok(json!({
                "ok": state.oauth.respond(login_id, prompt_id, value),
            }))
        }
        "pi-desktop/providers/oauth/cancel" => {
            let Some(login_id) = params.get("loginId").and_then(|v| v.as_str()) else {
                return err("INVALID_ARGUMENT", "loginId required");
            };
            ok(json!({ "ok": state.oauth.cancel(login_id) }))
        }
        "pi-desktop/providers/oauth/delete" => {
            let Some(provider_id) = params.get("providerId").and_then(|v| v.as_str()) else {
                return err("INVALID_ARGUMENT", "providerId required");
            };
            match crate::oauth::OAuthManager::delete_account(state, provider_id).await {
                Ok(result) => {
                    ok(json!({ "ok": result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) }))
                }
                Err(message) => err("INTERNAL", message),
            }
        }

        // ── projects ──────────────────────────────────────────────────────
        "pi-desktop/project/get" => {
            // workspace.get + git branch enrichment (withGitBranch port: the
            // branch comes from .git/HEAD, absent when not a work tree).
            let result = state.host.call("workspace.get", json!({})).await;
            match result {
                Ok(mut payload) => {
                    if let Some(branch) = payload
                        .pointer("/workspace/path")
                        .and_then(|p| p.as_str())
                        .and_then(|path| {
                            let head =
                                std::fs::read_to_string(Path::new(path).join(".git").join("HEAD"))
                                    .ok()?;
                            head.lines().find_map(|line| {
                                line.strip_prefix("ref: refs/heads/")
                                    .map(|b| b.trim().to_string())
                            })
                        })
                    {
                        payload["workspace"]["branch"] = json!(branch);
                    }
                    ok(payload)
                }
                Err(e) => envelope::envelope_from_rpc("host", &e),
            }
        }
        "pi-desktop/project/set" => {
            let result = state.host.call("workspace.set", params).await;
            if result.is_ok() {
                let project = workspace_root(state).await;
                crate::plugin_host::sync(state, project.as_deref()).await;
            }
            envelope_result("host", result)
        }
        "pi-desktop/project/open" => {
            let Some(path) = native::pick_folder(&app) else {
                return ok(json!({ "workspace": Value::Null, "canceled": true }));
            };
            let path = path.to_string_lossy().to_string();
            let result = state
                .host
                .call("workspace.set", json!({ "path": path }))
                .await;
            match result {
                Ok(mut payload) => {
                    if let Some(branch) = payload
                        .pointer("/workspace/path")
                        .and_then(|p| p.as_str())
                        .and_then(|p| {
                            std::fs::read_to_string(Path::new(p).join(".git").join("HEAD")).ok()
                        })
                        .and_then(|head| {
                            head.lines().find_map(|line| {
                                line.strip_prefix("ref: refs/heads/")
                                    .map(|b| b.trim().to_string())
                            })
                        })
                    {
                        payload["workspace"]["branch"] = json!(branch);
                    }
                    let project = workspace_root(state).await;
                    crate::plugin_host::sync(state, project.as_deref()).await;
                    ok(json!({ "workspace": payload["workspace"].clone(), "canceled": false }))
                }
                Err(e) => envelope::envelope_from_rpc("host", &e),
            }
        }
        "pi-desktop/project/pickFolders" => {
            let paths: Vec<String> = native::pick_folders(&app)
                .into_iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect();
            if paths.is_empty() {
                // Renderer contract (ProjectCreateDialog): `{folders, canceled}`.
                ok(json!({ "folders": [], "canceled": true }))
            } else {
                ok(json!({ "folders": paths, "canceled": false }))
            }
        }
        "pi-desktop/project/openFolder" => {
            let path = arg_str(&params, &["path"]).unwrap_or("").to_string();
            if path.is_empty() {
                return err("INVALID_ARGUMENT", "path required");
            }
            match native::open_path(&app, &path) {
                Ok(()) => ok(json!({ "ok": true })),
                Err(e) => err("INTERNAL", e),
            }
        }
        "pi-desktop/project/clone" => {
            // The renderer already validated the URL for its own UI; the shell
            // repeats that check because a renderer is never trusted, then asks
            // for the folder and lets git run without a shell or auth prompt.
            let url = arg_str(&params, &["url"]).unwrap_or("").trim().to_string();
            if url.is_empty() {
                return err("INVALID_ARGUMENT", "url required");
            }
            let Some(parent) = native::pick_folders(&app).into_iter().next() else {
                return ok(json!({ "workspace": Value::Null, "canceled": true }));
            };
            match crate::git_clone::clone(&url, &parent).await {
                Ok(path) => {
                    let name = path
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_default();
                    ok(json!({
                        "canceled": false,
                        "workspace": {
                            "path": path.to_string_lossy(),
                            "name": name,
                            "branch": crate::git_clone::head_branch(&path),
                        }
                    }))
                }
                Err(crate::git_clone::CloneFailure::InvalidUrl(message)) => {
                    err("INVALID_ARGUMENT", message)
                }
                Err(crate::git_clone::CloneFailure::Git(message)) => err("INTERNAL", message),
            }
        }

        // ── workspace file panel ──────────────────────────────────────────
        "pi-desktop/workspace/diff" => {
            let root = workspace_root(state).await;
            match root {
                None => ok(json!({ "repo": false, "clean": true, "files": [] })),
                Some(root) => ok(fsops::workspace_diff(Path::new(&root))),
            }
        }
        "pi-desktop/fs/list" => {
            let Some(root) = workspace_root(state).await else {
                return err("INVALID_ARGUMENT", "workspace required");
            };
            let rel = params.get("path").and_then(|v| v.as_str()).unwrap_or("");
            match fsops::list_dir(Path::new(&root), rel) {
                Ok(entries) => ok(entries),
                Err(e) => err("INVALID_ARGUMENT", e),
            }
        }
        "pi-desktop/fs/read" => {
            let (root, extra) = readable_roots(state).await;
            let path = params.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if root.is_none()
                && !Path::new(path).is_absolute()
                && !fsops::is_attachment_blob_ref(path)
            {
                return err("INVALID_ARGUMENT", "workspace required");
            }
            fsops::read_openable(path, root.as_deref(), &extra)
        }
        "pi-desktop/fs/readImageDataUrl" => {
            let (root, extra) = readable_roots(state).await;
            let raw = params.get("ref").and_then(|v| v.as_str()).unwrap_or("");
            fsops::read_image_data_url(raw, root.as_deref(), &extra)
        }
        "pi-desktop/fs/reveal" => {
            let (root, extra) = readable_roots(state).await;
            let path = params.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let Some(target) = fsops::resolve_openable_path(path, root.as_deref(), &extra) else {
                return err("INVALID_ARGUMENT", "path outside allowed roots");
            };
            match native::reveal_item(&app, &target.to_string_lossy()) {
                Ok(()) => ok(json!({ "ok": true })),
                Err(e) => err("INTERNAL", e),
            }
        }
        "pi-desktop/fs/open" => {
            let (root, extra) = readable_roots(state).await;
            let path = params.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let Some(target) = fsops::resolve_openable_path(path, root.as_deref(), &extra) else {
                return err("INVALID_ARGUMENT", "path is not openable");
            };
            match native::open_path(&app, &target.to_string_lossy()) {
                Ok(()) => ok(json!({ "ok": true })),
                Err(e) => err("INTERNAL", e),
            }
        }
        "pi-desktop/fs/index" => {
            let root = workspace_root(state).await;
            match root {
                None => ok(json!({ "entries": [], "truncated": false })),
                Some(root) => ok(fsops::workspace_file_index(Path::new(&root))),
            }
        }

        // ── pulls (gh pr list port is phase 2) ───────────────────────────
        "pi-desktop/pulls/list" => {
            let root = workspace_root(state).await.map(PathBuf::from);
            ok(crate::pulls::list(root).await)
        }

        // ── window / menu / chrome ────────────────────────────────────────
        "pi-desktop/window/control" => native::window_control(&app, &params),
        "pi-desktop/window/setWorkPanelReservation" | "pi-desktop/window/setWorkPanelChatWidth" => {
            ok(json!({}))
        }
        "pi-desktop/window/setBackgroundColor" => {
            let theme = arg_str(&params, &["theme"]).unwrap_or("dark").to_string();
            // macOS draws the sidebar through native `sidebar` vibrancy, so
            // the window appearance -- not a background colour -- is what has
            // to follow the renderer palette. Left on the system appearance,
            // a dark palette under a Light system shows the light material
            // through the sidebar tint as a washed-out overlay. `themeSource`
            // keeps "follow the system" genuinely automatic.
            #[cfg(target_os = "macos")]
            {
                let appearance = match arg_str(&params, &["themeSource"]) {
                    Some("light") => Some(tauri::Theme::Light),
                    Some("dark") => Some(tauri::Theme::Dark),
                    _ => None,
                };
                if let Some(window) = app.get_webview_window("main") {
                    if let Err(error) = window.set_theme(appearance) {
                        log::warn!("window appearance switch failed: {error}");
                    }
                }
            }
            ok(json!({ "applied": false, "theme": theme }))
        }
        "pi-desktop/window/closeBehavior/get" => ok(json!({
            "behavior": state.close_behavior.lock().unwrap().clone(),
            "supported": true,
        })),
        "pi-desktop/window/closeBehavior/set" => {
            let behavior = params
                .get("behavior")
                .and_then(|v| v.as_str())
                .unwrap_or("quit")
                .to_string();
            *state.close_behavior.lock().unwrap() = behavior.clone();
            ok(json!({ "behavior": behavior }))
        }
        "pi-desktop/menu/rendererReady" => ok(json!({})),
        "pi-desktop/menu/nativeAction" => {
            let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("");
            native::native_menu_action(&app, action)
        }
        "pi-desktop/log/openFolder" => {
            let logs = state.data_dir.join("logs");
            let _ = std::fs::create_dir_all(&logs);
            match native::open_path(&app, &logs.to_string_lossy()) {
                Ok(()) => ok(json!({ "ok": true })),
                Err(e) => err("INTERNAL", e),
            }
        }
        "pi-desktop/devtools/toggle" => {
            native::toggle_devtools(&app);
            ok(json!({}))
        }

        // ── skills / subagents extras ─────────────────────────────────────
        "pi-desktop/skill/import" => {
            let mut merged = params.clone();
            if merged.get("path").and_then(|v| v.as_str()).is_none() {
                let Some(path) = native::pick_file(&app, "Markdown", &["md"]) else {
                    return ok(json!({ "path": Value::Null }));
                };
                merged["path"] = json!(path.to_string_lossy());
            }
            envelope_result("host", state.host.call("skills.import", merged).await)
        }
        "pi-desktop/skill/reveal" | "pi-desktop/subagent/reveal" => {
            let read = state
                .host
                .call(
                    if channel.starts_with("pi-desktop/skill") {
                        "skills.read"
                    } else {
                        "agents.read"
                    },
                    params.clone(),
                )
                .await;
            let path = read.as_ref().ok().and_then(|value| {
                value
                    .pointer("/skill/path")
                    .or_else(|| value.pointer("/subagent/path"))
                    .and_then(|p| p.as_str())
                    .map(String::from)
            });
            let Some(path) = path else {
                return envelope_result("host", read);
            };
            match native::reveal_item(&app, &path) {
                Ok(()) => ok(json!({ "ok": true })),
                Err(e) => err("INTERNAL", e),
            }
        }
        "pi-desktop/subagent/catalog" => {
            let project_path = workspace_root(state).await;
            let catalog = crate::subagents::load_catalog(state, project_path.as_deref()).await;
            ok(json!({
                "subagents": catalog.definitions,
                "diagnostics": catalog.diagnostics,
                "projectPath": project_path,
            }))
        }

        // ── plugin surface (metadata + Node runtime) ──────────────────────
        "pi-desktop/plugin/reload" => {
            let id = arg_str(&params, &["id", "pluginId"])
                .unwrap_or("")
                .to_string();
            if id.is_empty() {
                return err("INVALID_ARGUMENT", "plugin id required");
            }
            let project = workspace_root(state).await;
            match state.plugins.reload(&id, project.as_deref()).await {
                Ok(()) => ok(json!({ "ok": true })),
                Err(error) => err(&error.code, error.message),
            }
        }
        "pi-desktop/plugin/loadDev" => {
            // Electron main owned this picker; the renderer sends no path. Ask
            // for the plugin directory here, then load it through host-core and
            // reconcile the Node runtime so the plugin is live immediately.
            let Some(path) = native::pick_folders(&app).into_iter().next() else {
                return ok(json!({ "canceled": true }));
            };
            let result = state
                .host
                .call("plugins.loadDev", json!({ "path": path.to_string_lossy() }))
                .await;
            if result.is_ok() {
                state.plugins.clear_failures();
                let project = workspace_root(state).await;
                crate::plugin_host::sync(state, project.as_deref()).await;
            }
            envelope_result("host", result)
        }
        "pi-desktop/plugin/settings/get" => plugins::settings_get(state, &params).await,
        "pi-desktop/plugin/settings/set" => plugins::settings_set(state, &params).await,
        "pi-desktop/plugin/themes" => plugins::themes(state).await,
        "pi-desktop/plugin/services" => plugins::services(state).await,
        "pi-desktop/plugin/views" => plugins::views(state).await,
        "pi-desktop/plugin/openPanel" => plugins::open_panel(state, &params).await,
        "pi-desktop/plugin/view/open" => plugins::view_open(state, &params).await,
        "pi-desktop/plugin/bridge/invoke" => {
            crate::plugin_bridge::invoke(&app, state, &params).await
        }
        // The view is a renderer iframe now: the host has no native overlay to
        // move, so bounds/visibility only exist for contract compatibility.
        "pi-desktop/plugin/view/close"
        | "pi-desktop/plugin/view/setBounds"
        | "pi-desktop/plugin/view/setVisible" => ok(json!({ "ok": true })),
        "pi-desktop/plugin/createFromTemplate" => {
            // Electron main called the devkit scaffold after picking a folder;
            // the shell owns both halves now (spec 07-plugins/10 §2).
            let template = params
                .get("template")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if template.is_empty() {
                return err("INVALID_ARGUMENT", "template required");
            }
            let Some(dir) = native::pick_folders(&app).into_iter().next() else {
                return ok(json!({ "canceled": true }));
            };
            let scaffold = tauri::async_runtime::spawn_blocking(move || {
                crate::plugin_scaffold::scaffold(&dir, &template)
            })
            .await;
            let scaffold = match scaffold {
                Ok(Ok(scaffold)) => scaffold,
                Ok(Err(message)) => return err("INVALID_ARGUMENT", message),
                Err(error) => return err("INTERNAL", error.to_string()),
            };
            let result = state
                .host
                .call(
                    "plugins.loadDev",
                    json!({ "path": scaffold.dir.to_string_lossy() }),
                )
                .await;
            if let Err(error) = &result {
                // The files stay on disk; the toast names why they do not load.
                return envelope::envelope_from_rpc("host", error);
            }
            state.plugins.clear_failures();
            let project = workspace_root(state).await;
            crate::plugin_host::sync(state, project.as_deref()).await;
            ok(json!({
                "canceled": false,
                "id": scaffold.id,
                "name": scaffold.name,
                "dir": scaffold.dir.to_string_lossy(),
                "files": scaffold.files,
            }))
        }
        "pi-desktop/plugin/importExtension" => {
            // The renderer already asked for the trust confirmation; the shell
            // only owns the path picker (D344), then copies the package under
            // `plugins/imported` and generates its manifest (spec 16 §3.2).
            let wants_folder = app
                .dialog()
                .message("导入 pi 扩展或技能包")
                .title("导入 pi 扩展")
                .kind(MessageDialogKind::Info)
                .buttons(MessageDialogButtons::OkCancelCustom(
                    "选择文件夹".to_string(),
                    "选择文件".to_string(),
                ))
                .blocking_show();
            let source = if wants_folder {
                native::pick_folders(&app).into_iter().next()
            } else {
                app.dialog()
                    .file()
                    .blocking_pick_file()
                    .and_then(|path| path.into_path().ok())
            };
            let Some(source) = source else {
                return ok(json!({ "canceled": true }));
            };
            let data_dir = state.data_dir.clone();
            let imported = tauri::async_runtime::spawn_blocking(move || {
                crate::extension_import::import(&source, &data_dir)
            })
            .await;
            let imported = match imported {
                Ok(Ok(imported)) => imported,
                Ok(Err(message)) => return err("INVALID_ARGUMENT", message),
                Err(error) => return err("INTERNAL", error.to_string()),
            };
            let dependencies = crate::extension_import::install_dependencies(
                &imported.path,
                &imported.dependencies,
            )
            .await;
            // A failed install never blocks the import; the renderer reports it.
            let result = state
                .host
                .call(
                    "plugins.loadDev",
                    json!({ "path": imported.path.to_string_lossy() }),
                )
                .await;
            if let Err(error) = &result {
                return envelope::envelope_from_rpc("host", error);
            }
            state.plugins.clear_failures();
            let project = workspace_root(state).await;
            crate::plugin_host::sync(state, project.as_deref()).await;
            ok(json!({
                "canceled": false,
                "id": imported.id,
                "path": imported.path.to_string_lossy(),
                "entries": imported.entries,
                "dependencies": dependencies,
            }))
        }
        "pi-desktop/plugin/list" => match state.host.call("plugins.list", params).await {
            Ok(mut value) => {
                state.plugins.annotate_plugin_rows(&mut value);
                ok(value)
            }
            Err(error) => envelope::envelope_from_rpc("host", &error),
        },
        "pi-desktop/extensions/commands/run" => extension_command_run(state, &params).await,
        "pi-desktop/extensions/ui/respond" => {
            let prompt_id = arg_str(&params, &["promptId"]).unwrap_or("");
            if prompt_id.is_empty() {
                return err("INVALID_ARGUMENT", "promptId required");
            }
            // `value` is absent when the owner dismissed the dialog.
            match state
                .plugins
                .respond_extension_prompt(prompt_id, params.get("value").cloned())
            {
                Ok(value) => ok(value),
                Err(error) => err(&error.code, error.message),
            }
        }
        "pi-desktop/pluginLauncher/toggle" | "pi-desktop/pluginLauncher/dismiss" => {
            ok(json!({ "visible": false }))
        }

        // ── composer attachments (pick → token → import, ported) ─────────
        "pi-desktop/composer/pickFiles" | "pi-desktop/composer/pickPhotos" => {
            let mut dialog = app.dialog().file();
            if channel.ends_with("pickPhotos") {
                dialog = dialog.add_filter(
                    "Images",
                    &["png", "jpg", "jpeg", "gif", "webp", "heic", "tif", "tiff"],
                );
            }
            // ADR 0181: the composer picker offers regular files only. A folder
            // picker here made the dialog refuse every file selection.
            let picked: Vec<String> = dialog
                .blocking_pick_files()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|p| p.into_path().ok())
                .map(|p| p.to_string_lossy().to_string())
                .collect();
            if picked.is_empty() {
                return ok(json!({ "token": Value::Null, "canceled": true }));
            }
            let token = uuid::Uuid::new_v4().to_string();
            state
                .composer_tokens
                .lock()
                .unwrap()
                .insert(token.clone(), picked);
            ok(json!({ "token": token, "canceled": false }))
        }
        "pi-desktop/composer/importFiles" => {
            let session_id = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let token = params
                .get("token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if session_id.is_empty() {
                return err("INVALID_ARGUMENT", "session required");
            }
            let paths = state
                .composer_tokens
                .lock()
                .unwrap()
                .remove(&token)
                .unwrap_or_default();
            match fsops::import_composer_files(&state.data_dir, &session_id, &paths) {
                Ok(files) => ok(json!({ "files": files })),
                Err(e) => err("INVALID_ARGUMENT", e),
            }
        }
        "pi-desktop/composer/pasteFiles" => {
            let session_id = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if session_id.is_empty() {
                return err("INVALID_ARGUMENT", "session required");
            }
            let session = state
                .host
                .call("session.get", json!({ "id": session_id }))
                .await;
            match &session {
                Ok(s) if s.pointer("/session").map(|v| !v.is_null()).unwrap_or(false) => {}
                Ok(_) => return err("NOT_FOUND", "session not found"),
                Err(e) => return envelope::envelope_from_rpc("host", e),
            }
            let files = params
                .get("files")
                .and_then(|f| f.as_array())
                .cloned()
                .unwrap_or_default();
            match fsops::save_composer_paste_files(&state.data_dir, &session_id, &files) {
                Ok(files) => ok(json!({ "files": files })),
                Err(e) => err("INVALID_ARGUMENT", e),
            }
        }
        "pi-desktop/clipboard/recordPaste" => {
            let text = params.get("text").and_then(|v| v.as_str()).unwrap_or("");
            state.plugins.record_clipboard_text(text);
            ok(json!({ "ok": true }))
        }
        "pi-desktop/composer/commands" => {
            // Builtin aliases, slash templates, plugin commands, and active
            // skills (spec 04-ux/04 §7–§8). Returning plugin rows alone left
            // the "/" menu empty on installs without plugins.
            let project = workspace_root(state).await;
            ok(json!({
                "commands": crate::composer::commands(state, project.as_deref()).await,
            }))
        }
        "pi-desktop/commandPalette/search" => {
            let query = params.as_str().unwrap_or("");
            ok(json!({ "commands": state.plugins.search_commands(query) }))
        }
        "pi-desktop/commandPalette/execute" => {
            let command_id = params.as_str().unwrap_or("");
            if !command_id.starts_with("plugin.") {
                return err("COMMAND_NOT_FOUND", format!("unknown command {command_id}"));
            }
            match state.plugins.run_command(command_id).await {
                Ok(()) => ok(json!({})),
                Err(error) => err(&error.code, error.message),
            }
        }

        // ── browser pane (host-owned child webview guest) ─────────────────
        "pi-desktop/browser/navigate" => {
            browser_result(state.browser.navigate(state, &app, &params).await)
        }
        "pi-desktop/browser/action" => {
            let action = arg_str(&params, &["action"]).unwrap_or("");
            browser_result(state.browser.action(&app, action))
        }
        "pi-desktop/browser/getState" => ok(state.browser.state().unwrap_or(Value::Null)),
        "pi-desktop/browser/setBounds" => {
            let mut bounds = params.clone();
            if let Some(view) = params.get("view").cloned() {
                bounds["view"] = view;
            }
            browser_result(state.browser.set_bounds(&app, &bounds))
        }
        "pi-desktop/browser/setVisible" => {
            let visible = params
                .as_bool()
                .or_else(|| params.get("visible").and_then(Value::as_bool))
                .unwrap_or(true);
            browser_result(state.browser.set_visible(visible))
        }
        "pi-desktop/browser/close" => {
            state.browser.close();
            ok(json!({ "ok": true }))
        }
        "pi-desktop/browser/openExternal" => {
            let url = params.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if url.is_empty() {
                return ok(json!({ "ok": true }));
            }
            match native::open_url(&app, url) {
                Ok(()) => ok(json!({ "ok": true })),
                Err(e) => err("INTERNAL", e),
            }
        }

        // ── 选股神器 (hithink-finance) ─────────────────────────────────────
        "pi-desktop/stocks/status" => ok(state.stocks.status()),
        "pi-desktop/stocks/apiKey/set" => {
            crate::stocks::stocks_result(crate::stocks::set_key(state, &params).await)
        }
        "pi-desktop/stocks/overview" => {
            crate::stocks::stocks_result(crate::stocks::overview(state, &params).await)
        }
        "pi-desktop/stocks/plan" => crate::stocks::stocks_result(crate::stocks::plan(state).await),
        "pi-desktop/stocks/watch/list" => {
            crate::stocks::stocks_result(crate::stocks::watch_list(state).await)
        }
        "pi-desktop/stocks/watch/add" => {
            crate::stocks::stocks_result(crate::stocks::watch_add_rpc(state, &params).await)
        }
        "pi-desktop/stocks/watch/remove" => {
            crate::stocks::stocks_result(crate::stocks::watch_remove(state, &params))
        }

        // ── staff residents ───────────────────────────────────────────────
        "pi-desktop/staff/scheduler/status" => ok(state.staff.status()),
        "pi-desktop/staff/watch/status" => ok(crate::staff_watch::status(state)),
        "pi-desktop/staff/watch/run" => ok(crate::staff_watch::run_now(&app, state).await),
        "pi-desktop/staff/employees/runNow" => {
            let employee_id = arg_str(&params, &["employeeId", "id"])
                .map(str::to_string)
                .or_else(|| params.as_str().map(str::to_string))
                .unwrap_or_default();
            if employee_id.is_empty() {
                return err("INVALID_ARGUMENT", "employeeId required");
            }
            crate::staff_driver::run_now(&app, state, &employee_id).await
        }
        "pi-desktop/staff/sites/list" => ok(crate::staff_sites::list(&app, state).await),
        "pi-desktop/staff/sites/signIn" => {
            let name = site_channel(&params);
            let result = crate::staff_sites::sign_in(&app, &name).await;
            site_result(result)
        }
        "pi-desktop/staff/sites/signOut" => {
            let name = site_channel(&params);
            let result = crate::staff_sites::sign_out(&app, state, &name).await;
            site_result(result)
        }
        "pi-desktop/staff/sites/update" => {
            let name = site_channel(&params);
            let result = crate::staff_sites::update(&app, state, &name, &params).await;
            site_result(result)
        }
        "pi-desktop/staff/sites/collect" => {
            let name = site_channel(&params);
            let result = crate::staff_sites::collect(&app, state, &name).await;
            site_result(result)
        }
        "pi-desktop/staff/data/export" => {
            // Ledger export through a native save dialog (ported from the
            // Electron staff-ipc handler): the renderer only suggests a NAME.
            let body = params.get("body").and_then(|v| v.as_str()).unwrap_or("");
            if body.is_empty() {
                return err("INVALID_ARGUMENT", "nothing to export");
            }
            let suggested = params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("pi-agent-data.json")
                .chars()
                .map(|c| match c {
                    '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                    other => other,
                })
                .collect::<String>()
                .trim()
                .to_string();
            let file_name = if suggested.is_empty() {
                "pi-agent-data.json".to_string()
            } else {
                suggested
            };
            let Some(target) = native::save_file(&app, &file_name) else {
                return ok(json!({ "saved": false }));
            };
            match std::fs::write(&target, body) {
                Ok(()) => ok(json!({
                    "saved": true,
                    "path": target.to_string_lossy(),
                    "bytes": body.len(),
                })),
                Err(e) => err("INTERNAL", e.to_string()),
            }
        }
        "pi-desktop/session/export" => {
            // CLI /export parity. host-core's own transcript JSONL is not
            // readable by the CLI, so translate the `session.get` detail into
            // pigo's header + entry-wire lines before the save dialog.
            let Some(session_id) = params.get("sessionId").and_then(Value::as_str) else {
                return err("INVALID_ARGUMENT", "sessionId required");
            };
            let detail = match state
                .host
                .call("session.get", json!({ "id": session_id }))
                .await
            {
                Ok(detail) => detail,
                Err(e) => return envelope::envelope_from_rpc("host", &e),
            };
            let session = detail.get("session").cloned().unwrap_or(Value::Null);
            let (body, messages) = match crate::session_export::export_body(&session) {
                Ok(result) => result,
                Err(message) => return err("INVALID_ARGUMENT", message),
            };
            let file_name = crate::session_export::suggested_file_name(&session);
            let Some(target) = native::save_file(&app, &file_name) else {
                return ok(json!({ "saved": false }));
            };
            match std::fs::write(&target, &body) {
                Ok(()) => ok(json!({
                    "saved": true,
                    "path": target.to_string_lossy(),
                    "bytes": body.len(),
                    "messages": messages,
                })),
                Err(e) => err("INTERNAL", e.to_string()),
            }
        }

        // ── desktop MCP surfaces and market search ────────────────────────
        "pi-desktop/mcp/test" => crate::mcp::test(state, &params).await,
        "pi-desktop/mcp/import" => crate::mcp::import(state, &params).await,
        "pi-desktop/mcp/market/search" => crate::market::mcp_search(state, &params).await,
        "pi-desktop/skill/market/search" => crate::market::skill_search(state, &params).await,
        "pi-desktop/skill/market/fetch" => match crate::market::skill_fetch(&params).await {
            Ok(value) => ok(value),
            Err(message) => err("INTERNAL", message),
        },

        _ => not_implemented(channel),
    }
}

fn host_session_method(channel: &str) -> String {
    match channel {
        "pi-desktop/session/list" => "session.list",
        "pi-desktop/session/create" => "session.create",
        "pi-desktop/session/get" => "session.get",
        "pi-desktop/session/open" => "session.get",
        "pi-desktop/session/fork" => "session.fork",
        "pi-desktop/session/configure" => "session.configure",
        "pi-desktop/session/moveProject" => "session.moveProject",
        "pi-desktop/session/replaceMessages" => "session.replaceMessages",
        "pi-desktop/session/activateRevision" => "session.activateRevision",
        _ => "session.get",
    }
    .to_string()
}

async fn workspace_root(state: &std::sync::Arc<AppState>) -> Option<String> {
    let workspace = state.host.call("workspace.get", json!({})).await.ok()?;
    workspace
        .pointer("/workspace/path")
        .and_then(|p| p.as_str())
        .map(String::from)
}

/// `extensions/commands/run`: one registered extension command, in the active
/// session. The extension process owns the command; this side only proves the
/// session exists and hands it the launch working directory.
async fn extension_command_run(state: &std::sync::Arc<AppState>, params: &Value) -> Value {
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params.get("args").and_then(Value::as_str).unwrap_or("");
    if session_id.is_empty() {
        return err("INVALID_ARGUMENT", "sessionId required");
    }
    if name.is_empty() {
        return err("INVALID_ARGUMENT", "name required");
    }
    let session = match state
        .host
        .call(
            "session.get",
            json!({ "id": session_id, "messageLimit": 1 }),
        )
        .await
    {
        Ok(value) => value,
        Err(error) => return envelope::envelope_from_rpc("host", &error),
    };
    if session.get("session").map(Value::is_null).unwrap_or(true) {
        return err(
            "SESSION_NOT_FOUND",
            format!("session {session_id} not found"),
        );
    }
    let cwd = session
        .pointer("/session/projectPath")
        .and_then(Value::as_str)
        .map(str::to_string);
    let cwd = match cwd {
        Some(path) => Some(path),
        None => workspace_root(state).await,
    };
    match state
        .plugins
        .run_extension_command(session_id, name, args, cwd.as_deref())
        .await
    {
        Ok(_) => ok(json!({ "ok": true })),
        Err(error) => err(&error.code, error.message),
    }
}

async fn readable_roots(
    state: &std::sync::Arc<AppState>,
) -> (Option<std::path::PathBuf>, Vec<std::path::PathBuf>) {
    let root = workspace_root(state).await.map(std::path::PathBuf::from);
    let extra = vec![state.scratch_dir(), state.attachments_dir()];
    (root, extra)
}

fn instruction_path(
    _state: &std::sync::Arc<AppState>,
    scope: &str,
    params: &Value,
) -> (String, std::path::PathBuf) {
    if scope == "project" {
        let project = arg_str(params, &["projectPath", "path"]).unwrap_or("");
        if !project.is_empty() {
            return (
                "project".into(),
                std::path::PathBuf::from(project).join("AGENTS.md"),
            );
        }
    }
    (
        "global".into(),
        dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".pi")
            .join("agent")
            .join("AGENTS.md"),
    )
}

async fn configure_sidecar(state: &std::sync::Arc<AppState>) -> Result<(), RpcError> {
    state
        .sidecar
        .call(
            "sidecar.configure",
            json!({
                "hostBinary": state.host.binary.to_string_lossy(),
                "dataDir": state.data_dir.to_string_lossy(),
                "networkProxy": Value::Null,
            }),
        )
        .await
        .map(|_| ())
}

/// `agent/prompt` — the reduced Electron turn flow: resolve launch, begin a
/// durable turn, persist + echo the user message, then hand off to the
/// sidecar. Regenerate truncation is honored; queueing, plan dispatch and
/// slash/skill expansion are phase 2.
async fn agent_prompt(app: &AppHandle, state: &std::sync::Arc<AppState>, params: &Value) -> Value {
    let Some(session_id) = params.get("sessionId").and_then(|v| v.as_str()) else {
        return err("INVALID_ARGUMENT", "sessionId required");
    };
    let Some(content) = params.get("content").and_then(|v| v.as_str()) else {
        return err("INVALID_ARGUMENT", "content required");
    };
    let session_id = session_id.to_string();
    let content = content.to_string();

    let settings = match state.host.call("settings.get", json!({})).await {
        Ok(s) => s,
        Err(e) => return envelope::envelope_from_rpc("host", &e),
    };
    let mut session = match state
        .host
        .call(
            "session.get",
            json!({ "id": session_id, "messageLimit": 1 }),
        )
        .await
    {
        Ok(s) => s.get("session").cloned().unwrap_or(Value::Null),
        Err(e) => return envelope::envelope_from_rpc("host", &e),
    };

    // Regenerate / edit-resend: truncate the durable tail first.
    let truncate_from = params.get("truncateFromMessageId").and_then(|v| v.as_str());
    let truncate_before = params.get("truncateBefore").and_then(|v| v.as_bool());
    if truncate_from.is_some() || truncate_before == Some(true) {
        let _ = state
            .sidecar
            .call("agent.abort", json!({ "sessionId": session_id }))
            .await;
        let tracked_turn = state.active_turns.lock().unwrap().remove(&session_id);
        if let Some(turn_id) = tracked_turn {
            let _ = state
                .host
                .call(
                    "session.endTurn",
                    json!({ "sessionId": session_id, "turnId": turn_id, "status": "aborted" }),
                )
                .await;
            crate::collaboration::settle_async(app, state, Some(&turn_id));
        }
        let mut truncate_params = json!({ "sessionId": session_id });
        if let Some(from_id) = truncate_from {
            truncate_params["fromMessageId"] = json!(from_id);
        }
        if truncate_before == Some(true) {
            truncate_params["truncateBefore"] = json!(true);
        }
        if let Err(e) = state
            .host
            .call("session.truncateFrom", truncate_params)
            .await
        {
            return envelope::envelope_from_rpc("host", &e);
        }
        session = match state
            .host
            .call(
                "session.get",
                json!({ "id": session_id, "messageLimit": 1 }),
            )
            .await
        {
            Ok(s) => s.get("session").cloned().unwrap_or(Value::Null),
            Err(e) => return envelope::envelope_from_rpc("host", &e),
        };
    }

    let launch = match launch::resolve_launch(state, &session, &settings, params).await {
        Ok(l) => l,
        Err(envelope) => return envelope,
    };

    let begin = match state
        .host
        .call(
            "session.beginTurn",
            json!({
                "sessionId": session_id,
                "providerId": launch.provider_id,
                "modelId": launch.model_id,
                "sessionMessageId": params.get("sessionMessageId").cloned().unwrap_or(Value::Null),
            }),
        )
        .await
    {
        Ok(b) => b,
        Err(e) => return envelope::envelope_from_rpc("host", &e),
    };
    let turn_id = begin
        .get("turnId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // UiMessage is strict serde on the host side: `createdAt` is a required
    // ISO string — a `ts` number lands as INVALID_PARAMS.
    // Reuse the renderer's optimistic row id: the user row is echoed below, and
    // a fresh id would leave the optimistic bubble and the durable row as two
    // copies of the same message.
    let user_message_id = params
        .get("messageId")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let mut user_message = json!({
        "id": user_message_id,
        "role": "user",
        "content": content,
        "createdAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    });

    // Attachments: the persisted UiMessage carries MessageAttachment rows
    // (`ref`, never bytes); the sidecar payload carries `path` plus inlined
    // base64 for images within the allowed roots (Electron's
    // preparePromptAttachments, reduced: no blob re-hashing, no vision gate).
    let request_attachments = params
        .get("attachments")
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();
    let mut message_attachments: Vec<Value> = Vec::new();
    let mut sidecar_attachments: Vec<Value> = Vec::new();
    if !request_attachments.is_empty() {
        let (root, extra) = readable_roots(state).await;
        for attachment in &request_attachments {
            let path = attachment
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let name = attachment
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let kind = attachment
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("file");
            let mime = attachment
                .get("mimeType")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let size = attachment.get("size").and_then(|v| v.as_u64());
            let mut row = json!({ "kind": kind, "name": name, "ref": path });
            if !mime.is_empty() {
                row["mimeType"] = json!(mime);
            }
            if let Some(size) = size {
                row["size"] = json!(size);
            }
            message_attachments.push(row);

            let mut sidecar_row = attachment.clone();
            let is_image = kind == "image" || mime.starts_with("image/");
            let allowed = fsops::resolve_openable_path(path, root.as_deref(), &extra).is_some();
            if is_image && allowed {
                if let Ok(bytes) = std::fs::read(path) {
                    use base64::Engine as _;
                    sidecar_row["data"] =
                        json!(base64::engine::general_purpose::STANDARD.encode(bytes));
                }
            }
            sidecar_attachments.push(sidecar_row);
        }
        user_message["attachments"] = json!(message_attachments);
    }
    if let Err(e) = state
        .host
        .call(
            "session.appendMessage",
            json!({ "sessionId": session_id, "message": user_message.clone(), "turnId": turn_id }),
        )
        .await
    {
        return envelope::envelope_from_rpc("host", &e);
    }
    for event_type in ["message_start", "message_end"] {
        let _ = app.emit(
            "pi-desktop/agent/event/message",
            json!({
                "sessionId": session_id,
                "turnId": turn_id,
                "ts": now_ms(),
                "event": { "type": event_type, "message": user_message },
            }),
        );
    }

    let mut prompt = launch.sidecar_params.clone();
    prompt["turnId"] = json!(turn_id);
    prompt["content"] = json!(content);
    prompt["attachments"] = json!(sidecar_attachments);
    prompt["userMessageId"] = json!(user_message_id);
    if let Some(session_message_id) = params.get("sessionMessageId") {
        prompt["sessionMessage"] = session_message_id.clone();
    }

    let result = state.sidecar.call("agent.prompt", prompt).await;
    match &result {
        Ok(_) => {
            state
                .active_turns
                .lock()
                .unwrap()
                .insert(session_id, turn_id);
        }
        Err(_) => {
            let _ = state
                .host
                .call(
                    "session.endTurn",
                    json!({ "sessionId": session_id, "turnId": turn_id, "status": "error" }),
                )
                .await;
            crate::collaboration::settle_async(app, state, Some(&turn_id));
        }
    }
    envelope_result("sidecar", result)
}

/// `prompt/enhance` — rewrite one composer draft through a one-shot provider
/// call. A draft can be enhanced before any session exists.
async fn prompt_enhance(state: &std::sync::Arc<AppState>, params: &Value) -> Value {
    let Some(draft) = params.get("draft").and_then(|v| v.as_str()) else {
        return err("INVALID_ARGUMENT", "draft required");
    };
    let settings = match state.host.call("settings.get", json!({})).await {
        Ok(s) => s,
        Err(e) => return envelope::envelope_from_rpc("host", &e),
    };
    let session = match params.get("sessionId").and_then(|v| v.as_str()) {
        Some(session_id) => match state
            .host
            .call(
                "session.get",
                json!({ "id": session_id, "messageLimit": 1 }),
            )
            .await
        {
            Ok(s) => s.get("session").cloned().unwrap_or(Value::Null),
            Err(e) => return envelope::envelope_from_rpc("host", &e),
        },
        None => Value::Null,
    };
    let launch = match launch::resolve_launch(state, &session, &settings, params).await {
        Ok(l) => l,
        Err(envelope) => return envelope,
    };
    let mut prompt = launch.sidecar_params.clone();
    prompt["draft"] = json!(draft);
    envelope_result(
        "sidecar",
        state.sidecar.call("prompt.enhance", prompt).await,
    )
}

/// `session/summarizeTitle` — generate the short title shown in the session
/// list from the first user prompt and, when present, the assistant reply.
async fn session_summarize_title(state: &std::sync::Arc<AppState>, params: &Value) -> Value {
    let Some(session_id) = params.get("sessionId").and_then(|v| v.as_str()) else {
        return err("INVALID_ARGUMENT", "sessionId required");
    };
    let Some(user_prompt) = params.get("userPrompt").and_then(|v| v.as_str()) else {
        return err("INVALID_ARGUMENT", "userPrompt required");
    };
    let settings = match state.host.call("settings.get", json!({})).await {
        Ok(s) => s,
        Err(e) => return envelope::envelope_from_rpc("host", &e),
    };
    let session = match state
        .host
        .call(
            "session.get",
            json!({ "id": session_id, "messageLimit": 1 }),
        )
        .await
    {
        Ok(s) => s.get("session").cloned().unwrap_or(Value::Null),
        Err(e) => return envelope::envelope_from_rpc("host", &e),
    };
    let launch = match launch::resolve_launch(state, &session, &settings, params).await {
        Ok(l) => l,
        Err(envelope) => return envelope,
    };
    let mut prompt = launch.sidecar_params.clone();
    prompt["userPrompt"] = json!(user_prompt);
    if let Some(reply) = params.get("assistantReply").and_then(|v| v.as_str()) {
        prompt["assistantReply"] = json!(reply);
    }
    envelope_result(
        "sidecar",
        state.sidecar.call("session.summarizeTitle", prompt).await,
    )
}

/// `agent/btw` — answer one out-of-band /btw question without touching the
/// durable session. Only the launch binding is resolved; unlike agent/prompt
/// there is no `session.beginTurn`, no user row and no `session.appendMessage`.
/// The sidecar re-reads the transcript itself and streams scoped events that
/// the renderer shows in a temporary panel.
async fn agent_btw(state: &std::sync::Arc<AppState>, params: &Value) -> Value {
    let Some(session_id) = params.get("sessionId").and_then(|v| v.as_str()) else {
        return err("INVALID_ARGUMENT", "sessionId required");
    };
    let Some(question) = params.get("question").and_then(|v| v.as_str()) else {
        return err("INVALID_ARGUMENT", "question required");
    };
    let session_id = session_id.to_string();
    let settings = match state.host.call("settings.get", json!({})).await {
        Ok(s) => s,
        Err(e) => return envelope::envelope_from_rpc("host", &e),
    };
    let session = match state
        .host
        .call(
            "session.get",
            json!({ "id": session_id, "messageLimit": 1 }),
        )
        .await
    {
        Ok(s) => s.get("session").cloned().unwrap_or(Value::Null),
        Err(e) => return envelope::envelope_from_rpc("host", &e),
    };
    let launch = match launch::resolve_launch(state, &session, &settings, params).await {
        Ok(l) => l,
        Err(envelope) => return envelope,
    };
    let mut binding = launch.sidecar_params.clone();
    binding["sessionId"] = json!(session_id);
    // A side question is not a turn: leave the turn/content slots empty so the
    // sidecar never mistakes the binding for a main prompt.
    binding["turnId"] = json!("");
    binding["content"] = json!("");
    binding["userMessageId"] = json!("");
    let request = json!({
        "sessionId": session_id,
        "sideId": params.get("sideId").and_then(Value::as_str).unwrap_or(""),
        "question": question,
        "binding": binding,
    });
    envelope_result("sidecar", state.sidecar.call("agent.btw", request).await)
}

/// `agent/dream` — run one /dream memory-consolidation pass.
///
/// The pass takes minutes (several model calls), so it uses a longer deadline
/// than an ordinary sidecar call. It is not a turn: no host turn is opened, no
/// transcript row is appended, and the session's live AgentContext is untouched.
/// The sidecar answers once the pass finished, with the pigo Report verbatim.
async fn agent_dream(state: &std::sync::Arc<AppState>, params: &Value) -> Value {
    const DREAM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);
    let Some(session_id) = params.get("sessionId").and_then(|v| v.as_str()) else {
        return err("INVALID_ARGUMENT", "sessionId required");
    };
    if session_id.is_empty() {
        return err("INVALID_ARGUMENT", "sessionId required");
    }
    let session_id = session_id.to_string();
    let settings = match state.host.call("settings.get", json!({})).await {
        Ok(s) => s,
        Err(e) => return envelope::envelope_from_rpc("host", &e),
    };
    let session = match state
        .host
        .call(
            "session.get",
            json!({ "id": session_id, "messageLimit": 1 }),
        )
        .await
    {
        Ok(s) => s.get("session").cloned().unwrap_or(Value::Null),
        Err(e) => return envelope::envelope_from_rpc("host", &e),
    };
    let launch = match launch::resolve_launch(state, &session, &settings, params).await {
        Ok(l) => l,
        Err(envelope) => return envelope,
    };
    let mut request = launch.sidecar_params.clone();
    request["sessionId"] = json!(session_id);
    // A consolidation pass is not a turn: clear the turn slots so the sidecar
    // never mistakes the binding for a prompt.
    request["turnId"] = json!("");
    request["content"] = json!("");
    request["userMessageId"] = json!("");
    request["dryRun"] = json!(params
        .get("dryRun")
        .and_then(Value::as_bool)
        .unwrap_or(false));
    envelope_result(
        "sidecar",
        state
            .sidecar
            .call_timeout("agent.dream", request, DREAM_TIMEOUT)
            .await,
    )
}

/// `agent/compact` — resolve the session's launch binding and let the sidecar
/// install one manual checkpoint. The renderer follows compaction_start/end on
/// agent.event; this reply only reports that the request finished.
async fn agent_compact(state: &std::sync::Arc<AppState>, params: &Value) -> Value {
    let Some(session_id) = params.get("sessionId").and_then(|v| v.as_str()) else {
        return err("INVALID_ARGUMENT", "sessionId required");
    };
    let session_id = session_id.to_string();
    let settings = match state.host.call("settings.get", json!({})).await {
        Ok(s) => s,
        Err(e) => return envelope::envelope_from_rpc("host", &e),
    };
    let session = match state
        .host
        .call(
            "session.get",
            json!({ "id": session_id, "messageLimit": 1 }),
        )
        .await
    {
        Ok(s) => s.get("session").cloned().unwrap_or(Value::Null),
        Err(e) => return envelope::envelope_from_rpc("host", &e),
    };
    let launch = match launch::resolve_launch(state, &session, &settings, params).await {
        Ok(l) => l,
        Err(envelope) => return envelope,
    };
    let mut prompt = launch.sidecar_params.clone();
    prompt["sessionId"] = json!(session_id);
    prompt["content"] = json!("");
    envelope_result("sidecar", state.sidecar.call("agent.compact", prompt).await)
}

/// `agent/executeApprovedPlan` — claim the host execution, begin a durable
/// turn, then hand the approved contract to the sidecar as an internal turn.
async fn agent_execute_approved_plan(
    _app: &AppHandle,
    state: &std::sync::Arc<AppState>,
    params: &Value,
) -> Value {
    let Some(session_id) = params.get("sessionId").and_then(|v| v.as_str()) else {
        return err("INVALID_ARGUMENT", "sessionId required");
    };
    let session_id = session_id.to_string();
    let Some(execution) = params.get("execution").cloned().filter(|v| !v.is_null()) else {
        return err("INVALID_ARGUMENT", "execution required");
    };
    let execution_id = execution
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if execution_id.is_empty() {
        return err("INVALID_ARGUMENT", "execution.id required");
    }

    let settings = match state.host.call("settings.get", json!({})).await {
        Ok(s) => s,
        Err(e) => return envelope::envelope_from_rpc("host", &e),
    };
    let session = match state
        .host
        .call(
            "session.get",
            json!({ "id": session_id, "messageLimit": 1 }),
        )
        .await
    {
        Ok(s) => s.get("session").cloned().unwrap_or(Value::Null),
        Err(e) => return envelope::envelope_from_rpc("host", &e),
    };

    let claimed = match state
        .host
        .call(
            "plans.claimExecution",
            json!({ "executionId": execution_id }),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => return envelope::envelope_from_rpc("host", &e),
    };
    let execution = claimed
        .get("execution")
        .cloned()
        .unwrap_or_else(|| execution.clone());

    let launch = match launch::resolve_launch(state, &session, &settings, params).await {
        Ok(l) => l,
        Err(envelope) => {
            let _ = state
                .host
                .call(
                    "plans.finishExecution",
                    json!({ "executionId": execution_id, "status": "interrupted", "errorCode": "PLAN_EXECUTION_FAILED" }),
                )
                .await;
            return envelope;
        }
    };
    let begin = match state
        .host
        .call(
            "session.beginTurn",
            json!({
                "sessionId": session_id,
                "providerId": launch.provider_id,
                "modelId": launch.model_id,
                "sessionMessageId": Value::Null,
            }),
        )
        .await
    {
        Ok(b) => b,
        Err(e) => {
            let _ = state
                .host
                .call(
                    "plans.finishExecution",
                    json!({ "executionId": execution_id, "status": "interrupted", "errorCode": "TURN_NOT_FOUND" }),
                )
                .await;
            return envelope::envelope_from_rpc("host", &e);
        }
    };
    let turn_id = begin
        .get("turnId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if turn_id.is_empty() {
        let _ = state
            .host
            .call(
                "plans.finishExecution",
                json!({ "executionId": execution_id, "status": "interrupted", "errorCode": "TURN_NOT_FOUND" }),
            )
            .await;
        return err("TURN_NOT_FOUND", "host did not create an execution turn");
    }

    let mut prompt = launch.sidecar_params.clone();
    prompt["mode"] = json!("agent");
    prompt["turnId"] = json!(turn_id);
    prompt["content"] = json!("");
    prompt["execution"] = execution;

    let result = state
        .sidecar
        .call("agent.executeApprovedPlan", prompt)
        .await;
    match &result {
        Ok(_) => {
            state
                .active_turns
                .lock()
                .unwrap()
                .insert(session_id.clone(), turn_id.clone());
            state
                .active_executions
                .lock()
                .unwrap()
                .insert(turn_id.clone(), execution_id.clone());
        }
        Err(_) => {
            let _ = state
                .host
                .call(
                    "plans.finishExecution",
                    json!({ "executionId": execution_id, "status": "interrupted", "errorCode": "AGENT_UNAVAILABLE" }),
                )
                .await;
            let _ = state
                .host
                .call(
                    "session.endTurn",
                    json!({ "sessionId": session_id, "turnId": turn_id, "status": "error" }),
                )
                .await;
        }
    }
    envelope_result("sidecar", result)
}
