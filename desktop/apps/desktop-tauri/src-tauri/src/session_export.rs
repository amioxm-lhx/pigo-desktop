//! Session export in the pigo JSONL format (CLI `/export` parity).
//!
//! The desktop stores transcripts in host-core's own JSONL shape, which is not
//! readable by the CLI. Exporting to the pigo wire format closes the loop with
//! `importer::parse_pi`: a session exported here can be imported by the same
//! desktop (or resumed by the CLI) without a converter.
//!
//! Wire format (see `internal/session/session.go`):
//!   line 1 — `SessionHeader`:  {"version":3,"id":…,"createdAt":…,"updatedAt":…}
//!   line N — `entryWire`:      {"id":…,"parentId":…,"timestamp":…,
//!                              "message":{"role":"user|assistant","content":…}}
//! Entries form a linear parent chain, the degenerate tree the CLI's
//! `PathToLeaf` replays.

use serde_json::{json, Map, Value};

/// Schema version the CLI currently writes (`session.SchemaVersion`).
const SCHEMA_VERSION: i64 = 3;

fn str_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn optional_field(header: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        header.insert(key.to_string(), json!(value));
    }
}

/// Serialize one `session.get` detail into (file contents, exported message
/// count). Only readable user/assistant rows with non-empty text are carried:
/// tool rows, thinking blocks and empty placeholders are presentation detail
/// that `parse_pi` would discard anyway.
pub fn export_body(session: &Value) -> Result<(String, usize), String> {
    let id = str_field(session, "id").ok_or_else(|| "session id required".to_string())?;
    let messages = session
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| "session messages missing".to_string())?;

    let mut header = Map::new();
    header.insert("version".to_string(), json!(SCHEMA_VERSION));
    header.insert("id".to_string(), json!(id));
    header.insert(
        "createdAt".to_string(),
        json!(str_field(session, "createdAt").unwrap_or("")),
    );
    header.insert(
        "updatedAt".to_string(),
        json!(str_field(session, "updatedAt").unwrap_or("")),
    );
    // `title`/`cwd` are additive: the CLI ignores them, while the desktop
    // importer uses them to restore the session title and project path.
    optional_field(&mut header, "title", str_field(session, "title"));
    optional_field(&mut header, "cwd", str_field(session, "projectPath"));
    optional_field(&mut header, "model", str_field(session, "modelId"));
    optional_field(&mut header, "provider", str_field(session, "providerId"));

    let mut lines = vec![
        serde_json::to_string(&Value::Object(header))
            .map_err(|e| format!("encode session header: {e}"))?,
    ];
    let mut parent: Option<String> = None;
    let mut count = 0usize;

    for (index, message) in messages.iter().enumerate() {
        let role = str_field(message, "role").unwrap_or("");
        if !matches!(role, "user" | "assistant") {
            continue;
        }
        let content = str_field(message, "content").unwrap_or("");
        if content.is_empty() {
            continue;
        }
        let entry_id = str_field(message, "id")
            .map(str::to_string)
            .unwrap_or_else(|| format!("export-{index}"));
        let timestamp = str_field(message, "createdAt").unwrap_or("");

        let mut entry = Map::new();
        entry.insert("id".to_string(), json!(entry_id));
        if let Some(previous) = &parent {
            entry.insert("parentId".to_string(), json!(previous));
        }
        entry.insert("timestamp".to_string(), json!(timestamp));
        entry.insert("message".to_string(), json!({ "role": role, "content": content }));
        lines.push(
            serde_json::to_string(&Value::Object(entry))
                .map_err(|e| format!("encode session entry: {e}"))?,
        );
        parent = Some(entry_id);
        count += 1;
    }

    Ok((format!("{}\n", lines.join("\n")), count))
}

/// A safe, human-recognizable save-dialog default. Renderer-supplied titles
/// can contain path separators, so sanitize before handing the name to the
/// native dialog.
pub fn suggested_file_name(session: &Value) -> String {
    let raw = str_field(session, "title")
        .or_else(|| str_field(session, "id"))
        .unwrap_or("session");
    let cleaned: String = raw
        .chars()
        .take(80)
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\n' | '\r' => '_',
            other => other,
        })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim().to_string();
    if cleaned.is_empty() {
        "session.jsonl".to_string()
    } else {
        format!("{cleaned}.jsonl")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail() -> Value {
        json!({
            "id": "sess-1",
            "title": "Fix/Export: bug?",
            "projectPath": "/work/pigo",
            "modelId": "deepseek-v4",
            "providerId": "deepseek",
            "createdAt": "2026-11-07T01:00:00Z",
            "updatedAt": "2026-11-07T02:00:00Z",
            "messages": [
                {"id":"m1","role":"user","content":"hello","createdAt":"2026-11-07T01:00:01Z"},
                {"id":"m2","role":"assistant","content":"hi","createdAt":"2026-11-07T01:00:02Z"},
                {"id":"m3","role":"toolResult","content":"ignored","createdAt":"2026-11-07T01:00:03Z"},
                {"id":"m4","role":"assistant","content":"   ","createdAt":"2026-11-07T01:00:04Z"},
                {"id":"m5","role":"user","content":"thanks","createdAt":"2026-11-07T01:00:05Z"}
            ]
        })
    }

    fn parse(body: &str) -> Vec<Value> {
        body.lines()
            .map(|line| serde_json::from_str(line).expect("valid json line"))
            .collect()
    }

    #[test]
    fn header_carries_cli_schema_and_round_trip_metadata() {
        let (body, _) = export_body(&detail()).expect("export");
        let header = &parse(&body)[0];
        assert_eq!(header["version"], 3);
        assert_eq!(header["id"], "sess-1");
        assert_eq!(header["createdAt"], "2026-11-07T01:00:00Z");
        assert_eq!(header["updatedAt"], "2026-11-07T02:00:00Z");
        assert_eq!(header["cwd"], "/work/pigo");
        assert_eq!(header["model"], "deepseek-v4");
        assert_eq!(header["provider"], "deepseek");
        assert_eq!(header["title"], "Fix/Export: bug?");
    }

    #[test]
    fn entries_form_a_linear_parent_chain_of_readable_rows_only() {
        let (body, count) = export_body(&detail()).expect("export");
        let rows = parse(&body);
        assert_eq!(count, 3);
        assert_eq!(rows.len(), 4);
        let entries = &rows[1..];
        assert_eq!(entries[0]["id"], "m1");
        assert!(entries[0].get("parentId").is_none());
        assert_eq!(entries[1]["parentId"], "m1");
        assert_eq!(entries[2]["id"], "m5");
        assert_eq!(entries[2]["parentId"], "m2");
        assert_eq!(entries[2]["message"]["role"], "user");
        assert_eq!(entries[2]["message"]["content"], "thanks");
        assert!(body.ends_with('\n'));
    }

    #[test]
    fn generated_ids_are_used_when_host_rows_have_none() {
        let session = json!({
            "id": "s",
            "messages": [{"role":"user","content":"a"},{"role":"assistant","content":"b"}]
        });
        let (body, count) = export_body(&session).expect("export");
        let rows = parse(&body);
        assert_eq!(count, 2);
        assert_eq!(rows[1]["id"], "export-0");
        assert_eq!(rows[2]["parentId"], "export-0");
    }

    #[test]
    fn invalid_details_are_rejected() {
        assert!(export_body(&json!({})).is_err());
        assert!(export_body(&json!({ "id": "s" })).is_err());
    }

    #[test]
    fn suggested_name_is_safe_and_short() {
        assert_eq!(suggested_file_name(&detail()), "Fix_Export_ bug_.jsonl");
        assert_eq!(suggested_file_name(&json!({"id":"abc"})), "abc.jsonl");
        assert_eq!(suggested_file_name(&json!({"title":"..."})), "session.jsonl");
        assert_eq!(suggested_file_name(&json!({})), "session.jsonl");
    }
}
