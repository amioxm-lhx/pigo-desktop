//! NDJSON JSON-RPC 2.0 client for the two backend child processes
//! (Rust host-core and the Go agent sidecar), ported from the Electron
//! `host-process.ts` / `agent-sidecar.ts` semantics: one JSON object per
//! line, 64 MiB line cap, 130 s default timeout, HOST_OVERLOADED retry
//! ladder, crash supervision with backoff.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot, Mutex as AsyncMutex, Notify};

use crate::envelope;

pub const DEFAULT_RPC_TIMEOUT: Duration = Duration::from_secs(130);
const MAX_LINE_BYTES: usize = 64 * 1024 * 1024;
const OVERLOADED_ATTEMPTS: [u64; 5] = [0, 50, 100, 200, 400];

#[derive(Debug, Clone)]
pub enum RpcError {
    /// Child dead / pipe broken / timeout.
    Transport(String),
    /// Remote returned a JSON-RPC error object.
    Remote { message: String, data: Value },
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RpcError::Transport(m) => write!(f, "{m}"),
            RpcError::Remote { message, .. } => write!(f, "{message}"),
        }
    }
}

/// Async handler for reverse requests (the sidecar calling `host.proxy`).
pub type RequestHandler = Arc<
    dyn Fn(
            String,
            Value,
        )
            -> std::pin::Pin<Box<dyn Future<Output = Result<Value, (i64, String, Value)>> + Send>>
        + Send
        + Sync,
>;

/// Post-respawn hook (host handshake + recovery replay).
pub type OnRestart = Arc<
    dyn Fn(Arc<RpcClient>, tauri::AppHandle) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

struct Conn {
    stdin_tx: mpsc::Sender<String>,
}

pub struct RpcClient {
    pub label: &'static str,
    pub binary: std::path::PathBuf,
    env_extra: Vec<(String, String)>,
    env_remove: Vec<String>,
    /// Host-core answers HOST_OVERLOADED with a retry ladder in the Electron
    /// main; keep that behaviour here (sidecar never retries).
    overloaded_retry: bool,
    core: Mutex<Option<Conn>>,
    spawn_lock: AsyncMutex<()>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, RpcError>>>>>,
    next_id: AtomicU64,
    pub notify_tx: mpsc::Sender<(String, Value)>,
    request_handler: Mutex<Option<RequestHandler>>,
    pub available: AtomicBool,
    autorestart: AtomicBool,
    exit_notify: Arc<Notify>,
    restart_times: Mutex<VecDeque<std::time::Instant>>,
}

impl RpcClient {
    pub fn new(
        label: &'static str,
        binary: std::path::PathBuf,
        env_extra: Vec<(String, String)>,
        env_remove: Vec<String>,
        overloaded_retry: bool,
        notify_tx: mpsc::Sender<(String, Value)>,
    ) -> Arc<Self> {
        Arc::new(Self {
            label,
            binary,
            env_extra,
            env_remove,
            overloaded_retry,
            core: Mutex::new(None),
            spawn_lock: AsyncMutex::new(()),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(1),
            notify_tx,
            request_handler: Mutex::new(None),
            available: AtomicBool::new(false),
            autorestart: AtomicBool::new(true),
            exit_notify: Arc::new(Notify::new()),
            restart_times: Mutex::new(VecDeque::new()),
        })
    }

    pub fn set_request_handler(&self, handler: RequestHandler) {
        *self.request_handler.lock().unwrap() = Some(handler);
    }

    pub fn is_available(&self) -> bool {
        self.available.load(Ordering::SeqCst)
    }

    pub fn stop_restart(&self) {
        self.autorestart.store(false, Ordering::SeqCst);
    }

    fn autorestart_enabled(&self) -> bool {
        self.autorestart.load(Ordering::SeqCst)
    }

    pub async fn spawn(&self) -> Result<(), String> {
        let _guard = self.spawn_lock.lock().await;
        if self.is_available() {
            return Ok(());
        }
        let mut cmd = Command::new(&self.binary);
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        // GUI subsystem has no console to inherit; without CREATE_NO_WINDOW
        // Windows allocates a visible console per backend (the two black
        // boxes). Electron's port used `windowsHide: true` for the same
        // reason.
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        for key in &self.env_remove {
            cmd.env_remove(key);
        }
        for (key, value) in &self.env_extra {
            cmd.env(key, value);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("spawn {}: {e}", self.binary.display()))?;
        let stdin = child.stdin.take().ok_or("child stdin missing")?;
        let stdout = child.stdout.take().ok_or("child stdout missing")?;
        let stderr = child.stderr.take().ok_or("child stderr missing")?;

        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(512);
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

        // stderr → log sink (child diagnostics only; never parsed).
        {
            let label = self.label;
            let mut stderr = BufReader::new(stderr);
            tokio::spawn(async move {
                let mut line = Vec::new();
                loop {
                    line.clear();
                    match stderr.read_until(b'\n', &mut line).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            let text = String::from_utf8_lossy(&line);
                            log::warn!(target: "rpc", "[{label}-stderr] {}", text.trim_end());
                        }
                    }
                }
            });
        }

        let pending = self.pending.clone();
        let notify_tx = self.notify_tx.clone();
        let request_handler: Mutex<Option<RequestHandler>> =
            Mutex::new(self.request_handler.lock().unwrap().clone());
        let stdin_for_requests = stdin_tx.clone();
        let label = self.label;
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = Vec::new();
            loop {
                line.clear();
                match reader.read_until(b'\n', &mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if line.len() > MAX_LINE_BYTES {
                            log::error!(target: "rpc", "[{label}] line exceeds 64 MiB cap, dropped");
                            continue;
                        }
                        let Ok(msg) = serde_json::from_slice::<Value>(&line) else {
                            continue;
                        };
                        let method = msg.get("method").and_then(|v| v.as_str()).map(String::from);
                        let has_id = msg.get("id").map(|v| !v.is_null()).unwrap_or(false);
                        match (method, has_id) {
                            (Some(method), true) => {
                                // Reverse request: answer over the same stdin.
                                let id = msg.get("id").cloned().unwrap_or(json!(null));
                                let params = msg.get("params").cloned().unwrap_or(Value::Null);
                                let handler = request_handler.lock().unwrap().clone();
                                let stdin_tx = stdin_for_requests.clone();
                                tokio::spawn(async move {
                                    let outcome = match handler {
                                        Some(handler) => handler(method, params).await,
                                        None => {
                                            Err((-32601, format!("method not found"), Value::Null))
                                        }
                                    };
                                    let reply = match outcome {
                                        Ok(result) => {
                                            json!({ "jsonrpc": "2.0", "id": id, "result": result })
                                        }
                                        Err((code, message, data)) => json!({
                                            "jsonrpc": "2.0", "id": id,
                                            "error": { "code": code, "message": message, "data": data }
                                        }),
                                    };
                                    let _ = stdin_tx.send(reply.to_string()).await;
                                });
                            }
                            (Some(method), false) => {
                                let params = msg.get("params").cloned().unwrap_or(Value::Null);
                                let _ = notify_tx.send((method, params)).await;
                            }
                            (None, true) => {
                                let id = msg.get("id").and_then(|v| v.as_u64());
                                if let Some(id) = id {
                                    let sender = pending.lock().unwrap().remove(&id);
                                    if let Some(sender) = sender {
                                        let outcome = if let Some(error) = msg.get("error") {
                                            let message = error
                                                .get("message")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("remote error")
                                                .to_string();
                                            let data =
                                                error.get("data").cloned().unwrap_or(Value::Null);
                                            Err(RpcError::Remote { message, data })
                                        } else {
                                            Ok(msg.get("result").cloned().unwrap_or(Value::Null))
                                        };
                                        let _ = sender.send(outcome);
                                    }
                                }
                            }
                            (None, false) => {}
                        }
                    }
                }
            }
        });

        let exit_notify = self.exit_notify.clone();
        let pending = self.pending.clone();
        let label = self.label;
        tokio::spawn(async move {
            let mut child = child;
            let _ = child.wait().await;
            log::warn!(target: "rpc", "[{label}] process exited");
            // Fail every in-flight call so callers see HOST_UNAVAILABLE instead
            // of hanging to the timeout.
            let mut pend = pending.lock().unwrap();
            for (_, sender) in pend.drain() {
                let _ = sender.send(Err(RpcError::Transport(format!("{label} exited"))));
            }
            drop(pend);
            exit_notify.notify_waiters();
        });

        *self.core.lock().unwrap() = Some(Conn { stdin_tx });
        self.available.store(true, Ordering::SeqCst);
        log::info!(target: "rpc", "[{}] spawned {}", self.label, self.binary.display());
        Ok(())
    }

    async fn send_line(&self, line: String) -> Result<(), RpcError> {
        // Clone the sender out of the guard: std::sync::MutexGuard is not
        // Send and must never be held across the await.
        let tx = self
            .core
            .lock()
            .unwrap()
            .as_ref()
            .map(|c| c.stdin_tx.clone());
        match tx {
            Some(tx) => tx
                .send(line)
                .await
                .map_err(|_| RpcError::Transport(format!("{} stdin closed", self.label))),
            None => Err(RpcError::Transport(format!("{} not running", self.label))),
        }
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.call_timeout(method, params, DEFAULT_RPC_TIMEOUT).await
    }

    pub async fn call_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, RpcError> {
        for (i, delay_ms) in OVERLOADED_ATTEMPTS.iter().enumerate() {
            if *delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(*delay_ms)).await;
            }
            match self.call_once(method, &params, timeout).await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    let retriable = matches!(&e, RpcError::Remote { data, .. }
                        if data.get("errorCode").and_then(|v| v.as_str()) == Some("HOST_OVERLOADED"));
                    if !(retriable && self.overloaded_retry && i + 1 < OVERLOADED_ATTEMPTS.len()) {
                        return Err(e);
                    }
                }
            }
        }
        unreachable!("retry loop always returns")
    }

    async fn call_once(
        &self,
        method: &str,
        params: &Value,
        timeout: Duration,
    ) -> Result<Value, RpcError> {
        if !self.is_available() {
            self.spawn().await.map_err(RpcError::Transport)?;
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if let Err(e) = self.send_line(request.to_string()).await {
            self.pending.lock().unwrap().remove(&id);
            return Err(e);
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(_)) => Err(RpcError::Transport(format!("{method}: call dropped"))),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(RpcError::Transport(format!("{method}: timed out")))
            }
        }
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<(), RpcError> {
        let request = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.send_line(request.to_string()).await
    }

    /// Restart supervision: exponential backoff, giving up after more than 3
    /// crashes inside a 120 s window (mirrors Electron `superviseRestart`).
    pub fn supervise(self: &Arc<Self>, app: tauri::AppHandle, on_restart: Option<OnRestart>) {
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                if this.is_available() {
                    this.exit_notify.notified().await;
                } else {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                if !this.autorestart_enabled() {
                    break;
                }
                // Crash-rate window.
                {
                    let mut times = this.restart_times.lock().unwrap();
                    let now = std::time::Instant::now();
                    while let Some(front) = times.front() {
                        if now.duration_since(*front) > Duration::from_secs(120) {
                            times.pop_front();
                        } else {
                            break;
                        }
                    }
                    times.push_back(now);
                    if times.len() > 3 {
                        log::error!(target: "rpc", "[{}] crash loop, giving up", this.label);
                        emit_host_status(
                            &app,
                            json!({ "ok": false, "component": this.label, "fatal": true,
                                   "message": format!("{} keeps crashing; restart abandoned", this.label) }),
                        );
                        this.stop_restart();
                        break;
                    }
                }
                let attempt = this.restart_times.lock().unwrap().len().max(1);
                let delay = 500u64
                    .saturating_mul(1u64 << (attempt - 1).min(3))
                    .min(4000);
                emit_host_status(
                    &app,
                    json!({ "ok": false, "component": this.label, "restarting": true }),
                );
                tokio::time::sleep(Duration::from_millis(delay)).await;
                if !this.autorestart_enabled() {
                    break;
                }
                match this.spawn().await {
                    Ok(()) => {
                        if let Some(cb) = &on_restart {
                            cb(this.clone(), app.clone()).await;
                        }
                        emit_host_status(
                            &app,
                            json!({ "ok": true, "component": this.label, "restarted": true }),
                        );
                    }
                    Err(e) => {
                        log::error!(target: "rpc", "[{}] respawn failed: {e}", this.label);
                    }
                }
            }
        });
    }
}

pub fn emit_host_status(app: &tauri::AppHandle, payload: Value) {
    let _ = app.emit("pi-desktop/app/event/hostStatus", payload);
}

/// Shared helper for handlers that must map a backend result into an envelope.
pub fn envelope_result(label: &str, result: Result<Value, RpcError>) -> Value {
    match result {
        Ok(data) => envelope::ok(data),
        Err(e) => envelope::envelope_from_rpc(label, &e),
    }
}
