//! Provider model listing, ported (reduced) from Electron
//! `ipc/provider-ipc.ts` `providersListModels` + `model-discovery.ts`:
//! cache path via host-core, live discovery over the provider's /models
//! endpoint (vendor-account rows read their account catalog instead), and
//! models.dev metadata decoration (ADR 0134).

use std::time::Duration;

use serde_json::{json, Value};

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
const OAUTH_AUTH_KIND: &str = "oauth";

fn str_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
}

/// Generic metadata row when the catalog has no record for the model. A model
/// absent from models.dev stays runnable with conservative text defaults
/// (ADR 0134 §6).
fn generic_model_info(
    model_id: &str,
    display_name: &str,
    provider_id: &str,
    source: &str,
) -> Value {
    json!({
        "modelId": model_id,
        "displayName": if display_name.trim().is_empty() { model_id } else { display_name },
        "providerId": provider_id,
        "modalities": { "input": ["text"], "output": ["text"] },
        "reasoning": false,
        "capabilities": ["text"],
        "supportedThinkingLevels": [],
        "source": source,
        "contextWindow": 128_000,
        "maxTokens": 8_192,
    })
}

/// One ModelInfo row: published models.dev metadata when available, the
/// generic shape otherwise.
fn decorate(
    state: &crate::state::AppState,
    model_id: &str,
    display_name: &str,
    provider_id: &str,
    vendor_key: &str,
    base_url: &str,
    source: &str,
) -> Value {
    if let Some(model) = state.catalog.find_model(vendor_key, base_url, model_id) {
        return model.to_model_info(provider_id, source);
    }
    generic_model_info(model_id, display_name, provider_id, source)
}

/// `modelListRequest` port: endpoint + auth headers per wire API style.
fn model_list_request(
    base_url: &str,
    api_key: &str,
    api_style: &str,
    custom_headers: Option<&Value>,
) -> (String, Vec<(String, String)>) {
    let base = base_url.trim().trim_end_matches('/').to_string();
    let mut headers: Vec<(String, String)> = Vec::new();
    let url;
    match api_style {
        "google_generative_ai" => {
            url = format!("{base}/models?pageSize=1000{}", {
                if api_key.is_empty() {
                    String::new()
                } else {
                    format!("&key={api_key}")
                }
            });
        }
        "anthropic_messages" => {
            let root = if base.ends_with("/v1") {
                base.clone()
            } else {
                format!("{base}/v1")
            };
            url = format!("{root}/models?limit=1000");
            if !api_key.is_empty() {
                headers.push(("x-api-key".into(), api_key.to_string()));
            }
            headers.push(("anthropic-version".into(), "2023-06-01".into()));
        }
        _ => {
            url = format!("{base}/models");
            if !api_key.is_empty() {
                headers.push(("Authorization".into(), format!("Bearer {api_key}")));
            }
        }
    }
    if let Some(Value::Object(map)) = custom_headers {
        for (key, value) in map {
            if let Some(v) = value.as_str() {
                headers.push((key.clone(), v.to_string()));
            }
        }
    }
    (url, headers)
}

/// `normalizeModelList` port: google / openai-style {data:[…]} / bare array.
fn normalize_model_list(api_style: &str, body: &Value) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push = |model_id: &str, display_name: &str| {
        if model_id.is_empty() {
            return;
        }
        if out.iter().any(|(id, _)| id == model_id) {
            return;
        }
        let display = if display_name.trim().is_empty() {
            model_id
        } else {
            display_name
        };
        out.push((model_id.to_string(), display.to_string()));
    };
    if api_style == "google_generative_ai" {
        if let Some(models) = body.get("models").and_then(|m| m.as_array()) {
            for entry in models {
                let raw = str_field(entry, "name").unwrap_or("");
                let id = raw.strip_prefix("models/").unwrap_or(raw);
                push(id, str_field(entry, "displayName").unwrap_or(""));
            }
        }
        return out;
    }
    let data = body
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| body.as_array())
        .cloned()
        .unwrap_or_default();
    for entry in data {
        push(
            str_field(&entry, "id").unwrap_or(""),
            str_field(&entry, "display_name").unwrap_or(""),
        );
    }
    out
}

fn http_get(url: &str, headers: &[(String, String)]) -> Result<Value, String> {
    let agent = ureq::AgentBuilder::new().timeout(DISCOVERY_TIMEOUT).build();
    let mut request = agent.get(url);
    for (key, value) in headers {
        request = request.set(key, value);
    }
    let response = request
        .call()
        .map_err(|e| format!("model list request failed: {e}"))?;
    let text = response
        .into_string()
        .map_err(|e| format!("model list body read failed: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("model list response parse failed: {e}"))
}

pub async fn list_models(state: &crate::state::AppState, params: &Value) -> Value {
    let host = &state.host;
    // Request shape: `string` providerId or an options object.
    let (req_provider_id, req_base_url, req_api_key, req_api_style, source) =
        if let Some(id) = params.as_str() {
            (
                id.to_string(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            )
        } else {
            (
                str_field(params, "providerId").unwrap_or("").to_string(),
                str_field(params, "baseUrl").unwrap_or("").to_string(),
                str_field(params, "apiKey").unwrap_or("").to_string(),
                str_field(params, "apiStyle").unwrap_or("").to_string(),
                str_field(params, "source").unwrap_or("").to_string(),
            )
        };

    let providers = match host
        .call("providers.list", json!({ "includeDisabled": true }))
        .await
    {
        Ok(p) => p
            .get("providers")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default(),
        Err(e) => return crate::envelope::envelope_from_rpc("host", &e),
    };
    let provider = providers
        .iter()
        .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(req_provider_id.as_str()));
    let provider_id = provider
        .and_then(|p| p.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let base_url = if !req_base_url.is_empty() {
        req_base_url.clone()
    } else {
        provider
            .and_then(|p| str_field(p, "baseUrl"))
            .unwrap_or("")
            .to_string()
    };
    let api_style = if !req_api_style.is_empty() {
        req_api_style.clone()
    } else {
        provider
            .and_then(|p| str_field(p, "apiStyle"))
            .unwrap_or("chat_completions")
            .to_string()
    };

    let vendor_key = provider
        .and_then(|p| str_field(p, "vendorKey"))
        .unwrap_or("custom")
        .to_string();
    let decorate_all = |rows: &[(String, String, String)]| -> Vec<Value> {
        rows.iter()
            .map(|(id, display, src)| {
                decorate(
                    state,
                    id,
                    display,
                    &provider_id,
                    &vendor_key,
                    &base_url,
                    src,
                )
            })
            .collect()
    };

    // Configured bindings always stay visible (withConfiguredBindings port).
    let configured_bindings: Vec<String> = provider
        .and_then(|p| p.get("models").and_then(|m| m.as_array()).cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|b| str_field(b, "id").map(String::from))
        .collect();

    if source == "cache" {
        if provider.is_some() {
            let mut seen: Vec<(String, String, String)> = Vec::new();
            if let Ok(cached) = host
                .call("providers.listModels", json!({ "providerId": provider_id }))
                .await
            {
                for model in cached
                    .get("models")
                    .and_then(|m| m.as_array())
                    .cloned()
                    .unwrap_or_default()
                {
                    let id = str_field(&model, "modelId").unwrap_or("").to_string();
                    let display = str_field(&model, "displayName").unwrap_or("").to_string();
                    let src = str_field(&model, "source")
                        .unwrap_or("discovered")
                        .to_string();
                    if !id.is_empty() && !seen.iter().any(|(sid, _, _)| sid == &id) {
                        seen.push((id, display, src));
                    }
                }
            }
            for binding in &configured_bindings {
                if !seen.iter().any(|(sid, _, _)| sid == binding) {
                    seen.push((binding.clone(), binding.clone(), "user".into()));
                }
            }
            if !seen.is_empty() {
                return crate::envelope::ok(json!({
                    "models": decorate_all(&seen),
                    "source": "cache",
                }));
            }
            let fallback = provider
                .and_then(|p| str_field(p, "defaultModelId"))
                .map(|id| vec![(id.to_string(), id.to_string(), "user".to_string())])
                .unwrap_or_default();
            return crate::envelope::ok(json!({
                "models": decorate_all(&fallback),
                "source": "fallback",
            }));
        }
        return crate::envelope::ok(json!({ "models": [], "source": "fallback" }));
    }

    // A vendor account has no API key to probe, so its model list comes from
    // the published catalog (an account-scoped endpoint could narrow it later)
    // plus any explicitly configured bindings.
    let is_vendor = provider
        .and_then(|p| str_field(p, "authKind"))
        .map(|k| k == OAUTH_AUTH_KIND)
        .unwrap_or(false);
    if is_vendor {
        // An account-scoped vendor catalog is narrower than models.dev: Copilot
        // lists what the subscription includes and Radius lists its gateway
        // models. A failed live read falls back to the published catalog.
        let account_ids = if provider_id.is_empty() {
            None
        } else {
            crate::oauth::account_model_ids(state, &provider_id, true).await
        };
        let mut seen: Vec<(String, String, String)> =
            match account_ids.filter(|ids| !ids.is_empty()) {
                Some(ids) => ids
                    .into_iter()
                    .map(|id| (id.clone(), id, "catalog".to_string()))
                    .collect(),
                None => state
                    .catalog
                    .models_for_provider(&vendor_key, &base_url)
                    .into_iter()
                    .map(|model| {
                        (
                            model.model_id().to_string(),
                            model.display_name(),
                            "bundled".to_string(),
                        )
                    })
                    .collect(),
            };
        for binding in &configured_bindings {
            if !seen.iter().any(|(id, _, _)| id == binding) {
                seen.push((binding.clone(), binding.clone(), "user".into()));
            }
        }
        let discovered = !seen.is_empty();
        return crate::envelope::ok(json!({
            "models": decorate_all(&seen),
            "source": if discovered { "catalog" } else { "fallback" },
        }));
    }

    let mut api_key = req_api_key.clone();
    if api_key.is_empty() && provider.is_some() {
        match host
            .call("providers.getSecret", json!({ "id": provider_id }))
            .await
        {
            Ok(secret) => {
                api_key = str_field(&secret, "value").unwrap_or("").to_string();
            }
            Err(e) => return crate::envelope::envelope_from_rpc("host", &e),
        }
    }

    let custom_headers = params.get("headers").cloned();
    let (url, headers) =
        model_list_request(&base_url, &api_key, &api_style, custom_headers.as_ref());
    // Blocking HTTP off the async workers.
    let fetch = tauri::async_runtime::spawn_blocking(move || http_get(&url, &headers))
        .await
        .unwrap_or_else(|e| Err(format!("discovery task failed: {e}")));
    let body = match fetch {
        Ok(body) => body,
        Err(e) => return crate::envelope::err("NETWORK_ERROR", e),
    };

    let mut rows: Vec<(String, String, String)> = normalize_model_list(&api_style, &body)
        .into_iter()
        .map(|(id, display)| (id, display, "discovered".to_string()))
        .collect();
    for binding in &configured_bindings {
        if !rows.iter().any(|(sid, _, _)| sid == binding) {
            rows.push((binding.clone(), binding.clone(), "user".into()));
        }
    }

    // Cache onto the provider row when the probed endpoint is the saved one.
    if provider.is_some() {
        let saved_base = provider
            .and_then(|p| str_field(p, "baseUrl"))
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string();
        let saved_style = provider
            .and_then(|p| str_field(p, "apiStyle"))
            .unwrap_or("chat_completions");
        if base_url.trim_end_matches('/') == saved_base && api_style == saved_style {
            let plain: Vec<Value> = rows
                .iter()
                .map(|(id, display, src)| {
                    json!({
                        "modelId": id,
                        "displayName": display,
                        "source": src,
                    })
                })
                .collect();
            let _ = host
                .call(
                    "providers.cacheModels",
                    json!({ "providerId": provider_id, "models": plain }),
                )
                .await;
        }
    }

    crate::envelope::ok(json!({
        "models": decorate_all(&rows),
        "source": "remote",
    }))
}
