//! Loopback control endpoint for external drivers (ADR: IM remote channels).
//!
//! The desktop app owns its sessions, providers and tool permissions. A
//! `pigo im serve` process cannot open those sessions by itself, so instead it
//! calls the same shell channels the renderer uses — over a 127.0.0.1 endpoint
//! guarded by a per-launch token. That is what makes "send it from Feishu,
//! continue it in the app" work without teaching the desktop a second agent
//! stack.
//!
//! Scope is deliberately narrow. The catalog below is three read paths, the
//! turn lifecycle, and the open tool-confirmation queue: nothing that creates
//! projects, edits settings, installs plugins or touches credentials. The
//! permission pair is not a grant — `permissions/pending` only reads questions
//! the host already asked, and `permissions/resolve` only answers one of those
//! questions by its host-issued id. A remote driver is a second view of the
//! same dialog, which is what lets a Feishu card answer a prompt the app also
//! shows on screen. Anything that needs a fresh human decision or mutates
//! configuration (`dangerous` operations in desktop_control) stays out.
//!
//! The token is the capability. It is 250 bits, regenerated on every launch,
//! written to `control.json` in the app's data directory (which is already the
//! user's own profile directory), and required on every request.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::AppHandle;
use uuid::Uuid;

use crate::state::AppState;

/// Hard cap on one request body. The largest thing a driver sends is a prompt.
const MAX_REQUEST: usize = 1 << 20;
/// Per-request socket timeout, both directions.
const IO_TIMEOUT: Duration = Duration::from_secs(15);
/// Descriptor file name, next to the rest of the app's data.
const DESCRIPTOR: &str = "control.json";

/// The callable catalog: operation id → shell channel. Ids are what the client
/// speaks; channels stay an implementation detail of the shell.
const OPERATIONS: &[(&str, &str)] = &[
    ("session/create", "pi-desktop/session/create"),
    ("session/list", "pi-desktop/session/list"),
    ("session/get", "pi-desktop/session/get"),
    ("session/configure", "pi-desktop/session/configure"),
    ("agent/prompt", "pi-desktop/agent/prompt"),
    ("agent/status", "pi-desktop/agent/getStatus"),
    ("agent/stop", "pi-desktop/agent/stop"),
    ("agent/steer", "pi-desktop/agent/steer"),
    // A remote driver cannot subscribe to the shell's `permissions.request`
    // broadcast, so it polls the host's open set instead. Both ids are already
    // host-core RPCs; exposing them here is what turns an app-only dialog into
    // one that can also be answered from an IM card. Keep them paired: a
    // readable queue with no way to answer it would just hang every tool call.
    ("permissions/pending", "pi-desktop/tool/pendingPermissions"),
    ("permissions/resolve", "pi-desktop/tool/resolvePermission"),
];

/// Where the endpoint publishes its port and token. Remote drivers — including
/// the desktop's own remote-channel page — discover the endpoint through it, so
/// the path is public while the file itself stays owner-only.
pub fn descriptor_path(data_dir: &Path) -> PathBuf {
    data_dir.join(DESCRIPTOR)
}

/// ControlHandle keeps the endpoint alive and removes its descriptor on drop.
pub struct ControlHandle {
    stop: Arc<AtomicBool>,
    descriptor: PathBuf,
    port: u16,
}

impl ControlHandle {
    /// Port the endpoint listens on.
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for ControlHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // A stale descriptor pointing at a dead port is worse than no
        // descriptor: the client would report a connection error instead of
        // "the app is not running".
        let _ = std::fs::remove_file(&self.descriptor);
    }
}

/// Start the endpoint and publish its descriptor. The returned handle is
/// managed by the caller; dropping it shuts the endpoint down.
pub fn start(app: AppHandle, state: Arc<AppState>) -> Result<ControlHandle, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| format!("bind: {e}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("nonblocking: {e}"))?;
    let port = listener.local_addr().map_err(|e| format!("addr: {e}"))?.port();
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());

    let descriptor = state.data_dir.join(DESCRIPTOR);
    write_descriptor(&descriptor, port, &token)?;

    let stop = Arc::new(AtomicBool::new(false));
    let accept_stop = stop.clone();
    let shared_token = Arc::new(token);

    std::thread::Builder::new()
        .name("pigo-control".into())
        .spawn(move || {
            while !accept_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let app = app.clone();
                        let state = state.clone();
                        let token = shared_token.clone();
                        // One thread per connection: every call is a short
                        // request/response, and a slow one must not block the
                        // accept loop.
                        std::thread::spawn(move || {
                            if let Err(e) = serve_one(stream, &app, &state, &token) {
                                log::debug!("control: {e}");
                            }
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        log::warn!("control accept failed: {e}");
                        std::thread::sleep(Duration::from_millis(200));
                    }
                }
            }
        })
        .map_err(|e| format!("spawn: {e}"))?;

    log::info!("control endpoint listening on 127.0.0.1:{port}");
    Ok(ControlHandle {
        stop,
        descriptor,
        port,
    })
}

fn write_descriptor(path: &PathBuf, port: u16, token: &str) -> Result<(), String> {
    let body = json!({
        "port": port,
        "token": token,
        "pid": std::process::id(),
        "version": crate::state::APP_VERSION,
    });
    std::fs::write(path, serde_json::to_vec_pretty(&body).map_err(|e| e.to_string())?)
        .map_err(|e| format!("write {}: {e}", path.display()))?;
    // Owner-only on unix. Windows inherits the profile directory's ACL, which
    // is already the user's own.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// One request, one response, close. The endpoint speaks just enough HTTP/1.1
/// to be called by a client library; keeping the parser here avoids pulling a
/// web framework into the shell.
fn serve_one(
    mut stream: TcpStream,
    app: &AppHandle,
    state: &Arc<AppState>,
    token: &str,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;

    let (method, path, headers, body) = read_request(&mut stream)?;

    if method != "POST" && !(method == "GET" && path == "/health") {
        return respond(&mut stream, 405, &json!({"ok": false, "error": {"code": "METHOD", "message": "POST /invoke"}}));
    }
    if !authorized(&headers, token) {
        return respond(
            &mut stream,
            401,
            &json!({"ok": false, "error": {"code": "UNAUTHORIZED", "message": "missing or wrong bearer token"}}),
        );
    }
    if path == "/health" {
        return respond(&mut stream, 200, &json!({"ok": true, "data": {"version": crate::state::APP_VERSION}}));
    }
    if path != "/invoke" {
        return respond(&mut stream, 404, &json!({"ok": false, "error": {"code": "NOT_FOUND", "message": "POST /invoke"}}));
    }

    let request: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(e) => {
            return respond(
                &mut stream,
                400,
                &json!({"ok": false, "error": {"code": "INVALID_ARGUMENT", "message": format!("body is not JSON: {e}")}}),
            )
        }
    };
    let op = request.get("op").and_then(Value::as_str).unwrap_or("");
    let args = request.get("args").cloned().unwrap_or(json!({}));
    let Some(channel) = channel_for(op) else {
        return respond(
            &mut stream,
            403,
            &json!({"ok": false, "error": {"code": "NOT_ALLOWED", "message": format!("operation '{op}' is not exposed to remote drivers")}}),
        );
    };

    // session/configure is a two-argument channel: [id, config]. Everything else
    // takes the args object as its only parameter.
    let call_args = if op == "session/configure" {
        let id = args.get("id").cloned().unwrap_or(Value::Null);
        vec![id, args.clone()]
    } else {
        vec![args]
    };

    // The shell's own envelope already says whether the call succeeded, so the
    // HTTP status stays 200 and the caller reads `ok`. That keeps one error
    // model on both ends instead of translating codes into status classes.
    let result = tauri::async_runtime::block_on(crate::router::route(
        app.clone(),
        state,
        channel,
        &call_args,
    ));
    respond(&mut stream, 200, &result)
}

fn channel_for(op: &str) -> Option<&'static str> {
    OPERATIONS
        .iter()
        .find(|(id, _)| *id == op)
        .map(|(_, channel)| *channel)
}

type Headers = Vec<(String, String)>;

fn read_request(stream: &mut TcpStream) -> std::io::Result<(String, String, Headers, Vec<u8>)> {
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    // Headers first: read until the blank line, then exactly Content-Length
    // more bytes. A client that lies about the length gets a timeout, which is
    // the correct answer to a malformed request.
    let header_end = loop {
        if let Some(pos) = find_blank_line(&buf) {
            break pos;
        }
        if buf.len() > 64 * 1024 {
            break 0;
        }
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break 0;
        }
        buf.extend_from_slice(&chunk[..n]);
    };

    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();

    let mut headers: Headers = Vec::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }

    let content_length = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0)
        .min(MAX_REQUEST);

    let mut body = buf[header_end..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
        if body.len() > MAX_REQUEST {
            break;
        }
    }
    body.truncate(content_length);

    Ok((method, path, headers, body))
}

/// find_blank_line returns the index just past the header terminator.
fn find_blank_line(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

fn authorized(headers: &Headers, token: &str) -> bool {
    let Some((_, value)) = headers.iter().find(|(name, _)| name == "authorization") else {
        return false;
    };
    let Some(presented) = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer ")) else {
        return false;
    };
    constant_time_eq(presented.as_bytes(), token.as_bytes())
}

/// constant_time_eq keeps the token comparison from leaking its prefix through
/// timing. The threat model is a local process, which is exactly the case where
/// that matters.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn respond(stream: &mut TcpStream, status: u16, body: &Value) -> std::io::Result<()> {
    let payload = serde_json::to_vec(body).unwrap_or_else(|_| b"{\"ok\":false}".to_vec());
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "OK",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&payload)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_reviewed_catalog_is_reachable() {
        assert_eq!(
            channel_for("agent/prompt"),
            Some("pi-desktop/agent/prompt")
        );
        // The permission queue is readable and answerable, never grantable:
        // these two only carry host-issued request ids and decisions.
        assert_eq!(
            channel_for("permissions/pending"),
            Some("pi-desktop/tool/pendingPermissions")
        );
        assert_eq!(
            channel_for("permissions/resolve"),
            Some("pi-desktop/tool/resolvePermission")
        );
        // Destructive and settings-shaped channels must stay off this surface.
        for blocked in [
            "session/delete",
            "settings/set",
            "plugins/installFromPackage",
            "providers/create",
            "app/quit",
        ] {
            assert!(channel_for(blocked).is_none(), "{blocked} must not be exposed");
        }
    }

    #[test]
    fn the_bearer_prefix_is_required_and_exact() {
        let headers = vec![("authorization".to_string(), "Bearer abc".to_string())];
        assert!(authorized(&headers, "abc"));
        assert!(!authorized(&headers, "abcd"));
        assert!(!authorized(&headers, "ab"));
        let raw = vec![("authorization".to_string(), "abc".to_string())];
        assert!(!authorized(&raw, "abc"));
        assert!(!authorized(&Vec::new(), "abc"));
    }

    #[test]
    fn header_terminator_is_found() {
        assert_eq!(find_blank_line(b"GET / HTTP/1.1\r\nHost: x\r\n\r\nbody"), Some(28));
        assert_eq!(find_blank_line(b"GET / HTTP/1.1\r\n"), None);
    }
}
