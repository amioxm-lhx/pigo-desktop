//! The remote-control channel's desktop half.
//!
//! The channel itself is `pigo im serve`: a separate process that talks to
//! Feishu and drives this app's sessions through the loopback control endpoint
//! (`control_server.rs`). The shell owns only the boring parts the user should
//! not have to do by hand — where the settings live, which `pigo` binary to
//! run, starting and stopping it, and showing the last lines it printed when
//! something goes wrong.
//!
//! Deliberately no agent logic here: if a future channel needs new behaviour it
//! belongs in the Go side, and this module only has to pass a flag.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Settings file, next to the session database in the app's data dir.
const SETTINGS_FILE: &str = "im-channel.json";
/// Generated `config.toml`, handed to `pigo im serve --config`.
const CONFIG_FILE: &str = "im-channel.toml";
/// QR image the login child writes for the page to display.
const QR_FILE: &str = "wechat-qr.png";
/// How many child-process lines are kept for the log panel.
const LOG_LINES: usize = 240;

/// Everything the page can edit. The secret is stored here too — it has to live
/// somewhere on the machine, and this file sits in the app's own data directory
/// rather than a shared config dir.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChannelSettings {
    /// Which channel this page configures: "feishu" or "wechat".
    ///
    /// One page, two very different channels: Feishu is configured with an app id
    /// and secret typed in, WeChat with a token that only a QR scan produces. The
    /// per-channel fields below are kept side by side rather than nested so
    /// switching channels back and forth never loses the other one's setup.
    pub channel: String,
    /// Whether the channel comes back with the app. The Start button ignores
    /// it — an explicit click always wins — so a channel switched off here can
    /// still be started for one session.
    pub enabled: bool,
    pub app_id: String,
    /// Never leaves the shell: the renderer only learns whether it is set.
    pub app_secret: String,
    pub allowed_users: Vec<String>,
    pub allow_groups: bool,
    /// WeChat sender ids allowed to control the agent.
    pub wechat_allowed_users: Vec<String>,
    /// WeChat API root override. Empty uses the host remembered at login.
    pub wechat_base_url: String,
    /// "desktop" drives this app's sessions; "cli" runs its own store.
    pub session_host: String,
    /// confirm | allow | deny
    pub tool_policy: String,
    /// Working directory new sessions bind to. Empty means the user's home.
    pub cwd: String,
    /// Explicit path to the pigo binary. Empty means auto-detect.
    pub binary_path: String,
}

impl Default for ChannelSettings {
    fn default() -> Self {
        Self {
            channel: default_channel(),
            enabled: false,
            app_id: String::new(),
            app_secret: String::new(),
            allowed_users: Vec::new(),
            allow_groups: false,
            wechat_allowed_users: Vec::new(),
            wechat_base_url: String::new(),
            // The desktop app is the point of this page: a chat continues the
            // conversation the window is showing.
            session_host: "desktop".to_string(),
            tool_policy: "confirm".to_string(),
            cwd: String::new(),
            binary_path: String::new(),
        }
    }
}

/// Channel ids this page can configure. Kept in sync with `pigo im --channel`.
const CHANNEL_FEISHU: &str = "feishu";
const CHANNEL_WECHAT: &str = "wechat";

fn default_channel() -> String {
    CHANNEL_FEISHU.to_string()
}

fn normalize_channel(channel: &str) -> String {
    match channel.trim().to_ascii_lowercase().as_str() {
        CHANNEL_WECHAT => CHANNEL_WECHAT.to_string(),
        _ => CHANNEL_FEISHU.to_string(),
    }
}

/// Where the resolved binary came from, so the page can say why it picked it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BinarySource {
    Settings,
    App,
    Path,
}

impl BinarySource {
    fn label(self) -> &'static str {
        match self {
            BinarySource::Settings => "settings",
            BinarySource::App => "app",
            BinarySource::Path => "path",
        }
    }
}

/// A finished run the page can still explain.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitInfo {
    pub code: Option<i32>,
    pub at: String,
}

struct Running {
    child: Child,
    pid: u32,
    started_at: String,
}

/// A QR pairing in progress. It is a child process like the channel, but a
/// short-lived one, and its stdout carries the QR url the page renders.
struct LoginRun {
    child: Child,
    pid: u32,
    started_at: String,
    /// Kept so the page can answer a pairing that asks for the digits shown on
    /// the phone. Without this the only way to finish such a pairing is a
    /// terminal, which defeats the page.
    stdin: Option<std::process::ChildStdin>,
}

/// Managed state: at most one child, one shared log ring.
pub struct RemoteChannel {
    data_dir: PathBuf,
    running: Mutex<Option<Running>>,
    login: Mutex<Option<LoginRun>>,
    /// The QR url last seen on the login child's output, for the page to render.
    qr_url: Arc<Mutex<Option<String>>>,
    /// Whether the child is waiting for the digits shown on the phone.
    login_needs_code: Arc<Mutex<bool>>,
    logs: Arc<Mutex<VecDeque<String>>>,
    last_exit: Mutex<Option<ExitInfo>>,
}

impl RemoteChannel {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            running: Mutex::new(None),
            login: Mutex::new(None),
            qr_url: Arc::new(Mutex::new(None)),
            login_needs_code: Arc::new(Mutex::new(false)),
            logs: Arc::new(Mutex::new(VecDeque::new())),
            last_exit: Mutex::new(None),
        }
    }

    /// Where the login child writes its QR image. The Go side renders the PNG so
    /// the page never has to depend on block-art font metrics.
    fn qr_path(&self) -> PathBuf {
        self.data_dir.join(QR_FILE)
    }

    fn settings_path(&self) -> PathBuf {
        self.data_dir.join(SETTINGS_FILE)
    }

    fn config_path(&self) -> PathBuf {
        self.data_dir.join(CONFIG_FILE)
    }

    fn load_settings(&self) -> ChannelSettings {
        std::fs::read(self.settings_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn store_settings(&self, settings: &ChannelSettings) -> Result<(), String> {
        let body = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
        let path = self.settings_path();
        std::fs::write(&path, body).map_err(|e| format!("write {}: {e}", path.display()))?;
        restrict_permissions(&path);
        Ok(())
    }

    fn push_log(&self, line: String) {
        let mut logs = self.logs.lock().unwrap();
        logs.push_back(line);
        while logs.len() > LOG_LINES {
            logs.pop_front();
        }
    }

    /// Reap the child if it died on its own. Called from every read, so the
    /// page's poll is enough to keep state honest — no watcher thread that
    /// could outlive the window.
    fn reap(&self) {
        let exited = {
            let mut slot = self.running.lock().unwrap();
            let Some(run) = slot.as_mut() else { return };
            match run.child.try_wait() {
                Ok(Some(status)) => Some((status.code(), run.pid)),
                Ok(None) => None,
                // A wait error means the handle is gone; treat it as an exit
                // with an unknown code rather than reporting "running" forever.
                Err(_) => Some((None, run.pid)),
            }
        };
        if let Some((code, pid)) = exited {
            self.running.lock().unwrap().take();
            self.finish(code, pid, "exited");
        }
    }

    fn finish(&self, code: Option<i32>, pid: u32, why: &str) {
        let at = now_label();
        self.push_log(match code {
            Some(code) => format!("[{at}] {why}: pid {pid} exited with code {code}"),
            None => format!("[{at}] {why}: pid {pid} terminated"),
        });
        *self.last_exit.lock().unwrap() = Some(ExitInfo { code, at });
    }

    /// The renderer's view. The secret is summarised, never echoed.
    pub fn view(&self) -> Value {
        self.reap();
        self.reap_login();
        let settings = self.load_settings();
        let binary = resolve_binary(&settings);
        let (running, pid, started_at) = {
            let slot = self.running.lock().unwrap();
            match slot.as_ref() {
                Some(run) => (true, Some(run.pid), Some(run.started_at.clone())),
                None => (false, None, None),
            }
        };
        let (login_running, login_started) = {
            let slot = self.login.lock().unwrap();
            match slot.as_ref() {
                Some(run) => (true, Some(run.started_at.clone())),
                None => (false, None),
            }
        };
        let qr_url = self.qr_url.lock().unwrap().clone();
        let needs_code = *self.login_needs_code.lock().unwrap();
        let qr_png = if login_running {
            read_qr_data_url(&self.qr_path())
        } else {
            None
        };
        let logs: Vec<String> = self.logs.lock().unwrap().iter().cloned().collect();
        let exit = self.last_exit.lock().unwrap().clone();

        // The WeChat token is not a settings field: it is written by the QR
        // login, so "configured" means the state file says so.
        let wechat_state = read_wechat_state();
        let field = |name: &str| -> String {
            wechat_state
                .as_ref()
                .and_then(|v| v.get(name))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let paired = !field("token").is_empty();

        json!({
            "settings": {
                "channel": settings.channel,
                "enabled": settings.enabled,
                "appId": settings.app_id,
                "hasSecret": !settings.app_secret.is_empty(),
                "allowedUsers": settings.allowed_users,
                "allowGroups": settings.allow_groups,
                "wechatAllowedUsers": settings.wechat_allowed_users,
                "wechatBaseUrl": settings.wechat_base_url,
                "sessionHost": settings.session_host,
                "toolPolicy": settings.tool_policy,
                "cwd": settings.cwd,
                "binaryPath": settings.binary_path,
            },
            "running": running,
            "pid": pid,
            "startedAt": started_at,
            "exit": exit,
            "binary": binary.as_ref().map(|(path, _)| path.display().to_string()),
            "binarySource": binary.as_ref().map(|(_, source)| source.label()),
            "endpoint": endpoint_alive(&self.data_dir),
            "configPath": self.config_path().display().to_string(),
            "wechat": {
                "paired": paired,
                "account": field("user_id"),
                "botId": field("bot_id"),
                "statePath": wechat_state_path().display().to_string(),
            },
            "login": {
                "running": login_running,
                "startedAt": login_started,
                "qrUrl": qr_url,
                "qrPng": qr_png,
                "needsCode": needs_code,
            },
            "logs": logs,
        })
    }

    /// Merge the page's edits into the stored settings. An empty secret means
    /// "unchanged" — that is what the masked field sends back. Clearing is a
    /// separate operation so a blank box cannot silently disarm the channel.
    pub fn save(&self, patch: Value) -> Result<Value, String> {
        let mut settings = self.load_settings();
        let incoming: ChannelSettings = serde_json::from_value(patch)
            .map_err(|e| format!("INVALID_ARGUMENT: settings is not a channel config: {e}"))?;
        let preserved = settings.app_secret.clone();

        settings.enabled = incoming.enabled;
        settings.channel = normalize_channel(&incoming.channel);
        settings.app_id = incoming.app_id.trim().to_string();
        settings.allowed_users = normalize_users(&incoming.allowed_users);
        settings.allow_groups = incoming.allow_groups;
        settings.wechat_allowed_users = normalize_users(&incoming.wechat_allowed_users);
        settings.wechat_base_url = incoming.wechat_base_url.trim().to_string();
        settings.session_host = normalize_host(&incoming.session_host);
        settings.tool_policy = normalize_policy(&incoming.tool_policy);
        settings.cwd = incoming.cwd.trim().to_string();
        settings.binary_path = incoming.binary_path.trim().to_string();
        settings.app_secret = if incoming.app_secret.trim().is_empty() {
            preserved
        } else {
            incoming.app_secret.trim().to_string()
        };
        self.store_settings(&settings)?;
        Ok(self.view())
    }

    /// Forget the stored secret.
    pub fn clear_secret(&self) -> Result<Value, String> {
        let mut settings = self.load_settings();
        settings.app_secret.clear();
        self.store_settings(&settings)?;
        Ok(self.view())
    }

    /// Start the channel. Idempotent: calling it while running returns the
    /// current view instead of spawning a second bot on the same app.
    pub fn start(&self) -> Result<Value, String> {
        self.reap();
        if self.running.lock().unwrap().is_some() {
            return Ok(self.view());
        }
        let settings = self.load_settings();
        let Some((binary, source)) = resolve_binary(&settings) else {
            return Err(
                "PIGO_NOT_FOUND: 找不到 pigo 可执行文件（放到 pigo-desktop.exe 同目录、指定路径，或加入 PATH）"
                    .into(),
            );
        };

        match settings.channel.as_str() {
            CHANNEL_WECHAT => {
                // WeChat has nothing to type in: the token comes from the scan,
                // so "not paired yet" is the one thing that can be missing.
                if !wechat_paired() {
                    return Err(
                        "WECHAT_NOT_PAIRED: 微信还没绑定，先点「扫码绑定」用手机微信扫一下".into(),
                    );
                }
                if settings.wechat_allowed_users.is_empty() {
                    return Err("INVALID_ARGUMENT: 微信允许的用户为空，通道会拒绝所有消息".into());
                }
            }
            _ => {
                if settings.app_id.trim().is_empty() {
                    return Err("INVALID_ARGUMENT: 飞书 App ID 为空".into());
                }
                if settings.app_secret.trim().is_empty() {
                    return Err("INVALID_ARGUMENT: 飞书 App Secret 未保存".into());
                }
                if settings.allowed_users.is_empty() {
                    return Err("INVALID_ARGUMENT: 允许的用户为空，通道会拒绝所有消息".into());
                }
            }
        }
        if settings.session_host == "desktop" && !endpoint_alive(&self.data_dir) {
            return Err("ENDPOINT_UNAVAILABLE: 桌面端控制端点未就绪，通道没有会话可驱动".into());
        }

        std::fs::write(self.config_path(), render_config(&settings))
            .map_err(|e| format!("INTERNAL: write config: {e}"))?;
        restrict_permissions(&self.config_path());

        let mut command = Command::new(&binary);
        command
            .arg("im")
            .arg("serve")
            .arg("--config")
            .arg(self.config_path())
            .arg("--channel")
            .arg(&settings.channel)
            .arg("--session-host")
            .arg(&settings.session_host)
            .arg("--tool-policy")
            .arg(&settings.tool_policy)
            .current_dir(working_dir(&settings))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // A bot started from a settings page must not flash a console window
        // the user then closes, killing the channel with it.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }

        let mut child = command
            .spawn()
            .map_err(|e| format!("INTERNAL: spawn {}: {e}", binary.display()))?;
        let pid = child.id();
        let started_at = now_label();
        self.push_log(format!(
            "[{started_at}] start: {} (from {})",
            binary.display(),
            source.label()
        ));

        // One reader per pipe. Both feed the ring: the channel is chatty about
        // connection retries, and those lines are the only evidence the user
        // gets for "why is it not answering".
        if let Some(stdout) = child.stdout.take() {
            let sink = self.logs.clone();
            std::thread::Builder::new()
                .name("pigo-im-out".into())
                .spawn(move || pump(stdout, sink))
                .ok();
        }
        if let Some(stderr) = child.stderr.take() {
            let sink = self.logs.clone();
            std::thread::Builder::new()
                .name("pigo-im-err".into())
                .spawn(move || pump(stderr, sink))
                .ok();
        }

        *self.running.lock().unwrap() = Some(Running {
            child,
            pid,
            started_at,
        });
        *self.last_exit.lock().unwrap() = None;
        Ok(self.view())
    }

    /// Stop the channel. Returns the view either way: "nothing was running" is
    /// not an error worth shouting about.
    pub fn stop(&self) -> Result<Value, String> {
        let taken = self.running.lock().unwrap().take();
        if let Some(mut run) = taken {
            let _ = run.child.kill();
            let code = run.child.wait().ok().and_then(|status| status.code());
            self.finish(code, run.pid, "stopped");
        }
        Ok(self.view())
    }

    /// Start the QR pairing for WeChat.
    ///
    /// This is the one part of the channel that cannot be a settings field: the
    /// bot token only exists as the result of a scan, so the page has to run the
    /// same command a terminal would and show what it prints.
    pub fn login(&self) -> Result<Value, String> {
        self.reap_login();
        if self.login.lock().unwrap().is_some() {
            return Ok(self.view());
        }
        if self.running.lock().unwrap().is_some() {
            return Err(
                "CHANNEL_RUNNING: 通道正在运行。先停止通道再扫码——重新登录会让正在用的 token 失效".into(),
            );
        }
        let settings = self.load_settings();
        let Some((binary, _)) = resolve_binary(&settings) else {
            return Err(
                "PIGO_NOT_FOUND: 找不到 pigo 可执行文件（放到 pigo-desktop.exe 同目录、指定路径，或加入 PATH）"
                    .into(),
            );
        };

        *self.qr_url.lock().unwrap() = None;
        *self.login_needs_code.lock().unwrap() = false;
        let _ = std::fs::remove_file(self.qr_path());

        let mut command = Command::new(&binary);
        command
            .arg("im")
            .arg("login")
            .arg("--channel")
            .arg(CHANNEL_WECHAT)
            // The QR is rendered by the Go side as a PNG: a block-art QR drawn in
            // a webview depends on font metrics this app does not control.
            .arg("--qr-out")
            .arg(self.qr_path())
            .current_dir(working_dir(&settings))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }

        let mut child = command
            .spawn()
            .map_err(|e| format!("INTERNAL: spawn {}: {e}", binary.display()))?;
        let pid = child.id();
        let started_at = now_label();
        self.push_log(format!("[{started_at}] login: {}", binary.display()));

        if let Some(stdout) = child.stdout.take() {
            let sink = self.logs.clone();
            let qr = self.qr_url.clone();
            let needs = self.login_needs_code.clone();
            std::thread::Builder::new()
                .name("pigo-im-login".into())
                .spawn(move || pump_login(stdout, sink, qr, needs))
                .ok();
        }
        if let Some(stderr) = child.stderr.take() {
            let sink = self.logs.clone();
            std::thread::Builder::new()
                .name("pigo-im-login-err".into())
                .spawn(move || pump(stderr, sink))
                .ok();
        }
        let stdin = child.stdin.take();

        *self.login.lock().unwrap() = Some(LoginRun {
            child,
            pid,
            started_at,
            stdin,
        });
        Ok(self.view())
    }

    /// Answer a pairing that asked for the digits shown on the phone.
    pub fn login_code(&self, code: &str) -> Result<Value, String> {
        let trimmed = code.trim();
        if trimmed.is_empty() {
            return Err("INVALID_ARGUMENT: 验证码为空".into());
        }
        let mut slot = self.login.lock().unwrap();
        let Some(run) = slot.as_mut() else {
            return Err("LOGIN_NOT_RUNNING: 现在没有进行中的扫码".into());
        };
        let Some(stdin) = run.stdin.as_mut() else {
            return Err("LOGIN_NO_STDIN: 扫码进程没有可写的输入".into());
        };
        use std::io::Write;
        writeln!(stdin, "{trimmed}").map_err(|e| format!("INTERNAL: write code: {e}"))?;
        stdin.flush().map_err(|e| format!("INTERNAL: flush code: {e}"))?;
        drop(slot);
        // The prompt is answered; if the code was wrong the child prints it again
        // and the flag comes back on its own.
        *self.login_needs_code.lock().unwrap() = false;
        Ok(self.view())
    }

    /// Abandon a pairing in progress.
    pub fn cancel_login(&self) -> Result<Value, String> {
        let taken = self.login.lock().unwrap().take();
        if let Some(mut run) = taken {
            let _ = run.child.kill();
            let code = run.child.wait().ok().and_then(|status| status.code());
            self.push_log(format!(
                "[{}] login: pid {} cancelled (code {})",
                now_label(),
                run.pid,
                code.map(|c| c.to_string()).unwrap_or_else(|| "?".into())
            ));
        }
        *self.login_needs_code.lock().unwrap() = false;
        let _ = std::fs::remove_file(self.qr_path());
        Ok(self.view())
    }

    /// Reap the login child if it finished on its own.
    fn reap_login(&self) {
        let exited = {
            let mut slot = self.login.lock().unwrap();
            let Some(run) = slot.as_mut() else { return };
            match run.child.try_wait() {
                Ok(Some(status)) => Some((status.code(), run.pid)),
                Ok(None) => None,
                Err(_) => Some((None, run.pid)),
            }
        };
        if let Some((code, pid)) = exited {
            self.login.lock().unwrap().take();
            let at = now_label();
            match code {
                Some(0) => self.push_log(format!("[{at}] login: 绑定完成（pid {pid}）")),
                Some(code) => self.push_log(format!("[{at}] login: 未完成，退出码 {code}")),
                None => self.push_log(format!("[{at}] login: 进程结束（pid {pid}）")),
            }
            *self.login_needs_code.lock().unwrap() = false;
            // The code is dead once the process is gone; leaving it on disk only
            // invites someone to scan a QR that no longer means anything.
            let _ = std::fs::remove_file(self.qr_path());
        }
    }

    /// Kill the child on app exit: it needs this app for sessions, so an orphan
    /// would sit there failing every message.
    pub fn shutdown(&self) {
        if let Some(mut run) = self.running.lock().unwrap().take() {
            let _ = run.child.kill();
            let _ = run.child.wait();
        }
        if let Some(mut run) = self.login.lock().unwrap().take() {
            let _ = run.child.kill();
            let _ = run.child.wait();
        }
    }
}

/// Read one child pipe into the log ring, and pick out the QR url while doing it.
///
/// The login child prints a block-character QR for a terminal plus the URL it
/// encodes; the page renders its own QR from that URL, because a block-art QR
/// rendered in a webview is a gamble on font metrics.
fn pump_login<R: std::io::Read>(
    pipe: R,
    sink: Arc<Mutex<VecDeque<String>>>,
    qr: Arc<Mutex<Option<String>>>,
    needs_code: Arc<Mutex<bool>>,
) {
    let reader = BufReader::new(pipe);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if let Some(url) = extract_qr_url(&line) {
            *qr.lock().unwrap() = Some(url);
        }
        if line.contains("显示的数字") {
            *needs_code.lock().unwrap() = true;
        }
        let mut logs = sink.lock().unwrap();
        logs.push_back(format!("[{}] {}", now_label(), line));
        while logs.len() > LOG_LINES {
            logs.pop_front();
        }
    }
}

/// Read the QR image into a data URL. Small (a few KB), so returning it with
/// every poll is cheaper than inventing a second endpoint for it.
fn read_qr_data_url(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    use base64::Engine as _;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:image/png;base64,{encoded}"))
}

/// The pairing URL is the only line the page needs to act on.
fn extract_qr_url(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let candidate = &line[start..];
    let end = candidate
        .find(|c: char| c.is_whitespace())
        .unwrap_or(candidate.len());
    let url = &candidate[..end];
    if url.starts_with("https://liteapp.weixin.qq.com/") {
        Some(url.to_string())
    } else {
        None
    }
}

/// Canonical WeChat state path, mirroring the Go side: `$PIGO_HOME/im/wechat.json`
/// or `~/.pigo/im/wechat.json`.
fn wechat_state_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("PIGO_HOME") {
        return PathBuf::from(dir).join("im").join("wechat.json");
    }
    home_dir().join(".pigo").join("im").join("wechat.json")
}

fn read_wechat_state() -> Option<Value> {
    let bytes = std::fs::read(wechat_state_path()).ok()?;
    serde_json::from_slice::<Value>(&bytes).ok()
}

/// Has a QR pairing been completed on this machine?
fn wechat_paired() -> bool {
    read_wechat_state()
        .as_ref()
        .and_then(|v| v.get("token"))
        .and_then(Value::as_str)
        .map(|t| !t.trim().is_empty())
        .unwrap_or(false)
}

fn pump<R: std::io::Read>(pipe: R, sink: Arc<Mutex<VecDeque<String>>>) {
    let reader = BufReader::new(pipe);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let mut logs = sink.lock().unwrap();
        logs.push_back(format!("[{}] {}", now_label(), line));
        while logs.len() > LOG_LINES {
            logs.pop_front();
        }
    }
}

fn now_label() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

fn restrict_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

fn working_dir(settings: &ChannelSettings) -> PathBuf {
    let trimmed = settings.cwd.trim();
    if !trimmed.is_empty() {
        let path = PathBuf::from(trimmed);
        if path.is_dir() {
            return path;
        }
    }
    home_dir()
}

fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Candidate order: explicit setting, the app's own directory (a portable
/// install then works with no setup), then PATH.
fn resolve_binary(settings: &ChannelSettings) -> Option<(PathBuf, BinarySource)> {
    let explicit = settings.binary_path.trim();
    if !explicit.is_empty() {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Some((path, BinarySource::Settings));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(executable_name());
            if candidate.is_file() {
                return Some((candidate, BinarySource::App));
            }
        }
    }
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(executable_name());
        if candidate.is_file() {
            return Some((candidate, BinarySource::Path));
        }
    }
    None
}

#[cfg(windows)]
fn executable_name() -> &'static str {
    "pigo.exe"
}

#[cfg(not(windows))]
fn executable_name() -> &'static str {
    "pigo"
}

/// Render the TOML `pigo im serve` reads. Pure and tiny on purpose: a
/// hand-rolled writer is less code than teaching the shell about a TOML
/// serializer for one file with eleven keys.
fn render_config(settings: &ChannelSettings) -> String {
    let mut out = String::new();
    out.push_str("# Generated by pigo-desktop's remote channel page.\n");
    out.push_str("# Edits here are overwritten the next time the channel starts.\n\n");
    out.push_str("[im]\n");
    out.push_str("enabled = true\n");
    out.push_str(&format!(
        "tool_policy = {}\n",
        toml_string(&settings.tool_policy)
    ));
    out.push_str(&format!(
        "session_host = {}\n",
        toml_string(&settings.session_host)
    ));
    if !settings.cwd.trim().is_empty() {
        out.push_str(&format!("session_cwd = {}\n", toml_string(&settings.cwd)));
    }

    // Only the selected channel is written. Emitting both sections would leave a
    // disabled-or-half-configured table in the file that looks meaningful and is
    // not, and `pigo im serve --channel` would ignore it anyway.
    match settings.channel.as_str() {
        CHANNEL_WECHAT => {
            out.push_str("\n[im.wechat]\n");
            out.push_str("enabled = true\n");
            if !settings.wechat_base_url.trim().is_empty() {
                out.push_str(&format!(
                    "base_url = {}\n",
                    toml_string(&settings.wechat_base_url)
                ));
            }
            // No token here on purpose: it is written by the QR login into
            // $PIGO_HOME/im/wechat.json, which is the one place it lives.
            out.push_str(&format!(
                "allowed_users = [{}]\n",
                settings
                    .wechat_allowed_users
                    .iter()
                    .map(|user| toml_string(user))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        _ => {
            out.push_str("\n[im.feishu]\n");
            out.push_str("enabled = true\n");
            out.push_str(&format!("app_id = {}\n", toml_string(&settings.app_id)));
            out.push_str(&format!(
                "app_secret = {}\n",
                toml_string(&settings.app_secret)
            ));
            out.push_str(&format!(
                "allowed_users = [{}]\n",
                settings
                    .allowed_users
                    .iter()
                    .map(|user| toml_string(user))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            out.push_str(&format!(
                "allow_groups = {}\n",
                if settings.allow_groups { "true" } else { "false" }
            ));
        }
    }
    out
}

/// TOML basic string. Control characters are escaped rather than dropped, so a
/// pasted secret with a stray newline fails loudly at parse time instead of
/// silently losing its tail.
fn toml_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn normalize_users(users: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for user in users {
        let trimmed = user.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            out.push(trimmed.to_string());
        }
    }
    out
}

fn normalize_host(host: &str) -> String {
    match host.trim().to_ascii_lowercase().as_str() {
        "cli" => "cli".to_string(),
        _ => "desktop".to_string(),
    }
}

fn normalize_policy(policy: &str) -> String {
    match policy.trim().to_ascii_lowercase().as_str() {
        "allow" => "allow".to_string(),
        "deny" => "deny".to_string(),
        _ => "confirm".to_string(),
    }
}

/// Is the app's own control endpoint listening? Reads its descriptor and tries
/// a connection; a descriptor with a dead port means the app is on its way out.
fn endpoint_alive(data_dir: &Path) -> bool {
    let descriptor = crate::control_server::descriptor_path(data_dir);
    let Ok(bytes) = std::fs::read(&descriptor) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return false;
    };
    let Some(port) = value.get("port").and_then(Value::as_u64) else {
        return false;
    };
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port as u16));
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(300)).is_ok()
}

/// Bring back a channel the user left switched on.
///
/// Called once during app setup, after the control endpoint is published — the
/// channel needs that endpoint, so starting any earlier would make every
/// desktop-backed channel fail its first message. A failure here is logged and
/// dropped: a channel that cannot start must not keep the app from opening.
pub fn autostart(state: &crate::state::AppState) -> Result<(), String> {
    if !state.remote_channel.load_settings().enabled {
        return Ok(());
    }
    state.remote_channel.start().map(|_| ())
}

/// Handle one of the remote-channel channels. `None` means "not mine".
pub fn handle(channel: &str, state: &crate::state::AppState, params: &Value) -> Option<Value> {
    let result = match channel {
        "pi-desktop/im/channel/get" => Ok(state.remote_channel.view()),
        "pi-desktop/im/channel/save" => state
            .remote_channel
            .save(params.get("settings").cloned().unwrap_or(Value::Null)),
        "pi-desktop/im/channel/clearSecret" => state.remote_channel.clear_secret(),
        "pi-desktop/im/channel/start" => state.remote_channel.start(),
        "pi-desktop/im/channel/stop" => state.remote_channel.stop(),
        "pi-desktop/im/channel/login" => state.remote_channel.login(),
        "pi-desktop/im/channel/loginCode" => state.remote_channel.login_code(
            params
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        "pi-desktop/im/channel/loginCancel" => state.remote_channel.cancel_login(),
        _ => return None,
    };
    Some(match result {
        Ok(data) => crate::envelope::ok(data),
        Err(message) => {
            // Errors carry a machine code and a human tail separated by ": ";
            // the page keys its own copy off the code and shows the tail.
            let (code, text) = match message.split_once(": ") {
                Some((code, rest)) if is_code(code) => (code, rest.to_string()),
                _ => ("INTERNAL", message),
            };
            crate::envelope::err(code, text)
        }
    })
}

fn is_code(candidate: &str) -> bool {
    !candidate.is_empty()
        && candidate.len() <= 32
        && candidate
            .chars()
            .all(|c| c.is_ascii_uppercase() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ChannelSettings {
        ChannelSettings {
            enabled: true,
            app_id: "cli_abc".into(),
            app_secret: "sec\"ret".into(),
            allowed_users: vec!["ou_1".into(), " ou_2 ".into(), "ou_1".into()],
            ..ChannelSettings::default()
        }
    }

    #[test]
    fn config_quotes_and_normalises() {
        let mut settings = sample();
        settings.allowed_users = normalize_users(&settings.allowed_users);
        let toml = render_config(&settings);
        assert!(toml.contains("app_secret = \"sec\\\"ret\""), "{toml}");
        assert!(toml.contains("allowed_users = [\"ou_1\", \"ou_2\"]"), "{toml}");
        assert!(toml.contains("session_host = \"desktop\""), "{toml}");
        assert!(toml.contains("[im.feishu]"), "{toml}");
    }

    #[test]
    fn unknown_host_and_policy_fall_back_to_safe_defaults() {
        assert_eq!(normalize_host("DESKTOP"), "desktop");
        assert_eq!(normalize_host("nonsense"), "desktop");
        assert_eq!(normalize_host("cli"), "cli");
        assert_eq!(normalize_policy("ALLOW"), "allow");
        assert_eq!(normalize_policy("whatever"), "confirm");
    }

    #[test]
    fn endpoint_is_dead_without_descriptor() {
        let dir = std::env::temp_dir().join("pigo-remote-channel-test");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::remove_file(crate::control_server::descriptor_path(&dir));
        assert!(!endpoint_alive(&dir));
    }

    #[test]
    fn wechat_config_writes_only_its_own_table() {
        let mut settings = ChannelSettings::default();
        settings.channel = CHANNEL_WECHAT.into();
        settings.wechat_allowed_users = vec!["wxid_1".into()];
        settings.wechat_base_url = "https://ilinkai.weixin.qq.com".into();
        settings.app_id = "cli_should_not_leak".into();
        let toml = render_config(&settings);
        assert!(toml.contains("[im.wechat]"), "{toml}");
        assert!(toml.contains("allowed_users = [\"wxid_1\"]"), "{toml}");
        // The token lives in the state file the scan writes, never in the config.
        assert!(!toml.contains("token"), "{toml}");
        // A half-configured Feishu table would look meaningful and be ignored.
        assert!(!toml.contains("[im.feishu]"), "{toml}");
        assert!(!toml.contains("cli_should_not_leak"), "{toml}");
    }

    #[test]
    fn qr_url_only_comes_from_the_expected_host() {
        assert_eq!(extract_qr_url("若二维码不方便扫描，用手机微信打开这个链接："), None);
        assert_eq!(
            extract_qr_url("https://liteapp.weixin.qq.com/q/abc?qrcode=d1&bot_type=3"),
            Some("https://liteapp.weixin.qq.com/q/abc?qrcode=d1&bot_type=3".to_string())
        );
        assert_eq!(extract_qr_url("[im] wechat: 登录成功"), None);
        // A different host is not the pairing link, however much it looks like one.
        assert_eq!(extract_qr_url("https://example.com/q/abc"), None);
    }
}
