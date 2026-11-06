//! Agent runtime launch, ported (reduced) from Electron
//! `runtime/session-launch.ts` `resolveAgentRuntimeLaunch`: pick provider +
//! model, resolve the API key, load project instructions, assemble the
//! sidecarParams for `agent.prompt`.
//!
//! Phase-2 reductions vs. the Electron original:
//! - MCP tools are not in `pluginTools` (the user MCP runtime is not ported);
//!   plugin agent tools are, from the Node plugin runtime.
//! - subagent catalog and pinned subagent providers are wired; the sidecar
//!   implements the `Task` tool against those definitions.
//! - OAuth vendor rows resolve their short-lived access token here; the
//!   sidecar may ask `provider.resolveAuth` for a fresh one per request.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::state::{AppState, PROTOCOL_VERSION};
use crate::{envelope, fsops};

const OAUTH_AUTH_KIND: &str = "oauth";

fn str_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
}

/// The wire API for the selected model: a per-model binding override wins over
/// the provider-wide format, so one endpoint can expose models on different
/// wires (for example an OAuth catalog that mixes OpenAI and Anthropic models).
pub(crate) fn effective_api_style(provider: &Value, model_id: &str) -> String {
    provider
        .get("models")
        .and_then(Value::as_array)
        .and_then(|models| {
            models
                .iter()
                .find(|binding| binding.get("id").and_then(Value::as_str) == Some(model_id))
        })
        .and_then(|binding| str_field(binding, "apiStyle"))
        .or_else(|| str_field(provider, "apiStyle"))
        .unwrap_or("")
        .to_string()
}

/// OpenCode Go / Zen gateways pin a chat to one backend by `x-opencode-session`,
/// which the upstream runtime does not send. Sessions and subagent streams run
/// in the sidecar, so the shell injects the routing headers at launch; the
/// one-shot path applies the same headers in `agent-runtime`.
pub(crate) fn opencode_session_headers(
    api_style: &str,
    vendor_key: &str,
    base_url: &str,
    headers: Option<&Value>,
    session_id: &str,
) -> Option<Value> {
    let session_id = session_id.trim();
    let eligible = api_style.eq_ignore_ascii_case("opencode_go")
        || matches!(
            vendor_key.trim().to_ascii_lowercase().as_str(),
            "opencode" | "opencode-go"
        )
        || url::Url::parse(base_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
            .map(|host| host == "opencode.ai" || host.ends_with(".opencode.ai"))
            .unwrap_or(false);
    if session_id.is_empty() || !eligible {
        return headers.cloned();
    }

    let mut merged = headers
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (name, value) in [
        ("x-opencode-session", session_id),
        ("x-opencode-client", "pi-desktop"),
        (
            "User-Agent",
            concat!("pi-desktop/", env!("CARGO_PKG_VERSION")),
        ),
    ] {
        match merged
            .iter_mut()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
        {
            // A caller value wins unless it is empty, which the gateway rejects.
            Some((_, existing)) if existing.as_str().is_some_and(|v| !v.trim().is_empty()) => {}
            Some((_, existing)) => *existing = json!(value),
            None => {
                merged.insert(name.to_string(), json!(value));
            }
        }
    }
    Some(Value::Object(merged))
}

fn generic_model_config(model_id: &str, base_url: &str) -> Value {
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
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn clamp_thinking(
    requested: Option<&str>,
    supported: &[String],
    binding_default: Option<&str>,
) -> String {
    if supported.is_empty() {
        return "off".into();
    }
    let requested = requested.unwrap_or("");
    if supported.iter().any(|level| level == requested) {
        return requested.to_string();
    }
    if let Some(default) =
        binding_default.filter(|level| supported.iter().any(|item| item == level))
    {
        return default.to_string();
    }
    supported
        .iter()
        .find(|level| level.as_str() != "off")
        .cloned()
        .unwrap_or_else(|| "off".into())
}

/// `~/.pi/agent/AGENTS.md` (mirrors `globalInstructionPath` in agent-runtime).
fn global_instruction_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".pi")
        .join("agent")
        .join("AGENTS.md")
}

async fn load_project_instructions(
    state: &AppState,
    project_path: Option<&str>,
) -> (Value, Option<String>) {
    let mut entries: Vec<Value> = Vec::new();
    let mut project_memory: Option<String> = None;
    let Some(project_path) = project_path else {
        return (json!({ "entries": entries }), None);
    };
    // Instruction chain (simplified): the global file plus the project's own
    // AGENTS.md. The Electron chain also walks ancestor directories.
    if let Some(content) = fsops::read_if_exists(&global_instruction_path()) {
        if !content.trim().is_empty() {
            entries.push(json!({ "source": "Global instructions", "content": content }));
        }
    }
    let agents_md = PathBuf::from(project_path).join("AGENTS.md");
    if let Some(content) = fsops::read_if_exists(&agents_md) {
        if !content.trim().is_empty() {
            entries.push(json!({ "source": "AGENTS.md", "content": content }));
        }
    }
    // Group context enrichment, best effort — it must never block a launch.
    if let Ok(context) = state
        .host
        .call("project.group.context", json!({ "path": project_path }))
        .await
    {
        if context.get("context").is_some_and(|c| !c.is_null()) {
            let context = &context["context"];
            let roots: Vec<&str> = context
                .get("roots")
                .and_then(|r| r.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|r| r.get("path").and_then(|p| p.as_str()))
                        .collect()
                })
                .unwrap_or_default();
            if roots.len() > 1 {
                let mut guide = format!("Primary root: {}", roots[0]);
                for root in &roots[1..] {
                    guide.push_str(&format!("\nAdditional root: {root}"));
                }
                guide
                    .push_str("\nUse an absolute path when reading or editing an additional root.");
                entries.push(json!({ "source": "ChatGPT Project folders", "content": guide }));
            }
            if let Some(instructions) = str_field(context, "instructions") {
                entries.push(json!({
                    "source": "ChatGPT Project instructions",
                    "content": instructions,
                }));
            }
            if let Some(memory) = context
                .get("memory")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .filter(|c| !c.trim().is_empty())
            {
                project_memory = Some(memory.to_string());
            }
        } else if let Ok(legacy) = state
            .host
            .call("project.memory.get", json!({ "path": project_path }))
            .await
        {
            if let Some(content) = legacy
                .get("memory")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .filter(|c| !c.trim().is_empty())
            {
                project_memory = Some(content.to_string());
            }
        }
    }
    (json!({ "entries": entries }), project_memory)
}

pub struct Launch {
    pub provider_id: String,
    pub model_id: String,
    pub sidecar_params: Value,
}

pub async fn resolve_launch(
    state: &Arc<AppState>,
    session: &Value,
    settings: &Value,
    overrides: &Value,
) -> Result<Launch, Value> {
    // Command shell (required — the sidecar executes Bash through it).
    let catalog = state
        .host
        .call("commandShells.list", json!({}))
        .await
        .map_err(|e| envelope::envelope_from_rpc("host", &e))?;
    let effective = catalog.get("effective").cloned().unwrap_or(Value::Null);
    let available = effective
        .get("available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if effective.is_null() || !available {
        return Err(envelope::err(
            "SHELL_NOT_FOUND",
            "No available command shell is configured for this session",
        ));
    }

    // Provider selection: session → default → first usable row.
    let providers = state
        .host
        .call("providers.list", json!({ "includeDisabled": false }))
        .await
        .map_err(|e| envelope::envelope_from_rpc("host", &e))?;
    let rows = providers
        .get("providers")
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();
    let requested_provider_id = overrides
        .get("providerId")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| str_field(session, "providerId").map(String::from));
    let default_provider_id = str_field(settings, "defaultProviderId");
    let usable = |row: &Value| {
        row.get("hasSecret")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            || row
                .get("hasOauth")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            || str_field(row, "authKind") == Some("none")
    };
    let provider = requested_provider_id
        .as_deref()
        .and_then(|id| {
            rows.iter()
                .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(id))
        })
        .or_else(|| {
            default_provider_id.and_then(|id| {
                rows.iter()
                    .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(id))
            })
        })
        .or_else(|| rows.iter().find(|r| usable(r)))
        .or_else(|| rows.first());
    let Some(provider) = provider else {
        return Err(envelope::err(
            "MODEL_NOT_CONFIGURED",
            "No provider configured",
        ));
    };
    let provider_id = str_field(provider, "id").unwrap_or("").to_string();

    // Credential: a vendor row resolves a fresh access token (never the
    // refresh token); an API-key row reads its stored secret. A staff run
    // carries its own resolved key, which can come from a credential profile
    // rather than the provider row, so that override wins when present.
    let auth_kind = str_field(provider, "authKind").unwrap_or("");
    let is_vendor = auth_kind == OAUTH_AUTH_KIND;
    let mut api_key = String::new();
    if let Some(key) = overrides
        .get("apiKey")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
    {
        api_key = key.to_string();
    } else if is_vendor {
        api_key = crate::oauth::resolve_access_token(state, &provider_id)
            .await
            .map_err(|message| envelope::err("PROVIDER_NOT_BOUND", message))?;
    } else if auth_kind != "none" {
        let secret = state
            .host
            .call("providers.getSecret", json!({ "id": provider_id }))
            .await
            .map_err(|e| envelope::envelope_from_rpc("host", &e))?;
        api_key = str_field(&secret, "value").unwrap_or("").to_string();
        if api_key.is_empty() {
            return Err(envelope::err(
                "PROVIDER_SECRET_MISSING",
                "Provider API key missing",
            ));
        }
    }

    // Model selection: override → session → default → provider first/default.
    let model_id = overrides
        .get("modelId")
        .and_then(|v| v.as_str())
        .filter(|_| Some(provider_id.as_str()) == requested_provider_id.as_deref())
        .map(String::from)
        .or_else(|| {
            str_field(session, "modelId")
                .filter(|_| Some(provider_id.as_str()) == requested_provider_id.as_deref())
                .map(String::from)
        })
        .or_else(|| {
            (Some(provider_id.as_str()) == default_provider_id)
                .then(|| str_field(settings, "defaultModelId").map(String::from))
                .flatten()
        })
        .or_else(|| {
            provider
                .get("models")
                .and_then(|m| m.as_array())
                .and_then(|m| m.first())
                .and_then(|m| m.get("id"))
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .or_else(|| str_field(provider, "defaultModelId").map(String::from));
    let Some(model_id) = model_id else {
        return Err(envelope::err(
            "MODEL_NOT_CONFIGURED",
            "No model selected for provider",
        ));
    };

    let base_url = overrides
        .get("baseUrl")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| str_field(provider, "baseUrl").unwrap_or("").to_string());
    let api_style = effective_api_style(provider, &model_id);
    let vendor_key = str_field(provider, "vendorKey").unwrap_or("custom");
    let mut model_config = state
        .catalog
        .find_model(vendor_key, &base_url, &model_id)
        .map(|model| model.to_model_config(&base_url))
        .unwrap_or_else(|| generic_model_config(&model_id, &base_url));
    // Provider overrides win over published limits.
    if let Some(value) = provider.get("contextWindow").and_then(|v| v.as_u64()) {
        model_config["contextWindow"] = json!(value);
        model_config["limit"]["context"] = json!(value);
        model_config["limit"]["input"] = json!(value);
    }
    if let Some(value) = provider.get("maxOutputTokens").and_then(|v| v.as_u64()) {
        model_config["maxTokens"] = json!(value);
        model_config["limit"]["output"] = json!(value);
    }

    let supports_reasoning = provider
        .get("supportsReasoning")
        .and_then(|v| v.as_bool())
        .or_else(|| model_config.get("reasoning").and_then(|v| v.as_bool()))
        .unwrap_or(false);
    let supported_levels = {
        let provider_levels = string_array(provider.get("supportedThinkingLevels"));
        if provider_levels.is_empty() {
            string_array(model_config.get("supportedThinkingLevels"))
        } else {
            provider_levels
        }
    };
    let binding_default = provider
        .get("models")
        .and_then(|models| models.as_array())
        .and_then(|models| {
            models.iter().find(|binding| {
                binding.get("id").and_then(|v| v.as_str()) == Some(model_id.as_str())
            })
        })
        .and_then(|binding| binding.get("defaultThinkingLevel"))
        .and_then(|v| v.as_str());
    let requested_thinking = overrides
        .get("thinkingLevel")
        .and_then(|v| v.as_str())
        .or_else(|| str_field(session, "thinkingLevel"))
        .or_else(|| str_field(settings, "defaultThinkingLevel"));
    let thinking_level = clamp_thinking(requested_thinking, &supported_levels, binding_default);
    let supports_vision = model_config
        .pointer("/input")
        .and_then(|v| v.as_array())
        .map(|input| input.iter().any(|item| item.as_str() == Some("image")))
        .unwrap_or(false);
    // A vendor row may carry account-specific outbound headers (Copilot's
    // integration headers, Anthropic's OAuth beta header).
    let vendor_headers = provider.get("headers").cloned().filter(|value| {
        value
            .as_object()
            .map(|map| !map.is_empty())
            .unwrap_or(false)
    });

    let project_path = str_field(session, "projectPath").map(String::from);
    let (project_instructions, project_memory) =
        load_project_instructions(state, project_path.as_deref()).await;

    // User skills cross as a catalog; the sidecar's local `Skill` tool loads a
    // body on demand from the absolute path the host resolved.
    let mut plugin_skills: Vec<Value> = Vec::new();
    if let Ok(active) = state
        .host
        .call(
            "skills.active",
            json!({ "projectPath": project_path.clone().map(Value::String).unwrap_or(Value::Null) }),
        )
        .await
    {
        if let Some(skills) = active.get("skills").and_then(|s| s.as_array()) {
            for skill in skills {
                if let (Some(id), Some(name)) =
                    (str_field(skill, "id"), str_field(skill, "name"))
                {
                    plugin_skills.push(json!({
                        "id": id,
                        "name": name,
                        "description": str_field(skill, "description").unwrap_or(""),
                        "path": str_field(skill, "path").unwrap_or(""),
                    }));
                }
            }
        }
    }

    let session_id = session
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let scratch_dir = state.scratch_dir().join(&session_id);
    let provider_headers = opencode_session_headers(
        &api_style,
        provider.get("vendorKey").and_then(Value::as_str).unwrap_or(""),
        &base_url,
        vendor_headers.as_ref(),
        &session_id,
    );

    // Plugin tools travel with every prompt: reconcile the runtime against
    // this project first so a disabled or project-scoped plugin never
    // advertises a schema its process cannot execute.
    let mut plugin_tools =
        crate::plugin_host::tool_definitions(state, project_path.as_deref()).await;
    // Agent-extension entry files travel with the prompt for a sidecar that
    // loads them. The Go agent loop does not consume them yet; the shell's own
    // extension host is what executes their commands in this port.
    let trusted_extensions = state.plugins.trusted_extensions();
    // User-owned MCP tools share the desktop-dispatched namespace and the same
    // host-core permission gate; a server that cannot connect is logged and
    // stays out of this prompt instead of failing the launch.
    plugin_tools.extend(state.mcp.definitions(state, project_path.as_deref()).await);

    let mode = overrides
        .get("mode")
        .and_then(|v| v.as_str())
        .or_else(|| str_field(session, "mode"))
        .or_else(|| str_field(settings, "defaultMode"))
        .unwrap_or("agent")
        .to_string();
    let mut sidecar_params = json!({
        "sessionId": session_id,
        "mode": mode,
        "thinkingLevel": thinking_level,
        "commandShell": effective,
        "scratchDir": scratch_dir.to_string_lossy(),
        "attachmentsDir": state.attachments_dir().to_string_lossy(),
        "projectInstructions": project_instructions,
        "provider": {
            "id": provider_id,
            "name": str_field(provider, "name").unwrap_or(""),
            "baseUrl": base_url,
            "modelId": model_id,
            "apiKey": api_key,
            "authKind": auth_kind,
            "apiStyle": api_style,
            "supportsReasoning": supports_reasoning,
            "supportsVision": supports_vision,
            "supportedThinkingLevels": supported_levels,
            "modelConfig": model_config,
        },
        "pluginTools": plugin_tools,
        "pluginSkills": plugin_skills,
        "trustedExtensions": trusted_extensions,
        "subagents": [],
        "subagentProviders": {},
        "subagentModelKeys": [],
    });

    // Subagent catalog: builtins plus the user's enabled global documents.
    // Definitions travel with every prompt so an edited document reaches the
    // next turn; a pin that cannot be resolved stays out of the provider map
    // so the sidecar reports the named pin instead of silently spending the
    // session model.
    let mut subagent_catalog = crate::subagents::load_catalog(state, project_path.as_deref()).await;
    let (resolved_subagents, pin_diagnostics) =
        crate::subagents::resolve_pinned_providers(state, &subagent_catalog.definitions, &rows)
            .await;
    subagent_catalog.diagnostics.extend(pin_diagnostics);
    sidecar_params["subagents"] = json!(subagent_catalog.definitions);
    sidecar_params["subagentProviders"] = resolved_subagents.providers;
    // Pinned subagent rows reach the same OpenCode gateways, so they carry the
    // same session routing headers as the session provider.
    if let Some(providers) = sidecar_params["subagentProviders"].as_object_mut() {
        for config in providers.values_mut() {
            let style = config
                .get("apiStyle")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let vendor = config
                .get("vendorKey")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let base = config
                .get("baseUrl")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if let Some(headers) = opencode_session_headers(
                &style,
                &vendor,
                &base,
                config.get("headers"),
                &session_id,
            ) {
                config["headers"] = headers;
            }
        }
    }
    sidecar_params["subagentModelKeys"] = json!(resolved_subagents.model_keys);
    // Optional strings: never serialize null — strict serde peers treat a
    // null for a non-Option field as a decode error.
    if let Some(vendor_key) = provider.get("vendorKey").and_then(|v| v.as_str()) {
        sidecar_params["provider"]["vendorKey"] = json!(vendor_key);
    }
    if let Some(headers) = provider_headers {
        sidecar_params["provider"]["headers"] = headers;
    }
    // Vendor tokens are resolved per request by the sidecar, so the binding
    // table carries every OAuth row this session may ask about: its own vendor
    // row plus any OAuth account a subagent definition pins.
    let mut bound_oauth: Vec<String> = Vec::new();
    if is_vendor {
        bound_oauth.push(provider_id.clone());
    }
    for id in resolved_subagents.oauth_provider_ids {
        if !bound_oauth.contains(&id) {
            bound_oauth.push(id);
        }
    }
    if bound_oauth.is_empty() {
        state.provider_bindings.lock().unwrap().remove(&session_id);
    } else {
        state
            .provider_bindings
            .lock()
            .unwrap()
            .insert(session_id.clone(), bound_oauth);
    }
    if let Some(project_path) = &project_path {
        sidecar_params["projectPath"] = json!(project_path);
    }
    if let Some(project_memory) = &project_memory {
        sidecar_params["projectMemory"] = json!(project_memory);
    }

    Ok(Launch {
        provider_id,
        model_id,
        sidecar_params,
    })
}

/// Electron's main wrapped the sidecar health answer in `engineInfo()`, and the
/// About row prints `${impl} sidecar`. The sidecar reports facts only (ok /
/// version / runtimes), so the shell names the engine here instead of adding
/// fields to the frozen sidecar protocol — without this the row reads
/// "undefined sidecar".
fn with_engine_identity(mut health: Value, binary: &std::path::Path) -> Value {
    if let Some(object) = health.as_object_mut() {
        object.insert("engine".into(), json!("pigo"));
        object.insert("impl".into(), json!("go"));
        if let Some(name) = binary.file_name() {
            object.insert("binary".into(), json!(name.to_string_lossy()));
        }
    }
    health
}

/// The `appGetVersion` composition (host version + engine identity).
pub async fn app_version(state: &AppState) -> Value {
    let host_version = state.host.call("app.getVersion", json!({})).await.ok();
    let agent = state
        .sidecar
        .call_timeout(
            "sidecar.health",
            json!({}),
            std::time::Duration::from_secs(2),
        )
        .await
        .ok()
        .map(|health| with_engine_identity(health, &state.sidecar.binary));
    json!({
        "name": crate::state::APP_NAME,
        "version": crate::state::APP_VERSION,
        "protocolVersion": PROTOCOL_VERSION,
        "hostProtocolVersion": host_version.as_ref().and_then(|v| v.get("protocolVersion")).cloned(),
        "hostVersion": host_version.as_ref().and_then(|v| v.get("version")).cloned(),
        "platform": std::env::consts::OS
            .replace("macos", "darwin")
            .replace("windows", "win32"),
        "arch": match std::env::consts::ARCH {
            "x86_64" => "x64",
            "aarch64" => "arm64",
            other => other,
        },
        "agent": agent,
    })
}

#[cfg(test)]
mod tests {
    use super::{effective_api_style, opencode_session_headers, with_engine_identity};
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn opencode_rows_get_session_routing_headers() {
        for (style, vendor, base) in [
            ("opencode_go", "", ""),
            ("chat_completions", "opencode", ""),
            ("openai", "opencode-go", ""),
            ("openai", "", "https://opencode.ai/zen/go/v1"),
            ("openai", "", "https://api.opencode.ai/v1"),
        ] {
            let headers =
                opencode_session_headers(style, vendor, base, None, " session-1 ").unwrap();
            assert_eq!(headers["x-opencode-session"], "session-1");
            assert_eq!(headers["x-opencode-client"], "pi-desktop");
            assert_eq!(headers["User-Agent"], concat!("pi-desktop/", env!("CARGO_PKG_VERSION")));
        }
    }

    #[test]
    fn other_providers_and_sessions_without_an_id_are_left_alone() {
        let existing = json!({ "X-Test": "1" });
        assert_eq!(
            opencode_session_headers("openai", "", "https://api.example.com", Some(&existing), "s"),
            Some(existing.clone())
        );
        assert_eq!(
            opencode_session_headers("opencode_go", "", "", Some(&existing), "  "),
            Some(existing)
        );
        assert_eq!(opencode_session_headers("openai", "", "", None, "s"), None);
    }

    #[test]
    fn caller_header_values_win_unless_empty() {
        let headers = json!({ "X-Opencode-Session": "caller", "x-opencode-client": "" });
        let merged = opencode_session_headers(
            "opencode_go",
            "",
            "",
            Some(&headers),
            "session-2",
        )
        .unwrap();
        assert_eq!(merged["X-Opencode-Session"], "caller");
        assert_eq!(merged["x-opencode-client"], "pi-desktop");
        // No second, differently-cased copy of the caller's session header.
        assert_eq!(merged["x-opencode-session"], serde_json::Value::Null);
    }

    #[test]
    fn a_model_binding_can_override_the_provider_api_style() {
        let provider = json!({
            "apiStyle": "chat_completions",
            "models": [
                { "id": "claude", "apiStyle": "anthropic_messages" },
                { "id": "gpt" }
            ]
        });
        assert_eq!(
            effective_api_style(&provider, "claude"),
            "anthropic_messages"
        );
        assert_eq!(effective_api_style(&provider, "gpt"), "chat_completions");
        assert_eq!(effective_api_style(&json!({}), "x"), "");
    }

    #[test]
    fn the_about_row_names_the_pigo_engine_and_sidecar_binary() {
        let health = json!({ "ok": true, "version": "0.1.0", "runtimes": 0 });
        let composed = with_engine_identity(health, Path::new("/opt/bin/pi-go-desktop-agent"));
        assert_eq!(composed["engine"], "pigo");
        assert_eq!(composed["impl"], "go");
        assert_eq!(composed["binary"], "pi-go-desktop-agent");
        // The health facts survive; nothing the sidecar reported is dropped.
        assert_eq!(composed["version"], "0.1.0");
        assert_eq!(composed["ok"], true);
    }
}
