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
use tokio::sync::Mutex;

use crate::state::AppState;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const CALL_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_TOOLS_PER_SERVER: usize = 64;
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

struct StdioConnection {
    /// Dropping the child (with `kill_on_drop`) ends the server process.
    _child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
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
        let mut client = connection.lock().await;
        client.initialize(CONNECT_TIMEOUT).await?;
        let result = client
            .request("tools/list", json!({}), CONNECT_TIMEOUT)
            .await?;
        Ok(result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
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
        let env = record.get("env").and_then(Value::as_object);
        let mut process = tokio::process::Command::new(&command);
        process
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // stderr would otherwise fill the pipe and stall the server.
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
        Ok(Self::Stdio(StdioConnection {
            _child: child,
            stdin,
            stdout: BufReader::new(stdout),
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
                    .write_all(line.as_bytes())
                    .await
                    .map_err(|error| error.to_string())
            }
            Self::Http(http) => http.notify(method, params).await,
        }
    }
}

impl StdioConnection {
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
        let line = format!("{payload}\n");
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|error| format!("MCP server stdin: {error}"))?;
        loop {
            let mut buffer = String::new();
            let read = tokio::time::timeout(timeout, self.stdout.read_line(&mut buffer)).await;
            let count = match read {
                Err(_) => return Err(format!("MCP {method} timed out")),
                Ok(Err(error)) => return Err(format!("MCP server stdout: {error}")),
                Ok(Ok(count)) => count,
            };
            if count == 0 {
                return Err("MCP server closed its output".into());
            }
            let Ok(value) = serde_json::from_str::<Value>(buffer.trim()) else {
                // Not every server speaks one JSON object per line; a log line
                // on stdout is ignored rather than treated as the answer.
                continue;
            };
            if value.get("id").and_then(Value::as_i64) != Some(id as i64) {
                continue;
            }
            if let Some(error) = value.get("error") {
                return Err(error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("MCP server returned an error")
                    .to_string());
            }
            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
        }
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

fn failure(code: &str, message: String) -> Value {
    json!({ "ok": false, "errorCode": code, "content": { "error": message } })
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
    // A test connection is disposable: connect, handshake, list, drop.
    let mut connection = match Connection::connect(&record).await {
        Ok(connection) => connection,
        Err(message) => return failed(message),
    };
    if let Err(message) = connection.initialize(CONNECT_TIMEOUT).await {
        return failed(message);
    }
    match connection
        .request("tools/list", json!({}), CONNECT_TIMEOUT)
        .await
    {
        Ok(result) => {
            let names: Vec<Value> = result
                .get("tools")
                .and_then(Value::as_array)
                .map(|tools| {
                    tools
                        .iter()
                        .take(MAX_TOOLS_PER_SERVER)
                        .filter_map(|tool| tool.get("name").cloned())
                        .collect()
                })
                .unwrap_or_default();
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
        exposed_name, new_runtime, plugin_mcp_record, resolve_refs, safe_token, server_input,
        sse_payload, Connection, ToolRef, CONNECT_TIMEOUT,
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
