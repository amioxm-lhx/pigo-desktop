//! Backend resolution and boot sequence, ported from the Electron
//! `runtime/host.ts` / `runtime/sidecar.ts` / `runtime/lifecycle.ts` flow:
//! spawn host → handshake (protocol 11) → recover inflight messages →
//! spawn sidecar → configure → supervise both.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use crate::rpc::{emit_host_status, RpcClient};
use crate::state::{AppState, PROTOCOL_VERSION};

pub fn resolve_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("PI_DESKTOP_DATA_DIR") {
        return PathBuf::from(dir);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".pi-agent-go-desktop")
}

fn workspace_root() -> PathBuf {
    // src-tauri → apps/desktop-tauri → apps → pi-go-desktop
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

/// Locate a backend binary that ships inside the installed application.
///
/// The macOS bundle ships both backends at `<App>.app/Contents/Resources/bin`
/// (`bundle.macOS.files`). Windows ships the same files under `bin/` next to
/// `pigo-desktop.exe` (see `tauri.windows.conf.json`). A build that used Tauri
/// external binaries keeps them directly next to the main executable, so that
/// layout is probed too. In a source checkout none of these exist and
/// resolution falls through to the workspace paths, preserving development.
fn packaged_binary(name: &str) -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let mut candidates = Vec::new();
    if let Some(contents) = exe_dir.parent() {
        candidates.push(contents.join("Resources").join("bin").join(name));
    }
    candidates.push(exe_dir.join("bin").join(name));
    candidates.push(exe_dir.join(name));
    candidates.into_iter().find(|candidate| candidate.exists())
}

pub fn resolve_host_binary() -> Option<PathBuf> {
    if let Some(bin) = std::env::var_os("PI_DESKTOP_HOST_BIN") {
        let path = PathBuf::from(bin);
        if path.exists() {
            return Some(path);
        }
    }
    let exe = format!("pi-desktop-host-core{}", std::env::consts::EXE_SUFFIX);
    if let Some(path) = packaged_binary(&exe) {
        return Some(path);
    }
    let root = workspace_root();
    for profile in ["debug", "release"] {
        let candidate = root.join("target").join(profile).join(&exe);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

pub fn resolve_sidecar_binary() -> Option<PathBuf> {
    if let Some(bin) = std::env::var_os("PI_DESKTOP_AGENT_BIN") {
        let path = PathBuf::from(bin);
        if path.exists() {
            return Some(path);
        }
    }
    let exe = format!("pi-go-desktop-agent{}", std::env::consts::EXE_SUFFIX);
    if let Some(path) = packaged_binary(&exe) {
        return Some(path);
    }
    let candidate = workspace_root().join("sidecar").join("build").join(&exe);
    candidate.exists().then_some(candidate)
}

async fn handshake(app: &tauri::AppHandle, host: &Arc<RpcClient>) -> Result<(), ()> {
    match host
        .call(
            "app.handshake",
            json!({ "protocolVersion": PROTOCOL_VERSION }),
        )
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            let schema_too_new = matches!(&e, crate::rpc::RpcError::Remote { data, .. }
                if data.get("errorCode").and_then(|v| v.as_str())
                    .is_some_and(|c| c.contains("SCHEMA")));
            emit_host_status(
                app,
                json!({
                    "ok": false,
                    "component": "host",
                    "fatal": true,
                    "message": e.to_string(),
                    "schema": if schema_too_new {
                        json!({ "found": schema_found(&e), "supported": 17 })
                    } else {
                        serde_json::Value::Null
                    },
                }),
            );
            host.stop_restart();
            Err(())
        }
    }
}

fn schema_found(e: &crate::rpc::RpcError) -> serde_json::Value {
    match e {
        crate::rpc::RpcError::Remote { data, .. } => data
            .get("found")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
        _ => serde_json::Value::Null,
    }
}

fn on_host_restart() -> crate::rpc::OnRestart {
    Arc::new(|host: Arc<RpcClient>, app: tauri::AppHandle| {
        Box::pin(async move {
            if handshake(&app, &host).await.is_ok() {
                let _ = host
                    .call("session.recoverInflightMessages", json!({}))
                    .await;
            }
        })
    })
}

pub async fn boot(app: tauri::AppHandle, state: Arc<AppState>) {
    // models.dev snapshot first: launch and the model pickers read it, and a
    // bundled read performs no network I/O (ADR 0134).
    let catalog = state.catalog.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || catalog.load_bundled()).await;

    // Host first: the sidecar is configured with the host binary path.
    match state.host.spawn().await {
        Ok(()) => {
            if handshake(&app, &state.host).await.is_ok() {
                // Drain host-side recovery before the sidecar can touch
                // sessions (mirrors the Electron boot order).
                let _ = state
                    .host
                    .call("session.recoverInflightMessages", json!({}))
                    .await;
            }
        }
        Err(e) => {
            emit_host_status(
                &app,
                json!({
                    "ok": false,
                    "component": "host",
                    "fatal": true,
                    "message": format!("host spawn failed: {e}"),
                }),
            );
            state.host.stop_restart();
        }
    }
    state.host.supervise(app.clone(), Some(on_host_restart()));

    // Plugin runtime: load enabled plugins once the registry and workspace
    // are reachable. A missing Node only disables the plugin surface.
    {
        let state = state.clone();
        tauri::async_runtime::spawn(async move {
            let project = state
                .host
                .call("workspace.get", json!({}))
                .await
                .ok()
                .and_then(|reply| {
                    reply
                        .pointer("/workspace/path")
                        .and_then(|path| path.as_str())
                        .map(String::from)
                });
            crate::plugin_host::sync(&state, project.as_deref()).await;
        });
    }

    // Sidecar.
    match state.sidecar.spawn().await {
        Ok(()) => {
            let configure = json!({
                "hostBinary": state.host.binary.to_string_lossy(),
                "dataDir": state.data_dir.to_string_lossy(),
                "networkProxy": serde_json::Value::Null,
            });
            if let Err(e) = state.sidecar.call("sidecar.configure", configure).await {
                log::warn!("sidecar.configure failed: {e}");
                emit_host_status(
                    &app,
                    json!({ "ok": false, "component": "sidecar", "message": e.to_string() }),
                );
            }
        }
        Err(e) => {
            emit_host_status(
                &app,
                json!({ "ok": false, "component": "sidecar", "fatal": true, "message": format!("sidecar spawn failed: {e}") }),
            );
            state.sidecar.stop_restart();
        }
    }
    state.sidecar.supervise(app.clone(), None);

    // The Electron main waits for renderer readiness before the first
    // hostStatus; the Tauri shell emits shortly after boot instead — the
    // renderer also subscribes late, so re-emit on an interval until the
    // window exists (cheap; payload is tiny).
    tokio::time::sleep(Duration::from_millis(300)).await;
    emit_host_status(&app, json!({ "ok": state.host.is_available() }));
}
