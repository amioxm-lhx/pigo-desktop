//! The `Result<T>` envelope the renderer expects from every invoke, mirroring
//! `packages/shared/src/errors.ts`. Every `pi_desktop_invoke` call resolves
//! with one of these — a handler failure must never reject the promise.

use serde_json::{json, Value};

use crate::rpc::RpcError;

pub const INTERNAL: &str = "INTERNAL";
pub const HOST_UNAVAILABLE: &str = "HOST_UNAVAILABLE";
pub const AGENT_UNAVAILABLE: &str = "AGENT_UNAVAILABLE";
pub const NOT_IMPLEMENTED: &str = "UNSUPPORTED";

pub fn ok(data: Value) -> Value {
    json!({ "ok": true, "data": data })
}

pub fn err(code: &str, message: impl Into<String>) -> Value {
    json!({ "ok": false, "error": { "code": code, "message": message.into() } })
}

/// Phase-2 marker: the Electron main implements this channel, the Tauri shell
/// does not yet. The renderer renders the error inline instead of crashing.
pub fn not_implemented(channel: &str) -> Value {
    err(
        NOT_IMPLEMENTED,
        format!("channel '{channel}' lands in Tauri phase 2 (see apps/desktop-tauri/README.md)"),
    )
}

/// Map a backend RPC failure to the envelope error. Remote errors keep the
/// backend's `errorCode` (it lives in `data.errorCode`, matching what the
/// Electron `wrap()` helper extracted from thrown errors); transport failures
/// become HOST_UNAVAILABLE / AGENT_UNAVAILABLE so the renderer can offer a
/// reconnect affordance.
pub fn envelope_from_rpc(label: &str, error: &RpcError) -> Value {
    match error {
        RpcError::Remote { message, data } => {
            let code = data
                .get("errorCode")
                .and_then(|v| v.as_str())
                .unwrap_or(INTERNAL);
            let mut err_obj = json!({ "code": code, "message": message });
            if let Some(details) = data.get("details") {
                err_obj["details"] = details.clone();
            }
            if data.get("retriable").and_then(|v| v.as_bool()) == Some(true) {
                err_obj["retriable"] = json!(true);
            }
            json!({ "ok": false, "error": err_obj })
        }
        RpcError::Transport(message) => {
            let code = if label == "sidecar" {
                AGENT_UNAVAILABLE
            } else {
                HOST_UNAVAILABLE
            };
            err(code, format!("{label} unavailable: {message}"))
        }
    }
}
