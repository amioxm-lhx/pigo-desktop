//! Plugin host shell surface.
//!
//! The Tauri shell runs plugin JavaScript in the Node runtime
//! (`crate::plugin_host`): commands, agent tools and runtime themes execute in
//! one child process per plugin. Panels and docked views are served by
//! `crate::plugin_surface` and rendered as iframes; this module resolves the
//! manifest entries, permissions and window metadata behind those URLs, and
//! serves the registry-derived metadata (views, manifest themes, services,
//! settings) the renderer reads. Channels the shell does not host (template
//! scaffolding, package import, trusted extensions) report `UNSUPPORTED`
//! instead of pretending to work.
//!
//! Theme CSS is injected verbatim by the renderer, so the sanitizer below is
//! the whole boundary. It is stricter than `packages/plugin-sdk` (comments and
//! string literals are not exempt) and allows only `data:` references and
//! assets a theme declared in `contributes.themes[].assets`. Declared assets are
//! served read-only through the `plugin-asset://` scheme registered in
//! `main.rs`; no plugin file is reachable without an enabled `ui.theme` grant.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

use crate::envelope::{err, ok};
use crate::state::AppState;

const THEME_CSS_MAX_BYTES: u64 = 256 * 1024;
const THEME_ASSET_MAX_BYTES: u64 = 4 * 1024 * 1024;

fn settings_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(crate) async fn plugin_rows(state: &AppState) -> Result<Vec<Value>, Value> {
    let reply = state
        .host
        .call("plugins.list", json!({}))
        .await
        .map_err(|error| crate::envelope::envelope_from_rpc("host", &error))?;
    Ok(reply
        .get("plugins")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default())
}

pub(crate) async fn current_project_path(state: &AppState) -> Option<String> {
    state
        .host
        .call("workspace.get", json!({}))
        .await
        .ok()
        .and_then(|reply| {
            reply
                .pointer("/workspace/path")
                .and_then(|path| path.as_str())
                .map(str::to_string)
        })
}

fn normalize_project_path(path: &str) -> String {
    let mut normalized = path.trim().replace('\\', "/");
    while normalized.len() > 1 && normalized.ends_with('/') && !normalized.ends_with(":/") {
        normalized.pop();
    }
    normalized
}

fn project_path_matches(entry: &str, target: &str) -> bool {
    let scoped = normalize_project_path(entry).to_lowercase();
    let target = normalize_project_path(target).to_lowercase();
    if scoped.is_empty() || target.is_empty() {
        return false;
    }
    target == scoped || target.starts_with(&format!("{scoped}/"))
}

/// Mirrors `isActiveInProject`: enabled, and either global or listed for this
/// open project. A project-scoped plugin is inactive without a workspace.
pub(crate) fn active_in_project(plugin: &Value, project_path: Option<&str>) -> bool {
    if !plugin
        .get("enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return false;
    }
    let scope = plugin.get("scope");
    let mode = scope
        .and_then(|value| value.get("mode"))
        .and_then(|value| value.as_str())
        .unwrap_or("global");
    if mode != "projects" {
        return true;
    }
    let Some(target) = project_path else {
        return false;
    };
    let projects = scope
        .and_then(|value| value.get("projects"))
        .and_then(|value| value.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    projects
        .iter()
        .any(|entry| project_path_matches(entry, target))
}

pub(crate) fn has_permission(plugin: &Value, permission: &str) -> bool {
    plugin
        .get("permissions")
        .and_then(|value| value.as_array())
        .map(|entries| {
            entries
                .iter()
                .any(|entry| entry.as_str() == Some(permission))
        })
        .unwrap_or(false)
}

pub(crate) fn plugin_id(plugin: &Value) -> &str {
    plugin
        .get("id")
        .and_then(|value| value.as_str())
        .unwrap_or("")
}

pub(crate) fn plugin_name(plugin: &Value) -> &str {
    plugin
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or("")
}

pub(crate) fn plugin_root(plugin: &Value) -> Option<PathBuf> {
    plugin
        .get("path")
        .and_then(|value| value.as_str())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

pub(crate) fn read_manifest(plugin: &Value) -> Option<Value> {
    let root = plugin_root(plugin)?;
    let raw = std::fs::read_to_string(root.join("manifest.json")).ok()?;
    serde_json::from_str(&raw).ok()
}

pub(crate) fn contributions<'a>(manifest: &'a Value, key: &str) -> &'a [Value] {
    manifest
        .get("contributes")
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_array())
        .map(|entries| entries.as_slice())
        .unwrap_or(&[])
}

pub(crate) fn safe_join(root: &Path, relative: &str) -> Option<PathBuf> {
    let relative = relative.trim().replace('\\', "/");
    if relative.is_empty() || relative.starts_with('/') || relative.contains(':') {
        return None;
    }
    let mut path = root.to_path_buf();
    for segment in relative.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return None;
        }
        path.push(segment);
    }
    Some(path)
}

/// Normalize one theme asset reference. Absolute paths, drive letters and any
/// parent traversal are rejected before the value can reach the asset scheme.
fn normalize_asset_path(raw: &str) -> Option<String> {
    let value = raw.trim().replace('\\', "/");
    if value.is_empty() || value.starts_with('/') || value.contains(':') {
        return None;
    }
    let mut segments: Vec<&str> = Vec::new();
    for segment in value.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return None;
        }
        segments.push(segment);
    }
    (!segments.is_empty()).then(|| segments.join("/"))
}

fn asset_extension(path: &str) -> Option<&'static str> {
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())?;
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "avif" => Some("image/avif"),
        "svg" => Some("image/svg+xml"),
        "woff2" => Some("font/woff2"),
        _ => None,
    }
}

/// The URL the renderer can load from CSS. Tauri serves custom schemes through
/// `localhost` on macOS/Linux and `http://<scheme>.localhost` on Windows.
fn plugin_asset_url(plugin_id: &str, asset: &str) -> Option<String> {
    let base = if cfg!(target_os = "windows") {
        format!("http://plugin-asset.localhost/{plugin_id}/")
    } else {
        format!("plugin-asset://localhost/{plugin_id}/")
    };
    url::Url::parse(&base)
        .ok()?
        .join(asset)
        .ok()
        .map(|url| url.to_string())
}

fn declared_theme_assets(manifest: &Value) -> Vec<String> {
    contributions(manifest, "themes")
        .iter()
        .filter_map(|entry| entry.get("assets").and_then(Value::as_array))
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(normalize_asset_path)
        .collect()
}

/// Split a scheme request into `(pluginId, assetPath)`, accepting both Tauri
/// platform shapes and a host-as-plugin-id form.
fn asset_request_path(raw: &str) -> Option<(String, String)> {
    let url = url::Url::parse(raw).ok()?;
    let scheme = url.scheme();
    let host = url.host_str()?.to_ascii_lowercase();
    if !(scheme == "plugin-asset" || (scheme == "http" && host == "plugin-asset.localhost")) {
        return None;
    }
    let segments = url
        .path_segments()?
        .map(|segment| {
            percent_encoding::percent_decode_str(segment)
                .decode_utf8()
                .ok()
                .map(|value| value.into_owned())
        })
        .collect::<Option<Vec<String>>>()?;
    let (plugin_id, rest) = if host == "localhost" || host == "plugin-asset.localhost" {
        (segments.first()?.clone(), &segments[1..])
    } else {
        (host, &segments[..])
    };
    if plugin_id.is_empty() || rest.is_empty() {
        return None;
    }
    let path = normalize_asset_path(&rest.join("/"))?;
    Some((plugin_id, path))
}

pub(crate) fn active_locale() -> String {
    sys_locale::get_locale()
        .unwrap_or_else(|| "en".to_string())
        .to_lowercase()
}

/// The tag a plugin sees from `pi.app.getLocale`, in the system's own casing.
pub(crate) fn system_locale() -> String {
    sys_locale::get_locale().unwrap_or_else(|| "en".to_string())
}

pub(crate) fn localized(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Object(map) => {
            let locale = active_locale();
            if locale.starts_with("zh") {
                if let Some(text) = map.get("zh-CN").and_then(|entry| entry.as_str()) {
                    return text.to_string();
                }
            }
            if let Some(text) = map.get("en").and_then(|entry| entry.as_str()) {
                return text.to_string();
            }
            map.values()
                .find_map(|entry| entry.as_str())
                .unwrap_or("")
                .to_string()
        }
        _ => String::new(),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// `contributes.views` as the work panel's view metadata, with titles resolved
/// for the active locale and entries checked against the plugin package.
pub async fn views(state: &AppState) -> Value {
    let project = current_project_path(state).await;
    let rows = match plugin_rows(state).await {
        Ok(rows) => rows,
        Err(error) => return error,
    };
    let mut views: Vec<Value> = Vec::new();
    for plugin in &rows {
        if !active_in_project(plugin, project.as_deref()) || !has_permission(plugin, "ui.view") {
            continue;
        }
        let Some(manifest) = read_manifest(plugin) else {
            continue;
        };
        let Some(root) = plugin_root(plugin) else {
            continue;
        };
        let id = plugin_id(plugin);
        if id.is_empty() {
            continue;
        }
        for (index, entry) in contributions(&manifest, "views").iter().enumerate() {
            let Some(object) = entry.as_object() else {
                continue;
            };
            let Some(view_id) = object.get("id").and_then(|value| value.as_str()) else {
                continue;
            };
            let Some(entry_path) = object.get("entry").and_then(|value| value.as_str()) else {
                continue;
            };
            let Some(resolved) = safe_join(&root, entry_path) else {
                continue;
            };
            if !resolved.is_file() {
                continue;
            }
            let title = object.get("title").map(localized).unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let mut view = json!({
                "pluginId": id,
                "viewId": view_id,
                "ref": format!("{id}/{view_id}"),
                "title": title,
                "pluginName": plugin_name(plugin),
                "order": object.get("order").and_then(|value| value.as_i64()).unwrap_or(index as i64),
            });
            if let Some(icon) = object.get("icon").and_then(|value| value.as_str()) {
                view["icon"] = json!(icon);
            }
            views.push(view);
        }
    }
    views.sort_by(|left, right| {
        let left_order = left
            .get("order")
            .and_then(|value| value.as_i64())
            .unwrap_or(0);
        let right_order = right
            .get("order")
            .and_then(|value| value.as_i64())
            .unwrap_or(0);
        left_order
            .cmp(&right_order)
            .then_with(|| left["ref"].as_str().cmp(&right["ref"].as_str()))
    });
    ok(Value::Array(views))
}

/// `contributes.themes` with sanitized CSS. A sheet this build cannot verify is
/// skipped rather than injected.
pub async fn themes(state: &AppState) -> Value {
    let rows = match plugin_rows(state).await {
        Ok(rows) => rows,
        Err(error) => return error,
    };
    let mut themes: Vec<Value> = Vec::new();
    for plugin in &rows {
        if !plugin
            .get("enabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
            || !has_permission(plugin, "ui.theme")
        {
            continue;
        }
        let Some(manifest) = read_manifest(plugin) else {
            continue;
        };
        let Some(root) = plugin_root(plugin) else {
            continue;
        };
        let id = plugin_id(plugin);
        if id.is_empty() {
            continue;
        }
        let appearance = manifest
            .pointer("/contributes/windowAppearance/backgroundColor")
            .filter(|_| has_permission(plugin, "ui.window.appearance"))
            .cloned();
        for entry in contributions(&manifest, "themes") {
            let Some(object) = entry.as_object() else {
                continue;
            };
            let Some(theme_id) = object.get("id").and_then(|value| value.as_str()) else {
                continue;
            };
            let label = object
                .get("label")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim();
            if label.is_empty() {
                continue;
            }
            let Some(path) = object.get("path").and_then(|value| value.as_str()) else {
                continue;
            };
            let Some(resolved) = safe_join(&root, path) else {
                continue;
            };
            let Ok(metadata) = std::fs::metadata(&resolved) else {
                continue;
            };
            if !metadata.is_file() || metadata.len() > THEME_CSS_MAX_BYTES {
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(&resolved) else {
                continue;
            };
            let assets: Vec<String> = object
                .get("assets")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let Ok(css) = sanitize_theme_css_with_assets(&raw, id, &assets) else {
                continue;
            };
            let base = object
                .get("base")
                .and_then(|value| value.as_str())
                .filter(|value| *value == "light")
                .unwrap_or("dark");
            let mut theme = json!({
                "id": format!("plugin:{id}:{theme_id}"),
                "pluginId": id,
                "themeId": theme_id,
                "label": label,
                "base": base,
                "css": css,
            });
            if let Some(background) = &appearance {
                theme["windowBackground"] = background.clone();
            }
            themes.push(theme);
        }
    }
    // Runtime `pi.themes.upsert` rows have no manifest entry; they still
    // belong to the picker while the owning plugin is enabled and granted.
    for theme in state.plugins.runtime_themes() {
        let Some(owner) = theme.get("pluginId").and_then(|value| value.as_str()) else {
            continue;
        };
        let allowed = rows.iter().any(|plugin| {
            plugin_id(plugin) == owner
                && plugin
                    .get("enabled")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false)
                && has_permission(plugin, "ui.theme")
        });
        if allowed {
            themes.push(theme);
        }
    }
    ok(Value::Array(themes))
}

fn asset_http_response(
    status: tauri::http::StatusCode,
    content_type: Option<&str>,
    body: Vec<u8>,
) -> tauri::http::Response<Vec<u8>> {
    use tauri::http::header;
    let mut builder = tauri::http::Response::builder()
        .status(status)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CACHE_CONTROL, "no-cache");
    if let Some(value) = content_type {
        builder = builder.header(header::CONTENT_TYPE, value);
    }
    builder
        .body(body)
        .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
}

/// Serve one declared theme asset over the `plugin-asset` scheme. The request
/// must name an enabled plugin with `ui.theme`, the path must be declared by one
/// of its themes, and the canonical file must stay inside the plugin root.
pub async fn asset_response(
    state: &AppState,
    request: tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    use tauri::http::{Method, StatusCode};
    let missing = || asset_http_response(StatusCode::NOT_FOUND, None, Vec::new());
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return asset_http_response(StatusCode::METHOD_NOT_ALLOWED, None, Vec::new());
    }
    let Some((plugin_key, asset)) = asset_request_path(&request.uri().to_string()) else {
        return missing();
    };
    let Some(content_type) = asset_extension(&asset) else {
        return missing();
    };
    let rows = match plugin_rows(state).await {
        Ok(rows) => rows,
        Err(_) => {
            return asset_http_response(StatusCode::SERVICE_UNAVAILABLE, None, Vec::new());
        }
    };
    let Some(plugin) = rows.iter().find(|plugin| plugin_id(plugin) == plugin_key) else {
        return missing();
    };
    if !plugin
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || !has_permission(plugin, "ui.theme")
    {
        return missing();
    }
    let Some(root) = plugin_root(plugin) else {
        return missing();
    };
    let Some(manifest) = read_manifest(plugin) else {
        return missing();
    };
    if !declared_theme_assets(&manifest)
        .iter()
        .any(|declared| declared == &asset)
    {
        return missing();
    }
    let Some(resolved) = safe_join(&root, &asset) else {
        return missing();
    };
    let (Ok(root_canonical), Ok(file_canonical)) = (root.canonicalize(), resolved.canonicalize())
    else {
        return missing();
    };
    if !file_canonical.starts_with(&root_canonical) {
        return missing();
    }
    let Ok(metadata) = std::fs::metadata(&file_canonical) else {
        return missing();
    };
    if !metadata.is_file() || metadata.len() > THEME_ASSET_MAX_BYTES {
        return missing();
    }
    if request.method() == Method::HEAD {
        return asset_http_response(StatusCode::OK, Some(content_type), Vec::new());
    }
    let bytes =
        match tauri::async_runtime::spawn_blocking(move || std::fs::read(file_canonical)).await {
            Ok(Ok(bytes)) => bytes,
            _ => {
                return asset_http_response(StatusCode::INTERNAL_SERVER_ERROR, None, Vec::new());
            }
        };
    asset_http_response(StatusCode::OK, Some(content_type), bytes)
}

pub(crate) struct PanelMeta {
    pub entry: String,
    pub title: String,
    pub width: u64,
    pub height: u64,
}

/// Resolve `manifest.ui` into window metadata. Width/height are clamped so a
/// manifest cannot open a surface larger than the screen or smaller than a
/// usable panel.
pub(crate) fn panel_meta(manifest: &Value, fallback_name: &str) -> Option<PanelMeta> {
    let entry = manifest
        .pointer("/ui/panel")
        .and_then(|value| value.as_str())?
        .trim()
        .to_string();
    if entry.is_empty() {
        return None;
    }
    let title = manifest
        .pointer("/ui/title")
        .map(localized)
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| fallback_name.to_string());
    let width = manifest
        .pointer("/ui/width")
        .and_then(Value::as_u64)
        .unwrap_or(520)
        .clamp(320, 1600);
    let height = manifest
        .pointer("/ui/height")
        .and_then(Value::as_u64)
        .unwrap_or(640)
        .clamp(240, 1200);
    Some(PanelMeta {
        entry,
        title,
        width,
        height,
    })
}

fn surface_url(state: &AppState, plugin: &Value, entry: &str) -> Result<String, Value> {
    let Some(server) = state.surfaces.as_ref() else {
        return Err(err(
            "PLUGIN_SURFACE_UNAVAILABLE",
            "the plugin surface service could not start",
        ));
    };
    let Some(root) = plugin_root(plugin) else {
        return Err(err("PLUGIN_INVALID", "plugin path is missing"));
    };
    let Some(path) = safe_join(&root, entry) else {
        return Err(err("PLUGIN_INVALID", "surface entry escapes the package"));
    };
    server
        .register(plugin_id(plugin), &root, &path)
        .ok_or_else(|| {
            err(
                "PLUGIN_SURFACE_NOT_FOUND",
                format!("surface entry \"{entry}\" is not a readable file"),
            )
        })
}

/// `plugin/openPanel`: resolve `ui.panel` and hand the renderer an iframe URL.
pub async fn open_panel(state: &AppState, params: &Value) -> Value {
    let id = params
        .get("id")
        .or_else(|| params.get("pluginId"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let project = current_project_path(state).await;
    let rows = match plugin_rows(state).await {
        Ok(rows) => rows,
        Err(error) => return error,
    };
    let Some(plugin) = find_plugin(&rows, id) else {
        return err("PLUGIN_NOT_FOUND", format!("unknown plugin \"{id}\""));
    };
    if !active_in_project(plugin, project.as_deref()) {
        return err(
            "PLUGIN_NOT_ACTIVE",
            format!("plugin \"{id}\" is not active"),
        );
    }
    if !has_permission(plugin, "ui.panel") {
        return err("PLUGIN_PERMISSION_DENIED", "ui.panel is not granted");
    }
    let Some(manifest) = read_manifest(plugin) else {
        return err("PLUGIN_INVALID", "manifest.json is unreadable");
    };
    let Some(meta) = panel_meta(&manifest, plugin_name(plugin)) else {
        return err(
            "PLUGIN_PANEL_NOT_DECLARED",
            "manifest.ui.panel is not declared",
        );
    };
    match surface_url(state, plugin, &meta.entry) {
        Ok(url) => ok(json!({
            "pluginId": id,
            "url": url,
            "title": meta.title,
            "width": meta.width,
            "height": meta.height,
        })),
        Err(error) => error,
    }
}

/// `plugin/view/open`: resolve one `contributes.views` entry and hand the
/// renderer the iframe URL for the docked tab.
pub async fn view_open(state: &AppState, params: &Value) -> Value {
    let plugin_id = params
        .get("pluginId")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let view_id = params
        .get("viewId")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if plugin_id.is_empty() || view_id.is_empty() {
        return err("INVALID_ARGUMENT", "pluginId and viewId are required");
    }
    let project = current_project_path(state).await;
    let rows = match plugin_rows(state).await {
        Ok(rows) => rows,
        Err(error) => return error,
    };
    let Some(plugin) = find_plugin(&rows, plugin_id) else {
        return err(
            "PLUGIN_NOT_FOUND",
            format!("unknown plugin \"{plugin_id}\""),
        );
    };
    if !active_in_project(plugin, project.as_deref()) {
        return err(
            "PLUGIN_NOT_ACTIVE",
            format!("plugin \"{plugin_id}\" is not active"),
        );
    }
    if !has_permission(plugin, "ui.view") {
        return err("PLUGIN_PERMISSION_DENIED", "ui.view is not granted");
    }
    let Some(manifest) = read_manifest(plugin) else {
        return err("PLUGIN_INVALID", "manifest.json is unreadable");
    };
    let entry = contributions(&manifest, "views")
        .iter()
        .find(|entry| entry.get("id").and_then(|value| value.as_str()) == Some(view_id))
        .and_then(|entry| entry.get("entry").and_then(|value| value.as_str()))
        .map(str::to_string);
    let Some(entry) = entry else {
        return err(
            "PLUGIN_VIEW_NOT_FOUND",
            format!("plugin \"{plugin_id}\" has no view \"{view_id}\""),
        );
    };
    match surface_url(state, plugin, &entry) {
        Ok(url) => ok(json!({ "url": url })),
        Err(error) => error,
    }
}

/// Declared resident services with the supervisor's live state. A declared
/// service whose plugin is not loaded is `stopped`, never invented as running.
pub async fn services(state: &AppState) -> Value {
    let rows = match plugin_rows(state).await {
        Ok(rows) => rows,
        Err(error) => return error,
    };
    let updated_at = now_ms();
    let mut services: Vec<Value> = Vec::new();
    for plugin in &rows {
        if !plugin
            .get("enabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
            || !has_permission(plugin, "background.service")
        {
            continue;
        }
        let Some(manifest) = read_manifest(plugin) else {
            continue;
        };
        let id = plugin_id(plugin);
        for entry in contributions(&manifest, "services") {
            let Some(object) = entry.as_object() else {
                continue;
            };
            let Some(service_id) = object.get("id").and_then(|value| value.as_str()) else {
                continue;
            };
            let label = object
                .get("label")
                .map(localized)
                .unwrap_or_else(|| service_id.to_string());
            let mut row = json!({
                "pluginId": id,
                "serviceId": service_id,
                "label": label,
                "state": "stopped",
                "restarts": 0,
                "updatedAt": updated_at,
            });
            if let Some(live) = state.plugins.service_state(id, service_id) {
                if let Some(live_label) = live
                    .get("label")
                    .and_then(Value::as_str)
                    .filter(|label| !label.is_empty())
                {
                    row["label"] = json!(live_label);
                }
                for key in ["state", "restarts", "updatedAt", "message"] {
                    if let Some(value) = live.get(key) {
                        row[key] = value.clone();
                    }
                }
            }
            services.push(row);
        }
    }
    ok(Value::Array(services))
}

fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("plugin-settings.json")
}

pub(crate) fn load_settings_map(data_dir: &Path) -> Map<String, Value> {
    std::fs::read_to_string(settings_path(data_dir))
        .ok()
        .and_then(|raw| serde_json::from_str::<Map<String, Value>>(&raw).ok())
        .unwrap_or_default()
}

fn save_settings_map(data_dir: &Path, settings: &Map<String, Value>) -> Result<(), String> {
    let path = settings_path(data_dir);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let raw = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    std::fs::write(&path, raw).map_err(|error| error.to_string())
}

/// Stored values for one plugin (declared keys only, as last saved).
pub(crate) fn stored_plugin_values(data_dir: &Path, id: &str) -> Map<String, Value> {
    load_settings_map(data_dir)
        .get(id)
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default()
}

fn declared_setting_keys(manifest: &Value) -> Vec<String> {
    manifest
        .get("contributes")
        .and_then(|value| value.get("settings"))
        .and_then(|value| value.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("key").and_then(|value| value.as_str()))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Merge and persist a plugin's own settings write. Only keys declared in the
/// manifest are accepted; an undeclared key is a plugin bug, not state.
pub(crate) fn apply_plugin_settings(
    data_dir: &Path,
    id: &str,
    manifest: &Value,
    values: &Map<String, Value>,
) -> Result<Map<String, Value>, String> {
    let declared = declared_setting_keys(manifest);
    let accepted: Map<String, Value> = values
        .iter()
        .filter(|(key, _)| declared.contains(key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let _guard = settings_lock().lock().unwrap();
    let mut stored = load_settings_map(data_dir);
    let mut plugin_values = stored
        .get(id)
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default();
    for (key, value) in &accepted {
        plugin_values.insert(key.clone(), value.clone());
    }
    stored.insert(id.to_string(), Value::Object(plugin_values.clone()));
    save_settings_map(data_dir, &stored)?;
    Ok(plugin_values)
}

fn plugin_settings_definitions(plugin: &Value) -> Vec<Value> {
    plugin
        .get("settings")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
}

fn merge_setting_values(definitions: &mut [Value], values: Option<&Value>) {
    let Some(values) = values.and_then(|value| value.as_object()) else {
        return;
    };
    for definition in definitions {
        let Some(key) = definition.get("key").and_then(|value| value.as_str()) else {
            continue;
        };
        if let Some(value) = values.get(key) {
            definition["value"] = value.clone();
        }
    }
}

pub(crate) fn find_plugin<'a>(rows: &'a [Value], id: &str) -> Option<&'a Value> {
    rows.iter().find(|plugin| plugin_id(plugin) == id)
}

pub async fn settings_get(state: &AppState, params: &Value) -> Value {
    let id = params
        .as_str()
        .map(str::to_string)
        .or_else(|| {
            params
                .get("id")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default();
    let rows = match plugin_rows(state).await {
        Ok(rows) => rows,
        Err(error) => return error,
    };
    let Some(plugin) = find_plugin(&rows, &id) else {
        return err("PLUGIN_NOT_FOUND", format!("unknown plugin \"{id}\""));
    };
    let mut definitions = plugin_settings_definitions(plugin);
    let stored = stored_plugin_values(&state.data_dir, &id);
    merge_setting_values(&mut definitions, Some(&Value::Object(stored)));
    ok(json!({ "settings": definitions }))
}

pub async fn settings_set(state: &AppState, params: &Value) -> Value {
    let id = params
        .get("id")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let values = params.get("settings").cloned().unwrap_or(json!({}));
    if !values.is_object() {
        return err("INVALID_ARGUMENT", "settings must be an object");
    }
    let rows = match plugin_rows(state).await {
        Ok(rows) => rows,
        Err(error) => return error,
    };
    let Some(plugin) = find_plugin(&rows, &id) else {
        return err("PLUGIN_NOT_FOUND", format!("unknown plugin \"{id}\""));
    };
    let Some(manifest) = read_manifest(plugin) else {
        return err("PLUGIN_INVALID", "manifest.json is unreadable");
    };
    let values = values.as_object().cloned().unwrap_or_default();
    let resolved = match apply_plugin_settings(&state.data_dir, &id, &manifest, &values) {
        Ok(values) => values,
        Err(message) => return err("INTERNAL", message),
    };
    // A running plugin sees the new values immediately, without a reload.
    state.plugins.push_settings(&id, &resolved).await;
    let mut definitions = plugin_settings_definitions(plugin);
    merge_setting_values(&mut definitions, Some(&Value::Object(resolved)));
    ok(json!({ "settings": definitions }))
}

// ── theme css sanitizer ─────────────────────────────────────────────────────

/// CSS escape decoding (`\69` hex and `\c` single-character escapes), so an
/// escaped `@\69mport` is judged as the keyword the browser would see.
fn decode_css_escapes(css: &str) -> String {
    let chars: Vec<char> = css.chars().collect();
    let mut out = String::with_capacity(css.len());
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if ch != '\\' {
            out.push(ch);
            index += 1;
            continue;
        }
        if index + 1 >= chars.len() {
            out.push(ch);
            break;
        }
        let next = chars[index + 1];
        if next.is_ascii_hexdigit() {
            let mut hexadecimal = String::new();
            let mut cursor = index + 1;
            while cursor < chars.len() && hexadecimal.len() < 6 && chars[cursor].is_ascii_hexdigit()
            {
                hexadecimal.push(chars[cursor]);
                cursor += 1;
            }
            if cursor < chars.len() && chars[cursor].is_whitespace() {
                cursor += 1;
            }
            if let Ok(code_point) = u32::from_str_radix(&hexadecimal, 16) {
                if code_point == 0
                    || code_point > 0x10ffff
                    || (0xd800..=0xdfff).contains(&code_point)
                {
                    out.push('\u{fffd}');
                } else if let Some(decoded) = char::from_u32(code_point) {
                    out.push(decoded);
                }
            }
            index = cursor;
            continue;
        }
        out.push(next);
        index += 2;
    }
    out
}

fn url_references(css: &str) -> Result<Vec<String>, String> {
    let bytes = css.as_bytes();
    let mut targets = Vec::new();
    let mut cursor = 0;
    while cursor + 4 <= bytes.len() {
        let is_url = bytes[cursor].eq_ignore_ascii_case(&b'u')
            && bytes[cursor + 1].eq_ignore_ascii_case(&b'r')
            && bytes[cursor + 2].eq_ignore_ascii_case(&b'l')
            && bytes[cursor + 3] == b'(';
        if !is_url {
            cursor += 1;
            continue;
        }
        let start = cursor + 4;
        let rest = &css[start..];
        let Some(end) = rest.find(')') else {
            return Err("theme css contains a malformed url() reference".into());
        };
        let target = rest[..end]
            .trim()
            .trim_matches(|ch| ch == '"' || ch == '\'')
            .trim();
        targets.push(target.to_string());
        cursor = start + end + 1;
    }
    Ok(targets)
}

fn contains_script_expression(css_lower: &str) -> bool {
    let mut cursor = 0;
    while let Some(offset) = css_lower[cursor..].find("expression") {
        let after = cursor + offset + "expression".len();
        if css_lower[after..].trim_start().starts_with('(') {
            return true;
        }
        cursor = after;
    }
    false
}

fn find_violation(css: &str) -> Option<String> {
    find_violation_with(css, &|_| None)
}

fn find_violation_with<F>(css: &str, resolve_asset: &F) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    let lower = css.to_lowercase();
    if lower.contains("@import") {
        return Some("theme css must not use @import".into());
    }
    if lower.contains("<style") || lower.contains("</style") || css.contains("<!--") {
        return Some("theme css must not contain markup".into());
    }
    if lower.contains("javascript:") || contains_script_expression(&lower) {
        return Some("theme css must not contain script expressions".into());
    }
    match url_references(css) {
        Err(error) => Some(error),
        Ok(targets) => {
            for target in targets {
                if target.to_lowercase().starts_with("data:") {
                    continue;
                }
                if resolve_asset(&target).is_none() {
                    return Some(format!(
                        "theme css may only reference data: urls or declared assets (found \"{target}\")"
                    ));
                }
            }
            None
        }
    }
}

/// Rewrite only the declared-asset `url()` targets, leaving `data:` and every
/// other token byte-for-byte intact.
fn rewrite_asset_urls<F>(css: &str, resolve_asset: &F) -> Result<String, String>
where
    F: Fn(&str) -> Option<String>,
{
    let bytes = css.as_bytes();
    let mut out = String::with_capacity(css.len() + 64);
    let mut cursor = 0;
    while cursor < bytes.len() {
        let is_url = cursor + 4 <= bytes.len()
            && bytes[cursor].eq_ignore_ascii_case(&b'u')
            && bytes[cursor + 1].eq_ignore_ascii_case(&b'r')
            && bytes[cursor + 2].eq_ignore_ascii_case(&b'l')
            && bytes[cursor + 3] == b'(';
        if !is_url {
            let ch = css[cursor..]
                .chars()
                .next()
                .ok_or_else(|| "theme css is not valid UTF-8".to_string())?;
            out.push(ch);
            cursor += ch.len_utf8();
            continue;
        }
        let start = cursor + 4;
        let rest = &css[start..];
        let Some(end) = rest.find(')') else {
            return Err("theme css contains a malformed url() reference".into());
        };
        let raw_target = rest[..end].trim();
        let target = raw_target.trim_matches(|ch| ch == '"' || ch == '\'').trim();
        out.push_str(&css[cursor..start]);
        if let Some(replacement) = resolve_asset(target) {
            out.push('"');
            out.push_str(&replacement);
            out.push('"');
        } else {
            out.push_str(raw_target);
        }
        out.push(')');
        cursor = start + end + 1;
    }
    Ok(out)
}

fn base_theme_css(raw: &str) -> Result<&str, String> {
    let css = raw.trim_start_matches('\u{feff}');
    let bytes = css.as_bytes().len() as u64;
    if bytes > THEME_CSS_MAX_BYTES {
        return Err(format!(
            "theme css exceeds {THEME_CSS_MAX_BYTES} bytes ({bytes})"
        ));
    }
    if css.trim().is_empty() {
        return Err("theme css is empty".into());
    }
    Ok(css)
}

pub fn sanitize_theme_css(raw: &str) -> Result<String, String> {
    let css = base_theme_css(raw)?;
    if let Some(violation) = find_violation(css) {
        return Err(violation);
    }
    let decoded = decode_css_escapes(css);
    if decoded != css {
        if let Some(violation) = find_violation(&decoded) {
            return Err(violation);
        }
    }
    Ok(css.trim().to_string())
}

/// Theme CSS for a manifest theme: `data:` and the theme's declared assets are
/// allowed, and each declared asset reference is rewritten to the
/// `plugin-asset://` URL the webview can load.
pub fn sanitize_theme_css_with_assets(
    raw: &str,
    plugin_id: &str,
    assets: &[String],
) -> Result<String, String> {
    let css = base_theme_css(raw)?;
    let declared: Vec<String> = assets
        .iter()
        .filter_map(|asset| normalize_asset_path(asset))
        .filter(|asset| asset_extension(asset).is_some())
        .collect();
    let resolve = |target: &str| -> Option<String> {
        let normalized = normalize_asset_path(target)?;
        if !declared.iter().any(|asset| asset == &normalized) {
            return None;
        }
        plugin_asset_url(plugin_id, &normalized)
    };
    if let Some(violation) = find_violation_with(css, &resolve) {
        return Err(violation);
    }
    let decoded = decode_css_escapes(css);
    if decoded != css {
        if let Some(violation) = find_violation_with(&decoded, &resolve) {
            return Err(violation);
        }
    }
    Ok(rewrite_asset_urls(css, &resolve)?.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_honours_project_scope() {
        let plugin = json!({
            "enabled": true,
            "scope": { "mode": "projects", "projects": ["/Users/me/proj"] }
        });
        assert!(!active_in_project(&plugin, None));
        assert!(active_in_project(&plugin, Some("/Users/me/proj")));
        assert!(active_in_project(
            &plugin,
            Some("/Users/me/proj/packages/app")
        ));
        assert!(!active_in_project(&plugin, Some("/Users/me/other")));
    }

    #[test]
    fn a_global_plugin_is_active_without_a_project() {
        let plugin = json!({ "enabled": true });
        assert!(active_in_project(&plugin, None));
    }

    #[test]
    fn a_theme_may_not_load_remote_resources() {
        assert!(sanitize_theme_css(":root { --bg: #fff; }").is_ok());
        assert!(
            sanitize_theme_css(":root { background: url(https://example.com/x.png); }").is_err()
        );
        assert!(sanitize_theme_css("@\\69mport \"x.css\";").is_err());
        assert!(sanitize_theme_css(":root { background: url(data:image/png;base64,AA); }").is_ok());
    }

    #[test]
    fn a_theme_with_markup_is_rejected() {
        assert!(sanitize_theme_css("</style><script>alert(1)</script>").is_err());
    }

    #[test]
    fn declared_theme_assets_are_rewritten_to_the_asset_scheme() {
        let assets = vec!["art/bg.png".to_string(), "font/ui.woff2".to_string()];
        let css = ":root { background: url('./art/bg.png'); } \
                   @font-face { src: url(font/ui.woff2) format('woff2'); }";
        let rewritten = sanitize_theme_css_with_assets(css, "demo.plugin", &assets).expect("css");
        let background = plugin_asset_url("demo.plugin", "art/bg.png").expect("asset url");
        let font = plugin_asset_url("demo.plugin", "font/ui.woff2").expect("asset url");
        assert!(rewritten.contains(&format!("\"{background}\"")));
        assert!(rewritten.contains(&format!("\"{font}\"")));
        assert!(!rewritten.contains("./art/bg.png"));

        assert!(sanitize_theme_css_with_assets(
            ":root { background: url(art/other.png); }",
            "demo.plugin",
            &assets
        )
        .is_err());
        assert!(sanitize_theme_css_with_assets(
            ":root { background: url(https://example.com/x.png); }",
            "demo.plugin",
            &assets
        )
        .is_err());
        assert!(sanitize_theme_css_with_assets(
            ":root { background: url(../secret.png); }",
            "demo.plugin",
            &["../secret.png".to_string()]
        )
        .is_err());
    }

    #[test]
    fn asset_paths_reject_traversal_and_absolute_forms() {
        assert_eq!(
            normalize_asset_path("./art//bg.png"),
            Some("art/bg.png".to_string())
        );
        assert_eq!(normalize_asset_path("../x.png"), None);
        assert_eq!(normalize_asset_path("/etc/passwd"), None);
        assert_eq!(normalize_asset_path("C:\\x.png"), None);
        assert_eq!(asset_extension("art/bg.png"), Some("image/png"));
        assert_eq!(asset_extension("font/ui.woff2"), Some("font/woff2"));
        assert_eq!(asset_extension("theme.css"), None);
        assert_eq!(asset_extension("art/run.exe"), None);
    }

    #[test]
    fn asset_requests_accept_both_tauri_scheme_shapes() {
        assert_eq!(
            asset_request_path("plugin-asset://localhost/demo.plugin/art/bg.png"),
            Some(("demo.plugin".to_string(), "art/bg.png".to_string()))
        );
        assert_eq!(
            asset_request_path("http://plugin-asset.localhost/demo.plugin/art/bg.png"),
            Some(("demo.plugin".to_string(), "art/bg.png".to_string()))
        );
        // A browser that keeps the plugin id in the host still resolves.
        assert_eq!(
            asset_request_path("plugin-asset://demo.plugin/art/bg.png"),
            Some(("demo.plugin".to_string(), "art/bg.png".to_string()))
        );
        assert_eq!(
            asset_request_path("plugin-asset://localhost/demo.plugin/art/a%20b.png"),
            Some(("demo.plugin".to_string(), "art/a b.png".to_string()))
        );
        assert_eq!(asset_request_path("https://example.com/art/bg.png"), None);
        assert_eq!(
            asset_request_path("plugin-asset://localhost/demo.plugin/"),
            None
        );
    }

    #[test]
    fn panel_metadata_is_clamped_and_falls_back_to_the_plugin_name() {
        let manifest = json!({
            "name": "Demo",
            "ui": { "panel": "renderer/index.html", "width": 120, "height": 4000 }
        });
        let meta = panel_meta(&manifest, "Demo plugin").expect("panel metadata");
        assert_eq!(meta.entry, "renderer/index.html");
        assert_eq!(meta.title, "Demo plugin");
        assert_eq!(meta.width, 320);
        assert_eq!(meta.height, 1200);

        let manifest = json!({ "ui": { "title": "Panelled", "panel": "panel.html" } });
        let meta = panel_meta(&manifest, "fallback").expect("panel metadata");
        assert_eq!(meta.title, "Panelled");
        assert_eq!(meta.width, 520);
        assert_eq!(meta.height, 640);

        assert!(panel_meta(&json!({ "ui": {} }), "none").is_none());
    }

    #[test]
    fn localized_titles_prefer_the_english_fallback() {
        let title = json!({ "en": "Board", "zh-CN": "看板" });
        assert!(!localized(&title).is_empty());
    }
}
