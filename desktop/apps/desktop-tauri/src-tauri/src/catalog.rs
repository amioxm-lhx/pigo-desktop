//! models.dev catalog (ADR 0134): a released JSON snapshot read without
//! network I/O plus an explicit, process-local refresh. The catalog owns
//! model metadata only — provider rows, credentials, and bindings stay
//! user-owned, and a refresh never writes the bundled release resource.
//!
//! Lookup order for one model: exact catalog provider key (`vendorKey`) →
//! normalized provider API URL → an unambiguous vendor-prefixed model id.
//! A miss leaves the caller on its explicit generic fallback.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use serde_json::{json, Map, Value};

pub const MODELS_DEV_API_URL: &str = "https://models.dev/api.json";
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_CONTEXT_WINDOW: u64 = 128_000;
const DEFAULT_MAX_TOKENS: u64 = 8_192;

/// Canonical thinking levels, in the order the renderer sorts them.
const CANONICAL_LEVELS: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];

#[derive(Debug)]
struct ModelEntry {
    provider_key: String,
    /// Published id, which may carry a `vendor/model` namespace.
    id: String,
    raw: Value,
}

impl ModelEntry {
    fn lower_id(&self) -> String {
        self.id.to_lowercase()
    }

    fn str(&self, key: &str) -> Option<&str> {
        self.raw.get(key).and_then(|v| v.as_str())
    }

    fn bool(&self, key: &str) -> Option<bool> {
        self.raw.get(key).and_then(|v| v.as_bool())
    }

    fn display_name(&self) -> String {
        self.str("name")
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&self.id)
            .to_string()
    }

    fn modalities(&self) -> (Vec<String>, Vec<String>) {
        let input = string_list(self.raw.pointer("/modalities/input"))
            .unwrap_or_else(|| vec!["text".to_string()]);
        let output = string_list(self.raw.pointer("/modalities/output"))
            .unwrap_or_else(|| vec!["text".to_string()]);
        (input, output)
    }

    fn supports_text(&self) -> bool {
        let (input, output) = self.modalities();
        input.iter().any(|m| m == "text") && output.iter().any(|m| m == "text")
    }

    fn context_window(&self) -> u64 {
        self.raw
            .pointer("/limit/context")
            .or_else(|| self.raw.pointer("/limit/input"))
            .and_then(|v| v.as_u64())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_CONTEXT_WINDOW)
    }

    fn max_tokens(&self) -> u64 {
        self.raw
            .pointer("/limit/output")
            .and_then(|v| v.as_u64())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_MAX_TOKENS)
    }

    /// Canonical levels a published record advertises (ADR 0134 mapping).
    fn thinking_levels(&self) -> Vec<String> {
        if self.bool("reasoning") != Some(true) {
            return Vec::new();
        }
        let options = self
            .raw
            .get("reasoning_options")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if options.is_empty() {
            return vec!["low".into(), "medium".into(), "high".into()];
        }
        let mut levels: Vec<String> = Vec::new();
        let mut saw_enabled = false;
        for option in &options {
            match option.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                "effort" => {
                    for value in option
                        .get("values")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default()
                    {
                        let Some(value) = value.as_str() else {
                            continue;
                        };
                        let level = if value == "none" { "off" } else { value };
                        if CANONICAL_LEVELS.contains(&level) {
                            if level != "off" {
                                saw_enabled = true;
                            }
                            push_level(&mut levels, level);
                        }
                    }
                }
                "toggle" | "budget_tokens" => {
                    push_level(&mut levels, "off");
                    push_level(&mut levels, "medium");
                    saw_enabled = true;
                }
                _ => {}
            }
        }
        if levels.is_empty() {
            return vec!["low".into(), "medium".into(), "high".into()];
        }
        if !saw_enabled && levels.len() == 1 {
            // An off-only set genuinely disables thinking; keep it verbatim.
            return levels;
        }
        levels
    }

    fn thinking_level_map(&self) -> Map<String, Value> {
        let mut map = Map::new();
        let options = self
            .raw
            .get("reasoning_options")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for option in options {
            match option.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                "effort" => {
                    for value in option
                        .get("values")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default()
                    {
                        let Some(value) = value.as_str() else {
                            continue;
                        };
                        if value == "none" {
                            map.insert("off".into(), Value::Null);
                        } else if CANONICAL_LEVELS.contains(&value) {
                            map.insert(value.into(), json!(value));
                        }
                    }
                }
                "toggle" | "budget_tokens" => {
                    map.insert("off".into(), Value::Null);
                    map.insert("medium".into(), json!("medium"));
                }
                _ => {}
            }
        }
        map
    }

    fn cost(&self) -> Value {
        let Some(raw) = self.raw.get("cost").and_then(|v| v.as_object()) else {
            return json!({});
        };
        let mut out = Map::new();
        for (key, value) in raw {
            let camel = match key.as_str() {
                "cache_read" => "cacheRead",
                "cache_write" => "cacheWrite",
                "input_audio" => "inputAudio",
                "output_audio" => "outputAudio",
                "context_over_200k" => "contextOver200k",
                other => other,
            };
            let mapped = if camel == "contextOver200k" || camel == "tiers" {
                camel_case_value(value)
            } else {
                value.clone()
            };
            out.insert(camel.to_string(), mapped);
        }
        Value::Object(out)
    }

    fn to_model_info(&self, provider_id: &str, source: &str) -> Value {
        let (input_modalities, output_modalities) = self.modalities();
        let levels = self.thinking_levels();
        let mut capabilities: Vec<&str> = vec!["text"];
        if self.bool("tool_call").unwrap_or(false) {
            capabilities.push("tools");
        }
        if self.bool("reasoning").unwrap_or(false) || !levels.is_empty() {
            capabilities.push("reasoning");
        }
        if self.bool("structured_output").unwrap_or(false) {
            capabilities.push("json");
        }
        if self.bool("attachment").unwrap_or(false) {
            capabilities.push("attachments");
        }
        if input_modalities.iter().any(|m| m == "image") {
            capabilities.push("vision");
        }
        for modality in ["audio", "video", "pdf"] {
            if input_modalities.iter().any(|m| m == modality)
                || output_modalities.iter().any(|m| m == modality)
            {
                capabilities.push(modality);
            }
        }
        if self.bool("temperature").unwrap_or(false) {
            capabilities.push("temperature");
        }

        let mut info = json!({
            "modelId": self.id,
            "displayName": self.display_name(),
            "providerId": provider_id,
            "modalities": {
                "input": input_modalities,
                "output": output_modalities,
            },
            "reasoning": self.bool("reasoning").unwrap_or(false),
            "capabilities": capabilities,
            "supportedThinkingLevels": levels,
            "thinkingLevelMap": self.thinking_level_map(),
            "limit": {
                "context": self.context_window(),
                "output": self.max_tokens(),
            },
            "cost": self.cost(),
            "contextWindow": self.context_window(),
            "maxTokens": self.max_tokens(),
            "source": source,
            "catalogSource": "models.dev",
        });
        if let Value::Object(object) = &mut info {
            copy_field(&self.raw, object, "description");
            copy_field(&self.raw, object, "family");
            copy_field(&self.raw, object, "knowledge");
            copy_field(&self.raw, object, "status");
            copy_field(&self.raw, object, "interleaved");
            copy_field(&self.raw, object, "experimental");
            copy_field(&self.raw, object, "provider");
            copy_field(&self.raw, object, "reasoning_options");
            if self.raw.get("reasoning_options").is_some() {
                object.insert(
                    "reasoningOptions".into(),
                    self.raw
                        .get("reasoning_options")
                        .cloned()
                        .unwrap_or(Value::Null),
                );
                object.remove("reasoning_options");
            }
            if let Some(value) = self.raw.get("attachment").and_then(|v| v.as_bool()) {
                object.insert("attachment".into(), json!(value));
            }
            if let Some(value) = self.raw.get("tool_call").and_then(|v| v.as_bool()) {
                object.insert("toolCall".into(), json!(value));
            }
            if let Some(value) = self.raw.get("structured_output").and_then(|v| v.as_bool()) {
                object.insert("structuredOutput".into(), json!(value));
            }
            if let Some(value) = self.raw.get("temperature").and_then(|v| v.as_bool()) {
                object.insert("temperature".into(), json!(value));
            }
            if let Some(value) = self.raw.get("open_weights").and_then(|v| v.as_bool()) {
                object.insert("openWeights".into(), json!(value));
            }
            for (from, to) in [
                ("release_date", "releaseDate"),
                ("last_updated", "lastUpdated"),
            ] {
                if let Some(value) = self.raw.get(from) {
                    object.insert(to.into(), value.clone());
                }
            }
            if let Some(limit) = self.raw.get("limit").and_then(|v| v.as_object()) {
                let mut normalized = Map::new();
                for (key, value) in limit {
                    let target = match key.as_str() {
                        "context" => "context",
                        "input" => "input",
                        "output" => "output",
                        other => other,
                    };
                    normalized.insert(target.to_string(), value.clone());
                }
                object.insert("limit".into(), Value::Object(normalized));
            }
        }
        info
    }

    fn to_model_config(&self, base_url: &str) -> Value {
        let (input, output) = self.modalities();
        let levels = self.thinking_levels();
        let mut config = json!({
            "source": "models.dev",
            "name": self.display_name(),
            "baseUrl": base_url,
            "reasoning": self.bool("reasoning").unwrap_or(false),
            "modalities": { "input": input, "output": output },
            "limit": {
                "context": self.context_window(),
                "input": self.context_window(),
                "output": self.max_tokens(),
            },
            "cost": self.cost(),
            "input": input
                .iter()
                .filter(|m| m.as_str() == "text" || m.as_str() == "image")
                .cloned()
                .collect::<Vec<_>>(),
            "contextWindow": self.context_window(),
            "maxTokens": self.max_tokens(),
            "supportedThinkingLevels": levels,
            "thinkingLevelMap": self.thinking_level_map(),
        });
        if let Value::Object(object) = &mut config {
            if let Some(value) = self.raw.get("api").and_then(|v| v.as_str()) {
                object.insert("api".into(), json!(value));
            }
            copy_field(&self.raw, object, "description");
            copy_field(&self.raw, object, "interleaved");
            copy_field(&self.raw, object, "experimental");
            if let Some(value) = self.raw.get("provider").cloned() {
                object.insert("catalogProvider".into(), value.clone());
                object.insert("provider".into(), value);
            }
            if let Some(value) = self.raw.get("tool_call").and_then(|v| v.as_bool()) {
                object.insert("toolCall".into(), json!(value));
            }
        }
        config
    }
}

fn copy_field(raw: &Value, object: &mut Map<String, Value>, key: &str) {
    if let Some(value) = raw.get(key) {
        object.insert(key.to_string(), value.clone());
    }
}

fn camel_case_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let camel = match key.as_str() {
                        "cache_read" => "cacheRead",
                        "cache_write" => "cacheWrite",
                        "input_audio" => "inputAudio",
                        "output_audio" => "outputAudio",
                        "context_over_200k" => "contextOver200k",
                        other => other,
                    };
                    (camel.to_string(), camel_case_value(value))
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(camel_case_value).collect()),
        other => other.clone(),
    }
}

fn string_list(value: Option<&Value>) -> Option<Vec<String>> {
    value?.as_array().map(|items| {
        items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect()
    })
}

fn push_level(levels: &mut Vec<String>, level: &str) {
    if !levels.iter().any(|existing| existing == level) {
        levels.push(level.to_string());
        levels.sort_by_key(|level| {
            CANONICAL_LEVELS
                .iter()
                .position(|canonical| canonical == level)
                .unwrap_or(CANONICAL_LEVELS.len())
        });
    }
}

fn normalize_url(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let mut parsed = match url::Url::parse(&with_scheme) {
        Ok(parsed) => parsed,
        Err(_) => return with_scheme.to_lowercase(),
    };
    parsed.set_fragment(None);
    parsed.set_query(None);
    let host = parsed.host_str().unwrap_or("").to_lowercase();
    let path = parsed.path().trim_end_matches('/').to_lowercase();
    format!("{host}{path}")
}

fn url_path(normalized: &str) -> &str {
    normalized
        .find('/')
        .map(|index| &normalized[index..])
        .unwrap_or("")
}

fn host_of(normalized: &str) -> &str {
    normalized
        .find('/')
        .map(|index| &normalized[..index])
        .unwrap_or(normalized)
}

/// modelIdsMatch port: namespaces, `@alias` suffixes, and vendor-prefixed ids.
fn model_ids_match(candidate: &str, requested: &str) -> bool {
    const VENDOR_PREFIXES: [&str; 20] = [
        "anthropic",
        "amazon",
        "aws",
        "cohere",
        "deepseek",
        "deepseek-ai",
        "gemini",
        "google",
        "meta",
        "minimax",
        "mistral",
        "moonshot",
        "moonshotai",
        "openai",
        "qwen",
        "z-ai",
        "zai",
        "zhipuai",
        "x-ai",
        "xai",
    ];
    let left = candidate.trim().to_lowercase();
    let right = requested.trim().to_lowercase();
    if left.is_empty() || right.is_empty() {
        return false;
    }
    if left == right {
        return true;
    }
    if left.ends_with(&format!("/{right}")) || right.ends_with(&format!("/{left}")) {
        return true;
    }
    if left.starts_with(&format!("{right}@")) || right.starts_with(&format!("{left}@")) {
        return true;
    }
    for separator in ["-", "."] {
        for (base, other) in [(&left, &right), (&right, &left)] {
            if let Some(index) = base.find(&format!("{separator}{other}")) {
                let prefix = &base[..index];
                if VENDOR_PREFIXES.contains(&prefix) {
                    return true;
                }
            }
        }
    }
    false
}

#[derive(Debug)]
struct Snapshot {
    providers: HashSet<String>,
    models_by_provider: HashMap<String, Vec<Arc<ModelEntry>>>,
    models_by_id: HashMap<String, Vec<Arc<ModelEntry>>>,
    provider_urls: Vec<(String, String)>,
    model_count: usize,
}

impl Snapshot {
    fn from_document(document: &Value) -> Self {
        let Some(root) = document.as_object() else {
            return Self::empty();
        };
        let mut providers = HashSet::new();
        let mut models_by_provider = HashMap::new();
        let mut models_by_id: HashMap<String, Vec<Arc<ModelEntry>>> = HashMap::new();
        let mut provider_urls = Vec::new();
        let mut model_count = 0;
        for (key, raw_provider) in root {
            let Some(provider_object) = raw_provider.as_object() else {
                continue;
            };
            providers.insert(key.clone());
            if let Some(api) = provider_object.get("api").and_then(|v| v.as_str()) {
                let normalized = normalize_url(api);
                if !normalized.is_empty() {
                    provider_urls.push((key.clone(), normalized));
                }
            }
            let mut entries = Vec::new();
            if let Some(models) = provider_object.get("models").and_then(|v| v.as_object()) {
                for (model_key, raw_model) in models {
                    let Some(model_object) = raw_model.as_object() else {
                        continue;
                    };
                    let id = model_object
                        .get("id")
                        .and_then(|v| v.as_str())
                        .filter(|id| !id.trim().is_empty())
                        .unwrap_or(model_key)
                        .to_string();
                    let entry = Arc::new(ModelEntry {
                        provider_key: key.clone(),
                        id,
                        raw: raw_model.clone(),
                    });
                    models_by_id
                        .entry(entry.lower_id())
                        .or_default()
                        .push(entry.clone());
                    entries.push(entry);
                    model_count += 1;
                }
            }
            models_by_provider.insert(key.clone(), entries);
        }
        Self {
            providers,
            models_by_provider,
            models_by_id,
            provider_urls,
            model_count,
        }
    }

    fn empty() -> Self {
        Self {
            providers: HashSet::new(),
            models_by_provider: HashMap::new(),
            models_by_id: HashMap::new(),
            provider_urls: Vec::new(),
            model_count: 0,
        }
    }

    fn resolve_provider(&self, vendor_key: &str, base_url: &str) -> Option<String> {
        let vendor_key = vendor_key.trim().to_lowercase();
        let mut candidates: Vec<&str> = Vec::new();
        if !vendor_key.is_empty() {
            if self.providers.contains(&vendor_key) {
                return Some(vendor_key);
            }
            candidates.extend(vendor_aliases(&vendor_key));
        }
        let normalized = normalize_url(base_url);
        if !normalized.is_empty() {
            if let Some((key, _)) = self
                .provider_urls
                .iter()
                .find(|(_, api)| api == &normalized)
            {
                return Some(key.clone());
            }
            // Same host, different path: prefer the longest shared path suffix.
            let same_host: Vec<&(String, String)> = self
                .provider_urls
                .iter()
                .filter(|(_, api)| host_of(api) == host_of(&normalized))
                .collect();
            if let Some((key, _)) = same_host
                .iter()
                .max_by_key(|(_, api)| shared_suffix_len(url_path(api), url_path(&normalized)))
            {
                return Some((*key).clone());
            }
        }
        if let Some(host) = Some(host_of(&normalized)).filter(|host| !host.is_empty()) {
            if let Some(alias) = host_provider_alias(host) {
                if self.providers.contains(alias) {
                    return Some(alias.to_string());
                }
            }
        }
        // A vendor/name alias is accepted only when it resolves to one row.
        candidates
            .into_iter()
            .find(|alias| self.providers.contains(*alias))
            .map(str::to_string)
    }

    fn resolve_model(&self, provider_key: &str, model_id: &str) -> Option<Arc<ModelEntry>> {
        let requested = model_id.trim().to_lowercase();
        if requested.is_empty() {
            return None;
        }
        let entries = self.models_by_provider.get(provider_key)?;
        if let Some(entry) = entries.iter().find(|entry| entry.lower_id() == requested) {
            return Some(entry.clone());
        }
        if let Some(entry) = entries
            .iter()
            .find(|entry| model_ids_match(&entry.id, model_id))
        {
            return Some(entry.clone());
        }
        let bucket = self.models_by_id.get(&requested)?;
        if bucket.len() == 1 {
            return bucket.first().cloned();
        }
        bucket
            .iter()
            .find(|entry| entry.provider_key == provider_key)
            .cloned()
    }
}

/// Official hosts whose catalog provider publishes no `api` URL field, so an
/// imported/custom row still resolves to the right metadata.
fn host_provider_alias(host: &str) -> Option<&'static str> {
    match host {
        "api.anthropic.com" => Some("anthropic"),
        "api.openai.com" => Some("openai"),
        "api.x.ai" => Some("xai"),
        "openrouter.ai" => Some("openrouter"),
        "generativelanguage.googleapis.com" => Some("google"),
        "api.deepseek.com" => Some("deepseek"),
        "api.moonshot.cn" => Some("moonshotai-cn"),
        "api.moonshot.ai" => Some("moonshotai"),
        "open.bigmodel.cn" => Some("zhipuai"),
        "api.z.ai" => Some("zai"),
        "api.minimaxi.com" => Some("minimax-cn"),
        "api.minimax.io" => Some("minimax"),
        _ => None,
    }
}

fn vendor_aliases(vendor_key: &str) -> Vec<&'static str> {
    match vendor_key {
        "openai-codex" => vec!["openai"],
        "kimi-coding" => vec![
            "kimi-code-plan-global",
            "kimi-code-plan-cn",
            "moonshotai",
            "moonshotai-cn",
        ],
        "github-copilot" => vec!["github-copilot"],
        "openrouter" => vec!["openrouter"],
        "xai" => vec!["xai"],
        "anthropic" => vec!["anthropic"],
        _ => Vec::new(),
    }
}

fn shared_suffix_len(left: &str, right: &str) -> usize {
    let mut length = 0;
    let mut left_parts = left.split('/').rev();
    let mut right_parts = right.split('/').rev();
    loop {
        match (left_parts.next(), right_parts.next()) {
            (Some(a), Some(b)) if a == b && !a.is_empty() => length += a.len(),
            _ => break,
        }
    }
    length
}

fn parse_document(text: &str) -> Result<Value, String> {
    let document: Value =
        serde_json::from_str(text).map_err(|e| format!("catalog parse failed: {e}"))?;
    if !document.is_object() {
        return Err("catalog must be a JSON object".into());
    }
    Ok(document)
}

fn read_catalog_file(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))
}

pub fn bundled_catalog_path() -> PathBuf {
    if let Some(path) = std::env::var_os("PI_DESKTOP_MODELS_DEV_PATH") {
        return PathBuf::from(path);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            if let Some(contents) = exe_dir.parent() {
                let packaged = contents
                    .join("Resources")
                    .join("models.dev")
                    .join("api.json");
                if packaged.exists() {
                    return packaged;
                }
            }
            let beside_exe = exe_dir.join("models.dev").join("api.json");
            if beside_exe.exists() {
                return beside_exe;
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("resources")
        .join("models.dev")
        .join("api.json")
}

struct CatalogState {
    snapshot: Option<Arc<Snapshot>>,
    source: &'static str,
    fetched_at: Option<String>,
    last_error: Option<String>,
}

pub struct Catalog {
    state: RwLock<CatalogState>,
    load_lock: Mutex<()>,
    path: PathBuf,
}

impl Catalog {
    pub fn new() -> Self {
        Self::with_path(bundled_catalog_path())
    }

    fn with_path(path: PathBuf) -> Self {
        Self {
            state: RwLock::new(CatalogState {
                snapshot: None,
                source: "empty",
                fetched_at: None,
                last_error: None,
            }),
            load_lock: Mutex::new(()),
            path,
        }
    }

    /// Read the bundled release snapshot once. No network I/O happens here.
    pub fn load_bundled(&self) -> bool {
        let _guard = self.load_lock.lock().unwrap();
        let mut state = self.state.write().unwrap();
        if state.snapshot.is_some() {
            return true;
        }
        match read_catalog_file(&self.path).and_then(|text| parse_document(&text)) {
            Ok(document) => {
                let snapshot = Snapshot::from_document(&document);
                state.snapshot = Some(Arc::new(snapshot));
                state.source = "bundled";
                state.last_error = None;
                true
            }
            Err(error) => {
                state.last_error = Some(error);
                false
            }
        }
    }

    fn ensure_loaded(&self) -> Option<Arc<Snapshot>> {
        let snapshot = self.state.read().unwrap().snapshot.clone();
        if snapshot.is_some() {
            return snapshot;
        }
        self.load_bundled();
        self.state.read().unwrap().snapshot.clone()
    }

    /// Settings refresh: fetch the fixed URL and replace the in-memory
    /// snapshot for this process. A failure keeps the previous snapshot.
    pub fn refresh(&self) -> bool {
        let _guard = self.load_lock.lock().unwrap();
        let agent = ureq::AgentBuilder::new().timeout(FETCH_TIMEOUT).build();
        let fetched = agent
            .get(MODELS_DEV_API_URL)
            .set("Accept", "application/json")
            .call()
            .map_err(|e| format!("catalog request failed: {e}"))
            .and_then(|response| {
                response
                    .into_string()
                    .map_err(|e| format!("catalog body read failed: {e}"))
            })
            .and_then(|text| parse_document(&text));
        let mut state = self.state.write().unwrap();
        match fetched {
            Ok(document) => {
                state.snapshot = Some(Arc::new(Snapshot::from_document(&document)));
                state.source = "remote";
                state.fetched_at =
                    Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
                state.last_error = None;
                true
            }
            Err(error) => {
                state.last_error = Some(error);
                false
            }
        }
    }

    pub fn status(&self) -> Value {
        let state = self.state.read().unwrap();
        let (provider_count, model_count) = state
            .snapshot
            .as_ref()
            .map(|snapshot| (snapshot.providers.len(), snapshot.model_count))
            .unwrap_or((0, 0));
        json!({
            "loaded": state.snapshot.is_some(),
            "source": state.source,
            "catalogPath": self.path.to_string_lossy(),
            "fetchedAt": state.fetched_at,
            "providerCount": provider_count,
            "modelCount": model_count,
            "lastError": state.last_error,
        })
    }

    /// Catalog provider identity for an endpoint alone (used by the importer to
    /// name a matching preset without guessing).
    pub fn provider_key_for_url(&self, base_url: &str) -> Option<String> {
        let snapshot = self.ensure_loaded()?;
        snapshot.resolve_provider("", base_url)
    }

    /// Find one published record for a configured provider/model pair.
    pub fn find_model(
        &self,
        vendor_key: &str,
        base_url: &str,
        model_id: &str,
    ) -> Option<ModelMatch> {
        let snapshot = self.ensure_loaded()?;
        let provider_key = snapshot.resolve_provider(vendor_key, base_url)?;
        let entry = snapshot.resolve_model(&provider_key, model_id)?;
        Some(ModelMatch { entry })
    }

    /// Text-capable published models for a configured provider, newest first
    /// as the catalog publishes them.
    pub fn models_for_provider(&self, vendor_key: &str, base_url: &str) -> Vec<ModelMatch> {
        let Some(snapshot) = self.ensure_loaded() else {
            return Vec::new();
        };
        let Some(provider_key) = snapshot.resolve_provider(vendor_key, base_url) else {
            return Vec::new();
        };
        snapshot
            .models_by_provider
            .get(&provider_key)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|entry| entry.supports_text())
                    .map(|entry| ModelMatch {
                        entry: entry.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

pub struct ModelMatch {
    entry: Arc<ModelEntry>,
}

impl ModelMatch {
    pub fn model_id(&self) -> &str {
        &self.entry.id
    }

    pub fn display_name(&self) -> String {
        self.entry.display_name()
    }

    pub fn to_model_info(&self, provider_id: &str, source: &str) -> Value {
        self.entry.to_model_info(provider_id, source)
    }

    pub fn to_model_config(&self, base_url: &str) -> Value {
        self.entry.to_model_config(base_url)
    }

    /// A persisted binding seeded from the published record.
    pub fn to_binding(&self) -> Value {
        let levels = self.entry.thinking_levels();
        let default_level = levels
            .iter()
            .find(|level| level.as_str() == "medium")
            .or_else(|| levels.iter().find(|level| level.as_str() != "off"))
            .cloned()
            .unwrap_or_else(|| "off".to_string());
        json!({
            "id": self.entry.id,
            "contextWindow": self.entry.context_window(),
            "maxTokens": self.entry.max_tokens(),
            "thinkingLevels": levels,
            "defaultThinkingLevel": default_level,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog_from(fixture: Value) -> Catalog {
        let dir = std::env::temp_dir().join(format!("pi-catalog-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("api.json");
        std::fs::write(&path, fixture.to_string()).unwrap();
        let catalog = Catalog::with_path(path);
        assert!(catalog.load_bundled());
        catalog
    }

    fn fixture() -> Value {
        json!({
            "anthropic": {
                "name": "Anthropic",
                "models": {
                    "claude-opus-4.6": {
                        "id": "claude-opus-4.6",
                        "name": "Claude 4.6 Opus",
                        "family": "claude-opus",
                        "reasoning": true,
                        "reasoning_options": [{
                            "type": "effort",
                            "values": ["low", "medium", "high"]
                        }],
                        "tool_call": true,
                        "modalities": { "input": ["text", "image"], "output": ["text"] },
                        "limit": { "context": 200_000, "output": 32_000 },
                        "cost": { "input": 5, "output": 25, "cache_read": 0.5 }
                    }
                }
            },
            "openai": {
                "name": "OpenAI",
                "models": {
                    "gpt-5.6-sol": {
                        "id": "gpt-5.6-sol",
                        "name": "GPT-5.6 Sol",
                        "reasoning": true,
                        "modalities": { "input": ["text", "image"], "output": ["text"] },
                        "limit": { "context": 1_000_000, "output": 128_000 }
                    }
                }
            }
        })
    }

    #[test]
    fn finds_by_vendor_key_and_vendor_alias() {
        let catalog = catalog_from(fixture());
        let model = catalog
            .find_model("anthropic", "https://api.anthropic.com", "claude-opus-4.6")
            .expect("anthropic model");
        assert_eq!(model.model_id(), "claude-opus-4.6");
        assert_eq!(
            model.to_binding()["thinkingLevels"],
            json!(["low", "medium", "high"])
        );

        let codex = catalog
            .find_model(
                "openai-codex",
                "https://chatgpt.com/backend-api",
                "gpt-5.6-sol",
            )
            .expect("openai alias");
        assert_eq!(codex.model_id(), "gpt-5.6-sol");
    }

    #[test]
    fn status_reports_the_bundled_snapshot() {
        let catalog = catalog_from(fixture());
        let status = catalog.status();
        assert_eq!(status["loaded"], true);
        assert_eq!(status["source"], "bundled");
        assert_eq!(status["providerCount"], 2);
        assert_eq!(status["modelCount"], 2);
    }

    #[test]
    fn model_info_carries_published_capabilities() {
        let catalog = catalog_from(fixture());
        let model = catalog
            .find_model("anthropic", "", "claude-opus-4.6")
            .expect("model");
        let info = model.to_model_info("row", "bundled");
        assert_eq!(info["displayName"], "Claude 4.6 Opus");
        assert_eq!(info["capabilities"][0], "text");
        assert!(info["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|capability| capability == "vision"));
        assert_eq!(info["cost"]["cacheRead"], 0.5);
    }

    #[test]
    fn unknown_model_has_no_match() {
        let catalog = catalog_from(fixture());
        assert!(catalog.find_model("anthropic", "", "unpublished").is_none());
    }

    #[test]
    fn bundled_release_snapshot_parses() {
        let catalog = Catalog::new();
        assert!(
            catalog.load_bundled(),
            "the bundled models.dev snapshot is missing"
        );
        let status = catalog.status();
        assert!(status["providerCount"].as_u64().unwrap_or(0) > 100);
        assert!(status["modelCount"].as_u64().unwrap_or(0) > 1000);
        assert!(catalog
            .find_model("anthropic", "https://api.anthropic.com", "claude-opus-4-6")
            .is_some());
        assert!(catalog
            .find_model(
                "github-copilot",
                "https://api.individual.githubcopilot.com",
                "gpt-5.4"
            )
            .is_some());
    }
}
