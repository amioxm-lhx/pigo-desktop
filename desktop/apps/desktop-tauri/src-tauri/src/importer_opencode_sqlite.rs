//! Read sessions from the OpenCode builds that keep their store in SQLite
//! instead of the `storage/{session,message,part}` JSON tree.
//!
//! The schema is discovered from the database instead of hard-coded: OpenCode
//! ships several storage generations, and a session that cannot be understood
//! is skipped by the caller rather than guessed at. The live file is copied
//! (with its WAL sidecars) before opening, so a running OpenCode never sees a
//! write and this process never holds a lock on the owner's store.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use serde_json::Value;

pub struct DbMessage {
    pub role: String,
    pub text: String,
    pub created_at: String,
}

pub struct DbSession {
    pub id: String,
    pub title: String,
    pub directory: Option<String>,
    pub model: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<DbMessage>,
}

/// A copied store plus its temporary directory; dropping it removes both.
struct Store {
    conn: Connection,
    dir: PathBuf,
}

impl Drop for Store {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

impl Store {
    fn open(db: &Path) -> Result<Self, String> {
        let dir =
            std::env::temp_dir().join(format!("pigo-opencode-import-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).map_err(|error| format!("import scratch: {error}"))?;
        let copy = dir.join("store.db");
        fs::copy(db, &copy).map_err(|error| format!("copy {}: {error}", db.display()))?;
        for suffix in ["-wal", "-shm"] {
            let sidecar = PathBuf::from(format!("{}{suffix}", db.display()));
            if sidecar.is_file() {
                let _ = fs::copy(&sidecar, dir.join(format!("store.db{suffix}")));
            }
        }
        let conn = Connection::open(&copy).map_err(|error| format!("open store copy: {error}"))?;
        Ok(Self { conn, dir })
    }
}

pub fn sessions(db: &Path) -> Result<Vec<DbSession>, String> {
    let store = Store::open(db)?;
    let conn = &store.conn;
    let tables = tables(conn)?;
    let session_table = session_table(conn, &tables)?;
    let message_table = message_table(conn, &tables);
    let part_table = part_table(conn, &tables);

    let mut stmt = conn
        .prepare(&format!("SELECT * FROM {}", quote(&session_table)))
        .map_err(|error| format!("read {session_table}: {error}"))?;
    let mut rows = stmt.query([]).map_err(|error| error.to_string())?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(|error| error.to_string())? {
        let Some(id) = text_at(row, &["id", "session_id", "sessionID"]) else {
            continue;
        };
        let title = text_at(row, &["title", "slug"]).unwrap_or_else(|| format!("OpenCode {id}"));
        let directory = text_at(row, &["directory", "path", "worktree"]).or_else(|| {
            let project_id = text_at(row, &["project_id", "projectID"])?;
            project_directory(conn, &tables, &project_id)
        });
        let model = text_at(row, &["model", "model_id", "modelID"]);
        let created = ms_at(row, &["time_created", "created_at", "created"]).map(iso);
        let updated = ms_at(row, &["time_updated", "updated_at", "updated"]).map(iso);
        let drafts = match &message_table {
            Some(table) => read_messages(conn, table, part_table.as_ref(), &id),
            None => Vec::new(),
        };
        let model = model.or_else(|| drafts.iter().find_map(|message| message.model.clone()));
        let created_at = created.unwrap_or_else(now_iso);
        let updated_at = updated.unwrap_or_else(|| created_at.clone());
        out.push(DbSession {
            id,
            title,
            directory,
            model,
            created_at: created_at.clone(),
            updated_at,
            messages: drafts
                .into_iter()
                .map(|message| DbMessage {
                    role: message.role,
                    text: message.text,
                    created_at: message.created_at.unwrap_or_else(|| created_at.clone()),
                })
                .collect(),
        });
    }
    Ok(out)
}

struct MessageTable {
    table: String,
    link: Option<String>,
    role: Option<String>,
    data: Option<String>,
    content: Option<String>,
    time: Option<String>,
}

struct PartTable {
    table: String,
    link: String,
    data: Option<String>,
    text: Option<String>,
    content: Option<String>,
}

struct DraftMessage {
    role: String,
    text: String,
    created_at: Option<String>,
    model: Option<String>,
}

fn read_messages(
    conn: &Connection,
    table: &MessageTable,
    parts: Option<&PartTable>,
    session_id: &str,
) -> Vec<DraftMessage> {
    let (sql, linked) = match &table.link {
        Some(link) => (
            format!(
                "SELECT * FROM {} WHERE {} = ?1",
                quote(&table.table),
                quote(link)
            ),
            true,
        ),
        None => (format!("SELECT * FROM {}", quote(&table.table)), false),
    };
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return Vec::new();
    };
    let mut query = if linked {
        match stmt.query(rusqlite::params![session_id]) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        }
    } else {
        match stmt.query([]) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        }
    };
    let mut out = Vec::new();
    while let Some(row) = query.next().unwrap_or(None) {
        let raw = table
            .data
            .as_deref()
            .and_then(|col| json_at(row, col))
            .unwrap_or(Value::Null);
        // Without a link column the session id has to live in the JSON blob.
        if !linked {
            let candidate = raw
                .get("sessionID")
                .or_else(|| raw.get("sessionId"))
                .or_else(|| raw.get("session_id"))
                .and_then(Value::as_str);
            if candidate != Some(session_id) {
                continue;
            }
        }
        let role = table
            .role
            .as_deref()
            .and_then(|col| text_at(row, &[col]))
            .or_else(|| raw.get("role").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_default();
        if !matches!(role.as_str(), "user" | "assistant") {
            continue;
        }
        let message_id = text_at(row, &["id", "message_id", "messageID"]).unwrap_or_default();
        let mut text = parts
            .map(|parts| part_text(conn, parts, &message_id))
            .unwrap_or_default();
        if text.is_empty() {
            text = raw.get("parts").and_then(parts_text).unwrap_or_default();
        }
        if text.is_empty() {
            text = table
                .content
                .as_deref()
                .and_then(|col| json_at(row, col))
                .map(|raw| content_text(&raw))
                .unwrap_or_default();
        }
        if text.is_empty() {
            continue;
        }
        let model = raw
            .get("modelID")
            .or_else(|| raw.get("model"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let created = table
            .time
            .as_deref()
            .and_then(|col| ms_at(row, &[col]))
            .map(iso);
        out.push(DraftMessage {
            role,
            text,
            created_at: created,
            model,
        });
    }
    out.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    out
}

fn part_text(conn: &Connection, table: &PartTable, message_id: &str) -> String {
    let sql = format!(
        "SELECT * FROM {} WHERE {} = ?1",
        quote(&table.table),
        quote(&table.link)
    );
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return String::new();
    };
    let Ok(mut rows) = stmt.query(rusqlite::params![message_id]) else {
        return String::new();
    };
    let mut pieces = Vec::new();
    while let Some(row) = rows.next().unwrap_or(None) {
        let raw = table
            .data
            .as_deref()
            .and_then(|col| json_at(row, col))
            .unwrap_or(Value::Null);
        if raw.get("type").and_then(Value::as_str) != Some("text") {
            continue;
        }
        let text = raw
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| table.text.as_deref().and_then(|col| text_at(row, &[col])))
            .or_else(|| {
                table
                    .content
                    .as_deref()
                    .and_then(|col| text_at(row, &[col]))
            })
            .unwrap_or_default();
        let text = text.trim();
        if !text.is_empty() {
            pieces.push(text.to_string());
        }
    }
    pieces.join("\n\n")
}

fn session_table(conn: &Connection, tables: &[String]) -> Result<String, String> {
    if let Some(table) = tables
        .iter()
        .find(|table| !is_noise(table) && table.eq_ignore_ascii_case("session"))
    {
        return Ok(table.clone());
    }
    for table in tables.iter().filter(|table| !is_noise(table)) {
        let cols = columns(conn, table)?;
        if has_any(&cols, &["id", "session_id"])
            && has_any(&cols, &["title", "slug", "time_created", "directory"])
        {
            return Ok(table.clone());
        }
    }
    Err("no OpenCode session table in this database".into())
}

fn is_noise(table: &str) -> bool {
    table.starts_with("sqlite_")
}

fn message_table(conn: &Connection, tables: &[String]) -> Option<MessageTable> {
    let pick = tables
        .iter()
        .find(|table| table.eq_ignore_ascii_case("message"))
        .or_else(|| {
            tables.iter().find(|table| {
                columns(conn, table).ok().is_some_and(|cols| {
                    has_any(&cols, &["role", "data"])
                        && has_any(&cols, &["session_id", "sessionID", "sessionId", "id"])
                })
            })
        })?;
    let cols = columns(conn, pick).ok()?;
    Some(MessageTable {
        table: pick.clone(),
        link: find_col(&cols, &["session_id", "sessionID", "sessionId"]).map(str::to_string),
        role: find_col(&cols, &["role"]).map(str::to_string),
        data: find_col(&cols, &["data", "message", "json"]).map(str::to_string),
        content: find_col(&cols, &["content", "text"]).map(str::to_string),
        time: find_col(&cols, &["time_created", "created_at", "created"]).map(str::to_string),
    })
}

fn part_table(conn: &Connection, tables: &[String]) -> Option<PartTable> {
    let pick = tables
        .iter()
        .find(|table| table.eq_ignore_ascii_case("part"))
        .or_else(|| {
            tables.iter().find(|table| {
                columns(conn, table).ok().is_some_and(|cols| {
                    has_any(&cols, &["message_id", "messageID", "messageId"])
                        && has_any(&cols, &["data", "text", "content", "json"])
                })
            })
        })?;
    let cols = columns(conn, pick).ok()?;
    Some(PartTable {
        table: pick.clone(),
        link: find_col(&cols, &["message_id", "messageID", "messageId"])?.to_string(),
        data: find_col(&cols, &["data", "json"]).map(str::to_string),
        text: find_col(&cols, &["text"]).map(str::to_string),
        content: find_col(&cols, &["content"]).map(str::to_string),
    })
}

fn project_directory(conn: &Connection, tables: &[String], project_id: &str) -> Option<String> {
    let table = tables
        .iter()
        .find(|table| table.eq_ignore_ascii_case("project"))?;
    let cols = columns(conn, table).ok()?;
    let dir_col = find_col(&cols, &["worktree", "directory", "path", "root"])?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {} FROM {} WHERE id = ?1 LIMIT 1",
            quote(dir_col),
            quote(table)
        ))
        .ok()?;
    stmt.query_row(rusqlite::params![project_id], |row| row.get::<_, String>(0))
        .ok()
}

fn tables(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    Ok(rows.flatten().collect())
}

fn columns(conn: &Connection, table: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", quote(table)))
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?;
    Ok(rows.flatten().collect())
}

fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn has_any(cols: &[String], names: &[&str]) -> bool {
    find_col(cols, names).is_some()
}

fn find_col<'a>(cols: &'a [String], names: &[&str]) -> Option<&'a str> {
    cols.iter()
        .map(String::as_str)
        .find(|col| names.iter().any(|want| col.eq_ignore_ascii_case(want)))
}

fn text_at(row: &rusqlite::Row<'_>, names: &[&str]) -> Option<String> {
    for name in names {
        let Ok(value) = row.get::<_, SqlValue>(*name) else {
            continue;
        };
        match value {
            SqlValue::Text(text) => return Some(text),
            SqlValue::Integer(number) => return Some(number.to_string()),
            SqlValue::Real(number) => return Some(number.to_string()),
            _ => continue,
        }
    }
    None
}

fn ms_at(row: &rusqlite::Row<'_>, names: &[&str]) -> Option<i64> {
    for name in names {
        let Ok(value) = row.get::<_, SqlValue>(*name) else {
            continue;
        };
        match value {
            SqlValue::Integer(number) => return Some(normalize_ms(number)),
            SqlValue::Real(number) => return Some(normalize_ms(number as i64)),
            SqlValue::Text(text) => {
                if let Ok(number) = text.trim().parse::<i64>() {
                    return Some(normalize_ms(number));
                }
                if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(text.trim()) {
                    return Some(parsed.timestamp_millis());
                }
            }
            _ => continue,
        }
    }
    None
}

fn normalize_ms(value: i64) -> i64 {
    if value.abs() < 1_000_000_000_000 {
        value * 1000
    } else {
        value
    }
}

fn iso(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn json_at(row: &rusqlite::Row<'_>, column: &str) -> Option<Value> {
    let Ok(value) = row.get::<_, SqlValue>(column) else {
        return None;
    };
    match value {
        SqlValue::Text(text) => Some(serde_json::from_str(&text).unwrap_or(Value::String(text))),
        _ => None,
    }
}

fn parts_text(parts: &Value) -> Option<String> {
    let items = parts.as_array()?;
    let text = items
        .iter()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    (!text.is_empty()).then_some(text)
}

fn content_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.trim().to_string(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                if item.get("type").and_then(Value::as_str) == Some("text") {
                    item.get("text").and_then(Value::as_str)
                } else {
                    None
                }
            })
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::sessions;
    use rusqlite::Connection;
    use std::path::PathBuf;

    fn fixture() -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pigo-sqlite-fixture-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("opencode.db");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE session (
                id TEXT PRIMARY KEY, title TEXT, directory TEXT, model TEXT,
                time_created INTEGER, time_updated INTEGER
            );
            CREATE TABLE message (
                id TEXT PRIMARY KEY, session_id TEXT, role TEXT, time_created INTEGER
            );
            CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, data TEXT);
            INSERT INTO session VALUES
                ('ses_1', 'Fix the parser', '/tmp/proj', 'claude-sonnet', 1700000000000, 1700000100000);
            INSERT INTO message VALUES
                ('msg_1', 'ses_1', 'user', 1700000000000),
                ('msg_2', 'ses_1', 'assistant', 1700000050000);
            INSERT INTO part VALUES
                ('p1', 'msg_1', '{"type":"text","text":"Why does this fail?"}'),
                ('p2', 'msg_2', '{"type":"text","text":"Because the tokenizer drops it."}'),
                ('p3', 'msg_2', '{"type":"tool","tool":"bash"}');
            "#,
        )
        .unwrap();
        db
    }

    #[test]
    fn reads_sessions_messages_and_parts_from_sqlite() {
        let db = fixture();
        let sessions = sessions(&db).unwrap();
        assert_eq!(sessions.len(), 1);
        let session = &sessions[0];
        assert_eq!(session.id, "ses_1");
        assert_eq!(session.title, "Fix the parser");
        assert_eq!(session.directory.as_deref(), Some("/tmp/proj"));
        assert_eq!(session.model.as_deref(), Some("claude-sonnet"));
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, "user");
        assert_eq!(session.messages[0].text, "Why does this fail?");
        assert_eq!(session.messages[1].role, "assistant");
        assert_eq!(session.messages[1].text, "Because the tokenizer drops it.");
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
    }

    #[test]
    fn a_database_without_a_session_table_is_reported() {
        let dir = std::env::temp_dir().join(format!("pigo-sqlite-empty-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("empty.db");
        Connection::open(&db).unwrap();
        assert!(sessions(&db).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
