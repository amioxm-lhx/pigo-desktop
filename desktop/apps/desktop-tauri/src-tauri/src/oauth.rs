//! Vendor-account OAuth (ADR 0095 / ADR 0098).
//!
//! Every login is one independent provider row plus one encrypted credential at
//! `secret:provider:<id>:oauth`. The renderer only sees progress events; tokens
//! stay in this process and are refreshed there. The sidecar receives a
//! short-lived access token at launch and may ask `provider.resolveAuth` for a
//! fresh one per request, never a refresh token.
//!
//! Browser-callback, device-code, select, and pasted-code steps all travel the
//! same event stream, so the dialog renders what the vendor asked for rather
//! than a per-vendor script.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Notify};

use crate::state::AppState;

const EVENT: &str = "pi-desktop/providers/oauth/event";
const OAUTH_AUTH_KIND: &str = "oauth";
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);
const REFRESH_SKEW_MS: i64 = 5 * 60 * 1000;
const CANCEL_MESSAGE: &str = "Login cancelled";

const COPILOT_HEADERS: &[(&str, &str)] = &[
    ("User-Agent", "GitHubCopilotChat/0.35.0"),
    ("Editor-Version", "vscode/1.107.0"),
    ("Editor-Plugin-Version", "copilot-chat/0.35.0"),
    ("Copilot-Integration-Id", "vscode-chat"),
];
const COPILOT_API_VERSION: &str = "2026-06-01";
const COPILOT_INDIVIDUAL_BASE: &str = "https://api.individual.githubcopilot.com";

struct VendorSpec {
    id: &'static str,
    name: &'static str,
    login_label: &'static str,
    is_subscription: bool,
    base_url: &'static str,
    api_style: &'static str,
    protocol: &'static str,
    fallback_models: &'static [&'static str],
    headers: &'static [(&'static str, &'static str)],
}

const VENDORS: &[VendorSpec] = &[
    VendorSpec {
        id: "anthropic",
        name: "Anthropic (Claude Pro/Max)",
        login_label: "Sign in with Claude Pro/Max",
        is_subscription: true,
        base_url: "https://api.anthropic.com/v1",
        api_style: "anthropic_messages",
        protocol: "anthropic",
        fallback_models: &["claude-opus-4-6", "claude-sonnet-4-5", "claude-haiku-4-5"],
        headers: &[("anthropic-beta", "oauth-2025-04-20")],
    },
    VendorSpec {
        id: "openai-codex",
        name: "OpenAI Codex",
        login_label: "Sign in with ChatGPT",
        is_subscription: true,
        base_url: "https://chatgpt.com/backend-api/codex",
        api_style: "openai_codex_responses",
        protocol: "openai_codex_responses",
        fallback_models: &["gpt-5.6-sol", "gpt-5.4", "gpt-5.3-codex"],
        headers: &[],
    },
    VendorSpec {
        id: "github-copilot",
        name: "GitHub Copilot",
        login_label: "Sign in with GitHub",
        is_subscription: true,
        base_url: "https://api.individual.githubcopilot.com",
        api_style: "chat_completions",
        protocol: "openai_compatible",
        fallback_models: &["gpt-5.4", "claude-sonnet-5", "gemini-3.5-flash"],
        headers: COPILOT_HEADERS,
    },
    VendorSpec {
        id: "openrouter",
        name: "OpenRouter",
        login_label: "Sign in with OpenRouter",
        is_subscription: false,
        base_url: "https://openrouter.ai/api/v1",
        api_style: "chat_completions",
        protocol: "openai_compatible",
        fallback_models: &["openrouter/auto"],
        headers: &[],
    },
    VendorSpec {
        id: "xai",
        name: "xAI",
        login_label: "Sign in with xAI",
        is_subscription: true,
        base_url: "https://api.x.ai/v1",
        api_style: "chat_completions",
        protocol: "openai_compatible",
        fallback_models: &["grok-4", "grok-3"],
        headers: &[],
    },
    VendorSpec {
        id: "kimi-coding",
        name: "Kimi Coding",
        login_label: "Sign in with Kimi",
        is_subscription: true,
        base_url: "https://api.kimi.com/coding/v1",
        api_style: "anthropic_messages",
        protocol: "anthropic",
        fallback_models: &["kimi-k3", "kimi-for-coding"],
        headers: &[],
    },
    VendorSpec {
        id: "radius",
        name: "Radius",
        login_label: "Sign in with Radius",
        is_subscription: false,
        base_url: "https://radius.pi.dev",
        api_style: "pi_messages",
        protocol: "custom_http",
        fallback_models: &[],
        headers: &[],
    },
];

fn vendor_for(vendor_id: &str) -> Option<&'static VendorSpec> {
    VENDORS.iter().find(|vendor| vendor.id == vendor_id)
}

/// A stored OAuth credential. Serialized into the encrypted secret store.
#[derive(Debug, Clone)]
struct Credential {
    vendor: String,
    access: String,
    refresh: String,
    expires: i64,
    enterprise_url: Option<String>,
    /// Model ids the vendor reports as available to this account (Copilot's
    /// subscription filter, Radius's gateway directory). `None` means the
    /// vendor exposes no account catalog, so the published catalog applies.
    available_model_ids: Option<Vec<String>>,
}

impl Credential {
    fn to_json(&self) -> Value {
        let mut value = json!({
            "type": "oauth",
            "vendorId": self.vendor,
            "access": self.access,
            "refresh": self.refresh,
            "expires": self.expires,
        });
        if let Some(url) = &self.enterprise_url {
            value["enterpriseUrl"] = json!(url);
        }
        if let Some(ids) = &self.available_model_ids {
            value["availableModelIds"] = json!(ids);
        }
        value
    }

    fn from_json(value: &Value) -> Option<Self> {
        let access = value.get("access")?.as_str()?.to_string();
        if access.is_empty() {
            return None;
        }
        Some(Self {
            vendor: value
                .get("vendorId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            access,
            refresh: value
                .get("refresh")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            expires: value.get("expires").and_then(|v| v.as_i64()).unwrap_or(0),
            enterprise_url: value
                .get("enterpriseUrl")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            available_model_ids: value
                .get("availableModelIds")
                .and_then(|v| v.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                // An empty list would hide every model; treat it as unknown so
                // the published catalog still applies.
                .filter(|ids| !ids.is_empty()),
        })
    }
}

struct Login {
    id: String,
    vendor: &'static VendorSpec,
    app: AppHandle,
    cancelled: AtomicBool,
    cancel_notify: Notify,
    prompts: Mutex<HashMap<String, mpsc::Sender<Option<String>>>>,
}

impl Login {
    fn emit(&self, mut event: Value) {
        event["loginId"] = json!(self.id);
        event["vendorId"] = json!(self.vendor.id);
        let _ = self.app.emit(EVENT, event);
    }

    fn info(&self, message: impl Into<String>) {
        self.emit(json!({ "kind": "info", "message": message.into() }));
    }

    fn progress(&self, message: impl Into<String>) {
        self.emit(json!({ "kind": "progress", "message": message.into() }));
    }

    fn ensure_active(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::SeqCst) {
            return Err(CANCEL_MESSAGE.into());
        }
        Ok(())
    }

    async fn cancelled(&self) {
        if self.cancelled.load(Ordering::SeqCst) {
            return;
        }
        self.cancel_notify.notified().await;
    }

    async fn sleep_or_cancel(&self, duration: Duration) -> Result<(), String> {
        self.ensure_active()?;
        tokio::select! {
            _ = tokio::time::sleep(duration) => Ok(()),
            _ = self.cancelled() => Err(CANCEL_MESSAGE.into()),
        }
    }

    /// Raise one prompt and hand the caller the answer channel. The answer is
    /// `None` when the flow cancelled the prompt itself (a callback won).
    fn begin_prompt(
        &self,
        prompt_type: &str,
        message: &str,
        placeholder: Option<&str>,
    ) -> (String, mpsc::Receiver<Option<String>>) {
        let prompt_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::channel(1);
        self.prompts.lock().unwrap().insert(prompt_id.clone(), tx);
        self.emit(json!({
            "kind": "prompt",
            "request": {
                "promptId": prompt_id,
                "type": prompt_type,
                "message": message,
                "placeholder": placeholder,
            }
        }));
        (prompt_id, rx)
    }

    fn close_prompt(&self, prompt_id: &str) {
        if self.prompts.lock().unwrap().remove(prompt_id).is_some() {
            self.emit(json!({ "kind": "promptCancelled", "promptId": prompt_id }));
        }
    }

    fn take_answer(&self, prompt_id: &str) -> Option<mpsc::Sender<Option<String>>> {
        self.prompts.lock().unwrap().remove(prompt_id)
    }
}

pub struct OAuthManager {
    logins: Arc<Mutex<HashMap<String, Arc<Login>>>>,
    active_for_vendor: Arc<Mutex<HashMap<String, String>>>,
}

impl OAuthManager {
    pub fn new() -> Self {
        Self {
            logins: Arc::new(Mutex::new(HashMap::new())),
            active_for_vendor: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn register(&self, login: Arc<Login>) {
        self.logins
            .lock()
            .unwrap()
            .insert(login.id.clone(), login.clone());
        self.active_for_vendor
            .lock()
            .unwrap()
            .insert(login.vendor.id.to_string(), login.id.clone());
    }

    fn cancel_active_for_vendor(&self, vendor_id: &str) -> Option<String> {
        let id = self
            .active_for_vendor
            .lock()
            .unwrap()
            .get(vendor_id)
            .cloned()?;
        if let Some(login) = self.logins.lock().unwrap().get(&id).cloned() {
            login.cancelled.store(true, Ordering::SeqCst);
            login.cancel_notify.notify_waiters();
        }
        Some(id)
    }

    pub fn respond(&self, login_id: &str, prompt_id: &str, value: Option<String>) -> bool {
        let Some(login) = self.logins.lock().unwrap().get(login_id).cloned() else {
            return false;
        };
        if value.is_none() {
            login.cancelled.store(true, Ordering::SeqCst);
            login.cancel_notify.notify_waiters();
            return true;
        }
        let Some(sender) = login.take_answer(prompt_id) else {
            return false;
        };
        let _ = sender.try_send(value);
        true
    }

    pub fn cancel(&self, login_id: &str) -> bool {
        let Some(login) = self.logins.lock().unwrap().get(login_id).cloned() else {
            return false;
        };
        login.cancelled.store(true, Ordering::SeqCst);
        login.cancel_notify.notify_waiters();
        true
    }

    pub async fn start(
        &self,
        state: Arc<AppState>,
        app: AppHandle,
        vendor_id: &str,
    ) -> Result<String, String> {
        let vendor =
            vendor_for(vendor_id).ok_or_else(|| format!("unknown OAuth vendor: {vendor_id}"))?;
        // A superseded attempt must let go of its callback port before the next
        // one binds it; the Electron flow cancelled the old login first too.
        if let Some(previous) = self.cancel_active_for_vendor(vendor.id) {
            for _ in 0..40 {
                if !self.logins.lock().unwrap().contains_key(&previous) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        let login = Arc::new(Login {
            id: uuid::Uuid::new_v4().to_string(),
            vendor,
            app,
            cancelled: AtomicBool::new(false),
            cancel_notify: Notify::new(),
            prompts: Mutex::new(HashMap::new()),
        });
        self.register(login.clone());
        let login_id = login.id.clone();
        let manager_logins = self.logins.clone();
        let manager_active = self.active_for_vendor.clone();
        tauri::async_runtime::spawn(async move {
            let outcome = run_login(state, login.clone()).await;
            match outcome {
                Ok((provider_id, label)) => login.emit(json!({
                    "kind": "done",
                    "providerId": provider_id,
                    "accountLabel": label,
                })),
                Err(error) if error == CANCEL_MESSAGE => {
                    login.emit(json!({ "kind": "cancelled" }));
                }
                Err(error) => login.emit(json!({ "kind": "error", "message": error })),
            }
            manager_logins.lock().unwrap().remove(&login.id);
            let mut active = manager_active.lock().unwrap();
            if active
                .get(login.vendor.id)
                .map(|id| id == &login.id)
                .unwrap_or(false)
            {
                active.remove(login.vendor.id);
            }
        });
        Ok(login_id)
    }

    pub async fn vendors(&self, state: &Arc<AppState>) -> Value {
        let rows = state
            .host
            .call("providers.list", json!({ "includeDisabled": true }))
            .await
            .ok()
            .and_then(|payload| payload.get("providers").and_then(|p| p.as_array()).cloned())
            .unwrap_or_default();
        let vendors: Vec<Value> = VENDORS
            .iter()
            .map(|vendor| {
                let accounts: Vec<Value> = rows
                    .iter()
                    .filter(|row| {
                        row.get("vendorKey").and_then(|v| v.as_str()) == Some(vendor.id)
                            && row.get("authKind").and_then(|v| v.as_str()) == Some(OAUTH_AUTH_KIND)
                    })
                    .map(|row| {
                        json!({
                            "providerId": row.get("id").cloned().unwrap_or(Value::Null),
                            "accountLabel": row.get("oauthAccountLabel").cloned().unwrap_or(Value::Null),
                            "connected": row.get("hasOauth").and_then(|v| v.as_bool()).unwrap_or(false),
                        })
                    })
                    .collect();
                json!({
                    "vendorId": vendor.id,
                    "name": vendor.name,
                    "loginLabel": vendor.login_label,
                    "isSubscription": vendor.is_subscription,
                    "accounts": accounts,
                })
            })
            .collect();
        json!({ "vendors": vendors })
    }

    pub async fn delete_account(state: &Arc<AppState>, provider_id: &str) -> Result<Value, String> {
        let result = state
            .host
            .call("providers.delete", json!({ "id": provider_id }))
            .await
            .map_err(|e| e.to_string())?;
        Ok(result)
    }
}

async fn http_blocking<T, F>(work: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .unwrap_or_else(|e| Err(format!("oauth task failed: {e}")))
}

fn call_result(result: Result<ureq::Response, ureq::Error>) -> Result<(u16, Value), String> {
    match result {
        Ok(response) => parse_response(response.status(), response),
        Err(ureq::Error::Status(status, response)) => parse_response(status, response),
        Err(error) => Err(format!("OAuth request failed: {error}")),
    }
}

fn parse_response(status: u16, response: ureq::Response) -> Result<(u16, Value), String> {
    let text = response
        .into_string()
        .map_err(|e| format!("OAuth body read failed: {e}"))?;
    let value = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({ "raw": text }));
    Ok((status, value))
}

fn http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(HTTP_TIMEOUT).build()
}

fn post_form(
    url: &str,
    fields: &[(&str, &str)],
    headers: &[(&str, &str)],
) -> Result<(u16, Value), String> {
    let mut request = http_agent().post(url);
    for (key, value) in headers {
        request = request.set(key, value);
    }
    let body = fields
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                url::form_urlencoded::byte_serialize(key.as_bytes()).collect::<String>(),
                url::form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>(),
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    call_result(
        request
            .set("Content-Type", "application/x-www-form-urlencoded")
            .set("Accept", "application/json")
            .send_string(&body),
    )
}

fn post_json(url: &str, body: Value) -> Result<(u16, Value), String> {
    post_json_with_headers(url, &body, &[])
}

fn post_json_with_headers(
    url: &str,
    body: &Value,
    headers: &[(&str, &str)],
) -> Result<(u16, Value), String> {
    let mut request = http_agent()
        .post(url)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json");
    for (key, value) in headers {
        request = request.set(key, value);
    }
    call_result(request.send_string(&body.to_string()))
}

fn get_json(url: &str, headers: &[(&str, &str)]) -> Result<(u16, Value), String> {
    let mut request = http_agent().get(url).set("Accept", "application/json");
    for (key, value) in headers {
        request = request.set(key, value);
    }
    call_result(request.call())
}

fn error_detail(body: &Value) -> String {
    if let Some(value) = body.get("error_description").and_then(|v| v.as_str()) {
        if !value.is_empty() {
            return format!(": {value}");
        }
    }
    if let Some(value) = body.get("error").and_then(|v| v.as_str()) {
        if !value.is_empty() {
            return format!(": {value}");
        }
    }
    String::new()
}

fn error_code(body: &Value) -> String {
    if let Some(code) = body.get("error").and_then(|v| v.as_str()) {
        return code.to_string();
    }
    body.pointer("/error/code")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn positive_int(body: &Value, key: &str, fallback: i64) -> i64 {
    body.get(key)
        .and_then(|v| v.as_i64())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn credential_from_token(
    body: &Value,
    vendor: &str,
    previous_refresh: &str,
) -> Result<Credential, String> {
    let access = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "OAuth token response carries no access_token{}",
                error_detail(body)
            )
        })?
        .to_string();
    let refresh = body
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| previous_refresh.to_string());
    let expires_in = positive_int(body, "expires_in", 3600);
    Ok(Credential {
        vendor: vendor.to_string(),
        access,
        refresh,
        expires: now_ms() + expires_in * 1000 - REFRESH_SKEW_MS,
        enterprise_url: None,
        available_model_ids: None,
    })
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn pkce_pair() -> (String, String) {
    use base64::Engine as _;
    use rand::RngExt as _;
    use sha2::{Digest, Sha256};
    let mut rng = rand::rng();
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes);
    let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    (verifier, challenge)
}

fn random_hex(len: usize) -> String {
    use rand::RngExt as _;
    let mut rng = rand::rng();
    let mut out = String::with_capacity(len * 2);
    for _ in 0..len {
        let byte: u8 = rng.random();
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn page_html(title: &str, message: &str) -> String {
    let title = escape_html(title);
    let message = escape_html(message);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title></head>\
         <body style=\"font-family:system-ui;background:#09090b;color:#fafafa;display:flex;\
         align-items:center;justify-content:center;min-height:100vh\"><main style=\"text-align:center\">\
         <h1>{title}</h1><p>{message}</p></main></body></html>"
    )
}

struct CallbackServer {
    redirect_uri: String,
    rx: mpsc::Receiver<Result<String, String>>,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for CallbackServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn start_callback(
    port: u16,
    path: &str,
    expected_state: &str,
) -> Result<CallbackServer, String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| format!("cannot open the OAuth callback port {port}: {e}"))?;
    let local = listener
        .local_addr()
        .map_err(|e| format!("callback address unavailable: {e}"))?;
    let redirect_uri = format!("http://localhost:{}{}", local.port(), path);
    let (tx, rx) = mpsc::channel(1);
    let expected = expected_state.to_string();
    let path = path.to_string();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let expected = expected.clone();
            let path = path.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut buffer = [0u8; 8192];
                let Ok(read) =
                    tokio::time::timeout(Duration::from_secs(10), stream.read(&mut buffer)).await
                else {
                    return;
                };
                let Ok(read) = read else { return };
                let request = String::from_utf8_lossy(&buffer[..read]);
                let target = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("");
                let (request_path, query) = target.split_once('?').unwrap_or((target, ""));
                if !request_path.starts_with(&path) {
                    let body = page_html("Not found", "This is not the OAuth callback path.");
                    let _ = respond(stream, 404, &body).await;
                    return;
                }
                let params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
                    .into_owned()
                    .collect();
                let result = if let Some(error) = params.get("error") {
                    Err(format!(
                        "OAuth login was denied: {}",
                        params
                            .get("error_description")
                            .cloned()
                            .unwrap_or_else(|| error.clone())
                    ))
                } else {
                    let code = params.get("code").cloned().unwrap_or_default();
                    let state = params.get("state").cloned().unwrap_or_default();
                    if code.is_empty() {
                        Err("The OAuth callback carried no authorization code.".into())
                    } else if !expected.is_empty() && state != expected {
                        Err("OAuth state mismatch".into())
                    } else {
                        Ok(code)
                    }
                };
                let (status, body) = match &result {
                    Ok(_) => (
                        200,
                        page_html(
                            "Authentication successful",
                            "You can return to pigo-desktop. This tab can be closed.",
                        ),
                    ),
                    Err(error) => (400, page_html("Authentication failed", error)),
                };
                let _ = respond(stream, status, &body).await;
                let _ = tx.try_send(result);
            });
        }
    });
    Ok(CallbackServer {
        redirect_uri,
        rx,
        handle,
    })
}

async fn respond(
    mut stream: tokio::net::TcpStream,
    status: u16,
    body: &str,
) -> std::io::Result<()> {
    let reason = if status == 200 { "OK" } else { "Bad Request" };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await
}

/// Parse a pasted authorization code or full redirect URL.
fn parse_pasted_code(input: &str) -> (String, Option<String>) {
    let value = input.trim();
    if value.is_empty() {
        return (String::new(), None);
    }
    if let Ok(parsed) = url::Url::parse(value) {
        if parsed.scheme() == "http" || parsed.scheme() == "https" {
            let code = parsed
                .query_pairs()
                .find(|(key, _)| key == "code")
                .map(|(_, value)| value.to_string())
                .unwrap_or_default();
            let state = parsed
                .query_pairs()
                .find(|(key, _)| key == "state")
                .map(|(_, value)| value.to_string());
            return (code, state);
        }
    }
    if let Some((code, state)) = value.split_once('#') {
        return (code.to_string(), Some(state.to_string()));
    }
    (value.to_string(), None)
}

struct AuthCodeOptions {
    authorize_url: &'static str,
    token_url: &'static str,
    client_id: &'static str,
    scope: &'static str,
    port: u16,
    path: &'static str,
    json_token: bool,
    state_is_verifier: bool,
}

async fn run_auth_code_flow(login: &Login, options: AuthCodeOptions) -> Result<Credential, String> {
    let (verifier, challenge) = pkce_pair();
    let state = if options.state_is_verifier {
        verifier.clone()
    } else {
        random_hex(16)
    };
    let callback = start_callback(options.port, options.path, &state).await?;
    let redirect_uri = callback.redirect_uri.clone();

    let mut query: Vec<(&str, String)> = vec![
        ("client_id", options.client_id.to_string()),
        ("response_type", "code".to_string()),
        ("redirect_uri", redirect_uri.clone()),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256".to_string()),
        ("state", state.clone()),
    ];
    if !options.scope.is_empty() {
        query.push(("scope", options.scope.to_string()));
    }
    if options.state_is_verifier {
        query.push(("code", "true".to_string()));
    }
    let auth_url = format!(
        "{}?{}",
        options.authorize_url,
        query
            .iter()
            .map(|(key, value)| format!(
                "{}={}",
                url::form_urlencoded::byte_serialize(key.as_bytes()).collect::<String>(),
                url::form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>()
            ))
            .collect::<Vec<_>>()
            .join("&")
    );
    let opened = crate::native::open_url(&login.app, &auth_url).is_ok();
    login.emit(json!({
        "kind": "authUrl",
        "url": auth_url,
        "instructions": "Complete sign-in in your browser. If the browser is on another machine, paste the final redirect URL here.",
        "opened": opened,
    }));

    let (prompt_id, mut answer) = login.begin_prompt(
        "manual_code",
        "Complete sign-in in your browser, or paste the authorization code / redirect URL here:",
        Some(&redirect_uri),
    );
    let mut callback = callback;
    let code = tokio::select! {
        result = callback.rx.recv() => {
            login.close_prompt(&prompt_id);
            match result {
                Some(Ok(code)) => code,
                Some(Err(error)) => return Err(error),
                None => return Err("The OAuth callback server stopped.".into()),
            }
        }
        value = answer.recv() => {
            let Some(Some(value)) = value else {
                return Err(CANCEL_MESSAGE.into());
            };
            let (code, returned_state) = parse_pasted_code(&value);
            if code.is_empty() {
                return Err("The pasted value carries no authorization code.".into());
            }
            if let Some(returned_state) = returned_state {
                if returned_state != state {
                    return Err("OAuth state mismatch".into());
                }
            }
            code
        }
        _ = login.cancelled() => {
            login.close_prompt(&prompt_id);
            return Err(CANCEL_MESSAGE.into());
        }
    };
    // Release the callback port before the token exchange, so a superseded
    // attempt or a retry can bind it immediately.
    drop(callback);
    login.ensure_active()?;
    login.progress("Exchanging the authorization code for tokens...");

    let vendor = login.vendor.id;
    let state_for_exchange = state.clone();
    let body = http_blocking(move || {
        let mut fields = vec![
            ("grant_type", "authorization_code"),
            ("client_id", options.client_id),
            ("code", code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("code_verifier", verifier.as_str()),
        ];
        if options.state_is_verifier {
            // Anthropic echoes the PKCE verifier as `state` on exchange.
            fields.push(("state", state_for_exchange.as_str()));
        }
        let (status, body) = if options.json_token {
            let mut payload = serde_json::Map::new();
            for (key, value) in fields {
                payload.insert(key.to_string(), json!(value));
            }
            post_json(options.token_url, Value::Object(payload))?
        } else {
            post_form(options.token_url, &fields, &[])?
        };
        if status < 200 || status >= 300 {
            return Err(format!(
                "{vendor} OAuth token exchange failed (HTTP {status}){}",
                error_detail(&body)
            ));
        }
        Ok(body)
    })
    .await?;
    credential_from_token(&body, vendor, "")
}

struct DeviceOptions {
    device_url: &'static str,
    token_url: &'static str,
    client_id: &'static str,
    scope: &'static str,
    extra_device_fields: &'static [(&'static str, &'static str)],
}

struct DeviceAuthorization {
    device_code: String,
    user_code: String,
    verification_uri: String,
    interval: i64,
    expires_in: i64,
}

fn parse_device_authorization(body: &Value) -> Result<DeviceAuthorization, String> {
    let device_code = body
        .get("device_code")
        .and_then(|v| v.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "OAuth device response carries no device_code".to_string())?;
    let user_code = body
        .get("user_code")
        .and_then(|v| v.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "OAuth device response carries no user_code".to_string())?;
    let verification_uri = body
        .get("verification_uri_complete")
        .and_then(|v| v.as_str())
        .or_else(|| body.get("verification_uri").and_then(|v| v.as_str()))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "OAuth device response carries no verification_uri".to_string())?;
    Ok(DeviceAuthorization {
        device_code: device_code.to_string(),
        user_code: user_code.to_string(),
        verification_uri: verification_uri.to_string(),
        interval: positive_int(body, "interval", 5),
        expires_in: positive_int(body, "expires_in", 900),
    })
}

async fn run_device_flow(login: &Login, options: DeviceOptions) -> Result<Credential, String> {
    login.progress("Requesting a device code...");
    let DeviceOptions {
        device_url,
        token_url,
        client_id,
        scope,
        extra_device_fields,
    } = options;
    let body = http_blocking(move || {
        let mut fields = vec![("client_id", client_id)];
        if !scope.is_empty() {
            fields.push(("scope", scope));
        }
        fields.extend(extra_device_fields.iter().copied());
        let (status, body) = post_form(device_url, &fields, &[])?;
        if status < 200 || status >= 300 {
            return Err(format!(
                "OAuth device authorization failed (HTTP {status}){}",
                error_detail(&body)
            ));
        }
        Ok(body)
    })
    .await?;
    let device = parse_device_authorization(&body)?;
    login.emit(json!({
        "kind": "deviceCode",
        "userCode": device.user_code,
        "verificationUri": device.verification_uri,
        "intervalSeconds": device.interval,
        "expiresInSeconds": device.expires_in,
    }));

    let vendor = login.vendor.id;
    let deadline = std::time::Instant::now() + Duration::from_secs(device.expires_in.max(1) as u64);
    let mut interval = Duration::from_secs(device.interval.max(1) as u64);
    loop {
        login.sleep_or_cancel(interval).await?;
        if std::time::Instant::now() >= deadline {
            return Err("The device authorization expired. Please restart the login.".into());
        }
        let device_code = device.device_code.clone();
        let (status, body) = http_blocking(move || {
            post_form(
                token_url,
                &[
                    ("client_id", client_id),
                    ("device_code", device_code.as_str()),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ],
                &[],
            )
        })
        .await?;
        if let Some(access) = body.get("access_token").and_then(|v| v.as_str()) {
            if !access.is_empty() {
                return credential_from_token(&body, vendor, "");
            }
        }
        if status >= 500 {
            return Err(format!(
                "OAuth token request failed (HTTP {status}){}",
                error_detail(&body)
            ));
        }
        match error_code(&body).as_str() {
            "authorization_pending" => continue,
            "slow_down" => {
                interval += Duration::from_secs(positive_int(&body, "interval", 5).max(1) as u64);
                continue;
            }
            "expired_token" => {
                return Err("The device authorization expired. Please restart the login.".into());
            }
            "access_denied" => return Err("OAuth login was denied.".into()),
            code => {
                return Err(format!(
                    "OAuth token request failed (HTTP {status}){}",
                    if code.is_empty() {
                        error_detail(&body)
                    } else {
                        format!(": {code}")
                    }
                ));
            }
        }
    }
}

/// Anthropic and OpenAI Codex use their own browser-callback parameters.
async fn login_anthropic(login: &Login) -> Result<Credential, String> {
    run_auth_code_flow(
        login,
        AuthCodeOptions {
            authorize_url: "https://claude.ai/oauth/authorize",
            token_url: "https://platform.claude.com/v1/oauth/token",
            client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
            scope: "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload",
            port: 53692,
            path: "/callback",
            json_token: true,
            state_is_verifier: true,
        },
    )
    .await
}

async fn login_openai_codex(login: &Login) -> Result<Credential, String> {
    run_auth_code_flow(
        login,
        AuthCodeOptions {
            authorize_url: "https://auth.openai.com/oauth/authorize",
            token_url: "https://auth.openai.com/oauth/token",
            client_id: "app_EMoamEEZ73f0CkXaXp7hrann",
            scope: "openid profile email offline_access",
            port: 1455,
            path: "/auth/callback",
            json_token: false,
            state_is_verifier: false,
        },
    )
    .await
}

async fn login_openrouter(login: &Login) -> Result<Credential, String> {
    let (verifier, challenge) = pkce_pair();
    let callback_path = format!("/oauth/callback/{}", random_hex(12));
    let callback = start_callback(0, &callback_path, "").await?;
    let redirect_uri = callback.redirect_uri.clone();
    let auth_url = format!(
        "https://openrouter.ai/auth?callback_url={}&code_challenge={}&code_challenge_method=S256",
        url::form_urlencoded::byte_serialize(redirect_uri.as_bytes()).collect::<String>(),
        challenge,
    );
    let opened = crate::native::open_url(&login.app, &auth_url).is_ok();
    login.emit(json!({
        "kind": "authUrl",
        "url": auth_url,
        "instructions": "Complete sign-in in your browser. If the browser is on another machine, paste the final redirect URL here.",
        "opened": opened,
    }));
    let (prompt_id, mut answer) = login.begin_prompt(
        "manual_code",
        "Complete sign-in in your browser, or paste the authorization code / redirect URL here:",
        Some(&redirect_uri),
    );
    let mut callback = callback;
    let code = tokio::select! {
        result = callback.rx.recv() => {
            login.close_prompt(&prompt_id);
            match result {
                Some(Ok(code)) => code,
                Some(Err(error)) => return Err(error),
                None => return Err("The OAuth callback server stopped.".into()),
            }
        }
        value = answer.recv() => {
            let Some(Some(value)) = value else {
                return Err(CANCEL_MESSAGE.into());
            };
            parse_pasted_code(&value).0
        }
        _ = login.cancelled() => {
            login.close_prompt(&prompt_id);
            return Err(CANCEL_MESSAGE.into());
        }
    };
    drop(callback);
    if code.is_empty() {
        return Err("The pasted value carries no authorization code.".into());
    }
    login.progress("Exchanging the authorization code for an API key...");
    let body = http_blocking(move || {
        let (status, body) = post_json(
            "https://openrouter.ai/api/v1/auth/keys",
            json!({
                "code": code,
                "code_verifier": verifier,
                "code_challenge_method": "S256",
            }),
        )?;
        if status < 200 || status >= 300 {
            return Err(format!(
                "OpenRouter OAuth key exchange failed (HTTP {status}){}",
                error_detail(&body)
            ));
        }
        Ok(body)
    })
    .await?;
    let key = body
        .get("key")
        .and_then(|v| v.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "OpenRouter OAuth response carries no key".to_string())?;
    Ok(Credential {
        vendor: login.vendor.id.to_string(),
        access: key.to_string(),
        refresh: String::new(),
        expires: i64::MAX / 2,
        enterprise_url: None,
        available_model_ids: None,
    })
}

async fn login_copilot(login: &Login) -> Result<Credential, String> {
    let (prompt_id, mut answer) = login.begin_prompt(
        "text",
        "GitHub Enterprise URL or domain (leave blank for github.com)",
        Some("company.ghe.com"),
    );
    let domain = tokio::select! {
        value = answer.recv() => match value {
            Some(Some(value)) => value.trim().to_string(),
            _ => return Err(CANCEL_MESSAGE.into()),
        },
        _ = login.cancelled() => {
            login.close_prompt(&prompt_id);
            return Err(CANCEL_MESSAGE.into());
        }
    };
    let domain = normalize_github_domain(&domain)?;
    login.progress(format!("Requesting a GitHub device code for {domain}..."));
    let device_url = format!("https://{domain}/login/device/code");
    let (status, body) = http_blocking(move || {
        post_form(
            &device_url,
            &[("client_id", COPILOT_CLIENT_ID)],
            COPILOT_HEADERS,
        )
    })
    .await?;
    if status < 200 || status >= 300 {
        return Err(format!(
            "GitHub Copilot device authorization failed (HTTP {status}){}",
            error_detail(&body)
        ));
    }
    let device = parse_device_authorization(&body)?;
    login.emit(json!({
        "kind": "deviceCode",
        "userCode": device.user_code,
        "verificationUri": device.verification_uri,
        "intervalSeconds": device.interval,
        "expiresInSeconds": device.expires_in,
    }));

    let deadline = std::time::Instant::now() + Duration::from_secs(device.expires_in.max(1) as u64);
    let mut interval = Duration::from_secs(device.interval.max(1) as u64);
    let github_token = loop {
        login.sleep_or_cancel(interval).await?;
        if std::time::Instant::now() >= deadline {
            return Err(
                "The GitHub device authorization expired. Please restart the login.".into(),
            );
        }
        let device_code = device.device_code.clone();
        let domain_for_poll = domain.clone();
        let (status, body) = http_blocking(move || {
            post_form(
                &format!("https://{domain_for_poll}/login/oauth/access_token"),
                &[
                    ("client_id", COPILOT_CLIENT_ID),
                    ("device_code", device_code.as_str()),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ],
                COPILOT_HEADERS,
            )
        })
        .await?;
        if let Some(token) = body.get("access_token").and_then(|v| v.as_str()) {
            if !token.is_empty() {
                break token.to_string();
            }
        }
        if status >= 500 {
            return Err(format!(
                "GitHub Copilot token request failed (HTTP {status}){}",
                error_detail(&body)
            ));
        }
        match error_code(&body).as_str() {
            "authorization_pending" => continue,
            "slow_down" => {
                interval += Duration::from_secs(positive_int(&body, "interval", 5).max(1) as u64);
                continue;
            }
            "expired_token" => {
                return Err(
                    "The GitHub device authorization expired. Please restart the login.".into(),
                );
            }
            "access_denied" => return Err("GitHub Copilot login was denied.".into()),
            code => {
                return Err(format!(
                    "GitHub Copilot token request failed (HTTP {status}){}",
                    if code.is_empty() {
                        error_detail(&body)
                    } else {
                        format!(": {code}")
                    }
                ));
            }
        }
    };
    login.progress("Exchanging the GitHub token for a Copilot token...");
    let domain_for_exchange = domain.clone();
    let copilot = http_blocking(move || {
        exchange_copilot_token(&github_token, &domain_for_exchange, &github_token)
    })
    .await?;
    let mut credential = copilot;
    if domain != "github.com" {
        credential.enterprise_url = Some(domain);
    }
    Ok(credential)
}

const COPILOT_CLIENT_ID: &str = "IX1.b507a08c87ecfe98";

fn normalize_github_domain(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok("github.com".into());
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let parsed = url::Url::parse(&candidate)
        .map_err(|_| "Invalid GitHub Enterprise URL/domain".to_string())?;
    parsed
        .host_str()
        .map(str::to_string)
        .ok_or_else(|| "Invalid GitHub Enterprise URL/domain".to_string())
}

fn exchange_copilot_token(
    github_token: &str,
    domain: &str,
    refresh: &str,
) -> Result<Credential, String> {
    let api_domain = if domain == "github.com" {
        "github.com".to_string()
    } else {
        domain.to_string()
    };
    let endpoint = format!("https://api.{api_domain}/copilot_internal/v2/token");
    let (status, body) = get_json(
        &endpoint,
        &[("Authorization", &format!("Bearer {github_token}"))],
    )?;
    if status < 200 || status >= 300 {
        return Err(format!(
            "GitHub Copilot token request failed (HTTP {status}){}",
            error_detail(&body)
        ));
    }
    let token = body
        .get("token")
        .and_then(|v| v.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Invalid Copilot token response".to_string())?;
    let expires_at = body
        .get("expires_at")
        .and_then(|v| v.as_i64())
        .filter(|value| *value > 0)
        .ok_or_else(|| "Invalid Copilot token response fields".to_string())?;
    Ok(Credential {
        vendor: "github-copilot".into(),
        access: token.to_string(),
        refresh: refresh.to_string(),
        expires: expires_at * 1000 - REFRESH_SKEW_MS,
        enterprise_url: None,
        available_model_ids: None,
    })
}

/// Copilot advertises its API host inside the short-lived token
/// (`proxy-ep=...`); enterprise and individual accounts differ.
fn copilot_api_base(access: &str, enterprise_url: Option<&str>) -> String {
    if let Some(host) = access
        .split(';')
        .find_map(|part| part.trim().strip_prefix("proxy-ep="))
    {
        let host = host.trim();
        if !host.is_empty() {
            let api_host = host
                .strip_prefix("proxy.")
                .map(|rest| format!("api.{rest}"))
                .unwrap_or_else(|| host.to_string());
            return format!("https://{api_host}");
        }
    }
    match enterprise_url.filter(|domain| !domain.is_empty()) {
        Some(domain) => format!("https://copilot-api.{domain}"),
        None => COPILOT_INDIVIDUAL_BASE.to_string(),
    }
}

fn dedup_model_ids(ids: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

/// `GET /models` for a Copilot account: the models this subscription can use.
/// Models without tool calling are unusable here, and picker-disabled or
/// policy-disabled models are not offered. Individual accounts sometimes report
/// every picker flag as false, so an explicit `enabled` policy is the fallback.
/// The account catalog split into usable ids and policy-disabled models that
/// the caller may try to enable.
#[derive(Debug, Default)]
struct CopilotCatalog {
    available: Vec<String>,
    policy_disabled: Vec<String>,
}

fn parse_copilot_catalog(
    body: &Value,
    allow_policy_fallback: bool,
) -> Result<CopilotCatalog, String> {
    let data = body
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Invalid Copilot models response".to_string())?;
    let mut account: Vec<(String, bool, String)> = Vec::new();
    for item in data {
        let Some(id) = item
            .get("id")
            .and_then(|v| v.as_str())
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        if item
            .pointer("/capabilities/supports/tool_calls")
            .and_then(|v| v.as_bool())
            == Some(false)
        {
            continue;
        }
        account.push((
            id.to_string(),
            item.get("model_picker_enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            item.pointer("/policy/state")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        ));
    }
    let mut available: Vec<String> = account
        .iter()
        .filter(|(_, picker, policy)| *picker && policy != "disabled")
        .map(|(id, _, _)| id.clone())
        .collect();
    if available.is_empty() && allow_policy_fallback {
        available = account
            .iter()
            .filter(|(_, _, policy)| policy == "enabled")
            .map(|(id, _, _)| id.clone())
            .collect();
    }
    let policy_disabled = account
        .iter()
        .filter(|(_, picker, policy)| *picker && policy == "disabled")
        .map(|(id, _, _)| id.clone())
        .collect();
    Ok(CopilotCatalog {
        available: dedup_model_ids(available),
        policy_disabled: dedup_model_ids(policy_disabled),
    })
}

#[cfg(test)]
fn parse_copilot_model_ids(
    body: &Value,
    allow_policy_fallback: bool,
) -> Result<Vec<String>, String> {
    Ok(parse_copilot_catalog(body, allow_policy_fallback)?.available)
}

/// Radius publishes its gateway directory at `/v1/config`.
fn parse_radius_model_ids(body: &Value) -> Result<Vec<String>, String> {
    let models = body
        .get("models")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Invalid Radius config".to_string())?;
    Ok(dedup_model_ids(
        models
            .iter()
            .filter_map(|model| model.get("id").and_then(|v| v.as_str()))
            .filter(|id| !id.trim().is_empty())
            .map(str::to_string)
            .collect(),
    ))
}

fn fetch_copilot_model_ids(access: &str, api_base: &str) -> Result<Vec<String>, String> {
    let bearer = format!("Bearer {access}");
    let mut headers: Vec<(&str, &str)> = vec![
        ("Authorization", bearer.as_str()),
        ("X-GitHub-Api-Version", COPILOT_API_VERSION),
    ];
    headers.extend_from_slice(COPILOT_HEADERS);
    let base = api_base.trim_end_matches('/');
    let endpoint = format!("{base}/models");
    let (status, body) = get_json(&endpoint, &headers)?;
    if status < 200 || status >= 300 {
        return Err(format!("Copilot model catalog failed (HTTP {status})"));
    }
    let individual = api_base == COPILOT_INDIVIDUAL_BASE;
    let catalog = parse_copilot_catalog(&body, individual)?;
    if catalog.policy_disabled.is_empty() {
        return Ok(catalog.available);
    }

    // Some subscription models exist but are policy-disabled until the account
    // opts in. Enable the picker-visible ones, then re-read the catalog so the
    // returned ids are what the account can actually use.
    let mut enabled_any = false;
    for id in catalog.policy_disabled.iter().take(20) {
        let policy_url = format!("{base}/models/{id}/policy");
        match post_json_with_headers(&policy_url, &json!({ "state": "enabled" }), &headers) {
            Ok((code, _)) if (200..300).contains(&code) => enabled_any = true,
            Ok((code, _)) => {
                log::warn!("copilot policy enable for {id} failed (HTTP {code})");
            }
            Err(error) => {
                log::warn!("copilot policy enable for {id} failed: {error}");
            }
        }
    }
    if !enabled_any {
        return Ok(catalog.available);
    }
    let (status, body) = get_json(&endpoint, &headers)?;
    if status < 200 || status >= 300 {
        return Ok(catalog.available);
    }
    Ok(parse_copilot_catalog(&body, individual)?.available)
}

fn fetch_radius_model_ids(access: &str, gateway: &str) -> Result<Vec<String>, String> {
    let endpoint = format!("{}/v1/config", gateway.trim_end_matches('/'));
    let (status, body) = get_json(&endpoint, &[("Authorization", &format!("Bearer {access}"))])?;
    if status < 200 || status >= 300 {
        return Err(format!("Radius model catalog failed (HTTP {status})"));
    }
    parse_radius_model_ids(&body)
}

/// Vendors whose model list is scoped to the signed-in account.
fn fetch_account_model_ids(
    vendor: &'static VendorSpec,
    credential: &Credential,
) -> Result<Vec<String>, String> {
    match vendor.id {
        "github-copilot" => {
            let base = copilot_api_base(&credential.access, credential.enterprise_url.as_deref());
            fetch_copilot_model_ids(&credential.access, &base)
        }
        "radius" => fetch_radius_model_ids(&credential.access, vendor.base_url),
        other => Err(format!("{other} has no account model catalog")),
    }
}

/// Best effort: a vendor catalog outage must not fail a completed login, and a
/// failed refresh keeps the previously stored ids.
fn attach_account_catalog(vendor: &'static VendorSpec, mut credential: Credential) -> Credential {
    if let Ok(ids) = fetch_account_model_ids(vendor, &credential) {
        if !ids.is_empty() {
            credential.available_model_ids = Some(ids);
        }
    }
    credential
}

/// Model ids scoped to one vendor account, if the vendor exposes such a
/// catalog. `allow_network` asks the vendor again (and persists the answer) so a
/// newly entitled model appears without signing in again; otherwise the stored
/// credential is authoritative.
pub async fn account_model_ids(
    state: &AppState,
    provider_id: &str,
    allow_network: bool,
) -> Option<Vec<String>> {
    let row = state
        .host
        .call("providers.get", json!({ "id": provider_id }))
        .await
        .ok()?
        .get("provider")
        .cloned()
        .filter(|value| !value.is_null())?;
    let vendor_key = row.get("vendorKey").and_then(|v| v.as_str()).unwrap_or("");
    let mut credential = read_credential(state, provider_id).await.ok()?;
    let vendor = vendor_id_for_row(vendor_key, Some(&credential))?;
    if allow_network && matches!(vendor.id, "github-copilot" | "radius") {
        // An expired access token would fail the catalog read, and a refresh
        // already re-reads the catalog for these vendors.
        if let Ok(fresh) = fresh_credential(state, provider_id, vendor).await {
            credential = fresh;
        }
        let fetched = http_blocking({
            let credential = credential.clone();
            move || fetch_account_model_ids(vendor, &credential)
        })
        .await
        .ok();
        if let Some(ids) = fetched.filter(|ids| !ids.is_empty()) {
            credential.available_model_ids = Some(ids.clone());
            // Re-read before writing: a concurrent token refresh must not be
            // overwritten by this catalog update.
            if let Ok(mut latest) = read_credential(state, provider_id).await {
                latest.available_model_ids = Some(ids);
                let secret_ref = format!("secret:provider:{provider_id}:oauth");
                if state
                    .host
                    .call(
                        "secrets.set",
                        json!({
                            "secretRef": secret_ref,
                            "value": latest.to_json().to_string(),
                        }),
                    )
                    .await
                    .is_ok()
                {
                    credential = latest;
                }
            }
        }
    }
    credential.available_model_ids.filter(|ids| !ids.is_empty())
}

async fn run_login(state: Arc<AppState>, login: Arc<Login>) -> Result<(String, String), String> {
    login.info("Starting the sign-in flow...");
    let credential = match login.vendor.id {
        "anthropic" => login_anthropic(&login).await?,
        "openai-codex" => login_openai_codex(&login).await?,
        "github-copilot" => login_copilot(&login).await?,
        "openrouter" => login_openrouter(&login).await?,
        "xai" => {
            run_device_flow(
                &login,
                DeviceOptions {
                    device_url: "https://auth.x.ai/oauth2/device/code",
                    token_url: "https://auth.x.ai/oauth2/token",
                    client_id: "b1a00492-073a-47ea-816f-4c329264a828",
                    scope: "openid profile email offline_access grok-cli:access api:access",
                    extra_device_fields: &[("referrer", "pi")],
                },
            )
            .await?
        }
        "kimi-coding" => {
            run_device_flow(
                &login,
                DeviceOptions {
                    device_url: "https://auth.kimi.com/api/oauth/device_authorization",
                    token_url: "https://auth.kimi.com/api/oauth/token",
                    client_id: "17e5f671-d194-4dfb-9706-5516cb48c098",
                    scope: "",
                    extra_device_fields: &[],
                },
            )
            .await?
        }
        "radius" => {
            run_device_flow(
                &login,
                DeviceOptions {
                    device_url: "https://radius.pi.dev/v1/oauth/device",
                    token_url: "https://radius.pi.dev/v1/oauth/token",
                    client_id: "pi-gateway",
                    scope: "gateway offline_access",
                    extra_device_fields: &[],
                },
            )
            .await?
        }
        other => return Err(format!("unknown OAuth vendor: {other}")),
    };
    login.ensure_active()?;
    // A vendor with an account-scoped catalog reports what this subscription may
    // use; a failure leaves the published catalog in charge.
    login.progress("Reading the models available to this account...");
    let vendor = login.vendor;
    let credential = http_blocking(move || Ok(attach_account_catalog(vendor, credential))).await?;
    login.progress("Saving the vendor account...");
    let label = login.vendor.name.to_string();
    let provider_id = create_account(&state, login.vendor, &credential).await?;
    Ok((provider_id, label))
}

async fn create_account(
    state: &Arc<AppState>,
    vendor: &'static VendorSpec,
    credential: &Credential,
) -> Result<String, String> {
    let (models, default_model) = vendor_models(state, vendor, Some(credential));
    let mut headers = serde_json::Map::new();
    for (key, value) in vendor.headers {
        headers.insert(key.to_string(), json!(value));
    }
    let mut input = json!({
        "name": vendor.name,
        "vendorKey": vendor.id,
        "type": "native",
        "protocol": vendor.protocol,
        "baseUrl": vendor.base_url,
        "authKind": OAUTH_AUTH_KIND,
        "apiStyle": vendor.api_style,
        "models": models,
        "oauthAccountLabel": vendor.name,
    });
    if !default_model.is_empty() {
        input["defaultModelId"] = json!(default_model);
    }
    if !headers.is_empty() {
        input["headers"] = Value::Object(headers);
    }
    let created = state
        .host
        .call("providers.create", input)
        .await
        .map_err(|e| format!("cannot create the vendor account: {e}"))?;
    let provider_id = created
        .pointer("/provider/id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "the provider row was not created".to_string())?
        .to_string();
    let secret_ref = format!("secret:provider:{provider_id}:oauth");
    if let Err(error) = state
        .host
        .call(
            "secrets.set",
            json!({ "secretRef": secret_ref, "value": credential.to_json().to_string() }),
        )
        .await
    {
        let _ = state
            .host
            .call("providers.delete", json!({ "id": provider_id }))
            .await;
        return Err(format!("cannot store the vendor credential: {error}"));
    }
    Ok(provider_id)
}

fn vendor_models(
    state: &Arc<AppState>,
    vendor: &'static VendorSpec,
    credential: Option<&Credential>,
) -> (Vec<Value>, String) {
    // An account-scoped catalog wins: it reflects what this subscription may
    // actually use, while models.dev only supplies metadata for those ids.
    let account_ids = credential
        .and_then(|credential| credential.available_model_ids.as_ref())
        .filter(|ids| !ids.is_empty());
    if let Some(ids) = account_ids {
        let bindings: Vec<Value> = ids
            .iter()
            .take(48)
            .map(|id| {
                state
                    .catalog
                    .find_model(vendor.id, vendor.base_url, id)
                    .map(|model| model.to_binding())
                    .unwrap_or_else(|| {
                        json!({
                            "id": id,
                            "contextWindow": 128_000,
                            "maxTokens": 8_192,
                            "thinkingLevels": ["off"],
                            "defaultThinkingLevel": "off",
                        })
                    })
            })
            .collect();
        if !bindings.is_empty() {
            let default = bindings
                .first()
                .and_then(|binding| binding.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            return (bindings, default);
        }
    }
    let catalog_models = state
        .catalog
        .models_for_provider(vendor.id, vendor.base_url);
    if !catalog_models.is_empty() {
        let bindings: Vec<Value> = catalog_models
            .iter()
            .take(48)
            .map(|model| model.to_binding())
            .collect();
        let default = bindings
            .first()
            .and_then(|binding| binding.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        return (bindings, default);
    }
    let bindings: Vec<Value> = vendor
        .fallback_models
        .iter()
        .map(|id| {
            json!({
                "id": id,
                "contextWindow": 128_000,
                "maxTokens": 8_192,
                "thinkingLevels": ["off"],
                "defaultThinkingLevel": "off",
            })
        })
        .collect();
    let default = vendor
        .fallback_models
        .first()
        .copied()
        .unwrap_or("")
        .to_string();
    (bindings, default)
}

fn vendor_id_for_row(
    vendor_key: &str,
    credential: Option<&Credential>,
) -> Option<&'static VendorSpec> {
    if let Some(vendor) = vendor_for(vendor_key) {
        return Some(vendor);
    }
    credential.and_then(|credential| vendor_for(&credential.vendor))
}

fn refresh_credential(
    vendor: &'static VendorSpec,
    credential: &Credential,
) -> Result<Credential, String> {
    match vendor.id {
        "anthropic" => {
            let (status, body) = post_json(
                "https://platform.claude.com/v1/oauth/token",
                json!({
                    "grant_type": "refresh_token",
                    "client_id": "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
                    "refresh_token": credential.refresh,
                }),
            )?;
            if status < 200 || status >= 300 {
                return Err(format!(
                    "Anthropic OAuth refresh failed (HTTP {status}){}",
                    error_detail(&body)
                ));
            }
            credential_from_token(&body, "anthropic", &credential.refresh)
        }
        "openai-codex" => {
            let (status, body) = post_form(
                "https://auth.openai.com/oauth/token",
                &[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", credential.refresh.as_str()),
                    ("client_id", "app_EMoamEEZ73f0CkXaXp7hrann"),
                ],
                &[],
            )?;
            if status < 200 || status >= 300 {
                return Err(format!(
                    "OpenAI Codex OAuth refresh failed (HTTP {status}){}",
                    error_detail(&body)
                ));
            }
            credential_from_token(&body, "openai-codex", &credential.refresh)
        }
        "github-copilot" => {
            let domain = credential
                .enterprise_url
                .clone()
                .unwrap_or_else(|| "github.com".into());
            let mut refreshed =
                exchange_copilot_token(&credential.refresh, &domain, &credential.refresh)?;
            if domain != "github.com" {
                refreshed.enterprise_url = Some(domain);
            }
            // Entitlements can change between tokens; keep the last known list
            // when the vendor catalog is temporarily unavailable.
            refreshed.available_model_ids = credential.available_model_ids.clone();
            Ok(attach_account_catalog(vendor, refreshed))
        }
        "xai" => {
            let (status, body) = post_form(
                "https://auth.x.ai/oauth2/token",
                &[
                    ("client_id", "b1a00492-073a-47ea-816f-4c329264a828"),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", credential.refresh.as_str()),
                ],
                &[],
            )?;
            if status < 200 || status >= 300 {
                return Err(format!(
                    "xAI OAuth refresh failed (HTTP {status}){}",
                    error_detail(&body)
                ));
            }
            credential_from_token(&body, "xai", &credential.refresh)
        }
        "kimi-coding" => {
            let (status, body) = post_form(
                "https://auth.kimi.com/api/oauth/token",
                &[
                    ("client_id", "17e5f671-d194-4dfb-9706-5516cb48c098"),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", credential.refresh.as_str()),
                ],
                &[],
            )?;
            if status < 200 || status >= 300 {
                return Err(format!(
                    "Kimi OAuth refresh failed (HTTP {status}){}",
                    error_detail(&body)
                ));
            }
            credential_from_token(&body, "kimi-coding", &credential.refresh)
        }
        "radius" => {
            let (status, body) = post_form(
                "https://radius.pi.dev/v1/oauth/token",
                &[
                    ("client_id", "pi-gateway"),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", credential.refresh.as_str()),
                ],
                &[],
            )?;
            if status < 200 || status >= 300 {
                return Err(format!(
                    "Radius OAuth refresh failed (HTTP {status}){}",
                    error_detail(&body)
                ));
            }
            let mut refreshed = credential_from_token(&body, "radius", &credential.refresh)?;
            refreshed.available_model_ids = credential.available_model_ids.clone();
            Ok(attach_account_catalog(vendor, refreshed))
        }
        "openrouter" => Ok(credential.clone()),
        other => Err(format!("unknown OAuth vendor: {other}")),
    }
}

async fn read_credential(state: &AppState, provider_id: &str) -> Result<Credential, String> {
    let secret_ref = format!("secret:provider:{provider_id}:oauth");
    let value = state
        .host
        .call("secrets.getForRuntime", json!({ "secretRef": secret_ref }))
        .await
        .map_err(|e| e.to_string())?
        .get("value")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| "the vendor account is not signed in".to_string())?;
    let parsed: Value = serde_json::from_str(&value)
        .map_err(|_| "the vendor credential is unreadable".to_string())?;
    Credential::from_json(&parsed).ok_or_else(|| "the vendor credential is incomplete".to_string())
}

/// Read the stored credential, refreshing it when it is past (or near) expiry.
async fn fresh_credential(
    state: &AppState,
    provider_id: &str,
    vendor: &'static VendorSpec,
) -> Result<Credential, String> {
    let credential = read_credential(state, provider_id).await?;
    if credential.expires > now_ms() + 60_000 {
        return Ok(credential);
    }
    let refreshed = http_blocking(move || refresh_credential(vendor, &credential)).await?;
    let secret_ref = format!("secret:provider:{provider_id}:oauth");
    state
        .host
        .call(
            "secrets.set",
            json!({ "secretRef": secret_ref, "value": refreshed.to_json().to_string() }),
        )
        .await
        .map_err(|e| format!("cannot store the refreshed credential: {e}"))?;
    Ok(refreshed)
}

/// Resolve one short-lived access token for a provider row, refreshing it when
/// it is past (or near) its expiry. Never returns the refresh token.
pub async fn resolve_access_token(state: &AppState, provider_id: &str) -> Result<String, String> {
    let row = state
        .host
        .call("providers.get", json!({ "id": provider_id }))
        .await
        .map_err(|e| e.to_string())?
        .get("provider")
        .cloned()
        .filter(|value| !value.is_null())
        .ok_or_else(|| "provider not found".to_string())?;
    if row.get("authKind").and_then(|v| v.as_str()) != Some(OAUTH_AUTH_KIND) {
        return Err("provider is not a vendor account".into());
    }
    let vendor_key = row.get("vendorKey").and_then(|v| v.as_str()).unwrap_or("");
    let credential = read_credential(state, provider_id).await?;
    let vendor = vendor_id_for_row(vendor_key, Some(&credential))
        .ok_or_else(|| "the vendor credential names an unknown vendor".to_string())?;
    Ok(fresh_credential(state, provider_id, vendor).await?.access)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_catalog_covers_the_runtime_vendors() {
        let ids: Vec<&str> = VENDORS.iter().map(|vendor| vendor.id).collect();
        for expected in [
            "anthropic",
            "openai-codex",
            "github-copilot",
            "openrouter",
            "xai",
            "kimi-coding",
            "radius",
        ] {
            assert!(ids.contains(&expected), "missing vendor {expected}");
        }
    }

    #[test]
    fn parses_a_pasted_redirect_url() {
        let (code, state) = parse_pasted_code("http://localhost:53692/callback?code=abc&state=xyz");
        assert_eq!(code, "abc");
        assert_eq!(state.as_deref(), Some("xyz"));
    }

    #[test]
    fn credential_round_trips_without_exposing_the_refresh_token_in_events() {
        let credential = Credential {
            vendor: "anthropic".into(),
            access: "access".into(),
            refresh: "refresh".into(),
            expires: 42,
            enterprise_url: None,
            available_model_ids: Some(vec!["gpt-5.4".into(), "claude-sonnet-5".into()]),
        };
        let value = credential.to_json();
        assert_eq!(value["access"], "access");
        let parsed = Credential::from_json(&value).expect("credential");
        assert_eq!(parsed.refresh, "refresh");
        assert_eq!(
            parsed.available_model_ids,
            Some(vec!["gpt-5.4".to_string(), "claude-sonnet-5".to_string()])
        );
        // An empty list must not hide every model; it is treated as unknown.
        let mut empty = value.clone();
        empty["availableModelIds"] = json!([]);
        assert!(Credential::from_json(&empty)
            .expect("credential")
            .available_model_ids
            .is_none());
    }

    #[test]
    fn copilot_catalog_keeps_only_usable_picker_models() {
        let body = json!({ "data": [
            {
                "id": "gpt-5.4",
                "model_picker_enabled": true,
                "policy": { "state": "enabled" },
                "capabilities": { "supports": { "tool_calls": true } }
            },
            {
                "id": "claude-sonnet-5",
                "model_picker_enabled": true,
                "policy": { "state": "unconfigured" }
            },
            { "id": "hidden", "model_picker_enabled": false, "policy": { "state": "enabled" } },
            { "id": "blocked", "model_picker_enabled": true, "policy": { "state": "disabled" } },
            {
                "id": "no-tools",
                "model_picker_enabled": true,
                "policy": { "state": "enabled" },
                "capabilities": { "supports": { "tool_calls": false } }
            },
            {
                "id": "gpt-5.4",
                "model_picker_enabled": true,
                "policy": { "state": "enabled" }
            }
        ]});
        assert_eq!(
            parse_copilot_model_ids(&body, true).expect("catalog"),
            vec!["gpt-5.4".to_string(), "claude-sonnet-5".to_string()]
        );

        // Individual accounts sometimes report every picker flag as false.
        let fallback = json!({ "data": [
            { "id": "gpt-5.4", "model_picker_enabled": false, "policy": { "state": "enabled" } },
            { "id": "gpt-5.5", "model_picker_enabled": false, "policy": { "state": "unconfigured" } }
        ]});
        assert_eq!(
            parse_copilot_model_ids(&fallback, true).expect("catalog"),
            vec!["gpt-5.4".to_string()]
        );
        assert!(parse_copilot_model_ids(&fallback, false)
            .expect("catalog")
            .is_empty());
        assert!(parse_copilot_model_ids(&json!({}), true).is_err());
    }

    #[test]
    fn copilot_policy_disabled_models_become_enable_candidates() {
        let body = json!({ "data": [
            {
                "id": "blocked",
                "model_picker_enabled": true,
                "policy": { "state": "disabled" }
            },
            {
                "id": "hidden-blocked",
                "model_picker_enabled": false,
                "policy": { "state": "disabled" }
            },
            {
                "id": "ready",
                "model_picker_enabled": true,
                "policy": { "state": "enabled" }
            }
        ]});
        let catalog = parse_copilot_catalog(&body, false).expect("catalog");
        assert_eq!(catalog.available, vec!["ready".to_string()]);
        assert_eq!(catalog.policy_disabled, vec!["blocked".to_string()]);
    }

    #[test]
    fn radius_catalog_reads_gateway_model_ids() {
        let body = json!({
            "baseUrl": "https://radius.pi.dev",
            "models": [
                { "id": "model-a", "name": "A" },
                { "id": "" },
                { "name": "no id" },
                { "id": "model-a" }
            ]
        });
        assert_eq!(
            parse_radius_model_ids(&body).expect("catalog"),
            vec!["model-a".to_string()]
        );
        assert!(parse_radius_model_ids(&json!({ "models": {} })).is_err());
    }

    #[test]
    fn copilot_api_base_follows_the_token_proxy() {
        assert_eq!(
            copilot_api_base(
                "tid=1;proxy-ep=proxy.individual.githubcopilot.com;exp=2",
                None
            ),
            "https://api.individual.githubcopilot.com"
        );
        assert_eq!(
            copilot_api_base("tid=1", Some("company.ghe.com")),
            "https://copilot-api.company.ghe.com"
        );
        assert_eq!(copilot_api_base("tid=1", None), COPILOT_INDIVIDUAL_BASE);
    }
}
