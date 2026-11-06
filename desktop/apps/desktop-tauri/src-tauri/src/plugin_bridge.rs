//! Panel bridge: host services reachable from a plugin panel/view page.
//!
//! The injected `window.pluginBridge` forwards every call to the renderer,
//! which forwards it here as `pi-desktop/plugin/bridge/invoke`. Keeping the
//! dispatch in Rust means the permission matrix, filesystem scopes and egress
//! allowlist are enforced on one boundary; plugin markup never touches a Tauri
//! channel directly.
//!
//! A capability the shell cannot host reports `UNSUPPORTED` instead of a fake
//! success. The browser guest is hosted by the shell and reached through this
//! same boundary, including its platform snapshot bridge.

use std::collections::HashSet;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use regex::RegexBuilder;
use serde_json::{json, Map, Value};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::plugins;
use crate::state::{AppState, APP_VERSION};

const TEXT_PREVIEW_MAX: u64 = 512 * 1024;
const IMAGE_PREVIEW_MAX: u64 = 8 * 1024 * 1024;
const READ_TEXT_MAX: u64 = 8 * 1024 * 1024;
const READ_RANGE_MAX: u64 = 4 * 1024 * 1024;
const GLOB_MAX_RESULTS: usize = 5000;
const GLOB_MAX_DEPTH: usize = 12;
const NET_BODY_MAX: u64 = 2 * 1024 * 1024;

const DENY_DIRS: &[&str] = &[".git", ".ssh", ".aws", ".gnupg", ".kube", ".docker"];

#[derive(Debug)]
pub(crate) struct BridgeError {
    pub(crate) code: String,
    pub(crate) message: String,
}

impl BridgeError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

type BridgeResult<T> = Result<T, BridgeError>;

pub async fn invoke(app: &AppHandle, state: &Arc<AppState>, params: &Value) -> Value {
    let plugin_id = params
        .get("pluginId")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let channel = params
        .get("channel")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if plugin_id.is_empty() || channel.is_empty() {
        return envelope_error(BridgeError::new(
            "INVALID_ARGUMENT",
            "pluginId and channel are required",
        ));
    }
    let payload = params.get("payload").cloned().unwrap_or(Value::Null);
    let session_id = params
        .get("sessionId")
        .and_then(|value| value.as_str())
        .map(str::to_string);

    let project = plugins::current_project_path(state).await;
    let rows = match plugins::plugin_rows(state).await {
        Ok(rows) => rows,
        Err(error) => return error,
    };
    let Some(plugin) = plugins::find_plugin(&rows, plugin_id) else {
        return envelope_error(BridgeError::new(
            "PLUGIN_NOT_FOUND",
            format!("unknown plugin \"{plugin_id}\""),
        ));
    };
    if !plugins::active_in_project(plugin, project.as_deref()) {
        return envelope_error(BridgeError::new(
            "PLUGIN_NOT_ACTIVE",
            format!("plugin \"{plugin_id}\" is not active"),
        ));
    }
    let Some(manifest) = plugins::read_manifest(plugin) else {
        return envelope_error(BridgeError::new(
            "PLUGIN_INVALID",
            "manifest.json is unreadable",
        ));
    };

    match dispatch(
        app,
        state,
        plugin_id,
        plugin,
        &manifest,
        channel,
        &payload,
        session_id.as_deref(),
    )
    .await
    {
        Ok(result) => json!({ "ok": true, "data": result }),
        Err(error) => envelope_error(error),
    }
}

/// Invoke one channel for a plugin whose row and manifest the caller already
/// holds. The Node host uses this path so a plugin tool can reach the same
/// permission/scope boundary without re-syncing the plugin registry (which
/// would recurse through the host request handler).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn invoke_loaded(
    app: &AppHandle,
    state: &Arc<AppState>,
    plugin_id: &str,
    plugin: &Value,
    manifest: &Value,
    channel: &str,
    payload: &Value,
    session_id: Option<&str>,
) -> Value {
    match dispatch(
        app, state, plugin_id, plugin, manifest, channel, payload, session_id,
    )
    .await
    {
        Ok(result) => json!({ "ok": true, "data": result }),
        Err(error) => envelope_error(error),
    }
}

fn envelope_error(error: BridgeError) -> Value {
    json!({
        "ok": false,
        "error": { "code": error.code, "message": error.message }
    })
}

fn require(manifest: &Value, permission: &str) -> BridgeResult<()> {
    if plugins::has_permission(manifest, permission) {
        Ok(())
    } else {
        Err(BridgeError::new(
            "PLUGIN_PERMISSION_DENIED",
            format!("{permission} is not granted"),
        ))
    }
}

#[allow(clippy::too_many_arguments)]
async fn dispatch(
    app: &AppHandle,
    state: &Arc<AppState>,
    plugin_id: &str,
    plugin: &Value,
    manifest: &Value,
    channel: &str,
    payload: &Value,
    session_id: Option<&str>,
) -> BridgeResult<Value> {
    match channel {
        "app.getVersion" => Ok(json!(APP_VERSION)),
        "app.getLocale" => Ok(json!(plugins::system_locale())),
        "app.getAppearance" => appearance(state).await,
        "app.setTheme" => set_theme(state, manifest, payload).await,
        "plugin.getId" => Ok(json!(plugin_id)),
        "plugin.getSettings" => Ok(json!(plugin_settings(manifest, plugin_id, state))),
        "plugin.setSettings" => {
            let values = payload
                .get("values")
                .and_then(|value| value.as_object())
                .cloned()
                .unwrap_or_default();
            let resolved =
                plugins::apply_plugin_settings(&state.data_dir, plugin_id, manifest, &values)
                    .map_err(|message| BridgeError::new("PLUGIN_INVALID", message))?;
            state.plugins.push_settings(plugin_id, &resolved).await;
            Ok(json!(null))
        }
        "plugin.getDataPath" => {
            let path = state.data_dir.join("plugin-data").join(plugin_id);
            fs::create_dir_all(&path)
                .map_err(|error| BridgeError::new("INTERNAL", error.to_string()))?;
            Ok(json!(path.to_string_lossy()))
        }
        "ui.showToast" => {
            let message = payload
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if !message.is_empty() {
                emit(
                    state,
                    "pi-desktop/app/event/toast",
                    json!({ "message": message }),
                );
            }
            Ok(json!(null))
        }
        "ui.notify" | "ui.showNativeNotification" => {
            require(manifest, "notify")?;
            let title = payload
                .get("title")
                .and_then(|value| value.as_str())
                .unwrap_or("pigo-desktop");
            let body = payload
                .get("body")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            crate::native::show_notification(app, title, body);
            Ok(json!({ "shown": true, "permission": "granted" }))
        }
        "ui.getNotificationPermission" | "ui.requestNotificationPermission" => {
            require(manifest, "notify")?;
            Ok(json!("granted"))
        }
        "ui.openPanel" => {
            require(manifest, "ui.panel")?;
            open_panel(app, state, plugin, manifest, payload)
        }
        "ui.closePanel" => {
            emit(
                state,
                crate::plugin_surface::PANEL_EVENT,
                json!({ "action": "close", "pluginId": plugin_id }),
            );
            Ok(json!(null))
        }
        "themes.upsert" => {
            require(manifest, "ui.theme")?;
            let theme = payload.get("theme").cloned().unwrap_or(Value::Null);
            state
                .plugins
                .upsert_theme(plugin_id, &theme)
                .map(|summary| summary.get("theme").cloned().unwrap_or(Value::Null))
                .map_err(host_error)
        }
        "themes.remove" => {
            require(manifest, "ui.theme")?;
            let theme_id = payload
                .get("themeId")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            state
                .plugins
                .remove_theme(plugin_id, theme_id)
                .map(|_| Value::Null)
                .map_err(host_error)
        }
        "themes.list" => {
            require(manifest, "ui.theme")?;
            Ok(json!(state.plugins.list_themes(plugin_id)))
        }
        "fs.readText" => fs_read_text(state, plugin_id, plugin, manifest, payload).await,
        "fs.stat" => fs_stat(state, plugin_id, plugin, manifest, payload).await,
        "fs.readRange" => fs_read_range(state, plugin_id, plugin, manifest, payload).await,
        "fs.readPreview" => fs_read_preview(state, plugin_id, plugin, manifest, payload).await,
        "fs.openDefault" => fs_open(app, state, plugin_id, plugin, manifest, payload, false).await,
        "fs.reveal" => fs_open(app, state, plugin_id, plugin, manifest, payload, true).await,
        "fs.writeText" => fs_write_text(state, plugin_id, plugin, manifest, payload).await,
        "fs.glob" => fs_glob(state, plugin_id, plugin, manifest, payload).await,
        "fs.list" => fs_list(state, plugin_id, plugin, manifest, payload).await,
        "fs.remove" => fs_remove(state, plugin_id, plugin, manifest, payload).await,
        "fs.requestDirectory" => fs_request_directory(app, state, plugin_id, manifest).await,
        "net.fetch" => net_fetch(manifest, payload).await,
        "clipboard.readText" => {
            require(manifest, "clipboard.read")?;
            clipboard_read_text().map(|text| json!(text))
        }
        "clipboard.writeText" => {
            require(manifest, "clipboard.write")?;
            let text = payload
                .get("text")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            clipboard_write_text(text)?;
            state.plugins.record_clipboard_text(text);
            Ok(Value::Null)
        }
        "clipboard.getHistory" => {
            require(manifest, "clipboard.read")?;
            Ok(state.plugins.clipboard_history())
        }
        "shell.openExternal" => {
            require(manifest, "shell.openExternal")?;
            let url = payload
                .get("url")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            open_external(app, url)
        }
        // Reviewed desktop catalog (ADR 0203/0208). The gateway delegates to
        // the same channel router the renderer uses; dangerous operations add
        // a native user consent step inside the controller.
        "desktop.listOperations" => {
            require(manifest, "desktop.control")?;
            Ok(json!(crate::desktop_control::catalog()))
        }
        "desktop.invoke" => {
            require(manifest, "desktop.control")?;
            crate::desktop_control::invoke(app, state, plugin_id, session_id, payload)
                .await
                .map_err(|error| BridgeError::new(error.code, error.message))
        }
        // The browser guest is host-owned (ADR 0170). Every call is gated on
        // `browser.cdp`; bounds are clamped by the host, not by the caller.
        "browser.navigate" => {
            require(manifest, "browser.cdp")?;
            state
                .browser
                .navigate(state, app, payload)
                .await
                .map_err(browser_error)
        }
        "browser.action" => {
            require(manifest, "browser.cdp")?;
            let action = payload.get("action").and_then(Value::as_str).unwrap_or("");
            state.browser.action(app, action).map_err(browser_error)
        }
        "browser.setBounds" => {
            require(manifest, "browser.cdp")?;
            state
                .browser
                .set_bounds(app, payload)
                .map_err(browser_error)
        }
        "browser.setVisible" => {
            require(manifest, "browser.cdp")?;
            let visible = payload
                .as_bool()
                .or_else(|| payload.get("visible").and_then(Value::as_bool))
                .unwrap_or(true);
            state.browser.set_visible(visible).map_err(browser_error)
        }
        "browser.getState" => {
            require(manifest, "browser.cdp")?;
            Ok(state.browser.state().unwrap_or(Value::Null))
        }
        "browser.openExternal" => {
            require(manifest, "browser.cdp")?;
            let url = state
                .browser
                .state()
                .and_then(|state| state.get("url").and_then(Value::as_str).map(str::to_string))
                .ok_or_else(|| BridgeError::new("BROWSER_UNAVAILABLE", "no page is open"))?;
            open_external(app, &url)
        }
        "browser.snapshot" => {
            require(manifest, "browser.cdp")?;
            state.browser.snapshot().await.map_err(browser_error)
        }
        "browser.screenshot" => {
            require(manifest, "browser.cdp")?;
            let full_page = payload.get("fullPage").and_then(Value::as_bool).unwrap_or(false);
            state
                .browser
                .screenshot(full_page)
                .await
                .map_err(browser_error)
        }
        "browser.click" => {
            require(manifest, "browser.cdp")?;
            let uid = payload.get("uid").and_then(Value::as_str).unwrap_or("");
            state.browser.click(uid).await.map_err(browser_error)
        }
        "browser.fill" => {
            require(manifest, "browser.cdp")?;
            let uid = payload.get("uid").and_then(Value::as_str).unwrap_or("");
            let text = payload.get("text").and_then(Value::as_str).unwrap_or("");
            state.browser.fill(uid, text).await.map_err(browser_error)
        }
        "browser.evaluate" => {
            require(manifest, "browser.cdp")?;
            let expression = payload
                .get("expression")
                .and_then(Value::as_str)
                .unwrap_or("");
            state
                .browser
                .evaluate(expression)
                .await
                .map_err(browser_error)
        }
        "browser.console" => {
            require(manifest, "browser.cdp")?;
            let limit = payload.get("limit").and_then(Value::as_u64).unwrap_or(100) as usize;
            state.browser.console(limit).await.map_err(browser_error)
        }
        "browser.cdp" => {
            require(manifest, "browser.cdp")?;
            let method = payload.get("method").and_then(Value::as_str).unwrap_or("");
            let params = payload.get("params").cloned().unwrap_or(json!({}));
            state
                .browser
                .cdp(method, &params)
                .await
                .map_err(browser_error)
        }
        "project.create" => project_create(state, manifest, payload).await,
        "models.list" => models_list(state, manifest).await,
        "agent.complete" => agent_complete(state, plugin_id, manifest, payload, session_id).await,
        "session.getLlmContext" => session_context(state, manifest, session_id).await,
        "session.import" => {
            let source = manifest_source(manifest, payload)?;
            session_call(
                state,
                plugin_id,
                manifest,
                "plugin.session.import",
                payload,
                Some(source),
            )
            .await
        }
        "session.importBatch" => {
            let source = manifest_source(manifest, payload)?;
            session_call(
                state,
                plugin_id,
                manifest,
                "plugin.session.importBatch",
                payload,
                Some(source),
            )
            .await
        }
        "session.list" => {
            session_permission(manifest, "session.read", "session.read.own")?;
            session_call(
                state,
                plugin_id,
                manifest,
                "plugin.session.list",
                payload,
                None,
            )
            .await
        }
        "session.get" => {
            session_permission(manifest, "session.read", "session.read.own")?;
            session_call(
                state,
                plugin_id,
                manifest,
                "plugin.session.get",
                payload,
                None,
            )
            .await
        }
        "session.listMessages" => {
            session_permission(manifest, "session.read", "session.read.own")?;
            session_call(
                state,
                plugin_id,
                manifest,
                "plugin.session.listMessages",
                payload,
                None,
            )
            .await
        }
        "session.rename" => {
            require(manifest, "session.update.own")?;
            session_call(
                state,
                plugin_id,
                manifest,
                "plugin.session.rename",
                payload,
                None,
            )
            .await
        }
        "session.delete" => {
            require(manifest, "session.delete.own")?;
            session_call(
                state,
                plugin_id,
                manifest,
                "plugin.session.delete",
                payload,
                None,
            )
            .await
        }
        other => Err(BridgeError::new(
            "PLUGIN_BRIDGE_CHANNEL_UNSUPPORTED",
            format!("panel bridge channel {other} is not hosted"),
        )),
    }
}

fn host_error(error: crate::plugin_host::HostError) -> BridgeError {
    BridgeError::new(error.code, error.message)
}

fn browser_error(error: crate::browser::BrowserError) -> BridgeError {
    BridgeError::new(error.code, error.message)
}

fn emit(state: &AppState, channel: &str, payload: Value) {
    if let Some(app) = state.plugins.app_handle() {
        use tauri::Emitter;
        let _ = app.emit(channel, payload);
    }
}

async fn appearance(state: &Arc<AppState>) -> BridgeResult<Value> {
    let settings = state
        .host
        .call("settings.get", json!({}))
        .await
        .map_err(|error| BridgeError::new("HOST_UNAVAILABLE", error.to_string()))?;
    let theme = settings
        .get("theme")
        .and_then(|value| value.as_str())
        .unwrap_or("system")
        .to_string();
    let plugin_theme = if theme.starts_with("plugin:") {
        match state.plugins.find_runtime_theme(&theme) {
            Some(entry) => Some(entry),
            None => manifest_theme(state, &theme).await,
        }
    } else {
        None
    };
    let base = match &plugin_theme {
        Some(entry) => entry
            .get("base")
            .and_then(|value| value.as_str())
            .unwrap_or("dark")
            .to_string(),
        None => match theme.as_str() {
            "light" => "light".to_string(),
            "dark" => "dark".to_string(),
            _ => "system".to_string(),
        },
    };
    let plugin_theme = plugin_theme.map(|entry| {
        json!({
            "id": entry.get("id").cloned().unwrap_or(Value::Null),
            "base": entry.get("base").cloned().unwrap_or(Value::Null),
            "css": entry.get("css").cloned().unwrap_or(Value::Null),
        })
    });
    Ok(json!({
        "theme": theme,
        "base": base,
        "locale": plugins::system_locale(),
        "pluginTheme": plugin_theme,
    }))
}

/// Resolve a manifest-contributed theme by its full `plugin:<id>:<theme>` id.
async fn manifest_theme(state: &AppState, theme_id: &str) -> Option<Value> {
    let rest = theme_id.strip_prefix("plugin:")?;
    let (owner, local) = rest.split_once(':')?;
    let rows = plugins::plugin_rows(state).await.ok()?;
    let plugin = plugins::find_plugin(&rows, owner)?;
    if !plugin
        .get("enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return None;
    }
    let manifest = plugins::read_manifest(plugin)?;
    let root = plugins::plugin_root(plugin)?;
    for entry in plugins::contributions(&manifest, "themes") {
        if entry.get("id").and_then(|value| value.as_str()) != Some(local) {
            continue;
        }
        let label = entry
            .get("label")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if label.trim().is_empty() {
            continue;
        }
        let path = entry.get("path").and_then(|value| value.as_str())?;
        let resolved = plugins::safe_join(&root, path)?;
        let raw = fs::read_to_string(resolved).ok()?;
        let css = plugins::sanitize_theme_css(&raw).ok()?;
        let base = if entry.get("base").and_then(|value| value.as_str()) == Some("light") {
            "light"
        } else {
            "dark"
        };
        return Some(json!({
            "id": theme_id,
            "pluginId": owner,
            "themeId": local,
            "label": label,
            "base": base,
            "css": css,
        }));
    }
    None
}

async fn set_theme(
    state: &Arc<AppState>,
    manifest: &Value,
    payload: &Value,
) -> BridgeResult<Value> {
    require(manifest, "ui.theme")?;
    let theme_id = payload
        .get("themeId")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let known = matches!(theme_id, "system" | "light" | "dark")
        || state.plugins.find_runtime_theme(theme_id).is_some()
        || manifest_theme(state, theme_id).await.is_some();
    if !known {
        return Err(BridgeError::new(
            "THEME_NOT_FOUND",
            format!("theme {theme_id} is not registered"),
        ));
    }
    let mut settings = state
        .host
        .call("settings.get", json!({}))
        .await
        .map_err(|error| BridgeError::new("HOST_UNAVAILABLE", error.to_string()))?
        .as_object()
        .cloned()
        .unwrap_or_default();
    settings.insert("theme".into(), json!(theme_id));
    state
        .host
        .call("settings.set", Value::Object(settings))
        .await
        .map_err(|error| BridgeError::new("HOST_UNAVAILABLE", error.to_string()))?;
    emit(
        state,
        "pi-desktop/app/event/settingsChanged",
        json!({ "theme": theme_id }),
    );
    Ok(json!(null))
}

fn plugin_settings(manifest: &Value, plugin_id: &str, state: &AppState) -> Map<String, Value> {
    let mut values = Map::new();
    for definition in plugins::contributions(manifest, "settings") {
        let Some(key) = definition.get("key").and_then(|value| value.as_str()) else {
            continue;
        };
        if let Some(default) = definition.get("default") {
            values.insert(key.to_string(), default.clone());
        }
    }
    for (key, value) in plugins::stored_plugin_values(&state.data_dir, plugin_id) {
        values.insert(key, value);
    }
    values
}

fn open_panel(
    _app: &AppHandle,
    state: &Arc<AppState>,
    plugin: &Value,
    manifest: &Value,
    payload: &Value,
) -> BridgeResult<Value> {
    let fallback = plugins::plugin_name(plugin);
    let meta = plugins::panel_meta(manifest, fallback).ok_or_else(|| {
        BridgeError::new(
            "PLUGIN_PANEL_NOT_DECLARED",
            "manifest.ui.panel is not declared",
        )
    })?;
    let Some(root) = plugins::plugin_root(plugin) else {
        return Err(BridgeError::new("PLUGIN_INVALID", "plugin path is missing"));
    };
    let entry = plugins::safe_join(&root, &meta.entry)
        .ok_or_else(|| BridgeError::new("PLUGIN_INVALID", "panel entry escapes the package"))?;
    let surfaces = state.surfaces.as_ref().ok_or_else(|| {
        BridgeError::new(
            "PLUGIN_SURFACE_UNAVAILABLE",
            "the plugin surface service could not start",
        )
    })?;
    let url = surfaces
        .register(plugins::plugin_id(plugin), &root, &entry)
        .ok_or_else(|| {
            BridgeError::new(
                "PLUGIN_PANEL_NOT_DECLARED",
                "manifest.ui.panel is not a readable file",
            )
        })?;
    let title = payload
        .get("title")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or(meta.title.as_str())
        .to_string();
    emit(
        state,
        crate::plugin_surface::PANEL_EVENT,
        json!({
            "action": "open",
            "pluginId": plugins::plugin_id(plugin),
            "url": url,
            "title": title,
            "width": meta.width,
            "height": meta.height,
        }),
    );
    Ok(json!(null))
}

// ── fs policy ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum FsMode {
    Read,
    Write,
    Delete,
}

impl FsMode {
    fn name(self) -> &'static str {
        match self {
            FsMode::Read => "read",
            FsMode::Write => "write",
            FsMode::Delete => "delete",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FsRoot {
    Workspace,
    UserSelected,
}

#[derive(Clone)]
struct FsRule {
    root: FsRoot,
    scope: Vec<String>,
    own: bool,
}

#[derive(Clone, Default)]
struct FsPolicy {
    read: Option<FsRule>,
    write: Option<FsRule>,
    delete: Option<FsRule>,
}

impl FsPolicy {
    fn rule(&self, mode: FsMode) -> Option<&FsRule> {
        match mode {
            FsMode::Read => self.read.as_ref(),
            FsMode::Write => self.write.as_ref(),
            FsMode::Delete => self.delete.as_ref(),
        }
    }
}

fn fs_access(plugin: &Value, manifest: &Value) -> BridgeResult<(HashSet<String>, FsPolicy)> {
    let mut permissions: HashSet<String> = plugin
        .get("permissions")
        .and_then(|value| value.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let legacy_read = permissions.contains("fs.read.workspace");
    let legacy_write = permissions.contains("fs.write.workspace");
    let legacy_delete = permissions.contains("fs.delete.workspace");
    let legacy = [
        ("fs.read.workspace", FsMode::Read),
        ("fs.write.workspace", FsMode::Write),
        ("fs.delete.workspace", FsMode::Delete),
    ];
    for (name, mode) in legacy {
        if permissions.contains(name) {
            permissions.remove(name);
            permissions.insert(format!("fs.{}", mode.name()));
        }
    }
    let mut policy = FsPolicy::default();
    if let Some(raw) = manifest.get("fs") {
        let Some(entry) = raw.as_object() else {
            return Err(BridgeError::new(
                "PLUGIN_INVALID",
                "manifest.fs must be an object",
            ));
        };
        for (key, value) in entry {
            let mode = match key.as_str() {
                "read" => FsMode::Read,
                "write" => FsMode::Write,
                "delete" => FsMode::Delete,
                other => {
                    return Err(BridgeError::new(
                        "PLUGIN_INVALID",
                        format!("fs.{other} is not a recognized mode"),
                    ))
                }
            };
            let rule = parse_fs_rule(value, mode)?;
            match mode {
                FsMode::Read => policy.read = Some(rule),
                FsMode::Write => policy.write = Some(rule),
                FsMode::Delete => policy.delete = Some(rule),
            }
        }
    }
    // Legacy names without an explicit rule keep their historical meaning.
    // A new-style permission without a scoped rule gets nothing: the scope
    // block is what says where the plugin may act.
    if policy.read.is_none() && legacy_read {
        policy.read = Some(FsRule {
            root: FsRoot::Workspace,
            scope: vec!["**/*".into()],
            own: false,
        });
    }
    if policy.delete.is_none() && legacy_delete {
        policy.delete = Some(FsRule {
            root: FsRoot::Workspace,
            scope: Vec::new(),
            own: true,
        });
    }
    let _ = legacy_write;
    Ok((permissions, policy))
}

fn parse_fs_rule(raw: &Value, mode: FsMode) -> BridgeResult<FsRule> {
    let Some(entry) = raw.as_object() else {
        return Err(BridgeError::new(
            "PLUGIN_INVALID",
            format!("fs.{} must be an object", mode.name()),
        ));
    };
    let root = match entry.get("root").and_then(|value| value.as_str()) {
        Some("userSelected") => FsRoot::UserSelected,
        Some("workspace") | None => FsRoot::Workspace,
        Some(_) => {
            return Err(BridgeError::new(
                "PLUGIN_INVALID",
                format!("fs.{}.root must be workspace or userSelected", mode.name()),
            ))
        }
    };
    let mut scope = Vec::new();
    if let Some(raw_scope) = entry.get("scope") {
        let Some(items) = raw_scope.as_array() else {
            return Err(BridgeError::new(
                "PLUGIN_INVALID",
                format!("fs.{}.scope must be an array", mode.name()),
            ));
        };
        for item in items {
            let Some(pattern) = item.as_str() else {
                return Err(BridgeError::new(
                    "PLUGIN_INVALID",
                    format!("fs.{}.scope entries must be strings", mode.name()),
                ));
            };
            let normalized = normalize_scope(pattern).ok_or_else(|| {
                BridgeError::new(
                    "PLUGIN_INVALID",
                    format!(
                        "fs.{}.scope entry is not root-relative: {pattern}",
                        mode.name()
                    ),
                )
            })?;
            if mode != FsMode::Read && is_whole_tree(&normalized) {
                return Err(BridgeError::new(
                    "PLUGIN_INVALID",
                    format!(
                        "fs.{}.scope must not cover the whole root: {pattern}",
                        mode.name()
                    ),
                ));
            }
            if !scope.contains(&normalized) {
                scope.push(normalized);
            }
        }
    }
    let own = entry
        .get("own")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if own && mode != FsMode::Delete {
        return Err(BridgeError::new(
            "PLUGIN_INVALID",
            format!("fs.{} does not support own", mode.name()),
        ));
    }
    Ok(FsRule { root, scope, own })
}

fn normalize_scope(raw: &str) -> Option<String> {
    let value = raw.trim().replace('\\', "/");
    if value.is_empty() || value.starts_with('/') || value.contains(':') {
        return None;
    }
    let segments: Vec<&str> = value
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect();
    if segments.is_empty() || segments.iter().any(|segment| *segment == "..") {
        return None;
    }
    Some(segments.join("/"))
}

fn is_whole_tree(pattern: &str) -> bool {
    let value = pattern.trim();
    !value.is_empty() && value.replace(['*', '/', '.'], "").is_empty()
}

fn glob_regex_source(pattern: &str) -> String {
    let mut source = String::new();
    let mut index = 0;
    let bytes: Vec<char> = pattern.chars().collect();
    while index < bytes.len() {
        let ch = bytes[index];
        if ch == '*' && index + 1 < bytes.len() && bytes[index + 1] == '*' {
            let at_segment_start = index == 0 || bytes[index - 1] == '/';
            let rest: String = bytes[index + 2..].iter().collect();
            if at_segment_start && rest.starts_with('/') {
                source.push_str("(?:.*/)?");
                index += 3;
                continue;
            }
            if at_segment_start && rest.is_empty() && index > 0 {
                source.pop();
                source.push_str("(?:/.*)?");
                index += 2;
                continue;
            }
            source.push_str(".*");
            index += 2;
            continue;
        }
        if ch == '*' {
            source.push_str("[^/]*");
        } else if ch == '?' {
            source.push_str("[^/]");
        } else {
            source.push_str(&regex::escape(&ch.to_string()));
        }
        index += 1;
    }
    source
}

fn match_glob(path: &str, pattern: &str) -> bool {
    let pattern = pattern.trim_end_matches('/');
    let pattern = if pattern.is_empty() { "/**" } else { pattern };
    if is_whole_tree(pattern) {
        return true;
    }
    let source = format!("^{}$", glob_regex_source(pattern));
    let ignore_case = cfg!(any(target_os = "macos", windows));
    RegexBuilder::new(&source)
        .case_insensitive(ignore_case)
        .build()
        .map(|regex| regex.is_match(path))
        .unwrap_or(false)
}

fn denied_relative(relative: &str) -> bool {
    let segments: Vec<&str> = relative
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if segments.is_empty() {
        return false;
    }
    if segments.iter().any(|segment| {
        let lower = segment.to_lowercase();
        DENY_DIRS.iter().any(|denied| *denied == lower)
    }) {
        return true;
    }
    let basename = segments.last().copied().unwrap_or("").to_lowercase();
    basename == ".env"
        || basename.starts_with(".env.")
        || matches!(
            basename.as_str(),
            ".npmrc" | ".netrc" | ".pypirc" | ".git-credentials"
        )
        || basename.starts_with("id_rsa")
        || basename.starts_with("id_dsa")
        || basename.starts_with("id_ecdsa")
        || basename.starts_with("id_ed25519")
        || basename.ends_with(".pem")
        || basename.ends_with(".p12")
        || basename.ends_with(".pfx")
        || basename.ends_with(".keystore")
}

fn path_arg(payload: &Value, key: &str) -> BridgeResult<String> {
    let raw = payload
        .get(key)
        .or_else(|| payload.get("path"))
        .or_else(|| payload.get("file_path"))
        .or_else(|| payload.get("pathFromRoot"))
        .and_then(|value| value.as_str())
        .or_else(|| payload.as_str())
        .unwrap_or("");
    normalize_relative_path(raw)
}

fn normalize_relative_path(raw: &str) -> BridgeResult<String> {
    let value = raw.trim().replace('\\', "/");
    if value.contains('\0') {
        return Err(BridgeError::new(
            "PLUGIN_FS_INVALID",
            "path contains a null byte",
        ));
    }
    if value.starts_with('/') || value.contains(':') {
        return Err(BridgeError::new(
            "PLUGIN_FS_INVALID",
            "path must be relative to the root",
        ));
    }
    let segments: Vec<&str> = value
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect();
    if segments.iter().any(|segment| *segment == "..") {
        return Err(BridgeError::new(
            "PLUGIN_FS_INVALID",
            "path must not contain ..",
        ));
    }
    Ok(segments.join("/"))
}

async fn fs_root(state: &Arc<AppState>, plugin_id: &str, rule: &FsRule) -> BridgeResult<PathBuf> {
    match rule.root {
        FsRoot::Workspace => {
            let reply = state
                .host
                .call("workspace.get", json!({}))
                .await
                .map_err(|error| BridgeError::new("HOST_UNAVAILABLE", error.to_string()))?;
            let Some(path) = reply
                .pointer("/workspace/path")
                .and_then(|value| value.as_str())
            else {
                return Err(BridgeError::new(
                    "PLUGIN_FS_NO_WORKSPACE",
                    "a workspace is required for this fs rule",
                ));
            };
            fs::canonicalize(path)
                .map_err(|error| BridgeError::new("PLUGIN_FS_NO_WORKSPACE", error.to_string()))
        }
        FsRoot::UserSelected => state.plugins.fs_grant(plugin_id).ok_or_else(|| {
            BridgeError::new(
                "PLUGIN_FS_SCOPE_DENIED",
                "call fs.requestDirectory before using a userSelected rule",
            )
        }),
    }
}

fn resolve_existing(root: &Path, relative: &str) -> BridgeResult<PathBuf> {
    let target = root.join(relative);
    let canonical = fs::canonicalize(&target)
        .map_err(|error| BridgeError::new("PLUGIN_FS_NOT_FOUND", error.to_string()))?;
    if !canonical.starts_with(root) {
        return Err(BridgeError::new(
            "PLUGIN_FS_SCOPE_DENIED",
            "path resolves outside the root",
        ));
    }
    Ok(canonical)
}

fn canonical_relative(root: &Path, path: &Path) -> BridgeResult<String> {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .map_err(|_| BridgeError::new("PLUGIN_FS_SCOPE_DENIED", "path resolves outside the root"))
}

fn check_read(root: &Path, rule: &FsRule, relative: &str) -> BridgeResult<(PathBuf, String)> {
    let canonical = resolve_existing(root, relative)?;
    let canonical_rel = canonical_relative(root, &canonical)?;
    if denied_relative(&canonical_rel) {
        return Err(BridgeError::new(
            "PLUGIN_FS_DENIED",
            "path is protected and cannot be read",
        ));
    }
    if !rule
        .scope
        .iter()
        .any(|scope| match_glob(&canonical_rel, scope))
    {
        return Err(BridgeError::new(
            "PLUGIN_FS_SCOPE_DENIED",
            format!("{canonical_rel} is outside the declared read scope"),
        ));
    }
    Ok((canonical, canonical_rel))
}

fn check_write(root: &Path, rule: &FsRule, relative: &str) -> BridgeResult<PathBuf> {
    let target = root.join(relative);
    if let Ok(metadata) = fs::symlink_metadata(&target) {
        if metadata.file_type().is_symlink() {
            return Err(BridgeError::new(
                "PLUGIN_FS_SCOPE_DENIED",
                "refusing to write through a symlink",
            ));
        }
    }
    let parent = target
        .parent()
        .ok_or_else(|| BridgeError::new("PLUGIN_FS_INVALID", "path has no parent directory"))?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|error| BridgeError::new("PLUGIN_FS_NOT_FOUND", error.to_string()))?;
    if !canonical_parent.starts_with(root) {
        return Err(BridgeError::new(
            "PLUGIN_FS_SCOPE_DENIED",
            "path resolves outside the root",
        ));
    }
    let name = target
        .file_name()
        .ok_or_else(|| BridgeError::new("PLUGIN_FS_INVALID", "path has no file name"))?;
    let resolved = canonical_parent.join(name);
    let canonical_rel = canonical_relative(root, &resolved)?;
    if denied_relative(&canonical_rel) {
        return Err(BridgeError::new(
            "PLUGIN_FS_DENIED",
            "path is protected and cannot be written",
        ));
    }
    if !rule
        .scope
        .iter()
        .any(|scope| match_glob(&canonical_rel, scope))
    {
        return Err(BridgeError::new(
            "PLUGIN_FS_SCOPE_DENIED",
            format!("{canonical_rel} is outside the declared write scope"),
        ));
    }
    Ok(resolved)
}

fn read_rule<'a>(
    policy: &'a FsPolicy,
    permissions: &HashSet<String>,
    manifest: &Value,
    mode: FsMode,
) -> BridgeResult<&'a FsRule> {
    if !permissions.contains(&format!("fs.{}", mode.name())) {
        return Err(BridgeError::new(
            "PLUGIN_PERMISSION_DENIED",
            format!("fs.{} is not granted", mode.name()),
        ));
    }
    let _ = manifest;
    policy.rule(mode).ok_or_else(|| {
        BridgeError::new(
            "PLUGIN_FS_SCOPE_DENIED",
            format!("manifest declares no fs.{} scope", mode.name()),
        )
    })
}

async fn fs_read_text(
    state: &Arc<AppState>,
    plugin_id: &str,
    plugin: &Value,
    manifest: &Value,
    payload: &Value,
) -> BridgeResult<Value> {
    let (permissions, policy) = fs_access(plugin, manifest)?;
    let rule = read_rule(&policy, &permissions, manifest, FsMode::Read)?;
    let relative = path_arg(payload, "path")?;
    let root = fs_root(state, plugin_id, rule).await?;
    let (path, _) = check_read(&root, rule, &relative)?;
    let metadata = fs::metadata(&path)
        .map_err(|error| BridgeError::new("PLUGIN_FS_NOT_FOUND", error.to_string()))?;
    if metadata.len() > READ_TEXT_MAX {
        return Err(BridgeError::new(
            "PLUGIN_FS_TOO_LARGE",
            format!("file exceeds {READ_TEXT_MAX} bytes"),
        ));
    }
    let bytes = fs::read(&path)
        .map_err(|error| BridgeError::new("PLUGIN_FS_READ_FAILED", error.to_string()))?;
    Ok(json!(String::from_utf8_lossy(&bytes)))
}

async fn fs_stat(
    state: &Arc<AppState>,
    plugin_id: &str,
    plugin: &Value,
    manifest: &Value,
    payload: &Value,
) -> BridgeResult<Value> {
    let (permissions, policy) = fs_access(plugin, manifest)?;
    let rule = read_rule(&policy, &permissions, manifest, FsMode::Read)?;
    let relative = path_arg(payload, "path")?;
    let root = fs_root(state, plugin_id, rule).await?;
    let (path, _) = check_read(&root, rule, &relative)?;
    let metadata = fs::metadata(&path)
        .map_err(|error| BridgeError::new("PLUGIN_FS_NOT_FOUND", error.to_string()))?;
    Ok(json!({
        "size": metadata.len(),
        "mtimeMs": metadata.modified().ok().and_then(|modified| {
            modified
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_millis() as u64)
        }).unwrap_or(0),
    }))
}

async fn fs_read_range(
    state: &Arc<AppState>,
    plugin_id: &str,
    plugin: &Value,
    manifest: &Value,
    payload: &Value,
) -> BridgeResult<Value> {
    let (permissions, policy) = fs_access(plugin, manifest)?;
    let rule = read_rule(&policy, &permissions, manifest, FsMode::Read)?;
    let array = payload.as_array();
    let relative = match array {
        Some(items) => {
            normalize_relative_path(items.first().and_then(|value| value.as_str()).unwrap_or(""))?
        }
        None => path_arg(payload, "path")?,
    };
    let offset = array
        .and_then(|items| items.get(1))
        .or_else(|| payload.get("offset"))
        .or_else(|| payload.get("byteOffset"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let length = array
        .and_then(|items| items.get(2))
        .or_else(|| payload.get("length"))
        .and_then(|value| value.as_u64())
        .unwrap_or(READ_RANGE_MAX)
        .min(READ_RANGE_MAX);
    let root = fs_root(state, plugin_id, rule).await?;
    let (path, _) = check_read(&root, rule, &relative)?;
    let mut file = fs::File::open(&path)
        .map_err(|error| BridgeError::new("PLUGIN_FS_READ_FAILED", error.to_string()))?;
    let total = file
        .metadata()
        .map_err(|error| BridgeError::new("PLUGIN_FS_READ_FAILED", error.to_string()))?
        .len();
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| BridgeError::new("PLUGIN_FS_READ_FAILED", error.to_string()))?;
    let mut buffer = Vec::with_capacity(length.min(1024 * 1024) as usize);
    file.take(length)
        .read_to_end(&mut buffer)
        .map_err(|error| BridgeError::new("PLUGIN_FS_READ_FAILED", error.to_string()))?;
    Ok(json!({
        "bytes": base64::engine::general_purpose::STANDARD.encode(&buffer),
        "totalSize": total,
    }))
}

async fn fs_read_preview(
    state: &Arc<AppState>,
    plugin_id: &str,
    plugin: &Value,
    manifest: &Value,
    payload: &Value,
) -> BridgeResult<Value> {
    let (permissions, policy) = fs_access(plugin, manifest)?;
    let rule = read_rule(&policy, &permissions, manifest, FsMode::Read)?;
    let relative = path_arg(payload, "path")?;
    let root = fs_root(state, plugin_id, rule).await?;
    let (path, _) = check_read(&root, rule, &relative)?;
    let size = fs::metadata(&path)
        .map_err(|error| BridgeError::new("PLUGIN_FS_NOT_FOUND", error.to_string()))?
        .len();
    if size > IMAGE_PREVIEW_MAX {
        return Ok(json!({ "kind": "tooLarge", "size": size }));
    }
    let content_type = crate::preview::content_type(&path);
    if content_type.starts_with("image/") {
        let bytes = fs::read(&path)
            .map_err(|error| BridgeError::new("PLUGIN_FS_READ_FAILED", error.to_string()))?;
        let mime = content_type
            .split(';')
            .next()
            .unwrap_or(content_type)
            .trim();
        return Ok(json!({
            "kind": "image",
            "dataUrl": format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&bytes)
            ),
            "size": size,
        }));
    }
    if content_type.starts_with("text/")
        || content_type.contains("json")
        || content_type.contains("xml")
    {
        if size > TEXT_PREVIEW_MAX {
            return Ok(json!({ "kind": "tooLarge", "size": size }));
        }
        let bytes = fs::read(&path)
            .map_err(|error| BridgeError::new("PLUGIN_FS_READ_FAILED", error.to_string()))?;
        return Ok(json!({
            "kind": "text",
            "content": String::from_utf8_lossy(&bytes),
            "size": size,
        }));
    }
    Ok(json!({ "kind": "binary", "size": size }))
}

async fn fs_open(
    app: &AppHandle,
    state: &Arc<AppState>,
    plugin_id: &str,
    plugin: &Value,
    manifest: &Value,
    payload: &Value,
    reveal: bool,
) -> BridgeResult<Value> {
    let (permissions, policy) = fs_access(plugin, manifest)?;
    let rule = read_rule(&policy, &permissions, manifest, FsMode::Read)?;
    let relative = path_arg(payload, "path")?;
    let root = fs_root(state, plugin_id, rule).await?;
    let (path, _) = check_read(&root, rule, &relative)?;
    let display = path.to_string_lossy().to_string();
    let result = if reveal {
        crate::native::reveal_item(app, &display)
    } else {
        crate::native::open_path(app, &display)
    };
    result.map_err(|error| BridgeError::new("PLUGIN_FS_OPEN_FAILED", error))?;
    Ok(json!(null))
}

async fn fs_write_text(
    state: &Arc<AppState>,
    plugin_id: &str,
    plugin: &Value,
    manifest: &Value,
    payload: &Value,
) -> BridgeResult<Value> {
    let (permissions, policy) = fs_access(plugin, manifest)?;
    let rule = read_rule(&policy, &permissions, manifest, FsMode::Write)?;
    let (relative, content) = if let Some(items) = payload.as_array() {
        (
            normalize_relative_path(items.first().and_then(|value| value.as_str()).unwrap_or(""))?,
            items.get(1).and_then(|value| value.as_str()).unwrap_or(""),
        )
    } else {
        let content = payload
            .get("content")
            .and_then(|value| value.as_str())
            .ok_or_else(|| BridgeError::new("INVALID_ARGUMENT", "content is required"))?;
        (path_arg(payload, "path")?, content)
    };
    let root = fs_root(state, plugin_id, rule).await?;
    let target = check_write(&root, rule, &relative)?;
    fs::write(&target, content)
        .map_err(|error| BridgeError::new("PLUGIN_FS_WRITE_FAILED", error.to_string()))?;
    state.plugins.record_written(plugin_id, target);
    Ok(json!(null))
}

async fn fs_remove(
    state: &Arc<AppState>,
    plugin_id: &str,
    plugin: &Value,
    manifest: &Value,
    payload: &Value,
) -> BridgeResult<Value> {
    let (permissions, policy) = fs_access(plugin, manifest)?;
    let rule = read_rule(&policy, &permissions, manifest, FsMode::Delete)?;
    let relative = path_arg(payload, "path")?;
    let root = fs_root(state, plugin_id, rule).await?;
    let canonical = resolve_existing(&root, &relative)?;
    let canonical_rel = canonical_relative(&root, &canonical)?;
    if denied_relative(&canonical_rel) {
        return Err(BridgeError::new(
            "PLUGIN_FS_DENIED",
            "path is protected and cannot be deleted",
        ));
    }
    let own_write = rule.own && state.plugins.is_written(plugin_id, &canonical);
    if !own_write
        && !rule
            .scope
            .iter()
            .any(|scope| match_glob(&canonical_rel, scope))
    {
        return Err(BridgeError::new(
            "PLUGIN_FS_SCOPE_DENIED",
            format!("{canonical_rel} is outside the declared delete scope"),
        ));
    }
    let metadata = fs::metadata(&canonical)
        .map_err(|error| BridgeError::new("PLUGIN_FS_NOT_FOUND", error.to_string()))?;
    if metadata.is_dir() {
        return Err(BridgeError::new(
            "PLUGIN_FS_NOT_A_FILE",
            "remove only accepts files",
        ));
    }
    fs::remove_file(&canonical)
        .map_err(|error| BridgeError::new("PLUGIN_FS_WRITE_FAILED", error.to_string()))?;
    Ok(json!({ "deleted": true }))
}

async fn fs_glob(
    state: &Arc<AppState>,
    plugin_id: &str,
    plugin: &Value,
    manifest: &Value,
    payload: &Value,
) -> BridgeResult<Value> {
    let (permissions, policy) = fs_access(plugin, manifest)?;
    let rule = read_rule(&policy, &permissions, manifest, FsMode::Read)?;
    let pattern = payload
        .get("pattern")
        .and_then(|value| value.as_str())
        .or_else(|| payload.as_str())
        .unwrap_or("");
    let pattern = normalize_scope(pattern)
        .ok_or_else(|| BridgeError::new("PLUGIN_FS_INVALID", "pattern must be root-relative"))?;
    let root = fs_root(state, plugin_id, rule).await?;
    let mut results: Vec<String> = Vec::new();
    let mut seen = 0usize;
    walk_glob(&root, &root, rule, &pattern, 0, &mut seen, &mut results);
    results.sort();
    Ok(json!(results))
}

fn walk_glob(
    root: &Path,
    directory: &Path,
    rule: &FsRule,
    pattern: &str,
    depth: usize,
    seen: &mut usize,
    results: &mut Vec<String>,
) {
    if depth > GLOB_MAX_DEPTH || *seen >= GLOB_MAX_RESULTS {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if *seen >= GLOB_MAX_RESULTS {
            return;
        }
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        if denied_relative(&relative) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            walk_glob(root, &path, rule, pattern, depth + 1, seen, results);
            continue;
        }
        *seen += 1;
        if rule.scope.iter().any(|scope| match_glob(&relative, scope))
            && match_glob(&relative, pattern)
        {
            results.push(relative);
        }
    }
}

async fn fs_list(
    state: &Arc<AppState>,
    plugin_id: &str,
    plugin: &Value,
    manifest: &Value,
    payload: &Value,
) -> BridgeResult<Value> {
    let (permissions, policy) = fs_access(plugin, manifest)?;
    let rule = read_rule(&policy, &permissions, manifest, FsMode::Read)?;
    let relative = path_arg(payload, "path")?;
    let root = fs_root(state, plugin_id, rule).await?;
    let directory = if relative.is_empty() {
        root.clone()
    } else {
        resolve_existing(&root, &relative)?
    };
    let metadata = fs::metadata(&directory)
        .map_err(|error| BridgeError::new("PLUGIN_FS_NOT_FOUND", error.to_string()))?;
    if !metadata.is_dir() {
        return Err(BridgeError::new(
            "PLUGIN_FS_NOT_A_DIRECTORY",
            "list requires a directory",
        ));
    }
    let entries = fs::read_dir(&directory)
        .map_err(|error| BridgeError::new("PLUGIN_FS_READ_FAILED", error.to_string()))?;
    let mut rows: Vec<Value> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(&root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        if denied_relative(&relative) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let is_directory = file_type.is_dir();
        if !is_directory && !rule.scope.iter().any(|scope| match_glob(&relative, scope)) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let mut row = json!({
            "name": name,
            "path": relative,
            "isDirectory": is_directory,
        });
        if !is_directory {
            if let Ok(metadata) = entry.metadata() {
                row["size"] = json!(metadata.len());
                row["mtimeMs"] = json!(metadata
                    .modified()
                    .ok()
                    .and_then(|modified| modified
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|duration| duration.as_millis() as u64))
                    .unwrap_or(0));
            }
        }
        rows.push(row);
    }
    rows.sort_by(|left, right| {
        left["name"]
            .as_str()
            .unwrap_or("")
            .cmp(right["name"].as_str().unwrap_or(""))
    });
    Ok(json!(rows))
}

async fn fs_request_directory(
    app: &AppHandle,
    state: &Arc<AppState>,
    plugin_id: &str,
    manifest: &Value,
) -> BridgeResult<Value> {
    if !plugins::has_permission(manifest, "fs.read")
        && !plugins::has_permission(manifest, "fs.write")
        && !plugins::has_permission(manifest, "fs.delete")
    {
        return Err(BridgeError::new(
            "PLUGIN_PERMISSION_DENIED",
            "fs.read, fs.write or fs.delete is required",
        ));
    }
    let app = app.clone();
    let picked =
        tauri::async_runtime::spawn_blocking(move || app.dialog().file().blocking_pick_folder())
            .await
            .map_err(|error| BridgeError::new("INTERNAL", error.to_string()))?
            .and_then(|path| path.into_path().ok());
    let Some(path) = picked else {
        return Ok(Value::Null);
    };
    let canonical = fs::canonicalize(&path)
        .map_err(|error| BridgeError::new("PLUGIN_FS_INVALID", error.to_string()))?;
    if !canonical.is_dir() {
        return Err(BridgeError::new("PLUGIN_FS_INVALID", "not a directory"));
    }
    state.plugins.set_fs_grant(plugin_id, canonical.clone());
    let name = canonical
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string();
    Ok(json!({ "path": canonical.to_string_lossy(), "name": name }))
}

// ── clipboard ──────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn clipboard_read_text() -> BridgeResult<String> {
    let output = std::process::Command::new("pbpaste")
        .output()
        .map_err(|error| BridgeError::new("CLIPBOARD_UNAVAILABLE", error.to_string()))?;
    if !output.status.success() {
        return Err(BridgeError::new("CLIPBOARD_UNAVAILABLE", "pbpaste failed"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(target_os = "macos")]
fn clipboard_write_text(text: &str) -> BridgeResult<()> {
    use std::io::Write as _;
    let mut child = std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| BridgeError::new("CLIPBOARD_UNAVAILABLE", error.to_string()))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|error| BridgeError::new("CLIPBOARD_UNAVAILABLE", error.to_string()))?;
    }
    let status = child
        .wait()
        .map_err(|error| BridgeError::new("CLIPBOARD_UNAVAILABLE", error.to_string()))?;
    if !status.success() {
        return Err(BridgeError::new("CLIPBOARD_UNAVAILABLE", "pbcopy failed"));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn clipboard_read_text() -> BridgeResult<String> {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-Clipboard -Raw",
        ])
        .output()
        .map_err(|error| BridgeError::new("CLIPBOARD_UNAVAILABLE", error.to_string()))?;
    if !output.status.success() {
        return Err(BridgeError::new(
            "CLIPBOARD_UNAVAILABLE",
            "Get-Clipboard failed",
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(target_os = "windows")]
fn clipboard_write_text(text: &str) -> BridgeResult<()> {
    use std::io::Write as _;
    let mut child = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$input | Set-Clipboard",
        ])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| BridgeError::new("CLIPBOARD_UNAVAILABLE", error.to_string()))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|error| BridgeError::new("CLIPBOARD_UNAVAILABLE", error.to_string()))?;
    }
    let status = child
        .wait()
        .map_err(|error| BridgeError::new("CLIPBOARD_UNAVAILABLE", error.to_string()))?;
    if !status.success() {
        return Err(BridgeError::new(
            "CLIPBOARD_UNAVAILABLE",
            "Set-Clipboard failed",
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn clipboard_read_text() -> BridgeResult<String> {
    for command in [
        ("wl-paste", vec!["--no-newline"]),
        ("xclip", vec!["-selection", "clipboard", "-o"]),
    ] {
        if let Ok(output) = std::process::Command::new(command.0)
            .args(command.1)
            .output()
        {
            if output.status.success() {
                return Ok(String::from_utf8_lossy(&output.stdout).to_string());
            }
        }
    }
    Err(BridgeError::new(
        "CLIPBOARD_UNAVAILABLE",
        "install wl-clipboard or xclip to use the clipboard",
    ))
}

#[cfg(target_os = "linux")]
fn clipboard_write_text(text: &str) -> BridgeResult<()> {
    use std::io::Write as _;
    for command in [
        ("wl-copy", Vec::<&str>::new()),
        ("xclip", vec!["-selection", "clipboard"]),
    ] {
        let Ok(mut child) = std::process::Command::new(command.0)
            .args(command.1)
            .stdin(std::process::Stdio::piped())
            .spawn()
        else {
            continue;
        };
        if let Some(mut stdin) = child.stdin.take() {
            if stdin.write_all(text.as_bytes()).is_err() {
                continue;
            }
        }
        if child.wait().map(|status| status.success()).unwrap_or(false) {
            return Ok(());
        }
    }
    Err(BridgeError::new(
        "CLIPBOARD_UNAVAILABLE",
        "install wl-clipboard or xclip to use the clipboard",
    ))
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn clipboard_read_text() -> BridgeResult<String> {
    Err(BridgeError::new(
        "CLIPBOARD_UNAVAILABLE",
        "clipboard is not available on this platform",
    ))
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn clipboard_write_text(_text: &str) -> BridgeResult<()> {
    Err(BridgeError::new(
        "CLIPBOARD_UNAVAILABLE",
        "clipboard is not available on this platform",
    ))
}

// ── net / shell / project / models ────────────────────────────────────────

async fn net_fetch(manifest: &Value, payload: &Value) -> BridgeResult<Value> {
    require(manifest, "net.fetch")?;
    let domains = net_domains(manifest)?;
    let raw_url = payload
        .get("url")
        .and_then(|value| value.as_str())
        .ok_or_else(|| BridgeError::new("INVALID_ARGUMENT", "url is required"))?;
    let url = url::Url::parse(raw_url)
        .map_err(|error| BridgeError::new("PLUGIN_NET_INVALID_URL", error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(BridgeError::new(
            "PLUGIN_NET_INVALID_URL",
            "net.fetch only accepts http(s) URLs",
        ));
    }
    let host = url.host_str().unwrap_or("").to_lowercase();
    if !host_allowed(&host, &domains) {
        return Err(BridgeError::new(
            "PLUGIN_NET_DOMAIN_DENIED",
            format!("{host} is not in manifest.net.domains"),
        ));
    }
    let method = payload
        .get("method")
        .and_then(|value| value.as_str())
        .unwrap_or("GET")
        .to_uppercase();
    let headers: Vec<(String, String)> = payload
        .get("headers")
        .and_then(|value| value.as_object())
        .map(|headers| {
            headers
                .iter()
                .filter_map(|(name, value)| {
                    value
                        .as_str()
                        .map(|value| (name.clone(), value.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    let body = payload
        .get("body")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let timeout_ms = payload
        .get("timeoutMs")
        .and_then(|value| value.as_u64())
        .unwrap_or(30_000)
        .clamp(1_000, 60_000);
    let url = url.to_string();

    let response = tauri::async_runtime::spawn_blocking(move || {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_millis(timeout_ms))
            .redirects(0)
            .build();
        let mut request = agent.request(&method, &url);
        for (name, value) in &headers {
            request = request.set(name, value);
        }
        match body {
            Some(body) => request.send_string(&body),
            None => request.call(),
        }
    })
    .await
    .map_err(|error| BridgeError::new("INTERNAL", error.to_string()))?;

    let response = match response {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(ureq::Error::Transport(error)) => {
            return Err(BridgeError::new("PLUGIN_NETWORK", error.to_string()))
        }
    };
    let status = response.status();
    let mut headers = Map::new();
    for name in response.headers_names() {
        let joined = response
            .all(&name)
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        headers.insert(name.to_lowercase(), json!(joined));
    }
    let mut reader = response.into_reader().take(NET_BODY_MAX);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| BridgeError::new("PLUGIN_NETWORK", error.to_string()))?;
    Ok(json!({
        "status": status,
        "headers": headers,
        "bodyText": String::from_utf8_lossy(&bytes),
    }))
}

pub(crate) fn net_domains(manifest: &Value) -> BridgeResult<Vec<String>> {
    let Some(raw) = manifest.pointer("/net/domains") else {
        return Ok(Vec::new());
    };
    let Some(entries) = raw.as_array() else {
        return Err(BridgeError::new(
            "PLUGIN_INVALID",
            "manifest.net.domains must be an array",
        ));
    };
    let mut domains = Vec::new();
    for entry in entries {
        let Some(value) = entry
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Err(BridgeError::new(
                "PLUGIN_INVALID",
                "manifest.net.domains entries must be strings",
            ));
        };
        let value = value.to_lowercase();
        if value == "*" || value.contains('/') || value.contains(':') || value.contains('?') {
            return Err(BridgeError::new(
                "PLUGIN_INVALID",
                format!("manifest.net.domains entry is not a bare hostname: {value}"),
            ));
        }
        domains.push(value);
    }
    Ok(domains)
}

pub(crate) fn host_allowed(host: &str, domains: &[String]) -> bool {
    domains.iter().any(|pattern| {
        if let Some(suffix) = pattern.strip_prefix("*.") {
            host == suffix || host.ends_with(&format!(".{suffix}"))
        } else {
            host == pattern
        }
    })
}

fn open_external(app: &AppHandle, raw: &str) -> BridgeResult<Value> {
    let url = url::Url::parse(raw)
        .map_err(|error| BridgeError::new("PLUGIN_NET_INVALID_URL", error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(BridgeError::new(
            "PLUGIN_NET_INVALID_URL",
            "openExternal only accepts http(s) URLs",
        ));
    }
    crate::native::open_url(app, url.as_str())
        .map_err(|error| BridgeError::new("PLUGIN_SHELL_FAILED", error))?;
    Ok(json!(null))
}

async fn project_create(
    state: &Arc<AppState>,
    manifest: &Value,
    payload: &Value,
) -> BridgeResult<Value> {
    require(manifest, "project.create")?;
    let path = payload
        .get("path")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| BridgeError::new("INVALID_ARGUMENT", "path is required"))?;
    let reply = state
        .host
        .call("projects.create", json!({ "path": path }))
        .await
        .map_err(|error| BridgeError::new("HOST_UNAVAILABLE", error.to_string()))?;
    let project = reply.get("project").cloned().unwrap_or(Value::Null);
    Ok(json!({
        "projectId": project.get("id").cloned().unwrap_or(Value::Null),
        "path": project.get("path").cloned().unwrap_or(json!(path)),
        "name": project.get("name").cloned().unwrap_or(json!(path)),
    }))
}

async fn models_list(state: &Arc<AppState>, manifest: &Value) -> BridgeResult<Value> {
    require(manifest, "models.list")?;
    let settings = state
        .host
        .call("settings.get", json!({}))
        .await
        .map_err(|error| BridgeError::new("HOST_UNAVAILABLE", error.to_string()))?;
    let default_provider = settings
        .get("defaultProviderId")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let default_model = settings
        .get("defaultModelId")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let reply = state
        .host
        .call("providers.list", json!({ "includeDisabled": false }))
        .await
        .map_err(|error| BridgeError::new("HOST_UNAVAILABLE", error.to_string()))?;
    let providers = reply
        .get("providers")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let mut rows: Vec<Value> = Vec::new();
    for provider in providers {
        let provider_id = provider
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let provider_name = provider
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or(provider_id);
        let provider_levels: Vec<String> = provider
            .get("supportedThinkingLevels")
            .and_then(|value| value.as_array())
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let supports_reasoning = provider
            .get("supportsReasoning")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let Some(models) = provider.get("models").and_then(|value| value.as_array()) else {
            continue;
        };
        for model in models {
            let model_id = model
                .get("id")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if model_id.is_empty() {
                continue;
            }
            let alias = model
                .get("alias")
                .and_then(|value| value.as_str())
                .filter(|alias| !alias.trim().is_empty());
            let thinking_levels: Vec<String> = model
                .get("thinkingLevels")
                .and_then(|value| value.as_array())
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|entry| entry.as_str().map(str::to_string))
                        .collect()
                })
                .filter(|levels: &Vec<String>| !levels.is_empty())
                .unwrap_or_else(|| provider_levels.clone());
            let mut row = json!({
                "key": format!("{provider_id}/{model_id}"),
                "providerId": provider_id,
                "providerName": provider_name,
                "modelId": model_id,
                "label": alias.unwrap_or(model_id),
                "supportsReasoning": supports_reasoning,
                "thinkingLevels": thinking_levels,
                "isDefault": provider_id == default_provider && model_id == default_model,
            });
            if let Some(alias) = alias {
                row["alias"] = json!(alias);
            }
            if let Some(available) = model
                .get("availableForSubagents")
                .and_then(|value| value.as_bool())
            {
                row["availableForSubagents"] = json!(available);
            }
            rows.push(row);
        }
    }
    Ok(json!(rows))
}

// ── plugin completions ──────────────────────────────────────────────────────

const PLUGIN_COMPLETE_SYSTEM_MAX: usize = 32 * 1024;
const PLUGIN_COMPLETE_CONTENT_MAX: usize = 200_000;

fn sidecar_bridge_error(error: crate::rpc::RpcError) -> BridgeError {
    match error {
        crate::rpc::RpcError::Remote { message, data } => BridgeError::new(
            data.get("errorCode")
                .and_then(Value::as_str)
                .unwrap_or("PLUGIN_COMPLETE_FAILED"),
            message,
        ),
        crate::rpc::RpcError::Transport(message) => BridgeError::new("AGENT_UNAVAILABLE", message),
    }
}

async fn agent_complete(
    state: &Arc<AppState>,
    plugin_id: &str,
    manifest: &Value,
    payload: &Value,
    session_id: Option<&str>,
) -> BridgeResult<Value> {
    require(manifest, "agent.complete")?;
    let model_key = payload
        .get("modelKey")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| BridgeError::new("INVALID_ARGUMENT", "modelKey is required"))?;
    let (provider_id, model_id) = model_key.split_once('/').ok_or_else(|| {
        BridgeError::new(
            "INVALID_ARGUMENT",
            "modelKey must be in provider/model form",
        )
    })?;
    if provider_id.is_empty() || model_id.is_empty() {
        return Err(BridgeError::new(
            "INVALID_ARGUMENT",
            "modelKey must be in provider/model form",
        ));
    }

    let system = payload.get("system").and_then(Value::as_str).unwrap_or("");
    if system.len() > PLUGIN_COMPLETE_SYSTEM_MAX {
        return Err(BridgeError::new(
            "INVALID_ARGUMENT",
            "system exceeds the 32 KiB limit",
        ));
    }

    let mut messages: Vec<Value> = payload
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let input_messages_empty = messages.is_empty();
    let mut total = system.chars().count();
    for message in &messages {
        let role = message.get("role").and_then(Value::as_str).unwrap_or("");
        if !matches!(role, "user" | "assistant") {
            return Err(BridgeError::new(
                "INVALID_ARGUMENT",
                "messages may only contain user and assistant roles",
            ));
        }
        let content = message.get("content").and_then(Value::as_str).unwrap_or("");
        total += content.chars().count();
    }

    if payload
        .get("includeSessionContext")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let context = session_context(state, manifest, session_id).await?;
        let mut context_messages: Vec<Value> = context
            .get("messages")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|message| {
                        let role = message.get("role").and_then(Value::as_str)?;
                        if !matches!(role, "user" | "assistant") {
                            return None;
                        }
                        Some(json!({
                            "role": role,
                            "content": message
                                .get("content")
                                .and_then(Value::as_str)
                                .unwrap_or(""),
                        }))
                    })
                    .collect()
            })
            .unwrap_or_default();
        context_messages.extend(messages);
        messages = context_messages;
        if input_messages_empty {
            messages.push(json!({
                "role": "user",
                "content": "Please respond to the request.",
            }));
        }
        total = system.chars().count()
            + messages
                .iter()
                .filter_map(|message| message.get("content").and_then(Value::as_str))
                .map(|content| content.chars().count())
                .sum::<usize>();
    }

    if total > PLUGIN_COMPLETE_CONTENT_MAX {
        return Err(BridgeError::new(
            "INVALID_ARGUMENT",
            "combined messages exceed the 200k character limit",
        ));
    }
    if messages.is_empty() && system.trim().is_empty() {
        return Err(BridgeError::new(
            "INVALID_ARGUMENT",
            "messages or system is required",
        ));
    }
    if !state.plugins.allow_plugin_complete(plugin_id) {
        return Err(BridgeError::new(
            "RATE_LIMITED",
            "agent.complete is limited to 8 calls per minute",
        ));
    }

    let providers = state
        .host
        .call("providers.list", json!({ "includeDisabled": false }))
        .await
        .map_err(|error| BridgeError::new("HOST_UNAVAILABLE", error.to_string()))?;
    let provider = providers
        .get("providers")
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter()
                .find(|row| row.get("id").and_then(Value::as_str) == Some(provider_id))
        })
        .cloned()
        .ok_or_else(|| {
            BridgeError::new(
                "MODEL_NOT_CONFIGURED",
                format!("provider {provider_id} is not configured"),
            )
        })?;
    let auth_kind = provider
        .get("authKind")
        .and_then(Value::as_str)
        .unwrap_or("");
    let api_key = if auth_kind == "oauth" {
        crate::oauth::resolve_access_token(state, provider_id)
            .await
            .map_err(|message| BridgeError::new("PROVIDER_NOT_BOUND", message))?
    } else if auth_kind == "none" {
        String::new()
    } else {
        let secret = state
            .host
            .call("providers.getSecret", json!({ "id": provider_id }))
            .await
            .map_err(|error| BridgeError::new("HOST_UNAVAILABLE", error.to_string()))?;
        let key = secret
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if key.is_empty() {
            return Err(BridgeError::new(
                "PROVIDER_SECRET_MISSING",
                "provider API key missing",
            ));
        }
        key
    };
    let base_url = provider
        .get("baseUrl")
        .and_then(Value::as_str)
        .unwrap_or("");
    let vendor_key = provider
        .get("vendorKey")
        .and_then(Value::as_str)
        .unwrap_or("custom");
    let model_config = state
        .catalog
        .find_model(vendor_key, base_url, model_id)
        .map(|model| model.to_model_config(base_url))
        .unwrap_or_else(|| {
            json!({
                "name": model_id,
                "baseUrl": base_url,
            })
        });
    let mut prompt = json!({
        "provider": {
            "id": provider_id,
            "name": provider.get("name").cloned().unwrap_or(Value::Null),
            "baseUrl": base_url,
            "modelId": model_id,
            "apiKey": api_key,
            "authKind": auth_kind,
            "apiStyle": crate::launch::effective_api_style(&provider, model_id),
            "supportsReasoning": provider.get("supportsReasoning").cloned().unwrap_or(json!(false)),
            "supportedThinkingLevels": provider
                .get("supportedThinkingLevels")
                .cloned()
                .unwrap_or(Value::Null),
            "modelConfig": model_config,
            "headers": provider.get("headers").cloned().unwrap_or(Value::Null),
        },
        "thinkingLevel": payload.get("thinkingLevel").cloned().unwrap_or(Value::Null),
        "system": system,
        "messages": Value::Array(messages),
    });
    if let Some(thinking) = payload.get("thinkingLevel").and_then(Value::as_str) {
        prompt["thinkingLevel"] = json!(thinking);
    }
    let reply = tokio::time::timeout(
        Duration::from_secs(90),
        state.sidecar.call("plugin.complete", prompt),
    )
    .await;
    let reply = match reply {
        Err(_) => {
            return Err(BridgeError::new(
                "TIMEOUT",
                "agent.complete exceeded the 90s budget",
            ))
        }
        Ok(Err(error)) => return Err(sidecar_bridge_error(error)),
        Ok(Ok(reply)) => reply,
    };
    let text = reply
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if text.is_empty() {
        return Err(BridgeError::new(
            "INVALID_ARGUMENT",
            "the model returned an empty response",
        ));
    }
    Ok(json!({
        "text": text,
        "modelKey": model_key,
        "thinkingLevel": payload.get("thinkingLevel").cloned().unwrap_or(Value::Null),
    }))
}

// ── sessions ───────────────────────────────────────────────────────────────

fn session_permission(manifest: &Value, any: &str, own: &str) -> BridgeResult<()> {
    if plugins::has_permission(manifest, any) || plugins::has_permission(manifest, own) {
        Ok(())
    } else {
        Err(BridgeError::new(
            "PLUGIN_PERMISSION_DENIED",
            format!("{any} is not granted"),
        ))
    }
}

fn manifest_source(manifest: &Value, payload: &Value) -> BridgeResult<String> {
    let source = payload
        .get("source")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|source| !source.is_empty())
        .ok_or_else(|| BridgeError::new("INVALID_ARGUMENT", "source is required"))?;
    let declared = plugins::contributions(manifest, "sessionSources")
        .iter()
        .any(|entry| entry.get("id").and_then(|value| value.as_str()) == Some(source));
    if !declared {
        return Err(BridgeError::new(
            "PLUGIN_SESSION_SOURCE_DENIED",
            format!("source \"{source}\" is not declared in contributes.sessionSources"),
        ));
    }
    Ok(source.to_string())
}

async fn session_call(
    state: &Arc<AppState>,
    plugin_id: &str,
    manifest: &Value,
    method: &str,
    payload: &Value,
    source: Option<String>,
) -> BridgeResult<Value> {
    let permission = match method {
        "plugin.session.import" | "plugin.session.importBatch" => "session.import",
        "plugin.session.rename" => "session.update.own",
        "plugin.session.delete" => "session.delete.own",
        _ => "session.read",
    };
    require(manifest, permission)?;
    let mut params = payload.as_object().cloned().unwrap_or_default();
    // The authenticated plugin id always wins over a caller-supplied one.
    params.insert("pluginId".into(), json!(plugin_id));
    if let Some(source) = source {
        params.insert("source".into(), json!(source));
        if !params.contains_key("sourceLabel") {
            let source = params
                .get("source")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if let Some(label) = plugins::contributions(manifest, "sessionSources")
                .iter()
                .find(|entry| entry.get("id").and_then(|value| value.as_str()) == Some(source))
                .and_then(|entry| entry.get("label"))
            {
                params.insert("sourceLabel".into(), json!(plugins::localized(label)));
            }
        }
    }
    let reply = state
        .host
        .call(method, Value::Object(params))
        .await
        .map_err(|error| {
            let code = match &error {
                crate::rpc::RpcError::Remote { data, .. } => data
                    .get("errorCode")
                    .and_then(|value| value.as_str())
                    .unwrap_or("HOST_UNAVAILABLE"),
                _ => "HOST_UNAVAILABLE",
            };
            BridgeError::new(code, error.to_string())
        })?;
    // The session list is renderer state; a plugin import/rename/delete must
    // refresh it (and reveal newly imported projects, per the renderer's
    // reason check). Reads stay silent.
    if matches!(
        method,
        "plugin.session.import"
            | "plugin.session.importBatch"
            | "plugin.session.rename"
            | "plugin.session.delete"
    ) {
        emit(
            state,
            "pi-desktop/session/event/changed",
            json!({ "reason": method, "pluginId": plugin_id }),
        );
    }
    Ok(reply)
}

async fn session_context(
    state: &Arc<AppState>,
    manifest: &Value,
    session_id: Option<&str>,
) -> BridgeResult<Value> {
    require(manifest, "session.read")?;
    let Some(session_id) = session_id else {
        return Err(BridgeError::new(
            "PLUGIN_SESSION_REQUIRED",
            "getLlmContext needs an open session",
        ));
    };
    let reply = state
        .host
        .call(
            "session.get",
            json!({ "id": session_id, "messageLimit": 60, "contentLimit": 65536 }),
        )
        .await
        .map_err(|error| BridgeError::new("HOST_UNAVAILABLE", error.to_string()))?;
    let session = reply.get("session").cloned().unwrap_or(Value::Null);
    let provider = session
        .get("providerId")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let model = session
        .get("modelId")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let model_key = if provider.is_empty() || model.is_empty() {
        Value::Null
    } else {
        json!(format!("{provider}/{model}"))
    };
    let messages: Vec<Value> = session
        .get("messages")
        .and_then(|value| value.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|message| {
                    let role = message.get("role").and_then(|value| value.as_str())?;
                    let role = match role {
                        "user" => "user",
                        "assistant" => "assistant",
                        "tool" => "tool",
                        _ => "system",
                    };
                    let mut row = json!({
                        "role": role,
                        "content": message.get("content").and_then(|value| value.as_str()).unwrap_or(""),
                    });
                    if let Some(tool) = message.get("toolName").and_then(|value| value.as_str()) {
                        row["toolName"] = json!(tool);
                    }
                    Some(row)
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(json!({
        "sessionId": session_id,
        "modelKey": model_key,
        "thinkingLevel": session.get("thinkingLevel").cloned().unwrap_or(Value::Null),
        "messages": messages,
        "truncated": reply
            .pointer("/session/hasMoreBefore")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_follow_the_sdk_segment_rules() {
        assert!(match_glob("src/a.ts", "src/**/*.ts"));
        assert!(match_glob("src/a/b.ts", "src/**/*.ts"));
        assert!(!match_glob("other/a.ts", "src/**/*.ts"));
        assert!(match_glob("src/a.ts", "src/**"));
        assert!(match_glob("src", "src/**"));
        assert!(match_glob("a.ts", "**/*.ts"));
        assert!(!match_glob("a/b.ts", "*.ts"));
        assert!(match_glob("anything", "**/*"));
    }

    #[test]
    fn protected_paths_are_denied_at_any_depth() {
        assert!(denied_relative(".env"));
        assert!(denied_relative("apps/web/.env.local"));
        assert!(denied_relative(".git/config"));
        assert!(denied_relative("keys/server.pem"));
        assert!(denied_relative("id_ed25519"));
        assert!(!denied_relative("env.example"));
        assert!(!denied_relative("src/main.ts"));
    }

    #[test]
    fn relative_paths_refuse_absolute_and_traversal() {
        assert_eq!(normalize_relative_path("./src/a.ts").unwrap(), "src/a.ts");
        assert!(normalize_relative_path("/etc/passwd").is_err());
        assert!(normalize_relative_path("C:/x").is_err());
        assert!(normalize_relative_path("../x").is_err());
    }

    #[test]
    fn legacy_permissions_migrate_and_new_scopes_are_required() {
        let plugin = json!({
            "permissions": ["fs.read.workspace", "fs.write.workspace", "fs.delete.workspace"]
        });
        let manifest = json!({});
        let (permissions, policy) = fs_access(&plugin, &manifest).unwrap();
        assert!(permissions.contains("fs.read"));
        assert!(permissions.contains("fs.write"));
        assert!(permissions.contains("fs.delete"));
        assert!(policy.read.is_some());
        assert!(policy.delete.is_some() && policy.delete.unwrap().own);

        let plugin = json!({ "permissions": ["fs.write"] });
        let manifest = json!({ "fs": { "write": { "scope": ["src/**"] } } });
        let (_, policy) = fs_access(&plugin, &manifest).unwrap();
        assert!(policy.write.is_some());
        assert!(!policy.write.unwrap().scope.is_empty());

        // A new-style permission without a rule grants no standing scope.
        let plugin = json!({ "permissions": ["fs.delete"] });
        let (_, policy) = fs_access(&plugin, &json!({})).unwrap();
        assert!(policy.delete.is_none());
    }

    #[test]
    fn whole_tree_writes_are_rejected() {
        let manifest = json!({ "fs": { "write": { "scope": ["**/*"] } } });
        assert!(fs_access(&json!({ "permissions": ["fs.write"] }), &manifest).is_err());
    }

    #[test]
    fn fs_scopes_and_denied_paths_are_enforced() {
        let root = std::env::temp_dir().join(format!(
            "pi-plugin-bridge-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("out")).unwrap();
        fs::write(root.join("src/a.txt"), "a").unwrap();
        fs::write(root.join("secret.txt"), "s").unwrap();
        fs::write(root.join(".env"), "e").unwrap();
        let canonical_root = fs::canonicalize(&root).unwrap();

        let read = FsRule {
            root: FsRoot::Workspace,
            scope: vec!["src/**".into()],
            own: false,
        };
        assert!(check_read(&canonical_root, &read, "src/a.txt").is_ok());
        assert!(check_read(&canonical_root, &read, "secret.txt").is_err());
        assert!(check_read(&canonical_root, &read, ".env").is_err());

        let write = FsRule {
            root: FsRoot::Workspace,
            scope: vec!["out/**".into()],
            own: false,
        };
        assert!(check_write(&canonical_root, &write, "out/new.txt").is_ok());
        assert!(check_write(&canonical_root, &write, "secret.txt").is_err());
        assert!(check_write(&canonical_root, &write, ".env").is_err());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn net_allowlist_matches_bare_and_wildcard_hosts() {
        let domains = vec!["example.com".to_string(), "*.example.org".to_string()];
        assert!(host_allowed("example.com", &domains));
        assert!(!host_allowed("api.example.com", &domains));
        assert!(host_allowed("example.org", &domains));
        assert!(host_allowed("api.example.org", &domains));
        assert!(!host_allowed("evil.org", &domains));
    }
}
