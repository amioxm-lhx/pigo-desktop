//! Session import from the local agent stores (Claude Code, Codex, OpenCode,
//! Pi). A scan is an explicit action and returns public candidates only; the
//! files stay in a main-process cache and a run converts them into host
//! sessions. Transcripts are normalized to user/assistant text rows — tool and
//! system lines have no durable counterpart without their call metadata.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};

use crate::envelope;
use crate::state::AppState;

const SAMPLE_THRESHOLD_BYTES: u64 = 8 * 1024 * 1024;
const SAMPLE_EDGE_BYTES: u64 = 64 * 1024;
const MAX_MESSAGES: usize = 10_000;
const MAX_TITLE_CHARS: usize = 80;

#[derive(Debug, Clone)]
struct ParsedSession {
    source: &'static str,
    external_id: String,
    title: String,
    project_path: Option<String>,
    model: Option<String>,
    created_at: String,
    updated_at: String,
    message_count: Option<usize>,
    messages: Vec<Value>,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn iso_from_ms(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn file_mtime(path: &Path) -> String {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .map(|time| {
            chrono::DateTime::<chrono::Utc>::from(time)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        })
        .unwrap_or_else(now_iso)
}

fn timestamp_value(value: Option<&Value>, fallback: &str) -> String {
    let Some(value) = value else {
        return fallback.to_string();
    };
    if let Some(text) = value.as_str() {
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(text) {
            return parsed
                .with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        }
        if let Ok(number) = text.parse::<i64>() {
            return iso_from_ms(if number > 1_000_000_000_000 {
                number
            } else {
                number * 1000
            });
        }
    }
    if let Some(number) = value.as_i64() {
        return iso_from_ms(if number > 1_000_000_000_000 {
            number
        } else {
            number * 1000
        });
    }
    fallback.to_string()
}

fn collapse_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_synthetic(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("# Context from my IDE setup:")
        || trimmed.starts_with("# Browser comments:")
        || trimmed.starts_with("<repo_setup")
        || trimmed.starts_with("<user_info>")
}

fn title_from(text: &str, fallback: &str) -> String {
    let collapsed = collapse_text(text);
    if collapsed.is_empty() {
        return fallback.to_string();
    }
    let clipped: String = collapsed.chars().take(MAX_TITLE_CHARS).collect();
    clipped
}

/// Text blocks from one message `content` value (string, block array, object).
fn content_text(content: Option<&Value>) -> String {
    let Some(content) = content else {
        return String::new();
    };
    if let Some(text) = content.as_str() {
        return text.trim().to_string();
    }
    if let Some(block) = content.as_object() {
        if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
            return text.trim().to_string();
        }
        if let Some(items) = block.get("content").and_then(|v| v.as_array()) {
            return content_text(Some(&Value::Array(items.clone())));
        }
    }
    let Some(blocks) = content.as_array() else {
        return String::new();
    };
    let mut parts: Vec<String> = Vec::new();
    for block in blocks {
        let kind = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if matches!(
            kind,
            "thinking" | "tool_use" | "tool_result" | "image" | "file"
        ) {
            continue;
        }
        let text = block
            .get("text")
            .or_else(|| block.get("content").filter(|v| v.is_string()))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !text.trim().is_empty() {
            parts.push(text.trim().to_string());
        }
    }
    parts.join("\n\n")
}

fn message_row(role: &str, text: &str, created_at: &str) -> Value {
    json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "role": role,
        "content": text,
        "createdAt": created_at,
    })
}

fn home_path(relative: &str) -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(relative))
}

fn collect_files(root: &Path, extension: &str, max_depth: usize, out: &mut Vec<PathBuf>) {
    if max_depth == 0 || !root.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, extension, max_depth - 1, out);
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case(extension))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
}

fn read_lines(path: &Path, sampled: bool) -> Result<(Vec<String>, bool), String> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .len();
    if sampled && size > SAMPLE_THRESHOLD_BYTES {
        let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut head = String::new();
        (&mut file)
            .take(SAMPLE_EDGE_BYTES)
            .read_to_string(&mut head)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let mut tail = String::new();
        let start = size.saturating_sub(SAMPLE_EDGE_BYTES);
        file.seek(SeekFrom::Start(start))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        file.read_to_string(&mut tail)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let mut lines: Vec<String> = Vec::new();
        lines.extend(head.lines().take(200).map(str::to_string));
        lines.extend(
            tail.lines()
                .skip(1)
                .collect::<Vec<_>>()
                .into_iter()
                .map(str::to_string),
        );
        return Ok((lines, true));
    }
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let reader = BufReader::new(file);
    let lines = reader
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.trim().is_empty())
        .collect();
    Ok((lines, false))
}

fn parse_json_lines(lines: &[String]) -> Vec<Value> {
    lines
        .iter()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .collect()
}

fn external_id_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("session")
        .to_string()
}

fn sanitized_session_id(source: &str, external_id: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("{source}:{external_id}").as_bytes());
    let mut hash = String::with_capacity(16);
    for byte in &digest[..8] {
        hash.push_str(&format!("{byte:02x}"));
    }
    format!("import-{source}-{hash}")
}

fn parse_claude(path: &Path, sampled: bool) -> Result<ParsedSession, String> {
    let (lines, incomplete) = read_lines(path, sampled)?;
    let documents = parse_json_lines(&lines);
    let fallback_ts = file_mtime(path);
    let mut messages: Vec<Value> = Vec::new();
    let mut project_path: Option<String> = None;
    let mut model: Option<String> = None;
    let mut first_ts: Option<String> = None;
    let mut last_ts: Option<String> = None;
    let mut first_user: Option<String> = None;
    for document in &documents {
        let kind = document.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(cwd) = document.get("cwd").and_then(|v| v.as_str()) {
            if project_path.is_none() && !cwd.trim().is_empty() {
                project_path = Some(cwd.to_string());
            }
        }
        if !matches!(kind, "user" | "assistant") {
            continue;
        }
        let message = document.get("message").cloned().unwrap_or(Value::Null);
        let role = message.get("role").and_then(|v| v.as_str()).unwrap_or(kind);
        if role != "user" && role != "assistant" {
            continue;
        }
        let text = content_text(message.get("content"));
        if text.is_empty() {
            continue;
        }
        if role == "assistant" {
            if let Some(value) = message.get("model").and_then(|v| v.as_str()) {
                if !value.trim().is_empty() {
                    model = Some(value.to_string());
                }
            }
        }
        let ts = timestamp_value(document.get("timestamp"), &fallback_ts);
        if first_ts.is_none() {
            first_ts = Some(ts.clone());
        }
        last_ts = Some(ts.clone());
        if role == "user" && first_user.is_none() && !is_synthetic(&text) {
            first_user = Some(text.clone());
        }
        messages.push(message_row(role, &text, &ts));
        if messages.len() > MAX_MESSAGES {
            break;
        }
    }
    if messages.is_empty() {
        return Err(format!("{}: no importable messages", path.display()));
    }
    let external_id = external_id_from_path(path);
    let title = title_from(
        first_user.as_deref().unwrap_or(""),
        &format!("Claude Code {external_id}"),
    );
    Ok(ParsedSession {
        source: "claude-code",
        external_id,
        title,
        project_path,
        model,
        created_at: first_ts.unwrap_or_else(|| fallback_ts.clone()),
        updated_at: last_ts.unwrap_or(fallback_ts),
        message_count: (!incomplete).then_some(messages.len()),
        messages,
    })
}

fn parse_codex(path: &Path, sampled: bool) -> Result<ParsedSession, String> {
    let (lines, incomplete) = read_lines(path, sampled)?;
    let documents = parse_json_lines(&lines);
    let fallback_ts = file_mtime(path);
    let mut external_id = external_id_from_path(path);
    let mut project_path: Option<String> = None;
    let mut model: Option<String> = None;
    let mut first_ts: Option<String> = None;
    let mut last_ts: Option<String> = None;
    let mut first_user: Option<String> = None;
    let mut response_messages: Vec<Value> = Vec::new();
    let mut event_messages: Vec<Value> = Vec::new();
    for document in &documents {
        let kind = document.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let payload = document.get("payload").cloned().unwrap_or(Value::Null);
        let ts = timestamp_value(document.get("timestamp"), &fallback_ts);
        if first_ts.is_none() {
            first_ts = Some(ts.clone());
        }
        last_ts = Some(ts.clone());
        match kind {
            "session_meta" => {
                if let Some(id) = payload.get("id").and_then(|v| v.as_str()) {
                    if !id.trim().is_empty() {
                        external_id = id.to_string();
                    }
                }
                if let Some(cwd) = payload.get("cwd").and_then(|v| v.as_str()) {
                    if !cwd.trim().is_empty() {
                        project_path = Some(cwd.to_string());
                    }
                }
            }
            "turn_context" => {
                if let Some(value) = payload.get("model").and_then(|v| v.as_str()) {
                    model = Some(value.to_string());
                }
            }
            "response_item" => {
                if payload.get("type").and_then(|v| v.as_str()) != Some("message") {
                    continue;
                }
                let role = payload.get("role").and_then(|v| v.as_str()).unwrap_or("");
                if !matches!(role, "user" | "assistant") {
                    continue;
                }
                let text = content_text(payload.get("content"));
                if text.is_empty() {
                    continue;
                }
                if role == "user" && first_user.is_none() && !is_synthetic(&text) {
                    first_user = Some(text.clone());
                }
                response_messages.push(message_row(role, &text, &ts));
            }
            "event_msg" => {
                let event = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let role = match event {
                    "user_message" => "user",
                    "agent_message" => "assistant",
                    _ => continue,
                };
                let text = payload
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .unwrap_or("");
                if text.is_empty() {
                    continue;
                }
                if role == "user" && first_user.is_none() && !is_synthetic(text) {
                    first_user = Some(text.to_string());
                }
                event_messages.push(message_row(role, text, &ts));
            }
            _ => {}
        }
    }
    let messages = if response_messages.is_empty() {
        event_messages
    } else {
        response_messages
    };
    if messages.is_empty() {
        return Err(format!("{}: no importable messages", path.display()));
    }
    let messages: Vec<Value> = messages.into_iter().take(MAX_MESSAGES).collect();
    let title = title_from(
        first_user.as_deref().unwrap_or(""),
        &format!("Codex {external_id}"),
    );
    Ok(ParsedSession {
        source: "codex",
        external_id,
        title,
        project_path,
        model,
        created_at: first_ts.unwrap_or_else(|| fallback_ts.clone()),
        updated_at: last_ts.unwrap_or(fallback_ts),
        message_count: (!incomplete).then_some(messages.len()),
        messages,
    })
}

fn parse_pi(path: &Path, sampled: bool) -> Result<ParsedSession, String> {
    let (lines, incomplete) = read_lines(path, sampled)?;
    let documents = parse_json_lines(&lines);
    let fallback_ts = file_mtime(path);
    let header = documents.first().cloned().unwrap_or(Value::Null);
    let path_id = external_id_from_path(path);
    let external_id = header
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|id| !id.trim().is_empty())
        .unwrap_or(&path_id)
        .to_string();
    let created_at = timestamp_value(header.get("createdAt"), &fallback_ts);
    let updated_at = timestamp_value(header.get("updatedAt"), &created_at);
    let project_path = header
        .get("cwd")
        .and_then(|v| v.as_str())
        .filter(|path| !path.trim().is_empty())
        .map(str::to_string);
    let model = header
        .get("model")
        .and_then(|v| v.as_str())
        .filter(|model| !model.trim().is_empty())
        .map(str::to_string);
    let mut messages: Vec<Value> = Vec::new();
    let mut first_user: Option<String> = None;
    for document in documents.iter().skip(1) {
        let message = document.get("message").unwrap_or(document);
        let role = message.get("role").and_then(|v| v.as_str()).unwrap_or("");
        if !matches!(role, "user" | "assistant") {
            continue;
        }
        let text = content_text(message.get("content"));
        if text.is_empty() {
            continue;
        }
        if role == "user" && first_user.is_none() && !is_synthetic(&text) {
            first_user = Some(text.clone());
        }
        let ts = timestamp_value(document.get("timestamp"), &created_at);
        messages.push(message_row(role, &text, &ts));
        if messages.len() > MAX_MESSAGES {
            break;
        }
    }
    if messages.is_empty() {
        return Err(format!("{}: no importable messages", path.display()));
    }
    let title = title_from(
        first_user.as_deref().unwrap_or(""),
        header
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or(&format!("Pi {external_id}")),
    );
    Ok(ParsedSession {
        source: "pi",
        external_id,
        title,
        project_path,
        model,
        created_at,
        updated_at,
        message_count: (!incomplete).then_some(messages.len()),
        messages,
    })
}

fn opencode_storage_root() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        let candidate = PathBuf::from(xdg).join("opencode").join("storage");
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    home_path(".local/share/opencode/storage").filter(|path| path.is_dir())
}

/// OpenCode builds that moved to SQLite keep one database beside the JSON
/// store (`opencode.db` today, but the store may pick another name), so every
/// top-level `.db` counts as a candidate.
fn opencode_db_paths() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        roots.push(PathBuf::from(xdg).join("opencode"));
    }
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".local/share/opencode"));
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_db = path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("db"));
            if is_db {
                out.push(path);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn read_json(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn parse_opencode(path: &Path, sampled: bool) -> Result<ParsedSession, String> {
    let _ = sampled;
    let document =
        read_json(path).ok_or_else(|| format!("{}: unreadable session", path.display()))?;
    let path_id = external_id_from_path(path);
    let external_id = document
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|id| !id.trim().is_empty())
        .unwrap_or(&path_id)
        .to_string();
    let fallback_ts = file_mtime(path);
    let created_at = timestamp_value(document.pointer("/time/created"), &fallback_ts);
    let updated_at = timestamp_value(document.pointer("/time/updated"), &created_at);
    let project_path = document
        .get("directory")
        .and_then(|v| v.as_str())
        .filter(|path| !path.trim().is_empty())
        .map(str::to_string);
    let title_seed = document
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let parent = path.parent().unwrap_or(Path::new(""));
    let storage_root = parent
        .parent()
        .and_then(|dir| dir.parent())
        .unwrap_or(Path::new(""));
    let messages_dir = storage_root.join("message").join(&external_id);
    let mut messages: Vec<Value> = Vec::new();
    let mut model: Option<String> = None;
    if messages_dir.is_dir() {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&messages_dir)
            .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
            .unwrap_or_default();
        files.sort();
        for file in files {
            let Some(message) = read_json(&file) else {
                continue;
            };
            let role = message.get("role").and_then(|v| v.as_str()).unwrap_or("");
            if !matches!(role, "user" | "assistant") {
                continue;
            }
            if let Some(value) = message.get("modelID").and_then(|v| v.as_str()) {
                model = Some(value.to_string());
            }
            let ts = timestamp_value(message.pointer("/time/created"), &created_at);
            let message_id = message.get("id").and_then(|v| v.as_str()).unwrap_or("");
            // Newer OpenCode builds keep parts on the message itself; older
            // ones store one JSON file per part, keyed by message id.
            let mut parts: Vec<Value> = message
                .get("parts")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if parts.is_empty() {
                let parts_dir = storage_root.join("part").join(message_id);
                if parts_dir.is_dir() {
                    let mut files: Vec<PathBuf> = std::fs::read_dir(&parts_dir)
                        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
                        .unwrap_or_default();
                    files.sort();
                    parts.extend(files.iter().filter_map(|file| read_json(file)));
                } else if let Some(Value::Array(items)) =
                    read_json(&storage_root.join("part").join(format!("{message_id}.json")))
                {
                    parts.extend(items);
                }
            }
            let mut text = String::new();
            for part in parts {
                if part.get("type").and_then(|v| v.as_str()) == Some("text") {
                    if let Some(value) = part.get("text").and_then(|v| v.as_str()) {
                        if !value.trim().is_empty() {
                            if !text.is_empty() {
                                text.push_str("\n\n");
                            }
                            text.push_str(value.trim());
                        }
                    }
                }
            }
            if text.is_empty() {
                text = content_text(message.get("content"));
            }
            if text.is_empty() {
                continue;
            }
            messages.push(message_row(role, &text, &ts));
            if messages.len() > MAX_MESSAGES {
                break;
            }
        }
    }
    if messages.is_empty() {
        return Err(format!("{}: no importable messages", path.display()));
    }
    let title = if title_seed.trim().is_empty() {
        format!("OpenCode {external_id}")
    } else {
        title_from(&title_seed, &format!("OpenCode {external_id}"))
    };
    Ok(ParsedSession {
        source: "opencode",
        external_id,
        title,
        project_path,
        model,
        created_at,
        updated_at,
        message_count: Some(messages.len()),
        messages,
    })
}

/// One session from a SQLite-backed OpenCode store, mapped into the same
/// `ParsedSession` the JSON reader produces so the import path stays one.
fn parse_opencode_sqlite(db: &Path, external_id: &str) -> Result<ParsedSession, String> {
    let sessions = crate::importer_opencode_sqlite::sessions(db)?;
    let session = sessions
        .into_iter()
        .find(|session| session.id == external_id)
        .ok_or_else(|| format!("{}: session {external_id} not found", db.display()))?;
    if session.messages.is_empty() {
        return Err(format!("{}: no importable messages", db.display()));
    }
    let messages: Vec<Value> = session
        .messages
        .iter()
        .map(|message| message_row(&message.role, &message.text, &message.created_at))
        .collect();
    Ok(ParsedSession {
        source: "opencode",
        external_id: external_id.to_string(),
        title: title_from(&session.title, &format!("OpenCode {external_id}")),
        project_path: session.directory,
        model: session.model,
        created_at: session.created_at,
        updated_at: session.updated_at,
        message_count: Some(messages.len()),
        messages,
    })
}

fn parse_source(source: &str, path: &Path, sampled: bool) -> Result<ParsedSession, String> {
    match source {
        "claude-code" => parse_claude(path, sampled),
        "codex" => parse_codex(path, sampled),
        "opencode" => parse_opencode(path, sampled),
        "pi" => parse_pi(path, sampled),
        other => Err(format!("unknown import source: {other}")),
    }
}

fn discover_sessions() -> Vec<(&'static str, PathBuf)> {
    let mut found: Vec<(&'static str, PathBuf)> = Vec::new();
    if let Some(root) = home_path(".claude/projects") {
        let mut files = Vec::new();
        collect_files(&root, "jsonl", 3, &mut files);
        found.extend(files.into_iter().map(|path| ("claude-code", path)));
    }
    if let Some(root) = home_path(".codex/sessions") {
        let mut files = Vec::new();
        collect_files(&root, "jsonl", 6, &mut files);
        found.extend(files.into_iter().map(|path| ("codex", path)));
    }
    if let Some(root) = opencode_storage_root() {
        let mut files = Vec::new();
        collect_files(&root.join("session"), "json", 3, &mut files);
        found.extend(files.into_iter().map(|path| ("opencode", path)));
    }
    if let Some(root) = home_path(".pigo/sessions") {
        let mut files = Vec::new();
        collect_files(&root, "jsonl", 1, &mut files);
        found.extend(files.into_iter().map(|path| ("pi", path)));
    }
    found
}

async fn existing_session_ids(state: &Arc<AppState>) -> HashSet<String> {
    state
        .host
        .call("session.list", json!({}))
        .await
        .ok()
        .and_then(|payload| payload.get("sessions").and_then(|v| v.as_array()).cloned())
        .map(|sessions| {
            sessions
                .iter()
                .filter_map(|session| session.get("id").and_then(|v| v.as_str()))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub async fn scan(state: &Arc<AppState>) -> Value {
    let existing = existing_session_ids(state).await;
    let mut candidates: Vec<Value> = Vec::new();
    let mut cache = state.session_import_cache.lock().unwrap();
    cache.clear();
    for (source, path) in discover_sessions() {
        let Ok(parsed) = parse_source(source, &path, true) else {
            continue;
        };
        let session_id = sanitized_session_id(parsed.source, &parsed.external_id);
        if existing.contains(&session_id) {
            continue;
        }
        let key = format!("{}:{}", parsed.source, parsed.external_id);
        cache.insert(
            key,
            json!({
                "source": parsed.source,
                "externalId": parsed.external_id,
                "path": path.to_string_lossy(),
            }),
        );
        candidates.push(json!({
            "source": parsed.source,
            "externalId": parsed.external_id,
            "title": parsed.title,
            "projectPath": parsed.project_path,
            "model": parsed.model,
            "createdAt": parsed.created_at,
            "updatedAt": parsed.updated_at,
            "messageCount": parsed.message_count,
        }));
    }
    // The same OpenCode session can exist in both stores during a migration;
    // keep the first (JSON) copy so one conversation is listed once.
    for db in opencode_db_paths() {
        let Ok(sessions) = crate::importer_opencode_sqlite::sessions(&db) else {
            continue;
        };
        for session in sessions {
            if session.messages.is_empty() {
                continue;
            }
            let session_id = sanitized_session_id("opencode", &session.id);
            if existing.contains(&session_id) {
                continue;
            }
            let key = format!("opencode:{}", session.id);
            if cache.contains_key(&key) {
                continue;
            }
            cache.insert(
                key,
                json!({
                    "source": "opencode",
                    "externalId": session.id.clone(),
                    "dbPath": db.to_string_lossy(),
                    "sqlite": true,
                }),
            );
            candidates.push(json!({
                "source": "opencode",
                "externalId": session.id.clone(),
                "title": title_from(&session.title, &format!("OpenCode {}", session.id)),
                "projectPath": session.directory,
                "model": session.model,
                "createdAt": session.created_at,
                "updatedAt": session.updated_at,
                "messageCount": session.messages.len(),
            }));
        }
    }
    // Most recent first; the renderer groups them by source/path.
    candidates.sort_by(|left, right| {
        right
            .get("updatedAt")
            .and_then(|v| v.as_str())
            .cmp(&left.get("updatedAt").and_then(|v| v.as_str()))
    });
    envelope::ok(json!({ "sessions": candidates }))
}

pub async fn run(state: &Arc<AppState>, items: &[Value]) -> Value {
    let mut imported = 0;
    let mut skipped = 0;
    let mut failed = 0;
    for item in items {
        let Some(source) = item.get("source").and_then(|v| v.as_str()) else {
            failed += 1;
            continue;
        };
        let Some(external_id) = item.get("externalId").and_then(|v| v.as_str()) else {
            failed += 1;
            continue;
        };
        let key = format!("{source}:{external_id}");
        let cached = state
            .session_import_cache
            .lock()
            .unwrap()
            .get(&key)
            .cloned();
        let Some(cached) = cached else {
            skipped += 1;
            continue;
        };
        let parsed = if cached.get("sqlite").and_then(Value::as_bool) == Some(true) {
            let db = cached
                .get("dbPath")
                .and_then(Value::as_str)
                .unwrap_or_default();
            parse_opencode_sqlite(Path::new(db), external_id)
        } else {
            let Some(path) = cached.get("path").and_then(|v| v.as_str()) else {
                skipped += 1;
                continue;
            };
            parse_source(source, Path::new(path), false)
        };
        let parsed = match parsed {
            Ok(parsed) => parsed,
            Err(error) => {
                log::warn!("session import failed: {error}");
                failed += 1;
                continue;
            }
        };
        if parsed.messages.is_empty() {
            skipped += 1;
            continue;
        }
        let summary = json!({
            "id": sanitized_session_id(parsed.source, &parsed.external_id),
            "title": parsed.title,
            "messageCount": parsed.messages.len(),
            "projectPath": parsed.project_path,
            "modelId": parsed.model,
            "providerId": null,
            "mode": "agent",
            "thinkingLevel": "off",
            "permissionMode": "default",
            "createdAt": parsed.created_at,
            "updatedAt": parsed.updated_at,
        });
        let result = state
            .host
            .call(
                "session.import",
                json!({ "session": summary, "messages": parsed.messages }),
            )
            .await;
        match result {
            Ok(value) if value.get("imported").and_then(|v| v.as_bool()) == Some(true) => {
                imported += 1
            }
            Ok(_) => skipped += 1,
            Err(error) => {
                log::warn!("session import failed: {error}");
                failed += 1;
            }
        }
    }
    envelope::ok(json!({ "imported": imported, "skipped": skipped, "failed": failed }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_are_stable_and_source_scoped() {
        let first = sanitized_session_id("claude-code", "abc");
        let second = sanitized_session_id("claude-code", "abc");
        assert_eq!(first, second);
        assert!(first.starts_with("import-claude-code-"));
        assert_ne!(
            sanitized_session_id("codex", "abc"),
            sanitized_session_id("pi", "abc")
        );
    }

    #[test]
    fn content_text_keeps_only_readable_blocks() {
        let content = json!([
            { "type": "thinking", "thinking": "hidden" },
            { "type": "text", "text": "hello" },
            { "type": "tool_use", "name": "Bash" },
            { "type": "text", "text": "world" },
        ]);
        assert_eq!(content_text(Some(&content)), "hello\n\nworld");
    }

    #[test]
    fn synthetic_injections_are_not_titles() {
        assert!(is_synthetic("# Context from my IDE setup: repo layout"));
        assert!(!is_synthetic("# Heading"));
    }
}
