//! Node.js plugin JavaScript host.
//!
//! Each active plugin runs in its own `node` process (`plugin_host.js`,
//! embedded below), mirroring the Electron build's one-utilityProcess-per-
//! plugin isolation. The child speaks NDJSON JSON-RPC over stdio; the reverse
//! direction lets plugin code reach shell services (`pi.workspace.get`,
//! `pi.plugin.getSettings`, `pi.ui.showToast`, `pi.themes.upsert`, …).
//!
//! Node is a runtime dependency of the plugin surface only: without it the
//! rest of the shell keeps working, plugin processes never start (the list
//! stays metadata-only and execution answers `PLUGIN_NOT_LOADED`). Set
//! `PI_DESKTOP_NODE_BIN` to point at a specific binary.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};
use tauri::{Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot, watch};

use crate::rpc::RpcClient;
use crate::state::APP_VERSION;

const SCRIPT_SOURCE: &str = include_str!("plugin_host.js");
const LOAD_TIMEOUT: Duration = Duration::from_secs(15);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const TOOL_TIMEOUT: Duration = Duration::from_secs(110);
const UNLOAD_TIMEOUT: Duration = Duration::from_secs(5);
const STOP_GRACE: Duration = Duration::from_secs(2);
/// The child gives each `service.start` a 5s budget; the frame allows for the
/// reply and a busy event loop on top of it.
const SERVICE_START_TIMEOUT: Duration = Duration::from_secs(10);
/// ADR 0040: five supervised restarts, then stay down.
const RESTART_MAX_ATTEMPTS: u32 = 5;
/// A process that stays up this long is healthy and resets the backoff.
const RESTART_HEALTHY_AFTER: Duration = Duration::from_secs(60);
/// A command can prompt the owner several times; each prompt may wait up to
/// the spec's five minutes, so the outer call is deliberately generous.
const EXTENSION_COMMAND_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// One interactive prompt (spec 16 §9).
const EXTENSION_PROMPT_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone)]
pub struct HostError {
    pub code: String,
    pub message: String,
}

impl HostError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// Resolve the `node` binary: explicit override, then PATH, then the usual
/// installer locations. A packaged app has no PATH guarantee, so the fallback
/// list matters there.
pub fn resolve_node_binary() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("PI_DESKTOP_NODE_BIN") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Some(path);
        }
    }
    let name = if cfg!(windows) { "node.exe" } else { "node" };
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let candidates: &[&str] = if cfg!(windows) {
        &[r"C:\Program Files\nodejs\node.exe"]
    } else {
        &[
            "/opt/homebrew/bin/node",
            "/usr/local/bin/node",
            "/usr/bin/node",
        ]
    };
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|candidate| candidate.is_file())
}

/// Write the embedded host script into the data dir and return its path.
/// Rewriting on every launch keeps a stale copy from surviving an upgrade.
fn ensure_script(data_dir: &Path) -> Result<PathBuf, HostError> {
    let dir = data_dir.join("plugin-host");
    std::fs::create_dir_all(&dir)
        .map_err(|error| HostError::new("PLUGIN_HOST_UNAVAILABLE", error.to_string()))?;
    let path = dir.join("plugin_host.js");
    let stale = std::fs::read_to_string(&path)
        .map(|existing| existing != SCRIPT_SOURCE)
        .unwrap_or(true);
    if stale {
        std::fs::write(&path, SCRIPT_SOURCE)
            .map_err(|error| HostError::new("PLUGIN_HOST_UNAVAILABLE", error.to_string()))?;
    }
    Ok(path)
}

type HostRequestFn = Arc<
    dyn Fn(
            String,
            Value,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, HostError>> + Send>>
        + Send
        + Sync,
>;

/// One plugin's Node process. Owns its pending call table and the process
/// supervisor; the shell-facing registry lives on `PluginHost`.
struct PluginProcess {
    plugin_id: String,
    exec_path: PathBuf,
    stdin: mpsc::Sender<String>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, HostError>>>>>,
    next_id: AtomicU64,
    kill: mpsc::Sender<()>,
    closed: watch::Sender<bool>,
    exited: Arc<AtomicBool>,
}

impl PluginProcess {
    fn spawn(
        node: &Path,
        script: &Path,
        plugin_id: String,
        exec_path: PathBuf,
        handler: HostRequestFn,
    ) -> Result<Arc<Self>, HostError> {
        let mut command = Command::new(node);
        command
            .arg(script)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = command
            .spawn()
            .map_err(|error| HostError::new("PLUGIN_HOST_UNAVAILABLE", error.to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| HostError::new("PLUGIN_HOST_UNAVAILABLE", "node stdin missing"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| HostError::new("PLUGIN_HOST_UNAVAILABLE", "node stdout missing"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| HostError::new("PLUGIN_HOST_UNAVAILABLE", "node stderr missing"))?;

        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(256);
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(line) = stdin_rx.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
                if stdin.write_all(b"\n").await.is_err() {
                    break;
                }
                let _ = stdin.flush().await;
            }
        });

        let stderr_plugin = plugin_id.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log::info!(target: "plugin", "[{stderr_plugin}] {line}");
            }
        });

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, HostError>>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (kill_tx, mut kill_rx) = mpsc::channel::<()>(1);
        let (closed_tx, _closed_rx) = watch::channel(false);
        let exited = Arc::new(AtomicBool::new(false));

        // Reader: replies resolve calls; frames with `method` are reverse
        // requests answered on the same stdin channel.
        {
            let pending = pending.clone();
            let stdin_for_requests = stdin_tx.clone();
            let plugin_label = plugin_id.clone();
            let handler = handler.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                                continue;
                            };
                            let method = message
                                .get("method")
                                .and_then(|value| value.as_str())
                                .map(str::to_string);
                            let id = message.get("id").cloned().unwrap_or(Value::Null);
                            match (method, id.is_null()) {
                                (Some(method), false) => {
                                    let handler = handler.clone();
                                    let stdin = stdin_for_requests.clone();
                                    tokio::spawn(async move {
                                        let params =
                                            message.get("params").cloned().unwrap_or(Value::Null);
                                        let reply = match handler(method, params).await {
                                            Ok(result) => {
                                                json!({ "jsonrpc": "2.0", "id": id, "result": result })
                                            }
                                            Err(error) => json!({
                                                "jsonrpc": "2.0",
                                                "id": id,
                                                "error": {
                                                    "code": -32000,
                                                    "message": error.message,
                                                    "data": { "errorCode": error.code }
                                                }
                                            }),
                                        };
                                        let _ = stdin.send(reply.to_string()).await;
                                    });
                                }
                                (Some(_), true) => {}
                                (None, false) => {
                                    let Some(numeric) = id.as_u64() else { continue };
                                    let sender = pending.lock().unwrap().remove(&numeric);
                                    let Some(sender) = sender else { continue };
                                    let outcome = if let Some(error) = message.get("error") {
                                        let message = error
                                            .get("message")
                                            .and_then(|value| value.as_str())
                                            .unwrap_or("plugin host error");
                                        let code = error
                                            .pointer("/data/errorCode")
                                            .and_then(|value| value.as_str())
                                            .unwrap_or("PLUGIN_ERROR");
                                        Err(HostError::new(code, message))
                                    } else {
                                        Ok(message.get("result").cloned().unwrap_or(Value::Null))
                                    };
                                    let _ = sender.send(outcome);
                                }
                                (None, true) => {}
                            }
                        }
                        Ok(None) | Err(_) => break,
                    }
                }
                log::debug!(target: "plugin", "[{plugin_label}] stdout closed");
            });
        }

        // Supervisor: owns the child so it can kill it without a second
        // handle, and fails every in-flight call on exit.
        {
            let pending = pending.clone();
            let closed = closed_tx.clone();
            let exited = exited.clone();
            let plugin_label = plugin_id.clone();
            tokio::spawn(async move {
                tokio::select! {
                    _ = child.wait() => {}
                    _ = kill_rx.recv() => {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                    }
                }
                exited.store(true, Ordering::SeqCst);
                let mut table = pending.lock().unwrap();
                for (_, sender) in table.drain() {
                    let _ = sender.send(Err(HostError::new(
                        "PLUGIN_CRASHED",
                        format!("plugin {plugin_label} exited"),
                    )));
                }
                drop(table);
                let _ = closed.send(true);
                log::warn!(target: "plugin", "[{plugin_label}] process exited");
            });
        }

        Ok(Arc::new(Self {
            plugin_id,
            exec_path,
            stdin: stdin_tx,
            pending,
            next_id: AtomicU64::new(1),
            kill: kill_tx,
            closed: closed_tx,
            exited,
        }))
    }

    async fn call(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, HostError> {
        if self.exited.load(Ordering::SeqCst) {
            return Err(HostError::new(
                "PLUGIN_CRASHED",
                format!("plugin {} is not running", self.plugin_id),
            ));
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if self.stdin.send(request.to_string()).await.is_err() {
            self.pending.lock().unwrap().remove(&id);
            return Err(HostError::new(
                "PLUGIN_CRASHED",
                format!("plugin {} stdin is closed", self.plugin_id),
            ));
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(_)) => Err(HostError::new("PLUGIN_CRASHED", "plugin call dropped")),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(HostError::new(
                    "PLUGIN_TIMEOUT",
                    format!("{method} timed out after {}s", timeout.as_secs()),
                ))
            }
        }
    }

    /// One-way parent → child frame: no id, no reply, no backpressure.
    async fn notify(&self, method: &str, params: Value) -> Result<(), HostError> {
        if self.exited.load(Ordering::SeqCst) {
            return Err(HostError::new(
                "PLUGIN_CRASHED",
                format!("plugin {} is not running", self.plugin_id),
            ));
        }
        let frame = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.stdin.send(frame.to_string()).await.map_err(|_| {
            HostError::new(
                "PLUGIN_CRASHED",
                format!("plugin {} stdin is closed", self.plugin_id),
            )
        })
    }

    async fn terminate(&self) {
        if self.exited.load(Ordering::SeqCst) {
            return;
        }
        let _ = self.call("unload", json!({}), UNLOAD_TIMEOUT).await;
        if self.exited.load(Ordering::SeqCst) {
            return;
        }
        // Ask the child to exit, then force it after the grace window.
        let notice = json!({ "jsonrpc": "2.0", "method": "shutdown" });
        let _ = self.stdin.send(notice.to_string()).await;
        let mut closed = self.closed.subscribe();
        if tokio::time::timeout(STOP_GRACE, closed.wait_for(|value| *value))
            .await
            .is_err()
        {
            let _ = self.kill.send(()).await;
            let _ = tokio::time::timeout(STOP_GRACE, closed.wait_for(|value| *value)).await;
        }
    }
}

#[derive(Clone)]
struct CommandRow {
    plugin_id: String,
    command_id: String,
    title: String,
    keywords: Vec<String>,
    category: Option<String>,
}

#[derive(Clone)]
struct ToolRow {
    plugin_id: String,
    tool_name: String,
    full_name: String,
    description: String,
    risk: String,
    plan_safe_actions: Vec<String>,
    parameters: Value,
}

/// One service declared in `contributes.services`. The manifest is the source
/// of truth; `pi.services.register` only supplies the handlers.
#[derive(Clone)]
struct DeclaredService {
    id: String,
    label: String,
    auto_restart: bool,
}

fn declared_services(manifest: &Value) -> Vec<DeclaredService> {
    crate::plugins::contributions(manifest, "services")
        .iter()
        .take(4)
        .filter_map(|entry| {
            let id = entry.get("id").and_then(Value::as_str)?.to_string();
            let label = entry
                .get("label")
                .map(crate::plugins::localized)
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| id.clone());
            let auto_restart = entry
                .get("autoRestart")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            Some(DeclaredService {
                id,
                label,
                auto_restart,
            })
        })
        .collect()
}

/// Supervisor state for one service, read over `plugin/services`.
#[derive(Clone)]
struct ServiceStatus {
    label: String,
    state: String,
    restarts: u32,
    message: Option<String>,
}

/// Restart bookkeeping for one plugin. `generation` invalidates a pending
/// backoff timer after a manual enable/disable/reload.
#[derive(Default, Clone, Copy)]
struct RestartSlot {
    attempts: u32,
    generation: u64,
}

fn restart_delay(attempt: u32) -> Duration {
    Duration::from_secs(1u64 << attempt.min(4)).min(Duration::from_secs(30))
}

#[derive(Clone)]
struct ExtensionCommandRow {
    plugin_id: String,
    name: String,
    description: String,
    extension_id: String,
    extension_label: String,
}

/// One interactive prompt waiting for the renderer. The sender resolves the
/// extension's `await ctx.ui.confirm/select/input` call.
struct PendingPrompt {
    session_id: String,
    prompt: Value,
    sender: Option<oneshot::Sender<Option<Value>>>,
}

#[derive(Default)]
struct Registry {
    commands: HashMap<String, CommandRow>,
    tools: HashMap<String, ToolRow>,
    themes: HashMap<String, Vec<Value>>,
    /// Extension commands keyed by their declared slash name; the first
    /// registration wins, matching the collision rule for the other kinds.
    extension_commands: HashMap<String, ExtensionCommandRow>,
    /// What the most recent load reported for one plugin's extensions.
    extension_reports: HashMap<String, Value>,
    /// Loadable entries, for `trustedExtensions` in the sidecar launch payload.
    extension_entries: HashMap<String, Vec<Value>>,
}

/// Shell-side plugin registry plus the child-process table.
pub struct PluginHost {
    data_dir: PathBuf,
    host: Arc<RpcClient>,
    /// Discovered plugin MCP tools are pruned here when the plugin goes away.
    mcp: Arc<crate::mcp::McpRuntime>,
    app: Mutex<Option<tauri::AppHandle>>,
    node: Mutex<Option<PathBuf>>,
    script: Mutex<Option<PathBuf>>,
    surfaces: Option<Arc<crate::plugin_surface::PluginSurfaceServer>>,
    processes: tokio::sync::Mutex<HashMap<String, Arc<PluginProcess>>>,
    registry: Mutex<Registry>,
    /// Per-plugin `fs.requestDirectory` grant (canonical path, run lifetime).
    fs_grants: Mutex<HashMap<String, PathBuf>>,
    /// Paths this plugin wrote, for `fs.delete.own` cleanup without a scope.
    written_paths: Mutex<HashMap<String, HashSet<PathBuf>>>,
    /// Rolling one-minute window for plugin `agent.complete` calls.
    complete_calls: Mutex<HashMap<String, VecDeque<Instant>>>,
    /// Newest-first text entries recorded from plugin writes and Composer
    /// pastes. Images are intentionally not captured in this build.
    clipboard_texts: Mutex<VecDeque<Value>>,
    /// Plugins whose last load failed. Retrying a broken plugin on every
    /// prompt would add its load budget to each launch; a reload or a
    /// registry mutation clears the mark.
    failed: Mutex<HashSet<String>>,
    /// Resident-service state, keyed by `(pluginId, serviceId)`.
    services: Mutex<HashMap<(String, String), ServiceStatus>>,
    /// Restart attempts and generation per plugin (ADR 0040).
    restarts: Mutex<HashMap<String, RestartSlot>>,
    /// Project the supervisor should reload into after a crash.
    project_path: Mutex<Option<String>>,
    sync_lock: tokio::sync::Mutex<()>,
    missing_node_logged: AtomicBool,
    /// One interactive extension prompt per session is visible; the rest wait
    /// behind it in arrival order.
    pending_prompts: Mutex<HashMap<String, PendingPrompt>>,
    prompt_queues: Mutex<HashMap<String, VecDeque<String>>>,
}

impl PluginHost {
    pub fn new(
        data_dir: PathBuf,
        host: Arc<RpcClient>,
        surfaces: Option<Arc<crate::plugin_surface::PluginSurfaceServer>>,
        mcp: Arc<crate::mcp::McpRuntime>,
    ) -> Self {
        Self {
            data_dir,
            host,
            mcp,
            app: Mutex::new(None),
            node: Mutex::new(None),
            script: Mutex::new(None),
            surfaces,
            processes: tokio::sync::Mutex::new(HashMap::new()),
            registry: Mutex::new(Registry::default()),
            fs_grants: Mutex::new(HashMap::new()),
            written_paths: Mutex::new(HashMap::new()),
            complete_calls: Mutex::new(HashMap::new()),
            clipboard_texts: Mutex::new(VecDeque::new()),
            failed: Mutex::new(HashSet::new()),
            services: Mutex::new(HashMap::new()),
            restarts: Mutex::new(HashMap::new()),
            project_path: Mutex::new(None),
            sync_lock: tokio::sync::Mutex::new(()),
            missing_node_logged: AtomicBool::new(false),
            pending_prompts: Mutex::new(HashMap::new()),
            prompt_queues: Mutex::new(HashMap::new()),
        }
    }

    pub fn attach_app(&self, app: tauri::AppHandle) {
        *self.app.lock().unwrap() = Some(app);
    }

    pub(crate) fn app_handle(&self) -> Option<tauri::AppHandle> {
        self.app.lock().unwrap().clone()
    }

    fn emit(&self, channel: &str, payload: Value) {
        if let Some(app) = self.app.lock().unwrap().as_ref() {
            let _ = app.emit(channel, payload);
        }
    }

    fn node_binary(&self) -> Result<PathBuf, HostError> {
        if let Some(path) = self.node.lock().unwrap().clone() {
            return Ok(path);
        }
        let resolved = resolve_node_binary().ok_or_else(|| {
            HostError::new(
                "PLUGIN_HOST_UNAVAILABLE",
                "Node.js was not found; install Node 18+ or set PI_DESKTOP_NODE_BIN to run plugins",
            )
        })?;
        *self.node.lock().unwrap() = Some(resolved.clone());
        Ok(resolved)
    }

    fn script_path(&self) -> Result<PathBuf, HostError> {
        if let Some(path) = self.script.lock().unwrap().clone() {
            return Ok(path);
        }
        let path = ensure_script(&self.data_dir)?;
        *self.script.lock().unwrap() = Some(path.clone());
        Ok(path)
    }

    // ── registry queries ───────────────────────────────────────────────────

    pub fn search_commands(&self, query: &str) -> Vec<Value> {
        let query = query.trim().to_lowercase();
        let registry = self.registry.lock().unwrap();
        let mut rows: Vec<Value> = registry
            .commands
            .iter()
            .filter(|(full_id, row)| {
                if query.is_empty() {
                    return true;
                }
                full_id.to_lowercase().contains(&query)
                    || row.title.to_lowercase().contains(&query)
                    || row
                        .category
                        .as_deref()
                        .map(|category| category.to_lowercase().contains(&query))
                        .unwrap_or(false)
                    || row
                        .keywords
                        .iter()
                        .any(|keyword| keyword.to_lowercase().contains(&query))
            })
            .map(|(full_id, row)| {
                json!({
                    "id": full_id,
                    "title": row.title,
                    "category": row.category,
                    "keywords": row.keywords,
                    "source": "plugin",
                    "pluginId": row.plugin_id,
                })
            })
            .collect();
        rows.sort_by(|left, right| {
            left["title"]
                .as_str()
                .unwrap_or("")
                .cmp(right["title"].as_str().unwrap_or(""))
        });
        // Extension commands sort after the plugin ones in the palette.
        let mut extensions: Vec<Value> = registry
            .extension_commands
            .values()
            .filter(|row| {
                if query.is_empty() {
                    return true;
                }
                row.name.to_lowercase().contains(&query)
                    || row.description.to_lowercase().contains(&query)
            })
            .map(|row| {
                json!({
                    "id": format!("extension:{}", row.name),
                    "title": format!("/{}", row.name),
                    "category": "extension",
                    "keywords": [],
                    "source": "extension",
                    "pluginId": row.plugin_id,
                    "extensionId": row.extension_id,
                    "description": row.description,
                })
            })
            .collect();
        extensions.sort_by(|left, right| {
            left["title"]
                .as_str()
                .unwrap_or("")
                .cmp(right["title"].as_str().unwrap_or(""))
        });
        rows.extend(extensions);
        rows
    }

    /// Entries for the composer "/" menu. The typed name is the command id the
    /// plugin registered; the id stays the globally unique palette id.
    pub fn composer_commands(&self) -> Vec<Value> {
        let registry = self.registry.lock().unwrap();
        let mut rows: Vec<Value> = registry
            .commands
            .iter()
            .map(|(full_id, row)| {
                json!({
                    "name": row.command_id,
                    "kind": "plugin",
                    "title": row.title,
                    "id": full_id,
                    "category": row.category,
                })
            })
            .collect();
        rows.extend(registry.extension_commands.values().map(|row| {
            json!({
                "name": row.name,
                "kind": "extension",
                "title": row.name,
                "id": format!("extension:{}", row.name),
                "description": row.description,
                "source": "extension",
                "extensionId": row.extension_id,
                "extensionLabel": row.extension_label,
            })
        }));
        rows
    }

    /// Run one extension command in the plugin process that registered it.
    pub async fn run_extension_command(
        &self,
        session_id: &str,
        name: &str,
        args: &str,
        cwd: Option<&str>,
    ) -> Result<Value, HostError> {
        let row = self
            .registry
            .lock()
            .unwrap()
            .extension_commands
            .get(name)
            .cloned()
            .ok_or_else(|| {
                HostError::new(
                    "COMMAND_NOT_FOUND",
                    format!("extension command \"{name}\" is not registered"),
                )
            })?;
        let process = self.process(&row.plugin_id).await?;
        process
            .call(
                "runExtensionCommand",
                json!({
                    "sessionId": session_id,
                    "name": row.name,
                    "args": args,
                    "cwd": cwd,
                }),
                EXTENSION_COMMAND_TIMEOUT,
            )
            .await
    }

    /// Ask the renderer one interactive prompt and wait for its answer.
    async fn request_extension_ui(
        &self,
        session_id: &str,
        extension_id: &str,
        extension_label: &str,
        request: Value,
    ) -> Result<Value, HostError> {
        let kind = request
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        match kind.as_str() {
            "notify" => {
                let message = request.get("message").and_then(Value::as_str).unwrap_or("");
                self.emit(
                    "pi-desktop/app/event/toast",
                    json!({
                        "message": message,
                        "level": request.get("level").cloned().unwrap_or_else(|| json!("info")),
                    }),
                );
                Ok(json!({ "kind": "notify" }))
            }
            "setStatus" | "setWorkingMessage" => {
                let key = if kind == "setStatus" {
                    request.get("key").and_then(Value::as_str).unwrap_or("")
                } else {
                    "working"
                };
                self.emit(
                    "pi-desktop/extensions/event/status",
                    json!({
                        "sessionId": session_id,
                        "extensionId": extension_id,
                        "key": key,
                        "text": request.get("text").cloned().unwrap_or(Value::Null),
                    }),
                );
                Ok(json!({ "kind": kind }))
            }
            "confirm" | "select" | "input" => {
                let prompt_id = uuid::Uuid::new_v4().to_string();
                let prompt = json!({
                    "promptId": prompt_id,
                    "sessionId": session_id,
                    "extensionId": extension_id,
                    "extensionLabel": extension_label,
                    "request": request,
                });
                let (tx, rx) = oneshot::channel();
                let emit_now = {
                    let mut queues = self.prompt_queues.lock().unwrap();
                    let queue = queues.entry(session_id.to_string()).or_default();
                    queue.push_back(prompt_id.clone());
                    queue.len() == 1
                };
                self.pending_prompts.lock().unwrap().insert(
                    prompt_id.clone(),
                    PendingPrompt {
                        session_id: session_id.to_string(),
                        prompt: prompt.clone(),
                        sender: Some(tx),
                    },
                );
                self.audit_extension_prompt(&prompt);
                if emit_now {
                    self.emit("pi-desktop/extensions/event/uiPrompt", prompt);
                }
                let value = match tokio::time::timeout(EXTENSION_PROMPT_TIMEOUT, rx).await {
                    Ok(Ok(value)) => value,
                    _ => {
                        self.settle_extension_prompt(&prompt_id, None);
                        None
                    }
                };
                Ok(json!({ "kind": kind, "value": value }))
            }
            other => Err(HostError::new(
                "INVALID_ARGUMENT",
                format!("unknown extension UI request {other}"),
            )),
        }
    }

    /// Resolve one prompt and surface the next one waiting for its session.
    fn settle_extension_prompt(&self, prompt_id: &str, value: Option<Value>) {
        let Some(pending) = self.pending_prompts.lock().unwrap().remove(prompt_id) else {
            return;
        };
        let next_id = {
            let mut queues = self.prompt_queues.lock().unwrap();
            match queues.get_mut(&pending.session_id) {
                Some(queue) => {
                    let was_head = queue.front().map(|id| id == prompt_id).unwrap_or(false);
                    queue.retain(|id| id != prompt_id);
                    if was_head {
                        queue.front().cloned()
                    } else {
                        None
                    }
                }
                None => None,
            }
        };
        if let Some(sender) = pending.sender {
            let _ = sender.send(value);
        }
        if let Some(next_id) = next_id {
            if let Some(next) = self
                .pending_prompts
                .lock()
                .unwrap()
                .get(&next_id)
                .map(|entry| entry.prompt.clone())
            {
                self.audit_extension_prompt(&next);
                self.emit("pi-desktop/extensions/event/uiPrompt", next);
            }
        }
    }

    pub fn respond_extension_prompt(
        &self,
        prompt_id: &str,
        value: Option<Value>,
    ) -> Result<Value, HostError> {
        if !self.pending_prompts.lock().unwrap().contains_key(prompt_id) {
            return Err(HostError::new(
                "PROMPT_NOT_FOUND",
                format!("prompt {prompt_id} is not pending"),
            ));
        }
        self.settle_extension_prompt(prompt_id, value);
        Ok(json!({ "ok": true }))
    }

    /// Aborting a turn dismisses every prompt that session has open.
    pub fn abort_extension_prompts(&self, session_id: &str) {
        let ids: Vec<String> = self
            .prompt_queues
            .lock()
            .unwrap()
            .remove(session_id)
            .map(|queue| queue.into_iter().collect())
            .unwrap_or_default();
        let mut pending = self.pending_prompts.lock().unwrap();
        for id in ids {
            if let Some(entry) = pending.remove(&id) {
                if let Some(sender) = entry.sender {
                    let _ = sender.send(None);
                }
            }
        }
    }

    fn audit_extension_prompt(&self, prompt: &Value) {
        log::info!(
            target: "plugin",
            "extension prompt {}",
            serde_json::to_string(prompt).unwrap_or_default()
        );
    }

    /// Attach runtime extension state to host-core's plugin rows.
    pub fn annotate_plugin_rows(&self, value: &mut Value) {
        let reports = self.registry.lock().unwrap().extension_reports.clone();
        let Some(rows) = value.get_mut("plugins").and_then(Value::as_array_mut) else {
            return;
        };
        for row in rows {
            let Some(id) = row.get("id").and_then(Value::as_str) else {
                continue;
            };
            if let Some(report) = reports.get(id) {
                row["agentExtension"] = report.clone();
                continue;
            }
            // Enabled but not loaded in this run (scope, Node missing, or the
            // process has not started yet): the console still shows the chip.
            if !row.get("enabled").and_then(Value::as_bool).unwrap_or(false) {
                continue;
            }
            let declares = crate::plugins::read_manifest(row)
                .map(|manifest| {
                    manifest
                        .pointer("/contributes/agentExtensions")
                        .and_then(Value::as_array)
                        .map(|entries| !entries.is_empty())
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if declares {
                row["agentExtension"] = json!({
                    "state": "enabled",
                    "toolNames": [],
                    "commandNames": [],
                    "diagnostics": [],
                });
            }
        }
    }

    /// The Loader entries for a sidecar launch (`trustedExtensions`).
    pub fn trusted_extensions(&self) -> Vec<Value> {
        let entries = self.registry.lock().unwrap().extension_entries.clone();
        let mut rows: Vec<Value> = entries
            .into_values()
            .flatten()
            .map(|entry| {
                json!({
                    "id": entry.get("id").cloned().unwrap_or(Value::Null),
                    "entry": entry.get("entry").cloned().unwrap_or(Value::Null),
                    "label": entry.get("label").cloned().unwrap_or(Value::Null),
                    "source": "plugin",
                    "root": entry.get("root").cloned().unwrap_or(Value::Null),
                })
            })
            .collect();
        rows.sort_by(|left, right| {
            left["id"]
                .as_str()
                .unwrap_or("")
                .cmp(right["id"].as_str().unwrap_or(""))
        });
        rows
    }

    pub async fn run_command(&self, full_id: &str) -> Result<(), HostError> {
        let row = self
            .registry
            .lock()
            .unwrap()
            .commands
            .get(full_id)
            .cloned()
            .ok_or_else(|| {
                HostError::new("COMMAND_NOT_FOUND", format!("unknown command {full_id}"))
            })?;
        let process = self.process(&row.plugin_id).await?;
        process
            .call(
                "runCommand",
                json!({ "commandId": row.command_id }),
                COMMAND_TIMEOUT,
            )
            .await?;
        Ok(())
    }

    /// Tool definitions for `agent.prompt`. Only loaded plugins contribute:
    /// advertising a tool that cannot execute would fail every call.
    pub fn tool_definitions(&self) -> Vec<Value> {
        let registry = self.registry.lock().unwrap();
        let mut rows: Vec<Value> = registry
            .tools
            .values()
            .map(|tool| {
                json!({
                    "name": tool.full_name,
                    "description": tool.description,
                    "risk": tool.risk,
                    "planSafeActions": tool.plan_safe_actions,
                    "parameters": tool.parameters,
                    "pluginId": tool.plugin_id,
                })
            })
            .collect();
        rows.sort_by(|left, right| {
            left["name"]
                .as_str()
                .unwrap_or("")
                .cmp(right["name"].as_str().unwrap_or(""))
        });
        rows
    }

    pub fn runtime_themes(&self) -> Vec<Value> {
        self.registry
            .lock()
            .unwrap()
            .themes
            .values()
            .flat_map(|themes| themes.iter().cloned())
            .collect()
    }

    async fn process(&self, plugin_id: &str) -> Result<Arc<PluginProcess>, HostError> {
        self.processes
            .lock()
            .await
            .get(plugin_id)
            .cloned()
            .ok_or_else(|| {
                HostError::new(
                    "PLUGIN_NOT_LOADED",
                    format!("plugin {plugin_id} is not loaded"),
                )
            })
    }

    pub async fn execute_tool(&self, params: &Value) -> Value {
        let tool_name = params
            .get("toolName")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let mode = params
            .get("mode")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let args = params.get("args").cloned().unwrap_or(Value::Null);
        let failure = |code: &str, message: String| json!({ "ok": false, "errorCode": code, "content": { "error": message } });
        let row = self.registry.lock().unwrap().tools.get(tool_name).cloned();
        let Some(row) = row else {
            return failure(
                "TOOL_NOT_FOUND",
                format!("plugin tool {tool_name} is not registered"),
            );
        };
        // Host-core admits a plugin tool in a contract mode only when the
        // plugin declared plan-safe actions; the per-action gate lives here.
        if matches!(mode, "plan" | "goal") {
            if row.plan_safe_actions.is_empty() {
                return failure(
                    "PLUGIN_DISABLED_IN_PLAN",
                    format!("{tool_name} is not available in contract modes"),
                );
            }
            let action = args
                .get("action")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if !row
                .plan_safe_actions
                .iter()
                .any(|allowed| allowed == action)
            {
                return failure(
                    "PLUGIN_ACTION_NOT_PLAN_SAFE",
                    format!("action \"{action}\" is not plan-safe for {tool_name}"),
                );
            }
        }
        let process = match self.process(&row.plugin_id).await {
            Ok(process) => process,
            Err(error) => return failure(&error.code, error.message),
        };
        let payload = json!({
            "toolName": row.tool_name,
            "args": args,
            "sessionId": params.get("sessionId").cloned().unwrap_or(Value::Null),
            "turnId": params.get("turnId").cloned().unwrap_or(Value::Null),
            "mode": mode,
        });
        match process.call("executeTool", payload, TOOL_TIMEOUT).await {
            Ok(reply) => {
                let ok = reply
                    .get("ok")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                let mut result = json!({
                    "ok": ok,
                    "content": reply.get("content").cloned().unwrap_or(Value::Null),
                });
                if !ok {
                    let code = reply
                        .get("errorCode")
                        .and_then(|value| value.as_str())
                        .unwrap_or("TOOL_FAILED");
                    result["errorCode"] = json!(code);
                }
                result
            }
            Err(error) => failure(&error.code, error.message),
        }
    }

    // ── resident services ──────────────────────────────────────────────────

    /// Push one agent-loop event to every loaded plugin that brought agent
    /// extensions. Handlers run in the plugin's Node process; a plugin with no
    /// matching handler simply ignores the frame.
    pub async fn emit_extension_event(&self, session_id: &str, event: &str, payload: Value) {
        let targets: Vec<String> = {
            let registry = self.registry.lock().unwrap();
            registry
                .extension_entries
                .iter()
                .filter(|(_, entries)| !entries.is_empty())
                .map(|(plugin_id, _)| plugin_id.clone())
                .collect()
        };
        if targets.is_empty() {
            return;
        }
        let processes = self.processes.lock().await;
        for plugin_id in targets {
            let Some(process) = processes.get(&plugin_id) else {
                continue;
            };
            let params = json!({
                "event": event,
                "sessionId": session_id,
                "payload": payload,
            });
            if let Err(error) = process.notify("extension.event", params).await {
                log::warn!(
                    target: "plugin",
                    "plugin {plugin_id} extension event {event}: {error}"
                );
            }
        }
    }

    /// Live state for one declared service, or `None` when the plugin is not
    /// loaded. `plugin/services` falls back to `stopped` for a missing row.
    pub(crate) fn service_state(&self, plugin_id: &str, service_id: &str) -> Option<Value> {
        let services = self.services.lock().unwrap();
        let status = services.get(&(plugin_id.to_string(), service_id.to_string()))?;
        let mut row = json!({
            "label": status.label,
            "state": status.state,
            "restarts": status.restarts,
            "updatedAt": crate::events::now_ms(),
        });
        if let Some(message) = &status.message {
            row["message"] = json!(message);
        }
        Some(row)
    }

    fn set_service(
        &self,
        plugin_id: &str,
        service: &DeclaredService,
        state: &str,
        message: Option<String>,
    ) {
        let restarts = self
            .restarts
            .lock()
            .unwrap()
            .get(plugin_id)
            .map(|slot| slot.attempts)
            .unwrap_or(0);
        self.services.lock().unwrap().insert(
            (plugin_id.to_string(), service.id.clone()),
            ServiceStatus {
                label: service.label.clone(),
                state: state.to_string(),
                restarts,
                message,
            },
        );
    }

    fn set_services(
        &self,
        plugin_id: &str,
        declared: &[DeclaredService],
        state: &str,
        message: Option<String>,
    ) {
        for service in declared {
            self.set_service(plugin_id, service, state, message.clone());
        }
    }

    fn clear_services(&self, plugin_id: &str) {
        self.services
            .lock()
            .unwrap()
            .retain(|(owner, _), _| owner != plugin_id);
    }

    /// Ask the child to run the declared services. `load` has already
    /// completed, so `pi.services.register` has had its chance to supply the
    /// handlers. A per-service failure leaves the plugin loaded.
    async fn start_services(
        &self,
        plugin_id: &str,
        declared: &[DeclaredService],
        process: &PluginProcess,
    ) {
        if declared.is_empty() {
            return;
        }
        self.set_services(plugin_id, declared, "starting", None);
        let payload = json!({
            "services": declared
                .iter()
                .map(|service| json!({ "id": service.id, "autoRestart": service.auto_restart }))
                .collect::<Vec<_>>(),
        });
        let reply = match process
            .call("services.start", payload, SERVICE_START_TIMEOUT)
            .await
        {
            Ok(reply) => reply,
            Err(error) => {
                self.set_services(plugin_id, declared, "failed", Some(error.message));
                return;
            }
        };
        let reported = reply
            .get("services")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for service in declared {
            let row = reported
                .iter()
                .find(|row| row.get("id").and_then(Value::as_str) == Some(service.id.as_str()));
            let state = row
                .and_then(|row| row.get("state").and_then(Value::as_str))
                .filter(|state| matches!(*state, "starting" | "running" | "stopped" | "failed"))
                .unwrap_or("failed");
            let message = row
                .and_then(|row| row.get("message").and_then(Value::as_str))
                .map(str::to_string)
                .or_else(|| {
                    (row.is_none()).then(|| format!("service \"{}\" did not report", service.id))
                });
            self.set_service(plugin_id, service, state, message);
        }
    }

    /// Invalidate a pending backoff timer and clear the counter. Manual
    /// enable/disable/reload always outranks the supervisor.
    fn cancel_restart(&self, plugin_id: &str) {
        let mut slots = self.restarts.lock().unwrap();
        let slot = slots.entry(plugin_id.to_string()).or_default();
        slot.generation = slot.generation.wrapping_add(1);
        slot.attempts = 0;
    }

    fn cancel_all_restarts(&self) {
        for slot in self.restarts.lock().unwrap().values_mut() {
            slot.generation = slot.generation.wrapping_add(1);
            slot.attempts = 0;
        }
    }

    /// Whole-process restart with the ADR 0040 backoff. A crash already
    /// destroyed the process the service lived in, so the unit is the plugin.
    fn schedule_restart(self: &Arc<Self>, plugin_id: String, declared: Vec<DeclaredService>) {
        let (attempt, generation) = {
            let mut slots = self.restarts.lock().unwrap();
            let slot = slots.entry(plugin_id.clone()).or_default();
            let attempt = slot.attempts;
            slot.attempts = attempt.saturating_add(1);
            (attempt, slot.generation)
        };
        if attempt >= RESTART_MAX_ATTEMPTS {
            self.failed.lock().unwrap().insert(plugin_id.clone());
            self.set_services(
                &plugin_id,
                &declared,
                "failed",
                Some("restart limit reached".to_string()),
            );
            self.emit(
                "pi-desktop/app/event/toast",
                json!({ "message": format!("Plugin {plugin_id} stopped unexpectedly") }),
            );
            self.emit(
                "pi-desktop/event/pluginChanged",
                json!({ "reason": "failed", "pluginId": plugin_id }),
            );
            return;
        }
        let delay = restart_delay(attempt);
        self.set_services(
            &plugin_id,
            &declared,
            "starting",
            Some(format!("restarting in {}s", delay.as_secs())),
        );
        self.emit(
            "pi-desktop/event/pluginChanged",
            json!({ "reason": "restart", "pluginId": plugin_id }),
        );
        let weak = Arc::downgrade(self);
        // Box the restart task: it calls `reload_inner` -> `load_row`, and
        // `load_row` spawns the crash watcher that schedules this restart. The
        // concrete task type would therefore be recursive, which rust-analyzer
        // rejects as an opaque-type cycle even though rustc can infer it.
        let task: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> =
            Box::pin(async move {
                tokio::time::sleep(delay).await;
                let Some(host) = weak.upgrade() else { return };
                let current = host
                    .restarts
                    .lock()
                    .unwrap()
                    .get(&plugin_id)
                    .map(|slot| slot.generation)
                    .unwrap_or(0);
                if current != generation {
                    return;
                }
                let project = host.project_path.lock().unwrap().clone();
                let rows = match host.plugins_list().await {
                    Ok(rows) => rows,
                    Err(error) => {
                        log::warn!(
                            target: "plugin",
                            "plugin {plugin_id} restart skipped: {error}"
                        );
                        return;
                    }
                };
                let Some(row) = rows
                    .iter()
                    .find(|row| crate::plugins::plugin_id(row) == plugin_id)
                else {
                    return;
                };
                if !crate::plugins::active_in_project(row, project.as_deref()) {
                    return;
                }
                if let Err(error) = host.reload_inner(&plugin_id, project.as_deref()).await {
                    log::warn!(target: "plugin", "plugin {plugin_id} restart failed: {error}");
                    host.schedule_restart(plugin_id.clone(), declared.clone());
                }
            });
        tokio::spawn(task);
    }

    /// A process that survives 60s is healthy; the backoff starts over.
    fn watch_health(self: &Arc<Self>, plugin_id: &str, process: &Arc<PluginProcess>) {
        let weak = Arc::downgrade(self);
        let id = plugin_id.to_string();
        let watched = process.clone();
        tokio::spawn(async move {
            tokio::time::sleep(RESTART_HEALTHY_AFTER).await;
            let Some(host) = weak.upgrade() else { return };
            let current = host
                .processes
                .lock()
                .await
                .get(&id)
                .map(|entry| Arc::ptr_eq(entry, &watched))
                .unwrap_or(false);
            if !current {
                return;
            }
            if let Some(slot) = host.restarts.lock().unwrap().get_mut(&id) {
                slot.attempts = 0;
            }
            for status in host
                .services
                .lock()
                .unwrap()
                .iter_mut()
                .filter(|((owner, _), _)| owner == &id)
                .map(|(_, status)| status)
            {
                status.restarts = 0;
            }
        });
    }

    // ── lifecycle ──────────────────────────────────────────────────────────

    async fn plugins_list(&self) -> Result<Vec<Value>, HostError> {
        let reply = self
            .host
            .call("plugins.list", json!({}))
            .await
            .map_err(|error| HostError::new("HOST_UNAVAILABLE", error.to_string()))?;
        Ok(reply
            .get("plugins")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default())
    }

    /// Make the running plugin set match the enabled + project-scoped rows.
    pub async fn sync(self: &Arc<Self>, project_path: Option<&str>) {
        if let Err(error) = self.sync_inner(project_path).await {
            log::warn!(target: "plugin", "plugin sync failed: {error}");
        }
    }

    async fn sync_inner(self: &Arc<Self>, project_path: Option<&str>) -> Result<(), HostError> {
        let _guard = self.sync_lock.lock().await;
        *self.project_path.lock().unwrap() = project_path.map(str::to_string);
        let rows = self.plugins_list().await?;
        let mut desired: Vec<Value> = Vec::new();
        for row in rows {
            if !crate::plugins::active_in_project(&row, project_path) {
                continue;
            }
            desired.push(row);
        }
        let desired_ids: Vec<String> = desired
            .iter()
            .map(|row| crate::plugins::plugin_id(row).to_string())
            .filter(|id| !id.is_empty())
            .collect();

        let loaded: Vec<String> = self.processes.lock().await.keys().cloned().collect();
        for plugin_id in loaded {
            if !desired_ids.contains(&plugin_id) {
                self.unload(&plugin_id).await;
            }
        }
        for row in &desired {
            let id = crate::plugins::plugin_id(row);
            if id.is_empty() {
                continue;
            }
            let stale = {
                let processes = self.processes.lock().await;
                processes
                    .get(id)
                    .map(|process| {
                        crate::plugins::plugin_root(row)
                            .map(|root| process.exec_path != root)
                            .unwrap_or(true)
                    })
                    .unwrap_or(false)
            };
            if stale {
                self.unload(id).await;
            }
            let already = self.processes.lock().await.contains_key(id);
            if already || self.failed.lock().unwrap().contains(id) {
                continue;
            }
            if let Err(error) = self.load_row(row, project_path).await {
                log::warn!(target: "plugin", "plugin {id} failed to load: {error}");
                self.failed.lock().unwrap().insert(id.to_string());
                self.report_load_failure(&error);
            }
        }
        Ok(())
    }

    fn report_load_failure(&self, error: &HostError) {
        if error.code == "PLUGIN_HOST_UNAVAILABLE" {
            if !self.missing_node_logged.swap(true, Ordering::SeqCst) {
                self.emit(
                    "pi-desktop/app/event/toast",
                    json!({ "message": error.message }),
                );
            }
            return;
        }
        self.emit(
            "pi-desktop/app/event/toast",
            json!({ "message": format!("Plugin failed to load: {}", error.message) }),
        );
    }

    async fn load_row(
        self: &Arc<Self>,
        row: &Value,
        project_path: Option<&str>,
    ) -> Result<(), HostError> {
        let plugin_id = crate::plugins::plugin_id(row).to_string();
        let root = crate::plugins::plugin_root(row)
            .ok_or_else(|| HostError::new("PLUGIN_INVALID", "plugin path is missing"))?;
        let manifest = crate::plugins::read_manifest(row)
            .ok_or_else(|| HostError::new("PLUGIN_INVALID", "manifest.json is unreadable"))?;
        let entry_relative = manifest
            .get("main")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| HostError::new("PLUGIN_INVALID", "manifest.main is required"))?;
        let _entry = crate::plugins::safe_join(&root, entry_relative)
            .filter(|path| path.is_file())
            .ok_or_else(|| {
                HostError::new(
                    "PLUGIN_LOAD_FAILED",
                    format!("entry point {entry_relative} does not exist"),
                )
            })?;

        // Declared resident services, gated on the recorded grant. The child
        // only holds handlers; the manifest decides what may run.
        let declared = if crate::plugins::has_permission(row, "background.service") {
            declared_services(&manifest)
        } else {
            Vec::new()
        };

        let node = self.node_binary()?;
        let script = self.script_path()?;
        let settings = crate::plugins::stored_plugin_values(&self.data_dir, &plugin_id);
        let workspace = project_path.map(|path| {
            let name = Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(path);
            json!({ "path": path, "name": name })
        });

        let weak = Arc::downgrade(self);
        let handler: HostRequestFn = {
            let plugin_id = plugin_id.clone();
            let plugin = row.clone();
            let manifest = manifest.clone();
            let weak = weak.clone();
            Arc::new(move |method, params| {
                let weak = weak.clone();
                let plugin_id = plugin_id.clone();
                let plugin = plugin.clone();
                let manifest = manifest.clone();
                Box::pin(async move {
                    let Some(host) = weak.upgrade() else {
                        return Err(HostError::new(
                            "PLUGIN_HOST_UNAVAILABLE",
                            "desktop shell is shutting down",
                        ));
                    };
                    host.handle_host_request(&plugin_id, &plugin, &manifest, &method, params)
                        .await
                })
            })
        };

        let process =
            PluginProcess::spawn(&node, &script, plugin_id.clone(), root.clone(), handler)?;
        // Crash supervision: an unexplained exit drops the process row,
        // unregisters its contributions and tells the renderer. A deliberate
        // unload removes the map entry first, so this task then stands down.
        {
            let weak = Arc::downgrade(self);
            let watched = process.clone();
            let watched_id = plugin_id.clone();
            let watched_services = declared.clone();
            tokio::spawn(async move {
                let mut closed = watched.closed.subscribe();
                let _ = closed.wait_for(|value| *value).await;
                let Some(host) = weak.upgrade() else { return };
                let mut processes = host.processes.lock().await;
                let current = processes
                    .get(&watched_id)
                    .map(|entry| Arc::ptr_eq(entry, &watched))
                    .unwrap_or(false);
                if !current {
                    return;
                }
                processes.remove(&watched_id);
                drop(processes);
                host.registry.lock().unwrap().remove_plugin(&watched_id);
                if let Some(surfaces) = &host.surfaces {
                    surfaces.revoke_plugin(&watched_id);
                }
                host.clear_fs_state(&watched_id);
                host.mcp.forget_plugin(&watched_id).await;
                host.set_services(
                    &watched_id,
                    &watched_services,
                    "failed",
                    Some("plugin process exited".to_string()),
                );
                host.emit(
                    "pi-desktop/event/pluginChanged",
                    json!({ "reason": "crash", "pluginId": watched_id }),
                );
                // ADR 0040: a crashed service takes its process down, so the
                // supervisor restarts the plugin. `autoRestart: false` on any
                // service, or no declared service at all, opts out.
                let supervised = !watched_services.is_empty()
                    && watched_services.iter().all(|service| service.auto_restart);
                if supervised {
                    host.schedule_restart(watched_id.clone(), watched_services);
                } else {
                    // Do not relaunch a crash loop on every prompt; the plugin
                    // page's reload or an enable/disable clears the mark.
                    host.failed.lock().unwrap().insert(watched_id.clone());
                    host.emit(
                        "pi-desktop/app/event/toast",
                        json!({ "message": format!("Plugin {} stopped unexpectedly", watched_id) }),
                    );
                }
            });
        }
        let load_params = json!({
            "pluginId": plugin_id.clone(),
            "root": root.to_string_lossy(),
            "manifest": manifest,
            "settings": settings,
            "workspace": workspace,
            "app": { "version": APP_VERSION, "locale": crate::plugins::system_locale() },
            // `contributes.agentExtensions` runs only under the recorded grant.
            "extensionGranted": crate::plugins::has_permission(row, "agent.extension"),
        });
        let reply = match process.call("load", load_params, LOAD_TIMEOUT).await {
            Ok(reply) => reply,
            Err(error) => {
                process.terminate().await;
                return Err(error);
            }
        };

        let commands = parse_commands(&plugin_id, reply.get("commands"));
        let tools = if crate::plugins::has_permission(row, "agent.tool.register") {
            parse_tools(&plugin_id, reply.get("tools"))
        } else {
            Vec::new()
        };
        let extension = reply
            .get("extension")
            .cloned()
            .filter(|value| !value.is_null());
        if let Some(extension) = &extension {
            log::info!(
                target: "plugin",
                "plugin {plugin_id} extensions: state={} commands={} diagnostics={}",
                extension.get("state").and_then(Value::as_str).unwrap_or(""),
                extension
                    .get("commandNames")
                    .and_then(Value::as_array)
                    .map(|rows| rows.len())
                    .unwrap_or(0),
                extension
                    .get("diagnostics")
                    .and_then(Value::as_array)
                    .map(|rows| rows.len())
                    .unwrap_or(0),
            );
            for diagnostic in extension
                .get("diagnostics")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                log::warn!(
                    target: "plugin",
                    "plugin {plugin_id} extension diagnostic: {} {}",
                    diagnostic.get("kind").and_then(Value::as_str).unwrap_or(""),
                    diagnostic.get("message").and_then(Value::as_str).unwrap_or(""),
                );
            }
        }
        {
            let mut registry = self.registry.lock().unwrap();
            registry.remove_plugin(&plugin_id);
            for command in commands {
                let full_id = format!("plugin.{}.{}", plugin_id, command.command_id);
                registry.commands.insert(full_id, command);
            }
            for tool in tools {
                if let Some(existing) = registry.tools.get(&tool.full_name) {
                    log::warn!(
                        target: "plugin",
                        "plugin {plugin_id} tool {} collides with {}; keeping the loaded one",
                        tool.full_name,
                        existing.plugin_id
                    );
                    continue;
                }
                registry.tools.insert(tool.full_name.clone(), tool);
            }
            if let Some(extension) = &extension {
                register_extension(&mut registry, &plugin_id, extension);
            }
        }
        self.failed.lock().unwrap().remove(&plugin_id);
        self.processes
            .lock()
            .await
            .insert(plugin_id.clone(), process.clone());
        // A crash between the load reply and this insert would leave a dead
        // row; the watcher may have already stood down, so check once here.
        if process.exited.load(Ordering::SeqCst) {
            self.processes.lock().await.remove(&plugin_id);
            self.registry.lock().unwrap().remove_plugin(&plugin_id);
            self.failed.lock().unwrap().insert(plugin_id.clone());
            return Err(HostError::new(
                "PLUGIN_CRASHED",
                format!("plugin {plugin_id} exited during load"),
            ));
        }
        log::info!(target: "plugin", "loaded {plugin_id}");
        // Contribution points are registered and the process is in the table:
        // the declared services may start now.
        self.start_services(&plugin_id, &declared, &process).await;
        self.watch_health(&plugin_id, &process);
        Ok(())
    }

    pub async fn unload(&self, plugin_id: &str) {
        self.cancel_restart(plugin_id);
        self.unload_inner(plugin_id).await;
    }

    async fn unload_inner(&self, plugin_id: &str) {
        self.failed.lock().unwrap().remove(plugin_id);
        self.clear_services(plugin_id);
        // The MCP connections and discovered tools follow the plugin out.
        self.mcp.forget_plugin(plugin_id).await;
        self.clear_fs_state(plugin_id);
        if let Some(surfaces) = &self.surfaces {
            surfaces.revoke_plugin(plugin_id);
        }
        let process = self.processes.lock().await.remove(plugin_id);
        {
            let mut registry = self.registry.lock().unwrap();
            registry.remove_plugin(plugin_id);
        }
        if let Some(process) = process {
            process.terminate().await;
            self.emit(
                "pi-desktop/event/pluginChanged",
                json!({ "reason": "unload", "pluginId": plugin_id }),
            );
        }
    }

    pub async fn reload(
        self: &Arc<Self>,
        plugin_id: &str,
        project_path: Option<&str>,
    ) -> Result<(), HostError> {
        // An explicit reload clears the supervisor's attempt counter.
        self.cancel_restart(plugin_id);
        self.reload_inner(plugin_id, project_path).await
    }

    async fn reload_inner(
        self: &Arc<Self>,
        plugin_id: &str,
        project_path: Option<&str>,
    ) -> Result<(), HostError> {
        self.failed.lock().unwrap().remove(plugin_id);
        self.unload_inner(plugin_id).await;
        let rows = self.plugins_list().await?;
        let Some(row) = rows
            .iter()
            .find(|row| crate::plugins::plugin_id(row) == plugin_id)
        else {
            return Err(HostError::new(
                "PLUGIN_NOT_FOUND",
                format!("unknown plugin {plugin_id}"),
            ));
        };
        if !crate::plugins::active_in_project(row, project_path) {
            return Ok(());
        }
        self.load_row(row, project_path).await
    }

    /// A plugin row changed: forget failed loads so the new state is tried,
    /// and cancel pending restarts (a manual action outranks the supervisor).
    pub fn clear_failures(&self) {
        self.failed.lock().unwrap().clear();
        self.cancel_all_restarts();
    }

    pub async fn push_settings(&self, plugin_id: &str, values: &Map<String, Value>) {
        let process = self.processes.lock().await.get(plugin_id).cloned();
        if let Some(process) = process {
            let _ = process
                .call(
                    "updateSettings",
                    json!({ "values": values }),
                    Duration::from_secs(5),
                )
                .await;
        }
    }

    // ── host services requested by plugin code ─────────────────────────────

    async fn handle_host_request(
        &self,
        plugin_id: &str,
        plugin: &Value,
        manifest: &Value,
        method: &str,
        params: Value,
    ) -> Result<Value, HostError> {
        match method {
            "workspace.get" => {
                let reply = self
                    .host
                    .call("workspace.get", json!({}))
                    .await
                    .map_err(|error| HostError::new("HOST_UNAVAILABLE", error.to_string()))?;
                let workspace = reply.get("workspace").cloned().unwrap_or(Value::Null);
                if workspace.is_null() {
                    Ok(Value::Null)
                } else {
                    Ok(json!({
                        "path": workspace.get("path").cloned().unwrap_or(Value::Null),
                        "name": workspace.get("name").cloned().unwrap_or(Value::Null),
                    }))
                }
            }
            "settings.get" => Ok(json!({
                "values": crate::plugins::stored_plugin_values(&self.data_dir, plugin_id),
            })),
            "settings.set" => {
                let values = params
                    .get("values")
                    .and_then(|value| value.as_object())
                    .cloned()
                    .unwrap_or_default();
                let merged = crate::plugins::apply_plugin_settings(
                    &self.data_dir,
                    plugin_id,
                    manifest,
                    &values,
                )
                .map_err(|message| HostError::new("PLUGIN_INVALID", message))?;
                Ok(json!({ "values": merged }))
            }
            "plugin.dataPath" => {
                let path = self.data_dir.join("plugin-data").join(plugin_id);
                std::fs::create_dir_all(&path)
                    .map_err(|error| HostError::new("INTERNAL", error.to_string()))?;
                Ok(json!(path.to_string_lossy()))
            }
            "app.info" => Ok(json!({
                "version": APP_VERSION,
                "locale": crate::plugins::system_locale(),
            })),
            "app.appearance" => {
                let settings = self.settings_snapshot().await?;
                let theme = settings
                    .get("theme")
                    .and_then(|value| value.as_str())
                    .unwrap_or("system")
                    .to_string();
                let base = match theme.as_str() {
                    "light" => "light",
                    "dark" => "dark",
                    _ => "system",
                };
                let plugin_theme = self
                    .registry
                    .lock()
                    .unwrap()
                    .themes
                    .values()
                    .flat_map(|themes| themes.iter())
                    .find(|entry| {
                        entry.get("id").and_then(|value| value.as_str()) == Some(theme.as_str())
                    })
                    .cloned();
                Ok(json!({
                    "theme": theme,
                    "base": base,
                    "locale": crate::plugins::system_locale(),
                    "pluginTheme": plugin_theme,
                }))
            }
            "app.setTheme" => {
                let theme_id = params
                    .get("themeId")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let known = matches!(theme_id, "system" | "light" | "dark")
                    || self
                        .registry
                        .lock()
                        .unwrap()
                        .themes
                        .values()
                        .flat_map(|themes| themes.iter())
                        .any(|entry| {
                            entry.get("id").and_then(|value| value.as_str()) == Some(theme_id)
                        });
                if !known {
                    return Err(HostError::new(
                        "THEME_NOT_FOUND",
                        format!("theme {theme_id} is not registered"),
                    ));
                }
                let mut settings = self.settings_snapshot().await?;
                settings["theme"] = json!(theme_id);
                let updated = self
                    .host
                    .call("settings.set", Value::Object(settings))
                    .await
                    .map_err(|error| HostError::new("HOST_UNAVAILABLE", error.to_string()))?;
                let _ = updated;
                self.emit(
                    "pi-desktop/app/event/settingsChanged",
                    json!({ "theme": theme_id }),
                );
                Ok(json!({ "ok": true }))
            }
            "extension.ui" => {
                if !crate::plugins::has_permission(manifest, "agent.extension") {
                    return Err(HostError::new(
                        "PERMISSION_DENIED",
                        "the agent.extension permission is required",
                    ));
                }
                let session_id = params
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if session_id.is_empty() {
                    return Err(HostError::new("SESSION_REQUIRED", "sessionId is required"));
                }
                let extension_id = params
                    .get("extensionId")
                    .and_then(Value::as_str)
                    .unwrap_or(plugin_id);
                let extension_label = params
                    .get("extensionLabel")
                    .and_then(Value::as_str)
                    .unwrap_or(plugin_id);
                let request = params.get("request").cloned().unwrap_or(Value::Null);
                self.request_extension_ui(session_id, extension_id, extension_label, request)
                    .await
            }
            "extension.session.get" => {
                if !crate::plugins::has_permission(manifest, "agent.extension") {
                    return Err(HostError::new(
                        "PERMISSION_DENIED",
                        "the agent.extension permission is required",
                    ));
                }
                let session_id = params
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if session_id.is_empty() {
                    return Err(HostError::new("SESSION_REQUIRED", "sessionId is required"));
                }
                let reply = self
                    .host
                    .call(
                        "session.get",
                        json!({ "id": session_id, "messageLimit": 1 }),
                    )
                    .await
                    .map_err(|error| HostError::new("HOST_UNAVAILABLE", error.to_string()))?;
                Ok(json!({
                    "title": reply.pointer("/session/title").and_then(Value::as_str).unwrap_or(""),
                }))
            }
            "extension.session.rename" => {
                if !crate::plugins::has_permission(manifest, "agent.extension") {
                    return Err(HostError::new(
                        "PERMISSION_DENIED",
                        "the agent.extension permission is required",
                    ));
                }
                let session_id = params
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let title = params.get("title").and_then(Value::as_str).unwrap_or("");
                if session_id.is_empty() || title.trim().is_empty() {
                    return Err(HostError::new(
                        "INVALID_ARGUMENT",
                        "sessionId and title are required",
                    ));
                }
                self.host
                    .call(
                        "session.rename",
                        json!({ "id": session_id, "title": title }),
                    )
                    .await
                    .map_err(|error| HostError::new("HOST_UNAVAILABLE", error.to_string()))?;
                self.emit(
                    "pi-desktop/session/event/changed",
                    json!({ "reason": "extension", "sessionId": session_id }),
                );
                Ok(json!({ "ok": true }))
            }
            "ui.openPanel" => {
                if !crate::plugins::has_permission(manifest, "ui.panel") {
                    return Err(HostError::new(
                        "PLUGIN_PERMISSION_DENIED",
                        "panels require the ui.panel permission",
                    ));
                }
                let root = self
                    .processes
                    .lock()
                    .await
                    .get(plugin_id)
                    .map(|process| process.exec_path.clone())
                    .ok_or_else(|| {
                        HostError::new(
                            "PLUGIN_NOT_LOADED",
                            format!("plugin {plugin_id} is not loaded"),
                        )
                    })?;
                let fallback = manifest
                    .get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or(plugin_id);
                let meta = crate::plugins::panel_meta(manifest, fallback).ok_or_else(|| {
                    HostError::new(
                        "PLUGIN_PANEL_NOT_DECLARED",
                        "manifest.ui.panel is not declared",
                    )
                })?;
                let entry = crate::plugins::safe_join(&root, &meta.entry).ok_or_else(|| {
                    HostError::new("PLUGIN_INVALID", "manifest.ui.panel escapes the package")
                })?;
                let surfaces = self.surfaces.as_ref().ok_or_else(|| {
                    HostError::new(
                        "PLUGIN_SURFACE_UNAVAILABLE",
                        "the plugin surface service could not start",
                    )
                })?;
                let url = surfaces.register(plugin_id, &root, &entry).ok_or_else(|| {
                    HostError::new(
                        "PLUGIN_PANEL_NOT_DECLARED",
                        "manifest.ui.panel is not a readable file",
                    )
                })?;
                let title = params
                    .get("title")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|title| !title.is_empty())
                    .unwrap_or(meta.title.as_str())
                    .to_string();
                self.emit(
                    crate::plugin_surface::PANEL_EVENT,
                    json!({
                        "action": "open",
                        "pluginId": plugin_id,
                        "url": url,
                        "title": title,
                        "width": meta.width,
                        "height": meta.height,
                    }),
                );
                Ok(json!({ "ok": true }))
            }
            "ui.closePanel" => {
                self.emit(
                    crate::plugin_surface::PANEL_EVENT,
                    json!({ "action": "close", "pluginId": plugin_id }),
                );
                Ok(json!({ "ok": true }))
            }
            "themes.upsert" | "themes.remove" | "themes.list" => {
                if !crate::plugins::has_permission(manifest, "ui.theme") {
                    return Err(HostError::new(
                        "PLUGIN_PERMISSION_DENIED",
                        "themes require the ui.theme permission",
                    ));
                }
                self.handle_theme_request(plugin_id, method, params)
            }
            "toast" => {
                let message = params
                    .get("message")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                self.emit("pi-desktop/app/event/toast", json!({ "message": message }));
                Ok(json!({ "ok": true }))
            }
            "notify" => {
                if !crate::plugins::has_permission(manifest, "notify") {
                    return Err(HostError::new(
                        "PLUGIN_PERMISSION_DENIED",
                        "notifications require the notify permission",
                    ));
                }
                let title = params
                    .get("title")
                    .and_then(|value| value.as_str())
                    .unwrap_or("pigo-desktop");
                let body = params
                    .get("body")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                if let Some(app) = self.app.lock().unwrap().clone() {
                    crate::native::show_notification(&app, title, body);
                }
                Ok(json!({ "shown": true, "permission": "granted" }))
            }
            "log" => {
                let message = params
                    .get("message")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                log::info!(target: "plugin", "[{plugin_id}] {message}");
                Ok(json!({ "ok": true }))
            }
            // Plugin Node APIs reuse the panel bridge so permissions, fs
            // scopes, the net allowlist and session ownership are enforced by
            // one host boundary instead of a second copy in the JS shim.
            "bridge.invoke" => {
                let channel = params
                    .get("channel")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if channel.is_empty() {
                    return Err(HostError::new(
                        "INVALID_ARGUMENT",
                        "bridge.invoke requires a channel",
                    ));
                }
                let Some(app) = self.app_handle() else {
                    return Err(HostError::new(
                        "PLUGIN_HOST_UNAVAILABLE",
                        "the desktop shell is still starting",
                    ));
                };
                let state = app.state::<Arc<crate::state::AppState>>().inner().clone();
                let payload = params.get("payload").cloned().unwrap_or(Value::Null);
                let session_id = params
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let bridge_future = crate::plugin_bridge::invoke_loaded(
                    &app,
                    &state,
                    plugin_id,
                    plugin,
                    manifest,
                    &channel,
                    &payload,
                    session_id.as_deref(),
                );
                let reply = bridge_future.await;
                if reply.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                    Ok(reply.get("data").cloned().unwrap_or(Value::Null))
                } else {
                    let error = reply.get("error").cloned().unwrap_or(Value::Null);
                    Err(HostError::new(
                        error
                            .get("code")
                            .and_then(Value::as_str)
                            .unwrap_or("PLUGIN_BRIDGE_ERROR"),
                        error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("plugin bridge call failed"),
                    ))
                }
            }
            other => Err(HostError::new(
                "UNSUPPORTED",
                format!("plugin host service {other} is not available"),
            )),
        }
    }

    fn handle_theme_request(
        &self,
        plugin_id: &str,
        method: &str,
        params: Value,
    ) -> Result<Value, HostError> {
        match method {
            "themes.upsert" => {
                let theme = params.get("theme").cloned().unwrap_or(Value::Null);
                self.upsert_theme(plugin_id, &theme)
            }
            "themes.remove" => {
                let theme_id = params
                    .get("themeId")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                self.remove_theme(plugin_id, theme_id)
            }
            "themes.list" => Ok(json!(self.list_themes(plugin_id))),
            other => Err(HostError::new(
                "METHOD_NOT_FOUND",
                format!("unknown theme request {other}"),
            )),
        }
    }

    /// Runtime theme registration shared by the plugin process API and the
    /// panel bridge. CSS goes through the same sanitizer as manifest themes.
    pub(crate) fn upsert_theme(&self, plugin_id: &str, theme: &Value) -> Result<Value, HostError> {
        let theme_id = theme
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();
        if theme_id.is_empty()
            || theme_id.contains(':')
            || !theme_id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
        {
            return Err(HostError::new(
                "PLUGIN_INVALID",
                "theme.id may only contain letters, digits, dots, dashes and underscores",
            ));
        }
        let label = theme
            .get("label")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();
        if label.is_empty() {
            return Err(HostError::new("PLUGIN_INVALID", "theme.label is required"));
        }
        let css = theme
            .get("css")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let css = crate::plugins::sanitize_theme_css(css)
            .map_err(|message| HostError::new("PLUGIN_INVALID", message))?;
        let base = if theme.get("base").and_then(|value| value.as_str()) == Some("light") {
            "light"
        } else {
            "dark"
        };
        let full_id = format!("plugin:{plugin_id}:{theme_id}");
        let row = json!({
            "id": full_id,
            "pluginId": plugin_id,
            "themeId": theme_id,
            "label": label,
            "base": base,
            "css": css,
        });
        {
            let mut registry = self.registry.lock().unwrap();
            let themes = registry.themes.entry(plugin_id.to_string()).or_default();
            if let Some(existing) = themes.iter_mut().find(|entry| {
                entry.get("themeId").and_then(|value| value.as_str()) == Some(theme_id)
            }) {
                *existing = row;
            } else {
                themes.push(row);
            }
        }
        self.emit(
            "pi-desktop/event/pluginChanged",
            json!({ "reason": "themes", "pluginId": plugin_id }),
        );
        Ok(json!({
            "theme": {
                "id": full_id,
                "themeId": theme_id,
                "label": label,
                "base": base,
            }
        }))
    }

    pub(crate) fn remove_theme(&self, plugin_id: &str, theme_id: &str) -> Result<Value, HostError> {
        {
            let mut registry = self.registry.lock().unwrap();
            if let Some(themes) = registry.themes.get_mut(plugin_id) {
                themes.retain(|entry| {
                    entry.get("themeId").and_then(|value| value.as_str()) != Some(theme_id)
                });
            }
        }
        self.emit(
            "pi-desktop/event/pluginChanged",
            json!({ "reason": "themes", "pluginId": plugin_id }),
        );
        Ok(json!({ "ok": true }))
    }

    pub(crate) fn list_themes(&self, plugin_id: &str) -> Vec<Value> {
        let themes = self
            .registry
            .lock()
            .unwrap()
            .themes
            .get(plugin_id)
            .cloned()
            .unwrap_or_default();
        themes
            .into_iter()
            .map(|theme| {
                json!({
                    "id": theme.get("id").cloned().unwrap_or(Value::Null),
                    "themeId": theme.get("themeId").cloned().unwrap_or(Value::Null),
                    "label": theme.get("label").cloned().unwrap_or(Value::Null),
                    "base": theme.get("base").cloned().unwrap_or(Value::Null),
                })
            })
            .collect()
    }

    /// Full row for an active runtime theme, used by `app.getAppearance`.
    pub fn find_runtime_theme(&self, theme_id: &str) -> Option<Value> {
        self.registry
            .lock()
            .unwrap()
            .themes
            .values()
            .flat_map(|themes| themes.iter())
            .find(|entry| entry.get("id").and_then(|value| value.as_str()) == Some(theme_id))
            .cloned()
    }

    // ── fs bridge state ────────────────────────────────────────────────────

    pub(crate) fn set_fs_grant(&self, plugin_id: &str, path: PathBuf) {
        self.fs_grants
            .lock()
            .unwrap()
            .insert(plugin_id.to_string(), path);
    }

    pub(crate) fn fs_grant(&self, plugin_id: &str) -> Option<PathBuf> {
        self.fs_grants.lock().unwrap().get(plugin_id).cloned()
    }

    pub(crate) fn record_written(&self, plugin_id: &str, path: PathBuf) {
        self.written_paths
            .lock()
            .unwrap()
            .entry(plugin_id.to_string())
            .or_default()
            .insert(path);
    }

    pub(crate) fn is_written(&self, plugin_id: &str, path: &Path) -> bool {
        self.written_paths
            .lock()
            .unwrap()
            .get(plugin_id)
            .map(|paths| paths.contains(path))
            .unwrap_or(false)
    }

    fn clear_fs_state(&self, plugin_id: &str) {
        self.fs_grants.lock().unwrap().remove(plugin_id);
        self.written_paths.lock().unwrap().remove(plugin_id);
        self.complete_calls.lock().unwrap().remove(plugin_id);
    }

    /// Eight plugin completions per rolling minute, matching the plugin API
    /// contract. The window is per plugin and lives for the app run.
    pub(crate) fn allow_plugin_complete(&self, plugin_id: &str) -> bool {
        let now = Instant::now();
        let mut calls = self.complete_calls.lock().unwrap();
        let entries = calls.entry(plugin_id.to_string()).or_default();
        while let Some(at) = entries.front() {
            if now.duration_since(*at) > Duration::from_secs(60) {
                entries.pop_front();
            } else {
                break;
            }
        }
        if entries.len() >= 8 {
            return false;
        }
        entries.push_back(now);
        true
    }

    /// Record one clipboard text entry. Consecutive identical content moves to
    /// the front and refreshes its timestamp instead of duplicating.
    pub(crate) fn record_clipboard_text(&self, text: &str) {
        let text = text.trim_end_matches(['\r', '\n']);
        if text.is_empty() || text.len() > 100 * 1024 {
            return;
        }
        let captured_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let mut entries = self.clipboard_texts.lock().unwrap();
        entries.retain(|entry| entry.get("text").and_then(Value::as_str) != Some(text));
        entries.push_front(json!({
            "type": "text",
            "text": text,
            "capturedAt": captured_at,
        }));
        entries.truncate(500);
    }

    pub(crate) fn clipboard_history(&self) -> Value {
        Value::Array(
            self.clipboard_texts
                .lock()
                .unwrap()
                .iter()
                .cloned()
                .collect(),
        )
    }

    async fn settings_snapshot(&self) -> Result<Map<String, Value>, HostError> {
        let value = self
            .host
            .call("settings.get", json!({}))
            .await
            .map_err(|error| HostError::new("HOST_UNAVAILABLE", error.to_string()))?;
        value
            .as_object()
            .cloned()
            .ok_or_else(|| HostError::new("HOST_UNAVAILABLE", "settings reply is not an object"))
    }
}

impl Registry {
    fn remove_plugin(&mut self, plugin_id: &str) {
        self.commands.retain(|_, row| row.plugin_id != plugin_id);
        self.tools.retain(|_, row| row.plugin_id != plugin_id);
        self.themes.remove(plugin_id);
        self.extension_commands
            .retain(|_, row| row.plugin_id != plugin_id);
        self.extension_reports.remove(plugin_id);
        self.extension_entries.remove(plugin_id);
    }
}

/// Fold one plugin's extension report into the registry.
fn register_extension(registry: &mut Registry, plugin_id: &str, extension: &Value) {
    let mut report = json!({
        "state": extension.get("state").and_then(Value::as_str).unwrap_or("loaded"),
        "toolNames": extension.get("toolNames").cloned().unwrap_or_else(|| json!([])),
        "commandNames": json!([]),
        "diagnostics": extension.get("diagnostics").cloned().unwrap_or_else(|| json!([])),
    });
    let mut accepted: Vec<String> = Vec::new();
    for entry in extension
        .get("commands")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        let extension_id = entry
            .get("extensionId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let extension_label = entry
            .get("extensionLabel")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if let Some(existing) = registry.extension_commands.get(&name) {
            let owner = existing.plugin_id.clone();
            if let Some(rows) = report.get_mut("diagnostics").and_then(Value::as_array_mut) {
                rows.push(json!({
                    "extensionId": extension_id,
                    "kind": "rejected_registration",
                    "message": format!("command \"{name}\" is already registered by {owner}"),
                    "member": name,
                    "count": 1,
                }));
            }
            continue;
        }
        registry.extension_commands.insert(
            name.clone(),
            ExtensionCommandRow {
                plugin_id: plugin_id.to_string(),
                name: name.clone(),
                description: entry
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                extension_id,
                extension_label,
            },
        );
        accepted.push(name);
    }
    report["commandNames"] = json!(accepted);
    registry
        .extension_reports
        .insert(plugin_id.to_string(), report);
    let entries = extension
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    registry
        .extension_entries
        .insert(plugin_id.to_string(), entries);
}

fn safe_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn plugin_tool_name(plugin_id: &str, tool_name: &str) -> String {
    format!(
        "plugin_{}_{}",
        safe_segment(plugin_id),
        safe_segment(tool_name)
    )
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(|value| value.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn parse_commands(plugin_id: &str, value: Option<&Value>) -> Vec<CommandRow> {
    value
        .and_then(|value| value.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let command_id = entry.get("id").and_then(|value| value.as_str())?.trim();
                    let title = entry.get("title").and_then(|value| value.as_str())?.trim();
                    if command_id.is_empty() || title.is_empty() {
                        return None;
                    }
                    Some(CommandRow {
                        plugin_id: plugin_id.to_string(),
                        command_id: command_id.to_string(),
                        title: title.to_string(),
                        keywords: string_list(entry.get("keywords")),
                        category: entry
                            .get("category")
                            .and_then(|value| value.as_str())
                            .map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_tools(plugin_id: &str, value: Option<&Value>) -> Vec<ToolRow> {
    value
        .and_then(|value| value.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let name = entry.get("name").and_then(|value| value.as_str())?.trim();
                    if name.is_empty() {
                        return None;
                    }
                    let description = entry
                        .get("description")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string();
                    let risk = entry
                        .get("risk")
                        .and_then(|value| value.as_str())
                        .filter(|risk| matches!(*risk, "low" | "medium" | "high"))
                        .unwrap_or("medium")
                        .to_string();
                    let parameters = entry
                        .get("schema")
                        .filter(|value| value.is_object())
                        .cloned()
                        .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
                    Some(ToolRow {
                        plugin_id: plugin_id.to_string(),
                        tool_name: name.to_string(),
                        full_name: plugin_tool_name(plugin_id, name),
                        description,
                        risk,
                        plan_safe_actions: string_list(entry.get("planSafeActions")),
                        parameters,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

// ── shell entry points ─────────────────────────────────────────────────────

/// Reconcile the running plugin set with the host registry and the open
/// project. Called at boot and whenever the workspace or plugin rows change.
pub async fn sync(state: &Arc<crate::state::AppState>, project_path: Option<&str>) {
    state.plugins.sync(project_path).await;
}

pub async fn tool_definitions(
    state: &Arc<crate::state::AppState>,
    project_path: Option<&str>,
) -> Vec<Value> {
    state.plugins.sync(project_path).await;
    state.plugins.tool_definitions()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_available() -> bool {
        resolve_node_binary().is_some()
    }

    #[test]
    fn extension_commands_register_once_and_a_collision_becomes_a_diagnostic() {
        let mut registry = Registry::default();
        let first = json!({
            "state": "loaded",
            "toolNames": ["t1"],
            "commandNames": [],
            "diagnostics": [],
            "commands": [{
                "name": "greet",
                "description": "a",
                "extensionId": "e1",
                "extensionLabel": "One",
            }],
            "entries": [],
        });
        register_extension(&mut registry, "p1", &first);
        assert_eq!(registry.extension_commands["greet"].plugin_id, "p1");
        assert_eq!(
            registry.extension_reports["p1"]["commandNames"],
            json!(["greet"])
        );

        let second = json!({
            "state": "loaded",
            "toolNames": [],
            "commandNames": [],
            "diagnostics": [],
            "commands": [{
                "name": "greet",
                "description": "b",
                "extensionId": "e2",
                "extensionLabel": "Two",
            }],
            "entries": [],
        });
        register_extension(&mut registry, "p2", &second);
        assert_eq!(registry.extension_commands["greet"].plugin_id, "p1");
        let diagnostics = registry.extension_reports["p2"]["diagnostics"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(diagnostics
            .iter()
            .any(|row| row["kind"] == "rejected_registration"));

        registry.remove_plugin("p1");
        assert!(!registry.extension_commands.contains_key("greet"));
        assert!(!registry.extension_reports.contains_key("p1"));
    }

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pi-plugin-host-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_millis())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn noop_handler() -> HostRequestFn {
        Arc::new(|method: String, _params: Value| {
            Box::pin(async move {
                match method.as_str() {
                    "settings.get" | "settings.set" => Ok(json!({ "values": {} })),
                    "workspace.get" => Ok(Value::Null),
                    "app.info" => Ok(json!({ "version": "0", "locale": "en" })),
                    "app.appearance" => Ok(json!({
                        "theme": "system",
                        "base": "system",
                        "locale": "en",
                        "pluginTheme": null,
                    })),
                    "toast" | "notify" | "log" | "plugin.dataPath" => Ok(json!({ "ok": true })),
                    "themes.upsert" => {
                        let theme = _params.get("theme").cloned().unwrap_or(Value::Null);
                        Ok(json!({ "theme": theme }))
                    }
                    "themes.remove" | "themes.list" => Ok(json!({ "ok": true })),
                    other => Err(HostError::new(
                        "UNSUPPORTED",
                        format!("test host has no {other}"),
                    )),
                }
            })
        })
    }

    #[test]
    fn declared_services_default_to_auto_restart_and_stop_at_four() {
        let manifest = json!({
            "contributes": {
                "services": [
                    { "id": "one", "label": "One" },
                    { "id": "two", "autoRestart": false },
                    { "id": "three" },
                    { "id": "four" },
                    { "id": "five" }
                ]
            }
        });
        let declared = declared_services(&manifest);
        assert_eq!(declared.len(), 4);
        assert_eq!(declared[0].label, "One");
        assert!(declared[0].auto_restart);
        assert!(!declared[1].auto_restart);
        assert_eq!(declared[1].label, "two");
    }

    #[test]
    fn restart_backoff_doubles_and_stops_at_five_attempts() {
        let delays: Vec<u64> = (0..RESTART_MAX_ATTEMPTS)
            .map(|attempt| restart_delay(attempt).as_secs())
            .collect();
        assert_eq!(delays, vec![1, 2, 4, 8, 16]);
        assert_eq!(restart_delay(9).as_secs(), 16);
    }

    #[test]
    fn safe_names_mirror_the_sdk_prefix() {
        assert_eq!(
            plugin_tool_name("demo.hello", "echo-text"),
            "plugin_demo_hello_echo_text"
        );
        assert_eq!(safe_segment("a.b/c"), "a_b_c");
    }

    #[test]
    fn parsing_keeps_declared_fields_and_defaults_the_schema() {
        let commands = parse_commands(
            "demo.hello",
            Some(&json!([
                { "id": "hello.open", "title": "Open", "keywords": ["hello"], "category": "Demo" },
                { "id": "", "title": "broken" }
            ])),
        );
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command_id, "hello.open");

        let tools = parse_tools(
            "demo.hello",
            Some(&json!([
                { "name": "echo", "description": "Echo", "risk": "low" },
                { "name": "no-desc" }
            ])),
        );
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].full_name, "plugin_demo_hello_echo");
        assert_eq!(tools[1].parameters["type"], "object");
        assert_eq!(tools[1].risk, "medium");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_shipped_example_plugin_loads() {
        if !node_available() {
            return;
        }
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../examples/plugins/hello");
        if !root.join("manifest.json").is_file() {
            return;
        }
        let dir = temp_dir("example");
        let script = ensure_script(&dir).unwrap();
        let manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("manifest.json")).unwrap())
                .unwrap();
        let process = PluginProcess::spawn(
            &resolve_node_binary().unwrap(),
            &script,
            "demo.hello".to_string(),
            root.clone(),
            noop_handler(),
        )
        .unwrap();
        let reply = process
            .call(
                "load",
                json!({
                    "pluginId": "demo.hello",
                    "root": root.to_string_lossy(),
                    "manifest": manifest,
                    "settings": {},
                    "app": { "version": "0", "locale": "en" },
                }),
                LOAD_TIMEOUT,
            )
            .await
            .expect("example load");
        let commands = parse_commands("demo.hello", reply.get("commands"));
        let tools = parse_tools("demo.hello", reply.get("tools"));
        assert_eq!(commands.len(), 1);
        assert_eq!(tools.len(), 1);
        let result = process
            .call(
                "executeTool",
                json!({ "toolName": "echo_text", "args": { "text": "hi" } }),
                TOOL_TIMEOUT,
            )
            .await
            .expect("example tool");
        assert_eq!(result["content"]["echo"], "hi");
        process.terminate().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_real_node_process_loads_runs_and_executes() {
        if !node_available() {
            return;
        }
        let dir = temp_dir("node");
        let script = ensure_script(&dir).unwrap();
        std::fs::write(
            dir.join("manifest.json"),
            r#"{"id":"demo.test","main":"main.js","name":"Test","version":"1.0.0","schemaVersion":1}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("main.js"),
            r#"
const pi = globalThis.pi;
module.exports = {
  async onLoad() {
    await pi.commands.register({
      id: "test.run",
      title: "Test run",
      run: async () => { await pi.ui.showToast("ran"); },
    });
    await pi.agent.registerTool({
      name: "echo",
      description: "Echo",
      risk: "low",
      schema: { type: "object", properties: { text: { type: "string" } } },
      execute: async (args) => ({ echoed: String(args.text ?? "") }),
    });
  },
  onUnload() {},
};
"#,
        )
        .unwrap();

        let process = PluginProcess::spawn(
            &resolve_node_binary().unwrap(),
            &script,
            "demo.test".to_string(),
            dir.clone(),
            noop_handler(),
        )
        .unwrap();
        let manifest = json!({ "id": "demo.test", "main": "main.js", "name": "Test", "version": "1.0.0", "schemaVersion": 1 });
        let reply = process
            .call(
                "load",
                json!({
                    "pluginId": "demo.test",
                    "root": dir.to_string_lossy(),
                    "manifest": manifest,
                    "settings": {},
                    "app": { "version": "0", "locale": "en" },
                }),
                LOAD_TIMEOUT,
            )
            .await
            .expect("load");
        let commands = parse_commands("demo.test", reply.get("commands"));
        let tools = parse_tools("demo.test", reply.get("tools"));
        assert_eq!(commands.len(), 1);
        assert_eq!(tools.len(), 1);

        process
            .call(
                "runCommand",
                json!({ "commandId": "test.run" }),
                COMMAND_TIMEOUT,
            )
            .await
            .expect("command");
        let result = process
            .call(
                "executeTool",
                json!({ "toolName": "echo", "args": { "text": "hi" } }),
                TOOL_TIMEOUT,
            )
            .await
            .expect("tool");
        assert_eq!(result["content"]["echoed"], "hi");

        let missing = process
            .call(
                "executeTool",
                json!({ "toolName": "nope", "args": {} }),
                TOOL_TIMEOUT,
            )
            .await
            .expect_err("unknown tool");
        assert_eq!(missing.code, "TOOL_NOT_FOUND");

        process.terminate().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_extension_receives_loop_events_pushed_by_the_shell() {
        if !node_available() {
            return;
        }
        let dir = temp_dir("extension-event");
        let script = ensure_script(&dir).unwrap();
        std::fs::write(
            dir.join("main.js"),
            "module.exports = { onLoad() {}, onUnload() {} };\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("ext.js"),
            r#"
const fs = require("node:fs");
const path = require("node:path");
const log = path.join(__dirname, "ext.log");
module.exports = (pi) => {
  pi.on("session_start", () => fs.appendFileSync(log, "session_start\n"));
  pi.on("turn_start", (event) => {
    fs.appendFileSync(log, `turn_start:${event.sessionId}\n`);
  });
};
"#,
        )
        .unwrap();

        let process = PluginProcess::spawn(
            &resolve_node_binary().unwrap(),
            &script,
            "demo.extension".to_string(),
            dir.clone(),
            noop_handler(),
        )
        .unwrap();
        let manifest = json!({
            "id": "demo.extension",
            "main": "main.js",
            "name": "Extension",
            "version": "1.0.0",
            "schemaVersion": 1,
            "contributes": { "agentExtensions": ["ext.js"] },
        });
        let reply = process
            .call(
                "load",
                json!({
                    "pluginId": "demo.extension",
                    "root": dir.to_string_lossy(),
                    "manifest": manifest,
                    "settings": {},
                    "app": { "version": "0", "locale": "en" },
                    "extensionGranted": true,
                }),
                LOAD_TIMEOUT,
            )
            .await
            .expect("load");
        assert_eq!(reply.pointer("/extension/state"), Some(&json!("loaded")));

        process
            .notify(
                "extension.event",
                json!({
                    "event": "turn_start",
                    "sessionId": "session-1",
                    "payload": { "type": "turn_start" },
                }),
            )
            .await
            .expect("notify");

        let log = dir.join("ext.log");
        let mut recorded = String::new();
        for _ in 0..40 {
            recorded = std::fs::read_to_string(&log).unwrap_or_default();
            if recorded.contains("turn_start:session-1") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(recorded.contains("session_start"), "log: {recorded:?}");
        assert!(
            recorded.contains("turn_start:session-1"),
            "log: {recorded:?}"
        );
        process.terminate().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_declared_service_starts_after_load_and_stops_before_unload() {
        if !node_available() {
            return;
        }
        let dir = temp_dir("service");
        let script = ensure_script(&dir).unwrap();
        std::fs::write(
            dir.join("main.js"),
            r#"
const fs = require("node:fs");
const path = require("node:path");
const pi = globalThis.pi;
const log = path.join(__dirname, "service.log");
module.exports = {
  async onLoad() {
    pi.services.register({
      id: "watcher",
      start: () => { fs.appendFileSync(log, "start\n"); },
      stop: () => { fs.appendFileSync(log, "stop\n"); },
    });
  },
  onUnload() { fs.appendFileSync(log, "unload\n"); },
};
"#,
        )
        .unwrap();

        let process = PluginProcess::spawn(
            &resolve_node_binary().unwrap(),
            &script,
            "demo.service".to_string(),
            dir.clone(),
            noop_handler(),
        )
        .unwrap();
        let manifest = json!({
            "id": "demo.service",
            "main": "main.js",
            "name": "Service",
            "version": "1.0.0",
            "schemaVersion": 1,
            "contributes": { "services": [{ "id": "watcher" }] },
        });
        process
            .call(
                "load",
                json!({
                    "pluginId": "demo.service",
                    "root": dir.to_string_lossy(),
                    "manifest": manifest,
                    "settings": {},
                    "app": { "version": "0", "locale": "en" },
                }),
                LOAD_TIMEOUT,
            )
            .await
            .expect("load");

        let reply = process
            .call(
                "services.start",
                json!({ "services": [{ "id": "watcher" }, { "id": "ghost" }] }),
                SERVICE_START_TIMEOUT,
            )
            .await
            .expect("service start");
        let rows = reply["services"].as_array().cloned().unwrap_or_default();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["state"], "running");
        assert_eq!(rows[1]["state"], "failed");
        assert!(rows[1]["message"]
            .as_str()
            .unwrap_or("")
            .contains("not registered"));

        process.terminate().await;
        let recorded = std::fs::read_to_string(dir.join("service.log")).unwrap_or_default();
        let start_at = recorded.find("start").expect("started");
        let stop_at = recorded.find("stop").expect("stopped");
        let unload_at = recorded.find("unload").expect("unloaded");
        assert!(
            start_at < stop_at && stop_at < unload_at,
            "order: {recorded:?}"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
