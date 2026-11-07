//! Composer "/" menu sources (D123/D405): the builtin aliases, slash
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

/// One first-party composer alias. `arg_mode` is the renderer's dispatch
/// contract: `"none"` goes straight through the palette switch, `"optional"`
/// may carry a prompt argument, and `"required"` refuses to run without one.
/// Declaring it here keeps the renderer from hard-coding per-command id
/// branches every time a builtin is added.
#[derive(Clone, Copy)]
struct BuiltinAlias {
    name: &'static str,
    id: &'static str,
    title: &'static str,
    arg_mode: &'static str,
}

const BUILTIN_ALIASES: &[BuiltinAlias] = &[
    BuiltinAlias {
        name: "new",
        id: "builtin.session.new",
        title: "New Task",
        arg_mode: "none",
    },
    BuiltinAlias {
        name: "btw",
        id: "builtin.agent.btw",
        title: "Ask Aside (by the way)",
        arg_mode: "required",
    },
    BuiltinAlias {
        name: "compact",
        id: "builtin.agent.compact",
        title: "Compact Conversation Context",
        arg_mode: "none",
    },
    BuiltinAlias {
        name: "copy",
        id: "builtin.session.copy",
        title: "Copy Last Reply",
        arg_mode: "none",
    },
    BuiltinAlias {
        name: "session",
        id: "builtin.session.info",
        title: "Session Info",
        arg_mode: "none",
    },
    BuiltinAlias {
        name: "status",
        id: "builtin.app.status",
        title: "Runtime Status",
        arg_mode: "none",
    },
    BuiltinAlias {
        name: "agent-mode",
        id: "builtin.mode.agent",
        title: "Switch to Agent",
        arg_mode: "optional",
    },
    BuiltinAlias {
        name: "plan-mode",
        id: "builtin.mode.plan",
        title: "Switch to Plan",
        arg_mode: "optional",
    },
    BuiltinAlias {
        name: "goal-mode",
        id: "builtin.mode.goal",
        title: "Switch to Goal",
        arg_mode: "optional",
    },
    BuiltinAlias {
        name: "export",
        id: "builtin.session.export",
        title: "Export Session (pigo JSONL)",
        arg_mode: "none",
    },
    BuiltinAlias {
        name: "rebuild",
        id: "builtin.agent.rebuild",
        title: "Rebuild Conversation Context",
        arg_mode: "none",
    },
    BuiltinAlias {
        name: "goal",
        id: "builtin.agent.goal",
        title: "Set or Inspect Goal",
        arg_mode: "optional",
    },
    BuiltinAlias {
        name: "tree",
        id: "builtin.session.tree",
        title: "Conversation Nodes (tree)",
        arg_mode: "optional",
    },
    BuiltinAlias {
        name: "rewind",
        id: "builtin.session.rewind",
        title: "Restore Points (rewind)",
        arg_mode: "optional",
    },
    BuiltinAlias {
        name: "memory",
        id: "builtin.memory.report",
        title: "Persistent Memory Report",
        arg_mode: "none",
    },
    BuiltinAlias {
        name: "dream",
        id: "builtin.agent.dream",
        title: "Consolidate Memory (dream)",
        arg_mode: "optional",
    },
];

pub async fn commands(state: &AppState, project: Option<&str>) -> Vec<Value> {
    let mut rows: Vec<Value> = Vec::new();
    let mut taken: HashSet<String> = HashSet::new();

    for alias in BUILTIN_ALIASES {
        taken.insert(alias.name.to_string());
        rows.push(json!({
            "name": alias.name,
            "kind": "builtin",
            "title": alias.title,
            "id": alias.id,
            "argMode": alias.arg_mode,
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
