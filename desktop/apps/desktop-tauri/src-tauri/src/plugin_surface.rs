//! Plugin surface server: docked views and detached panels.
//!
//! Each surface is one HTML entry from a plugin package served over a
//! loopback-only HTTP endpoint. Tauri has no WebContentsView overlay, so both
//! placements are renderer iframes fed by this service — the same shape the
//! embedded BrowserPreview already uses. The token is the capability: it is
//! unguessable, scoped to one registered entry, and revoked when the plugin
//! unloads or crashes.
//!
//! HTML responses get one injected script that defines `window.pluginBridge`,
//! the preload surface plugin pages expect. The bridge talks to the renderer
//! that owns the iframe with `postMessage`, which forwards each call to the
//! Rust panel bridge (`crate::plugin_bridge`). Permissions, fs scopes and the
//! network allowlist are enforced there, so plugin markup never reaches a
//! Tauri channel directly.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::preview::{
    content_type, encode_path, normalize_relative, percent_decode, resolve_inside, respond,
    respond_text,
};

const MAX_REQUEST_BYTES: usize = 16 * 1024;
const MAX_SURFACE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOKENS: usize = 256;

/// Renderer event carrying `{action: "open"|"close", pluginId, ...}`. The
/// shell pushes it when plugin code calls `pi.ui.openPanel`/`closePanel`;
/// renderer-initiated opens get the payload from the invoke reply instead.
pub(crate) const PANEL_EVENT: &str = "pi-desktop/plugin/event/panel";

#[derive(Clone)]
struct SurfaceTarget {
    plugin_id: String,
    root: PathBuf,
}

pub struct PluginSurfaceServer {
    port: u16,
    tokens: Arc<Mutex<HashMap<String, SurfaceTarget>>>,
}

impl PluginSurfaceServer {
    pub fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let tokens: Arc<Mutex<HashMap<String, SurfaceTarget>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let thread_tokens = tokens.clone();
        std::thread::Builder::new()
            .name("plugin-surface".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let _ = handle_connection(stream, &thread_tokens);
                }
            })?;
        Ok(Self { port, tokens })
    }

    /// Register one plugin HTML entry and return its iframe URL. `entry` must
    /// canonicalize inside `root` and be a file.
    pub fn register(&self, plugin_id: &str, root: &Path, entry: &Path) -> Option<String> {
        let root = fs::canonicalize(root).ok()?;
        let entry = fs::canonicalize(entry).ok()?;
        if !entry.starts_with(&root) || !entry.is_file() {
            return None;
        }
        let relative = entry
            .strip_prefix(&root)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/");
        let token = uuid::Uuid::new_v4().simple().to_string();
        {
            let mut tokens = self.tokens.lock().unwrap();
            if tokens.len() >= MAX_TOKENS {
                tokens.clear();
            }
            tokens.insert(
                token.clone(),
                SurfaceTarget {
                    plugin_id: plugin_id.to_string(),
                    root,
                },
            );
        }
        Some(format!(
            "http://127.0.0.1:{}/plugin/{}/{}",
            self.port,
            token,
            encode_path(&relative)
        ))
    }

    /// Drop every surface belonging to a plugin. Open iframes then 404 until
    /// the renderer reopens them, which it does on the next `pluginChanged`.
    pub fn revoke_plugin(&self, plugin_id: &str) {
        let mut tokens = self.tokens.lock().unwrap();
        tokens.retain(|_, target| target.plugin_id != plugin_id);
    }
}

fn handle_connection(mut stream: TcpStream, tokens: &Mutex<HashMap<String, SurfaceTarget>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    let mut raw = Vec::with_capacity(1024);
    let mut chunk = [0u8; 2048];
    while raw.len() < MAX_REQUEST_BYTES {
        let Ok(count) = stream.read(&mut chunk) else {
            return;
        };
        if count == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..count]);
        if raw.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let request = String::from_utf8_lossy(&raw);
    let Some(line) = request.lines().next() else {
        return;
    };
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");
    let head_only = method == "HEAD";
    if method != "GET" && !head_only {
        respond_text(&mut stream, "405 Method Not Allowed", b"Method Not Allowed");
        return;
    }

    let path = target.split('?').next().unwrap_or("");
    let decoded = percent_decode(path);
    let Some(rest) = decoded.strip_prefix("/plugin/") else {
        respond_text(&mut stream, "404 Not Found", b"Not Found");
        return;
    };
    let Some((token, relative)) = rest.split_once('/') else {
        respond_text(&mut stream, "404 Not Found", b"Not Found");
        return;
    };
    let surface = {
        let tokens = tokens.lock().unwrap();
        tokens.get(token).cloned()
    };
    let Some(surface) = surface else {
        respond_text(&mut stream, "404 Not Found", b"Not Found");
        return;
    };
    let Some(relative) = normalize_relative(relative) else {
        respond_text(&mut stream, "403 Forbidden", b"Forbidden");
        return;
    };
    // Dotfiles stay private even inside a token-scoped package.
    if relative.split('/').any(|segment| segment.starts_with('.')) {
        respond_text(&mut stream, "404 Not Found", b"Not Found");
        return;
    }
    let Some(path) = resolve_inside(&surface.root, &relative) else {
        respond_text(&mut stream, "404 Not Found", b"Not Found");
        return;
    };
    let Ok(metadata) = fs::metadata(&path) else {
        respond_text(&mut stream, "404 Not Found", b"Not Found");
        return;
    };
    if !metadata.is_file() || metadata.len() > MAX_SURFACE_BYTES {
        respond_text(&mut stream, "404 Not Found", b"Not Found");
        return;
    }
    let Ok(bytes) = fs::read(&path) else {
        respond_text(&mut stream, "404 Not Found", b"Not Found");
        return;
    };
    let content_type = content_type(&path);
    if content_type.starts_with("text/html") {
        let body = inject_bridge(&String::from_utf8_lossy(&bytes), &surface.plugin_id);
        respond(
            &mut stream,
            "200 OK",
            content_type,
            body.as_bytes(),
            head_only,
        );
    } else {
        respond(&mut stream, "200 OK", content_type, &bytes, head_only);
    }
}

/// Insert the bridge (and the host-chrome variable) at the top of `<head>` so
/// it exists before any page script runs.
fn inject_bridge(html: &str, plugin_id: &str) -> String {
    let injection = format!(
        "<style>:root{{--pi-plugin-titlebar-height:0px}}</style>{}",
        bridge_script(plugin_id)
    );
    let lower = html.to_ascii_lowercase();
    if let Some(head) = lower.find("<head") {
        if let Some(close) = lower[head..].find('>') {
            let index = head + close + 1;
            let mut out = String::with_capacity(html.len() + injection.len());
            out.push_str(&html[..index]);
            out.push_str(&injection);
            out.push_str(&html[index..]);
            return out;
        }
    }
    format!("{injection}{html}")
}

fn bridge_script(plugin_id: &str) -> String {
    let id_json = serde_json::to_string(plugin_id).unwrap_or_else(|_| "\"\"".into());
    BRIDGE_TEMPLATE.replace("__PLUGIN_ID__", &id_json)
}

const BRIDGE_TEMPLATE: &str = r#"<script>(function(){
"use strict";
var pluginId = __PLUGIN_ID__;
var seq = 0;
var pending = new Map();
var listeners = new Map();
function send(message) {
  if (window.parent && window.parent !== window) {
    window.parent.postMessage(message, "*");
  }
}
function invoke(channel, payload) {
  return new Promise(function(resolve, reject) {
    var id = ++seq;
    var timer = setTimeout(function() {
      if (pending.has(id)) {
        pending.delete(id);
        reject(new Error("plugin bridge timed out"));
      }
    }, 15000);
    pending.set(id, {
      resolve: function(value) { clearTimeout(timer); resolve(value); },
      reject: function(error) { clearTimeout(timer); reject(error); }
    });
    send({ __piPluginBridge: true, kind: "request", pluginId: pluginId, id: id, channel: channel, payload: payload });
  });
}
function on(event, handler) {
  if (typeof handler !== "function") return;
  var list = listeners.get(event);
  if (!list) { list = []; listeners.set(event, list); }
  list.push(handler);
}
function off(event, handler) {
  var list = listeners.get(event);
  if (!list) return;
  if (!handler) { listeners.delete(event); return; }
  var index = list.indexOf(handler);
  if (index >= 0) list.splice(index, 1);
}
window.pluginBridge = { invoke: invoke, on: on, off: off, getDroppedFilePath: function() { return null; } };
window.addEventListener("message", function(event) {
  var data = event.data;
  if (!data || data.__piPluginHost !== true) return;
  if (data.kind === "reply") {
    var entry = pending.get(data.id);
    if (!entry) return;
    pending.delete(data.id);
    if (data.ok) entry.resolve(data.result);
    else entry.reject(new Error((data.error && data.error.message) || "plugin bridge error"));
  } else if (data.kind === "event") {
    var list = listeners.get(data.event) || [];
    for (var i = 0; i < list.length; i++) {
      try { list[i](data.payload); } catch (error) { console.error(error); }
    }
  }
});
})();</script>"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn temp_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "pi-plugin-surface-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn fetch(port: u16, path: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes())
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn serves_html_with_the_bridge_and_sibling_assets() {
        let root = temp_root("serve");
        fs::create_dir_all(root.join("views")).unwrap();
        fs::write(
            root.join("views/index.html"),
            "<html><head><script>window.__before = 1;</script></head><body>view</body></html>",
        )
        .unwrap();
        fs::write(root.join("views/app.css"), "body{color:red}").unwrap();
        let server = PluginSurfaceServer::start().unwrap();
        let url = server
            .register("demo.hello", &root, &root.join("views/index.html"))
            .unwrap();
        let path = &url[url.find("/plugin/").unwrap()..];
        let response = fetch(server.port, path);
        assert!(response.contains("200 OK"), "{response}");
        assert!(response.contains("window.pluginBridge"), "{response}");
        assert!(response.contains("\"demo.hello\""), "{response}");
        assert!(
            response.contains("--pi-plugin-titlebar-height"),
            "{response}"
        );
        // The bridge is injected before the page's own head script.
        assert!(
            response.find("window.pluginBridge").unwrap()
                < response.find("window.__before").unwrap()
        );

        let asset_path = path.rsplit_once('/').unwrap().0.to_string() + "/app.css";
        let asset = fetch(server.port, &asset_path);
        assert!(asset.contains("text/css"), "{asset}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn traversal_dotfiles_and_unknown_tokens_are_refused() {
        let root = temp_root("deny");
        fs::write(root.join("index.html"), "<html></html>").unwrap();
        fs::write(root.join(".secret"), "secret").unwrap();
        let server = PluginSurfaceServer::start().unwrap();
        let url = server
            .register("demo.deny", &root, &root.join("index.html"))
            .unwrap();
        let path = &url[url.find("/plugin/").unwrap()..];
        let token_end = path[8..].find('/').unwrap() + 8;
        let base = &path[..token_end];
        assert!(
            fetch(server.port, &format!("{base}/../.secret")).contains("403")
                || fetch(server.port, &format!("{base}/../.secret")).contains("404")
        );
        assert!(fetch(server.port, &format!("{base}/.secret")).contains("404"));
        assert!(fetch(server.port, "/plugin/not-a-token/index.html").contains("404"));

        server.revoke_plugin("demo.deny");
        assert!(fetch(server.port, path).contains("404"));
        let _ = fs::remove_dir_all(root);
    }
}
