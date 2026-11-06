//! Embedded BrowserPreview service.
//!
//! The work-panel browser tab is an iframe owned by the renderer. This module
//! serves the previewed workspace files over a loopback-only HTTP endpoint (a
//! custom URI scheme cannot be framed reliably on every platform), keeps the
//! request inside the originating session's workspace, and injects a tiny
//! live-reload poller into HTML responses. Every request re-reads the file, so
//! the iframe always shows the current bytes; the poller reloads it when the
//! page or a sibling asset changes.
//!
//! There is no JavaScript runtime or template engine here: a BrowserPreview
//! page is served as authored, plus one fixed script tag.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

const MAX_REQUEST_BYTES: usize = 16 * 1024;
const MAX_PREVIEW_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOKENS: usize = 128;

#[derive(Clone)]
struct PreviewTarget {
    root: PathBuf,
    file: PathBuf,
}

pub struct PreviewServer {
    port: u16,
    tokens: Arc<Mutex<HashMap<String, PreviewTarget>>>,
}

impl PreviewServer {
    /// Bind an ephemeral loopback port and start serving. The caller stores the
    /// server for the app lifetime; a bind failure is non-fatal (BrowserPreview
    /// falls back to the system browser).
    pub fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let tokens: Arc<Mutex<HashMap<String, PreviewTarget>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let thread_tokens = tokens.clone();
        std::thread::Builder::new()
            .name("browser-preview".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let _ = handle_connection(stream, &thread_tokens);
                }
            })?;
        Ok(Self { port, tokens })
    }

    /// Register one canonical workspace file and return its iframe URL.
    pub fn register(&self, root: &Path, file: &Path) -> Option<String> {
        let root = fs::canonicalize(root).ok()?;
        let file = fs::canonicalize(file).ok()?;
        if !file.starts_with(&root) || !file.is_file() {
            return None;
        }
        let relative = file
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
            tokens.insert(token.clone(), PreviewTarget { root, file });
        }
        Some(format!(
            "http://127.0.0.1:{}/preview/{}/{}",
            self.port,
            token,
            encode_path(&relative)
        ))
    }
}

fn handle_connection(mut stream: TcpStream, tokens: &Mutex<HashMap<String, PreviewTarget>>) {
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
    let Some(rest) = decoded.strip_prefix("/preview/") else {
        respond_text(&mut stream, "404 Not Found", b"Not Found");
        return;
    };
    let Some((token, relative)) = rest.split_once('/') else {
        respond_text(&mut stream, "404 Not Found", b"Not Found");
        return;
    };
    let preview = {
        let tokens = tokens.lock().unwrap();
        tokens.get(token).cloned()
    };
    let Some(preview) = preview else {
        respond_text(&mut stream, "404 Not Found", b"Not Found");
        return;
    };

    if relative == "__version" {
        let version = preview_version(&preview);
        let body = format!("{{\"version\":\"{version}\"}}");
        respond(
            &mut stream,
            "200 OK",
            "application/json; charset=utf-8",
            body.as_bytes(),
            head_only,
        );
        return;
    }

    let Some(relative) = normalize_relative(relative) else {
        respond_text(&mut stream, "403 Forbidden", b"Forbidden");
        return;
    };
    let Some(path) = resolve_inside(&preview.root, &relative) else {
        respond_text(&mut stream, "404 Not Found", b"Not Found");
        return;
    };
    let Ok(metadata) = fs::metadata(&path) else {
        respond_text(&mut stream, "404 Not Found", b"Not Found");
        return;
    };
    if !metadata.is_file() || metadata.len() > MAX_PREVIEW_BYTES {
        respond_text(&mut stream, "404 Not Found", b"Not Found");
        return;
    }
    let Ok(bytes) = fs::read(&path) else {
        respond_text(&mut stream, "404 Not Found", b"Not Found");
        return;
    };
    let content_type = content_type(&path);
    if content_type.starts_with("text/html") {
        let body = inject_live_reload(&String::from_utf8_lossy(&bytes), token);
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

/// Normalize a URL path after the token: reject `..`, drop empty and `.`
/// segments. This runs before any filesystem access.
pub(crate) fn normalize_relative(relative: &str) -> Option<String> {
    let mut segments: Vec<&str> = Vec::new();
    for segment in relative.split('/') {
        match segment {
            "" | "." => continue,
            ".." => return None,
            _ => segments.push(segment),
        }
    }
    if segments.is_empty() {
        return None;
    }
    Some(segments.join("/"))
}

/// Resolve a normalized relative path inside the canonical root. Symlinks are
/// canonicalized so a link cannot escape the workspace.
pub(crate) fn resolve_inside(root: &Path, relative: &str) -> Option<PathBuf> {
    let candidate = root.join(relative);
    let canonical = fs::canonicalize(candidate).ok()?;
    canonical.starts_with(root).then_some(canonical)
}

fn preview_version(target: &PreviewTarget) -> String {
    let mut newest = file_stamp(&target.file);
    let mut count = 1u64;
    if let Some(directory) = target.file.parent() {
        if let Ok(entries) = fs::read_dir(directory) {
            for entry in entries.flatten() {
                if entry
                    .file_type()
                    .map(|kind| kind.is_file())
                    .unwrap_or(false)
                {
                    newest = newest.max(file_stamp(&entry.path()));
                    count += 1;
                }
            }
        }
    }
    format!("{newest:x}-{count:x}")
}

fn file_stamp(path: &Path) -> u64 {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0)
}

fn inject_live_reload(html: &str, token: &str) -> String {
    let mut script = String::from("<script>(function(){var v=null,u=\"/preview/");
    script.push_str(token);
    script.push_str(
        "/__version\";setInterval(function(){fetch(u,{cache:\"no-store\"}).then(function(r){return r.json()}).then(function(d){if(v!==null&&d.version!==v){location.reload()}v=d.version}).catch(function(){})},700)})();</script>",
    );
    let lower = html.to_ascii_lowercase();
    if let Some(index) = lower.rfind("</body>") {
        let mut out = String::with_capacity(html.len() + script.len());
        out.push_str(&html[..index]);
        out.push_str(&script);
        out.push_str(&html[index..]);
        out
    } else {
        html.to_string() + &script
    }
}

pub(crate) fn content_type(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_lowercase();
    match extension.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "txt" | "md" => "text/plain; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "wasm" => "application/wasm",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

pub(crate) fn respond(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
    head_only: bool,
) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    if !head_only {
        let _ = stream.write_all(body);
    }
    let _ = stream.flush();
}

pub(crate) fn respond_text(stream: &mut TcpStream, status: &str, body: &[u8]) {
    respond(stream, status, "text/plain; charset=utf-8", body, false);
}

pub(crate) fn encode_path(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            let mut out = String::new();
            for byte in segment.as_bytes() {
                if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b':')
                {
                    out.push(*byte as char);
                } else {
                    out.push_str(&format!("%{byte:02X}"));
                }
            }
            out
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = (bytes[index + 1] as char).to_digit(16);
            let low = (bytes[index + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                out.push(((high << 4) | low) as u8);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    fn temp_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "pi-preview-server-{label}-{}-{}",
            std::process::id(),
            UNIX_EPOCH.elapsed().map(|d| d.as_nanos()).unwrap_or(0)
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
    fn serves_html_and_its_version() {
        let root = temp_root("serve");
        let file = root.join("index.html");
        fs::write(&file, "<html><body>preview</body></html>").unwrap();
        let server = PreviewServer::start().unwrap();
        let url = server.register(&root, &file).unwrap();
        let path = &url[url.find("/preview/").unwrap()..];
        let response = fetch(server.port, path);
        assert!(response.contains("200 OK"), "{response}");
        assert!(response.contains("preview"), "{response}");
        assert!(
            response.contains("__version"),
            "live reload script is missing"
        );

        let token_end = path[9..].find('/').unwrap() + 9;
        let version_path = format!("{}/__version", &path[..token_end]);
        let version = fetch(server.port, &version_path);
        assert!(version.contains("\"version\""), "{version}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn traversal_and_unknown_tokens_are_refused() {
        let root = temp_root("deny");
        let file = root.join("index.html");
        fs::write(&file, "<html></html>").unwrap();
        fs::write(root.join("secret.txt"), "secret").unwrap();
        let server = PreviewServer::start().unwrap();
        let url = server.register(&root, &file).unwrap();
        let path = &url[url.find("/preview/").unwrap()..];
        let token_end = path[9..].find('/').unwrap() + 9;
        let traversal = format!("{}/../secret.txt", &path[..token_end]);
        assert!(
            fetch(server.port, &traversal).contains("403")
                || fetch(server.port, &traversal).contains("404")
        );
        assert!(fetch(server.port, "/preview/not-a-token/index.html").contains("404"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn percent_decoding_and_relative_normalization() {
        assert_eq!(percent_decode("/a%20b/c%23d.html"), "/a b/c#d.html");
        assert_eq!(
            normalize_relative("./a//b.html"),
            Some("a/b.html".to_string())
        );
        assert_eq!(normalize_relative("a/../b.html"), None);
        assert_eq!(encode_path("notes: draft.html"), "notes:%20draft.html");
    }
}
