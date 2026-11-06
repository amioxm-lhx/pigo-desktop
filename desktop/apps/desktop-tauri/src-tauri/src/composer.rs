//! Composer "/" menu sources (D123/D405): the five builtin aliases, slash
//! templates from the workspace and the user's pi home, plugin commands, and
//! the skills active for the current project, merged into the single
//! `{ name, kind, title, … }` list the renderer filters locally.
//!
//! Collision order follows spec 04-ux/04 §7: builtin aliases win, then project
//! templates, then user templates, then plugin commands; skills are always
//! last and never shadow a command or template.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::state::AppState;
use crate::subagents::{as_scalar, split_frontmatter};

/// `(alias, palette id, title)`. The aliases are the composer surface of the
/// five first-party commands; dispatch reuses the palette switch.
const BUILTIN_ALIASES: &[(&str, &str, &str)] = &[
    ("new", "builtin.session.new", "New Task"),
    (
        "compact",
        "builtin.agent.compact",
        "Compact Conversation Context",
    ),
    ("agent-mode", "builtin.mode.agent", "Switch to Agent"),
    ("plan-mode", "builtin.mode.plan", "Switch to Plan"),
    ("goal-mode", "builtin.mode.goal", "Switch to Goal"),
];

pub async fn commands(state: &AppState, project: Option<&str>) -> Vec<Value> {
    let mut rows: Vec<Value> = Vec::new();
    let mut taken: HashSet<String> = HashSet::new();

    for (name, id, title) in BUILTIN_ALIASES {
        taken.insert((*name).to_string());
        rows.push(json!({
            "name": name,
            "kind": "builtin",
            "title": title,
            "id": id,
        }));
    }

    if let Some(project) = project {
        push_templates(
            &mut rows,
            &mut taken,
            Path::new(project).join(".pi").join("prompts"),
            "project",
        );
    }
    if let Some(home) = dirs::home_dir() {
        push_templates(
            &mut rows,
            &mut taken,
            home.join(".pi").join("agent").join("prompts"),
            "user",
        );
    }

    for command in state.plugins.composer_commands() {
        let name = command
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim_start_matches('/')
            .to_string();
        if name.is_empty() || !taken.insert(name.clone()) {
            continue;
        }
        let mut row = command;
        row["name"] = json!(name);
        rows.push(row);
    }

    if let Ok(result) = state
        .host
        .call("skills.active", json!({ "projectPath": project }))
        .await
    {
        push_skills(&mut rows, &mut taken, &result);
    }

    rows
}

/// `<dir>/*.md` → template rows. The file stem is the slash name; the
/// frontmatter supplies the description and the `argument-hint` ghost text.
fn push_templates(rows: &mut Vec<Value>, taken: &mut HashSet<String>, dir: PathBuf, source: &str) {
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut found: Vec<Value> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let raw = std::fs::read_to_string(&path).unwrap_or_default();
        let (frontmatter, _body) = split_frontmatter(&raw);
        let mut row = json!({
            "name": name,
            "kind": "template",
            "title": name,
            "source": source,
        });
        if let Some(description) = as_scalar(frontmatter.get("description")) {
            row["description"] = json!(description);
        }
        if let Some(hint) = as_scalar(frontmatter.get("argumenthint")) {
            row["argumentHint"] = json!(hint);
        }
        found.push(row);
    }
    found.sort_by(|left, right| {
        left["name"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .cmp(&right["name"].as_str().unwrap_or("").to_lowercase())
    });
    for row in found {
        let name = row["name"].as_str().unwrap_or("").to_string();
        if taken.insert(name) {
            rows.push(row);
        }
    }
}

/// Active skills use their id as the slash name and stay in the last group.
fn push_skills(rows: &mut Vec<Value>, taken: &mut HashSet<String>, result: &Value) {
    let Some(skills) = result.get("skills").and_then(Value::as_array) else {
        return;
    };
    for skill in skills {
        let id = skill.get("id").and_then(Value::as_str).unwrap_or("");
        let id = id.trim();
        if id.is_empty() || !taken.insert(id.to_string()) {
            continue;
        }
        let title = skill.get("name").and_then(Value::as_str).unwrap_or(id);
        let mut row = json!({
            "name": id,
            "kind": "skill",
            "title": title,
            "skillId": id,
        });
        if let Some(description) = skill.get("description").and_then(Value::as_str) {
            if !description.is_empty() {
                row["description"] = json!(description);
            }
        }
        rows.push(row);
    }
}
