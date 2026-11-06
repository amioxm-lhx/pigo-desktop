//! Session collaboration delivery.
//!
//! Host-core owns the durable ledger but never starts a runtime (ADR 0239).
//! The shell is the Agent Host in this port, so it turns a queued ledger
//! message into a Host-owned turn-queue entry, starts the target session when
//! it is idle, and settles the delivery when the bound turn ends.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tauri::AppHandle;

use crate::queue;
use crate::state::AppState;

const PENDING_BATCH: usize = 64;
static DRAINING: AtomicBool = AtomicBool::new(false);

/// Map a queued ledger message to the queue entry that will run it.
///
/// The content is passed through unchanged: host-core rejects a collaboration
/// turn whose transcript input differs from the delivery (provenance check).
fn queue_entry(message: &Value) -> Option<(String, Value)> {
    if message.get("status").and_then(Value::as_str) != Some("queued") {
        return None;
    }
    let id = message.get("id").and_then(Value::as_str)?;
    let target = message.get("targetSessionId").and_then(Value::as_str)?;
    let content = message.get("content").and_then(Value::as_str)?;
    Some((
        target.to_string(),
        json!({
            "content": content,
            "sessionMessageId": id,
            "idempotencyKey": format!("collaboration:{id}"),
        }),
    ))
}

/// Enqueue one delivery and start the target when it is idle.
pub async fn deliver(app: &AppHandle, state: &Arc<AppState>, message: &Value) {
    let Some((target, payload)) = queue_entry(message) else {
        return;
    };
    let message_id = payload
        .get("sessionMessageId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let pushed = queue::push(app, state, &payload, &target).await;
    if pushed.get("ok").and_then(Value::as_bool) == Some(true) {
        queue::run_next(app, state, &target).await;
        return;
    }

    let error = pushed
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("collaboration delivery failed")
        .to_string();
    log::warn!("collaboration delivery {message_id} failed: {error}");
    if let Err(e) = state
        .host
        .call(
            "session.collaboration.fail",
            json!({ "messageId": message_id, "error": error }),
        )
        .await
    {
        log::warn!("session.collaboration.fail failed: {e}");
    }
    // A failed delivery can create its own completion callback; box the drain
    // so the two async functions are not infinitely sized.
    Box::pin(drain_pending(app, state)).await;
}

/// Spawn one delivery without making the caller's future depend on the
/// delivery pipeline (the route graph is recursive, see `desktop_control`).
pub fn deliver_async(app: &AppHandle, state: &Arc<AppState>, message: Value) {
    let app = app.clone();
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        deliver(&app, &state, &message).await;
    });
}

/// Settle the turn and deliver its completion callback, if it has one.
pub async fn settle_turn(app: &AppHandle, state: &Arc<AppState>, turn_id: &str) {
    let settled = state
        .host
        .call(
            "session.collaboration.settle",
            json!({ "turnId": turn_id }),
        )
        .await;
    match settled {
        Ok(value) => {
            if let Some(callback) = value.get("callback").filter(|value| !value.is_null()) {
                deliver(app, state, callback).await;
            }
            drain_pending(app, state).await;
        }
        Err(error) => log::warn!("session.collaboration.settle failed: {error}"),
    }
}

/// Deliver completion callbacks that are still queued. The guard keeps a
/// failed delivery from recursing into the drain that produced it.
pub async fn drain_pending(app: &AppHandle, state: &Arc<AppState>) {
    if DRAINING.swap(true, Ordering::SeqCst) {
        return;
    }
    let pending = state
        .host
        .call("session.collaboration.pending", json!({}))
        .await;
    if let Ok(value) = pending {
        if let Some(messages) = value.get("messages").and_then(Value::as_array) {
            for message in messages.iter().take(PENDING_BATCH) {
                deliver(app, state, message).await;
            }
        }
    }
    DRAINING.store(false, Ordering::SeqCst);
}

/// Settle from event handling without blocking the event loop.
pub fn settle_async(app: &AppHandle, state: &Arc<AppState>, turn_id: Option<&str>) {
    let Some(turn_id) = turn_id else {
        return;
    };
    let app = app.clone();
    let state = state.clone();
    let turn_id = turn_id.to_string();
    tauri::async_runtime::spawn(async move {
        settle_turn(&app, &state, &turn_id).await;
    });
}

#[cfg(test)]
mod tests {
    use super::queue_entry;
    use serde_json::json;

    #[test]
    fn a_queued_delivery_becomes_an_idempotent_queue_entry() {
        let message = json!({
            "id": "m1",
            "targetSessionId": "target",
            "content": "do the thing",
            "kind": "task",
            "status": "queued",
        });
        let (target, payload) = queue_entry(&message).unwrap();
        assert_eq!(target, "target");
        assert_eq!(payload["content"], "do the thing");
        assert_eq!(payload["sessionMessageId"], "m1");
        assert_eq!(payload["idempotencyKey"], "collaboration:m1");
    }

    #[test]
    fn completion_callbacks_are_delivered_the_same_way() {
        let message = json!({
            "id": "m2",
            "targetSessionId": "source",
            "content": "{\"status\":\"completed\"}",
            "kind": "completion",
            "status": "queued",
        });
        let (target, payload) = queue_entry(&message).unwrap();
        assert_eq!(target, "source");
        assert_eq!(payload["content"], "{\"status\":\"completed\"}");
    }

    #[test]
    fn claimed_or_malformed_messages_are_skipped() {
        let running = json!({
            "id": "m3",
            "targetSessionId": "t",
            "content": "x",
            "status": "running",
        });
        assert!(queue_entry(&running).is_none());
        assert!(queue_entry(&json!({ "id": "m4", "status": "queued" })).is_none());
        assert!(queue_entry(&json!({ "status": "queued" })).is_none());
    }
}
