//! Host-owned turn queue (D375 / ADR 0213). The host persists the entries; this
//! module supplies the renderer-owned fields the RPC requires (principal,
//! input hash, permission mode) and runs the head entry after a turn ends.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::envelope;
use crate::router;
use crate::rpc::envelope_result;
use crate::state::AppState;

const QUEUE_PRINCIPAL: &str = "renderer";
const QUEUE_CHANGED_EVENT: &str = "pi-desktop/agent/event/queueChanged";

/// Persist one queued prompt. The renderer sends only content/attachments; the
/// shell adds the fields the Host contract requires.
pub async fn push(
    app: &AppHandle,
    state: &Arc<AppState>,
    params: &Value,
    session_id: &str,
) -> Value {
    let Some(content) = params.get("content").and_then(|v| v.as_str()) else {
        return envelope::err("INVALID_ARGUMENT", "content required");
    };
    let permission_mode = match params.get("permissionMode").and_then(|v| v.as_str()) {
        Some(mode) => mode.to_string(),
        None => match state.host.call("settings.get", json!({})).await {
            Ok(settings) => settings
                .get("defaultMode")
                .and_then(|v| v.as_str())
                .unwrap_or("agent")
                .to_string(),
            Err(_) => "agent".to_string(),
        },
    };
    let attachments = params.get("attachments").cloned().unwrap_or(Value::Null);
    let mut payload = json!({
        "sessionId": session_id,
        "principal": QUEUE_PRINCIPAL,
        "inputHash": input_hash(content, &attachments),
        "content": content,
        "permissionMode": permission_mode,
    });
    if let Some(id) = params.get("id").and_then(|v| v.as_str()) {
        payload["id"] = json!(id);
    }
    if let Some(key) = params.get("idempotencyKey").and_then(|v| v.as_str()) {
        payload["idempotencyKey"] = json!(key);
    }
    if let Some(message_id) = params.get("sessionMessageId").and_then(|v| v.as_str()) {
        payload["sessionMessageId"] = json!(message_id);
    }
    if !attachments.is_null() {
        payload["attachments"] = attachments;
    }
    let result = state.host.call("session.queuePush", payload).await;
    match result {
        Ok(response) => {
            emit_changed(app, state, session_id).await;
            // The renderer consumes the entry directly, not the host's
            // `{ entry }` wrapper.
            envelope::ok(response.get("entry").cloned().unwrap_or(Value::Null))
        }
        Err(error) => envelope::envelope_from_rpc("host", &error),
    }
}

pub async fn list(state: &Arc<AppState>, params: &Value) -> Value {
    envelope_result(
        "host",
        state.host.call("session.queueList", params.clone()).await,
    )
}

/// Remove one entry and refresh the session it belonged to.
pub async fn remove(app: &AppHandle, state: &Arc<AppState>, _params: &Value, id: &str) -> Value {
    let session_id = session_for_entry(state, id).await;
    let result = state
        .host
        .call("session.queueRemove", json!({ "id": id }))
        .await;
    if let Some(session_id) = session_id {
        emit_changed(app, state, &session_id).await;
    }
    envelope_result("host", result)
}

pub async fn prioritize(
    app: &AppHandle,
    state: &Arc<AppState>,
    _params: &Value,
    id: &str,
) -> Value {
    let session_id = session_for_entry(state, id).await;
    let result = state
        .host
        .call("session.queuePrioritize", json!({ "id": id }))
        .await;
    if let Some(session_id) = session_id {
        emit_changed(app, state, &session_id).await;
    }
    envelope_result("host", result)
}

/// Start the head entry after a turn ended. A failed start leaves the entry in
/// the queue; an AGENT_BUSY races the sidecar's post-agent_end cleanup and is
/// retried briefly.
pub async fn run_next(app: &AppHandle, state: &Arc<AppState>, session_id: &str) {
    if state.active_turns.lock().unwrap().contains_key(session_id) {
        return;
    }
    let list = match state
        .host
        .call("session.queueList", json!({ "sessionId": session_id }))
        .await
    {
        Ok(list) => list,
        Err(e) => {
            log::warn!("session.queueList failed: {e}");
            return;
        }
    };
    let Some(entry) = list
        .get("entries")
        .and_then(|v| v.as_array())
        .and_then(|entries| entries.first())
        .cloned()
    else {
        return;
    };
    let Some(entry_id) = entry.get("id").and_then(|v| v.as_str()) else {
        return;
    };
    let mut prompt = json!({
        "sessionId": session_id,
        "content": entry.get("content").cloned().unwrap_or(json!("")),
    });
    if let Some(attachments) = entry.get("attachments").filter(|v| !v.is_null()) {
        prompt["attachments"] = attachments.clone();
    }
    if let Some(message_id) = entry.get("sessionMessageId").and_then(|v| v.as_str()) {
        prompt["sessionMessageId"] = json!(message_id);
    }

    for attempt in 0..5 {
        let result = router::route(
            app.clone(),
            state,
            "pi-desktop/agent/prompt",
            &[prompt.clone()],
        )
        .await;
        if result.get("ok").and_then(|v| v.as_bool()) == Some(true) {
            let _ = state
                .host
                .call("session.queueRemove", json!({ "id": entry_id }))
                .await;
            emit_changed(app, state, session_id).await;
            return;
        }
        let code = result
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if code == "AGENT_BUSY" && attempt < 4 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            continue;
        }
        log::warn!("queued turn start failed: {result}");
        emit_changed(app, state, session_id).await;
        return;
    }
}

async fn session_for_entry(state: &Arc<AppState>, id: &str) -> Option<String> {
    let list = state.host.call("session.queueList", json!({})).await.ok()?;
    list.get("entries")
        .and_then(|v| v.as_array())
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.get("id").and_then(|v| v.as_str()) == Some(id))
                .and_then(|entry| entry.get("sessionId").and_then(|v| v.as_str()))
                .map(str::to_string)
        })
}

pub async fn emit_changed(app: &AppHandle, state: &Arc<AppState>, session_id: &str) {
    let entries = state
        .host
        .call("session.queueList", json!({ "sessionId": session_id }))
        .await
        .ok()
        .and_then(|list| list.get("entries").cloned())
        .unwrap_or_else(|| json!([]));
    let _ = app.emit(
        QUEUE_CHANGED_EVENT,
        json!({ "sessionId": session_id, "entries": entries }),
    );
}

fn input_hash(content: &str, attachments: &Value) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    attachments.to_string().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
