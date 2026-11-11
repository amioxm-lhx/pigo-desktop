//! User-owned MCP servers: connect, discover tools, execute them, and answer
//! the Settings connection test.
//!
//! Tool names reach the model as `mcp_<serverId>_<toolName>` (D193). Host-core
//! gates permission, risk and contract modes exactly like `plugin_*` tools and
//! forwards admitted calls over the same `plugins.execute` dispatch, so this
//! module is the desktop runner for both namespaces: the event handler picks it
//! by prefix.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{oneshot, Mutex};

use crate::state::AppState;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const CALL_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_TOOLS_PER_SERVER: usize = 64;
/// tools/list pagination cap (gap 2): a server that never stops paging is cut
/// off here instead of looping forever.
const MAX_LIST_PAGES: usize = 8;
/// ADR 0038: at most eight servers per plugin.
const MAX_PLUGIN_SERVERS: usize = 8;
const PROTOCOL_VERSION: &str = "2024-11-05";

pub fn new_runtime() -> Arc<McpRuntime> {
    Arc::new(McpRuntime {
        connections: Mutex::new(HashMap::new()),
        tools: std::sync::RwLock::new(HashMap::new()),
        records: std::sync::RwLock::new(HashMap::new()),
    })
}

pub struct McpRuntime {
    /// One live client per server id, created on first use and reused.
    connections: Mutex<HashMap<String, Arc<Mutex<Connection>>>>,
    /// Exposed tool name → the server and tool it came from.
    tools: std::sync::RwLock<HashMap<String, ToolRef>>,
    /// Active server records from the last discovery, for reconnect.
    records: std::sync::RwLock<HashMap<String, Value>>,
}

#[derive(Clone)]
struct ToolRef {
    server_id: String,
    tool_name: String,
}

enum Connection {
    Stdio(StdioConnection),
    Http(HttpConnection),
}

/// In-flight stdio request id → the oneshot the reader task resolves with the
/// reply envelope. The read loop lives in one dedicated task (gap 1, docs
/// mcp-local-integration.md §4) instead of being inlined into request().
type StdioPending = std::sync::Mutex<HashMap<u64, oneshot::Sender<Value>>>;

struct StdioConnection {
    /// Dropping the child (with `kill_on_drop`) ends the server process,
    /// which EOFs stdout and ends the reader task.
    _child: tokio::process::Child,
    /// Shared with the reader task so it can answer server→client requests.
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    /// Requests awaiting their reply, keyed by id.
    pending: Arc<StdioPending>,
    /// Request ids the server cancelled via notifications/cancelled; the next
    /// matching reply is turned into an error instead of a fake result.
    cancelled: Arc<std::sync::Mutex<std::collections::HashSet<u64>>>,
    next_id: u64,
    initialized: bool,
}

struct HttpConnection {
    url: String,
    headers: Vec<(String, String)>,
    session_id: Option<String>,
    next_id: u64,
    initialized: bool,
}

impl McpRuntime {
    /// Discover the active servers' tools for one prompt. A server that fails
    /// to connect is logged and left out; it must not fail the whole launch.
    pub async fn definitions(
        &self,
        state: &Arc<AppState>,
        project_path: Option<&str>,
    ) -> Vec<Value> {
        let reply = state
            .host
            .call("mcp.active", json!({ "projectPath": project_path }))
            .await;
        let servers = match reply {
            Ok(value) => value
                .get("servers")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            Err(error) => {
                log::warn!("mcp.active failed: {error}");
                return Vec::new();
            }
        };
        let mut tools = HashMap::new();
        let mut records = HashMap::new();
        let mut definitions = Vec::new();
        for server in servers {
            let id = server.get("id").and_then(Value::as_str).unwrap_or("");
            if id.is_empty() || server.get("enabled").and_then(Value::as_bool) == Some(false) {
                continue;
            }
            let discovered = match self.list_tools(id, &server).await {
                Ok(tools) => tools,
                Err(message) => {
                    log::warn!("MCP server {id} is unavailable: {message}");
                    continue;
                }
            };
            records.insert(id.to_string(), server.clone());
            for tool in discovered.iter().take(MAX_TOOLS_PER_SERVER) {
                let Some(tool_name) = tool.get("name").and_then(Value::as_str) else {
                    continue;
                };
                if tool_name.is_empty() {
                    continue;
                }
                let exposed = exposed_name(id, tool_name);
                tools.insert(
                    exposed.clone(),
                    ToolRef {
                        server_id: id.to_string(),
                        tool_name: tool_name.to_string(),
                    },
                );
                definitions.push(json!({
                    "name": exposed,
                    "description": tool
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                    // A third-party schema never declares its own risk (spec
                    // 07-plugins/04 §7.1): MCP tools are always medium.
                    "risk": "medium",
                    "planSafeActions": [],
                    "parameters": tool
                        .get("inputSchema")
                        .cloned()
                        .unwrap_or_else(|| json!({ "type": "object", "properties": {} })),
                }));
            }
        }
        self.plugin_tools_into(
            state,
            project_path,
            &mut tools,
            &mut records,
            &mut definitions,
        )
        .await;
        if let Ok(mut known) = self.tools.write() {
            *known = tools;
        }
        if let Ok(mut known) = self.records.write() {
            *known = records;
        }
        definitions
    }

    /// Append tools from `contributes.mcpServers` of the plugins active for
    /// this project. Declaring a server grants nothing: the manifest still has
    /// to hold its transport permission, and a remote endpoint must be inside
    /// the plugin's `net.domains` allowlist.
    async fn plugin_tools_into(
        &self,
        state: &Arc<AppState>,
        project_path: Option<&str>,
        tools: &mut HashMap<String, ToolRef>,
        records: &mut HashMap<String, Value>,
        definitions: &mut Vec<Value>,
    ) {
        let rows = match crate::plugins::plugin_rows(state).await {
            Ok(rows) => rows,
            Err(_) => return,
        };
        for plugin in &rows {
            let plugin_id = crate::plugins::plugin_id(plugin);
            if plugin_id.is_empty() || !crate::plugins::active_in_project(plugin, project_path) {
                continue;
            }
            let Some(manifest) = crate::plugins::read_manifest(plugin) else {
                continue;
            };
            let values = crate::plugins::stored_plugin_values(&state.data_dir, plugin_id);
            for entry in crate::plugins::contributions(&manifest, "mcpServers")
                .iter()
                .take(MAX_PLUGIN_SERVERS)
            {
                let record = match plugin_mcp_record(plugin, &manifest, &values, entry) {
                    Ok(record) => record,
                    Err(message) => {
                        log::warn!("plugin {plugin_id} MCP server skipped: {message}");
                        continue;
                    }
                };
                let server_id = record
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if server_id.is_empty() || records.contains_key(&server_id) {
                    continue;
                }
                let discovered = match self.list_tools(&server_id, &record).await {
                    Ok(tools) => tools,
                    Err(message) => {
                        log::warn!(
                            "plugin {plugin_id} MCP server {server_id} unavailable: {message}"
                        );
                        continue;
                    }
                };
                records.insert(server_id.clone(), record);
                for tool in discovered.iter().take(MAX_TOOLS_PER_SERVER) {
                    let Some(tool_name) = tool
                        .get("name")
                        .and_then(Value::as_str)
                        .filter(|name| !name.is_empty())
                    else {
                        continue;
                    };
                    let exposed = exposed_name(&server_id, tool_name);
                    if tools.contains_key(&exposed) {
                        continue;
                    }
                    tools.insert(
                        exposed.clone(),
                        ToolRef {
                            server_id: server_id.clone(),
                            tool_name: tool_name.to_string(),
                        },
                    );
                    definitions.push(json!({
                        "name": exposed,
                        "description": tool
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                        "risk": "medium",
                        "planSafeActions": [],
                        "parameters": tool
                            .get("inputSchema")
                            .cloned()
                            .unwrap_or_else(|| json!({ "type": "object", "properties": {} })),
                    }));
                }
            }
        }
    }

    /// Drop a plugin's discovered tools and close its live server connections.
    /// Called from unload and from the crash supervisor so a disabled plugin
    /// cannot keep answering through a connection opened before it went away.
    pub async fn forget_plugin(&self, plugin_id: &str) {
        let prefix = format!("plugin.{plugin_id}.");
        if let Ok(mut tools) = self.tools.write() {
            tools.retain(|_, tool| !tool.server_id.starts_with(&prefix));
        }
        if let Ok(mut records) = self.records.write() {
            records.retain(|id, _| !id.starts_with(&prefix));
        }
        self.connections
            .lock()
            .await
            .retain(|id, _| !id.starts_with(&prefix));
    }

    /// Run one admitted `mcp_*` call dispatched by host-core.
    pub async fn execute(&self, params: &Value) -> Value {
        let name = params.get("toolName").and_then(Value::as_str).unwrap_or("");
        let tool = self
            .tools
            .read()
            .ok()
            .and_then(|tools| tools.get(name).cloned());
        let Some(tool) = tool else {
            return failure(
                "TOOL_NOT_FOUND",
                format!("MCP tool {name} is not registered"),
            );
        };
        let record = self
            .records
            .read()
            .ok()
            .and_then(|records| records.get(&tool.server_id).cloned());
        let Some(record) = record else {
            return failure(
                "MCP_SERVER_NOT_CONNECTED",
                format!("MCP server {} has no live connection", tool.server_id),
            );
        };
        let connection = match self.connection(&tool.server_id, &record).await {
            Ok(connection) => connection,
            Err(message) => return failure("MCP_SERVER_UNAVAILABLE", message),
        };
        let args = params.get("args").cloned().unwrap_or_else(|| json!({}));
        let mut client = connection.lock().await;
        match client
            .request(
                "tools/call",
                json!({ "name": tool.tool_name, "arguments": args }),
                CALL_TIMEOUT,
            )
            .await
        {
            Ok(result) => {
                let is_error = result
                    .get("isError")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if is_error {
                    json!({ "ok": false, "errorCode": "TOOL_FAILED", "content": result })
                } else {
                    json!({ "ok": true, "content": result })
                }
            }
            Err(message) => failure("TOOL_FAILED", message),
        }
    }

    async fn list_tools(&self, server_id: &str, record: &Value) -> Result<Vec<Value>, String> {
        let connection = self.connection(server_id, record).await?;
        list_tools_paginated(&connection, server_id).await
    }

    async fn connection(
        &self,
        server_id: &str,
        record: &Value,
    ) -> Result<Arc<Mutex<Connection>>, String> {
        if let Some(existing) = self.connections.lock().await.get(server_id).cloned() {
            return Ok(existing);
        }
        let connection = Arc::new(Mutex::new(Connection::connect(record).await?));
        let mut connections = self.connections.lock().await;
        Ok(connections
            .entry(server_id.to_string())
            .or_insert(connection)
            .clone())
    }
}

impl Connection {
    async fn connect(record: &Value) -> Result<Self, String> {
        let transport = record
            .get("transport")
            .and_then(Value::as_str)
            .unwrap_or("stdio");
        if transport == "http" {
            let url = record
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if url.is_empty() {
                return Err("MCP server has no url".into());
            }
            let headers = string_map(record.get("headers"));
            return Ok(Self::Http(HttpConnection {
                url,
                headers,
                session_id: None,
                next_id: 1,
                initialized: false,
            }));
        }
        let command = record
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if command.is_empty() {
            return Err("MCP server has no command".into());
        }
        let env = record.get("env").and_then(Value::as_object);
        // Gap 3: Windows' CreateProcess neither consults PATHEXT nor executes
        // batch files directly, so a bare `npx` cannot start as written.
        // resolve_windows_command looks the name up on PATH and routes
        // .cmd/.bat launchers through `cmd /c`.
        let (program, command_prefix) = resolve_windows_command(&command);
        let args: Vec<String> = record
            .get("args")
            .and_then(Value::as_array)
            .map(|args| {
                args.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let mut full_args = command_prefix;
        full_args.extend(args);
        let mut process = tokio::process::Command::new(&program);
        process
            .args(&full_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // stderr would otherwise fill the pipe and stall the server. It is
            // intentionally not surfaced (the launch contract predates gap 1);
            // stdout log lines are skipped by the reader instead.
            .stderr(Stdio::null())
            .kill_on_drop(true);
        // A plugin-declared server never inherits the host environment: it
        // holds provider keys. PATH and process basics still cross or a bare
        // command name could not be found (ADR 0038 §6).
        if let Some(plugin_id) = record.get("pluginId").and_then(Value::as_str) {
            process.env_clear();
            for key in [
                "PATH",
                "HOME",
                "TMPDIR",
                "TEMP",
                "TMP",
                "LANG",
                "LC_ALL",
                "SystemRoot",
                "WINDIR",
                "ComSpec",
                "USERPROFILE",
            ] {
                if let Ok(value) = std::env::var(key) {
                    process.env(key, value);
                }
            }
            process.env("PI_PLUGIN_ID", plugin_id);
            if let Some(root) = record.get("pluginRoot").and_then(Value::as_str) {
                process.current_dir(root);
            }
        }
        if let Some(env) = env {
            for (key, value) in env {
                if let Some(value) = value.as_str() {
                    process.env(key, value);
                }
            }
        }
        let mut child = process
            .spawn()
            .map_err(|error| format!("could not start MCP server {command}: {error}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "MCP server stdin is unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "MCP server stdout is unavailable".to_string())?;
        let stdin = Arc::new(Mutex::new(stdin));
        let pending: Arc<StdioPending> = Arc::default();
        let cancelled: Arc<std::sync::Mutex<std::collections::HashSet<u64>>> = Arc::default();
        spawn_stdio_reader(BufReader::new(stdout), stdin.clone(), pending.clone(), cancelled.clone());
        Ok(Self::Stdio(StdioConnection {
            _child: child,
            stdin,
            pending,
            cancelled,
            next_id: 1,
            initialized: false,
        }))
    }

    async fn initialize(&mut self, timeout: Duration) -> Result<(), String> {
        let already = match self {
            Self::Stdio(stdio) => stdio.initialized,
            Self::Http(http) => http.initialized,
        };
        if already {
            return Ok(());
        }
        let params = json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "pigo-desktop", "version": crate::state::APP_VERSION },
        });
        let result = self.request("initialize", params, timeout).await?;
        let _ = self.notify("notifications/initialized", json!({})).await;
        // A server that answers with an explicit error is unusable even when
        // the envelope itself parsed.
        if result.get("protocolVersion").is_none() {
            return Err("MCP server did not complete the initialize handshake".into());
        }
        match self {
            Self::Stdio(stdio) => stdio.initialized = true,
            Self::Http(http) => http.initialized = true,
        }
        Ok(())
    }

    async fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        match self {
            Self::Stdio(stdio) => stdio.request(method, params, timeout).await,
            Self::Http(http) => http.request(method, params, timeout).await,
        }
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        match self {
            Self::Stdio(stdio) => {
                let payload = json!({ "jsonrpc": "2.0", "method": method, "params": params });
                let line = format!("{payload}\n");
                stdio
                    .stdin
                    .lock()
                    .await
                    .write_all(line.as_bytes())
                    .await
                    .map_err(|error| error.to_string())
            }
            Self::Http(http) => http.notify(method, params).await,
        }
    }
}

impl StdioConnection {
    /// Send one request and await its reply from the reader task via the
    /// pending map. Writing and reading are decoupled (gap 1): out-of-order or
    /// foreign traffic can no longer stall the caller, and a server that
    /// closes stdout fails every in-flight call instead of timing out one.
    async fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        // Register before writing: a fast reply must never race the map insert.
        let (sender, receiver) = oneshot::channel();
        if let Some(stale) = self.pending.lock().unwrap().insert(id, sender) {
            // A timed-out request's slot got reused? Impossible (ids are
            // unique), but dropping a stale sender only fails that old call.
            drop(stale);
        }
        let line = format!("{payload}\n");
        if let Err(error) = self.stdin.lock().await.write_all(line.as_bytes()).await {
            self.pending.lock().unwrap().remove(&id);
            return Err(format!("MCP server stdin: {error}"));
        }
        let reply = match tokio::time::timeout(timeout, receiver).await {
            // On timeout the call is abandoned: the reader drops the sender
            // slot, and a late reply is reported as unknown-request-id.
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                return Err(format!("MCP {method} timed out"));
            }
            Ok(Err(_dropped)) => return Err("MCP server closed its output".into()),
            Ok(Ok(reply)) => reply,
        };
        // The server told us it stopped working on this call (gap 1: the
        // notification used to be dropped silently and the caller waited out
        // the whole timeout).
        if self.cancelled.lock().unwrap().remove(&id) {
            return Err(format!("MCP {method} was cancelled by the server"));
        }
        if let Some(error) = reply.get("error") {
            return Err(error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("MCP server returned an error")
                .to_string());
        }
        Ok(reply.get("result").cloned().unwrap_or(Value::Null))
    }
}

impl HttpConnection {
    async fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let url = self.url.clone();
        let headers = self.headers.clone();
        let session = self.session_id.clone();
        let body = payload.to_string();
        let (value, session) = tokio::task::spawn_blocking(move || {
            http_post(&url, &headers, session.as_deref(), &body, timeout)
        })
        .await
        .map_err(|error| error.to_string())??;
        if session.is_some() {
            self.session_id = session;
        }
        if let Some(error) = value.get("error") {
            return Err(error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("MCP server returned an error")
                .to_string());
        }
        Ok(value.get("result").cloned().unwrap_or(Value::Null))
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        let payload = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        let url = self.url.clone();
        let headers = self.headers.clone();
        let session = self.session_id.clone();
        let body = payload.to_string();
        let _ = tokio::task::spawn_blocking(move || {
            http_post(&url, &headers, session.as_deref(), &body, CONNECT_TIMEOUT)
        })
        .await;
        Ok(())
    }
}

fn http_post(
    url: &str,
    headers: &[(String, String)],
    session_id: Option<&str>,
    body: &str,
    timeout: Duration,
) -> Result<(Value, Option<String>), String> {
    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    let mut request = agent
        .post(url)
        .set("content-type", "application/json")
        .set("accept", "application/json, text/event-stream");
    if let Some(session_id) = session_id {
        request = request.set("mcp-session-id", session_id);
    }
    for (name, value) in headers {
        request = request.set(name, value);
    }
    let response = match request.send_string(body) {
        Ok(response) => response,
        Err(ureq::Error::Status(code, response)) => {
            let text = response.into_string().unwrap_or_default();
            return Err(format!("MCP server answered HTTP {code}: {text}"));
        }
        Err(error) => return Err(format!("MCP request failed: {error}")),
    };
    let session = response.header("mcp-session-id").map(str::to_string);
    let content_type = response.header("content-type").unwrap_or("").to_string();
    let text = response
        .into_string()
        .map_err(|error| format!("MCP response: {error}"))?;
    let value = if content_type.contains("text/event-stream") {
        sse_payload(&text).ok_or_else(|| "MCP server sent no JSON-RPC event".to_string())?
    } else {
        serde_json::from_str(&text).map_err(|error| format!("MCP response: {error}"))?
    };
    Ok((value, session))
}

/// Streamable HTTP servers answer with one `data:` line per JSON-RPC message.
fn sse_payload(text: &str) -> Option<Value> {
    for line in text.lines() {
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        if let Ok(value) = serde_json::from_str::<Value>(data.trim()) {
            if value.get("id").is_some() || value.get("result").is_some() {
                return Some(value);
            }
        }
    }
    None
}

/// Resolve `env` / `headers` from literal strings and `{ "setting": "<key>" }`
/// references into the plugin's own settings (D018). A missing referenced
/// setting fails the server instead of starting it without its credential.
fn resolve_refs(
    value: Option<&Value>,
    values: &serde_json::Map<String, Value>,
) -> Result<Vec<(String, String)>, String> {
    let Some(object) = value.and_then(Value::as_object) else {
        return Ok(Vec::new());
    };
    let mut resolved = Vec::new();
    for (key, entry) in object {
        let text = if let Some(text) = entry.as_str() {
            text.to_string()
        } else if let Some(setting) = entry.get("setting").and_then(Value::as_str) {
            match values.get(setting) {
                Some(Value::String(text)) => text.clone(),
                Some(Value::Number(number)) => number.to_string(),
                Some(Value::Bool(flag)) => flag.to_string(),
                _ => return Err(format!("setting \"{setting}\" is not configured")),
            }
        } else {
            continue;
        };
        resolved.push((key.clone(), text));
    }
    Ok(resolved)
}

fn string_object(pairs: Vec<(String, String)>) -> Value {
    Value::Object(
        pairs
            .into_iter()
            .map(|(key, value)| (key, Value::String(value)))
            .collect(),
    )
}

/// Turn one `contributes.mcpServers` entry into the record the MCP runtime
/// already understands. Everything the server may reach is checked here.
fn plugin_mcp_record(
    plugin: &Value,
    manifest: &Value,
    values: &serde_json::Map<String, Value>,
    entry: &Value,
) -> Result<Value, String> {
    let server_id = entry.get("id").and_then(Value::as_str).unwrap_or("");
    if server_id.is_empty() {
        return Err("server id is missing".into());
    }
    let transport = entry.get("transport").and_then(Value::as_str).unwrap_or("");
    let permission = match transport {
        "stdio" => "mcp.server.local",
        "http" => "mcp.server.remote",
        _ => {
            return Err(format!(
                "server {server_id} transport must be stdio or http"
            ))
        }
    };
    if !crate::plugins::has_permission(plugin, permission) {
        return Err(format!("server {server_id} requires {permission}"));
    }
    let plugin_id = crate::plugins::plugin_id(plugin);
    let key = format!("plugin.{plugin_id}.{server_id}");
    if transport == "http" {
        let url = entry
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let parsed =
            url::Url::parse(url).map_err(|error| format!("server {server_id} url: {error}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(format!("server {server_id} url must be http(s)"));
        }
        let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
        let domains = crate::plugin_bridge::net_domains(manifest).map_err(|error| error.message)?;
        if !crate::plugin_bridge::host_allowed(&host, &domains) {
            return Err(format!("{host} is not in manifest.net.domains"));
        }
        let headers = resolve_refs(entry.get("headers"), values)
            .map_err(|error| format!("server {server_id} headers: {error}"))?;
        return Ok(json!({
            "id": key,
            "transport": "http",
            "url": url,
            "headers": string_object(headers),
            "pluginId": plugin_id,
        }));
    }
    let root = crate::plugins::plugin_root(plugin);
    let command = entry
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let command = if command.is_empty() {
        return Err(format!("server {server_id} has no command"));
    } else if command.contains('/') || command.contains('\\') {
        let root = root
            .as_ref()
            .ok_or_else(|| format!("server {server_id} has no plugin root"))?;
        let path = crate::plugins::safe_join(root, command)
            .ok_or_else(|| format!("server {server_id} command escapes the plugin"))?;
        if !path.exists() {
            return Err(format!("server {server_id} command is missing"));
        }
        path.to_string_lossy().to_string()
    } else {
        command.to_string()
    };
    let args = entry
        .get("args")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let env = resolve_refs(entry.get("env"), values)
        .map_err(|error| format!("server {server_id} env: {error}"))?;
    let mut record = json!({
        "id": key,
        "transport": "stdio",
        "command": command,
        "args": args,
        "env": string_object(env),
        "pluginId": plugin_id,
    });
    if let Some(root) = root {
        record["pluginRoot"] = json!(root.to_string_lossy());
    }
    Ok(record)
}

fn string_map(value: Option<&Value>) -> Vec<(String, String)> {
    value
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn exposed_name(server_id: &str, tool_name: &str) -> String {
    format!("mcp_{}_{}", safe_token(server_id), safe_token(tool_name))
}

fn safe_token(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            out.push(character);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "unnamed".to_string()
    } else {
        out
    }
}

/// Gap 1 (docs/mcp-local-integration.md §4): one task owns stdout for the
/// connection's whole lifetime and routes every line to its consumer —
/// replies resolve the pending map, server→client requests are answered with
/// method-not-supported so the server's own timeout does not wedge the pipe,
/// and notifications are logged (cancelled ids fail in-flight calls fast).
/// Modeled on the Go client's read loop (internal/jsonrpc); the old in-request
/// sync loop dropped any interleaved traffic and could never make progress
/// past it.
fn spawn_stdio_reader(
    mut stdout: BufReader<tokio::process::ChildStdout>,
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    pending: Arc<StdioPending>,
    cancelled: Arc<std::sync::Mutex<std::collections::HashSet<u64>>>,
) {
    tokio::spawn(async move {
        loop {
            let mut buffer = String::new();
            match stdout.read_line(&mut buffer).await {
                Ok(0) | Err(_) => break, // EOF or pipe error: fail everything below
                Ok(_) => {}
            }
            let Ok(value) = serde_json::from_str::<Value>(buffer.trim()) else {
                continue; // a log line on stdout is not an answer
            };
            match classify_message(&value) {
                MessageKind::Reply => {
                    if let Some(id) = message_id(&value) {
                        if let Some(sender) = pending.lock().unwrap().remove(&id) {
                            let _ = sender.send(value);
                        } else {
                            log::warn!("MCP server sent a reply for unknown request id {id}");
                        }
                    }
                }
                MessageKind::ServerRequest(method) => {
                    log::warn!(
                        "MCP server request {method} is not supported; answering method-not-supported"
                    );
                    let reply = json!({
                        "jsonrpc": "2.0",
                        "id": value.get("id").cloned().unwrap_or(Value::Null),
                        "error": {
                            "code": -32601,
                            "message": format!("pigo-desktop does not support {method}"),
                        },
                    });
                    let line = format!("{reply}\n");
                    let _ = stdin.lock().await.write_all(line.as_bytes()).await;
                }
                MessageKind::Notification(method) => {
                    if method == "notifications/cancelled" {
                        if let Some(id) = value.pointer("/params/id").and_then(Value::as_u64) {
                            cancelled.lock().unwrap().insert(id);
                        }
                    }
                    log::debug!("MCP server notification: {method}");
                }
            }
        }
        // EOF or pipe error: every in-flight request fails immediately —
        // dropping the senders makes the receivers report "server closed its
        // output" instead of waiting out their timeouts.
        pending.lock().unwrap().clear();
    });
}

/// The three shapes a stdio line can take.
#[derive(Debug)]
enum MessageKind {
    /// A reply to one of our requests (id present, no method).
    Reply,
    /// A request the server sent to us (id + method): sampling/createMessage,
    /// roots/list, elicitation/create.
    ServerRequest(String),
    /// A one-way notification (method, no id): progress, cancelled.
    Notification(String),
}

/// Classify one decoded JSON-RPC message for the reader's dispatch.
fn classify_message(value: &Value) -> MessageKind {
    let method = value.get("method").and_then(Value::as_str);
    match (method, message_id(value)) {
        (Some(method), Some(_)) => MessageKind::ServerRequest(method.to_string()),
        (Some(method), None) => MessageKind::Notification(method.to_string()),
        _ => MessageKind::Reply,
    }
}

/// The request id of a message, when it is a real id (present and not null).
fn message_id(value: &Value) -> Option<u64> {
    value
        .get("id")
        .filter(|id| !id.is_null())
        .and_then(Value::as_u64)
}

/// The nextCursor of a tools/list (or resources/list) reply, if the server
/// says there is another page.
fn next_cursor(result: &Value) -> Option<String> {
    result
        .get("nextCursor")
        .and_then(Value::as_str)
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_string)
}

/// Gap 3: Windows' CreateProcess neither consults PATHEXT nor executes batch
/// files directly, so a bare `npx` or `uvx` cannot start as written. Resolve
/// the command the same way the CLI does (internal/mcp/client.go
/// resolveCommand): look a bare name up on PATH honoring PATHEXT, and route
/// .cmd/.bat launchers through `cmd /c <path>`. Non-Windows targets pass
/// through unchanged.
fn resolve_windows_command(command: &str) -> (String, Vec<String>) {
    if !cfg!(windows) {
        return (command.to_string(), Vec::new());
    }
    let ext = std::path::Path::new(command)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !ext.is_empty() {
        if ext == "cmd" || ext == "bat" {
            return ("cmd".to_string(), vec!["/c".to_string(), command.to_string()]);
        }
        return (command.to_string(), Vec::new());
    }
    match locate_on_path(command) {
        Some(path) => {
            let ext = std::path::Path::new(&path)
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext == "cmd" || ext == "bat" {
                ("cmd".to_string(), vec!["/c".to_string(), path])
            } else {
                (path, Vec::new())
            }
        }
        // Unresolvable: pass the name through and let spawn report the
        // failure with a clear error.
        None => (command.to_string(), Vec::new()),
    }
}

/// Search PATH × PATHEXT for a bare command name, first hit in PATHEXT order.
/// The search set is injected for tests; locate_on_path supplies the process
/// environment.
fn locate_on_path_in(path_var: &str, pathext: &str, command: &str) -> Option<String> {
    let exts: Vec<&str> = pathext
        .split(';')
        .map(str::trim)
        .filter(|ext| ext.starts_with('.'))
        .collect();
    for dir in path_var.split(';') {
        if dir.is_empty() {
            continue;
        }
        for ext in &exts {
            let candidate = std::path::Path::new(dir).join(format!("{command}{ext}"));
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }
    None
}

fn locate_on_path(command: &str) -> Option<String> {
    let path_var = std::env::var("PATH").ok()?;
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    locate_on_path_in(&path_var, &pathext, command)
}

fn failure(code: &str, message: String) -> Value {
    json!({ "ok": false, "errorCode": code, "content": { "error": message } })
}

/// Follow the tools/list cursor pagination on a connection (gap 2): merge
/// pages until the server is done, the page cap trips, or the per-server tool
/// cap is reached — servers that page their catalogs were silently truncated
/// to page one. Shared by the runtime's tool catalog and `mcp/test`.
async fn list_tools_paginated(
    connection: &Arc<Mutex<Connection>>,
    server_id: &str,
) -> Result<Vec<Value>, String> {
    let mut client = connection.lock().await;
    client.initialize(CONNECT_TIMEOUT).await?;
    let mut tools = Vec::new();
    let mut cursor: Option<String> = None;
    for _page in 0..MAX_LIST_PAGES {
        let params = match &cursor {
            Some(cursor) => json!({ "cursor": cursor }),
            None => json!({}),
        };
        let result = client.request("tools/list", params, CONNECT_TIMEOUT).await?;
        if let Some(page) = result.get("tools").and_then(Value::as_array).cloned() {
            tools.extend(page);
        }
        if tools.len() >= MAX_TOOLS_PER_SERVER {
            log::warn!(
                "MCP server {server_id} exposes more than {MAX_TOOLS_PER_SERVER} tools; the list is truncated"
            );
            tools.truncate(MAX_TOOLS_PER_SERVER);
            return Ok(tools);
        }
        cursor = next_cursor(&result);
        if cursor.is_none() {
            return Ok(tools);
        }
    }
    log::warn!(
        "MCP server {server_id} kept sending cursors after {MAX_LIST_PAGES} pages; the tool list may be incomplete"
    );
    Ok(tools)
}

/// `mcp/test`: one isolated connection through the supplied config.
pub async fn test(state: &Arc<AppState>, params: &Value) -> Value {
    let server_id = params
        .get("id")
        .or_else(|| params.get("serverId"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let failed = |message: String| {
        json!({ "status": {
            "serverId": server_id,
            "state": "failed",
            "toolCount": 0,
            "message": message,
            "updatedAt": crate::events::now_ms(),
        }})
    };
    if server_id.is_empty() {
        return failed("server id required".into());
    }
    let reply = state.host.call("mcp.list", params.clone()).await;
    let record = match reply {
        Ok(value) => value
            .get("servers")
            .and_then(Value::as_array)
            .and_then(|servers| {
                servers
                    .iter()
                    .find(|server| server.get("id").and_then(Value::as_str) == Some(server_id))
                    .cloned()
            }),
        Err(error) => return failed(error.to_string()),
    };
    let Some(record) = record else {
        return failed(format!("MCP server {server_id} is not configured"));
    };
    // A test connection is disposable: connect, handshake, list, drop. The
    // same paginated listing the runtime uses keeps toolCount honest for
    // servers that page their catalog.
    let connection = match Connection::connect(&record).await {
        Ok(connection) => Arc::new(Mutex::new(connection)),
        Err(message) => return failed(message),
    };
    match list_tools_paginated(&connection, server_id).await {
        Ok(tools) => {
            let names: Vec<Value> = tools
                .iter()
                .take(MAX_TOOLS_PER_SERVER)
                .filter_map(|tool| tool.get("name").cloned())
                .collect();
            json!({ "status": {
                "serverId": server_id,
                "state": "ready",
                "toolCount": names.len(),
                "toolNames": names,
                "updatedAt": crate::events::now_ms(),
            }})
        }
        Err(message) => failed(message),
    }
}

/// `mcp/import`: accept a pasted `{"mcpServers": {...}}` block (Claude
/// Desktop's shape) or a bare map, and write each record through host-core so
/// its own validation and scope rules stay the only writer.
pub async fn import(state: &Arc<AppState>, params: &Value) -> Value {
    let text = params.get("text").and_then(Value::as_str).unwrap_or("");
    let parsed: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(error) => {
            return json!({ "imported": [], "failed": [{ "id": "", "reason": format!("invalid JSON: {error}") }] })
        }
    };
    let block = parsed.get("mcpServers").cloned().unwrap_or(parsed);
    let Some(servers) = block.as_object() else {
        return json!({ "imported": [], "failed": [{ "id": "", "reason": "expected an object of MCP servers" }] });
    };
    let mut imported = Vec::new();
    let mut failed = Vec::new();
    for (id, config) in servers {
        let input = server_input(id, config);
        match state
            .host
            .call("mcp.upsert", json!({ "server": input }))
            .await
        {
            Ok(value) => imported.push(
                value
                    .get("server")
                    .cloned()
                    .unwrap_or_else(|| json!({ "id": id })),
            ),
            Err(error) => failed.push(json!({ "id": id, "reason": error.to_string() })),
        }
    }
    json!({ "imported": imported, "failed": failed })
}

fn server_input(id: &str, config: &Value) -> Value {
    let command = config
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let url = config
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let transport = if !command.is_empty() {
        "stdio"
    } else if !url.is_empty() {
        "http"
    } else {
        ""
    };
    let mut input = json!({
        "id": id,
        "label": config.get("label").and_then(Value::as_str).unwrap_or(id),
        "transport": transport,
        "enabled": true,
    });
    for (field, target) in [
        ("description", "description"),
        ("command", "command"),
        ("url", "url"),
        ("level", "level"),
        ("projectPath", "projectPath"),
    ] {
        if let Some(value) = config.get(field).and_then(Value::as_str) {
            input[target] = json!(value);
        }
    }
    for (field, target) in [("args", "args"), ("env", "env"), ("headers", "headers")] {
        if let Some(value) = config.get(field) {
            input[target] = value.clone();
        }
    }
    input
}

#[cfg(test)]
mod tests {
    use super::{
        classify_message, exposed_name, locate_on_path_in, message_id, new_runtime, next_cursor,
        plugin_mcp_record, resolve_refs, resolve_windows_command, safe_token, server_input,
        sse_payload, Connection, MessageKind, ToolRef, CONNECT_TIMEOUT,
    };
    use serde_json::json;

    #[test]
    fn exposed_names_are_namespaced_and_safe() {
        assert_eq!(
            exposed_name("github", "search_repos"),
            "mcp_github_search_repos"
        );
        assert_eq!(
            exposed_name("my-server", "tool.v2"),
            "mcp_my_server_tool_v2"
        );
        assert_eq!(safe_token(""), "unnamed");
    }

    // Gap 1: message classification for the reader task's dispatch.

    #[test]
    fn replies_are_classified_by_absent_method() {
        let reply = json!({ "jsonrpc": "2.0", "id": 7, "result": {} });
        assert!(matches!(classify_message(&reply), MessageKind::Reply));
        // A reply envelope whose error replaces result is still a reply.
        let error = json!({ "jsonrpc": "2.0", "id": 8, "error": { "code": -1 } });
        assert!(matches!(classify_message(&error), MessageKind::Reply));
    }

    #[test]
    fn server_requests_and_notifications_are_distinguished_by_id() {
        // sampling/createMessage with an id: the server asks us something.
        let request = json!({ "jsonrpc": "2.0", "id": 3, "method": "sampling/createMessage" });
        match classify_message(&request) {
            MessageKind::ServerRequest(method) => assert_eq!(method, "sampling/createMessage"),
            other => panic!("expected ServerRequest, got {other:?}"),
        }
        // The same method without an id is a one-way notification.
        let notification = json!({ "jsonrpc": "2.0", "method": "notifications/progress" });
        match classify_message(&notification) {
            MessageKind::Notification(method) => assert_eq!(method, "notifications/progress"),
            other => panic!("expected Notification, got {other:?}"),
        }
        // An id of null is not a real id (some servers emit id: null for
        // notifications), so it must classify as a notification.
        let null_id = json!({ "jsonrpc": "2.0", "id": null, "method": "notifications/progress" });
        assert!(matches!(classify_message(&null_id), MessageKind::Notification(_)));
    }

    #[test]
    fn message_ids_accept_u64_and_reject_null() {
        assert_eq!(message_id(&json!({ "id": 42 })), Some(42));
        assert_eq!(message_id(&json!({ "id": null })), None);
        assert_eq!(message_id(&json!({})), None);
    }

    // Gap 2: tools/list cursor pagination helpers.

    #[test]
    fn next_cursor_reads_and_filters_empty_strings() {
        assert_eq!(next_cursor(&json!({ "nextCursor": "page-2" })), Some("page-2".into()));
        assert_eq!(next_cursor(&json!({ "nextCursor": "" })), None);
        assert_eq!(next_cursor(&json!({})), None);
        // A numeric nextCursor is not a cursor (the protocol uses strings).
        assert_eq!(next_cursor(&json!({ "nextCursor": 5 })), None);
    }

    // Gap 3: Windows command resolution.

    #[test]
    fn batch_launchers_ride_cmd_c() {
        let (program, prefix) = resolve_windows_command("npx.cmd");
        assert_eq!(program, "cmd");
        assert_eq!(prefix, vec!["/c".to_string(), "npx.cmd".to_string()]);
        let (program, prefix) = resolve_windows_command("C:/tools/server.bat");
        assert_eq!(program, "cmd");
        assert_eq!(prefix, vec!["/c".to_string(), "C:/tools/server.bat".to_string()]);
    }

    #[test]
    fn real_executables_and_paths_pass_through() {
        let (program, prefix) = resolve_windows_command("server.exe");
        assert_eq!(program, "server.exe");
        assert!(prefix.is_empty());
        let (program, prefix) = resolve_windows_command("C:/bin/server.exe");
        assert_eq!(program, "C:/bin/server.exe");
        assert!(prefix.is_empty());
    }

    #[test]
    fn bare_names_resolve_on_path_in_pathext_order() {
        // dirA has the .exe first in PATHEXT order; dirB only has the .cmd.
        let dir = std::env::temp_dir().join("pigo-mcp-pathext-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a")).unwrap();
        std::fs::create_dir_all(dir.join("b")).unwrap();
        std::fs::write(dir.join("a").join("tool.EXE"), b"mz").unwrap();
        std::fs::write(dir.join("b").join("wrapper.CMD"), b"@echo off").unwrap();
        let path_var = format!("{};{}", dir.join("a").display(), dir.join("b").display());
        // PATHEXT order puts .EXE before .CMD, and dir a comes first: the exe wins.
        let found = locate_on_path_in(&path_var, ".COM;.EXE;.BAT;.CMD", "tool").unwrap();
        assert!(found.ends_with("tool.EXE"), "found = {found}");
        // A name that only exists as a batch file resolves to the .cmd file...
        let found = locate_on_path_in(&path_var, ".COM;.EXE;.BAT;.CMD", "wrapper").unwrap();
        assert!(found.ends_with("wrapper.CMD"), "found = {found}");
        // An unknown bare name passes through unchanged so spawn reports it.
        let (program, prefix) = resolve_windows_command("definitely-not-installed-tool");
        if cfg!(windows) {
            assert_eq!(program, "definitely-not-installed-tool");
            assert!(prefix.is_empty());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plugin_mcp_refs_resolve_from_plugin_settings_only() {
        let mut values = serde_json::Map::new();
        values.insert("token".into(), json!("t0ken"));
        let resolved = resolve_refs(
            Some(&json!({ "TOKEN": { "setting": "token" }, "LITERAL": "x" })),
            &values,
        )
        .unwrap();
        assert_eq!(
            resolved,
            vec![
                ("LITERAL".to_string(), "x".to_string()),
                ("TOKEN".to_string(), "t0ken".to_string()),
            ]
        );
        let missing = resolve_refs(Some(&json!({ "TOKEN": { "setting": "absent" } })), &values);
        assert!(missing.unwrap_err().contains("not configured"));
    }

    #[test]
    fn plugin_mcp_records_require_permission_and_network_allowlist() {
        let manifest = json!({ "net": { "domains": ["mcp.example.test"] } });
        let values = serde_json::Map::new();
        let entry = json!({
            "id": "remote",
            "transport": "http",
            "url": "https://mcp.example.test/mcp",
        });
        let unpermitted = json!({ "id": "demo.mcp", "permissions": [] });
        assert!(plugin_mcp_record(&unpermitted, &manifest, &values, &entry)
            .unwrap_err()
            .contains("mcp.server.remote"));

        let permitted = json!({ "id": "demo.mcp", "permissions": ["mcp.server.remote"] });
        let outside = json!({
            "id": "remote",
            "transport": "http",
            "url": "https://other.example.test/mcp",
        });
        assert!(plugin_mcp_record(&permitted, &manifest, &values, &outside)
            .unwrap_err()
            .contains("net.domains"));

        let record = plugin_mcp_record(&permitted, &manifest, &values, &entry).unwrap();
        assert_eq!(record["id"], "plugin.demo.mcp.remote");
        assert_eq!(record["pluginId"], "demo.mcp");
    }

    #[test]
    fn a_plugin_relative_mcp_command_stays_inside_the_package() {
        let dir = std::env::temp_dir().join(format!("pigo-mcp-plugin-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::write(dir.join("bin/serve"), b"#!/bin/sh\n").unwrap();
        let plugin = json!({
            "id": "demo.mcp",
            "path": dir.to_string_lossy(),
            "permissions": ["mcp.server.local"],
        });
        let manifest = json!({});
        let values = serde_json::Map::new();
        let record = plugin_mcp_record(
            &plugin,
            &manifest,
            &values,
            &json!({ "id": "local", "transport": "stdio", "command": "./bin/serve" }),
        )
        .unwrap();
        assert_eq!(
            record["command"],
            json!(dir.join("bin/serve").to_string_lossy())
        );
        assert_eq!(record["pluginRoot"], json!(dir.to_string_lossy()));
        let escape = plugin_mcp_record(
            &plugin,
            &manifest,
            &values,
            &json!({ "id": "bad", "transport": "stdio", "command": "../outside" }),
        );
        assert!(escape.unwrap_err().contains("escapes the plugin"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn forgetting_a_plugin_drops_only_its_mcp_tools() {
        let runtime = new_runtime();
        {
            let mut tools = runtime.tools.write().unwrap();
            tools.insert(
                "mcp_plugin_demo_a_x".into(),
                ToolRef {
                    server_id: "plugin.demo.a.x".into(),
                    tool_name: "x".into(),
                },
            );
            tools.insert(
                "mcp_plugin_demo_b_y".into(),
                ToolRef {
                    server_id: "plugin.demo.b.y".into(),
                    tool_name: "y".into(),
                },
            );
        }
        runtime.forget_plugin("demo.a").await;
        let tools = runtime.tools.read().unwrap();
        assert!(!tools.contains_key("mcp_plugin_demo_a_x"));
        assert!(tools.contains_key("mcp_plugin_demo_b_y"));
    }

    #[test]
    fn sse_payload_finds_the_jsonrpc_event() {
        let text =
            "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"tools\":[]}}\n\n";
        let value = sse_payload(text).unwrap();
        assert_eq!(value["id"], 2);
    }

    #[test]
    fn pasted_servers_map_to_the_stored_input_shape() {
        let stdio = server_input(
            "github",
            &json!({ "command": "npx", "args": ["-y", "server"], "env": { "TOKEN": "x" } }),
        );
        assert_eq!(stdio["transport"], "stdio");
        assert_eq!(stdio["label"], "github");
        assert_eq!(stdio["args"][1], "server");
        let http = server_input("web", &json!({ "url": "https://example.com/mcp" }));
        assert_eq!(http["transport"], "http");
    }

    #[tokio::test]
    async fn a_stdio_server_completes_the_handshake_and_answers_tools() {
        // Node is the same runtime the plugin host requires; without it this
        // integration check is meaningless rather than a failure.
        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let dir = std::env::temp_dir().join(format!("pigo-mcp-fake-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("server.js");
        std::fs::write(
            &script,
            r#"
const readline = require("node:readline");
const rl = readline.createInterface({ input: process.stdin });
rl.on("line", (line) => {
  let message;
  try { message = JSON.parse(line); } catch { return; }
  const reply = (result) => {
    if (message.id === undefined) return;
    console.log(JSON.stringify({ jsonrpc: "2.0", id: message.id, result }));
  };
  if (message.method === "initialize") {
    reply({ protocolVersion: "2024-11-05", capabilities: {}, serverInfo: { name: "fake", version: "1" } });
  } else if (message.method === "tools/list") {
    reply({ tools: [{ name: "echo", description: "echo", inputSchema: { type: "object", properties: { text: { type: "string" } } } }] });
  } else if (message.method === "tools/call") {
    reply({ content: [{ type: "text", text: JSON.stringify(message.params.arguments) }] });
  }
});
"#,
        )
        .unwrap();

        let record = json!({
            "transport": "stdio",
            "command": "node",
            "args": [script.to_string_lossy()],
        });
        let mut connection = Connection::connect(&record).await.unwrap();
        connection.initialize(CONNECT_TIMEOUT).await.unwrap();
        let tools = connection
            .request("tools/list", json!({}), CONNECT_TIMEOUT)
            .await
            .unwrap();
        assert_eq!(tools["tools"][0]["name"], "echo");
        let call = connection
            .request(
                "tools/call",
                json!({ "name": "echo", "arguments": { "text": "hi" } }),
                CONNECT_TIMEOUT,
            )
            .await
            .unwrap();
        assert_eq!(call["content"][0]["text"], "{\"text\":\"hi\"}");
        drop(connection);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
