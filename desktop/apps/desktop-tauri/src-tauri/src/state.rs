//! Shared application state handed to every invoke handler.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;

use crate::catalog::Catalog;
use crate::oauth::OAuthManager;
use crate::rpc::RpcClient;

/// Metadata captured at tool_start so tool_end can persist a durable row.
#[derive(Clone)]
pub struct ActiveTool {
    pub name: String,
    pub args: Value,
    pub started_at: u64,
}

pub const APP_NAME: &str = "pigo-desktop";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROTOCOL_VERSION: u32 = 11;

pub struct AppState {
    pub data_dir: PathBuf,
    pub host: Arc<RpcClient>,
    pub sidecar: Arc<RpcClient>,
    /// Session whose notifications the user is currently reading (mirrors the
    /// Electron main's `notificationViewingSessionId`).
    pub viewing_session: std::sync::Mutex<Option<String>>,
    /// Close behavior: "ask" | "tray" | "quit" — Electron default is "ask".
    pub close_behavior: std::sync::Mutex<String>,
    /// sessionId → durable turnId for turns handed to the sidecar; drives
    /// abort bookkeeping and session.endTurn on teardown.
    pub active_turns: std::sync::Mutex<std::collections::HashMap<String, String>>,
    /// turnId → host plan execution id for approved-plan turns.
    pub active_executions: std::sync::Mutex<std::collections::HashMap<String, String>>,
    /// (sessionId, toolCallId) → metadata captured at tool_start, consumed at
    /// tool_end to persist the durable tool row.
    pub active_tools: std::sync::Mutex<std::collections::HashMap<(String, String), ActiveTool>>,
    /// Composer picker tokens → picked absolute paths (two-step pick/import
    /// handshake, mirroring Electron's composer-picker.ts).
    pub composer_tokens: std::sync::Mutex<std::collections::HashMap<String, Vec<String>>>,
    /// models.dev metadata (ADR 0134).
    pub catalog: Arc<Catalog>,
    /// In-flight vendor account logins and the vendor→login table that keeps a
    /// superseded attempt from fighting the new one for its callback port.
    pub oauth: Arc<OAuthManager>,
    /// Node-backed plugin runtime: child processes, command/tool registry and
    /// runtime themes. Metadata-only when Node is absent.
    pub plugins: Arc<crate::plugin_host::PluginHost>,
    /// User-owned MCP servers: live clients, discovered tools and the
    /// `mcp_<server>_<tool>` name map host-core dispatches against.
    pub mcp: Arc<crate::mcp::McpRuntime>,
    /// Host-owned browser guest (ADR 0170): one child webview shared by every
    /// chrome, positioned through `browser/setBounds`.
    pub browser: Arc<crate::browser::BrowserHost>,
    /// Cursor and page state for the desktop MCP market browse.
    pub market: Arc<crate::market::MarketRuntime>,
    /// Loopback file service for the embedded BrowserPreview iframe. `None`
    /// when binding failed; BrowserPreview then falls back to the system
    /// browser.
    pub preview: Option<Arc<crate::preview::PreviewServer>>,
    /// Loopback service for plugin view/panel iframes. `None` when binding
    /// failed; plugin surfaces then report `PLUGIN_SURFACE_UNAVAILABLE`.
    pub surfaces: Option<Arc<crate::plugin_surface::PluginSurfaceServer>>,
    /// sessionId → OAuth provider row ids allowed to ask for a fresh token:
    /// the session's own vendor row plus any OAuth row a subagent definition
    /// pins. Rewritten on every launch; `provider.resolveAuth` validates
    /// against it.
    pub provider_bindings: std::sync::Mutex<std::collections::HashMap<String, Vec<String>>>,
    /// Import scan caches: `source:externalId` → the row a later run may
    /// convert. Session rows carry a file path; model rows carry the secret
    /// that must never reach the renderer.
    pub session_import_cache: std::sync::Mutex<std::collections::HashMap<String, Value>>,
    pub model_import_cache: std::sync::Mutex<std::collections::HashMap<String, Value>>,
    /// Resident staff driver (spec 03-runtime/20 §12): the timer, the in-flight
    /// ledger, and the per-turn watchers the scheduler loop runs against
    /// host-core's claims.
    pub staff: Arc<crate::staff_driver::Resident>,
    /// Resident reply watch (spec 03-runtime/20 §15): the return leg that reads
    /// published pieces back on a timer and on the owner's request.
    pub watch: Arc<crate::staff_watch::Watch>,
    /// 选股神器: the hithink-finance engine, its caches and the watchlist.
    pub stocks: Arc<crate::stocks::StocksEngine>,
    /// Remote-control channel (Feishu): its settings and the `pigo im serve`
    /// child it supervises. One child at most — two bots on the same app would
    /// answer every message twice.
    pub remote_channel: Arc<crate::remote_channel::RemoteChannel>,
}

impl AppState {
    pub fn scratch_dir(&self) -> PathBuf {
        self.data_dir.join("scratch")
    }

    pub fn attachments_dir(&self) -> PathBuf {
        self.data_dir.join("attachments")
    }
}
