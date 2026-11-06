//! `pulls/list`: open pull requests for the active workspace, read through the
//! GitHub CLI. A missing or signed-out `gh` is reported as an error instead of
//! an empty list that would read as "nothing needs review".

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};

const FIELDS: &str = "number,title,url,author,headRefName,baseRefName,updatedAt,isDraft";
const TIMEOUT: Duration = Duration::from_secs(30);

pub async fn list(root: Option<PathBuf>) -> Value {
    let Some(root) = root else {
        return json!({ "pulls": [], "error": "NO_WORKSPACE" });
    };
    let output = tokio::time::timeout(
        TIMEOUT,
        tokio::process::Command::new("gh")
            .args(["pr", "list", "--limit", "30", "--json", FIELDS])
            .current_dir(&root)
            // A prompt would hang a process the owner cannot answer.
            .env("GH_PROMPT_DISABLED", "1")
            .kill_on_drop(true)
            .output(),
    )
    .await;

    let output = match output {
        Err(_) => return json!({ "pulls": [], "error": "gh pr list timed out" }),
        Ok(Err(error)) => {
            return json!({
                "pulls": [],
                "error": format!("the GitHub CLI (gh) is not available: {error}")
            });
        }
        Ok(Ok(output)) => output,
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .map(str::trim)
            .unwrap_or("gh pr list failed");
        return json!({ "pulls": [], "error": detail });
    }

    match serde_json::from_slice::<Value>(&output.stdout) {
        Ok(payload) => json!({ "pulls": map_rows(&payload) }),
        Err(_) => json!({ "pulls": [], "error": "gh pr list returned unreadable output" }),
    }
}

fn map_rows(payload: &Value) -> Vec<Value> {
    payload
        .as_array()
        .map(|rows| rows.iter().filter_map(map_row).collect())
        .unwrap_or_default()
}

fn map_row(row: &Value) -> Option<Value> {
    let number = row.get("number")?.as_i64()?;
    let mut summary = json!({
        "number": number,
        "title": row.get("title").and_then(Value::as_str).unwrap_or(""),
        "url": row.get("url").and_then(Value::as_str).unwrap_or(""),
        "isDraft": row.get("isDraft").and_then(Value::as_bool).unwrap_or(false),
    });
    // `gh` reports the author as an object; the page only needs the login.
    let author = row
        .get("author")
        .and_then(|author| author.get("login"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if !author.is_empty() {
        summary["author"] = json!(author);
    }
    for (from, to) in [
        ("headRefName", "headRefName"),
        ("baseRefName", "baseRefName"),
        ("updatedAt", "updatedAt"),
    ] {
        if let Some(value) = row.get(from).and_then(Value::as_str) {
            summary[to] = json!(value);
        }
    }
    Some(summary)
}

#[cfg(test)]
mod tests {
    use super::map_rows;
    use serde_json::json;

    #[test]
    fn rows_flatten_the_author_object_and_keep_the_page_fields() {
        let rows = map_rows(&json!([
            {
                "number": 42,
                "title": "Fix the flaky test",
                "url": "https://github.com/org/repo/pull/42",
                "author": { "login": "octocat" },
                "headRefName": "fix/flaky",
                "baseRefName": "main",
                "updatedAt": "2026-11-04T08:00:00Z",
                "isDraft": false
            },
            {
                "number": 7,
                "title": "Draft: refactor",
                "url": "https://github.com/org/repo/pull/7",
                "isDraft": true
            }
        ]));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["number"], 42);
        assert_eq!(rows[0]["author"], "octocat");
        assert_eq!(rows[0]["baseRefName"], "main");
        assert_eq!(rows[1]["isDraft"], true);
        assert!(rows[1].get("author").is_none());
    }

    #[test]
    fn rows_without_a_number_are_dropped() {
        assert!(map_rows(&json!([{ "title": "no number" }])).is_empty());
        assert!(map_rows(&json!({ "not": "an array" })).is_empty());
    }
}
