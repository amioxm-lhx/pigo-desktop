//! Read-only workspace file panel, ported from Electron `fs-panel.ts` /
//! `fs-index.ts` / `git-diff.ts`. User-initiated browsing stays inside the
//! workspace root plus the scratch/attachments roots and honors the default
//! ignore subset.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use base64::Engine;
use serde_json::{json, Value};

use crate::envelope::{self, err};

const MAX_TEXT_BYTES: u64 = 512 * 1024;
const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;
const FS_INDEX_MAX_ENTRIES: usize = 8000;

const IGNORED_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    ".DS_Store",
    "target",
    "dist",
    "build",
    "out",
    ".venv",
    "venv",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
];

fn image_mime(ext: &str) -> Option<&'static str> {
    Some(match ext.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        _ => return None,
    })
}

pub fn is_attachment_blob_ref(path: &str) -> bool {
    let cleaned = path.trim().replace('\\', "/");
    cleaned.len() == "attachments/".len() + 64
        && cleaned.starts_with("attachments/")
        && cleaned["attachments/".len()..]
            .chars()
            .all(|c| c.is_ascii_hexdigit())
}

/// Resolve `rel` inside `root`, rejecting `..` escapes.
fn resolve_within_root(root: &Path, rel: &str) -> Option<PathBuf> {
    let clean_rel = rel.trim_start_matches(['/', '\\']);
    let target = root.join(clean_rel);
    let canonical_root = root.canonicalize().ok()?;
    let canonical = target.canonicalize().unwrap_or_else(|_| normalize(&target));
    if canonical == canonical_root || canonical.starts_with(&canonical_root) {
        Some(canonical)
    } else {
        None
    }
}

/// Best-effort normalization without requiring the path to exist.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Canonicalize when the path exists, else fall back to lexical normalization.
///
/// Everything that gets compared must go through this. On Windows
/// `canonicalize` returns a verbatim `\\?\D:\...` path while a plain join
/// yields `D:\...`, so mixing the two spaces makes a resolved candidate fail
/// `starts_with` against its own root — every click in the file tree was
/// answered with "path outside allowed roots", even though listing the same
/// directory worked (listing never compares against the roots).
fn canonical_or_normalized(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| normalize(path))
}

fn path_is_within(root: &Path, target: &Path) -> bool {
    target == root || target.starts_with(root)
}

/// Resolve a user-clicked path against the workspace plus extra containment
/// roots (mirrors `resolveOpenablePath`).
pub fn resolve_openable_path(
    raw: &str,
    workspace_root: Option<&Path>,
    extra_roots: &[PathBuf],
) -> Option<PathBuf> {
    let raw = raw.trim();
    if raw.is_empty() || raw.starts_with('~') {
        return None;
    }
    let allowed: Vec<PathBuf> = workspace_root
        .map(canonical_or_normalized)
        .into_iter()
        .chain(extra_roots.iter().map(|r| canonical_or_normalized(r)))
        .collect();
    if allowed.is_empty() {
        return None;
    }
    let candidate = if is_attachment_blob_ref(raw) {
        let attachment_root = extra_roots
            .iter()
            .find(|r| r.file_name().map(|n| n == "attachments").unwrap_or(false))?;
        resolve_within_root(
            attachment_root,
            &raw.replace('\\', "/")["attachments/".len()..],
        )?
    } else if Path::new(raw).is_absolute() {
        canonical_or_normalized(Path::new(raw))
    } else {
        let root = workspace_root?;
        resolve_within_root(&normalize(Path::new(root)), raw)?
    };
    allowed
        .iter()
        .any(|root| path_is_within(root, &candidate))
        .then_some(candidate)
}

pub fn list_dir(root: &Path, rel: &str) -> Result<Value, String> {
    let dir = if rel.trim().is_empty() {
        root.to_path_buf()
    } else {
        resolve_within_root(root, rel).ok_or("path outside workspace")?
    };
    let mut entries: Vec<Value> = Vec::new();
    let read_dir = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;
    for entry in read_dir.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if IGNORED_NAMES.contains(&name.as_str()) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let kind = if meta.is_dir() { "dir" } else { "file" };
        entries.push(json!({ "name": name, "kind": kind, "size": meta.len() }));
    }
    entries.sort_by(|a, b| {
        let ka = if a["kind"] == "dir" { 0 } else { 1 };
        let kb = if b["kind"] == "dir" { 0 } else { 1 };
        (ka, a["name"].as_str().unwrap_or("").to_lowercase())
            .cmp(&(kb, b["name"].as_str().unwrap_or("").to_lowercase()))
    });
    Ok(json!({ "entries": entries }))
}

pub fn read_openable(path: &str, workspace_root: Option<&Path>, extra_roots: &[PathBuf]) -> Value {
    let Some(target) = resolve_openable_path(path, workspace_root, extra_roots) else {
        return err("INVALID_ARGUMENT", "path outside allowed roots");
    };
    let Ok(meta) = std::fs::metadata(&target) else {
        return err("NOT_FOUND", format!("file not found: {path}"));
    };
    let size = meta.len();
    let ext = target
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if let Some(mime) = image_mime(&ext) {
        if size <= MAX_IMAGE_BYTES {
            match std::fs::read(&target) {
                Ok(bytes) => {
                    let data_url = format!(
                        "data:{mime};base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(bytes)
                    );
                    return envelope::ok(
                        json!({ "kind": "image", "dataUrl": data_url, "size": size }),
                    );
                }
                Err(e) => return err("INTERNAL", e.to_string()),
            }
        }
        return envelope::ok(json!({ "kind": "tooLarge", "size": size }));
    }
    if size > MAX_TEXT_BYTES {
        return envelope::ok(json!({ "kind": "tooLarge", "size": size }));
    }
    match std::fs::read(&target) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(content) => {
                envelope::ok(json!({ "kind": "text", "content": content, "size": size }))
            }
            Err(_) => envelope::ok(json!({ "kind": "binary", "size": size })),
        },
        Err(e) => err("INTERNAL", e.to_string()),
    }
}

pub fn read_image_data_url(
    raw: &str,
    workspace_root: Option<&Path>,
    extra_roots: &[PathBuf],
) -> Value {
    let Some(target) = resolve_openable_path(raw, workspace_root, extra_roots) else {
        return envelope::ok(json!({ "kind": "missing" }));
    };
    if !target.is_file() {
        return envelope::ok(json!({ "kind": "missing" }));
    }
    let ext = target
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let Some(mime) = image_mime(&ext) else {
        return envelope::ok(json!({ "kind": "notImage" }));
    };
    let Ok(meta) = std::fs::metadata(&target) else {
        return envelope::ok(json!({ "kind": "missing" }));
    };
    if meta.len() > MAX_IMAGE_BYTES {
        return envelope::ok(json!({ "kind": "tooLarge", "size": meta.len() }));
    }
    match std::fs::read(&target) {
        Ok(bytes) => {
            let data_url = format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            );
            envelope::ok(json!({ "kind": "image", "dataUrl": data_url, "size": meta.len() }))
        }
        Err(_) => envelope::ok(json!({ "kind": "missing" })),
    }
}

/// Recursive index with the ignore subset; capped like the Electron one.
pub fn workspace_file_index(root: &Path) -> Value {
    let mut entries: Vec<Value> = Vec::new();
    let mut truncated = false;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read_dir) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read_dir.flatten() {
            if entries.len() >= FS_INDEX_MAX_ENTRIES {
                truncated = true;
                break;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if IGNORED_NAMES.contains(&name.as_str()) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let is_dir = meta.is_dir();
            let rel = entry
                .path()
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| name.clone());
            entries.push(json!({ "path": rel, "kind": if is_dir { "dir" } else { "file" } }));
            if is_dir {
                stack.push(entry.path());
            }
        }
        if truncated {
            break;
        }
    }
    json!({ "entries": entries, "truncated": truncated })
}

/// `git status --porcelain` + `git diff --numstat` → the workpanel shape.
pub fn workspace_diff(cwd: &Path) -> Value {
    let run = |args: &[&str]| -> Option<String> {
        let mut cmd = Command::new("git");
        cmd.current_dir(cwd).args(args);
        // Suppress the console flash a GUI app gets for console children.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let Ok(out) = cmd.output() else { return None };
        if !out.status.success() {
            return None;
        }
        String::from_utf8(out.stdout).ok()
    };
    let Some(_) = run(&["rev-parse", "--is-inside-work-tree"]) else {
        return json!({ "repo": false, "clean": true, "files": [] });
    };
    let Some(status) = run(&["status", "--porcelain"]) else {
        return json!({ "repo": true, "clean": true, "files": [] });
    };
    let mut files: Vec<Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in status.lines() {
        if line.len() < 4 {
            continue;
        }
        let (code, path) = line.split_at(2);
        let path = path.trim().to_string();
        if path.is_empty() || !seen.insert(path.clone()) {
            continue;
        }
        let status = match code.trim() {
            "??" | "A" => "added",
            "D" => "deleted",
            "R" => "renamed",
            _ => "modified",
        };
        files.push(json!({
            "path": path,
            "status": status,
            "additions": 0,
            "deletions": 0,
            "hunks": [],
        }));
    }
    if let Some(numstat) = run(&["diff", "--numstat"]) {
        for line in numstat.lines() {
            let mut parts = line.split('\t');
            let (Some(add), Some(del)) = (parts.next(), parts.next()) else {
                continue;
            };
            let Some(path) = parts.next() else { continue };
            if let Some(file) = files
                .iter_mut()
                .find(|f| f["path"] == path.replace('\\', "/") || f["path"] == path)
            {
                file["additions"] = json!(add.parse::<u64>().unwrap_or(0));
                file["deletions"] = json!(del.parse::<u64>().unwrap_or(0));
            }
        }
    }
    json!({ "repo": true, "clean": files.is_empty(), "files": files })
}

/// Append text to a file, creating parents (used by instruction save).
pub fn write_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    file.write_all(content.as_bytes())
        .map_err(|e| e.to_string())
}

pub fn read_if_exists(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

// ── Composer attachment import (port of electron/main/composer-paste.ts) ──

const COMPOSER_MAX_FILES: usize = 20;
const COMPOSER_MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const COMPOSER_MAX_TOTAL_BYTES: u64 = 128 * 1024 * 1024;
const IMAGE_EXTENSIONS: &[&str] = &[
    ".avif", ".bmp", ".gif", ".heic", ".jpeg", ".jpg", ".png", ".tif", ".tiff", ".webp",
];

fn mime_for_extension(ext: &str) -> &'static str {
    match ext {
        ".gif" => "image/gif",
        ".jpg" => "image/jpeg",
        ".jpeg" => "image/jpeg",
        ".png" => "image/png",
        ".tif" => "image/tiff",
        ".tiff" => "image/tiff",
        ".webp" => "image/webp",
        ".avif" => "image/avif",
        ".bmp" => "image/bmp",
        ".heic" => "image/heic",
        ".csv" => "text/csv",
        ".html" => "text/html",
        ".txt" => "text/plain",
        ".json" => "application/json",
        ".pdf" => "application/pdf",
        ".zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

fn extension_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/gif" => ".gif",
        "image/jpeg" => ".jpg",
        "image/png" => ".png",
        "image/tiff" => ".tiff",
        "image/webp" => ".webp",
        "text/csv" => ".csv",
        "text/html" => ".html",
        "text/plain" => ".txt",
        "application/json" => ".json",
        "application/pdf" => ".pdf",
        "application/zip" => ".zip",
        _ => ".bin",
    }
}

/// Leaf-name sanitizer: unicode letters/digits plus `._-`, mirroring the
/// Electron `\p{L}\p{N}._-` collapse.
fn sanitize_leaf_name(raw: &str) -> String {
    let collapsed: String = raw
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or("")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = collapsed.trim_matches('.').trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        trimmed.to_string()
    }
}

pub fn valid_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Copy picked files into the session scratch paste root with unique
/// `pasted-<uuid>-` prefixes; returns the ComposerPastedFile rows.
pub fn import_composer_files(
    data_dir: &Path,
    session_id: &str,
    paths: &[String],
) -> Result<Value, String> {
    if !valid_session_id(session_id) {
        return Err("invalid session id".into());
    }
    if paths.is_empty() {
        return Ok(json!([]));
    }
    if paths.len() > COMPOSER_MAX_FILES {
        return Err(format!(
            "too many imported files (maximum {COMPOSER_MAX_FILES})"
        ));
    }
    let mut total: u64 = 0;
    let mut prepared: Vec<(PathBuf, String, String, u64)> = Vec::new();
    for (index, raw) in paths.iter().enumerate() {
        let requested = raw.trim();
        if requested.is_empty() {
            return Err("import file path is invalid".into());
        }
        let source = PathBuf::from(requested);
        if !source.is_absolute() {
            return Err("import file path must be absolute".into());
        }
        let Ok(source) = source.canonicalize() else {
            return Err("import file was not found".into());
        };
        let Ok(meta) = std::fs::metadata(&source) else {
            return Err("import file could not be read".into());
        };
        if !meta.is_file() {
            return Err("selected path is not a file".into());
        }
        let size = meta.len();
        if size > COMPOSER_MAX_FILE_BYTES {
            return Err(format!(
                "imported file is too large (maximum {COMPOSER_MAX_FILE_BYTES} bytes)"
            ));
        }
        total += size;
        if total > COMPOSER_MAX_TOTAL_BYTES {
            return Err(format!(
                "imported files are too large (maximum {COMPOSER_MAX_TOTAL_BYTES} bytes)"
            ));
        }
        let ext = source
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
            .unwrap_or_default();
        let mime = mime_for_extension(&ext).to_string();
        let leaf = sanitize_leaf_name(
            &source
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        );
        let candidate = if leaf.is_empty() {
            format!("pasted-file-{}", index + 1)
        } else {
            leaf
        };
        let name = if candidate.contains('.') {
            candidate
        } else {
            format!("{candidate}{}", extension_for_mime(&mime))
        };
        prepared.push((source, name, mime, size));
    }
    let root = data_dir.join("scratch").join(session_id).join("pasted");
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    for (source, name, mime, size) in prepared {
        let path = root.join(format!("pasted-{}-{}", uuid::Uuid::new_v4(), name));
        std::fs::copy(&source, &path).map_err(|e| e.to_string())?;
        let kind = if mime.starts_with("image/")
            || IMAGE_EXTENSIONS.contains(
                &path
                    .extension()
                    .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
                    .unwrap_or_default()
                    .as_str(),
            ) {
            "image"
        } else {
            "file"
        };
        files.push(json!({
            "path": path.to_string_lossy(),
            "name": name,
            "kind": kind,
            "mimeType": mime,
            "size": size,
        }));
    }
    Ok(json!(files))
}

/// Materialize renderer clipboard bytes (base64 over the Tauri JSON bridge)
/// into the session scratch paste root — port of `saveComposerPasteFiles`.
/// The renderer's ArrayBuffer arrives as `{"__bytes_b64": "…"}` (the shim
/// converts before invoke).
pub fn save_composer_paste_files(
    data_dir: &Path,
    session_id: &str,
    files: &[Value],
) -> Result<Value, String> {
    use base64::Engine as _;
    if !valid_session_id(session_id) {
        return Err("invalid session id".into());
    }
    if files.is_empty() {
        return Ok(json!([]));
    }
    if files.len() > COMPOSER_MAX_FILES {
        return Err(format!(
            "too many pasted files (maximum {COMPOSER_MAX_FILES})"
        ));
    }
    let mut total: usize = 0;
    let mut prepared: Vec<(Vec<u8>, String, String)> = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let Some(b64) = file.pointer("/data/__bytes_b64").and_then(|v| v.as_str()) else {
            return Err("clipboard file data is invalid".into());
        };
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|_| "clipboard file data is invalid".to_string())?;
        if bytes.len() as u64 > COMPOSER_MAX_FILE_BYTES {
            return Err(format!(
                "pasted file is too large (maximum {COMPOSER_MAX_FILE_BYTES} bytes)"
            ));
        }
        total += bytes.len();
        if total as u64 > COMPOSER_MAX_TOTAL_BYTES {
            return Err(format!(
                "pasted files are too large (maximum {COMPOSER_MAX_TOTAL_BYTES} bytes)"
            ));
        }
        let mime = str_or(file, "mimeType", "application/octet-stream")
            .trim()
            .to_lowercase();
        let raw_name = str_or(file, "name", "");
        let leaf = sanitize_leaf_name(raw_name);
        let candidate = if leaf.is_empty() {
            format!("pasted-file-{}", index + 1)
        } else {
            leaf
        };
        let name = if candidate.contains('.') {
            candidate
        } else {
            format!("{candidate}{}", extension_for_mime(&mime))
        };
        prepared.push((bytes, name, mime));
    }
    let root = data_dir.join("scratch").join(session_id).join("pasted");
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (bytes, name, mime) in prepared {
        let path = root.join(format!("pasted-{}-{}", uuid::Uuid::new_v4(), name));
        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        let kind = if mime.starts_with("image/")
            || IMAGE_EXTENSIONS.contains(
                &Path::new(&name)
                    .extension()
                    .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
                    .unwrap_or_default()
                    .as_str(),
            ) {
            "image"
        } else {
            "file"
        };
        out.push(json!({
            "path": path.to_string_lossy(),
            "name": name,
            "kind": kind,
            "mimeType": mime,
            "size": bytes.len(),
        }));
    }
    Ok(json!(out))
}

fn str_or<'a>(value: &'a Value, key: &str, default: &'a str) -> &'a str {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Scratch workspace outside the repo so canonicalization has something to
    /// resolve (and so the Windows verbatim-prefix path shape is exercised).
    fn workspace(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("pigo-fsops-{}-{name}", std::process::id()));
        fs::remove_dir_all(&root).ok();
        fs::create_dir_all(root.join("src")).expect("create workspace");
        fs::write(root.join("src").join("main.go"), "package main\n").expect("write file");
        root
    }

    /// Regression for the Windows-only bug: a relative path inside the
    /// workspace resolved to a `\\?\D:/...` verbatim path and then failed
    /// `starts_with` against the plainly-normalized root, so every file click
    /// answered "path outside allowed roots".
    #[test]
    fn relative_file_inside_root_resolves() {
        let root = workspace("rel");
        let resolved = resolve_openable_path("src/main.go", Some(&root), &[]);
        assert!(
            resolved.is_some(),
            "relative path inside the workspace must resolve (root {:?})",
            root
        );
        let target = resolved.unwrap();
        assert!(target.ends_with("main.go"), "resolved to {target:?}");
        let reply = read_openable("src/main.go", Some(&root), &[]);
        assert_eq!(
            reply.get("ok").and_then(Value::as_bool),
            Some(true),
            "read_openable rejected a file inside the workspace: {reply}"
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn nested_relative_path_resolves() {
        let root = workspace("nested");
        assert!(resolve_openable_path("src", Some(&root), &[]).is_some());
        assert!(resolve_openable_path("./src/main.go", Some(&root), &[]).is_some());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn absolute_path_inside_root_resolves() {
        let root = workspace("abs");
        let file = root.join("src").join("main.go");
        assert!(resolve_openable_path(&file.to_string_lossy(), Some(&root), &[]).is_some());
        assert!(resolve_openable_path(&root.to_string_lossy(), Some(&root), &[]).is_some());
        fs::remove_dir_all(&root).ok();
    }

    /// Containment must survive the normalization change: escapes stay closed.
    #[test]
    fn traversal_and_outside_paths_stay_rejected() {
        let root = workspace("escape");
        assert!(resolve_openable_path("../outside.txt", Some(&root), &[]).is_none());
        assert!(resolve_openable_path("src/../../outside.txt", Some(&root), &[]).is_none());
        let outside = std::env::temp_dir().join("definitely-outside-pigo-fsops.txt");
        assert!(resolve_openable_path(&outside.to_string_lossy(), Some(&root), &[]).is_none());
        assert!(resolve_openable_path("~/.ssh/id_rsa", Some(&root), &[]).is_none());
        fs::remove_dir_all(&root).ok();
    }

    /// Extra roots (scratch, attachments) must compare in the same space too.
    #[test]
    fn extra_roots_are_honored() {
        let root = workspace("main");
        let scratch = workspace("scratch");
        let file = scratch.join("src").join("main.go");
        let extra = vec![scratch.clone()];
        assert!(resolve_openable_path(&file.to_string_lossy(), Some(&root), &extra).is_some());
        fs::remove_dir_all(&root).ok();
        fs::remove_dir_all(&scratch).ok();
    }
}
