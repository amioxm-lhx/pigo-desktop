//! Desktop-only MCP / Skill market channels.
//!
//! Both markets are read-only HTTP: every source URL must be a public HTTPS
//! address, redirects are revalidated hop by hop, and responses are bounded.
//! One failing source never discards the others; the UI receives its id in
//! `failedSources` and keeps the successful entries.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Map, Value};
use url::Url;

use crate::state::AppState;

const MAX_SOURCES: usize = 16;
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_REDIRECTS: usize = 3;
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_SKILL_ENTRIES: usize = 400;
const MAX_MARKET_PAGES: usize = 2000;
const MAX_SKILL_RESOURCES: usize = 16;
const MAX_SKILL_RESOURCE_BYTES: usize = 64 * 1024;

const SKILL_SOURCE_TTL: Duration = Duration::from_secs(600);
const MAX_CACHED_SOURCES: usize = 128;

struct CachedSource {
    at: std::time::Instant,
    entries: Vec<Value>,
}

/// Cursor/page state for MCP browse pagination plus a short-lived skill-source
/// cache: a GitHub scan costs two API calls, and the search box is debounced
/// per keystroke.
pub struct MarketRuntime {
    cursors: Mutex<HashMap<String, String>>,
    pages: Mutex<HashMap<String, Vec<Value>>>,
    skill_sources: Mutex<HashMap<String, CachedSource>>,
}

pub fn new_runtime() -> Arc<MarketRuntime> {
    Arc::new(MarketRuntime {
        cursors: Mutex::new(HashMap::new()),
        pages: Mutex::new(HashMap::new()),
        skill_sources: Mutex::new(HashMap::new()),
    })
}

// ── MCP market ──────────────────────────────────────────────────────────────

pub async fn mcp_search(state: &Arc<AppState>, params: &Value) -> Value {
    let query = params
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let more = params.get("more").and_then(Value::as_bool).unwrap_or(false);
    let sources: Vec<Value> = params
        .get("sources")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(MAX_SOURCES)
        .collect();
    if !more {
        if let Ok(mut pages) = state.market.pages.lock() {
            pages.clear();
        }
        if let Ok(mut cursors) = state.market.cursors.lock() {
            cursors.clear();
        }
    }
    let cursor_snapshot: HashMap<String, String> = state
        .market
        .cursors
        .lock()
        .map(|cursors| cursors.clone())
        .unwrap_or_default();

    let mut failed: Vec<String> = Vec::new();
    let mut any_cursor = false;
    for raw in &sources {
        let Some(source) = valid_market_source(raw) else {
            continue;
        };
        let id = source.id.clone();
        let cursor = cursor_snapshot.get(&id).cloned();
        let search = source_search(&source, &query, cursor);
        let result = tokio::task::spawn_blocking(search)
            .await
            .unwrap_or_else(|error| Err(error.to_string()));
        match result {
            Ok((entries, next_cursor)) => {
                if let Ok(mut pages) = state.market.pages.lock() {
                    let bucket = pages.entry(id.clone()).or_default();
                    for entry in entries {
                        if bucket.len() >= MAX_MARKET_PAGES {
                            break;
                        }
                        bucket.push(entry);
                    }
                }
                match next_cursor {
                    Some(cursor) => {
                        any_cursor = true;
                        if let Ok(mut cursors) = state.market.cursors.lock() {
                            cursors.insert(id, cursor);
                        }
                    }
                    None => {
                        if let Ok(mut cursors) = state.market.cursors.lock() {
                            cursors.remove(&id);
                        }
                    }
                }
            }
            Err(error) => {
                log::warn!("MCP market source {id} failed: {error}");
                failed.push(id);
            }
        }
    }

    let mut entries: Vec<Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Ok(pages) = state.market.pages.lock() {
        for source in &sources {
            let Some(source) = valid_market_source(source) else {
                continue;
            };
            let Some(bucket) = pages.get(&source.id) else {
                continue;
            };
            for entry in bucket {
                let id = entry.get("id").and_then(Value::as_str).unwrap_or("");
                if !seen.insert(format!("{}:{id}", source.id)) {
                    continue;
                }
                entries.push(entry.clone());
            }
        }
    }
    json!({
        "entries": entries,
        "failedSources": failed,
        "exhausted": !any_cursor,
    })
}

struct MarketSource {
    id: String,
    url: String,
    kind: String,
}

fn valid_market_source(raw: &Value) -> Option<MarketSource> {
    let id = raw.get("id").and_then(Value::as_str).unwrap_or("");
    if !valid_source_id(id) {
        return None;
    }
    let url = raw.get("url").and_then(Value::as_str).unwrap_or("");
    let kind = raw
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("registry");
    if kind != "registry" && kind != "catalog" {
        return None;
    }
    let _ = safe_https(url)?;
    Some(MarketSource {
        id: id.to_string(),
        url: url.to_string(),
        kind: kind.to_string(),
    })
}

fn valid_source_id(id: &str) -> bool {
    let mut chars = id.chars();
    let starts = chars
        .next()
        .map(|c| c.is_ascii_lowercase())
        .unwrap_or(false);
    starts
        && id.len() <= 64
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn source_search(
    source: &MarketSource,
    query: &str,
    cursor: Option<String>,
) -> impl FnOnce() -> Result<(Vec<Value>, Option<String>), String> {
    let id = source.id.clone();
    let url = source.url.clone();
    let query = query.to_string();
    let kind = source.kind.clone();
    move || {
        if kind == "catalog" {
            let text = fetch_text(&url)?;
            let payload: Value =
                serde_json::from_str(&text).map_err(|error| format!("{}: {error}", id))?;
            let entries = payload
                .get("servers")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let mut out = Vec::new();
            for entry in entries {
                let mut entry = entry;
                if !valid_catalog_entry(&entry) || !matches_query(&entry, &query, true) {
                    continue;
                }
                entry["sourceId"] = json!(id);
                out.push(entry);
                if out.len() >= MAX_MARKET_PAGES {
                    break;
                }
            }
            return Ok((out, None));
        }
        let mut request = Url::parse(&url).map_err(|error| error.to_string())?;
        {
            let mut pairs = request.query_pairs_mut();
            pairs.append_pair("limit", "100");
            if !query.is_empty() {
                pairs.append_pair("search", &query);
            }
            if let Some(cursor) = &cursor {
                pairs.append_pair("cursor", cursor);
            }
        }
        let text = fetch_text(request.as_str())?;
        let payload: Value =
            serde_json::from_str(&text).map_err(|error| format!("{}: {error}", id))?;
        let mut out = Vec::new();
        for record in payload
            .get("servers")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let Some(mut entry) = map_registry_record(&record) else {
                continue;
            };
            if !matches_query(&entry, &query, false) {
                continue;
            }
            entry["sourceId"] = json!(id);
            out.push(entry);
            if out.len() >= MAX_MARKET_PAGES {
                break;
            }
        }
        let next = payload
            .pointer("/metadata/nextCursor")
            .and_then(Value::as_str)
            .filter(|cursor| !cursor.is_empty())
            .map(str::to_string);
        Ok((out, next))
    }
}

fn matches_query(entry: &Value, query: &str, client_side: bool) -> bool {
    if query.is_empty() || !client_side {
        return true;
    }
    let needle = query.to_lowercase();
    ["name", "description", "author"]
        .iter()
        .filter_map(|key| entry.get(*key).and_then(Value::as_str))
        .any(|text| text.to_lowercase().contains(&needle))
}

// ── Skill market ────────────────────────────────────────────────────────────

pub async fn skill_search(state: &Arc<AppState>, params: &Value) -> Value {
    let sources: Vec<Value> = params
        .get("sources")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(MAX_SOURCES)
        .collect();
    let mut failed: Vec<String> = Vec::new();
    let mut entries: Vec<Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for raw in &sources {
        let Some(id) = raw.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(url) = raw.get("url").and_then(Value::as_str) else {
            continue;
        };
        if !valid_source_id(id) || safe_https(url).is_none() {
            continue;
        }
        let url = url.to_string();
        let cached = state.market.skill_sources.lock().ok().and_then(|cache| {
            cache
                .get(&url)
                .filter(|cached| cached.at.elapsed() < SKILL_SOURCE_TTL)
                .map(|cached| cached.entries.clone())
        });
        let result = match cached {
            Some(entries) => Ok(entries),
            None => {
                let fetch_url = url.clone();
                let result = tokio::task::spawn_blocking(move || skill_source_entries(&fetch_url))
                    .await
                    .unwrap_or_else(|error| Err(error.to_string()));
                if let Ok(entries) = &result {
                    if let Ok(mut cache) = state.market.skill_sources.lock() {
                        if cache.len() >= MAX_CACHED_SOURCES {
                            cache.clear();
                        }
                        cache.insert(
                            url,
                            CachedSource {
                                at: std::time::Instant::now(),
                                entries: entries.clone(),
                            },
                        );
                    }
                }
                result
            }
        };
        match result {
            Ok(source_entries) => {
                for mut entry in source_entries {
                    // The panel maps `sourceId` back to the configured id when
                    // it renders "via <source name>".
                    entry["sourceId"] = json!(id);
                    let entry_id = entry.get("id").and_then(Value::as_str).unwrap_or("");
                    if entry_id.is_empty() || !seen.insert(entry_id.to_string()) {
                        continue;
                    }
                    entries.push(entry);
                    if entries.len() >= MAX_SKILL_ENTRIES {
                        break;
                    }
                }
            }
            Err(error) => {
                log::warn!("skill market source {id} failed: {error}");
                failed.push(id.to_string());
            }
        }
    }
    json!({ "entries": entries, "failedSources": failed })
}

fn skill_source_entries(url: &str) -> Result<Vec<Value>, String> {
    let parsed = safe_https(url).ok_or_else(|| "url must be public https".to_string())?;
    if parsed.host_str() == Some("github.com") {
        return github_skill_scan(&parsed);
    }
    let text = fetch_text(url)?;
    let payload: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let mut out = Vec::new();
    for raw in payload
        .get("skills")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let id = raw.get("id").and_then(Value::as_str).unwrap_or("");
        let id = skill_id(id, &format!("skill-{}", out.len()));
        let Some(url) = raw.get("url").and_then(Value::as_str) else {
            continue;
        };
        if safe_https(url).is_none() {
            continue;
        }
        let name = raw
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(&id);
        let mut entry = json!({
            "id": id,
            "name": name,
            "url": url,
        });
        for field in [
            "description",
            "author",
            "homepage",
            "version",
            "notes",
            "categories",
            "verified",
        ] {
            if let Some(value) = raw.get(field) {
                entry[field] = value.clone();
            }
        }
        out.push(entry);
        if out.len() >= MAX_SKILL_ENTRIES {
            break;
        }
    }
    Ok(out)
}

fn github_skill_scan(parsed: &Url) -> Result<Vec<Value>, String> {
    let segments: Vec<&str> = parsed
        .path_segments()
        .map(|segments| segments.filter(|part| !part.is_empty()).collect())
        .unwrap_or_default();
    if segments.len() < 2 {
        return Err("github source must name a repository".into());
    }
    let owner = segments[0];
    let repo = segments[1].trim_end_matches(".git");
    if !safe_token(owner) || !safe_token(repo) {
        return Err("github owner or repository is not a safe name".into());
    }
    let repo_api = format!("https://api.github.com/repos/{owner}/{repo}");
    let meta: Value =
        serde_json::from_str(&fetch_text(&repo_api)?).map_err(|error| error.to_string())?;
    let branch = meta
        .get("default_branch")
        .and_then(Value::as_str)
        .unwrap_or("main");
    let tree_url =
        format!("https://api.github.com/repos/{owner}/{repo}/git/trees/{branch}?recursive=1");
    let tree: Value =
        serde_json::from_str(&fetch_text(&tree_url)?).map_err(|error| error.to_string())?;
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in tree
        .get("tree")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let Some(path) = item.get("path").and_then(Value::as_str) else {
            continue;
        };
        let lower = path.to_ascii_lowercase();
        if !lower.ends_with("/skill.md") && lower != "skill.md" {
            continue;
        }
        let directory = path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
        let directory_name = directory.rsplit('/').next().unwrap_or("skill");
        let id = skill_id(directory_name, "skill");
        if !seen.insert(id.clone()) {
            continue;
        }
        let display = directory_name.replace(['-', '_'], " ");
        let homepage = if directory.is_empty() {
            format!("https://github.com/{owner}/{repo}")
        } else {
            format!("https://github.com/{owner}/{repo}/tree/{branch}/{directory}")
        };
        out.push(json!({
            "id": id,
            "name": display,
            "url": format!("https://cdn.jsdelivr.net/gh/{owner}/{repo}@{branch}/{path}"),
            "author": owner,
            "homepage": homepage,
        }));
        if out.len() >= MAX_SKILL_ENTRIES {
            break;
        }
    }
    Ok(out)
}

/// `skill/market/fetch`: the document plus its sibling Markdown resources.
pub async fn skill_fetch(params: &Value) -> Result<Value, String> {
    let entry = params
        .get("entry")
        .cloned()
        .ok_or_else(|| "entry required".to_string())?;
    let url = entry
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| "entry url required".to_string())?
        .to_string();
    if safe_https(&url).is_none() {
        return Err("entry url must be public https".into());
    }
    let document = tokio::task::spawn_blocking(move || fetch_skill_document(&url))
        .await
        .map_err(|error| error.to_string())??;
    Ok(document)
}

fn fetch_skill_document(url: &str) -> Result<Value, String> {
    let text = fetch_text(url)?;
    let (name, description, body) = split_frontmatter(&text);
    let mut document = json!({ "body": body });
    if let Some(name) = name {
        document["name"] = json!(name);
    }
    if let Some(description) = description {
        document["description"] = json!(description);
    }
    let resources = sibling_resources(url)?;
    if !resources.is_empty() {
        document["resources"] = json!(resources);
    }
    Ok(document)
}

fn split_frontmatter(text: &str) -> (Option<String>, Option<String>, String) {
    let trimmed = text.trim_start_matches('\u{feff}');
    let Some(rest) = trimmed.strip_prefix("---") else {
        return (None, None, trimmed.to_string());
    };
    let Some(end) = rest.find("\n---") else {
        return (None, None, trimmed.to_string());
    };
    let front = &rest[..end];
    let body_start = end + 4;
    let body = rest[body_start..].trim_start_matches('\n').to_string();
    let mut name = None;
    let mut description = None;
    for line in front.lines() {
        let Some((key, value)) = line.trim().split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'').trim();
        match key.trim() {
            "name" if !value.is_empty() => name = Some(value.to_string()),
            "description" if !value.is_empty() => description = Some(value.to_string()),
            _ => {}
        }
    }
    (name, description, body)
}

fn sibling_resources(url: &str) -> Result<Vec<Value>, String> {
    let Some(parsed) = Url::parse(url).ok() else {
        return Ok(Vec::new());
    };
    if parsed.host_str() != Some("cdn.jsdelivr.net") {
        return Ok(Vec::new());
    }
    let segments: Vec<&str> = parsed
        .path_segments()
        .map(|segments| segments.filter(|part| !part.is_empty()).collect())
        .unwrap_or_default();
    // /gh/<owner>/<repo>@<ref>/<path...>
    if segments.len() < 4 || segments[0] != "gh" {
        return Ok(Vec::new());
    }
    let owner = segments[1];
    let repo_ref = segments[2];
    let (repo, reference) = match repo_ref.split_once('@') {
        Some(parts) => parts,
        None => return Ok(Vec::new()),
    };
    let file_path = segments[3..].join("/");
    let directory = file_path
        .rsplit_once('/')
        .map(|(dir, _)| dir.to_string())
        .unwrap_or_default();
    let listing_url = format!(
        "https://data.jsdelivr.com/v1/packages/gh/{owner}/{repo}@{reference}?structure=flat"
    );
    let listing: Value =
        serde_json::from_str(&fetch_text(&listing_url)?).map_err(|error| error.to_string())?;
    let mut out = Vec::new();
    for item in listing
        .get("files")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            continue;
        };
        let name = name.trim_start_matches('/');
        let (file_dir, file_name) = match name.rsplit_once('/') {
            Some(parts) => parts,
            None => ("", name),
        };
        if file_dir != directory
            || !file_name.to_ascii_lowercase().ends_with(".md")
            || file_name.eq_ignore_ascii_case("SKILL.md")
        {
            continue;
        }
        let body = fetch_text(&format!(
            "https://cdn.jsdelivr.net/gh/{owner}/{repo}@{reference}/{name}"
        ))?;
        if body.len() > MAX_SKILL_RESOURCE_BYTES {
            continue;
        }
        out.push(json!({ "path": file_name, "body": body }));
        if out.len() >= MAX_SKILL_RESOURCES {
            break;
        }
    }
    Ok(out)
}

// ── shared HTTP policy ──────────────────────────────────────────────────────

fn safe_https(url: &str) -> Option<Url> {
    let parsed = Url::parse(url).ok()?;
    if parsed.scheme() != "https" || parsed.password().is_some() {
        return None;
    }
    let host = parsed.host_str()?;
    crate::git_clone::host_is_public(host).then_some(parsed)
}

/// Public HTTPS only, bounded redirects revalidated hop by hop, bounded body.
fn fetch_text(url: &str) -> Result<String, String> {
    let mut current = safe_https(url).ok_or_else(|| "url must be public https".to_string())?;
    for _ in 0..=MAX_REDIRECTS {
        let agent = ureq::AgentBuilder::new()
            .timeout(HTTP_TIMEOUT)
            .redirects(0)
            .build();
        let response = match agent.get(current.as_str()).call() {
            Ok(response) => response,
            Err(ureq::Error::Status(status, response)) if (300..400).contains(&status) => {
                current = redirect_target(&response, &current)?;
                continue;
            }
            Err(ureq::Error::Status(status, _)) => {
                return Err(format!("HTTP {status}"));
            }
            Err(error) => return Err(error.to_string()),
        };
        if (300..400).contains(&response.status()) {
            current = redirect_target(&response, &current)?;
            continue;
        }
        if let Some(length) = response
            .header("content-length")
            .and_then(|value| value.parse::<usize>().ok())
        {
            if length > MAX_RESPONSE_BYTES {
                return Err("response is too large".into());
            }
        }
        let text = response.into_string().map_err(|error| error.to_string())?;
        if text.len() > MAX_RESPONSE_BYTES {
            return Err("response is too large".into());
        }
        return Ok(text);
    }
    Err("too many redirects".into())
}

fn redirect_target(response: &ureq::Response, base: &Url) -> Result<Url, String> {
    let Some(location) = response.header("location") else {
        return Err("redirect has no location".into());
    };
    let next = base.join(location).map_err(|error| error.to_string())?;
    safe_https(next.as_str()).ok_or_else(|| "redirect left public https".to_string())
}

// ── registry mapping (port of shared/mcp-registry.ts) ───────────────────────

fn map_registry_record(record: &Value) -> Option<Value> {
    let server = record.get("server")?;
    let name = server.get("name").and_then(Value::as_str)?.trim();
    if name.is_empty() {
        return None;
    }
    let id = registry_id(name);
    let display = server
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .or_else(|| name.rsplit('/').next().map(str::to_string))
        .unwrap_or_else(|| name.to_string());
    let homepage = server
        .get("homepage")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            server
                .get("repository")
                .and_then(|repository| repository.get("url"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| {
            server
                .get("repository")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let description = server
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let category = guess_category(server);

    let packages = server.get("packages").and_then(Value::as_array);
    let npm = packages.and_then(|packages| {
        packages.iter().find(|package| {
            package
                .get("registryType")
                .and_then(Value::as_str)
                .map(|kind| kind.eq_ignore_ascii_case("npm"))
                .unwrap_or(false)
                && package
                    .get("identifier")
                    .and_then(Value::as_str)
                    .map(|identifier| !identifier.trim().is_empty())
                    .unwrap_or(false)
        })
    });
    if let Some(package) = npm {
        let mut args = argument_values(package.get("runtimeArguments"));
        let package_args = argument_values(package.get("packageArguments"));
        if let Some(specifier) = package_specifier(package, "@") {
            if !args.contains(&specifier) && !package_args.contains(&specifier) {
                args.push(specifier);
            }
        }
        args.extend(package_args);
        let command = package
            .get("runtimeHint")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|hint| !hint.is_empty())
            .unwrap_or("npx");
        let mut entry = json!({
            "id": id,
            "name": display,
            "categories": [category],
            "transport": "stdio",
            "command": command,
            "args": args,
        });
        decorate(&mut entry, description, homepage.as_deref());
        if let Some(env) = env_templates(package) {
            entry["env"] = json!(env);
        }
        if let Some(specs) = env_specs(package) {
            entry["requiredEnv"] = json!(specs);
        }
        return valid_catalog_entry(&entry).then_some(entry);
    }

    let pypi = packages.and_then(|packages| {
        packages.iter().find(|package| {
            package
                .get("registryType")
                .and_then(Value::as_str)
                .map(|kind| kind.eq_ignore_ascii_case("pypi"))
                .unwrap_or(false)
                && package
                    .get("identifier")
                    .and_then(Value::as_str)
                    .map(|identifier| !identifier.trim().is_empty())
                    .unwrap_or(false)
        })
    });
    if let Some(package) = pypi {
        let mut args = argument_values(package.get("runtimeArguments"));
        let package_args = argument_values(package.get("packageArguments"));
        if let Some(specifier) = package_specifier(package, "==") {
            if !args.contains(&specifier) && !package_args.contains(&specifier) {
                args.push(specifier);
            }
        }
        args.extend(package_args);
        let command = package
            .get("runtimeHint")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|hint| !hint.is_empty())
            .unwrap_or("uvx");
        let mut entry = json!({
            "id": id,
            "name": display,
            "categories": [category],
            "transport": "stdio",
            "command": command,
            "args": args,
            "prerequisites": ["Requires uv/uvx on PATH"],
        });
        decorate(&mut entry, description, homepage.as_deref());
        if let Some(env) = env_templates(package) {
            entry["env"] = json!(env);
        }
        if let Some(specs) = env_specs(package) {
            entry["requiredEnv"] = json!(specs);
        }
        return valid_catalog_entry(&entry).then_some(entry);
    }

    let remote = server
        .get("remotes")
        .and_then(Value::as_array)
        .and_then(|remotes| {
            remotes.iter().find(|remote| {
                remote
                    .get("type")
                    .and_then(Value::as_str)
                    .map(|kind| kind.eq_ignore_ascii_case("streamable-http"))
                    .unwrap_or(false)
                    && remote
                        .get("url")
                        .and_then(Value::as_str)
                        .and_then(safe_https)
                        .is_some()
            })
        })?;
    let url = remote.get("url").and_then(Value::as_str)?.to_string();
    let mut headers = Map::new();
    for header in remote
        .get("headers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        if let (Some(name), Some(value)) = (
            header.get("name").and_then(Value::as_str),
            header.get("value").and_then(Value::as_str),
        ) {
            headers.insert(name.to_string(), json!(value));
        }
    }
    let mut required: Vec<Value> = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    for value in headers.values() {
        let text = value.as_str().unwrap_or("");
        for name in placeholders(text) {
            names.insert(name);
        }
    }
    for name in names {
        required.push(json!({ "name": name }));
    }
    let mut entry = json!({
        "id": id,
        "name": display,
        "categories": [category],
        "transport": "http",
        "url": url,
    });
    decorate(&mut entry, description, homepage.as_deref());
    if !headers.is_empty() {
        entry["headers"] = Value::Object(headers);
    }
    if !required.is_empty() {
        entry["requiredEnv"] = json!(required);
    }
    valid_catalog_entry(&entry).then_some(entry)
}

fn decorate(entry: &mut Value, description: Option<&str>, homepage: Option<&str>) {
    if let Some(description) = description {
        entry["description"] = json!(description);
    }
    if let Some(homepage) = homepage {
        entry["homepage"] = json!(homepage);
    }
}

fn registry_id(name: &str) -> String {
    let slug = name
        .to_lowercase()
        .split('/')
        .map(|part| {
            part.chars()
                .map(|c| {
                    if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' {
                        c
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
                .trim_matches('-')
                .to_string()
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .chars()
        .take(60)
        .collect::<String>();
    if slug.is_empty() {
        return "mcp-server".into();
    }
    if slug.starts_with(|c: char| c.is_ascii_lowercase()) {
        slug
    } else {
        format!("mcp-{slug}").chars().take(64).collect()
    }
}

fn argument_values(args: Option<&Value>) -> Vec<String> {
    let mut out = Vec::new();
    for argument in args
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let named = argument.get("type").and_then(Value::as_str) == Some("named")
            || (argument.get("name").and_then(Value::as_str).is_some()
                && argument.get("type").and_then(Value::as_str) != Some("positional"));
        if named {
            if let Some(name) = argument.get("name").and_then(Value::as_str) {
                if !name.is_empty() {
                    out.push(name.to_string());
                }
            }
            if let Some(value) = argument.get("value").and_then(Value::as_str) {
                if !value.is_empty() {
                    out.push(value.to_string());
                }
            }
        } else if let Some(value) = argument.get("value").and_then(Value::as_str) {
            if !value.is_empty() {
                out.push(value.to_string());
            }
        }
    }
    out
}

fn package_specifier(package: &Value, separator: &str) -> Option<String> {
    let identifier = package.get("identifier").and_then(Value::as_str)?.trim();
    if identifier.is_empty() {
        return None;
    }
    let version = package
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if version.is_empty() {
        Some(identifier.to_string())
    } else {
        Some(format!("{identifier}{separator}{version}"))
    }
}

fn env_templates(package: &Value) -> Option<Map<String, Value>> {
    let variables = package
        .get("environmentVariables")
        .and_then(Value::as_array)?;
    let mut env = Map::new();
    for variable in variables {
        if let Some(name) = variable.get("name").and_then(Value::as_str) {
            if !name.is_empty() {
                env.insert(name.to_string(), json!(format!("${{{name}}}")));
            }
        }
    }
    (!env.is_empty()).then_some(env)
}

fn env_specs(package: &Value) -> Option<Vec<Value>> {
    let variables = package
        .get("environmentVariables")
        .and_then(Value::as_array)?;
    let mut out = Vec::new();
    for variable in variables {
        let Some(name) = variable.get("name").and_then(Value::as_str) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let mut spec = json!({ "name": name });
        if let Some(description) = variable
            .get("description")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            spec["description"] = json!(description);
        }
        if variable.get("isRequired").and_then(Value::as_bool) != Some(true) {
            spec["optional"] = json!(true);
        }
        if let Some(default) = variable
            .get("value")
            .or_else(|| variable.get("default"))
            .and_then(Value::as_str)
        {
            spec["defaultValue"] = json!(default);
        }
        out.push(spec);
    }
    (!out.is_empty()).then_some(out)
}

const CATEGORY_KEYWORDS: &[(&str, &[&str])] = &[
    (
        "data",
        &[
            "database",
            "sql",
            "postgres",
            "mysql",
            "mongo",
            "redis",
            "sqlite",
            "dataset",
            "warehouse",
            "analytics",
            "supabase",
            "snowflake",
        ],
    ),
    (
        "productivity",
        &[
            "todo", "task", "calendar", "email", "mail", "remind", "schedule", "slack", "notion",
            "jira", "linear", "asana", "habit", "time",
        ],
    ),
    (
        "web",
        &[
            "search",
            "scrape",
            "crawl",
            "browser",
            "fetch",
            "playwright",
            "puppeteer",
            "seo",
            "web",
            "surf",
        ],
    ),
    (
        "devtools",
        &[
            "github",
            "gitlab",
            "git ",
            "docker",
            "kubernetes",
            "k8s",
            "deploy",
            "terminal",
            "shell",
            "code",
            "repo",
            "issue",
            "build",
            "lint",
            "test",
            "ci ",
            "ide",
            "api",
            "sentry",
            " observability",
        ],
    ),
    (
        "docs",
        &[
            "doc",
            "wiki",
            "knowledge",
            "context",
            "reference",
            "manual",
            "library",
            "framework",
            "changelog",
            "arxiv",
            "paper",
        ],
    ),
];

fn guess_category(server: &Value) -> &'static str {
    let haystack = format!(
        "{} {} {}",
        server.get("name").and_then(Value::as_str).unwrap_or(""),
        server.get("title").and_then(Value::as_str).unwrap_or(""),
        server
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or(""),
    )
    .to_lowercase();
    for (category, keywords) in CATEGORY_KEYWORDS {
        if keywords.iter().any(|keyword| haystack.contains(*keyword)) {
            return *category;
        }
    }
    "devtools"
}

fn placeholders(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            break;
        };
        let name = &after[..end];
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        {
            names.push(name.to_string());
        }
        rest = &after[end + 1..];
    }
    names
}

fn valid_catalog_entry(entry: &Value) -> bool {
    let id = entry.get("id").and_then(Value::as_str).unwrap_or("");
    let valid_id = {
        let mut chars = id.chars();
        chars
            .next()
            .map(|c| c.is_ascii_lowercase())
            .unwrap_or(false)
            && id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
            && id.len() <= 64
    };
    if !valid_id {
        return false;
    }
    if entry
        .get("name")
        .and_then(Value::as_str)
        .map(|name| name.trim().is_empty())
        .unwrap_or(true)
    {
        return false;
    }
    match entry.get("transport").and_then(Value::as_str) {
        Some("stdio") => entry
            .get("command")
            .and_then(Value::as_str)
            .map(|command| !command.trim().is_empty() && !command.contains(".."))
            .unwrap_or(false),
        Some("http") => entry
            .get("url")
            .and_then(Value::as_str)
            .map(|url| safe_https(url).is_some())
            .unwrap_or(false),
        _ => false,
    }
}

// ── skill ids ───────────────────────────────────────────────────────────────

fn skill_id(raw: &str, fallback: &str) -> String {
    let slug = sanitize_slug(raw);
    if valid_skill_id(&slug) {
        return slug;
    }
    let prefixed = sanitize_slug(&format!("skill-{slug}"));
    if valid_skill_id(&prefixed) {
        return prefixed;
    }
    let fallback = sanitize_slug(fallback);
    if valid_skill_id(&fallback) {
        fallback
    } else {
        "skill".to_string()
    }
}

fn sanitize_slug(raw: &str) -> String {
    let slug: String = raw
        .to_lowercase()
        .replace('_', "-")
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    slug.chars()
        .take(64)
        .collect::<String>()
        .trim_end_matches('-')
        .to_string()
}

fn valid_skill_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars
        .next()
        .map(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .unwrap_or(false)
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && id.len() <= 64
}

fn safe_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::{map_registry_record, skill_id, split_frontmatter, valid_catalog_entry};
    use serde_json::json;

    #[test]
    fn registry_npm_records_become_stdio_templates() {
        let entry = map_registry_record(&json!({ "server": {
            "name": "com.example/weather",
            "title": "Weather",
            "description": "Forecasts",
            "packages": [{
                "registryType": "npm",
                "identifier": "@example/weather-mcp",
                "version": "1.2.0",
                "environmentVariables": [{ "name": "API_KEY", "isRequired": true }]
            }]
        }}))
        .unwrap();
        assert_eq!(entry["id"], "com-example-weather");
        assert_eq!(entry["transport"], "stdio");
        assert_eq!(entry["command"], "npx");
        assert_eq!(entry["args"][0], "@example/weather-mcp@1.2.0");
        assert_eq!(entry["env"]["API_KEY"], "${API_KEY}");
        assert_eq!(entry["requiredEnv"][0]["name"], "API_KEY");
    }

    #[test]
    fn registry_remotes_require_public_https() {
        let entry = map_registry_record(&json!({ "server": {
            "name": "com.example/web",
            "remotes": [{ "type": "streamable-http", "url": "https://example.com/mcp" }]
        }}))
        .unwrap();
        assert_eq!(entry["transport"], "http");
        let private = map_registry_record(&json!({ "server": {
            "name": "com.example/local",
            "remotes": [{ "type": "streamable-http", "url": "https://127.0.0.1/mcp" }]
        }}));
        assert!(private.is_none());
    }

    #[test]
    fn catalog_entries_are_validated() {
        let good = json!({
            "id": "browser", "name": "Browser", "transport": "stdio",
            "command": "npx", "args": ["@modelcontextprotocol/server"],
        });
        assert!(valid_catalog_entry(&good));
        let bad_http = json!({
            "id": "web", "name": "Web", "transport": "http", "url": "http://example.com",
        });
        assert!(!valid_catalog_entry(&bad_http));
        let bad_id = json!({
            "id": "Bad Id", "name": "Bad", "transport": "stdio", "command": "npx",
        });
        assert!(!valid_catalog_entry(&bad_id));
    }

    #[test]
    fn skill_ids_follow_the_host_rule() {
        assert_eq!(skill_id("Frontend_Design", "skill"), "frontend-design");
        assert_eq!(skill_id("pdf", "skill"), "pdf");
        assert_eq!(skill_id("", "skill"), "skill");
    }

    #[test]
    fn frontmatter_is_split_without_leaking_into_the_body() {
        let (name, description, body) = split_frontmatter(
            "---\nname: pdf\ndescription: \"Reads PDFs\"\n---\n\nRead FORMS.md.\n",
        );
        assert_eq!(name.as_deref(), Some("pdf"));
        assert_eq!(description.as_deref(), Some("Reads PDFs"));
        assert_eq!(body.trim(), "Read FORMS.md.");
    }
}
