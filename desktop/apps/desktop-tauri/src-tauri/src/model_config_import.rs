//! Model-configuration import from local agent stores (ADR 0179 / ADR 0188).
//!
//! A scan reads Claude Code, Codex, OpenCode, Pi, and CC Switch configuration
//! and returns public drafts without credentials. Keys stay in the main-process
//! cache and are written through host `providers.create` only when the user
//! runs the selection. OAuth grants from those tools are never copied.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::catalog::Catalog;
use crate::envelope;
use crate::state::AppState;

const MAX_MODELS_PER_PROVIDER: usize = 64;
const MAX_NAME_LENGTH: usize = 80;

#[derive(Debug, Clone)]
struct Draft {
    source: String,
    external_id: String,
    name: String,
    base_url: Option<String>,
    api_style: String,
    vendor_key: String,
    model_ids: Vec<String>,
    models: Vec<Value>,
    secret: Option<String>,
    supports_reasoning: Option<bool>,
}

impl Draft {
    fn key(&self) -> String {
        format!("{}:{}", self.source, self.external_id)
    }

    fn public(&self) -> Value {
        json!({
            "source": self.source,
            "externalId": self.external_id,
            "name": self.name,
            "baseUrl": self.base_url,
            "apiStyle": self.api_style,
            "modelIds": self.model_ids,
            "hasSecret": self.secret.is_some(),
        })
    }

    fn to_json(&self) -> Value {
        json!({
            "source": self.source,
            "externalId": self.external_id,
            "name": self.name,
            "baseUrl": self.base_url,
            "apiStyle": self.api_style,
            "vendorKey": self.vendor_key,
            "modelIds": self.model_ids,
            "models": self.models,
            "secret": self.secret,
            "supportsReasoning": self.supports_reasoning,
        })
    }
}

fn home_path(relative: &str) -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(relative))
}

fn read_text(path: &PathBuf) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn clip_name(name: &str) -> String {
    let trimmed = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.is_empty() {
        return "Imported provider".into();
    }
    if trimmed.chars().count() > MAX_NAME_LENGTH {
        let clipped: String = trimmed.chars().take(MAX_NAME_LENGTH - 1).collect();
        format!("{clipped}...")
    } else {
        trimmed
    }
}

fn first_string(values: &[Option<&str>]) -> Option<String> {
    values
        .iter()
        .flatten()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
        .map(str::to_string)
}

fn as_object(value: Option<&Value>) -> Option<&serde_json::Map<String, Value>> {
    value.and_then(|value| value.as_object())
}

fn string_map(value: Option<&Value>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(object) = as_object(value) {
        for (key, value) in object {
            if let Some(text) = value.as_str() {
                if !text.trim().is_empty() {
                    map.insert(key.clone(), text.to_string());
                }
            }
        }
    }
    map
}

fn is_placeholder_secret(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return true;
    }
    if trimmed.starts_with("${") && trimmed.ends_with('}') && !trimmed.contains(':') {
        return true;
    }
    if let Some(stripped) = trimmed.strip_prefix("$") {
        if stripped
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return true;
        }
    }
    if trimmed.to_ascii_lowercase().contains("your_api_key")
        || trimmed.to_ascii_lowercase().contains("your-api-key")
    {
        return true;
    }
    trimmed.eq_ignore_ascii_case("changeme")
        || trimmed.chars().all(|ch| ch == 'x') && !trimmed.is_empty() && trimmed.len() > 1
}

fn sanitize_secret(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if is_placeholder_secret(value) {
        return None;
    }
    Some(value.to_string())
}

fn resolve_env_secret(value: Option<&str>, env: &HashMap<String, String>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(name) = value.strip_prefix("env:") {
        return sanitize_secret(env.get(name.trim()).map(String::as_str));
    }
    if let Some(rest) = value.strip_prefix("${env:") {
        if let Some(name) = rest.strip_suffix('}') {
            return sanitize_secret(env.get(name.trim()).map(String::as_str));
        }
    }
    sanitize_secret(Some(value))
}

fn first_secret(values: &[Option<String>]) -> Option<String> {
    values
        .iter()
        .flatten()
        .find_map(|value| sanitize_secret(Some(value.as_str())))
}

fn api_style_alias(raw: &str) -> Option<&'static str> {
    let key = raw.trim().to_ascii_lowercase().replace(' ', "-");
    match key.as_str() {
        "chat" | "chat_completions" | "chat-completions" | "completions" | "openai-chat"
        | "openai-completions" | "openai_completions" => Some("chat_completions"),
        "responses" | "openai-responses" | "openai_responses" => Some("responses"),
        "anthropic" | "anthropic_messages" | "anthropic-messages" => Some("anthropic_messages"),
        "google" | "google_generative_ai" | "google-generative-ai" => Some("google_generative_ai"),
        "openai_codex_responses"
        | "openai-codex-responses"
        | "openai-codex"
        | "openai-codex-response" => Some("openai_codex_responses"),
        "pi_messages" | "pi-messages" => Some("pi_messages"),
        "opencode_go" | "opencode-go" => Some("opencode_go"),
        _ => None,
    }
}

fn api_style_for_adapter(npm: Option<&str>) -> &'static str {
    match npm.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "@ai-sdk/anthropic" | "@ai-sdk/google-vertex/anthropic" => "anthropic_messages",
        "@ai-sdk/google" | "@ai-sdk/google-vertex" => "google_generative_ai",
        "@ai-sdk/openai" => "responses",
        _ => "chat_completions",
    }
}

fn protocol_for_style(style: &str) -> &'static str {
    match style {
        "anthropic_messages" => "anthropic",
        "responses" => "openai_responses",
        "google_generative_ai" => "google",
        "openai_codex_responses" => "openai_codex_responses",
        "pi_messages" => "custom_http",
        _ => "openai_compatible",
    }
}

fn default_base_url(vendor_key: &str) -> Option<&'static str> {
    match vendor_key {
        "anthropic" => Some("https://api.anthropic.com/v1"),
        "openai" => Some("https://api.openai.com/v1"),
        "openrouter" => Some("https://openrouter.ai/api/v1"),
        "xai" => Some("https://api.x.ai/v1"),
        "moonshot" | "moonshotai" => Some("https://api.moonshot.cn/v1"),
        _ => None,
    }
}

fn generic_binding(model_id: &str, reasoning: bool) -> Value {
    let thinking = if reasoning {
        json!(["off", "low", "medium", "high"])
    } else {
        json!([])
    };
    json!({
        "id": model_id,
        "contextWindow": 128_000,
        "maxTokens": 8_192,
        "thinkingLevels": thinking,
        "defaultThinkingLevel": if reasoning { json!("medium") } else { Value::Null },
    })
}

fn binding_from_model_entry(entry: &Value) -> Option<Value> {
    let model_id = if let Some(id) = entry.as_str() {
        id.trim().to_string()
    } else {
        let record = as_object(Some(entry))?;
        record
            .get("id")
            .and_then(|v| v.as_str())
            .or_else(|| record.get("model").and_then(|v| v.as_str()))
            .or_else(|| record.get("modelId").and_then(|v| v.as_str()))?
            .trim()
            .to_string()
    };
    if model_id.is_empty() {
        return None;
    }
    let record = as_object(Some(entry));
    let context_window = record
        .and_then(|record| {
            record
                .get("contextWindow")
                .or_else(|| record.get("context_window"))
                .or_else(|| record.get("limit").and_then(|limit| limit.get("context")))
        })
        .and_then(|v| v.as_u64())
        .filter(|value| *value > 0)
        .unwrap_or(128_000);
    let max_tokens = record
        .and_then(|record| {
            record
                .get("maxTokens")
                .or_else(|| record.get("max_tokens"))
                .or_else(|| record.get("maxOutputTokens"))
                .or_else(|| record.get("limit").and_then(|limit| limit.get("output")))
        })
        .and_then(|v| v.as_u64())
        .filter(|value| *value > 0)
        .unwrap_or(8_192);
    let reasoning = record
        .and_then(|record| record.get("reasoning").and_then(|v| v.as_bool()))
        .unwrap_or(false);
    let mut binding = generic_binding(&model_id, reasoning);
    binding["contextWindow"] = json!(context_window);
    binding["maxTokens"] = json!(max_tokens);
    if let Some(input) = record
        .and_then(|record| record.get("input"))
        .and_then(|v| v.as_array())
    {
        if input.iter().any(|item| item.as_str() == Some("image")) {
            binding["supportsImages"] = json!(true);
        }
    }
    Some(binding)
}

fn bindings_for_ids(
    catalog: &Catalog,
    vendor_key: &str,
    base_url: Option<&str>,
    ids: &[String],
    explicit: &[Value],
) -> Vec<Value> {
    let mut bindings: Vec<Value> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for id in ids.iter().take(MAX_MODELS_PER_PROVIDER) {
        let trimmed = id.trim();
        if trimmed.is_empty() || !seen.insert(trimmed.to_string()) {
            continue;
        }
        let explicit = explicit
            .iter()
            .find(|binding| binding.get("id").and_then(|v| v.as_str()) == Some(trimmed));
        if let Some(binding) = explicit {
            bindings.push(binding.clone());
            continue;
        }
        if let Some(model) = catalog.find_model(vendor_key, base_url.unwrap_or(""), trimmed) {
            bindings.push(model.to_binding());
            continue;
        }
        bindings.push(generic_binding(trimmed, false));
    }
    bindings
}

fn finalize_draft(catalog: &Catalog, mut draft: Draft) -> Option<Draft> {
    draft.external_id = draft.external_id.trim().to_string();
    if draft.external_id.is_empty() {
        draft.external_id = "default".into();
    }
    draft.name = clip_name(&draft.name);
    let base_url = draft
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty());
    let vendor_key = if let Some(url) = base_url {
        catalog
            .provider_key_for_url(url)
            .unwrap_or_else(|| "custom".to_string())
    } else if draft.vendor_key != "custom" {
        draft.vendor_key.clone()
    } else {
        "custom".into()
    };
    if base_url.is_none() {
        if let Some(default) = default_base_url(&vendor_key) {
            draft.base_url = Some(default.to_string());
        }
    }
    let models = bindings_for_ids(
        catalog,
        &vendor_key,
        draft.base_url.as_deref(),
        &draft.model_ids,
        &draft.models,
    );
    if models.is_empty() {
        return None;
    }
    draft.vendor_key = vendor_key;
    draft.model_ids = models
        .iter()
        .filter_map(|binding| {
            binding
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect();
    draft.models = models;
    draft.secret = draft
        .secret
        .and_then(|secret| sanitize_secret(Some(&secret)));
    Some(draft)
}

fn parse_claude_settings(
    catalog: &Catalog,
    settings: Option<&Value>,
    local_settings: Option<&Value>,
) -> Vec<Draft> {
    let global = as_object(settings);
    let local = as_object(local_settings);
    let mut env = HashMap::new();
    if let Some(global) = global {
        env.extend(string_map(global.get("env")));
    }
    if let Some(local) = local {
        env.extend(string_map(local.get("env")));
    }
    let base_url = first_string(&[
        env.get("ANTHROPIC_BASE_URL").map(String::as_str),
        env.get("ANTHROPIC_API_URL").map(String::as_str),
    ]);
    let secret = first_secret(&[
        sanitize_secret(env.get("ANTHROPIC_API_KEY").map(String::as_str)),
        sanitize_secret(env.get("ANTHROPIC_AUTH_TOKEN").map(String::as_str)),
    ]);
    let model_ids: Vec<String> = [
        local.and_then(|local| local.get("model").and_then(|v| v.as_str())),
        global.and_then(|global| global.get("model").and_then(|v| v.as_str())),
        env.get("ANTHROPIC_DEFAULT_SONNET_MODEL")
            .map(String::as_str),
        env.get("ANTHROPIC_DEFAULT_OPUS_MODEL").map(String::as_str),
        env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL").map(String::as_str),
        env.get("ANTHROPIC_MODEL").map(String::as_str),
    ]
    .iter()
    .flatten()
    .map(|id| id.trim())
    .filter(|id| !id.is_empty())
    .map(str::to_string)
    .collect();
    if model_ids.is_empty() || (base_url.is_none() && secret.is_none()) {
        return Vec::new();
    }
    let draft = Draft {
        source: "claude-code".into(),
        external_id: "default".into(),
        name: "Claude Code".into(),
        base_url,
        api_style: "anthropic_messages".into(),
        vendor_key: "custom".into(),
        model_ids,
        models: Vec::new(),
        secret,
        supports_reasoning: None,
    };
    finalize_draft(catalog, draft).into_iter().collect()
}

fn parse_open_code_settings(
    catalog: &Catalog,
    config: Option<&Value>,
    auth: Option<&Value>,
    env: &HashMap<String, String>,
) -> Vec<Draft> {
    let root = as_object(config).cloned().unwrap_or_default();
    let providers = root
        .get("provider")
        .and_then(|v| v.as_object())
        .or_else(|| root.get("providers").and_then(|v| v.as_object()))
        .cloned()
        .unwrap_or_default();
    let auth_map = as_object(auth).cloned().unwrap_or_default();
    let default_model = root
        .get("model")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let mut drafts = Vec::new();
    for (id, raw) in providers {
        let Some(record) = raw.as_object() else {
            continue;
        };
        let options = record.get("options").and_then(|v| v.as_object());
        let base_url = first_string(&[
            options
                .and_then(|o| o.get("baseURL"))
                .and_then(|v| v.as_str()),
            options
                .and_then(|o| o.get("baseUrl"))
                .and_then(|v| v.as_str()),
            record.get("baseURL").and_then(|v| v.as_str()),
            record.get("baseUrl").and_then(|v| v.as_str()),
        ]);
        let npm = first_string(&[
            record.get("npm").and_then(|v| v.as_str()),
            record.get("adapter").and_then(|v| v.as_str()),
        ]);
        let api_style = first_string(&[
            record.get("api").and_then(|v| v.as_str()),
            options.and_then(|o| o.get("api")).and_then(|v| v.as_str()),
            record.get("apiStyle").and_then(|v| v.as_str()),
        ])
        .and_then(|raw| api_style_alias(&raw).map(str::to_string))
        .unwrap_or_else(|| api_style_for_adapter(npm.as_deref()).to_string());
        let mut model_ids: Vec<String> = record
            .get("models")
            .map(|models| {
                if let Some(object) = models.as_object() {
                    object.keys().cloned().collect()
                } else if let Some(items) = models.as_array() {
                    items
                        .iter()
                        .filter_map(|item| {
                            item.as_str().map(str::to_string).or_else(|| {
                                item.get("id").and_then(|v| v.as_str()).map(str::to_string)
                            })
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            })
            .unwrap_or_default();
        if model_ids.is_empty() {
            if let Some(default_model) = &default_model {
                if let Some((prefix, model)) = default_model.split_once('/') {
                    if prefix == id {
                        model_ids.push(model.to_string());
                    }
                }
            }
        }
        if model_ids.is_empty() {
            continue;
        }
        let header_secret = record
            .get("headers")
            .or_else(|| options.and_then(|o| o.get("headers")))
            .and_then(|headers| {
                as_object(Some(headers)).and_then(|object| {
                    first_string(&[
                        object.get("Authorization").and_then(|v| v.as_str()),
                        object.get("authorization").and_then(|v| v.as_str()),
                        object.get("x-api-key").and_then(|v| v.as_str()),
                        object.get("X-Api-Key").and_then(|v| v.as_str()),
                    ])
                })
            })
            .and_then(|value| {
                value
                    .strip_prefix("Bearer ")
                    .or_else(|| value.strip_prefix("bearer "))
                    .map(str::to_string)
                    .or(Some(value))
            });
        let auth_entry = auth_map.get(&id);
        let auth_secret = auth_entry.and_then(|entry| {
            let kind = first_string(&[
                entry.get("type").and_then(|v| v.as_str()),
                entry.get("kind").and_then(|v| v.as_str()),
            ])
            .unwrap_or_default()
            .to_ascii_lowercase();
            if !kind.is_empty() && !matches!(kind.as_str(), "api" | "apikey" | "api_key") {
                return None;
            }
            first_string(&[
                entry.get("key").and_then(|v| v.as_str()),
                entry.get("apiKey").and_then(|v| v.as_str()),
                entry.get("token").and_then(|v| v.as_str()),
            ])
        });
        let secret = first_secret(&[
            resolve_env_secret(
                options
                    .and_then(|o| o.get("apiKey"))
                    .and_then(|v| v.as_str())
                    .or_else(|| record.get("apiKey").and_then(|v| v.as_str())),
                env,
            ),
            auth_secret,
            header_secret,
        ]);
        let name = first_string(&[
            record.get("name").and_then(|v| v.as_str()),
            record.get("label").and_then(|v| v.as_str()),
        ])
        .unwrap_or_else(|| id.clone());
        let explicit: Vec<Value> = record
            .get("models")
            .and_then(|models| models.as_object())
            .map(|models| {
                models
                    .iter()
                    .filter_map(|(model_id, raw)| {
                        let mut binding = binding_from_model_entry(raw)?;
                        binding["id"] = json!(model_id);
                        Some(binding)
                    })
                    .collect()
            })
            .unwrap_or_default();
        drafts.extend(finalize_draft(
            catalog,
            Draft {
                source: "opencode".into(),
                external_id: id.clone(),
                name,
                base_url,
                api_style,
                vendor_key: id,
                model_ids,
                models: explicit,
                secret,
                supports_reasoning: None,
            },
        ));
    }
    drafts
}

fn parse_pi_models(catalog: &Catalog, models_json: Option<&Value>) -> Vec<Draft> {
    let root = as_object(models_json).cloned().unwrap_or_default();
    let providers = root
        .get("providers")
        .and_then(|v| v.as_object())
        .cloned()
        .or_else(|| {
            let looks_like_map = root.values().any(|value| {
                value.get("baseUrl").is_some()
                    || value.get("baseURL").is_some()
                    || value.get("models").is_some()
                    || value.get("api").is_some()
            });
            looks_like_map.then(|| root.clone())
        })
        .unwrap_or_default();
    let mut drafts = Vec::new();
    for (id, raw) in providers {
        if id == "providers" {
            continue;
        }
        let Some(record) = raw.as_object() else {
            continue;
        };
        let base_url = first_string(&[
            record.get("baseUrl").and_then(|v| v.as_str()),
            record.get("baseURL").and_then(|v| v.as_str()),
            record.get("url").and_then(|v| v.as_str()),
        ]);
        let api_style = first_string(&[
            record.get("api").and_then(|v| v.as_str()),
            record.get("apiStyle").and_then(|v| v.as_str()),
            record.get("type").and_then(|v| v.as_str()),
        ])
        .and_then(|raw| api_style_alias(&raw).map(str::to_string))
        .unwrap_or_else(|| "chat_completions".to_string());
        let secret = first_secret(&[
            record
                .get("apiKey")
                .and_then(|v| v.as_str())
                .and_then(|value| sanitize_secret(Some(value))),
            record
                .get("api_key")
                .and_then(|v| v.as_str())
                .and_then(|value| sanitize_secret(Some(value))),
            record
                .get("headers")
                .and_then(|headers| {
                    as_object(Some(headers)).and_then(|object| {
                        first_string(&[
                            object.get("Authorization").and_then(|v| v.as_str()),
                            object.get("authorization").and_then(|v| v.as_str()),
                        ])
                    })
                })
                .and_then(|value| {
                    value
                        .strip_prefix("Bearer ")
                        .map(str::to_string)
                        .or(Some(value))
                }),
        ]);
        let entries: Vec<Value> = record
            .get("models")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut model_ids: Vec<String> = entries
            .iter()
            .filter_map(|entry| binding_from_model_entry(entry))
            .filter_map(|binding| {
                binding
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .collect();
        if model_ids.is_empty() {
            model_ids = record
                .get("models")
                .and_then(|v| v.as_object())
                .map(|models| models.keys().cloned().collect())
                .unwrap_or_default();
        }
        if model_ids.is_empty() {
            continue;
        }
        let explicit: Vec<Value> = entries
            .iter()
            .filter_map(binding_from_model_entry)
            .collect();
        let supports_reasoning = entries
            .iter()
            .any(|entry| entry.get("reasoning").and_then(|v| v.as_bool()) == Some(true));
        let name = first_string(&[
            record.get("name").and_then(|v| v.as_str()),
            record.get("label").and_then(|v| v.as_str()),
        ])
        .unwrap_or_else(|| id.clone());
        drafts.extend(finalize_draft(
            catalog,
            Draft {
                source: "pi".into(),
                external_id: id.clone(),
                name,
                base_url,
                api_style,
                vendor_key: id,
                model_ids,
                models: explicit,
                secret,
                supports_reasoning: supports_reasoning.then_some(true),
            },
        ));
    }
    drafts
}

struct TomlExtract {
    root: HashMap<String, TomlValue>,
    tables: Vec<(String, HashMap<String, TomlValue>)>,
}

#[derive(Clone, Debug, PartialEq)]
enum TomlValue {
    Text(String),
    Integer(i64),
    Boolean(bool),
}

impl TomlValue {
    fn as_text(&self) -> Option<String> {
        match self {
            TomlValue::Text(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
            TomlValue::Integer(value) => Some(value.to_string()),
            TomlValue::Boolean(value) => Some(value.to_string()),
            _ => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            TomlValue::Boolean(value) => Some(*value),
            _ => None,
        }
    }
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut quote = '\0';
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = false;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_string = true;
            quote = ch;
            continue;
        }
        if ch == '#' {
            return &line[..index];
        }
    }
    line
}

fn parse_toml_quoted(input: &str, quote: char) -> Option<(String, &str)> {
    if !input.starts_with(quote) {
        return None;
    }
    if quote == '\'' {
        let end = input[1..].find('\'')? + 1;
        return Some((input[1..end].to_string(), &input[end + 1..]));
    }
    let mut out = String::new();
    let mut index = 1;
    let bytes = input.as_bytes();
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if ch == '\\' {
            let next = bytes.get(index + 1)?;
            let mapped = match *next {
                b'n' => '\n',
                b't' => '\t',
                b'r' => '\r',
                b'"' => '"',
                b'\\' => '\\',
                other => other as char,
            };
            out.push(mapped);
            index += 2;
            continue;
        }
        if ch == '"' {
            return Some((out, &input[index + 1..]));
        }
        out.push(ch);
        index += 1;
    }
    None
}

fn parse_toml_value(raw: &str) -> Option<TomlValue> {
    let trimmed = raw.trim_start();
    if trimmed.starts_with('"') {
        return parse_toml_quoted(trimmed, '"').map(|(value, _)| TomlValue::Text(value));
    }
    if trimmed.starts_with('\'') {
        return parse_toml_quoted(trimmed, '\'').map(|(value, _)| TomlValue::Text(value));
    }
    if trimmed.starts_with("true") {
        return Some(TomlValue::Boolean(true));
    }
    if trimmed.starts_with("false") {
        return Some(TomlValue::Boolean(false));
    }
    trimmed
        .split(|ch: char| matches!(ch, ' ' | '\t' | '#' | ',' | '}' | ']'))
        .next()
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<i64>().ok())
        .map(TomlValue::Integer)
}

fn parse_toml_table_header(line: &str) -> Option<String> {
    if !line.starts_with('[') || !line.ends_with(']') {
        return None;
    }
    let inner = line[1..line.len() - 1].trim();
    let mut parts: Vec<String> = Vec::new();
    let mut rest = inner;
    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.starts_with('"') || rest.starts_with('\'') {
            let quote = rest.chars().next()?;
            let (value, tail) = parse_toml_quoted(rest, quote)?;
            parts.push(value);
            rest = tail.trim_start();
        } else {
            let end = rest
                .find(|ch: char| matches!(ch, '.' | ' ' | '\t'))
                .unwrap_or(rest.len());
            let part = rest[..end].trim();
            if part.is_empty() {
                return None;
            }
            parts.push(part.to_string());
            rest = &rest[end..];
        }
        if let Some(tail) = rest.strip_prefix('.') {
            rest = tail;
        }
    }
    Some(parts.join("."))
}

fn parse_toml_subset(text: &str) -> TomlExtract {
    let mut root: HashMap<String, TomlValue> = HashMap::new();
    let mut tables: Vec<(String, HashMap<String, TomlValue>)> = Vec::new();
    let mut current_is_root = true;
    let mut current_index: Option<usize> = None;
    for raw_line in text.lines() {
        let line = strip_toml_comment(raw_line).trim();
        if line.is_empty() || line.starts_with("[[") {
            continue;
        }
        if line.starts_with('[') {
            if let Some(header) = parse_toml_table_header(line) {
                tables.push((header, HashMap::new()));
                current_index = Some(tables.len() - 1);
                current_is_root = false;
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().trim_matches('"').to_string();
        let value = match parse_toml_value(value) {
            Some(value) => value,
            None => continue,
        };
        if key.is_empty() {
            continue;
        }
        if current_is_root {
            root.insert(key, value);
        } else if let Some(index) = current_index {
            tables[index].1.insert(key, value);
        }
    }
    TomlExtract { root, tables }
}

fn parse_codex_toml(catalog: &Catalog, text: &str, env: &HashMap<String, String>) -> Vec<Draft> {
    let extracted = parse_toml_subset(text);
    let default_model = extracted.root.get("model").and_then(TomlValue::as_text);
    let mut drafts = Vec::new();
    for (table_key, fields) in &extracted.tables {
        let Some(id) = table_key.strip_prefix("model_providers.") else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        let requires_auth = fields
            .get("requires_openai_auth")
            .and_then(TomlValue::as_bool)
            .unwrap_or(false);
        let env_key = fields
            .get("env_key")
            .or_else(|| fields.get("envKey"))
            .and_then(TomlValue::as_text);
        let api_key = fields
            .get("api_key")
            .or_else(|| fields.get("apiKey"))
            .and_then(TomlValue::as_text);
        if requires_auth && env_key.is_none() && api_key.is_none() {
            continue;
        }
        let base_url = fields
            .get("base_url")
            .or_else(|| fields.get("baseUrl"))
            .and_then(TomlValue::as_text);
        let secret = first_secret(&[
            api_key.and_then(|value| resolve_env_secret(Some(&value), env)),
            env_key.and_then(|key| sanitize_secret(env.get(&key).map(String::as_str))),
        ]);
        let api_style = fields
            .get("wire_api")
            .or_else(|| fields.get("wireApi"))
            .and_then(TomlValue::as_text)
            .and_then(|raw| api_style_alias(&raw).map(str::to_string))
            .unwrap_or_else(|| "chat_completions".into());
        let mut model_ids: Vec<String> = Vec::new();
        if let Some(value) = default_model.as_ref() {
            model_ids.push(value.clone());
        }
        if let Some(models) = fields.get("models").and_then(TomlValue::as_text) {
            model_ids.extend(
                models
                    .split(|ch: char| ch == ',' || ch.is_whitespace())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
            );
        }
        model_ids.dedup();
        if model_ids.is_empty() {
            continue;
        }
        let name = fields
            .get("name")
            .and_then(TomlValue::as_text)
            .unwrap_or_else(|| id.to_string());
        drafts.extend(finalize_draft(
            catalog,
            Draft {
                source: "codex".into(),
                external_id: id.to_string(),
                name,
                base_url,
                api_style,
                vendor_key: id.to_string(),
                model_ids,
                models: Vec::new(),
                secret,
                supports_reasoning: None,
            },
        ));
    }
    drafts
}

struct CcSwitchRow {
    id: String,
    app_type: String,
    name: String,
    settings_config: Value,
}

fn read_cc_switch_providers() -> Vec<CcSwitchRow> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let db_path = home.join(".cc-switch").join("cc-switch.db");
    if db_path.exists() {
        if let Ok(rows) = read_cc_switch_db(&db_path) {
            return rows;
        }
    }
    let legacy = home.join(".cc-switch").join("config.json");
    let Some(document) = read_text(&legacy).and_then(|text| parse_json_document(&text)) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for app_type in [
        "claude",
        "claude-desktop",
        "codex",
        "gemini",
        "grokbuild",
        "opencode",
        "openclaw",
        "hermes",
        "pi",
    ] {
        let Some(providers) = document
            .get(app_type)
            .and_then(|app| app.get("providers"))
            .and_then(|v| v.as_object())
        else {
            continue;
        };
        for (id, raw) in providers {
            let Some(record) = raw.as_object() else {
                continue;
            };
            let name = first_string(&[record.get("name").and_then(|v| v.as_str())])
                .unwrap_or_else(|| id.clone());
            let settings = record
                .get("settingsConfig")
                .or_else(|| record.get("settings_config"))
                .cloned()
                .unwrap_or_else(|| Value::Object(record.clone()));
            rows.push(CcSwitchRow {
                id: first_string(&[record.get("id").and_then(|v| v.as_str())])
                    .unwrap_or_else(|| id.clone()),
                app_type: app_type.to_string(),
                name,
                settings_config: settings,
            });
        }
    }
    rows
}

fn read_cc_switch_db(path: &std::path::Path) -> Result<Vec<CcSwitchRow>, String> {
    use rusqlite::Connection;
    let connection = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let mut columns: Vec<String> = Vec::new();
    {
        let mut statement = connection
            .prepare("PRAGMA table_info(providers)")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| e.to_string())?;
        for row in rows.flatten() {
            columns.push(row);
        }
    }
    let find = |candidates: &[&str]| -> Option<String> {
        candidates
            .iter()
            .find(|candidate| {
                columns
                    .iter()
                    .any(|column| column.eq_ignore_ascii_case(candidate))
            })
            .map(|candidate| {
                columns
                    .iter()
                    .find(|column| column.eq_ignore_ascii_case(candidate))
                    .cloned()
                    .unwrap_or_else(|| candidate.to_string())
            })
    };
    let id_col = find(&["id"]).ok_or_else(|| "providers table has no id column".to_string())?;
    let app_col = find(&["app_type", "appType", "app"]);
    let name_col = find(&["name"]);
    let settings_col = find(&["settings_config", "settingsConfig", "config"]);
    let Some(settings_col) = settings_col else {
        return Ok(Vec::new());
    };
    let mut sql = format!("SELECT \"{id_col}\", \"{settings_col}\"");
    sql.push_str(&format!(
        ", \"{}\" FROM providers",
        name_col.as_deref().unwrap_or(&id_col)
    ));
    if let Some(app_col) = &app_col {
        sql.push_str(&format!(", \"{app_col}\""));
    }
    let mut statement = connection.prepare(&sql).map_err(|e| e.to_string())?;
    let mut rows: Vec<CcSwitchRow> = Vec::new();
    let mapped = statement
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let settings: String = row.get(1).unwrap_or_default();
            let name: String = row.get(2).unwrap_or_else(|_| id.clone());
            let app_type: String = if app_col.is_some() {
                row.get(3).unwrap_or_default()
            } else {
                String::new()
            };
            Ok((id, settings, name, app_type))
        })
        .map_err(|e| e.to_string())?;
    for row in mapped.flatten() {
        let (id, settings, name, app_type) = row;
        let settings_config = parse_json_document(&settings).unwrap_or_else(|| json!({}));
        rows.push(CcSwitchRow {
            id,
            app_type: if app_type.is_empty() {
                "claude".into()
            } else {
                app_type
            },
            name,
            settings_config,
        });
    }
    Ok(rows)
}

fn parse_cc_switch(catalog: &Catalog, env: &HashMap<String, String>) -> Vec<Draft> {
    let mut drafts = Vec::new();
    for row in read_cc_switch_providers() {
        let app_type = row.app_type.to_ascii_lowercase();
        let retagged = match app_type.as_str() {
            "claude" | "claude-desktop" => {
                let parsed = parse_claude_settings(catalog, Some(&row.settings_config), None);
                if parsed.is_empty() {
                    let settings = as_object(Some(&row.settings_config))
                        .cloned()
                        .unwrap_or_default();
                    let env_map = string_map(settings.get("env"));
                    let secret = first_secret(&[
                        sanitize_secret(env_map.get("ANTHROPIC_API_KEY").map(String::as_str)),
                        sanitize_secret(env_map.get("ANTHROPIC_AUTH_TOKEN").map(String::as_str)),
                    ]);
                    let base_url = first_string(&[
                        env_map.get("ANTHROPIC_BASE_URL").map(String::as_str),
                        env_map.get("ANTHROPIC_API_URL").map(String::as_str),
                    ]);
                    if base_url.is_none() && secret.is_none() {
                        Vec::new()
                    } else {
                        finalize_draft(
                            catalog,
                            Draft {
                                source: "cc-switch".into(),
                                external_id: row.id.clone(),
                                name: row.name.clone(),
                                base_url,
                                api_style: "anthropic_messages".into(),
                                vendor_key: "custom".into(),
                                model_ids: vec!["default".into()],
                                models: Vec::new(),
                                secret,
                                supports_reasoning: None,
                            },
                        )
                        .into_iter()
                        .collect()
                    }
                } else {
                    parsed
                }
            }
            "opencode" | "hermes" => parse_open_code_settings(
                catalog,
                Some(&json!({ "provider": { row.id.clone(): row.settings_config.clone() } })),
                None,
                env,
            ),
            "pi" => parse_pi_models(
                catalog,
                Some(&json!({ "providers": { row.id.clone(): row.settings_config.clone() } })),
            ),
            "codex" | "grokbuild" => {
                let config = row
                    .settings_config
                    .get("config")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let auth_secret = row
                    .settings_config
                    .get("auth")
                    .and_then(|auth| {
                        first_string(&[
                            auth.get("OPENAI_API_KEY").and_then(|v| v.as_str()),
                            auth.get("api_key").and_then(|v| v.as_str()),
                        ])
                    })
                    .and_then(|value| sanitize_secret(Some(&value)));
                let mut parsed = parse_codex_toml(catalog, config, env);
                if parsed.is_empty() {
                    let base_url = config
                        .lines()
                        .find_map(|line| line.split_once("base_url"))
                        .and_then(|(_, rest)| rest.split('"').nth(1))
                        .map(str::to_string);
                    let model = config.lines().find_map(|line| {
                        let line = line.trim();
                        line.strip_prefix("model")
                            .and_then(|rest| rest.trim_start().strip_prefix('='))
                            .and_then(|rest| rest.split('"').nth(1))
                            .map(str::to_string)
                    });
                    if base_url.is_none() && auth_secret.is_none() {
                        Vec::new()
                    } else {
                        finalize_draft(
                            catalog,
                            Draft {
                                source: "cc-switch".into(),
                                external_id: row.id.clone(),
                                name: row.name.clone(),
                                base_url,
                                api_style: if app_type == "codex" {
                                    "responses".into()
                                } else {
                                    "chat_completions".into()
                                },
                                vendor_key: "custom".into(),
                                model_ids: vec![model.unwrap_or_else(|| "default".into())],
                                models: Vec::new(),
                                secret: auth_secret,
                                supports_reasoning: None,
                            },
                        )
                        .into_iter()
                        .collect()
                    }
                } else {
                    for draft in &mut parsed {
                        if draft.secret.is_none() {
                            draft.secret = auth_secret.clone();
                        }
                    }
                    parsed
                }
            }
            "gemini" => {
                let settings = as_object(Some(&row.settings_config))
                    .cloned()
                    .unwrap_or_default();
                let env_map = string_map(settings.get("env"));
                let base_url = first_string(&[
                    env_map.get("GOOGLE_GEMINI_BASE_URL").map(String::as_str),
                    env_map.get("GEMINI_BASE_URL").map(String::as_str),
                    env_map.get("GOOGLE_API_BASE").map(String::as_str),
                ]);
                let secret = first_secret(&[
                    resolve_env_secret(env_map.get("GEMINI_API_KEY").map(String::as_str), env),
                    resolve_env_secret(env_map.get("GOOGLE_API_KEY").map(String::as_str), env),
                ]);
                let model = first_string(&[
                    settings
                        .get("config")
                        .and_then(|config| config.get("model"))
                        .and_then(|v| v.as_str()),
                    settings.get("model").and_then(|v| v.as_str()),
                    env_map.get("GEMINI_MODEL").map(String::as_str),
                    env_map.get("GOOGLE_MODEL").map(String::as_str),
                ]);
                if base_url.is_none() && secret.is_none() {
                    Vec::new()
                } else {
                    finalize_draft(
                        catalog,
                        Draft {
                            source: "cc-switch".into(),
                            external_id: row.id.clone(),
                            name: row.name.clone(),
                            base_url: base_url.clone(),
                            api_style: if base_url.is_some() {
                                "chat_completions".into()
                            } else {
                                "google_generative_ai".into()
                            },
                            vendor_key: "google".into(),
                            model_ids: vec![model.unwrap_or_else(|| "default".into())],
                            models: Vec::new(),
                            secret,
                            supports_reasoning: None,
                        },
                    )
                    .into_iter()
                    .collect()
                }
            }
            _ => Vec::new(),
        };
        let count = retagged.len();
        for (index, mut draft) in retagged.into_iter().enumerate() {
            draft.source = "cc-switch".into();
            draft.external_id = if count == 1 {
                format!("{}:{}", row.app_type, row.id)
            } else {
                format!("{}:{}:{}", row.app_type, row.id, index)
            };
            draft.name = clip_name(&row.name);
            drafts.push(draft);
        }
    }
    drafts
}

fn parse_json_document(text: &str) -> Option<Value> {
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        return Some(value);
    }
    let without_comments = strip_jsonc(text);
    let without_trailing = strip_trailing_commas(&without_comments);
    serde_json::from_str::<Value>(&without_trailing).ok()
}

fn strip_jsonc(text: &str) -> String {
    let mut out = String::new();
    let mut in_string = false;
    let mut quote = '\0';
    let mut escaped = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = false;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_string = true;
            quote = ch;
            out.push(ch);
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'/') {
            for next in chars.by_ref() {
                if next == '\n' {
                    out.push('\n');
                    break;
                }
            }
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(next) = chars.next() {
                if next == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn strip_trailing_commas(text: &str) -> String {
    let mut out = String::new();
    let mut in_string = false;
    let mut quote = '\0';
    let mut escaped = false;
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_string = true;
            quote = ch;
            out.push(ch);
            index += 1;
            continue;
        }
        if ch == ',' {
            let mut cursor = index + 1;
            while cursor < chars.len() && chars[cursor].is_whitespace() {
                cursor += 1;
            }
            if matches!(chars.get(cursor), Some('}') | Some(']')) {
                index += 1;
                continue;
            }
        }
        out.push(ch);
        index += 1;
    }
    out
}

fn env_map() -> HashMap<String, String> {
    std::env::vars().collect()
}

fn claude_config_drafts(catalog: &Catalog) -> Vec<Draft> {
    let Some(settings_path) = home_path(".claude/settings.json") else {
        return Vec::new();
    };
    let settings = read_text(&settings_path).and_then(|text| parse_json_document(&text));
    let local_settings = home_path(".claude/settings.local.json")
        .and_then(|path| read_text(&path))
        .and_then(|text| parse_json_document(&text));
    if settings.is_none() && local_settings.is_none() {
        return Vec::new();
    }
    parse_claude_settings(catalog, settings.as_ref(), local_settings.as_ref())
}

fn codex_config_drafts(catalog: &Catalog, env: &HashMap<String, String>) -> Vec<Draft> {
    let Some(path) = home_path(".codex/config.toml") else {
        return Vec::new();
    };
    let Some(text) = read_text(&path) else {
        return Vec::new();
    };
    parse_codex_toml(catalog, &text, env)
}

fn opencode_config_drafts(catalog: &Catalog, env: &HashMap<String, String>) -> Vec<Draft> {
    let config = [
        "~/.config/opencode/opencode.json",
        "~/.config/opencode/opencode.jsonc",
    ]
    .iter()
    .find_map(|relative| {
        let relative = relative.trim_start_matches("~/");
        home_path(relative)
            .and_then(|path| read_text(&path))
            .and_then(|text| parse_json_document(&text))
    });
    let Some(config) = config else {
        return Vec::new();
    };
    let auth = [
        "~/.local/share/opencode/auth.json",
        "~/.config/opencode/auth.json",
    ]
    .iter()
    .find_map(|relative| {
        let relative = relative.trim_start_matches("~/");
        home_path(relative)
            .and_then(|path| read_text(&path))
            .and_then(|text| parse_json_document(&text))
    });
    parse_open_code_settings(catalog, Some(&config), auth.as_ref(), env)
}

fn pi_config_drafts(catalog: &Catalog) -> Vec<Draft> {
    let config = ["~/.pi/agent/models.json", "~/.pi/models.json"]
        .iter()
        .find_map(|relative| {
            let relative = relative.trim_start_matches("~/");
            home_path(relative)
                .and_then(|path| read_text(&path))
                .and_then(|text| parse_json_document(&text))
        });
    let Some(config) = config else {
        return Vec::new();
    };
    parse_pi_models(catalog, Some(&config))
}

fn provider_key(base_url: Option<&str>, api_style: &str, vendor_key: &str) -> String {
    if let Some(url) = base_url.filter(|url| !url.trim().is_empty()) {
        return format!("url:{}|{api_style}", normalize_url(url));
    }
    format!("vendor:{}|{api_style}", vendor_key)
}

fn normalize_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_ascii_lowercase()
}

async fn existing_providers(state: &Arc<AppState>) -> Vec<Value> {
    state
        .host
        .call("providers.list", json!({ "includeDisabled": true }))
        .await
        .ok()
        .and_then(|payload| payload.get("providers").and_then(|v| v.as_array()).cloned())
        .unwrap_or_default()
}

fn existing_identity(row: &Value, secret: Option<&str>) -> (String, Option<String>) {
    let base_url = row.get("baseUrl").and_then(|v| v.as_str());
    let api_style = row.get("apiStyle").and_then(|v| v.as_str()).unwrap_or("");
    let vendor_key = row
        .get("vendorKey")
        .and_then(|v| v.as_str())
        .unwrap_or("custom");
    let key = if let Some(url) = base_url.filter(|url| !url.trim().is_empty()) {
        format!("url:{}|{api_style}", normalize_url(url))
    } else {
        format!("vendor:{}|{api_style}", vendor_key)
    };
    (key, secret.map(str::to_string))
}

async fn existing_matches(state: &Arc<AppState>, draft: &Draft) -> bool {
    let rows = existing_providers(state).await;
    if rows.is_empty() {
        return false;
    }
    let key = provider_key(
        draft.base_url.as_deref(),
        &draft.api_style,
        &draft.vendor_key,
    );
    for row in rows {
        let Some(id) = row.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let secret = state
            .host
            .call("providers.getSecret", json!({ "id": id }))
            .await
            .ok()
            .and_then(|value| {
                value
                    .get("value")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            });
        let (row_key, row_secret) = existing_identity(&row, secret.as_deref());
        if row_key != key {
            continue;
        }
        let left = draft.secret.clone();
        let right = row_secret;
        if left.is_some() || right.is_some() {
            if left == right {
                return true;
            }
        } else {
            return true;
        }
    }
    false
}

pub async fn scan(state: &Arc<AppState>) -> Value {
    let env = env_map();
    let catalog = &state.catalog;
    let mut drafts: Vec<Draft> = Vec::new();
    drafts.extend(claude_config_drafts(catalog));
    drafts.extend(codex_config_drafts(catalog, &env));
    drafts.extend(opencode_config_drafts(catalog, &env));
    drafts.extend(pi_config_drafts(catalog));
    drafts.extend(parse_cc_switch(catalog, &env));
    // Live files come first; a CC Switch row that repeats their endpoint and
    // credential is not listed twice (ADR 0179 §2).
    let mut seen_keys: HashSet<String> = HashSet::new();
    let mut seen_draft_identity: HashSet<(String, Option<String>)> = HashSet::new();
    let mut kept: Vec<Draft> = Vec::new();
    for draft in drafts {
        let identity = (
            provider_key(
                draft.base_url.as_deref(),
                &draft.api_style,
                &draft.vendor_key,
            ),
            draft.secret.clone(),
        );
        if draft.source == "cc-switch" && seen_draft_identity.contains(&identity) {
            continue;
        }
        seen_draft_identity.insert(identity);
        if !seen_keys.insert(draft.key()) {
            continue;
        }
        if existing_matches(state, &draft).await {
            continue;
        }
        kept.push(draft);
    }
    let mut candidates: Vec<Value> = Vec::with_capacity(kept.len());
    {
        let mut cache = state.model_import_cache.lock().unwrap();
        cache.clear();
        for draft in kept {
            candidates.push(draft.public());
            cache.insert(draft.key(), draft.to_json());
        }
    }
    envelope::ok(json!({ "providers": candidates }))
}

pub async fn run(state: &Arc<AppState>, items: &[Value]) -> Value {
    let mut imported = 0;
    let mut skipped = 0;
    let mut failed = 0;
    let settings = state.host.call("settings.get", json!({})).await.ok();
    let mut default_set = settings
        .as_ref()
        .and_then(|settings| settings.get("defaultProviderId"))
        .and_then(|v| v.as_str())
        .map(|id| !id.is_empty())
        .unwrap_or(false);
    for item in items {
        let Some(source) = item.get("source").and_then(|v| v.as_str()) else {
            failed += 1;
            continue;
        };
        let Some(external_id) = item.get("externalId").and_then(|v| v.as_str()) else {
            failed += 1;
            continue;
        };
        let cached = state
            .model_import_cache
            .lock()
            .unwrap()
            .get(&format!("{source}:{external_id}"))
            .cloned();
        let Some(cached) = cached else {
            skipped += 1;
            continue;
        };
        let draft = match draft_from_cache(&cached) {
            Some(draft) => draft,
            None => {
                failed += 1;
                continue;
            }
        };
        if existing_matches(state, &draft).await {
            skipped += 1;
            continue;
        }
        let default_model = draft
            .models
            .first()
            .and_then(|binding| binding.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mut input = json!({
            "name": draft.name,
            "vendorKey": draft.vendor_key,
            "type": "openai_compatible",
            "protocol": protocol_for_style(&draft.api_style),
            "authKind": "api_key_and_base_url",
            "models": draft.models,
            "defaultModelId": default_model,
            "apiStyle": draft.api_style,
        });
        if let Some(base_url) = &draft.base_url {
            input["baseUrl"] = json!(base_url);
        }
        if let Some(secret) = &draft.secret {
            input["secretValue"] = json!(secret);
        }
        if let Some(supports_reasoning) = draft.supports_reasoning {
            input["supportsReasoning"] = json!(supports_reasoning);
        }
        match state.host.call("providers.create", input).await {
            Ok(created) => {
                imported += 1;
                if !default_set {
                    if let Some(provider_id) =
                        created.pointer("/provider/id").and_then(|v| v.as_str())
                    {
                        if let Some(mut settings) = settings.clone() {
                            settings["defaultProviderId"] = json!(provider_id);
                            settings["defaultModelId"] = json!(default_model);
                            let _ = state.host.call("settings.set", settings).await;
                            default_set = true;
                        }
                    }
                }
            }
            Err(error) => {
                log::warn!("model import failed: {error}");
                failed += 1;
            }
        }
    }
    envelope::ok(json!({ "imported": imported, "skipped": skipped, "failed": failed }))
}

fn draft_from_cache(value: &Value) -> Option<Draft> {
    Some(Draft {
        source: value.get("source")?.as_str()?.to_string(),
        external_id: value.get("externalId")?.as_str()?.to_string(),
        name: value.get("name")?.as_str()?.to_string(),
        base_url: value
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        api_style: value.get("apiStyle")?.as_str()?.to_string(),
        vendor_key: value.get("vendorKey")?.as_str()?.to_string(),
        model_ids: value
            .get("modelIds")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|id| id.as_str().map(str::to_string))
            .collect(),
        models: value
            .get("models")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
        secret: value
            .get("secret")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        supports_reasoning: value.get("supportsReasoning").and_then(|v| v.as_bool()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonc_documents_lose_comments_and_trailing_commas() {
        let text = r#"{
            // a comment
            "model": "claude-opus",
            "env": { "A": "b", },
        }"#;
        let parsed = parse_json_document(text).expect("jsonc");
        assert_eq!(parsed["model"], "claude-opus");
        assert_eq!(parsed["env"]["A"], "b");
    }

    #[test]
    fn codex_toml_tables_are_read() {
        let text = r#"
model = "gpt-5.4"
[model_providers.azure]
name = "Azure"
base_url = "https://example.com/v1"
wire_api = "responses"
models = "gpt-5.4, gpt-5.3"
"#;
        let extracted = parse_toml_subset(text);
        assert_eq!(
            extracted.root.get("model").and_then(TomlValue::as_text),
            Some("gpt-5.4".into())
        );
        let (key, fields) = &extracted.tables[0];
        assert_eq!(key, "model_providers.azure");
        assert_eq!(
            fields.get("base_url").and_then(TomlValue::as_text),
            Some("https://example.com/v1".into())
        );
    }

    #[test]
    fn secrets_reject_placeholders() {
        assert_eq!(sanitize_secret(Some("YOUR_API_KEY")), None);
        assert_eq!(sanitize_secret(Some("${OPENAI_API_KEY}")), None);
        assert_eq!(sanitize_secret(Some("sk-real")), Some("sk-real".into()));
    }
}
