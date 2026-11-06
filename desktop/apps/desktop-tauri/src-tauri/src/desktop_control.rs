//! Plugin gateway to the reviewed desktop-control catalog (ADR 0203/0208).
//!
//! The catalog is the project/session/Agent/workspace flow the local MCP
//! control plane exposes, plus the six plugin-only `session/collaboration/*`
//! operations. Invocation delegates to the same Tauri channel router the
//! renderer uses, so permission checks, lifecycle side effects and error
//! envelopes are not reimplemented here.
//!
//! A `dangerous` operation needs two answers: the plugin's `confirm: true`
//! acknowledgement, then a native dialog the user answers. A plugin (or a
//! model behind it) cannot relabel the operation: the dialog shows only the
//! catalog id, the catalog description, and a bounded argument preview.

use std::sync::Arc;

use serde_json::{json, Map, Value};
use tauri::Emitter;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::state::AppState;

const RESULT_LOG_LIMIT: usize = 400;

#[derive(Debug)]
pub struct DesktopError {
    pub code: String,
    pub message: String,
}

impl DesktopError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Risk {
    Read,
    Write,
    Dangerous,
}

impl Risk {
    fn label(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Dangerous => "dangerous",
        }
    }
}

#[derive(Clone, Copy)]
enum Args {
    /// No positional arguments.
    None,
    /// One session id, accepted as a bare string or `{ id }`.
    Id,
    /// One session id, accepted as a bare string or `{ sessionId }`.
    Session,
    /// `[id, title]`.
    Rename,
    /// `[id, config]`.
    Configure,
    /// `{ path }`, accepted as a bare string.
    FsList,
    /// `{ path, mimeType? }`, accepted as a bare string.
    FsRead,
    /// `{ sessionId? }`, accepted as a bare string.
    PlansPending,
    /// `{ id }` plus explicit renderer navigation afterwards.
    SessionOpen,
    /// Everything the caller passed, unchanged.
    PassThrough,
    /// Collaboration input; the gateway injects plugin and session identity.
    Collaboration,
}

struct Operation {
    id: &'static str,
    channel: &'static str,
    description: &'static str,
    risk: Risk,
    args: Args,
}

const OPERATIONS: &[Operation] = &[
    Operation {
        id: "app/getVersion",
        channel: "pi-desktop/app/getVersion",
        description: "Read the desktop app name and version",
        risk: Risk::Read,
        args: Args::None,
    },
    Operation {
        id: "project/get",
        channel: "pi-desktop/project/get",
        description: "Read the project currently open in the desktop",
        risk: Risk::Read,
        args: Args::None,
    },
    Operation {
        id: "project/list",
        channel: "pi-desktop/project/list",
        description: "List durable desktop projects",
        risk: Risk::Read,
        args: Args::None,
    },
    Operation {
        id: "project/set",
        channel: "pi-desktop/project/set",
        description: "Open a project folder in the desktop",
        risk: Risk::Write,
        args: Args::PassThrough,
    },
    Operation {
        id: "project/clear",
        channel: "pi-desktop/project/clear",
        description: "Close the project currently open in the desktop",
        risk: Risk::Write,
        args: Args::None,
    },
    Operation {
        id: "session/list",
        channel: "pi-desktop/session/list",
        description: "List durable desktop sessions",
        risk: Risk::Read,
        args: Args::PassThrough,
    },
    Operation {
        id: "session/get",
        channel: "pi-desktop/session/get",
        description: "Read one durable session",
        risk: Risk::Read,
        args: Args::Id,
    },
    Operation {
        id: "session/open",
        channel: "pi-desktop/session/open",
        description: "Open a durable session in the desktop",
        risk: Risk::Write,
        args: Args::SessionOpen,
    },
    Operation {
        id: "session/create",
        channel: "pi-desktop/session/create",
        description: "Create a durable session",
        risk: Risk::Write,
        args: Args::PassThrough,
    },
    Operation {
        id: "session/rename",
        channel: "pi-desktop/session/rename",
        description: "Rename a durable session",
        risk: Risk::Write,
        args: Args::Rename,
    },
    Operation {
        id: "session/fork",
        channel: "pi-desktop/session/fork",
        description: "Fork a durable session",
        risk: Risk::Write,
        args: Args::PassThrough,
    },
    Operation {
        id: "session/delete",
        channel: "pi-desktop/session/delete",
        description: "Delete a durable session",
        risk: Risk::Dangerous,
        args: Args::Id,
    },
    Operation {
        id: "session/configure",
        channel: "pi-desktop/session/configure",
        description: "Change a session's mode or permission mode",
        risk: Risk::Dangerous,
        args: Args::Configure,
    },
    Operation {
        id: "agent/prompt",
        channel: "pi-desktop/agent/prompt",
        description: "Send a prompt to a durable session",
        risk: Risk::Write,
        args: Args::PassThrough,
    },
    Operation {
        id: "agent/status",
        channel: "pi-desktop/agent/getStatus",
        description: "Read a session's agent status",
        risk: Risk::Read,
        args: Args::Session,
    },
    Operation {
        id: "agent/stop",
        channel: "pi-desktop/agent/stop",
        description: "Stop a session's current turn",
        risk: Risk::Write,
        args: Args::Session,
    },
    Operation {
        id: "agent/abort",
        channel: "pi-desktop/agent/abort",
        description: "Abort a session's current turn",
        risk: Risk::Write,
        args: Args::Session,
    },
    Operation {
        id: "agent/compact",
        channel: "pi-desktop/agent/compact",
        description: "Compact a session's conversation context",
        risk: Risk::Write,
        args: Args::PassThrough,
    },
    Operation {
        id: "plans/pending",
        channel: "pi-desktop/plans/pending",
        description: "List pending plan approvals",
        risk: Risk::Read,
        args: Args::PlansPending,
    },
    Operation {
        id: "plans/resolve",
        channel: "pi-desktop/plans/resolve",
        description: "Approve or reject a pending plan",
        risk: Risk::Dangerous,
        args: Args::PassThrough,
    },
    Operation {
        id: "workspace/diff",
        channel: "pi-desktop/workspace/diff",
        description: "Read the working-tree diff",
        risk: Risk::Read,
        args: Args::None,
    },
    Operation {
        id: "fs/list",
        channel: "pi-desktop/fs/list",
        description: "List a directory in the open project",
        risk: Risk::Read,
        args: Args::FsList,
    },
    Operation {
        id: "fs/read",
        channel: "pi-desktop/fs/read",
        description: "Read a file in the open project",
        risk: Risk::Read,
        args: Args::FsRead,
    },
    // Plugin-only: callable through this gateway, absent from the MCP catalog
    // because they need an authenticated plugin invocation context.
    Operation {
        id: "session/collaboration/spawn",
        channel: "session.collaboration.spawn",
        description: "Create a worker session and deliver a task to it",
        risk: Risk::Write,
        args: Args::Collaboration,
    },
    Operation {
        id: "session/collaboration/send",
        channel: "session.collaboration.send",
        description: "Send a task or message to a durable session",
        risk: Risk::Write,
        args: Args::Collaboration,
    },
    Operation {
        id: "session/collaboration/status",
        channel: "session.collaboration.status",
        description: "Read a session's collaboration status",
        risk: Risk::Read,
        args: Args::Collaboration,
    },
    Operation {
        id: "session/collaboration/list",
        channel: "session.collaboration.list",
        description: "List sessions that can receive a collaboration message",
        risk: Risk::Read,
        args: Args::Collaboration,
    },
    Operation {
        id: "session/collaboration/result",
        channel: "session.collaboration.result",
        description: "Read a collaboration result",
        risk: Risk::Read,
        args: Args::Collaboration,
    },
    Operation {
        id: "session/collaboration/cancel",
        channel: "session.collaboration.cancel",
        description: "Cancel a queued collaboration delivery",
        risk: Risk::Write,
        args: Args::Collaboration,
    },
];

/// The plugin-facing catalog: ids, descriptions and risk only. Channel names
/// stay inside the shell, matching the MCP surface.
pub fn catalog() -> Vec<Value> {
    OPERATIONS
        .iter()
        .map(|operation| {
            json!({
                "id": operation.id,
                "description": operation.description,
                "risk": operation.risk.label(),
            })
        })
        .collect()
}

fn find(id: &str) -> Option<&'static Operation> {
    OPERATIONS.iter().find(|operation| operation.id == id)
}

fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase().replace(['_', '-'], "");
    [
        "secret",
        "apikey",
        "token",
        "password",
        "authorization",
        "credential",
        "cookie",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

/// Secret-shaped fields never reach the IPC handler, even inside an object the
/// operation would otherwise forward verbatim (ADR 0203 amendment D372).
fn strip_secrets(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(key, _)| !is_secret_key(key))
                .map(|(key, value)| (key.clone(), strip_secrets(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(strip_secrets).collect()),
        other => other.clone(),
    }
}

fn string_arg(args: &[Value], key: &str) -> Option<String> {
    args.first()
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| args.first().and_then(Value::as_str).map(str::to_string))
}

fn object_arg(args: &[Value]) -> Map<String, Value> {
    args.first()
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn require_string(
    value: Option<String>,
    code: &str,
    message: &str,
) -> Result<String, DesktopError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| DesktopError::new(code, message))
}

fn routed_args(operation: &Operation, args: &[Value]) -> Result<Vec<Value>, DesktopError> {
    let routed = match operation.args {
        Args::None => Vec::new(),
        Args::Id => vec![json!({
            "id": require_string(string_arg(args, "id"), "INVALID_ARGUMENT", "session id required")?,
        })],
        Args::Session => vec![json!({
            "sessionId": require_string(
                string_arg(args, "sessionId"),
                "INVALID_ARGUMENT",
                "session id required",
            )?,
        })],
        Args::Rename => vec![json!({
            "id": require_string(
                string_arg(args, "id"),
                "INVALID_ARGUMENT",
                "session id required",
            )?,
            "title": args
                .get(1)
                .and_then(Value::as_str)
                .or_else(|| args.first().and_then(|value| value.get("title")).and_then(Value::as_str))
                .ok_or_else(|| DesktopError::new("INVALID_ARGUMENT", "title required"))?,
        })],
        Args::Configure => vec![
            args.first().cloned().unwrap_or(Value::Null),
            args.get(1).cloned().unwrap_or(Value::Null),
        ],
        Args::FsList => vec![json!({
            "path": string_arg(args, "path").unwrap_or_default(),
        })],
        Args::FsRead => {
            let mut params = object_arg(args);
            if let Some(path) = string_arg(args, "path") {
                params.insert("path".into(), json!(path));
            }
            vec![Value::Object(params)]
        }
        Args::PlansPending => match string_arg(args, "sessionId") {
            Some(session_id) => vec![json!({ "sessionId": session_id })],
            None => Vec::new(),
        },
        Args::SessionOpen => vec![json!({
            "id": require_string(string_arg(args, "id"), "INVALID_ARGUMENT", "session id required")?,
        })],
        Args::PassThrough => args.to_vec(),
        // Collaboration inputs are assembled by `invoke`, which injects the
        // plugin and session identity; no renderer channel is involved.
        Args::Collaboration => Vec::new(),
    };
    Ok(routed)
}

fn preview(args: &[Value]) -> String {
    let text = serde_json::to_string(args).unwrap_or_else(|_| "[]".to_string());
    text.chars().take(RESULT_LOG_LIMIT).collect()
}

/// Native, blocking consent for a dangerous operation. The text is built from
/// catalog data and the bounded argument preview only.
async fn confirm_dangerous(
    app: &tauri::AppHandle,
    plugin_id: &str,
    operation: &'static Operation,
    args: &[Value],
) -> bool {
    let zh = crate::plugins::system_locale()
        .to_lowercase()
        .starts_with("zh");
    let detail = preview(args);
    let body = if zh {
        format!(
            "插件：{plugin_id}\n操作：{}（{}）\n参数：{detail}\n\n本次调用是否允许？",
            operation.id, operation.description
        )
    } else {
        format!(
            "Plugin: {plugin_id}\nOperation: {} ({})\nArguments: {detail}\n\nAllow this call?",
            operation.id, operation.description
        )
    };
    let allow = if zh { "允许一次" } else { "Allow once" };
    let deny = if zh { "拒绝" } else { "Deny" };
    let title = if zh {
        "允许插件运行桌面操作？"
    } else {
        "Allow a plugin to run a desktop operation?"
    };
    let app = app.clone();
    let (allow, deny, title) = (allow.to_string(), deny.to_string(), title.to_string());
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .message(body)
            .title(title)
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(allow, deny))
            .blocking_show()
    })
    .await
    .unwrap_or(false)
}

/// Run the shared channel router on a separate task.
///
/// Awaiting `router::route` inline would put its whole future inside the
/// plugin host's request handler future, which already reaches the plugin
/// runtime: rustc then detects a Send cycle between `load_row`, this gateway
/// and `staff_driver::run_now`. Spawning is also the honest shape — the route
/// is independent work, not part of the plugin call's stack.
type RouteFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Value> + Send>>;

/// A boxed router future is a deliberate compiler barrier: its `Send` bound is
/// declared, so the plugin host's future no longer has to prove the router's
/// internals (which reach `staff_driver::run_now` and back) are `Send`.
fn route_future(
    app: tauri::AppHandle,
    state: Arc<AppState>,
    channel: String,
    args: Vec<Value>,
) -> RouteFuture {
    Box::pin(async move { crate::router::route(app, &state, &channel, &args).await })
}

async fn route_detached(
    app: &tauri::AppHandle,
    state: &Arc<AppState>,
    channel: &str,
    args: Vec<Value>,
) -> Value {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let future = route_future(app.clone(), state.clone(), channel.to_string(), args);
    tauri::async_runtime::spawn(async move {
        let _ = tx.send(future.await);
    });
    rx.await
        .unwrap_or_else(|_| crate::envelope::err("INTERNAL", "desktop operation did not complete"))
}

fn audit(plugin_id: &str, operation: &Operation, ok: bool, message: &str) {
    log::info!(
        target: "plugin.audit",
        "desktop plugin={plugin_id} operation={} risk={} ok={ok} {message}",
        operation.id,
        operation.risk.label(),
    );
}

/// Run one catalog operation for a plugin.
pub async fn invoke(
    app: &tauri::AppHandle,
    state: &Arc<AppState>,
    plugin_id: &str,
    session_id: Option<&str>,
    input: &Value,
) -> Result<Value, DesktopError> {
    let operation_id = input.get("operation").and_then(Value::as_str).unwrap_or("");
    let Some(operation) = find(operation_id) else {
        return Err(DesktopError::new(
            "NOT_FOUND",
            format!("operation is not exposed: {operation_id}"),
        ));
    };
    let raw_args = input
        .get("args")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let args: Vec<Value> = raw_args.iter().map(strip_secrets).collect();

    if operation.risk == Risk::Dangerous {
        if input.get("confirm").and_then(Value::as_bool) != Some(true) {
            audit(plugin_id, operation, false, "CONFIRMATION_REQUIRED");
            return Err(DesktopError::new(
                "CONFIRMATION_REQUIRED",
                "confirm: true is required for this desktop operation",
            ));
        }
        if !confirm_dangerous(app, plugin_id, operation, &args).await {
            audit(plugin_id, operation, false, "PERMISSION_DENIED");
            return Err(DesktopError::new(
                "PERMISSION_DENIED",
                "the user did not allow this desktop operation",
            ));
        }
    }

    let envelope = if operation.channel.starts_with("session.collaboration.") {
        // Host-core owns these methods; no renderer channel exists. Plugin and
        // session identity are host facts the caller cannot forge.
        let mut params = object_arg(&args);
        params.insert("pluginId".into(), json!(plugin_id));
        if let Some(session_id) = session_id {
            params
                .entry("sourceSessionId")
                .or_insert_with(|| json!(session_id));
        }
        if let Some(task) = params.remove("task") {
            params.entry("content").or_insert(task);
        }
        // `modelKey` is the public `provider/model` shape; host-core stores the
        // two ids separately.
        if let Some(key) = params
            .remove("modelKey")
            .and_then(|value| value.as_str().map(str::to_string))
        {
            if let Some((provider, model)) = key.split_once('/') {
                params.insert("providerId".into(), json!(provider));
                params.insert("modelId".into(), json!(model));
            }
        }
        if params
            .get("idempotencyKey")
            .and_then(Value::as_str)
            .map(str::is_empty)
            .unwrap_or(true)
        {
            params.insert(
                "idempotencyKey".into(),
                json!(uuid::Uuid::new_v4().to_string()),
            );
        }
        match state
            .host
            .call(operation.channel, Value::Object(params))
            .await
        {
            Ok(data) => json!({ "ok": true, "data": data }),
            Err(error) => crate::envelope::envelope_from_rpc("host", &error),
        }
    } else {
        let routed = routed_args(operation, &args)?;
        route_detached(app, state, operation.channel, routed).await
    };

    if envelope.get("ok").and_then(Value::as_bool) != Some(true) {
        let code = envelope
            .pointer("/error/code")
            .and_then(Value::as_str)
            .unwrap_or("INTERNAL");
        let message = envelope
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("desktop operation failed");
        audit(plugin_id, operation, false, code);
        return Err(DesktopError::new(code, message));
    }
    audit(plugin_id, operation, true, "ok");

    let data = envelope.get("data").cloned().unwrap_or(Value::Null);
    // A queued ledger message still needs an Agent Host action. Starting the
    // target turn is asynchronous so the plugin call returns as soon as the
    // durable delivery exists.
    if matches!(
        operation.channel,
        "session.collaboration.spawn" | "session.collaboration.send"
    ) {
        if let Some(message) = data.get("message").cloned() {
            crate::collaboration::deliver_async(app, state, message);
        }
    }
    if !matches!(operation.risk, Risk::Read) {
        let mut event = Map::new();
        let reason = match operation.id {
            "session/open" => "plugin.session.open",
            "session/create" => "plugin.session",
            "agent/prompt" => "plugin.prompt",
            _ => "plugin.desktop",
        };
        event.insert("reason".into(), json!(reason));
        if operation.id == "session/open" {
            if let Some(id) = data.pointer("/session/id").and_then(Value::as_str) {
                event.insert("selectSessionId".into(), json!(id));
            }
            if let Some(path) = data.pointer("/session/projectPath").and_then(Value::as_str) {
                event.insert("projectPath".into(), json!(path));
            }
        }
        let _ = app.emit("pi-desktop/session/event/changed", Value::Object(event));
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_covers_the_flow_and_keeps_collaboration_plugin_only() {
        let ids: Vec<&str> = OPERATIONS.iter().map(|operation| operation.id).collect();
        for expected in [
            "app/getVersion",
            "project/set",
            "session/create",
            "session/delete",
            "session/configure",
            "agent/prompt",
            "plans/resolve",
            "workspace/diff",
            "fs/read",
        ] {
            assert!(ids.contains(&expected), "{expected} is missing");
        }
        for excluded in ["plugin/loadDev", "providers/create", "settings/set"] {
            assert!(!ids.contains(&excluded), "{excluded} must stay out");
        }
        let collaboration = ids
            .iter()
            .filter(|id| id.starts_with("session/collaboration/"))
            .count();
        assert_eq!(collaboration, 6);
        assert_eq!(
            find("session/configure").map(|operation| operation.risk),
            Some(Risk::Dangerous)
        );
        assert_eq!(
            find("session/delete").map(|operation| operation.risk),
            Some(Risk::Dangerous)
        );
        assert_eq!(
            find("agent/status").map(|operation| operation.risk),
            Some(Risk::Read)
        );
    }

    #[test]
    fn secret_shaped_arguments_are_stripped_recursively() {
        let stripped = strip_secrets(&json!({
            "title": "ok",
            "secretValue": "sk-live",
            "apiKey": "x",
            "headers": { "Authorization": "Bearer x", "Accept": "application/json" },
            "env": { "PATH": "/bin", "OPENAI_API_KEY": "sk" },
            "nested": { "client_secret": "nope", "keep": 1 },
        }));
        assert_eq!(
            stripped,
            json!({
                "title": "ok",
                "headers": { "Accept": "application/json" },
                "env": { "PATH": "/bin" },
                "nested": { "keep": 1 },
            })
        );
    }

    #[test]
    fn positional_arguments_are_normalized_for_each_shape() {
        assert_eq!(
            routed_args(find("session/get").unwrap(), &[json!("s1")]).unwrap(),
            vec![json!({ "id": "s1" })]
        );
        assert_eq!(
            routed_args(find("agent/status").unwrap(), &[json!("s1")]).unwrap(),
            vec![json!({ "sessionId": "s1" })]
        );
        assert_eq!(
            routed_args(
                find("session/rename").unwrap(),
                &[json!("s1"), json!("New title")],
            )
            .unwrap(),
            vec![json!({ "id": "s1", "title": "New title" })]
        );
        assert_eq!(
            routed_args(find("fs/list").unwrap(), &[json!("src")]).unwrap(),
            vec![json!({ "path": "src" })]
        );
        assert_eq!(
            routed_args(
                find("fs/read").unwrap(),
                &[json!({ "path": "a.txt", "mimeType": "text/plain" })]
            )
            .unwrap(),
            vec![json!({ "path": "a.txt", "mimeType": "text/plain" })]
        );
        assert_eq!(
            routed_args(find("project/set").unwrap(), &[json!("/tmp/p")]).unwrap(),
            vec![json!("/tmp/p")]
        );
        assert!(routed_args(find("session/get").unwrap(), &[]).is_err());
    }

    #[test]
    fn the_catalog_hides_channel_names() {
        let rows = catalog();
        assert_eq!(rows.len(), OPERATIONS.len());
        for row in rows {
            assert!(row.get("channel").is_none());
            assert!(row.get("id").and_then(Value::as_str).is_some());
            assert!(row.get("description").and_then(Value::as_str).is_some());
            assert!(row.get("risk").and_then(Value::as_str).is_some());
        }
    }
}
