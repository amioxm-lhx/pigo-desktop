//! Subagent catalog for the `Task` tool: the shipped builtins plus the user's
//! global `~/.agents/subagents/*.md` documents owned by host-core.
//!
//! The parser mirrors `packages/shared/src/subagent-definition.ts` so the
//! renderer, the shell and the sidecar agree on what a document means. Builtins
//! are inline for the same reason as in `agent-runtime`: they must exist in
//! every install, and a missing packaged resource is a worse failure than a
//! constant.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Serialize;
use serde_json::{json, Value};

use crate::state::AppState;

pub const MAX_SUBAGENT_DEFINITIONS: usize = 16;
pub const MAX_SUBAGENT_PROVIDERS: usize = 8;
const MAX_SUBAGENT_MAX_TURNS: u32 = 80;
const MAX_SUBAGENT_MAX_TOKENS: u32 = 200_000;
const DEFAULT_IDLE_TIMEOUT_SECONDS: u32 = 300;
const MIN_IDLE_TIMEOUT_SECONDS: u32 = 10;
const MAX_IDLE_TIMEOUT_SECONDS: u32 = 21_600;
const DEFAULT_MAX_DURATION_SECONDS: u32 = 21_600;
const MIN_MAX_DURATION_SECONDS: u32 = 60;
const MAX_MAX_DURATION_SECONDS: u32 = 21_600;

const OAUTH_AUTH_KIND: &str = "oauth";
const INHERIT_TOKEN: &str = "inherit";
const ASSIGNABLE_TOOLS: &[&str] = &[
    "Read",
    "Glob",
    "Grep",
    "BrowserPreview",
    "Bash",
    "Edit",
    "Write",
];
const DEFAULT_TOOLS: &[&str] = &["Read", "Glob", "Grep"];
const THINKING_LEVELS: &[&str] = &[
    "off", "minimal", "low", "medium", "high", "xhigh", "max", "omit",
];
const PERMISSIONS: &[&str] = &["inherit", "ask", "accept-edits", "auto"];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentDefinition {
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub inherit_tools: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<SubagentModelPin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    pub idle_timeout_seconds: u32,
    pub max_duration_seconds: u32,
    pub prompt: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentModelPin {
    pub provider_id: String,
    pub model_id: String,
}

#[derive(Default)]
pub struct Catalog {
    pub definitions: Vec<SubagentDefinition>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) enum FrontmatterValue {
    Scalar(String),
    List(Vec<String>),
}

pub(crate) type Frontmatter = HashMap<String, FrontmatterValue>;

fn normalize_key(key: &str) -> String {
    key.trim()
        .to_lowercase()
        .chars()
        .filter(|c| !matches!(c, '-' | '_' | ' '))
        .collect()
}

fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let first = trimmed.chars().next().unwrap_or(' ');
        let last = trimmed.chars().last().unwrap_or(' ');
        if (first == '"' || first == '\'') && first == last {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}

/// Split the leading `---` block. Only the flat subset definitions use is
/// supported: `key: value` scalars plus inline (`[a, b]` / `a, b`) or block
/// (`- item`) lists. Shared with the composer slash-template loader.
pub(crate) fn split_frontmatter(raw: &str) -> (Frontmatter, String) {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut frontmatter = Frontmatter::new();
    if !normalized.starts_with("---\n") {
        return (frontmatter, normalized.trim().to_string());
    }
    let Some(end) = normalized[3..].find("\n---").map(|offset| offset + 3) else {
        return (frontmatter, normalized.trim().to_string());
    };
    let block = normalized.get(4..end).unwrap_or("");
    let body = normalized[end + 4..]
        .trim_start_matches([' ', '\t', '\n'])
        .to_string();

    let mut last_key: Option<String> = None;
    for line in block.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let item = line
            .trim_start_matches([' ', '\t'])
            .strip_prefix("- ")
            .or_else(|| line.trim_start_matches([' ', '\t']).strip_prefix("-\t"));
        if let Some(item) = item {
            if let Some(key) = &last_key {
                let value = unquote(item);
                let entry = frontmatter
                    .entry(key.clone())
                    .or_insert_with(|| FrontmatterValue::List(Vec::new()));
                if let FrontmatterValue::List(list) = entry {
                    if !value.is_empty() {
                        list.push(value);
                    }
                }
                continue;
            }
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if !key
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic())
            .unwrap_or(false)
        {
            continue;
        }
        if !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ' '))
        {
            continue;
        }
        let key = normalize_key(key);
        let value = value.trim();
        last_key = Some(key.clone());
        let entry = if value.is_empty() {
            FrontmatterValue::List(Vec::new())
        } else {
            FrontmatterValue::Scalar(unquote(value))
        };
        frontmatter.insert(key, entry);
    }
    (frontmatter, body.trim().to_string())
}

pub(crate) fn as_scalar(value: Option<&FrontmatterValue>) -> Option<String> {
    match value {
        Some(FrontmatterValue::Scalar(text)) if !text.is_empty() => Some(text.clone()),
        Some(FrontmatterValue::List(items)) if items.len() == 1 => Some(items[0].clone()),
        _ => None,
    }
}

fn as_list(value: Option<&FrontmatterValue>) -> Vec<String> {
    match value {
        Some(FrontmatterValue::List(items)) => items.clone(),
        Some(FrontmatterValue::Scalar(text)) => text
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split(',')
            .map(unquote)
            .filter(|entry| !entry.is_empty())
            .collect(),
        None => Vec::new(),
    }
}

/// Filename (or frontmatter `name`) to definition id.
pub fn normalize_name(value: &str) -> String {
    let basename = value
        .replace('\\', "/")
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string();
    let basename = if basename.to_lowercase().ends_with(".md") {
        &basename[..basename.len() - 3]
    } else {
        basename.as_str()
    };
    basename.trim().to_lowercase().replace([' ', '_'], "-")
}

fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    name.len() <= 40 && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

struct ParseOptions<'a> {
    source: &'a str,
    fallback_name: Option<&'a str>,
    file_path: Option<&'a str>,
}

fn parse_definition(
    raw: &str,
    options: ParseOptions<'_>,
) -> Result<(SubagentDefinition, Vec<String>), Vec<String>> {
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let (frontmatter, body) = split_frontmatter(raw);

    let declared_name = as_scalar(frontmatter.get("name"));
    let name = normalize_name(
        declared_name
            .as_deref()
            .or(options.fallback_name)
            .unwrap_or(""),
    );
    if name.is_empty() {
        errors.push("missing `name` and no filename to fall back on".into());
    } else if !is_valid_name(&name) {
        errors.push(format!(
            "invalid name \"{name}\": use lowercase letters, digits and dashes (max 40 chars)"
        ));
    }

    let description = as_scalar(frontmatter.get("description"))
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    if description.is_empty() {
        errors.push("missing `description`".into());
    }

    let declared_tools = as_list(frontmatter.get("tools"));
    let mut inherit_tools = false;
    let tools: Vec<String> = if declared_tools.is_empty() {
        DEFAULT_TOOLS.iter().map(|tool| tool.to_string()).collect()
    } else if declared_tools.len() == 1 && declared_tools[0] == "*" {
        ASSIGNABLE_TOOLS
            .iter()
            .map(|tool| tool.to_string())
            .collect()
    } else {
        let mut accepted: Vec<String> = Vec::new();
        for tool in &declared_tools {
            if tool == INHERIT_TOKEN {
                inherit_tools = true;
                continue;
            }
            if ASSIGNABLE_TOOLS.contains(&tool.as_str()) {
                if !accepted.contains(tool) {
                    accepted.push(tool.clone());
                }
            } else {
                warnings.push(format!("ignoring unknown tool \"{tool}\""));
            }
        }
        if !inherit_tools && accepted.is_empty() {
            errors.push("`tools` lists no usable tool".into());
            DEFAULT_TOOLS.iter().map(|tool| tool.to_string()).collect()
        } else {
            accepted
        }
    };

    let model = parse_model_pin(&frontmatter, &mut errors);

    let thinking_level = as_scalar(frontmatter.get("thinkinglevel")).and_then(|declared| {
        let candidate = declared.trim().to_lowercase();
        if THINKING_LEVELS.contains(&candidate.as_str()) {
            Some(candidate)
        } else {
            warnings.push(format!("ignoring unknown thinking level \"{declared}\""));
            None
        }
    });

    let permission = as_scalar(frontmatter.get("permission")).and_then(|declared| {
        let candidate = declared.trim().to_lowercase();
        if !PERMISSIONS.contains(&candidate.as_str()) {
            warnings.push(format!(
                "ignoring unknown permission \"{declared}\" (use inherit, ask, accept-edits or auto)"
            ));
            None
        } else if candidate != "inherit" {
            Some(candidate)
        } else {
            None
        }
    });

    let max_turns = parse_max_turns(as_scalar(frontmatter.get("maxturns")), &mut warnings);
    let max_tokens = parse_max_tokens(as_scalar(frontmatter.get("maxtokens")), &mut warnings);
    let idle_timeout_seconds = parse_timeout(
        as_scalar(frontmatter.get("idletimeout"))
            .or_else(|| as_scalar(frontmatter.get("idletimeoutseconds"))),
        "idle-timeout",
        DEFAULT_IDLE_TIMEOUT_SECONDS,
        MIN_IDLE_TIMEOUT_SECONDS,
        MAX_IDLE_TIMEOUT_SECONDS,
        &mut warnings,
    );
    let max_duration_seconds = parse_timeout(
        as_scalar(frontmatter.get("maxduration"))
            .or_else(|| as_scalar(frontmatter.get("maxdurationseconds"))),
        "max-duration",
        DEFAULT_MAX_DURATION_SECONDS,
        MIN_MAX_DURATION_SECONDS,
        MAX_MAX_DURATION_SECONDS,
        &mut warnings,
    );

    let prompt = body.trim().to_string();
    if prompt.is_empty() {
        errors.push("document body is empty (nothing to instruct)".into());
    }

    if !errors.is_empty() {
        return Err(errors);
    }
    Ok((
        SubagentDefinition {
            name,
            description,
            tools,
            inherit_tools,
            model,
            thinking_level,
            permission,
            max_turns,
            max_tokens,
            idle_timeout_seconds,
            max_duration_seconds,
            prompt,
            source: options.source.to_string(),
            file_path: options.file_path.map(str::to_string),
        },
        warnings,
    ))
}

fn parse_model_pin(
    frontmatter: &Frontmatter,
    errors: &mut Vec<String>,
) -> Option<SubagentModelPin> {
    let declared_provider = as_scalar(frontmatter.get("provider")).map(|v| v.trim().to_string());
    let declared_model = as_scalar(frontmatter.get("model")).map(|v| v.trim().to_string());
    if declared_provider.as_deref().unwrap_or("").is_empty()
        && declared_model.as_deref().unwrap_or("").is_empty()
    {
        return None;
    }
    let Some(model) = declared_model.filter(|value| !value.is_empty()) else {
        errors.push("`provider` given without `model`".into());
        return None;
    };
    if let Some(provider) = declared_provider.filter(|value| !value.is_empty()) {
        return Some(SubagentModelPin {
            provider_id: provider,
            model_id: model,
        });
    }
    match model.find('/') {
        Some(slash) if slash > 0 && slash < model.len() - 1 => Some(SubagentModelPin {
            provider_id: model[..slash].to_string(),
            model_id: model[slash + 1..].to_string(),
        }),
        _ => {
            errors.push(format!(
                "`model` must be \"<provider>/<model>\" or paired with `provider` (got \"{model}\")"
            ));
            None
        }
    }
}

fn parse_max_turns(value: Option<String>, warnings: &mut Vec<String>) -> Option<u32> {
    let value = value?;
    if value.trim().is_empty() || value.trim().eq_ignore_ascii_case("none") {
        return None;
    }
    let Ok(parsed) = value.trim().parse::<i64>() else {
        warnings.push(format!(
            "ignoring invalid `maxTurns` \"{value}\" (unlimited)"
        ));
        return None;
    };
    if parsed == 0 {
        return None;
    }
    if parsed < 0 {
        warnings.push(format!(
            "ignoring invalid `maxTurns` \"{value}\" (unlimited)"
        ));
        return None;
    }
    if parsed > i64::from(MAX_SUBAGENT_MAX_TURNS) {
        warnings.push(format!(
            "clamping `maxTurns` {parsed} to {MAX_SUBAGENT_MAX_TURNS}"
        ));
        return Some(MAX_SUBAGENT_MAX_TURNS);
    }
    Some(parsed as u32)
}

fn parse_max_tokens(value: Option<String>, warnings: &mut Vec<String>) -> Option<u32> {
    let value = value?;
    if value.trim().is_empty() || value.trim().eq_ignore_ascii_case("none") {
        return None;
    }
    let Ok(parsed) = value.trim().parse::<i64>() else {
        warnings.push(format!(
            "ignoring invalid `maxTokens` \"{value}\" (following the model limit)"
        ));
        return None;
    };
    if parsed == 0 {
        return None;
    }
    if parsed < 1 {
        warnings.push(format!(
            "ignoring invalid `maxTokens` \"{value}\" (following the model limit)"
        ));
        return None;
    }
    if parsed > i64::from(MAX_SUBAGENT_MAX_TOKENS) {
        warnings.push(format!(
            "clamping `maxTokens` {parsed} to {MAX_SUBAGENT_MAX_TOKENS}"
        ));
        return Some(MAX_SUBAGENT_MAX_TOKENS);
    }
    Some(parsed as u32)
}

fn parse_timeout(
    value: Option<String>,
    key: &str,
    fallback: u32,
    minimum: u32,
    maximum: u32,
    warnings: &mut Vec<String>,
) -> u32 {
    let Some(value) = value else {
        return fallback;
    };
    let Ok(parsed) = value.trim().parse::<i64>() else {
        warnings.push(format!(
            "ignoring invalid `{key}` \"{value}\" (using {fallback})"
        ));
        return fallback;
    };
    let clamped = parsed.clamp(i64::from(minimum), i64::from(maximum));
    if clamped != parsed {
        warnings.push(format!("clamping `{key}` {parsed} to {clamped}"));
    }
    clamped as u32
}

/// Builtins shipped with the desktop; mirrors
/// `packages/agent-runtime/src/subagent-definitions.ts`.
const BUILTIN_DOCUMENTS: &[&str] = &[
    r#"---
name: explorer
description: Fast codebase search and pattern matching — find files, locate implementations and answer "where is X?" / "how does Y work?". Use when answering needs a sweep over many files and you only want the conclusion.
tools: [Read, Glob, Grep, Bash]
maxTurns: 60
---

You are Explorer — a fast codebase navigation specialist.

- Prefer Grep for text/regex patterns (strings, symbols, comments), Glob for
  file discovery by name or extension, Read for specific files.
- Fire several searches in parallel when the answer needs more than one place.
- Follow definitions and call sites; do not stop at the first hit if the
  question implies more than one place.
- Quote the few lines that answer the question and cite `path:line` for each.

Report in this shape:

<files>
- src/app.ts:42 — brief description of what's there
</files>
<answer>
Concise answer to the question. If you could not find it, say what you
searched and where the trail went cold — a precise dead end is more useful
than a guess.
</answer>"#,
    r#"---
name: code-reviewer
description: Review specific code or a specific change for defects. Use for a second opinion on correctness, edge cases and missing tests before you commit.
tools: [Read, Glob, Grep]
maxTurns: 50
---

Review only what the task names, and read enough surrounding code to judge it.

- Prefer defects that change behavior: wrong results, unhandled failures,
  broken invariants, races, resource leaks, missing test coverage.
- Check the code against how its callers and neighbors actually use it, not
  against a style preference.
- Say nothing about formatting, naming or structure unless it causes a defect.

Report: each finding as `path:line` plus one sentence on what breaks and under
what input. Order by severity. If the code is sound, say so plainly and name
the cases you checked — an empty review with no evidence is not a review."#,
    r#"---
name: test-runner
description: Run a specific test or build command and report what failed and why. Use when a command's output is long and only the failures matter.
tools: [Read, Glob, Grep, Bash]
maxTurns: 40
---

Run the command the task names. Do not invent a different one, and do not fix
anything: diagnosis is the deliverable.

- Run the command once. If it fails to start (missing script, wrong directory),
  find the right invocation and say what you changed.
- For each failure, read the failing test and the code under it far enough to
  name the cause.

Report: pass/fail counts, then one entry per failure with the test name, the
assertion or error, and the `path:line` you believe is responsible. Keep the
raw output out of the report except for the lines that carry the failure."#,
    r#"---
name: fixer
description: Implement a complete multi-file change from a spec. Use when a feature or fix spans several files and the work is separable — it can write files inside the workspace while you keep working.
tools: [Read, Glob, Grep, Edit, Write, Bash]
maxTurns: 80
---

You are Fixer — a fast, focused implementation specialist. The main agent
delegates a complete, self-contained spec; implement it. Do not re-plan and do
not research beyond what the task needs.

- Read every file you will change first; never Edit or Write from memory or
  from stale content.
- Keep changes minimal and scoped to the task. Do not touch unrelated code.
- You may write inside the workspace; never write outside it. Prefer the
  workspace-relative paths the main agent gave you.
- Run the relevant validation when it is clearly applicable (test, build or
  lint command the task names); otherwise report it skipped with a reason.
- Do not delegate, do not ask the user, do not search the web. If the spec
  lacks context you truly need, use Grep/Glob/Read yourself.

Report in this shape:

<summary>
2-3 sentences: what was implemented and the outcome.
</summary>
<changes>
- path/file.ts: what changed (function or line level)
</changes>
<verification>
- Tests: [passed / failed / skipped: reason]
- Validation: [passed / failed / skipped: reason]
</verification>"#,
    r#"---
name: ui-designer
description: Design and implement a web interface from a brief — visual system, motion and complete interaction states, inspected in the browser preview or project browser tests. Use for building or restyling a UI when the visual work should run in its own context.
tools: [Read, Glob, Grep, BrowserPreview, Bash, Edit, Write]
maxTurns: 80
---

You are UI designer — a senior UI/UX designer and frontend engineer. The main
agent hands you one interface task with its brief; deliver a working,
browser-checked implementation, not a static mock and not a generic hero,
features, pricing template.

- Read the files you will touch and the project's existing design system
  first. Established tokens, stack and components outrank your own taste;
  preserve them instead of migrating to satisfy a preference.
- When the project has no UI to match, write a small design contract before
  coding: mission, semantic color/typography/spacing/radius/motion tokens on
  a 4px/8px rhythm, and the Do/Don't rules you will hold the result to.
- Build the whole interaction: semantic controls with real actions, visible
  keyboard focus, and the loading, empty, error, success, disabled and
  selected states the flow can reach. Keep grid tracks stable so long
  content reflows without overlap; never hide a layout defect behind
  overflow clipping. No TODOs, pseudo-handlers or invented backend behavior
  — label fixture data as demo data.
- Motion carries state changes, never decorates: immediate hover and press
  feedback, spring-like entrances with a small stagger for lists, and
  reduced-motion variants. Do not use `transition: all`, a generic
  `0.3s ease`, or constant-speed linear movement for stateful UI, and do
  not add an animation dependency for what one CSS transition covers.
- The brief is your confirmation; there is no user to ask mid-run. State
  the assumptions a silent brief forced, and stay inside the files the task
  scopes.
- Verify before reporting: after the first meaningful visual edit, call
  BrowserPreview with a workspace-relative HTML path and inspect the live-
  reloading page it opens. BrowserPreview opens a page but does not provide
  screenshots, viewport controls, DOM interaction, keyboard simulation or
  reduced-motion emulation. Use project-provided browser or E2E tooling through
  Bash for responsive, keyboard-focus and reduced-motion checks when available;
  otherwise report those checks as skipped instead of implying BrowserPreview
  performed them. Fix what you observe and re-check. Run the project's build or
  typecheck when it covers your change. A result you did not look at is not
  evidence.

Report in this shape:

<summary>
2-3 sentences: what was built and the design direction taken.
</summary>
<changes>
- path/file.tsx: what changed
</changes>
<verification>
- Browser: [what was opened and checked, issues fixed, issues remaining]
- Build: [passed / failed / skipped: reason]
</verification>"#,
];

fn builtin_definitions() -> Catalog {
    let mut catalog = Catalog::default();
    for raw in BUILTIN_DOCUMENTS {
        match parse_definition(
            raw,
            ParseOptions {
                source: "builtin",
                fallback_name: None,
                file_path: None,
            },
        ) {
            Ok((definition, warnings)) => {
                catalog.definitions.push(definition);
                catalog.diagnostics.extend(warnings);
            }
            Err(errors) => catalog
                .diagnostics
                .push(format!("builtin subagent invalid: {}", errors.join("; "))),
        }
    }
    catalog
}

/// Merge definitions with user documents winning by name, capped so the Task
/// catalog stays a menu the model can reason about.
fn merge(definitions: Vec<SubagentDefinition>) -> (Vec<SubagentDefinition>, Vec<String>) {
    let mut by_name: Vec<SubagentDefinition> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut dropped: Vec<String> = Vec::new();
    let ordered = definitions
        .iter()
        .filter(|definition| definition.source == "user")
        .chain(
            definitions
                .iter()
                .filter(|definition| definition.source != "user"),
        );
    for definition in ordered {
        if seen.contains(&definition.name) {
            continue;
        }
        if by_name.len() >= MAX_SUBAGENT_DEFINITIONS {
            dropped.push(definition.name.clone());
            continue;
        }
        seen.insert(definition.name.clone());
        by_name.push(definition.clone());
    }
    (by_name, dropped)
}

/// Load the catalog host-core would offer: enabled global documents shadow the
/// builtins by name. Unreadable documents degrade to diagnostics.
pub async fn load_catalog(state: &std::sync::Arc<AppState>, project_path: Option<&str>) -> Catalog {
    let builtins = builtin_definitions();
    let mut catalog = Catalog {
        definitions: builtins.definitions,
        diagnostics: builtins.diagnostics,
    };

    let active = state
        .host
        .call(
            "agents.active",
            json!({ "projectPath": project_path.map(|path| Value::String(path.to_string())).unwrap_or(Value::Null) }),
        )
        .await;
    match active {
        Ok(reply) => {
            if let Some(rows) = reply.get("subagents").and_then(|value| value.as_array()) {
                for row in rows {
                    let id = row.get("id").and_then(|value| value.as_str()).unwrap_or("");
                    let path = row
                        .get("path")
                        .and_then(|value| value.as_str())
                        .unwrap_or("");
                    if id.is_empty() {
                        continue;
                    }
                    let raw = if path.is_empty() {
                        None
                    } else {
                        crate::fsops::read_if_exists(Path::new(path))
                    };
                    let Some(raw) = raw else {
                        catalog.diagnostics.push(if path.is_empty() {
                            format!("user subagent \"{id}\": no document path")
                        } else {
                            format!("{path}: unreadable")
                        });
                        continue;
                    };
                    match parse_definition(
                        &raw,
                        ParseOptions {
                            source: "user",
                            fallback_name: Some(id),
                            file_path: if path.is_empty() { None } else { Some(path) },
                        },
                    ) {
                        Ok((definition, warnings)) => {
                            let label = if path.is_empty() { id } else { path };
                            catalog.diagnostics.extend(
                                warnings
                                    .into_iter()
                                    .map(|warning| format!("{label}: {warning}")),
                            );
                            catalog.definitions.push(definition);
                        }
                        Err(errors) => catalog
                            .diagnostics
                            .push(format!("{path}: {}", errors.join("; "))),
                    }
                }
            }
        }
        Err(error) => catalog
            .diagnostics
            .push(format!("user subagent registry unavailable: {error}")),
    }

    let (definitions, dropped) = merge(catalog.definitions);
    catalog.definitions = definitions;
    if !dropped.is_empty() {
        catalog.diagnostics.push(format!(
            "dropped subagents past the catalog cap: {}",
            dropped.join(", ")
        ));
    }
    catalog
}

/// `subagentModelKey(pin)` — the sidecar looks a delegate's pinned provider up
/// by this key, so an unresolved pin is a missing entry rather than a silent
/// fallback to the session model.
pub fn model_key(pin: &SubagentModelPin) -> String {
    format!("{}/{}", pin.provider_id, pin.model_id)
}

fn provider_alias(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn find_provider<'a>(provider_id: &str, rows: &'a [Value]) -> Option<&'a Value> {
    let alias = provider_alias(provider_id);
    if let Some(exact) = rows
        .iter()
        .find(|row| row.get("id").and_then(|value| value.as_str()) == Some(provider_id))
    {
        return Some(exact);
    }
    let vendor_matches: Vec<&Value> = rows
        .iter()
        .filter(|row| {
            provider_alias(
                row.get("vendorKey")
                    .and_then(|value| value.as_str())
                    .unwrap_or(""),
            ) == alias
        })
        .collect();
    if vendor_matches.len() == 1 {
        return Some(vendor_matches[0]);
    }
    let name_matches: Vec<&Value> = rows
        .iter()
        .filter(|row| {
            provider_alias(
                row.get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or(""),
            ) == alias
        })
        .collect();
    if name_matches.len() == 1 {
        return Some(name_matches[0]);
    }
    None
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// One pinned `provider/model` pair as a runtime provider config.
async fn resolve_provider_config(
    state: &AppState,
    provider: &Value,
    model_id: &str,
    secret: &str,
) -> Value {
    let provider_id = provider
        .get("id")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let base_url = provider
        .get("baseUrl")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let vendor_key = provider
        .get("vendorKey")
        .and_then(|value| value.as_str())
        .unwrap_or("custom");
    let mut model_config = state
        .catalog
        .find_model(vendor_key, &base_url, model_id)
        .map(|model| model.to_model_config(&base_url))
        .unwrap_or_else(|| {
            json!({
                "source": "generic",
                "name": model_id,
                "baseUrl": base_url,
                "reasoning": false,
                "modalities": { "input": ["text"], "output": ["text"] },
                "limit": { "context": 128_000, "input": 128_000, "output": 8_192 },
                "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0 },
                "input": ["text"],
                "contextWindow": 128_000,
                "maxTokens": 8_192,
                "supportedThinkingLevels": [],
            })
        });
    if let Some(value) = provider.get("contextWindow").and_then(|v| v.as_u64()) {
        model_config["contextWindow"] = json!(value);
        model_config["limit"]["context"] = json!(value);
        model_config["limit"]["input"] = json!(value);
    }
    if let Some(value) = provider.get("maxOutputTokens").and_then(|v| v.as_u64()) {
        model_config["maxTokens"] = json!(value);
        model_config["limit"]["output"] = json!(value);
    }
    let mut supported_levels = string_array(provider.get("supportedThinkingLevels"));
    if supported_levels.is_empty() {
        supported_levels = string_array(model_config.get("supportedThinkingLevels"));
    }
    let supports_reasoning = provider
        .get("supportsReasoning")
        .and_then(|v| v.as_bool())
        .or_else(|| model_config.get("reasoning").and_then(|v| v.as_bool()))
        .unwrap_or(false);

    let mut config = json!({
        "id": provider_id,
        "name": provider.get("name").and_then(|value| value.as_str()).unwrap_or(""),
        "baseUrl": base_url,
        "modelId": model_id,
        "apiKey": secret,
        "apiStyle": crate::launch::effective_api_style(provider, &model_id),
        "supportsReasoning": supports_reasoning,
        "supportedThinkingLevels": supported_levels,
        "modelConfig": model_config,
    });
    if let Some(auth_kind) = provider.get("authKind").and_then(|value| value.as_str()) {
        config["authKind"] = json!(auth_kind);
    }
    if let Some(vendor_key) = provider.get("vendorKey").and_then(|value| value.as_str()) {
        config["vendorKey"] = json!(vendor_key);
    }
    if let Some(headers) = provider.get("headers").cloned().filter(|value| {
        value
            .as_object()
            .map(|map| !map.is_empty())
            .unwrap_or(false)
    }) {
        config["headers"] = headers;
    }
    config
}

pub struct ResolvedSubagents {
    pub providers: Value,
    pub model_keys: Vec<String>,
    /// OAuth rows whose short-lived token the sidecar may refresh.
    pub oauth_provider_ids: Vec<String>,
}

/// Resolve every distinct model pin into the provider config map the sidecar
/// consumes, keyed by `provider/model`. Unresolved pins stay out of the map on
/// purpose: the runtime turns the missing entry into a named tool error instead
/// of silently spending the session model.
pub async fn resolve_pinned_providers(
    state: &std::sync::Arc<AppState>,
    definitions: &[SubagentDefinition],
    provider_rows: &[Value],
) -> (ResolvedSubagents, Vec<String>) {
    let mut providers = serde_json::Map::new();
    let mut model_keys: Vec<String> = Vec::new();
    let mut oauth_provider_ids: Vec<String> = Vec::new();
    let mut diagnostics: Vec<String> = Vec::new();
    let mut secrets: HashMap<String, Option<String>> = HashMap::new();

    // Distinct pins in definition order; the provider cap bounds live clients.
    let mut pins: Vec<(String, SubagentModelPin)> = Vec::new();
    let mut allowed_providers: Vec<String> = Vec::new();
    for definition in definitions {
        let Some(pin) = &definition.model else {
            continue;
        };
        if !allowed_providers.contains(&pin.provider_id) {
            if allowed_providers.len() >= MAX_SUBAGENT_PROVIDERS {
                diagnostics.push(format!(
                    "{}: too many pinned providers, ignoring \"{}\"",
                    definition.name,
                    model_key(pin)
                ));
                continue;
            }
            allowed_providers.push(pin.provider_id.clone());
        }
        let key = model_key(pin);
        if !model_keys.contains(&key) {
            model_keys.push(key.clone());
        }
        if !pins.iter().any(|(existing, _)| existing == &key) {
            pins.push((key, pin.clone()));
        }
    }

    for (key, pin) in pins {
        let Some(provider) = find_provider(&pin.provider_id, provider_rows) else {
            diagnostics.push(format!(
                "no enabled provider matches \"{}\"",
                pin.provider_id
            ));
            continue;
        };
        let provider_id = provider
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        let auth_kind = provider
            .get("authKind")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let is_vendor = auth_kind == OAUTH_AUTH_KIND;
        let secret = if is_vendor {
            match crate::oauth::resolve_access_token(state, &provider_id).await {
                Ok(token) => token,
                Err(message) => {
                    diagnostics.push(format!("provider \"{}\": {message}", key));
                    continue;
                }
            }
        } else if auth_kind == "none" {
            String::new()
        } else {
            if !secrets.contains_key(&provider_id) {
                let secret = state
                    .host
                    .call("providers.getSecret", json!({ "id": provider_id }))
                    .await
                    .ok()
                    .and_then(|reply| {
                        reply
                            .get("value")
                            .and_then(|value| value.as_str())
                            .map(str::to_string)
                    });
                secrets.insert(provider_id.clone(), secret);
            }
            match secrets.get(&provider_id).cloned().flatten() {
                Some(secret) if !secret.is_empty() => secret,
                _ => {
                    diagnostics.push(format!(
                        "provider \"{}\" has no API key",
                        provider
                            .get("name")
                            .and_then(|value| value.as_str())
                            .unwrap_or(&key)
                    ));
                    continue;
                }
            }
        };
        if is_vendor && !oauth_provider_ids.contains(&provider_id) {
            oauth_provider_ids.push(provider_id.clone());
        }
        providers.insert(
            key,
            resolve_provider_config(state, provider, &pin.model_id, &secret).await,
        );
    }

    (
        ResolvedSubagents {
            providers: Value::Object(providers),
            model_keys,
            oauth_provider_ids,
        },
        diagnostics,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_builtin_document() {
        let catalog = builtin_definitions();
        assert_eq!(catalog.definitions.len(), 5);
        assert!(catalog.diagnostics.is_empty());
        let explorer = catalog
            .definitions
            .iter()
            .find(|definition| definition.name == "explorer")
            .expect("explorer");
        assert_eq!(explorer.tools, vec!["Read", "Glob", "Grep", "Bash"]);
        assert_eq!(explorer.max_turns, Some(60));
        assert!(explorer.prompt.starts_with("You are Explorer"));
    }

    #[test]
    fn user_documents_shadow_builtins_and_the_cap_drops_extras() {
        let mut definitions = builtin_definitions().definitions;
        let shadow = SubagentDefinition {
            name: "explorer".into(),
            description: "custom explorer".into(),
            tools: vec!["Read".into()],
            inherit_tools: false,
            model: None,
            thinking_level: None,
            permission: None,
            max_turns: None,
            max_tokens: None,
            idle_timeout_seconds: DEFAULT_IDLE_TIMEOUT_SECONDS,
            max_duration_seconds: DEFAULT_MAX_DURATION_SECONDS,
            prompt: "custom".into(),
            source: "user".into(),
            file_path: None,
        };
        definitions.push(shadow);
        for index in 0..20 {
            definitions.push(SubagentDefinition {
                name: format!("extra-{index}"),
                description: "extra".into(),
                tools: vec!["Read".into()],
                inherit_tools: false,
                model: None,
                thinking_level: None,
                permission: None,
                max_turns: None,
                max_tokens: None,
                idle_timeout_seconds: DEFAULT_IDLE_TIMEOUT_SECONDS,
                max_duration_seconds: DEFAULT_MAX_DURATION_SECONDS,
                prompt: "extra".into(),
                source: "user".into(),
                file_path: None,
            });
        }
        let (merged, dropped) = merge(definitions);
        assert_eq!(merged.len(), MAX_SUBAGENT_DEFINITIONS);
        assert_eq!(
            merged
                .iter()
                .find(|definition| definition.name == "explorer")
                .map(|definition| definition.description.as_str()),
            Some("custom explorer")
        );
        assert!(!dropped.is_empty());
    }

    #[test]
    fn inherit_and_explicit_extras_are_kept() {
        let raw = "---\nname: helper\ndescription: helps\ntools: [inherit, Bash]\n---\n\nbody";
        let (definition, warnings) = parse_definition(
            raw,
            ParseOptions {
                source: "user",
                fallback_name: None,
                file_path: None,
            },
        )
        .expect("parse");
        assert!(definition.inherit_tools);
        assert_eq!(definition.tools, vec!["Bash"]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn a_missing_description_is_a_parse_error() {
        let raw = "---\nname: helper\n---\n\nbody";
        assert!(parse_definition(
            raw,
            ParseOptions {
                source: "user",
                fallback_name: None,
                file_path: None
            }
        )
        .is_err());
    }

    #[test]
    fn a_model_pin_splits_on_the_first_slash() {
        let raw = "---\nname: helper\ndescription: helps\nmodel: openrouter/deepseek/deepseek-chat\n---\n\nbody";
        let (definition, _) = parse_definition(
            raw,
            ParseOptions {
                source: "user",
                fallback_name: None,
                file_path: None,
            },
        )
        .expect("parse");
        let pin = definition.model.expect("pin");
        assert_eq!(pin.provider_id, "openrouter");
        assert_eq!(pin.model_id, "deepseek/deepseek-chat");
    }

    #[test]
    fn the_name_normalizer_matches_the_shared_contract() {
        assert_eq!(normalize_name("My Agent.md"), "my-agent");
        assert_eq!(normalize_name("path/to/Agent_Name.MD"), "agent-name");
    }
}
