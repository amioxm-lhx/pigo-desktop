//! Host-owned browser guest (ADR 0170, Tauri port).
//!
//! Electron stacked a `WebContentsView` under plugin chrome. Tauri has no
//! `WebContentsView`, so the guest is a child webview of the main window that
//! the chrome positions through `browser/setBounds`. The guest is a singleton:
//! every chrome shares it, and bounds are clamped before they reach the
//! platform so a caller cannot cover the chat or composer.
//!
//! WKWebView and WebKitGTK expose no Chrome DevTools Protocol. The `cdp`
//! surface is therefore a deny-by-default allowlist implemented over script
//! evaluation plus the platform snapshot bridge; a method without a faithful
//! local equivalent answers `PERMISSION_DENIED`, never a fake result.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::webview::{PageLoadEvent, Webview, WebviewBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl};

use crate::state::AppState;

const GUEST_LABEL: &str = "work-browser";
const STATE_EVENT: &str = "pi-desktop/browser/event/state";
const EVAL_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_CONSOLE_LINES: usize = 200;

#[derive(Debug)]
pub struct BrowserError {
    pub code: String,
    pub message: String,
}

impl BrowserError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

type BrowserResult<T> = Result<T, BrowserError>;

fn invalid(message: impl Into<String>) -> BrowserError {
    BrowserError::new("INVALID_ARGUMENT", message)
}

fn unavailable() -> BrowserError {
    BrowserError::new(
        "BROWSER_UNAVAILABLE",
        "the browser guest is not available in this window",
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn rect_from(value: Option<&Value>) -> Option<Rect> {
    let value = value?;
    let number = |key: &str| value.get(key).and_then(Value::as_f64);
    let rect = Rect {
        x: number("x")?,
        y: number("y")?,
        width: number("width")?,
        height: number("height")?,
    };
    (rect.width.is_finite() && rect.height.is_finite() && rect.x.is_finite() && rect.y.is_finite())
        .then_some(rect)
}

/// Intersect a hole (relative to `view`) with the view. A hole that leaves no
/// visible area is refused instead of clamping to a one-pixel sliver.
fn clamp_guest(view: Rect, hole: Rect) -> Option<Rect> {
    let left = view.x + hole.x;
    let top = view.y + hole.y;
    let x = left.max(view.x);
    let y = top.max(view.y);
    let right = (left + hole.width).min(view.x + view.width);
    let bottom = (top + hole.height).min(view.y + view.height);
    let width = right - x;
    let height = bottom - y;
    if width < 1.0 || height < 1.0 {
        return None;
    }
    Some(Rect {
        x: x.floor(),
        y: y.floor(),
        width: width.floor(),
        height: height.floor(),
    })
}

fn allowed_scheme(url: &url::Url) -> bool {
    match url.scheme() {
        "http" | "https" => true,
        "about" => url.path() == "blank",
        _ => false,
    }
}

/// Normalize what the chrome typed. A bare host gets `https://`; every other
/// scheme is refused before it reaches the platform webview.
fn normalize_navigation_url(raw: &str) -> BrowserResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(invalid("url is required"));
    }
    let candidate = if trimmed.contains("://") || trimmed.starts_with("about:") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let url =
        url::Url::parse(&candidate).map_err(|error| invalid(format!("invalid URL: {error}")))?;
    if !allowed_scheme(&url) {
        return Err(invalid(format!(
            "{} URLs are not allowed in the browser guest",
            url.scheme()
        )));
    }
    Ok(url.to_string())
}

fn safe_uid(uid: &str) -> bool {
    let bytes = uid.as_bytes();
    bytes.len() > 1 && bytes[0] == b'e' && bytes[1..].iter().all(u8::is_ascii_digit)
}

/// Raw CDP is deny-by-default. Only methods with a faithful evaluation-backed
/// equivalent are allowed; cookies, storage, targets and interception never
/// are (ADR 0170 §4).
fn cdp_allowed(method: &str) -> bool {
    matches!(
        method,
        "Runtime.evaluate" | "Accessibility.getFullAXTree" | "Page.captureScreenshot"
    )
}

/// Patches the page console before any page script runs, so `pi.browser.console`
/// sees the same entries a developer would. Bounded to the newest 200 lines.
const CONSOLE_CAPTURE: &str = r#"
(() => {
  const lines = [];
  const levels = ["log", "info", "warn", "error", "debug"];
  for (const level of levels) {
    const original = console[level];
    console[level] = (...args) => {
      try {
        const text = args
          .map((arg) => {
            if (typeof arg === "string") return arg;
            try { return JSON.stringify(arg); } catch { return String(arg); }
          })
          .join(" ");
        lines.push({ level, text, at: new Date().toISOString() });
        if (lines.length > 200) lines.shift();
      } catch {}
      try { original.apply(console, args); } catch {}
    };
  }
  Object.defineProperty(window, "__PI_CONSOLE__", { value: lines, configurable: true });
})();
"#;

/// One flattened accessibility-style tree. uids are assigned to the live DOM
/// (`data-pi-uid`) so `click`/`fill` address the same nodes `snapshot` showed.
const SNAPSHOT_JS: &str = r#"
(() => {
  const MAX = 800;
  const roleOf = (el) => {
    const tag = el.tagName.toLowerCase();
    if (tag === "button") return "button";
    if (tag === "a") return "link";
    if (tag === "textarea") return "textbox";
    if (tag === "select") return "combobox";
    if (tag === "img") return "image";
    if (/^h[1-6]$/.test(tag)) return "heading";
    if (tag === "input") {
      const type = String(el.type || "").toLowerCase();
      if (type === "checkbox" || type === "radio") return "checkbox";
      if (type === "submit" || type === "button") return "button";
      return "textbox";
    }
    return "generic";
  };
  const nameOf = (el) => {
    const value =
      el.getAttribute("aria-label") ||
      el.getAttribute("alt") ||
      el.getAttribute("placeholder") ||
      (el.textContent || "").trim();
    return String(value || "").slice(0, 160);
  };
  const lines = [];
  let count = 0;
  const walk = (el, depth) => {
    if (count >= MAX) return;
    const tag = el.tagName.toLowerCase();
    if (["script", "style", "noscript", "template"].includes(tag)) return;
    const uid = `e${++count}`;
    el.setAttribute("data-pi-uid", uid);
    const name = nameOf(el);
    lines.push(`${"  ".repeat(Math.min(depth, 12))}- ${uid} ${roleOf(el)}${name ? ` ${JSON.stringify(name)}` : ""}`);
    for (const child of el.children) walk(child, depth + 1);
  };
  if (document.body) walk(document.body, 0);
  return { tree: lines.join("\n") || "(empty)", url: location.href, title: document.title };
})()
"#;

const AX_TREE_JS: &str = r#"
(() => {
  const nodes = [];
  let next = 1;
  const roleOf = (el) => {
    const tag = el.tagName.toLowerCase();
    if (tag === "button") return "button";
    if (tag === "a") return "link";
    if (tag === "textarea" || tag === "input") return "textbox";
    if (tag === "select") return "combobox";
    if (tag === "img") return "image";
    if (/^h[1-6]$/.test(tag)) return "heading";
    return "generic";
  };
  const nameOf = (el) =>
    String(
      el.getAttribute("aria-label") ||
      el.getAttribute("alt") ||
      (el.textContent || "").trim() ||
      "",
    ).slice(0, 160);
  const walk = (el) => {
    const nodeId = String(next++);
    const node = {
      nodeId,
      role: { value: roleOf(el) },
      name: { value: nameOf(el) },
      backendDOMNodeId: next,
      childIds: [],
    };
    nodes.push(node);
    for (const child of el.children) node.childIds.push(walk(child));
    return nodeId;
  };
  if (document.body) walk(document.body);
  return { nodes };
})()
"#;

struct Inner {
    webview: Option<Webview<tauri::Wry>>,
    url: String,
    title: String,
    loading: bool,
    history: Vec<String>,
    cursor: usize,
    bounds: Option<(f64, f64, f64, f64)>,
    visible: bool,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            webview: None,
            url: String::new(),
            title: String::new(),
            loading: false,
            history: Vec::new(),
            cursor: 0,
            bounds: None,
            visible: false,
        }
    }
}

pub struct BrowserHost {
    inner: Mutex<Inner>,
}

impl BrowserHost {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
        })
    }

    fn record_navigation(inner: &mut Inner, url: &str) {
        if inner.history.get(inner.cursor).map(String::as_str) == Some(url) {
            return;
        }
        inner.history.truncate(inner.cursor + 1);
        inner.history.push(url.to_string());
        inner.cursor = inner.history.len() - 1;
    }

    fn snapshot_state(inner: &Inner) -> Option<Value> {
        if inner.webview.is_none() && inner.url.is_empty() {
            return None;
        }
        Some(json!({
            "url": inner.url,
            "title": inner.title,
            "isLoading": inner.loading,
            "canGoBack": inner.cursor > 0,
            "canGoForward": inner.cursor + 1 < inner.history.len(),
        }))
    }

    pub fn state(&self) -> Option<Value> {
        Self::snapshot_state(&self.inner.lock().unwrap())
    }

    fn emit_state(&self, app: &AppHandle) {
        if let Some(state) = self.state() {
            let _ = app.emit(STATE_EVENT, state);
        }
    }

    /// Create the hidden guest on first use. Tauri requires the `unstable`
    /// multiwebview API; the webview is positioned by `set_bounds`.
    fn ensure(self: &Arc<Self>, app: &AppHandle) -> BrowserResult<Webview<tauri::Wry>> {
        if let Some(webview) = self.inner.lock().unwrap().webview.clone() {
            return Ok(webview);
        }
        let window = app.get_window("main").ok_or_else(unavailable)?;
        let weak = Arc::downgrade(self);
        let load_app = app.clone();
        let builder = WebviewBuilder::new(
            GUEST_LABEL,
            WebviewUrl::External("about:blank".parse().map_err(|_| unavailable())?),
        )
        .on_navigation(|url| allowed_scheme(url))
        .initialization_script(CONSOLE_CAPTURE)
        .on_page_load(move |webview, payload| {
            let Some(host) = weak.upgrade() else { return };
            let url = payload.url().to_string();
            match payload.event() {
                PageLoadEvent::Started => {
                    {
                        let mut inner = host.inner.lock().unwrap();
                        inner.url = url;
                        inner.loading = true;
                    }
                    host.emit_state(&load_app);
                }
                PageLoadEvent::Finished => {
                    {
                        let mut inner = host.inner.lock().unwrap();
                        inner.loading = false;
                        if inner.url != url && inner.webview.is_some() {
                            // A link inside the page moved the guest; keep the
                            // history in step so Back has somewhere to go.
                            inner.url = url.clone();
                            Self::record_navigation(&mut inner, &url);
                        }
                    }
                    host.emit_state(&load_app);
                    let weak = weak.clone();
                    let app = load_app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Ok(title) = eval_json(&webview, "document.title").await {
                            if let Some(host) = weak.upgrade() {
                                host.inner.lock().unwrap().title =
                                    title.as_str().unwrap_or_default().to_string();
                                host.emit_state(&app);
                            }
                        }
                    });
                }
            }
        });
        let stored = self.inner.lock().unwrap().bounds;
        let (position, size) = match stored {
            Some((x, y, width, height)) => {
                (LogicalPosition::new(x, y), LogicalSize::new(width, height))
            }
            None => (LogicalPosition::new(0.0, 0.0), LogicalSize::new(1.0, 1.0)),
        };
        let webview = window
            .add_child(builder, position, size)
            .map_err(|error| BrowserError::new("BROWSER_UNAVAILABLE", error.to_string()))?;
        let _ = webview.hide();
        let mut inner = self.inner.lock().unwrap();
        if inner.webview.is_none() {
            inner.webview = Some(webview.clone());
        }
        Ok(webview)
    }

    pub async fn navigate(
        self: &Arc<Self>,
        state: &Arc<AppState>,
        app: &AppHandle,
        input: &Value,
    ) -> BrowserResult<Value> {
        let raw = input
            .get("url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|url| !url.is_empty());
        let path = input
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty());
        let url = match (raw, path) {
            (Some(url), _) => normalize_navigation_url(url)?,
            (None, Some(path)) => preview_url(state, path).await?,
            (None, None) => return Err(invalid("url or path is required")),
        };
        let parsed = url::Url::parse(&url).map_err(|error| invalid(error.to_string()))?;
        let webview = self.ensure(app)?;
        webview
            .navigate(parsed)
            .map_err(|error| BrowserError::new("BROWSER_UNAVAILABLE", error.to_string()))?;
        {
            let mut inner = self.inner.lock().unwrap();
            inner.loading = true;
            inner.url = url.clone();
            Self::record_navigation(&mut inner, &url);
        }
        self.emit_state(app);
        self.state().ok_or_else(unavailable)
    }

    pub fn action(self: &Arc<Self>, app: &AppHandle, action: &str) -> BrowserResult<Value> {
        let webview = {
            let inner = self.inner.lock().unwrap();
            inner.webview.clone()
        };
        let Some(webview) = webview else {
            // No page has been opened yet: Back/Forward/Reload are no-ops.
            return Ok(json!(null));
        };
        match action {
            "back" | "forward" => {
                let target = {
                    let inner = self.inner.lock().unwrap();
                    let next = if action == "back" {
                        inner.cursor.checked_sub(1)
                    } else {
                        (inner.cursor + 1 < inner.history.len()).then_some(inner.cursor + 1)
                    };
                    next.and_then(|index| inner.history.get(index).cloned().map(|url| (index, url)))
                };
                if let Some((index, url)) = target {
                    let parsed =
                        url::Url::parse(&url).map_err(|error| invalid(error.to_string()))?;
                    webview.navigate(parsed).map_err(|error| {
                        BrowserError::new("BROWSER_UNAVAILABLE", error.to_string())
                    })?;
                    let mut inner = self.inner.lock().unwrap();
                    inner.cursor = index;
                    inner.url = url;
                    inner.loading = true;
                }
            }
            "reload" => webview
                .eval("location.reload()")
                .map_err(|error| BrowserError::new("BROWSER_UNAVAILABLE", error.to_string()))?,
            "stop" => webview
                .eval("window.stop()")
                .map_err(|error| BrowserError::new("BROWSER_UNAVAILABLE", error.to_string()))?,
            other => return Err(invalid(format!("unknown browser action \"{other}\""))),
        }
        self.emit_state(app);
        Ok(json!(null))
    }

    /// Position the guest inside the calling chrome. `hole` is relative to
    /// `view`; both are logical pixels relative to the window client area.
    pub fn set_bounds(self: &Arc<Self>, app: &AppHandle, input: &Value) -> BrowserResult<Value> {
        let hole =
            rect_from(Some(input)).ok_or_else(|| invalid("x, y, width and height are required"))?;
        let window = app.get_window("main").ok_or_else(unavailable)?;
        let scale = window.scale_factor().unwrap_or(1.0);
        let size = window.inner_size().map_err(|_| unavailable())?;
        let window_rect = Rect {
            x: 0.0,
            y: 0.0,
            width: size.width as f64 / scale,
            height: size.height as f64 / scale,
        };
        let view = rect_from(input.get("view")).unwrap_or(window_rect);
        let Some(bounds) = clamp_guest(view, hole) else {
            let webview = self.inner.lock().unwrap().webview.clone();
            if let Some(webview) = webview {
                let _ = webview.hide();
            }
            self.inner.lock().unwrap().visible = false;
            return Ok(json!({ "visible": false }));
        };
        let webview = self.inner.lock().unwrap().webview.clone();
        if let Some(webview) = webview {
            webview
                .set_position(LogicalPosition::new(bounds.x, bounds.y))
                .map_err(|error| BrowserError::new("BROWSER_UNAVAILABLE", error.to_string()))?;
            webview
                .set_size(LogicalSize::new(bounds.width, bounds.height))
                .map_err(|error| BrowserError::new("BROWSER_UNAVAILABLE", error.to_string()))?;
        }
        let mut inner = self.inner.lock().unwrap();
        inner.bounds = Some((bounds.x, bounds.y, bounds.width, bounds.height));
        Ok(json!({
            "visible": inner.visible,
            "bounds": { "x": bounds.x, "y": bounds.y, "width": bounds.width, "height": bounds.height },
        }))
    }

    pub fn set_visible(self: &Arc<Self>, visible: bool) -> BrowserResult<Value> {
        let webview = self.inner.lock().unwrap().webview.clone();
        if let Some(webview) = webview {
            if visible {
                if self.inner.lock().unwrap().bounds.is_some() {
                    webview.show().map_err(|error| {
                        BrowserError::new("BROWSER_UNAVAILABLE", error.to_string())
                    })?;
                    self.inner.lock().unwrap().visible = true;
                }
            } else {
                let _ = webview.hide();
                self.inner.lock().unwrap().visible = false;
            }
        }
        Ok(json!(null))
    }

    /// Tear the singleton down (window teardown or explicit close).
    pub fn close(&self) {
        let webview = self.inner.lock().unwrap().webview.take();
        if let Some(webview) = webview {
            let _ = webview.close();
        }
        *self.inner.lock().unwrap() = Inner::default();
    }

    fn require_webview(&self) -> BrowserResult<Webview<tauri::Wry>> {
        self.inner
            .lock()
            .unwrap()
            .webview
            .clone()
            .ok_or_else(unavailable)
    }

    pub async fn snapshot(&self) -> BrowserResult<Value> {
        let webview = self.require_webview()?;
        eval_json(&webview, SNAPSHOT_JS).await
    }

    /// Capture the guest's visible viewport as PNG.
    ///
    /// `fullPage` would need a stitched capture across scroll positions, which
    /// this shell does not implement; it is refused instead of silently
    /// returning a cropped image.
    pub async fn screenshot(&self, full_page: bool) -> BrowserResult<Value> {
        if full_page {
            return Err(BrowserError::new(
                "UNSUPPORTED",
                "full-page screenshots are not available; omit fullPage to capture the visible viewport",
            ));
        }
        let webview = self.require_webview()?;
        let png = capture_png(&webview).await?;
        use base64::Engine as _;
        Ok(json!({
            "mimeType": "image/png",
            "data": base64::engine::general_purpose::STANDARD.encode(png),
        }))
    }

    pub async fn click(&self, uid: &str) -> BrowserResult<Value> {
        if !safe_uid(uid) {
            return Err(invalid("uid must be a snapshot handle like e12"));
        }
        let webview = self.require_webview()?;
        let script = format!(
            r#"(() => {{
              const el = document.querySelector('[data-pi-uid="{}"]');
              if (!el) return {{ error: "unknown or stale uid" }};
              el.scrollIntoView({{ block: "center", inline: "center" }});
              el.click();
              return {{ ok: true }};
            }})()"#,
            uid
        );
        let result = eval_json(&webview, &script).await?;
        if let Some(error) = result.get("error").and_then(Value::as_str) {
            return Err(BrowserError::new("BROWSER_STALE_UID", error.to_string()));
        }
        Ok(result)
    }

    pub async fn fill(&self, uid: &str, text: &str) -> BrowserResult<Value> {
        if !safe_uid(uid) {
            return Err(invalid("uid must be a snapshot handle like e12"));
        }
        let webview = self.require_webview()?;
        let text = serde_json::to_string(text).map_err(|error| invalid(error.to_string()))?;
        let script = format!(
            r#"(() => {{
              const el = document.querySelector('[data-pi-uid="{}"]');
              if (!el) return {{ error: "unknown or stale uid" }};
              el.focus();
              el.value = {};
              el.dispatchEvent(new Event("input", {{ bubbles: true }}));
              el.dispatchEvent(new Event("change", {{ bubbles: true }}));
              return {{ ok: true }};
            }})()"#,
            uid, text
        );
        let result = eval_json(&webview, &script).await?;
        if let Some(error) = result.get("error").and_then(Value::as_str) {
            return Err(BrowserError::new("BROWSER_STALE_UID", error.to_string()));
        }
        Ok(result)
    }

    /// Evaluate one synchronous expression in the page. A Promise is not
    /// awaited: WKWebView's completion handler does not settle it.
    pub async fn evaluate(&self, expression: &str) -> BrowserResult<Value> {
        let webview = self.require_webview()?;
        let expression =
            serde_json::to_string(expression).map_err(|error| invalid(error.to_string()))?;
        let script = format!(
            r#"(() => {{
              try {{
                const value = (0, eval)({expression});
                return {{ value }};
              }} catch (error) {{
                return {{ error: String((error && error.message) || error) }};
              }}
            }})()"#
        );
        let result = eval_json(&webview, &script).await?;
        if let Some(error) = result.get("error").and_then(Value::as_str) {
            return Err(BrowserError::new(
                "BROWSER_EVALUATE_FAILED",
                error.to_string(),
            ));
        }
        Ok(result.get("value").cloned().unwrap_or(Value::Null))
    }

    pub async fn console(&self, limit: usize) -> BrowserResult<Value> {
        let webview = self.require_webview()?;
        let limit = limit.clamp(1, MAX_CONSOLE_LINES);
        let script = format!(
            "(() => {{ const lines = window.__PI_CONSOLE__ || []; return lines.slice(-{limit}); }})()"
        );
        let messages = eval_json(&webview, &script).await?;
        Ok(json!({ "messages": messages }))
    }

    /// Deny-by-default CDP facade over the evaluation surface.
    pub async fn cdp(&self, method: &str, params: &Value) -> BrowserResult<Value> {
        if !cdp_allowed(method) {
            return Err(BrowserError::new(
                "PERMISSION_DENIED",
                format!("CDP method not allowed: {method}"),
            ));
        }
        match method {
            "Runtime.evaluate" => {
                let expression = params
                    .get("expression")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("Runtime.evaluate requires expression"))?;
                let value = self.evaluate(expression).await?;
                Ok(json!({ "result": { "type": json_type(&value), "value": value } }))
            }
            "Accessibility.getFullAXTree" => {
                let webview = self.require_webview()?;
                eval_json(&webview, AX_TREE_JS).await
            }
            "Page.captureScreenshot" => {
                let full_page = params
                    .get("captureBeyondViewport")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let shot = self.screenshot(full_page).await?;
                Ok(json!({ "data": shot.get("data").cloned().unwrap_or(Value::Null) }))
            }
            _ => unreachable!("cdp_allowed gates the match"),
        }
    }
}

/// Snapshot the guest through the platform webview and return PNG bytes.
///
/// WKWebView's snapshot API is asynchronous and main-thread only, so the
/// request is dispatched through `with_webview` and the result is awaited on a
/// blocking task; the agent loop never waits on the UI thread.
#[cfg(target_os = "macos")]
async fn capture_png(webview: &Webview<tauri::Wry>) -> BrowserResult<Vec<u8>> {
    use std::sync::mpsc;

    use block2::RcBlock;
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSImage;
    use objc2_foundation::NSError;
    use objc2_web_kit::{WKSnapshotConfiguration, WKWebView};

    let (tx, rx) = mpsc::channel::<Result<Vec<u8>, String>>();
    let sender = Mutex::new(Some(tx));
    webview
        .with_webview(move |platform| {
            let Some(mtm) = MainThreadMarker::new() else {
                if let Some(sender) = sender.lock().unwrap().take() {
                    let _ = sender.send(Err("the snapshot must run on the main thread".into()));
                }
                return;
            };
            // SAFETY: `with_webview` hands back the WKWebView backing this
            // guest and runs its callback on the main thread.
            unsafe {
                let view = &*platform.inner().cast::<WKWebView>();
                let configuration = WKSnapshotConfiguration::new(mtm);
                let sender = sender.lock().unwrap().take();
                let handler = RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
                    let result = if let Some(error) = error.as_ref() {
                        Err(error.localizedDescription().to_string())
                    } else if let Some(image) = image.as_ref() {
                        png_from_image(image)
                    } else {
                        Err("the webview returned no snapshot".to_string())
                    };
                    if let Some(sender) = sender.as_ref() {
                        let _ = sender.send(result);
                    }
                });
                view.takeSnapshotWithConfiguration_completionHandler(
                    Some(&configuration),
                    &handler,
                );
            }
        })
        .map_err(|error| BrowserError::new("BROWSER_UNAVAILABLE", error.to_string()))?;

    let received = tokio::task::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(10)))
        .await
        .map_err(|error| BrowserError::new("BROWSER_SCREENSHOT_FAILED", error.to_string()))?;
    match received {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(message)) => Err(BrowserError::new("BROWSER_SCREENSHOT_FAILED", message)),
        Err(_) => Err(BrowserError::new(
            "BROWSER_SCREENSHOT_FAILED",
            "the webview did not return a snapshot in time",
        )),
    }
}

/// Encode an AppKit snapshot as PNG. NSImage only exposes TIFF directly, so
/// the bytes are decoded into a bitmap representation and re-encoded.
#[cfg(target_os = "macos")]
unsafe fn png_from_image(image: &objc2_app_kit::NSImage) -> Result<Vec<u8>, String> {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::AllocAnyThread;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep};
    use objc2_foundation::{NSDictionary, NSString};

    let tiff = image
        .TIFFRepresentation()
        .ok_or_else(|| "the snapshot has no bitmap representation".to_string())?;
    let bitmap = NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &tiff)
        .ok_or_else(|| "the snapshot could not be decoded".to_string())?;
    let properties: Retained<NSDictionary<NSString, AnyObject>> =
        NSDictionary::from_slices::<NSString>(&[], &[]);
    bitmap
        .representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)
        .map(|png| png.to_vec())
        .ok_or_else(|| "the snapshot could not be encoded as PNG".to_string())
}

#[cfg(not(target_os = "macos"))]
async fn capture_png(_webview: &Webview<tauri::Wry>) -> BrowserResult<Vec<u8>> {
    Err(BrowserError::new(
        "UNSUPPORTED",
        "browser screenshots need a platform snapshot bridge; only the macOS shell provides one",
    ))
}

fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "undefined",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "object",
        Value::Object(_) => "object",
    }
}

/// Evaluate `js` and parse the platform's JSON-serialized result.
async fn eval_json(webview: &Webview<tauri::Wry>, js: &str) -> BrowserResult<Value> {
    let raw = eval_raw(webview, js).await?;
    if raw.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&raw)
        .map_err(|error| BrowserError::new("BROWSER_EVALUATE_FAILED", error.to_string()))
}

async fn eval_raw(webview: &Webview<tauri::Wry>, js: &str) -> BrowserResult<String> {
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let sender = Mutex::new(Some(tx));
    webview
        .eval_with_callback(js.to_string(), move |value| {
            if let Some(sender) = sender.lock().unwrap().take() {
                let _ = sender.send(value);
            }
        })
        .map_err(|error| BrowserError::new("BROWSER_UNAVAILABLE", error.to_string()))?;
    match tokio::time::timeout(EVAL_TIMEOUT, rx).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(_)) => Err(BrowserError::new(
            "BROWSER_UNAVAILABLE",
            "the browser guest closed during evaluation",
        )),
        Err(_) => Err(BrowserError::new(
            "BROWSER_TIMEOUT",
            "the browser guest did not answer in time",
        )),
    }
}

/// Resolve a workspace-relative HTML file through the loopback preview
/// service, so a plugin can show a local page in the guest the same way the
/// `BrowserPreview` tool does.
async fn preview_url(state: &Arc<AppState>, path: &str) -> BrowserResult<String> {
    let Some(server) = state.preview.as_ref() else {
        return Err(BrowserError::new(
            "BROWSER_UNAVAILABLE",
            "the preview service could not start",
        ));
    };
    let reply = state
        .host
        .call("workspace.get", json!({}))
        .await
        .map_err(|_| invalid("no project is open"))?;
    let Some(root) = reply
        .pointer("/workspace/path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
    else {
        return Err(invalid("no project is open"));
    };
    let root = Path::new(root);
    let file = crate::plugins::safe_join(root, path)
        .ok_or_else(|| invalid("path escapes the workspace"))?;
    let allowed = matches!(
        file.extension().and_then(|value| value.to_str()),
        Some("html") | Some("htm")
    );
    if !allowed || !file.is_file() {
        return Err(invalid("path must be an existing .html or .htm file"));
    }
    server
        .register(root, &file)
        .ok_or_else(|| BrowserError::new("BROWSER_UNAVAILABLE", "preview registration failed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_accepts_http_and_blank_but_refuses_local_schemes() {
        assert_eq!(
            normalize_navigation_url("example.com").unwrap(),
            "https://example.com/"
        );
        assert_eq!(
            normalize_navigation_url("http://127.0.0.1:8080/a").unwrap(),
            "http://127.0.0.1:8080/a"
        );
        assert_eq!(
            normalize_navigation_url("about:blank").unwrap(),
            "about:blank"
        );
        for refused in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,x",
        ] {
            let error = normalize_navigation_url(refused).unwrap_err();
            assert_eq!(error.code, "INVALID_ARGUMENT");
        }
        assert!(normalize_navigation_url("   ").is_err());
    }

    #[test]
    fn a_hole_is_clamped_to_the_calling_view() {
        let view = Rect {
            x: 400.0,
            y: 40.0,
            width: 360.0,
            height: 600.0,
        };
        let hole = Rect {
            x: 0.0,
            y: 36.0,
            width: 360.0,
            height: 564.0,
        };
        assert_eq!(
            clamp_guest(view, hole),
            Some(Rect {
                x: 400.0,
                y: 76.0,
                width: 360.0,
                height: 564.0
            })
        );
        let overhang = Rect {
            x: -80.0,
            y: -10.0,
            width: 900.0,
            height: 20.0,
        };
        assert_eq!(clamp_guest(view, overhang).map(|rect| rect.x), Some(400.0));
        let below = Rect {
            x: 0.0,
            y: 700.0,
            width: 10.0,
            height: 10.0,
        };
        assert_eq!(clamp_guest(view, below), None);
    }

    #[test]
    fn cdp_is_deny_by_default_and_stale_uids_are_not_selectors() {
        assert!(cdp_allowed("Runtime.evaluate"));
        assert!(cdp_allowed("Accessibility.getFullAXTree"));
        assert!(cdp_allowed("Page.captureScreenshot"));
        for denied in [
            "Network.getAllCookies",
            "Network.setCookie",
            "Storage.getCookies",
            "Target.getTargets",
            "Fetch.enable",
            "Page.navigate",
        ] {
            assert!(!cdp_allowed(denied), "{denied} must stay denied");
        }
        assert!(safe_uid("e1"));
        assert!(safe_uid("e120"));
        assert!(!safe_uid("e"));
        assert!(!safe_uid("e1\"]"));
        assert!(!safe_uid("../e1"));
    }

    #[test]
    fn history_records_new_pages_and_drops_the_forward_branch() {
        let mut inner = Inner::default();
        BrowserHost::record_navigation(&mut inner, "https://a.test/");
        BrowserHost::record_navigation(&mut inner, "https://b.test/");
        assert_eq!(inner.cursor, 1);
        inner.cursor = 0;
        BrowserHost::record_navigation(&mut inner, "https://c.test/");
        assert_eq!(inner.history, vec!["https://a.test/", "https://c.test/"]);
        assert_eq!(inner.cursor, 1);
        BrowserHost::record_navigation(&mut inner, "https://c.test/");
        assert_eq!(inner.history.len(), 2);
    }

    #[tokio::test]
    async fn a_full_page_screenshot_is_refused_instead_of_cropped() {
        let host = BrowserHost::new();
        let error = host.screenshot(true).await.unwrap_err();
        assert_eq!(error.code, "UNSUPPORTED");
    }

    #[tokio::test]
    async fn a_viewport_screenshot_requires_the_guest() {
        let host = BrowserHost::new();
        let error = host.screenshot(false).await.unwrap_err();
        assert_eq!(error.code, "BROWSER_UNAVAILABLE");
    }
}
