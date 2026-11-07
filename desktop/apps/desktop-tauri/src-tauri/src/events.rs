//! Backend notification dispatch — the Electron main's event fan-out rules,
//! ported: sidecar `agent.event` → renderer transcript + durable message
//! persistence; host `permissions.request` → renderer approval prompt;
//! `plugins.execute` → the Node plugin runtime; everything else → forwarded
//! to the sidecar as `host.notification`.

use std::sync::Arc;

use serde_json::{json, Value};
use tauri::Emitter;

use crate::proxy::HOST_PROXY_ALLOWED;
use crate::state::{ActiveTool, AppState};

pub fn dispatch(
    app: tauri::AppHandle,
    state: Arc<AppState>,
    mut host_rx: tokio::sync::mpsc::Receiver<(String, Value)>,
    mut sidecar_rx: tokio::sync::mpsc::Receiver<(String, Value)>,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            let item = tokio::select! {
                item = host_rx.recv() => item.map(|(m, p)| ("host", m, p)),
                item = sidecar_rx.recv() => item.map(|(m, p)| ("sidecar", m, p)),
            };
            let Some((source, method, params)) = item else {
                break;
            };
            match (source, method.as_str()) {
                ("sidecar", "agent.event") => handle_agent_event(&app, &state, params).await,
                ("host", "permissions.request") => {
                    let envelope = json!({
                        "sessionId": params.get("sessionId").cloned().unwrap_or(Value::Null),
                        "ts": now_ms(),
                        "event": { "type": "tool_permission_request", "request": params },
                    });
                    let _ = app.emit("pi-desktop/agent/event/message", envelope);
                }
                ("host", "plugins.execute") => {
                    // host-core already applied its permission policy. Both
                    // namespaces arrive here: `plugin_*` runs in the Node
                    // runtime, `mcp_*` runs through the user's MCP server.
                    let execution_id = params.get("executionId").cloned().unwrap_or(Value::Null);
                    let tool_name = params.get("toolName").and_then(Value::as_str).unwrap_or("");
                    let mut result = if tool_name.starts_with("mcp_") {
                        state.mcp.execute(&params).await
                    } else {
                        state.plugins.execute_tool(&params).await
                    };
                    result["executionId"] = execution_id;
                    let _ = state.host.call("plugins.resolveExecution", result).await;
                }
                ("host", "plans.changed") => {
                    let _ = app.emit("pi-desktop/plans/event/changed", params);
                }
                ("host", _) => {
                    // Everything else the sidecar subscribed to. It must never
                    // see permissions.request twice (the agent event above is
                    // the renderer leg).
                    if method != "permissions.request" {
                        let _ = state
                            .sidecar
                            .notify(
                                "host.notification",
                                json!({ "method": method, "params": params }),
                            )
                            .await;
                    }
                }
                (_, _) => {}
            }
        }
    });
}

async fn handle_agent_event(app: &tauri::AppHandle, state: &Arc<AppState>, envelope: Value) {
    let _ = app.emit("pi-desktop/agent/event/message", envelope.clone());
    // Side-thread (/btw) answers are renderer-only. They were forwarded above,
    // but must not be persisted, attributed to the session's turn lifecycle,
    // or replayed to staff/plugins — every one of those paths would write the
    // answer into the main conversation.
    if envelope.get("scope").and_then(|v| v.as_str()) == Some("side") {
        return;
    }
    // Staff step turns await their own events through the resident driver.
    // Terminal events are handed over only after the turn is settled below:
    // the next step's beginTurn would otherwise race session.endTurn and be
    // refused as busy.
    let terminal = envelope
        .get("event")
        .and_then(|e| e.get("type"))
        .and_then(|v| v.as_str())
        .map(|event| matches!(event, "agent_end" | "error"))
        .unwrap_or(false);
    if !terminal {
        state.staff.notify_turn(&envelope);
    }

    let session_id = envelope.get("sessionId").and_then(|v| v.as_str());
    let turn_id = envelope.get("turnId").and_then(|v| v.as_str());
    let parent_tool_call_id = envelope.get("parentToolCallId").and_then(|v| v.as_str());
    let event_type = envelope
        .get("event")
        .and_then(|e| e.get("type"))
        .and_then(|v| v.as_str());

    let (Some(session_id), Some(event_type)) = (session_id, event_type) else {
        return;
    };
    // Rows a subagent produced are transcript content too (D201, ADR 0062),
    // but their events must never drive the parent turn lifecycle.
    let is_subagent = parent_tool_call_id.is_some();
    let agent_name = envelope.get("agentName").and_then(|v| v.as_str());

    // Agent extensions see the parent session's loop (spec 07-plugins/16 §6):
    // the Go sidecar's event names are mapped to the ExtensionAPI names and
    // pushed to the plugins that brought extensions. A subagent's rows are
    // transcript content and are not replayed into a session handler.
    if !is_subagent {
        if let Some(name) = extension_event_name(event_type) {
            let mut payload = envelope.get("event").cloned().unwrap_or(Value::Null);
            if let Some(object) = payload.as_object_mut() {
                object.insert("type".into(), json!(name));
                object.insert("sessionId".into(), json!(session_id));
                if let Some(turn_id) = turn_id {
                    object.insert("turnId".into(), json!(turn_id));
                }
            }
            state
                .plugins
                .emit_extension_event(session_id, name, payload)
                .await;
        }
    }

    match event_type {
        // Durable transcript rows: user and assistant messages land in the
        // host database through session.appendMessage (the outbox file queue
        // is phase 2; the DB write itself must not be skipped).
        "message_end" => {
            let message = envelope
                .get("event")
                .and_then(|e| e.get("message"))
                .cloned()
                .unwrap_or(Value::Null);
            let role = message
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            // A subagent's own rows are persisted with their Task attribution.
            // A parent-level tool row arriving on message_end is the delegation
            // settlement refreshing the Task card written at tool_end, so it
            // reuses that row's durable id instead of appending a duplicate.
            if role != "user" && role != "assistant" && !(role == "tool" && !is_subagent) {
                return;
            }
            let mut row = message;
            let row_id = if role == "tool" {
                let tool_call_id = row.get("toolCallId").and_then(|v| v.as_str()).unwrap_or("");
                if tool_call_id.is_empty() {
                    return;
                }
                format!("{}:{}", turn_id.unwrap_or(session_id), tool_call_id)
            } else {
                row.get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            if let Some(object) = row.as_object_mut() {
                object.insert("id".into(), json!(row_id));
                if is_subagent {
                    stamp_subagent_row(object, parent_tool_call_id, agent_name);
                }
            }
            // Persist inline so the delegation settlement cannot overtake the
            // Task row it refreshes: the dispatch loop handles events in order.
            let params = json!({
                "sessionId": session_id,
                "message": row,
                "turnId": turn_id,
            });
            if let Err(e) = state.host.call("session.appendMessage", params).await {
                log::warn!("session.appendMessage failed: {e}");
            }
        }
        // Tool rows are durable transcript content: capture the call at
        // tool_start, then append the completed row at tool_end. Without this
        // a refreshed transcript loses every tool call and its result.
        "tool_start" => {
            let event = envelope.get("event").cloned().unwrap_or(Value::Null);
            let Some(tool_call_id) = event.get("toolCallId").and_then(|v| v.as_str()) else {
                return;
            };
            let name = event
                .get("toolName")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let args = event.get("args").cloned().unwrap_or(Value::Null);
            state.active_tools.lock().unwrap().insert(
                (session_id.to_string(), tool_call_id.to_string()),
                ActiveTool {
                    name,
                    args,
                    started_at: now_ms(),
                },
            );
        }
        "tool_end" => {
            let event = envelope.get("event").cloned().unwrap_or(Value::Null);
            let Some(tool_call_id) = event.get("toolCallId").and_then(|v| v.as_str()) else {
                return;
            };
            let active = state
                .active_tools
                .lock()
                .unwrap()
                .remove(&(session_id.to_string(), tool_call_id.to_string()));
            let Some(active) = active else { return };
            let result = event.get("result").cloned().unwrap_or(Value::Null);
            let is_error = event
                .get("isError")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let completed_at =
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            let mut row = json!({
                "id": format!("{}:{}", turn_id.unwrap_or(session_id), tool_call_id),
                "role": "tool",
                "content": tool_result_text(&result),
                "createdAt": completed_at,
                "toolName": active.name,
                "toolCallId": tool_call_id,
                "toolArgs": active.args,
                "toolResult": result,
                "toolStatus": "complete",
                "isError": is_error,
                "toolCompletedAt": completed_at,
                "toolDurationMs": now_ms().saturating_sub(active.started_at),
            });
            if is_subagent {
                if let Some(object) = row.as_object_mut() {
                    stamp_subagent_row(object, parent_tool_call_id, agent_name);
                }
            }
            // Inline for the same reason as message_end: ordered durable writes.
            let params = json!({
                "sessionId": session_id,
                "message": row,
                "turnId": turn_id,
            });
            if let Err(e) = state.host.call("session.appendMessage", params).await {
                log::warn!("session.appendMessage(tool) failed: {e}");
            }
        }
        // Only the parent session drives the turn lifecycle; a delegate's
        // error and agent_end belong to its Task card, not the session.
        _ if is_subagent => {}
        "error" => {
            clear_active_tools(state, session_id);
            state.active_turns.lock().unwrap().remove(session_id);
            let error_code = envelope
                .get("event")
                .and_then(|e| e.get("error"))
                .and_then(|e| e.get("code"))
                .and_then(|v| v.as_str())
                .unwrap_or("AGENT_ERROR");
            let execution_id = state
                .active_executions
                .lock()
                .unwrap()
                .remove(turn_id.unwrap_or(""));
            if let Some(execution_id) = execution_id {
                let _ = state
                    .host
                    .call(
                        "plans.finishExecution",
                        json!({ "executionId": execution_id, "status": "interrupted", "errorCode": error_code }),
                    )
                    .await;
            }
            let params = json!({ "sessionId": session_id, "turnId": turn_id, "status": "error" });
            if let Err(e) = state.host.call("session.endTurn", params).await {
                log::warn!("session.endTurn(error) failed: {e}");
            }
            crate::collaboration::settle_async(app, state, turn_id);
            state.staff.notify_turn(&envelope);
            let app = app.clone();
            let state = state.clone();
            let session_id = session_id.to_string();
            tauri::async_runtime::spawn(async move {
                crate::queue::run_next(&app, &state, &session_id).await;
            });
        }
        "agent_end" => {
            clear_active_tools(state, session_id);
            state.active_turns.lock().unwrap().remove(session_id);
            let params =
                json!({ "sessionId": session_id, "turnId": turn_id, "status": "completed" });
            if let Err(e) = state.host.call("session.endTurn", params).await {
                log::warn!("session.endTurn(completed) failed: {e}");
            }
            crate::collaboration::settle_async(app, state, turn_id);
            if let Err(e) = state
                .host
                .call(
                    "session.saveActiveRevision",
                    json!({ "sessionId": session_id }),
                )
                .await
            {
                log::warn!("session.saveActiveRevision failed: {e}");
            }
            let execution_id = state
                .active_executions
                .lock()
                .unwrap()
                .remove(turn_id.unwrap_or(""));
            if let Some(execution_id) = execution_id {
                let _ = state
                    .host
                    .call(
                        "plans.finishExecution",
                        json!({ "executionId": execution_id, "status": "completed" }),
                    )
                    .await;
            }
            state.staff.notify_turn(&envelope);
            let app = app.clone();
            let state = state.clone();
            let session_id = session_id.to_string();
            tauri::async_runtime::spawn(async move {
                crate::queue::run_next(&app, &state, &session_id).await;
            });
        }
        _ => {}
    }
}

/// Wire the sidecar's reverse channel (`host.proxy`): whitelist, serve the
/// shell-local BrowserPreview, reject the remaining Electron-only local tools,
/// and forward the rest to host-core.
pub fn install_sidecar_proxy(app: tauri::AppHandle, state: &Arc<AppState>) {
    let state = state.clone();
    let sidecar = state.sidecar.clone();
    sidecar.set_request_handler(Arc::new(move |method: String, params: Value| {
        let state = state.clone();
        let app = app.clone();
        Box::pin(async move {
            if method != "host.proxy" {
                return Err((
                    -32601,
                    format!("unexpected sidecar request {method}"),
                    Value::Null,
                ));
            }
            let inner_method = params
                .get("method")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let inner_params = params.get("params").cloned().unwrap_or(Value::Null);
            if !HOST_PROXY_ALLOWED.contains(&inner_method.as_str()) {
                return Err((
                    -32601,
                    format!("method not allowed: {inner_method}"),
                    json!({ "errorCode": "METHOD_NOT_FOUND" }),
                ));
            }
            // Vendor-account auth is answered here: host-core must never
            // hand a refresh token to the sidecar. The pair is validated
            // against the binding table the last launch rewrote.
            if inner_method == "provider.resolveAuth" {
                let session_id = inner_params
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let provider_id = inner_params
                    .get("providerId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let bound = state
                    .provider_bindings
                    .lock()
                    .unwrap()
                    .get(session_id)
                    .cloned()
                    .unwrap_or_default();
                if provider_id.is_empty() || !bound.iter().any(|id| id == provider_id) {
                    return Err((
                        -32000,
                        "provider is not bound to this session".to_string(),
                        json!({ "errorCode": "PROVIDER_NOT_BOUND" }),
                    ));
                }
                return match crate::oauth::resolve_access_token(&state, provider_id).await {
                    Ok(token) => Ok(json!({ "apiKey": token })),
                    Err(message) => Err((
                        -32000,
                        message,
                        json!({ "errorCode": "PROVIDER_NOT_BOUND" }),
                    )),
                };
            }
            // Local short-circuit tools own a shell surface: BrowserPreview
            // opens a workspace HTML file, while Skill/Plugin* belong to the
            // Node plugin host this build does not carry.
            if inner_method == "tools.execute" {
                let tool_name = inner_params
                    .get("toolName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if tool_name == "BrowserPreview" {
                    return Ok(handle_browser_preview(&app, &state, &inner_params).await);
                }
                if matches!(
                    tool_name,
                    "Skill" | "PluginScaffold" | "PluginCheck" | "PluginPack"
                ) {
                    return Err((
                        -32000,
                        format!("local tool {tool_name} is not available in the Tauri shell yet"),
                        json!({ "errorCode": "TOOL_NOT_FOUND" }),
                    ));
                }
            }
            match state.host.call(&inner_method, inner_params).await {
                Ok(result) => Ok(result),
                Err(e) => {
                    let data = match &e {
                        crate::rpc::RpcError::Remote { data, .. } => data.clone(),
                        crate::rpc::RpcError::Transport(_) => {
                            json!({ "errorCode": "HOST_UNAVAILABLE" })
                        }
                    };
                    Err((-32000, e.to_string(), data))
                }
            }
        })
    }));
}

/// `BrowserPreview`: resolve the file against the originating session's
/// workspace (never the visible shell project) and open it in the default
/// browser. The embedded preview pane is not hosted in this build, so the
/// result says exactly what happened.
async fn handle_browser_preview(
    app: &tauri::AppHandle,
    state: &Arc<AppState>,
    params: &Value,
) -> Value {
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let raw = params
        .get("args")
        .and_then(|args| {
            args.get("path")
                .or_else(|| args.get("file_path"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .trim()
        .to_string();
    let failure = |code: &str, message: String| json!({ "ok": false, "isError": true, "errorCode": code, "content": message });
    if raw.is_empty() {
        return failure(
            "BROWSER_PREVIEW_INVALID",
            "BrowserPreview requires a workspace-relative `path`.".to_string(),
        );
    }
    let detail = state
        .host
        .call("session.get", json!({ "id": session_id }))
        .await;
    let root = detail.ok().and_then(|reply| {
        reply
            .pointer("/session/projectPath")
            .and_then(|path| path.as_str())
            .map(str::to_string)
    });
    let Some(root) = root else {
        return failure(
            "BROWSER_PREVIEW_NO_WORKSPACE",
            "BrowserPreview needs a session opened on a workspace.".to_string(),
        );
    };
    let resolved = match resolve_preview_path(&root, &raw) {
        Ok(path) => path,
        Err(message) => return failure("BROWSER_PREVIEW_INVALID", message),
    };
    if let Some(server) = state.preview.as_ref() {
        if let Some(url) = server.register(std::path::Path::new(&root), &resolved) {
            let _ = app.emit(
                "pi-desktop/browser/event/preview",
                json!({ "sessionId": session_id, "url": url }),
            );
            return json!({
                "ok": true,
                "content": format!(
                    "Opened {raw} in the embedded browser preview. It live-reloads when the page or its sibling assets change."
                ),
            });
        }
    }
    // No loopback service: the system browser is still a usable preview.
    if let Err(error) = crate::native::open_path(app, &resolved.to_string_lossy()) {
        return failure(
            "BROWSER_PREVIEW_FAILED",
            format!("BrowserPreview could not open {raw}: {error}"),
        );
    }
    json!({
        "ok": true,
        "content": format!(
            "Opened {raw} in the default browser. This build could not start its embedded preview service; the page does not live-reload."
        ),
    })
}

/// Resolve `raw` inside `root` and require an existing HTML file. The lexical
/// path is validated first; symlinks are canonicalized so a link cannot escape
/// the workspace root.
fn resolve_preview_path(root: &str, raw: &str) -> Result<std::path::PathBuf, String> {
    let normalized = normalize_preview_relative(raw)?;
    let root_path = std::fs::canonicalize(root)
        .map_err(|error| format!("workspace root is unreadable: {error}"))?;
    let target = root_path.join(&normalized);
    let canonical = std::fs::canonicalize(&target)
        .map_err(|error| format!("`{raw}` cannot be opened: {error}"))?;
    if !canonical.starts_with(&root_path) {
        return Err(format!("`{raw}` resolves outside the workspace."));
    }
    let is_html = canonical
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_lowercase().as_str(), "html" | "htm"))
        .unwrap_or(false);
    if !is_html {
        return Err(format!("`{raw}` is not an HTML file."));
    }
    if !canonical.is_file() {
        return Err(format!("`{raw}` is not a file."));
    }
    Ok(canonical)
}

/// Accept one workspace-relative path: forward or Windows separators, no
/// absolute or drive prefix, and no `..` segment. Empty and `.` segments are
/// dropped, matching the sidecar's normalization.
fn normalize_preview_relative(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("BrowserPreview requires a workspace-relative `path`.".to_string());
    }
    if trimmed.contains('\0') {
        return Err("BrowserPreview `path` contains an invalid character.".to_string());
    }
    let normalized = trimmed.replace('\\', "/");
    if normalized.starts_with('/') {
        return Err("BrowserPreview `path` must be workspace-relative, not absolute.".to_string());
    }
    if has_windows_drive_prefix(&normalized) {
        return Err(
            "BrowserPreview `path` must be workspace-relative, not a drive path.".to_string(),
        );
    }
    let mut segments: Vec<&str> = Vec::new();
    for segment in normalized.split('/') {
        match segment {
            "" | "." => continue,
            ".." => {
                return Err(
                    "BrowserPreview `path` must stay inside the workspace (no `..`).".to_string(),
                )
            }
            _ => segments.push(segment),
        }
    }
    if segments.is_empty() {
        return Err("BrowserPreview requires a workspace-relative file path.".to_string());
    }
    Ok(segments.join("/"))
}

/// A Windows drive path starts with `X:`. A colon later in a segment is a legal
/// POSIX file name and stays allowed.
fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// Drop pending tool metadata for a session that ended or was aborted.
fn clear_active_tools(state: &AppState, session_id: &str) {
    state
        .active_tools
        .lock()
        .unwrap()
        .retain(|(pending_session, _), _| pending_session != session_id);
}

/// Project one tool result into the text a transcript row stores. Host rows
/// keep the raw result separately (`toolResult`); `content` is what the model
/// context replays.
fn tool_result_text(result: &Value) -> String {
    match result {
        Value::String(text) => text.clone(),
        Value::Object(object) => match object.get("content") {
            Some(Value::String(text)) => text.clone(),
            Some(content) => content.to_string(),
            None => result.to_string(),
        },
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Stamp a persisted row with the `Task` call and delegate definition that
/// produced it (D201, ADR 0062). The transcript nests these under that card.
fn stamp_subagent_row(
    object: &mut serde_json::Map<String, Value>,
    parent_tool_call_id: Option<&str>,
    agent_name: Option<&str>,
) {
    if let Some(parent) = parent_tool_call_id {
        object.insert("parentToolCallId".into(), json!(parent));
    }
    if let Some(agent) = agent_name {
        object.insert("agentName".into(), json!(agent));
    }
}

/// Sidecar loop event → the name an ExtensionAPI handler registered with.
/// Events the sidecar does not emit stay absent rather than being invented.
fn extension_event_name(event_type: &str) -> Option<&'static str> {
    Some(match event_type {
        "agent_start" => "agent_start",
        "agent_end" => "agent_end",
        "turn_start" => "turn_start",
        "turn_end" => "turn_end",
        "message_start" => "message_start",
        "message_update" => "message_update",
        "message_end" => "message_end",
        "tool_start" => "tool_execution_start",
        "tool_update" => "tool_execution_update",
        "tool_end" => "tool_execution_end",
        _ => return None,
    })
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "pi-preview-{label}-{}-{}",
            std::process::id(),
            now_ms()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn sidecar_loop_events_map_to_extension_names() {
        assert_eq!(extension_event_name("agent_start"), Some("agent_start"));
        assert_eq!(
            extension_event_name("tool_start"),
            Some("tool_execution_start")
        );
        assert_eq!(extension_event_name("tool_end"), Some("tool_execution_end"));
        assert_eq!(extension_event_name("status"), None);
        assert_eq!(extension_event_name("planning_state"), None);
    }

    #[test]
    fn a_workspace_html_file_resolves() {
        let root = temp_root("ok");
        std::fs::write(root.join("index.html"), "<html></html>").unwrap();
        let resolved = resolve_preview_path(&root.to_string_lossy(), "index.html").unwrap();
        assert!(resolved.ends_with("index.html"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn traversal_and_non_html_are_refused() {
        let root = temp_root("invalid");
        std::fs::write(root.join("notes.txt"), "text").unwrap();
        assert!(resolve_preview_path(&root.to_string_lossy(), "../index.html").is_err());
        assert!(resolve_preview_path(&root.to_string_lossy(), "/etc/passwd").is_err());
        assert!(resolve_preview_path(&root.to_string_lossy(), "C:/demo/index.html").is_err());
        assert!(resolve_preview_path(&root.to_string_lossy(), "notes.txt").is_err());
        assert!(resolve_preview_path(&root.to_string_lossy(), "missing.html").is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn nested_dot_segments_and_colons_normalize() {
        let root = temp_root("normalize");
        std::fs::create_dir_all(root.join("demo")).unwrap();
        std::fs::write(root.join("demo/index.html"), "<html></html>").unwrap();
        let resolved = resolve_preview_path(&root.to_string_lossy(), "./demo//index.html").unwrap();
        assert!(resolved.ends_with("index.html"));
        // A colon is only a drive prefix in position 1; a POSIX file name keeps it.
        std::fs::write(root.join("notes: draft.html"), "<html></html>").unwrap();
        assert!(resolve_preview_path(&root.to_string_lossy(), "notes: draft.html").is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn subagent_rows_keep_their_task_attribution() {
        let mut row = json!({ "role": "tool", "toolName": "Read" });
        let object = row.as_object_mut().unwrap();
        stamp_subagent_row(object, Some("task-call-1"), Some("explorer"));
        assert_eq!(row["parentToolCallId"], "task-call-1");
        assert_eq!(row["agentName"], "explorer");
    }
}
