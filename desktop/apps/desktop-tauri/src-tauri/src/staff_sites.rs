//! Target-site sessions and collection (spec 03-runtime/20 §15).
//!
//! A site's content sits behind its own login, so the only honest client is the
//! site's own front end: the sign-in window is the site's page, the session is
//! the cookie that page sets, and the collector reads the DOM the site rendered
//! inside a window the owner never sees. Nothing here forges a request, replays
//! a signature or stores a credential.
//!
//! Platform note: on macOS WKWebView shares one cookie store across every
//! webview in the process, so the per-site `persist:pi-staff-site-<channel>`
//! partition the spec describes cannot be expressed there. The login cookie is
//! still the only source of truth; sign-out deletes the cookies that belong to
//! the site instead of clearing the store, because clearing the store would
//! also drop the renderer's own preferences.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::staff_site_scripts::{detail_script, list_script};
use crate::state::AppState;

/// The closed channel set a probe may name, in console order.
pub const CHANNELS: [&str; 5] = ["juejin", "zhihu", "bilibili", "wechatMp", "wechatChannels"];

pub const DEFAULT_MAX_URLS: u32 = 1;
pub const MAX_MAX_URLS: u32 = 20;
pub const DEFAULT_DETAIL_PAGES: usize = 3;
pub const MAX_DETAIL_PAGES: usize = 5;
pub const MAX_KEYWORDS: usize = 20;
/// One page's share of a probe step's material.
pub const PAGE_CHARS: usize = 6_000;
/// Every page a sweep read shares this ceiling in one step's prompt.
pub const TOTAL_CHARS: usize = 24_000;

const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const SITE_TIMEOUT: Duration = Duration::from_secs(45);
const SIGN_IN_POLL: Duration = Duration::from_millis(1_200);

/// An error the router can turn into an envelope without guessing a code.
#[derive(Debug)]
pub struct SiteError {
    pub code: &'static str,
    pub message: String,
}

impl SiteError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self::new("INVALID_ARGUMENT", message)
    }
}

/// What the collector knows about one channel.
pub struct SiteSpec {
    pub channel: &'static str,
    /// False when no collector exists yet; the console lists it as a gap.
    pub supported: bool,
    pub sign_in_url: &'static str,
    /// `{query}` is replaced with the percent-encoded keyword.
    pub search_url: &'static str,
    pub default_keywords: &'static [&'static str],
    /// The site's own login cookie; its presence is the session.
    pub auth_cookies: &'static [&'static str],
    /// Hosts the collector is allowed to follow one level down.
    pub hosts: &'static [&'static str],
}

const ZHIHU_KEYWORDS: &[&str] = &[
    "AI 副业",
    "一人公司",
    "独立开发 获客",
    "自动化 工作流",
    "远程办公 工具",
];

const JUEJIN_KEYWORDS: &[&str] = &[
    "AI 副业",
    "独立开发",
    "自动化 工作流",
    "开源 项目",
    "前端 副业",
];

const BILIBILI_KEYWORDS: &[&str] = &["AI 副业", "独立开发", "副业 项目", "自动化 工作流", "出海"];

static SITES: [SiteSpec; 5] = [
    SiteSpec {
        channel: "juejin",
        supported: true,
        sign_in_url: "https://juejin.cn/login",
        search_url: "https://juejin.cn/search?query={query}",
        default_keywords: JUEJIN_KEYWORDS,
        auth_cookies: &["sessionid", "sessionid_ss"],
        hosts: &["juejin.cn"],
    },
    SiteSpec {
        channel: "zhihu",
        supported: true,
        sign_in_url: "https://www.zhihu.com/signin",
        search_url: "https://www.zhihu.com/search?type=content&q={query}",
        default_keywords: ZHIHU_KEYWORDS,
        auth_cookies: &["z_c0"],
        hosts: &["zhihu.com"],
    },
    SiteSpec {
        channel: "bilibili",
        supported: true,
        sign_in_url: "https://passport.bilibili.com/login",
        search_url: "https://search.bilibili.com/all?keyword={query}",
        default_keywords: BILIBILI_KEYWORDS,
        auth_cookies: &["SESSDATA"],
        hosts: &["bilibili.com"],
    },
    SiteSpec {
        channel: "wechatMp",
        supported: true,
        sign_in_url: "https://mp.weixin.qq.com/",
        // The MP backend has no public keyword search. The owner pins the
        // "published content" list URL as the seed page instead.
        search_url: "",
        default_keywords: &[],
        auth_cookies: &["slave_sid", "slave_user"],
        hosts: &["mp.weixin.qq.com"],
    },
    SiteSpec {
        channel: "wechatChannels",
        supported: true,
        sign_in_url: "https://channels.weixin.qq.com/platform",
        // The creator console is a wujie micro-frontend with no public search.
        // A fixed content-list route is the list page; rows live in shadow DOM.
        search_url: "https://channels.weixin.qq.com/platform/post/list",
        default_keywords: &[],
        auth_cookies: &["sessionid", "wxuin"],
        hosts: &["channels.weixin.qq.com"],
    },
];

pub fn site_spec(channel: &str) -> Option<&'static SiteSpec> {
    SITES.iter().find(|spec| spec.channel == channel)
}

/// The site a published address really sits on.
///
/// A draft's `channel` says what kind of wording it is, not who hosts it, so a
/// piece that really is out there must be routed by its URL. Only https, and
/// only a host the registry recognises.
pub fn site_by_url(raw: &str) -> Option<&'static SiteSpec> {
    let url = url::Url::parse(raw.trim()).ok()?;
    if url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    SITES.iter().find(|spec| {
        spec.supported
            && spec
                .hosts
                .iter()
                .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
    })
}

// ── settings file ──────────────────────────────────────────────────────────

/// One channel's settings. Login state is deliberately absent: the cookie is
/// the session and the file must not become a second, staler copy of it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct SiteConfig {
    #[serde(default, rename = "seedUrl")]
    seed_url: String,
    #[serde(default, rename = "maxUrls")]
    max_urls: Option<Value>,
    #[serde(default)]
    keywords: Vec<String>,
}

impl SiteConfig {
    fn max_urls(&self) -> u32 {
        normalize_max_urls(self.max_urls.as_ref())
    }

    fn seed_url(&self) -> String {
        normalize_seed_url(&self.seed_url, "")
    }
}

type SiteSettings = HashMap<String, SiteConfig>;

fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("staff-sites.json")
}

/// A hostile or half-written file reads as "no settings", never as an error.
fn read_settings(data_dir: &Path) -> SiteSettings {
    let Ok(raw) = std::fs::read_to_string(settings_path(data_dir)) else {
        return SiteSettings::new();
    };
    let Ok(Value::Object(entries)) = serde_json::from_str::<Value>(&raw) else {
        return SiteSettings::new();
    };
    let mut settings = SiteSettings::new();
    for (channel, entry) in entries {
        if !CHANNELS.contains(&channel.as_str()) {
            continue;
        }
        if let Ok(config) = serde_json::from_value::<SiteConfig>(entry) {
            settings.insert(channel, config);
        }
    }
    settings
}

fn write_settings(data_dir: &Path, settings: &SiteSettings) -> std::io::Result<()> {
    let body = serde_json::to_string_pretty(settings)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;
    std::fs::create_dir_all(data_dir)?;
    let target = settings_path(data_dir);
    let temp = target.with_extension("json.tmp");
    std::fs::write(&temp, body)?;
    std::fs::rename(temp, target)
}

// ── pure helpers (unit-tested without a browser) ───────────────────────────

/// http(s) or the fallback. A stored value that fails this predicate is not a
/// page this process is allowed to open.
fn normalize_seed_url(raw: &str, fallback: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    match url::Url::parse(trimmed) {
        Ok(parsed) if matches!(parsed.scheme(), "http" | "https") => parsed.to_string(),
        _ => fallback.to_string(),
    }
}

/// One page unless the owner raises it, never past the hard ceiling.
fn normalize_max_urls(raw: Option<&Value>) -> u32 {
    let value = match raw {
        None => return DEFAULT_MAX_URLS,
        Some(Value::Number(number)) => number.as_f64().unwrap_or(DEFAULT_MAX_URLS as f64),
        Some(Value::String(text)) => match text.trim().parse::<f64>() {
            Ok(number) => number,
            Err(_) => return DEFAULT_MAX_URLS,
        },
        Some(_) => return DEFAULT_MAX_URLS,
    };
    if !value.is_finite() || value <= 0.0 {
        return 1;
    }
    (value.floor() as u32).clamp(1, MAX_MAX_URLS)
}

/// Trimmed, whitespace-collapsed, deduped, capped. An empty result means the
/// caller falls back to the site's shipped watchlist.
fn normalize_keywords(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for entry in raw {
        let cleaned = collapse_whitespace(entry);
        if cleaned.is_empty() || out.iter().any(|existing| existing == &cleaned) {
            continue;
        }
        out.push(cleaned);
        if out.len() >= MAX_KEYWORDS {
            break;
        }
    }
    out
}

fn collapse_whitespace(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A newline-separated textarea and a JSON array are the two shapes the
/// console sends; both become the same list.
fn keywords_from_value(raw: Option<&Value>) -> Vec<String> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    match raw {
        Value::String(text) => normalize_keywords(
            &text
                .split('\n')
                .map(str::to_string)
                .collect::<Vec<String>>(),
        ),
        Value::Array(rows) => normalize_keywords(
            &rows
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<String>>(),
        ),
        _ => Vec::new(),
    }
}

fn site_keywords(spec: &SiteSpec, config: &SiteConfig) -> Vec<String> {
    let saved = normalize_keywords(&config.keywords);
    if saved.is_empty() {
        spec.default_keywords
            .iter()
            .map(|entry| entry.to_string())
            .collect()
    } else {
        saved
    }
}

/// The list pages a sweep opens, in reading order.
///
/// A pinned seed URL wins outright. Otherwise the watchlist supplies one
/// search page per keyword, rotated by the sweep counter so a raised cap
/// reaches the rest of the list instead of the same rows every run.
fn sweep_urls(spec: &SiteSpec, config: &SiteConfig, offset: usize) -> Vec<String> {
    if !spec.supported {
        return Vec::new();
    }
    let seed = config.seed_url();
    if !seed.is_empty() {
        return vec![seed];
    }
    if spec.search_url.is_empty() {
        return Vec::new();
    }
    // A fixed creator-console route (no keyword placeholder) is one page.
    if !spec.search_url.contains("{query}") {
        return vec![spec.search_url.to_string()];
    }
    let keywords = site_keywords(spec, config);
    if keywords.is_empty() {
        return Vec::new();
    }
    let cap = config.max_urls() as usize;
    let start = offset % keywords.len();
    let count = cap.min(keywords.len());
    (0..count)
        .map(|index| {
            let keyword = &keywords[(start + index) % keywords.len()];
            spec.search_url.replace("{query}", &encode_query(keyword))
        })
        .collect()
}

fn encode_query(raw: &str) -> String {
    url::form_urlencoded::byte_serialize(raw.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

/// A detail page is https, on the site's own hosts, and one level down the
/// path that channel's reader knows.
fn is_detail_url(channel: &str, raw: &str) -> bool {
    let Some(spec) = site_spec(channel) else {
        return false;
    };
    if !spec.supported {
        return false;
    }
    let Some(url) = parse_site_url(spec, raw) else {
        return false;
    };
    detail_path_matches(channel, &url)
}

fn detail_path_matches(channel: &str, url: &url::Url) -> bool {
    let segments: Vec<&str> = url
        .path_segments()
        .map(|parts| parts.filter(|part| !part.is_empty()).collect())
        .unwrap_or_default();
    match channel {
        "zhihu" => {
            segments.first() == Some(&"question")
                && segments
                    .get(1)
                    .map(|id| id.chars().all(|c| c.is_ascii_digit()))
                    .unwrap_or(false)
        }
        // https://juejin.cn/post/7123456789012345678
        "juejin" => {
            segments.first() == Some(&"post")
                && segments
                    .get(1)
                    .map(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric()))
                    .unwrap_or(false)
        }
        // https://www.bilibili.com/video/BV1xx411c7mD or .../video/av170001
        "bilibili" => {
            segments.first() == Some(&"video")
                && segments
                    .get(1)
                    .map(|id| {
                        (id.starts_with("BV")
                            && id.len() > 2
                            && id.chars().all(|c| c.is_ascii_alphanumeric()))
                            || id
                                .strip_prefix("av")
                                .map(|digits| {
                                    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
                                })
                                .unwrap_or(false)
                    })
                    .unwrap_or(false)
        }
        // https://mp.weixin.qq.com/s/<slug> or /s?__biz=...&mid=...&idx=...&sn=...
        "wechatMp" => segments.first() == Some(&"s"),
        _ => false,
    }
}

fn parse_site_url(spec: &SiteSpec, raw: &str) -> Option<url::Url> {
    let url = url::Url::parse(raw.trim()).ok()?;
    if url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    let own = spec
        .hosts
        .iter()
        .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")));
    own.then_some(url)
}

/// Canonical form: the question's own address, without a fragment or a
/// specific answer, so a question reached from two lists is one page.
fn canonical_detail_url(channel: &str, raw: &str) -> Option<String> {
    let spec = site_spec(channel)?;
    let url = parse_site_url(spec, raw)?;
    if !detail_path_matches(channel, &url) {
        return None;
    }
    let segments: Vec<&str> = url
        .path_segments()
        .map(|parts| parts.filter(|part| !part.is_empty()).collect())
        .unwrap_or_default();
    let host = url.host_str()?.to_ascii_lowercase();

    // MP articles exist in two shapes: a `/s/<slug>` permalink and the older
    // `/s?__biz=...&mid=...&idx=...&sn=...` query form. The query form has no
    // second path segment, so it keeps the identity parameters instead.
    if channel == "wechatMp" {
        if segments.first() != Some(&"s") {
            return None;
        }
        if let Some(slug) = segments.get(1).filter(|slug| !slug.is_empty()) {
            return Some(format!("https://{host}/s/{slug}"));
        }
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in url.query_pairs() {
            if matches!(key.as_ref(), "__biz" | "mid" | "idx" | "sn") {
                serializer.append_pair(&key, &value);
            }
        }
        let query = serializer.finish();
        return (!query.is_empty()).then(|| format!("https://{host}/s?{query}"));
    }

    Some(format!(
        "https://{host}/{}/{}",
        segments.first()?,
        segments.get(1)?
    ))
}

/// The pages the list itself linked to, in the page's own order, capped.
fn detail_targets(channel: &str, items: &[Value], limit: usize) -> Vec<String> {
    let limit = limit.min(MAX_DETAIL_PAGES);
    if limit == 0 {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let Some(raw) = item.get("url").and_then(Value::as_str) else {
            continue;
        };
        let Some(url) = canonical_detail_url(channel, raw) else {
            continue;
        };
        if out.iter().any(|existing| existing == &url) {
            continue;
        }
        out.push(url);
        if out.len() >= limit {
            break;
        }
    }
    out
}

// ── cookie session ─────────────────────────────────────────────────────────

/// One cookie the store reported, reduced to what session detection needs.
#[derive(Clone, Debug, Default)]
struct SessionCookie {
    name: String,
    value: String,
    domain: Option<String>,
    path: String,
    /// `None` is a session cookie; `Some` is an absolute expiry.
    expires_unix: Option<i64>,
    secure: bool,
}

#[cfg(test)]
impl SessionCookie {
    fn new(name: &str, value: &str) -> Self {
        Self {
            name: name.to_string(),
            value: value.to_string(),
            path: "/".to_string(),
            ..Self::default()
        }
    }

    fn with_domain(self, domain: &str) -> Self {
        Self {
            domain: Some(domain.to_string()),
            ..self
        }
    }

    fn expires_at(self, unix: i64) -> Self {
        Self {
            expires_unix: Some(unix),
            ..self
        }
    }
}

#[cfg(target_os = "macos")]
impl SessionCookie {
    fn from_wk(cookie: &objc2_foundation::NSHTTPCookie) -> Self {
        let domain = cookie.domain().to_string();
        Self {
            name: cookie.name().to_string(),
            value: cookie.value().to_string(),
            domain: (!domain.is_empty()).then_some(domain),
            path: cookie.path().to_string(),
            expires_unix: if cookie.isSessionOnly() {
                None
            } else {
                cookie
                    .expiresDate()
                    .map(|date| date.timeIntervalSince1970() as i64)
            },
            secure: cookie.isSecure(),
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl SessionCookie {
    fn from_tauri(cookie: &tauri::webview::Cookie<'static>) -> Self {
        use tauri::webview::cookie::{time::OffsetDateTime, Expiration};
        Self {
            name: cookie.name().to_string(),
            value: cookie.value().to_string(),
            domain: cookie.domain().map(str::to_string),
            path: cookie.path().unwrap_or("/").to_string(),
            expires_unix: match cookie.expires() {
                Some(Expiration::DateTime(at)) => Some(at.unix_timestamp()),
                _ => None,
            },
            secure: cookie.secure().unwrap_or(false),
        }
    }
}

const COOKIE_READ_TIMEOUT: Duration = Duration::from_secs(3);

/// Read the shared cookie store without blocking the main thread.
///
/// `WebviewWindow::cookies()` dispatches the read onto the AppKit main thread
/// and then spins that thread's run loop until WebKit answers. When the main
/// thread is already inside a Core Animation transaction -- which is what the
/// first frames after a wake look like -- the nested commit re-enters drawing
/// and deadlocks the process (hang report 2026-11-06 09:02, PID 56615).
/// macOS therefore asks `WKHTTPCookieStore` directly: the request is
/// registered on the main thread and returns at once, and the completion
/// handler feeds a channel the caller awaits.
#[cfg(target_os = "macos")]
async fn read_cookies(app: &AppHandle) -> Vec<SessionCookie> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let sender = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
    let dispatched = app.run_on_main_thread({
        let sender = sender.clone();
        move || {
            use block2::RcBlock;
            use objc2::MainThreadMarker;
            use objc2_foundation::{NSArray, NSHTTPCookie};
            use objc2_web_kit::WKWebsiteDataStore;

            // SAFETY: this closure is on the AppKit main thread, which is
            // what `MainThreadMarker` asserts, and the default data store is
            // the one wry configures for every non-incognito webview.
            unsafe {
                let mtm = MainThreadMarker::new_unchecked();
                let store = WKWebsiteDataStore::defaultDataStore(mtm);
                let block = RcBlock::new(move |rows: std::ptr::NonNull<NSArray<NSHTTPCookie>>| {
                    let parsed: Vec<SessionCookie> = rows
                        .as_ref()
                        .to_vec()
                        .into_iter()
                        .map(|cookie| SessionCookie::from_wk(&cookie))
                        .collect();
                    if let Some(tx) = sender.lock().unwrap().take() {
                        let _ = tx.send(parsed);
                    }
                });
                store.httpCookieStore().getAllCookies(&block);
            }
        }
    });
    if dispatched.is_err() {
        log::warn!(target: "staff-sites", "cookie read could not reach the main thread");
        return Vec::new();
    }
    match tokio::time::timeout(COOKIE_READ_TIMEOUT, rx).await {
        Ok(Ok(cookies)) => cookies,
        _ => {
            log::warn!(target: "staff-sites", "cookie read timed out");
            Vec::new()
        }
    }
}

#[cfg(not(target_os = "macos"))]
async fn read_cookies(app: &AppHandle) -> Vec<SessionCookie> {
    let Some(window) = session_window(app) else {
        return Vec::new();
    };
    window
        .cookies()
        .unwrap_or_default()
        .into_iter()
        .map(|cookie| SessionCookie::from_tauri(&cookie))
        .collect()
}

fn cookie_live(cookie: &SessionCookie) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0);
    match cookie.expires_unix {
        None => true,
        Some(at) => at > now,
    }
}

fn auth_cookie<'a>(cookies: &'a [SessionCookie], spec: &SiteSpec) -> Option<&'a SessionCookie> {
    cookies.iter().find(|cookie| {
        spec.auth_cookies.iter().any(|name| cookie.name == *name)
            && cookie_live(cookie)
            && cookie_domain_matches(cookie, spec)
    })
}

/// A cookie with no recorded domain is treated as the store's own; one that
/// names a domain has to belong to the site.
fn cookie_domain_matches(cookie: &SessionCookie, spec: &SiteSpec) -> bool {
    let Some(domain) = cookie.domain.as_deref() else {
        return true;
    };
    domain_matches_host(domain, spec)
}

fn domain_matches_host(domain: &str, spec: &SiteSpec) -> bool {
    let domain = domain.trim_start_matches('.').to_ascii_lowercase();
    spec.hosts
        .iter()
        .any(|host| domain == *host || domain.ends_with(&format!(".{host}")))
}

fn session_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next())
}

async fn signed_in(app: &AppHandle, spec: &SiteSpec) -> bool {
    if !spec.supported {
        return false;
    }
    let cookies = read_cookies(app).await;
    auth_cookie(&cookies, spec).is_some()
}

/// Whether this machine holds a signed-in session for one channel, for callers
/// outside this module (the reply watch).
pub async fn site_signed_in(app: &AppHandle, channel: &str) -> bool {
    let Some(spec) = site_spec(channel) else {
        return false;
    };
    signed_in(app, spec).await
}

// ── RPC surface ────────────────────────────────────────────────────────────

/// `staff/sites/list`: one row per channel, whether this machine holds a
/// signed-in session, and what a sweep would read.
pub async fn list(app: &AppHandle, state: &AppState) -> Value {
    let settings = read_settings(&state.data_dir);
    let cookies = read_cookies(app).await;
    json!({ "sites": site_rows(&settings, &cookies) })
}

fn site_rows(settings: &SiteSettings, cookies: &[SessionCookie]) -> Vec<Value> {
    SITES
        .iter()
        .map(|spec| {
            let config = settings.get(spec.channel).cloned().unwrap_or_default();
            let signed_in = spec.supported && auth_cookie(cookies, spec).is_some();
            json!({
                "channel": spec.channel,
                "supported": spec.supported,
                "signedIn": signed_in,
                "seedUrl": config.seed_url(),
                "maxUrls": config.max_urls(),
                "keywords": site_keywords(spec, &config),
                "gate": "login",
            })
        })
        .collect()
}

/// `staff/sites/update`: the seed URL, the per-run page cap, and the watchlist.
pub async fn update(
    app: &AppHandle,
    state: &AppState,
    channel: &str,
    patch: &Value,
) -> Result<Value, SiteError> {
    let spec = site_spec(channel)
        .ok_or_else(|| SiteError::invalid(format!("unknown site channel '{channel}'")))?;
    if !spec.supported {
        return Err(SiteError::new(
            "UNSUPPORTED",
            format!("'{channel}' has no collector yet"),
        ));
    }

    let mut settings = read_settings(&state.data_dir);
    let mut config = settings.get(channel).cloned().unwrap_or_default();

    if let Some(raw) = patch.get("seedUrl").and_then(Value::as_str) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            let parsed = url::Url::parse(trimmed)
                .map_err(|_| SiteError::invalid("seedUrl must be an http(s) URL"))?;
            if !matches!(parsed.scheme(), "http" | "https") {
                return Err(SiteError::invalid("seedUrl must be an http(s) URL"));
            }
            config.seed_url = parsed.to_string();
        } else {
            config.seed_url = String::new();
        }
    }
    if let Some(raw) = patch.get("maxUrls") {
        config.max_urls = Some(json!(normalize_max_urls(Some(raw))));
    }
    if let Some(raw) = patch.get("keywords") {
        config.keywords = keywords_from_value(Some(raw));
    }

    settings.insert(channel.to_string(), config.clone());
    write_settings(&state.data_dir, &settings).map_err(|error| {
        SiteError::new("INTERNAL", format!("could not save site settings: {error}"))
    })?;

    let cookies = read_cookies(app).await;
    let signed_in = auth_cookie(&cookies, spec).is_some();
    Ok(json!({
        "channel": spec.channel,
        "supported": spec.supported,
        "signedIn": signed_in,
        "seedUrl": config.seed_url(),
        "maxUrls": config.max_urls(),
        "keywords": site_keywords(spec, &config),
        "gate": "login",
    }))
}

/// `staff/sites/signIn`: the site's own page in its own window. The window
/// closes by itself once the session carries the site's login cookie.
pub async fn sign_in(app: &AppHandle, channel: &str) -> Result<Value, SiteError> {
    let spec = site_spec(channel)
        .ok_or_else(|| SiteError::invalid(format!("unknown site channel '{channel}'")))?;
    if !spec.supported {
        return Err(SiteError::new(
            "UNSUPPORTED",
            format!("'{channel}' has no collector yet"),
        ));
    }
    let label = format!("staff-signin-{channel}");
    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.close();
    }

    let baseline = auth_cookie(&read_cookies(app).await, spec).map(|cookie| cookie.value.clone());

    let url = url::Url::parse(spec.sign_in_url)
        .map_err(|error| SiteError::new("INTERNAL", format!("bad sign-in URL: {error}")))?;
    let window = WebviewWindowBuilder::new(app, label.clone(), WebviewUrl::External(url))
        .title(format!("{channel} sign-in"))
        .inner_size(1024.0, 800.0)
        .min_inner_size(480.0, 480.0)
        .visible(true)
        .build()
        .map_err(|error| {
            SiteError::new(
                "INTERNAL",
                format!("could not open the sign-in window: {error}"),
            )
        })?;

    let deadline = tokio::time::Instant::now() + SIGN_IN_TIMEOUT;
    loop {
        // The owner may close the window after scanning. The cookie, not the
        // close itself, decides whether the session is there.
        if app.get_webview_window(&label).is_none() {
            let signed_in = signed_in(app, spec).await;
            return Ok(json!({ "signedIn": signed_in, "cancelled": !signed_in }));
        }
        let cookies = read_cookies(app).await;
        let current = auth_cookie(&cookies, spec).map(|cookie| cookie.value.clone());
        let changed = match (&baseline, &current) {
            (Some(before), Some(after)) => before != after,
            (None, Some(_)) => true,
            _ => false,
        };
        if changed {
            let _ = window.close();
            return Ok(json!({ "signedIn": true }));
        }
        if tokio::time::Instant::now() >= deadline {
            let _ = window.close();
            let signed_in = signed_in(app, spec).await;
            return Ok(json!({ "signedIn": signed_in, "cancelled": !signed_in }));
        }
        tokio::time::sleep(SIGN_IN_POLL).await;
    }
}

/// `staff/sites/signOut`: delete the cookies the site owns. The renderer's
/// own preferences live in the same store on macOS, so the store is not wiped.
pub async fn sign_out(
    app: &AppHandle,
    _state: &AppState,
    channel: &str,
) -> Result<Value, SiteError> {
    let spec = site_spec(channel)
        .ok_or_else(|| SiteError::invalid(format!("unknown site channel '{channel}'")))?;
    if !spec.supported {
        return Err(SiteError::new(
            "UNSUPPORTED",
            format!("'{channel}' has no collector yet"),
        ));
    }
    let cookies = read_cookies(app).await;
    if let Some(window) = session_window(app) {
        for cookie in &cookies {
            if cookie_belongs_to(cookie, spec) {
                let _ = window.delete_cookie(owned_cookie(cookie));
            }
        }
    }
    // Any sign-in window still open belongs to the session just cleared.
    let label = format!("staff-signin-{channel}");
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.close();
    }
    Ok(json!({ "signedIn": false }))
}

/// Rebuild the wire cookie `delete_cookie` expects from the snapshot.
fn owned_cookie(cookie: &SessionCookie) -> tauri::webview::Cookie<'static> {
    let mut builder = tauri::webview::Cookie::build((cookie.name.clone(), cookie.value.clone()))
        .path(cookie.path.clone());
    if let Some(domain) = &cookie.domain {
        builder = builder.domain(domain.clone());
    }
    if cookie.secure {
        builder = builder.secure(true);
    }
    builder.build()
}

fn cookie_belongs_to(cookie: &SessionCookie, spec: &SiteSpec) -> bool {
    cookie
        .domain
        .as_deref()
        .map(|domain| domain_matches_host(domain, spec))
        .unwrap_or(false)
}

// ── collection ─────────────────────────────────────────────────────────────

static SWEEP_OFFSET: AtomicUsize = AtomicUsize::new(0);

/// `staff/sites/collect`: one manual sweep the owner can inspect before
/// trusting a scheduled run to it.
pub async fn collect(app: &AppHandle, state: &AppState, channel: &str) -> Result<Value, SiteError> {
    let spec = site_spec(channel)
        .ok_or_else(|| SiteError::invalid(format!("unknown site channel '{channel}'")))?;
    if !spec.supported {
        return Err(SiteError::new(
            "UNSUPPORTED",
            format!("'{channel}' has no collector yet"),
        ));
    }
    let pages = collect_pages(app, state, spec).await;
    Ok(json!({ "pages": pages }))
}

/// The probe step's half: read every supported source the step names, return
/// the bounded notes the driver hands to the model.
///
/// A manual run reaching a signed-out site opens its sign-in window and waits,
/// because the owner is at the console. A scheduled run never opens a window
/// and reports the site as skipped instead.
pub async fn collect_for_step(
    app: &AppHandle,
    state: &AppState,
    sources: &[String],
    manual: bool,
) -> Vec<String> {
    let mut seen: Vec<&str> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut spent = 0usize;
    let mut dropped_pages = 0usize;

    for channel in sources {
        let Some(spec) = site_spec(channel) else {
            continue;
        };
        if !spec.supported || seen.contains(&spec.channel) {
            continue;
        }
        seen.push(spec.channel);

        if !signed_in(app, spec).await {
            if manual {
                let _ = sign_in(app, spec.channel).await;
            }
            if !signed_in(app, spec).await {
                notes.push(if manual {
                    format!(
                        "- [collected:{}] 已跳过：登录窗关闭时这台机器仍没有该站点的登录会话",
                        spec.channel
                    )
                } else {
                    format!(
                        "- [collected:{}] 已跳过：这台机器没有该站点的登录会话（定时运行不会打开登录窗）",
                        spec.channel
                    )
                });
                continue;
            }
        }

        let pages = collect_pages(app, state, spec).await;
        for page in pages {
            if spent >= TOTAL_CHARS {
                dropped_pages += 1;
                continue;
            }
            let note = format_note(spec.channel, &page, PAGE_CHARS);
            let note = trim_chars(&note, TOTAL_CHARS - spent);
            spent += note.chars().count();
            notes.push(note);
        }
    }

    if dropped_pages > 0 {
        notes.push(format!(
            "- [collected] 另有 {dropped_pages} 页因本轮材料总长度上限没有放入"
        ));
    }
    notes
}

/// One sweep: the configured list page(s), then the detail pages the first
/// list linked to. A signed-out session is reported, never read as empty.
async fn collect_pages(app: &AppHandle, state: &AppState, spec: &'static SiteSpec) -> Vec<Value> {
    let settings = read_settings(&state.data_dir);
    let config = settings.get(spec.channel).cloned().unwrap_or_default();
    let offset = SWEEP_OFFSET.load(Ordering::Relaxed);
    let urls = sweep_urls(spec, &config, offset);
    SWEEP_OFFSET.store(offset.saturating_add(urls.len().max(1)), Ordering::Relaxed);
    if urls.is_empty() {
        return vec![error_page(
            spec.channel,
            "",
            "no page configured: set a seed URL or a watchlist keyword",
        )];
    }

    if !signed_in(app, spec).await {
        return vec![error_page(
            spec.channel,
            &urls[0],
            "no signed-in session for this site",
        )];
    }

    let label = format!(
        "staff-collect-{}-{}",
        spec.channel,
        uuid::Uuid::new_v4().simple()
    );
    let channel = spec.channel.to_string();
    let first = urls[0].clone();
    let first_url = match url::Url::parse(&first) {
        Ok(url) => url,
        Err(_) => return vec![error_page(spec.channel, &first, "invalid page URL")],
    };
    let script_channel = channel.clone();
    let (window, mut rx) = match open_page_window(app, label, first_url, move |url| {
        if is_detail_url(&script_channel, url.as_str()) {
            detail_script(&script_channel)
        } else {
            list_script(&script_channel)
        }
    }) {
        Ok((window, rx)) => (window, rx),
        Err(error) => {
            log::warn!(
                "staff site {} collector window failed: {error}",
                spec.channel
            );
            return vec![error_page(
                spec.channel,
                &first,
                "could not open the collector window",
            )];
        }
    };

    let mut pages: Vec<Value> = Vec::new();
    let mut list_items: Vec<Value> = Vec::new();
    let mut session_lost = false;

    // Every configured list page first, in order. One page unless the owner
    // raised the cap.
    for (index, target) in urls.iter().enumerate() {
        match read_target(&window, &mut rx, target, index > 0).await {
            PageRead::Result(result) => {
                let page = page_from_result(spec.channel, &result, target);
                if !is_detail_url(&channel, target) {
                    if let Some(rows) = page.get("items").and_then(Value::as_array) {
                        list_items.extend(rows.clone());
                    }
                }
                pages.push(page);
            }
            PageRead::Wall(url) => {
                pages.push(error_page(
                    spec.channel,
                    &url,
                    "no signed-in session for this site",
                ));
                session_lost = true;
                break;
            }
            PageRead::Closed => {
                pages.push(error_page(
                    spec.channel,
                    target,
                    "the collector window closed before the page answered",
                ));
                session_lost = true;
                break;
            }
            PageRead::Timeout => {
                pages.push(error_page(
                    spec.channel,
                    target,
                    "the page did not finish rendering in time",
                ));
            }
        }
    }

    // Then the questions the lists linked to, in the page's own order. One
    // unreachable question is recorded and skipped, not fatal.
    if !session_lost {
        let details = detail_targets(&channel, &list_items, DEFAULT_DETAIL_PAGES);
        for target in &details {
            match read_target(&window, &mut rx, target, true).await {
                PageRead::Result(result) => {
                    pages.push(page_from_result(spec.channel, &result, target));
                }
                PageRead::Wall(url) => {
                    log::warn!(
                        "staff site {} detail {target} met the login wall",
                        spec.channel
                    );
                    pages.push(error_page(
                        spec.channel,
                        &url,
                        "no signed-in session for this site",
                    ));
                    break;
                }
                PageRead::Closed => {
                    log::warn!(
                        "staff site {} detail {target} lost the window",
                        spec.channel
                    );
                    pages.push(error_page(
                        spec.channel,
                        target,
                        "the collector window closed before the page answered",
                    ));
                    break;
                }
                PageRead::Timeout => {
                    log::warn!("staff site {} detail {target} timed out", spec.channel);
                    pages.push(error_page(
                        spec.channel,
                        target,
                        "the page did not finish rendering in time",
                    ));
                }
            }
        }
    }

    let _ = window.close();
    pages
}

enum PageRead {
    Result(Value),
    /// The page landed on the site's login wall; the string is where it landed.
    Wall(String),
    Closed,
    Timeout,
}

async fn read_target(
    window: &WebviewWindow,
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<Value>,
    target: &str,
    navigate: bool,
) -> PageRead {
    if navigate {
        while rx.try_recv().is_ok() {}
        if let Ok(url) = url::Url::parse(target) {
            let _ = window.navigate(url);
        }
    }
    match tokio::time::timeout(SITE_TIMEOUT, rx.recv()).await {
        Ok(Some(value)) => {
            if is_login_wall(&value) {
                let url = value
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or(target)
                    .to_string();
                PageRead::Wall(url)
            } else {
                PageRead::Result(value)
            }
        }
        Ok(None) => PageRead::Closed,
        Err(_) => PageRead::Timeout,
    }
}

/// One hidden window that loads a site page and hands its extraction back
/// through the custom scheme. Shared by the sweep and the reply watch.
fn open_page_window<F>(
    app: &AppHandle,
    label: String,
    first_url: url::Url,
    script_for: F,
) -> Result<(WebviewWindow, tokio::sync::mpsc::UnboundedReceiver<Value>), String>
where
    F: Fn(&url::Url) -> String + Send + Sync + 'static,
{
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
    let nav_tx = tx.clone();
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::External(first_url))
        .title("staff page")
        .inner_size(1280.0, 900.0)
        .visible(false)
        .skip_taskbar(true)
        .background_throttling(BackgroundThrottlingPolicy::Disabled)
        .on_navigation(move |url| {
            if url.scheme() == "pigo-collect" {
                if let Some((_, data)) = url.query_pairs().find(|(key, _)| key.as_ref() == "data") {
                    if let Ok(value) = serde_json::from_str::<Value>(&data) {
                        let _ = nav_tx.send(value);
                    }
                }
                return false;
            }
            true
        })
        .on_page_load(move |window, payload| {
            if payload.event() != PageLoadEvent::Finished {
                return;
            }
            let url = payload.url().clone();
            if !matches!(url.scheme(), "http" | "https") {
                return;
            }
            let _ = window.eval(script_for(&url));
        })
        .build()
        .map_err(|error| error.to_string())?;
    Ok((window, rx))
}

/// How many replies one page may hand to the lead board.
const MAX_REPLY_ITEMS: usize = 20;

/// Read one published piece back for the reply watch.
///
/// The fingerprint (the head of the body we published) is how the page is
/// asked "is our text here, and what did anybody say under it". A page that
/// does not carry it is an ordinary answer, not an error: a comment can be
/// pushed off the first page, folded behind a link, or deleted.
pub async fn read_published(
    app: &AppHandle,
    channel: &str,
    url: &str,
    fingerprint: &str,
) -> Result<Value, SiteError> {
    let spec = site_spec(channel)
        .ok_or_else(|| SiteError::invalid(format!("unknown site channel '{channel}'")))?;
    if !spec.supported {
        return Err(SiteError::new(
            "UNSUPPORTED",
            format!("'{channel}' has no collector yet"),
        ));
    }
    let parsed = url::Url::parse(url.trim())
        .ok()
        .filter(|url| url.scheme() == "https")
        .ok_or_else(|| SiteError::invalid("publishedUrl must be an https URL"))?;
    if !signed_in(app, spec).await {
        return Ok(json!({ "found": false, "reason": "没有登录会话" }));
    }

    let script = published_script(fingerprint);
    let label = format!(
        "staff-watch-{}-{}",
        spec.channel,
        uuid::Uuid::new_v4().simple()
    );
    let (window, mut rx) = match open_page_window(app, label, parsed, move |_| script.clone()) {
        Ok((window, rx)) => (window, rx),
        Err(error) => {
            return Err(SiteError::new(
                "INTERNAL",
                format!("could not open the collector window: {error}"),
            ))
        }
    };

    let outcome = read_target(&window, &mut rx, url, false).await;
    let _ = window.close();
    Ok(match outcome {
        PageRead::Result(result) => published_from_result(&result),
        PageRead::Wall(_) => json!({ "found": false, "reason": "没有登录会话" }),
        PageRead::Closed => json!({
            "found": false,
            "reason": "采集窗口在页面回应之前关闭了",
        }),
        PageRead::Timeout => json!({
            "found": false,
            "reason": "页面没有在限定时间内渲染完成",
        }),
    })
}

fn published_from_result(result: &Value) -> Value {
    if !result
        .get("found")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let reason = result
            .get("reason")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|reason| !reason.is_empty())
            .unwrap_or("第一页上没有找到我们发布的内容");
        return json!({ "found": false, "reason": reason });
    }

    let mut out = json!({ "found": true });
    for key in ["replies", "views", "likes"] {
        if let Some(number) = result.get(key).and_then(Value::as_i64).filter(|n| *n >= 0) {
            out[key] = json!(number);
        }
    }
    if let Some(comment_id) = result
        .get("commentId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        out["commentId"] = json!(comment_id);
    }
    let items: Vec<Value> = result
        .get("items")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let text = row
                        .get("text")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|text| !text.is_empty())?;
                    let mut item = json!({
                        "text": text.chars().take(280).collect::<String>(),
                    });
                    if let Some(author) = row
                        .get("author")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|author| !author.is_empty())
                    {
                        item["author"] = json!(author);
                    }
                    if let Some(at) = row
                        .get("at")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|at| !at.is_empty())
                    {
                        item["at"] = json!(at);
                    }
                    Some(item)
                })
                .take(MAX_REPLY_ITEMS)
                .collect()
        })
        .unwrap_or_default();
    out["items"] = json!(items);
    out
}

fn is_login_wall(result: &Value) -> bool {
    if result
        .get("loginWall")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }
    let path = result.get("path").and_then(Value::as_str).unwrap_or("");
    path.contains("signin") || path.contains("signup") || path.contains("login")
}

fn page_from_result(channel: &str, result: &Value, fallback_url: &str) -> Value {
    let url = result
        .get("url")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback_url);
    let title = result
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let summary = result
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let mut items: Vec<Value> = Vec::new();
    if let Some(rows) = result.get("items").and_then(Value::as_array) {
        for row in rows {
            let Some(title) = row.get("title").and_then(Value::as_str) else {
                continue;
            };
            let title = title.trim();
            if title.is_empty() {
                continue;
            }
            let mut item = json!({ "title": title });
            if let Some(url) = row
                .get("url")
                .and_then(Value::as_str)
                .filter(|u| !u.is_empty())
            {
                item["url"] = json!(url);
            }
            if let Some(excerpt) = row
                .get("excerpt")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|e| !e.is_empty())
            {
                item["excerpt"] = json!(excerpt);
            }
            if let Some(meta) = row
                .get("meta")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|m| !m.is_empty())
            {
                item["meta"] = json!(meta);
            }
            if let Some(kind) = row.get("kind").and_then(Value::as_str) {
                let kind = match kind {
                    "answer" => "answer",
                    "comment" => "comment",
                    _ => "item",
                };
                item["kind"] = json!(kind);
            }
            items.push(item);
        }
    }
    let count = items.len();
    let mut page = json!({
        "channel": channel,
        "url": url,
        "title": title,
        "itemCount": count,
        "items": items,
    });
    if !summary.is_empty() {
        page["summary"] = json!(summary);
    }
    page
}

fn error_page(channel: &str, url: &str, error: &str) -> Value {
    json!({
        "channel": channel,
        "url": url,
        "title": "",
        "itemCount": 0,
        "items": [],
        "error": error,
    })
}

// ── prompt material ────────────────────────────────────────────────────────

/// One page as the step reads it: the header, the page's own preamble, then
/// each row. The note is capped, and rows that do not fit are counted rather
/// than dropped silently.
fn format_note(channel: &str, page: &Value, limit: usize) -> String {
    let url = page.get("url").and_then(Value::as_str).unwrap_or("");
    if let Some(error) = page
        .get("error")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        return trim_chars(
            &format!("- [collected:{channel}] {url}\n  读不到：{error}"),
            limit,
        );
    }

    let title = page
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let mut out = if title.is_empty() {
        format!("- [collected:{channel}] {url}")
    } else {
        format!("- [collected:{channel}] {title} — {url}")
    };
    if let Some(summary) = page
        .get("summary")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        out.push_str(&format!("\n  正文：{summary}"));
    }

    let items = page
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        push_within(
            &mut out,
            "  （没有读到列表项，页面可能没有渲染或是空的）",
            limit,
        );
        return trim_chars(&out, limit);
    }

    let mut written = 0usize;
    for (index, item) in items.iter().enumerate() {
        let lines = item_lines(item);
        let block = format!("\n{}", lines.join("\n"));
        if out.chars().count() + block.chars().count() > limit {
            let omitted = items.len() - index;
            push_within(
                &mut out,
                &format!("  （其余 {omitted} 项因长度省略）"),
                limit,
            );
            return trim_chars(&out, limit);
        }
        out.push_str(&block);
        written += 1;
    }
    if written == 0 {
        push_within(
            &mut out,
            "  （没有读到列表项，页面可能没有渲染或是空的）",
            limit,
        );
    }
    trim_chars(&out, limit)
}

fn item_lines(item: &Value) -> Vec<String> {
    let title = item.get("title").and_then(Value::as_str).unwrap_or("");
    let url = item.get("url").and_then(Value::as_str).unwrap_or("");
    let kind = item.get("kind").and_then(Value::as_str).unwrap_or("item");
    let mut lines = Vec::new();
    match kind {
        "answer" => lines.push(if url.is_empty() {
            format!("  - 回答｜{title}")
        } else {
            format!("  - 回答｜{title} — {url}")
        }),
        "comment" => lines.push(format!("  - 评论｜{title}")),
        _ => lines.push(if url.is_empty() {
            format!("  - {title}")
        } else {
            format!("  - {title} — {url}")
        }),
    }
    if let Some(meta) = item
        .get("meta")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("    {meta}"));
    }
    if let Some(excerpt) = item
        .get("excerpt")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("    {excerpt}"));
    }
    lines
}

fn push_within(out: &mut String, line: &str, limit: usize) {
    let block = format!("\n{line}");
    if out.chars().count() + block.chars().count() <= limit {
        out.push_str(&block);
    }
}

fn trim_chars(raw: &str, limit: usize) -> String {
    if raw.chars().count() <= limit {
        return raw.to_string();
    }
    raw.chars().take(limit).collect()
}

/// The extraction the reply watch injects into a published page.
///
/// The fingerprint is the only way to find our own paragraph among the page's
/// answers, because the site never told us which one was ours. The script
/// reads what rendered: it never calls an endpoint or forges a signature.
const PUBLISHED_SCRIPT: &str = r#"
(function () {
  var finished = false;
  var FINGERPRINT = __FINGERPRINT__;
  function clean(value) {
    return (value || "").replace(/\u200b/g, "").replace(/\s+/g, " ").trim();
  }
  function loginWall() {
    var path = location.pathname || "";
    if (/signin|signup|login/i.test(path)) return true;
    var sample = document.body ? document.body.innerText : "";
    return /扫码登录|登录\/注册|请先登录|密码登录/.test(sample);
  }
  function finish(payload) {
    if (finished) return;
    finished = true;
    try {
      payload.url = location.href;
      payload.path = location.pathname;
      payload.loginWall = loginWall();
      window.location.href = "pigo-collect://result?data=" +
        encodeURIComponent(JSON.stringify(payload));
    } catch (_) {}
  }
  function holders() {
    return Array.from(document.querySelectorAll(
      ".RichText, [class*='RichText'], .AnswerItem, [class*='AnswerItem'], article, [class*='Post'], .topic_content, .reply_content, .article-content, #js_content, .rich_media_content"
    ));
  }
  function findHolder() {
    var rows = holders();
    var best = null;
    for (var i = 0; i < rows.length; i++) {
      var text = clean(rows[i].innerText);
      if (text.indexOf(FINGERPRINT) === -1) continue;
      if (!best || clean(best.innerText).length > text.length) best = rows[i];
    }
    return best;
  }
  function answerScope(holder) {
    if (!holder) return document.body;
    var scope = holder.closest(
      ".AnswerItem, [class*='AnswerItem'], [class*='AnswerCard'], article, [class*='Post'], .ContentItem, [class*='topic'], [class*='article'], .cell, #js_content, .rich_media_content"
    );
    return scope || holder.parentElement || holder;
  }
  function findCount(scope) {
    var node = scope;
    for (var depth = 0; depth < 4 && node; depth++) {
      var rows = Array.from(node.querySelectorAll("button, [role='button'], a, span"));
      for (var i = 0; i < rows.length; i++) {
        var label = clean(rows[i].textContent);
        var m = /^(\d+)\s*(?:条)?(?:评论|回复)/.exec(label);
        if (m) return { button: rows[i], replies: parseInt(m[1], 10) };
      }
      node = node.parentElement;
    }
    return null;
  }
  function findPanel(scope) {
    var node = scope;
    for (var depth = 0; depth < 4 && node; depth++) {
      var panel = node.querySelector(
        ".Comments-container, [class*='CommentList'], [class*='Comments']"
      );
      if (panel) return panel;
      node = node.parentElement;
    }
    var panels = Array.from(document.querySelectorAll(
      ".Comments-container, [class*='CommentList']"
    ));
    if (panels.length) return panels[panels.length - 1];
    // V2EX and Juejin render replies as siblings of the topic/article body
    // rather than inside a comments panel.
    if (document.querySelector(".reply_content, .comment-item")) return document.body;
    return null;
  }
  function readComments(scope, next) {
    var count = findCount(scope);
    if (!count) {
      next({ replies: null, items: [] });
      return;
    }
    try { count.button.click(); } catch (_) {}
    setTimeout(function () {
      var panel = findPanel(scope);
      var rows = panel ? Array.from(panel.querySelectorAll(
        "[class*='CommentItem'], .CommentContent, [class*='comment-item'], [class*='Reply'], .reply_content"
      )) : [];
      var items = [];
      var seen = {};
      for (var i = 0; i < rows.length && items.length < 20; i++) {
        var row = rows[i];
        var authorNode = row.querySelector(
          "[class*='AuthorInfo-name'], .CommentAuthor, [class*='author'], .dark, [class*='user']"
        );
        var bodyNode = row.querySelector(".RichText, [class*='content'], p, .reply_content, [class*='reply']");
        var timeNode = row.querySelector("[class*='time'], [class*='Time']");
        var author = clean(authorNode ? authorNode.textContent : "");
        var text = clean(bodyNode ? bodyNode.textContent : row.textContent);
        if (!text) continue;
        var key = (author || "") + "\n" + text.slice(0, 80);
        if (seen[key]) continue;
        seen[key] = true;
        items.push({
          author: author,
          text: text.slice(0, 280),
          at: clean(timeNode ? timeNode.textContent : "")
        });
      }
      try { count.button.click(); } catch (_) {}
      next({ replies: count.replies, items: items });
    }, 900);
  }
  function read() {
    var holder = findHolder();
    var bodyText = clean(document.body ? document.body.innerText : "");
    if (!holder && bodyText.indexOf(FINGERPRINT) === -1) {
      finish({ found: false, reason: "第一页上没有找到我们发布的内容" });
      return;
    }
    var scope = answerScope(holder);
    var scopeText = clean(scope ? scope.innerText : bodyText);
    var views = null;
    var viewMatch = /(\d+)\s*(?:次)?(?:浏览|阅读)/.exec(bodyText);
    if (viewMatch) views = parseInt(viewMatch[1], 10);
    var likes = null;
    var likeMatch = /(?:赞同|喜欢|点赞|感谢|在看)\s*(\d+)/.exec(scopeText);
    if (likeMatch) likes = parseInt(likeMatch[1], 10);
    readComments(scope, function (comments) {
      finish({
        found: true,
        replies: comments.replies,
        views: views,
        likes: likes,
        items: comments.items
      });
    });
  }
  function start() {
    if (loginWall()) {
      finish({ found: false, reason: "没有登录会话" });
      return;
    }
    if (!FINGERPRINT) {
      finish({ found: false, reason: "这份稿子没有可匹配的正文" });
      return;
    }
    var rounds = 0;
    function scrollStep() {
      window.scrollTo(0, document.body ? document.body.scrollHeight : 0);
      rounds += 1;
      if (rounds < 2) {
        setTimeout(scrollStep, 900);
        return;
      }
      read();
    }
    scrollStep();
  }
  setTimeout(start, 1200);
})();
"#;

fn published_script(fingerprint: &str) -> String {
    let literal = serde_json::to_string(fingerprint).unwrap_or_else(|_| "\"\"".to_string());
    PUBLISHED_SCRIPT.replace("__FINGERPRINT__", &literal)
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_probe_channel_has_a_site_entry() {
        for channel in CHANNELS {
            let spec = site_spec(channel).expect(channel);
            assert_eq!(spec.channel, channel);
        }
        assert!(site_spec("nope").is_none());
    }

    #[test]
    fn juejin_bilibili_wechat_and_zhihu_are_wired() {
        let wired: Vec<&str> = SITES
            .iter()
            .filter(|spec| spec.supported)
            .map(|spec| spec.channel)
            .collect();
        assert_eq!(
            wired,
            vec!["juejin", "zhihu", "bilibili", "wechatMp", "wechatChannels"]
        );

        let zhihu = site_spec("zhihu").expect("zhihu");
        assert_eq!(zhihu.sign_in_url, "https://www.zhihu.com/signin");
        assert_eq!(
            zhihu.search_url,
            "https://www.zhihu.com/search?type=content&q={query}"
        );

        let juejin = site_spec("juejin").expect("juejin");
        assert_eq!(juejin.sign_in_url, "https://juejin.cn/login");
        assert_eq!(juejin.search_url, "https://juejin.cn/search?query={query}");
        assert_eq!(juejin.auth_cookies, &["sessionid", "sessionid_ss"]);

        let bilibili = site_spec("bilibili").expect("bilibili");
        assert_eq!(bilibili.sign_in_url, "https://passport.bilibili.com/login");
        assert_eq!(
            bilibili.search_url,
            "https://search.bilibili.com/all?keyword={query}"
        );
        assert_eq!(bilibili.auth_cookies, &["SESSDATA"]);

        let wechat = site_spec("wechatMp").expect("wechatMp");
        assert_eq!(wechat.sign_in_url, "https://mp.weixin.qq.com/");
        assert_eq!(wechat.search_url, "");
        assert!(wechat.default_keywords.is_empty());
        assert_eq!(wechat.auth_cookies, &["slave_sid", "slave_user"]);

        let channels = site_spec("wechatChannels").expect("wechatChannels");
        assert_eq!(
            channels.sign_in_url,
            "https://channels.weixin.qq.com/platform"
        );
        assert_eq!(
            channels.search_url,
            "https://channels.weixin.qq.com/platform/post/list"
        );
        assert!(channels.default_keywords.is_empty());
        assert_eq!(channels.auth_cookies, &["sessionid", "wxuin"]);

        // A channel without a collector can never recognise its own pages.
        for spec in SITES.iter() {
            assert_eq!(!spec.hosts.is_empty(), spec.supported, "{}", spec.channel);
            if !matches!(spec.channel, "wechatMp" | "wechatChannels") {
                assert!(!spec.default_keywords.is_empty(), "{}", spec.channel);
            }
        }
    }

    #[test]
    fn a_published_link_names_the_site_on_its_host() {
        for (url, channel) in [
            ("https://zhuanlan.zhihu.com/p/1997658098362508695", "zhihu"),
            ("https://www.zhihu.com/question/1/answer/2", "zhihu"),
            ("https://juejin.cn/post/7123456789012345678", "juejin"),
            ("https://www.bilibili.com/video/BV1xx411c7mD", "bilibili"),
            ("https://mp.weixin.qq.com/s/AbCdEfGh", "wechatMp"),
            ("https://channels.weixin.qq.com/platform", "wechatChannels"),
        ] {
            assert_eq!(site_by_url(url).map(|spec| spec.channel), Some(channel));
        }
        assert_eq!(
            site_by_url("https://notzhihu.com/p/1").map(|s| s.channel),
            None
        );
        assert_eq!(
            site_by_url("https://zhihu.com.evil.test/p/1").map(|s| s.channel),
            None
        );
        assert_eq!(
            site_by_url("https://juejin.cn.evil.test/post/1").map(|s| s.channel),
            None
        );
        assert_eq!(
            site_by_url("https://bilibili.com.evil.test/video/BV1xx411c7mD").map(|s| s.channel),
            None
        );
        assert_eq!(
            site_by_url("https://mp.weixin.qq.com.evil.test/s/abc").map(|s| s.channel),
            None
        );
        assert_eq!(site_by_url("http://zhihu.com/p/1").map(|s| s.channel), None);
        assert_eq!(site_by_url("javascript:alert(1)").map(|s| s.channel), None);
        assert_eq!(site_by_url("").map(|s| s.channel), None);
        assert_eq!(site_by_url("not a url").map(|s| s.channel), None);
    }

    #[test]
    fn a_published_extraction_becomes_the_shape_the_watch_ingests() {
        let found = published_from_result(&json!({
            "found": true,
            "replies": 3,
            "views": 1200,
            "likes": 5,
            "commentId": "",
            "items": [
                { "author": "张三", "text": "有联系方式吗", "at": "2026-09-18" },
                { "author": "", "text": "   ", "at": "" },
                { "author": "李四", "text": "我也想问", "at": "" },
            ],
        }));
        assert_eq!(found["found"], json!(true));
        assert_eq!(found["replies"], json!(3));
        assert_eq!(found["views"], json!(1200));
        assert_eq!(found["likes"], json!(5));
        let items = found["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["author"], json!("张三"));
        assert_eq!(items[0]["at"], json!("2026-09-18"));
        assert_eq!(items[1]["author"], json!("李四"));

        let missing = published_from_result(&json!({
            "found": false,
            "reason": "第一页上没有找到我们发布的内容",
        }));
        assert_eq!(missing["found"], json!(false));
        assert_eq!(missing["reason"], json!("第一页上没有找到我们发布的内容"));
        assert_eq!(
            published_from_result(&json!({ "found": false }))["reason"],
            json!("第一页上没有找到我们发布的内容")
        );
    }

    #[test]
    fn the_published_script_is_valid_javascript_and_never_calls_the_site_api() {
        let script = published_script("一个人怎么把生意做起来");
        assert!(script.contains("document.querySelectorAll"));
        assert!(script.contains("__FINGERPRINT__") == false);
        assert!(script.contains("\"一个人怎么把生意做起来\""));
        assert!(!script.contains("fetch("));
        assert!(!script.contains("XMLHttpRequest"));
        assert!(script.contains("评论"));
        assert!(script.contains("回复"));
        assert!(script.contains(".reply_content"));
        assert!(script.contains(".article-content"));
        assert!(script.contains("#js_content"));
        assert!(script.contains("阅读"));
        assert!(script.contains("在看"));
        assert!(script.contains(".click()"));
    }

    #[test]
    fn a_session_is_only_the_sites_own_live_cookie() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs() as i64)
            .unwrap_or(0);
        let zhihu = site_spec("zhihu").expect("zhihu");
        assert!(auth_cookie(&[], zhihu).is_none());
        let plain = SessionCookie::new("d_c0", "x");
        assert!(auth_cookie(&[plain], zhihu).is_none());
        let login = SessionCookie::new("z_c0", "x");
        assert!(auth_cookie(&[login], zhihu).is_some());

        let expired = SessionCookie::new("z_c0", "x").expires_at(now - 60);
        assert!(auth_cookie(&[expired], zhihu).is_none());

        let live = SessionCookie::new("z_c0", "x").expires_at(now + 3600);
        assert!(auth_cookie(&[live], zhihu).is_some());

        let owned = SessionCookie::new("z_c0", "x").with_domain(".zhihu.com");
        assert!(auth_cookie(&[owned], zhihu).is_some());
        let foreign = SessionCookie::new("z_c0", "x").with_domain("zhihu.com.evil.test");
        assert!(auth_cookie(&[foreign], zhihu).is_none());

        // The MP backend's cookie name is specific enough that a shared-store
        // `sessionid` from another site must not sign this channel in.
        let wechat = site_spec("wechatMp").expect("wechatMp");
        assert!(auth_cookie(&[SessionCookie::new("slave_sid", "x")], wechat).is_some());
        assert!(auth_cookie(&[SessionCookie::new("sessionid", "x")], wechat).is_none());
        let mp_owned = SessionCookie::new("slave_sid", "x").with_domain(".mp.weixin.qq.com");
        assert!(auth_cookie(&[mp_owned], wechat).is_some());
        let mp_foreign =
            SessionCookie::new("slave_sid", "x").with_domain("mp.weixin.qq.com.evil.test");
        assert!(auth_cookie(&[mp_foreign], wechat).is_none());
    }

    #[test]
    fn the_console_row_matches_the_shared_wire_type() {
        let rows = site_rows(&SiteSettings::new(), &[]);
        assert_eq!(rows.len(), CHANNELS.len());
        let wired = ["bilibili", "juejin", "wechatChannels", "wechatMp", "zhihu"];
        for (row, channel) in rows.iter().zip(CHANNELS) {
            assert_eq!(row["channel"], json!(channel));
            assert_eq!(row["supported"].as_bool(), Some(wired.contains(&channel)));
            assert_eq!(row["signedIn"], json!(false));
            assert_eq!(row["seedUrl"], json!(""));
            assert_eq!(row["maxUrls"], json!(1));
            assert_eq!(row["gate"], json!("login"));
            assert!(row["keywords"].is_array());
            if wired.contains(&channel) && !matches!(channel, "wechatMp" | "wechatChannels") {
                assert!(row["keywords"].as_array().unwrap().len() > 1);
            }
            if matches!(channel, "wechatMp" | "wechatChannels") {
                assert!(row["keywords"].as_array().unwrap().is_empty());
            }
        }

        let cookies = vec![SessionCookie::new("z_c0", "x")];
        let rows = site_rows(&SiteSettings::new(), &cookies);
        let zhihu = rows.iter().find(|row| row["channel"] == "zhihu").unwrap();
        assert_eq!(zhihu["signedIn"], json!(true));
        for row in rows.iter().filter(|row| row["channel"] != "zhihu") {
            assert_eq!(row["signedIn"], json!(false));
        }
    }

    #[test]
    fn a_seed_url_is_http_or_it_is_refused() {
        assert_eq!(
            normalize_seed_url("https://www.zhihu.com/hot", "fallback"),
            "https://www.zhihu.com/hot"
        );
        assert_eq!(
            normalize_seed_url(" http://example.com/a ", "fallback"),
            "http://example.com/a"
        );
        assert_eq!(
            normalize_seed_url("file:///etc/passwd", "fallback"),
            "fallback"
        );
        assert_eq!(
            normalize_seed_url("javascript:alert(1)", "fallback"),
            "fallback"
        );
        assert_eq!(normalize_seed_url("", "fallback"), "");
        assert_eq!(normalize_seed_url("   ", "fallback"), "");
    }

    #[test]
    fn the_sweep_cap_stays_between_one_and_the_hard_ceiling() {
        assert_eq!(normalize_max_urls(None), DEFAULT_MAX_URLS);
        assert_eq!(normalize_max_urls(None), 1);
        assert_eq!(normalize_max_urls(Some(&json!(0))), 1);
        assert_eq!(normalize_max_urls(Some(&json!(-5))), 1);
        assert_eq!(normalize_max_urls(Some(&json!(2.7))), 2);
        assert_eq!(normalize_max_urls(Some(&json!(9999))), MAX_MAX_URLS);
        assert_eq!(normalize_max_urls(Some(&json!("3"))), 3);
        assert_eq!(normalize_max_urls(Some(&json!("abc"))), DEFAULT_MAX_URLS);
    }

    #[test]
    fn keywords_are_cleaned_deduped_and_capped() {
        assert!(normalize_keywords(&[]).is_empty());
        assert_eq!(
            normalize_keywords(&[
                "  a  b ".to_string(),
                String::new(),
                "   ".to_string(),
                "a b".to_string()
            ]),
            vec!["a b"]
        );
        let many = (0..MAX_KEYWORDS + 5)
            .map(|index| format!("k{index}"))
            .collect::<Vec<_>>();
        assert_eq!(normalize_keywords(&many).len(), MAX_KEYWORDS);
    }

    #[test]
    fn an_emptied_watchlist_falls_back_to_the_shipped_list() {
        let zhihu = site_spec("zhihu").expect("zhihu");
        let config = SiteConfig::default();
        assert_eq!(
            site_keywords(zhihu, &config),
            ZHIHU_KEYWORDS
                .iter()
                .map(|entry| entry.to_string())
                .collect::<Vec<String>>()
        );
        let saved = SiteConfig {
            keywords: vec!["自建清单".to_string()],
            ..SiteConfig::default()
        };
        assert_eq!(site_keywords(zhihu, &saved), vec!["自建清单"]);
    }

    #[test]
    fn a_pinned_page_wins_and_a_watchlist_stays_a_search() {
        let zhihu = site_spec("zhihu").expect("zhihu");
        let config = SiteConfig::default();
        let urls = sweep_urls(zhihu, &config, 0);
        assert_eq!(urls.len(), DEFAULT_MAX_URLS as usize);
        assert!(urls[0].starts_with("https://www.zhihu.com/search?type=content&q="));
        assert_eq!(decode_query(&urls[0]), ZHIHU_KEYWORDS[0]);
        assert_eq!(urls[0].matches("%20").count(), 1, "spaces are encoded");

        let pinned = SiteConfig {
            seed_url: "https://www.zhihu.com/hot".to_string(),
            ..SiteConfig::default()
        };
        assert_eq!(
            sweep_urls(zhihu, &pinned, 0),
            vec!["https://www.zhihu.com/hot"]
        );

        let rotated = sweep_urls(zhihu, &config, 1);
        assert_eq!(decode_query(&rotated[0]), ZHIHU_KEYWORDS[1]);
        assert_eq!(sweep_urls(zhihu, &config, ZHIHU_KEYWORDS.len())[0], urls[0]);

        let wide = SiteConfig {
            max_urls: Some(json!(3)),
            ..SiteConfig::default()
        };
        assert_eq!(sweep_urls(zhihu, &wide, 0).len(), 3);

        // The Channels console has one fixed content-list route and no
        // keyword search or detail pages.
        let channels = site_spec("wechatChannels").expect("wechatChannels");
        assert_eq!(
            sweep_urls(channels, &SiteConfig::default(), 0),
            vec!["https://channels.weixin.qq.com/platform/post/list"]
        );

        let juejin = site_spec("juejin").expect("juejin");
        let juejin_urls = sweep_urls(juejin, &SiteConfig::default(), 0);
        assert!(juejin_urls[0].starts_with("https://juejin.cn/search?query="));

        let bilibili = site_spec("bilibili").expect("bilibili");
        let bilibili_urls = sweep_urls(bilibili, &SiteConfig::default(), 0);
        assert!(bilibili_urls[0].starts_with("https://search.bilibili.com/all?keyword="));

        // The MP backend has no keyword search: an empty configuration is a
        // gap the console must report, and a pinned list URL is the only input.
        let wechat = site_spec("wechatMp").expect("wechatMp");
        assert!(sweep_urls(wechat, &SiteConfig::default(), 0).is_empty());
        let pinned_wechat = SiteConfig {
            seed_url: "https://mp.weixin.qq.com/cgi-bin/appmsg?t=media/appmsg_edit&action=edit&type=10&isMul=1".to_string(),
            ..SiteConfig::default()
        };
        assert_eq!(
            sweep_urls(wechat, &pinned_wechat, 0),
            vec![pinned_wechat.seed_url()]
        );
    }

    fn decode_query(raw: &str) -> String {
        let encoded = raw.split("q=").nth(1).unwrap_or("");
        let bytes = encoded.as_bytes();
        let mut out: Vec<u8> = Vec::new();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'%' && index + 2 < bytes.len() {
                if let Ok(hex) = u8::from_str_radix(&encoded[index + 1..index + 3], 16) {
                    out.push(hex);
                    index += 3;
                    continue;
                }
            }
            out.push(bytes[index]);
            index += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    #[test]
    fn only_a_same_site_https_question_is_a_detail_target() {
        let zhihu = "zhihu";
        assert!(is_detail_url(zhihu, "https://www.zhihu.com/question/12345"));
        assert!(is_detail_url(zhihu, "https://zhihu.com/question/12345"));
        assert!(is_detail_url(
            zhihu,
            "https://www.zhihu.com/question/1/answer/2"
        ));
        assert!(!is_detail_url(zhihu, "http://www.zhihu.com/question/1"));
        assert!(!is_detail_url(zhihu, "https://evil-zhihu.com/question/1"));
        assert!(!is_detail_url(
            zhihu,
            "https://zhihu.com.evil.example/question/1"
        ));
        assert!(!is_detail_url(zhihu, "file:///etc/passwd"));
        assert!(!is_detail_url(zhihu, "javascript:alert(1)"));
        assert!(!is_detail_url(zhihu, "not a url"));
        assert!(!is_detail_url(zhihu, "https://www.zhihu.com/hot"));
        assert!(!is_detail_url(zhihu, "https://www.zhihu.com/p/12345"));
        assert!(!is_detail_url("juejin", "https://www.zhihu.com/question/1"));

        assert!(is_detail_url(
            "juejin",
            "https://juejin.cn/post/7123456789012345678"
        ));
        assert!(!is_detail_url("juejin", "https://juejin.cn/post/"));
        assert!(!is_detail_url("juejin", "https://juejin.cn/user/123"));
        assert!(!is_detail_url(
            "juejin",
            "https://juejin.cn.evil.test/post/1"
        ));

        assert!(is_detail_url(
            "bilibili",
            "https://www.bilibili.com/video/BV1xx411c7mD"
        ));
        assert!(is_detail_url(
            "bilibili",
            "https://www.bilibili.com/video/av170001?p=2"
        ));
        assert!(!is_detail_url(
            "bilibili",
            "https://www.bilibili.com/video/"
        ));
        assert!(!is_detail_url("bilibili", "https://space.bilibili.com/123"));
        assert!(!is_detail_url(
            "bilibili",
            "https://bilibili.com.evil.test/video/BV1xx411c7mD"
        ));

        assert!(is_detail_url(
            "wechatMp",
            "https://mp.weixin.qq.com/s/AbCdEfGh"
        ));
        assert!(is_detail_url(
            "wechatMp",
            "https://mp.weixin.qq.com/s?__biz=MzA&mid=1&idx=1&sn=abc"
        ));
        assert!(!is_detail_url(
            "wechatMp",
            "https://mp.weixin.qq.com/cgi-bin/appmsg?t=media/appmsg_edit"
        ));
        assert!(!is_detail_url(
            "wechatMp",
            "https://mp.weixin.qq.com.evil.test/s/abc"
        ));

        // The Channels console is list-only: its rows are not addressable pages.
        assert!(!is_detail_url(
            "wechatChannels",
            "https://channels.weixin.qq.com/platform/post/list"
        ));
        assert_eq!(
            canonical_detail_url(
                "wechatChannels",
                "https://channels.weixin.qq.com/platform/post/list"
            ),
            None
        );
    }

    #[test]
    fn network_site_detail_addresses_are_canonical() {
        assert_eq!(
            canonical_detail_url("juejin", "https://juejin.cn/post/7123456789012345678?foo=1"),
            Some("https://juejin.cn/post/7123456789012345678".to_string())
        );
        assert_eq!(
            canonical_detail_url(
                "bilibili",
                "https://www.bilibili.com/video/BV1xx411c7mD?p=2&spm=abc"
            ),
            Some("https://www.bilibili.com/video/BV1xx411c7mD".to_string())
        );
        assert_eq!(
            canonical_detail_url("wechatMp", "https://mp.weixin.qq.com/s/AbCdEfGh?scene=1"),
            Some("https://mp.weixin.qq.com/s/AbCdEfGh".to_string())
        );
        let query_form = canonical_detail_url(
            "wechatMp",
            "https://mp.weixin.qq.com/s?__biz=MzA&mid=1&idx=1&sn=abc&chksm=zzz#rd",
        )
        .expect("query-form MP article");
        assert!(query_form.starts_with("https://mp.weixin.qq.com/s?"));
        assert!(query_form.contains("__biz=MzA"));
        assert!(query_form.contains("mid=1"));
        assert!(query_form.contains("sn=abc"));
        assert!(!query_form.contains("chksm"));
        assert_eq!(
            canonical_detail_url("wechatMp", "https://mp.weixin.qq.com/cgi-bin/home"),
            None
        );
    }

    #[test]
    fn detail_targets_keep_the_pages_order_once_each_and_stop_at_the_cap() {
        let list = vec![
            json!({ "url": "https://www.zhihu.com/question/1" }),
            json!({ "url": "https://www.zhihu.com/hot" }),
            json!({ "url": "https://www.zhihu.com/question/2#answer-9" }),
            json!({ "url": "https://www.zhihu.com/question/1#answer-3" }),
            json!({ "url": "https://www.zhihu.com/question/3" }),
            json!({ "url": "https://www.zhihu.com/question/4" }),
            json!({ "url": "https://www.zhihu.com/question/5" }),
            json!({ "url": "https://www.zhihu.com/question/6" }),
        ];
        assert_eq!(
            detail_targets("zhihu", &list, DEFAULT_DETAIL_PAGES),
            vec![
                "https://www.zhihu.com/question/1",
                "https://www.zhihu.com/question/2",
                "https://www.zhihu.com/question/3",
            ]
        );
        assert_eq!(detail_targets("zhihu", &list, 99).len(), MAX_DETAIL_PAGES);
        assert_eq!(detail_targets("zhihu", &list, 1).len(), 1);
        assert!(detail_targets("zhihu", &list, 0).is_empty());
    }

    #[test]
    fn site_settings_survive_a_hostile_payload() {
        let dir =
            std::env::temp_dir().join(format!("pi-staff-sites-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(read_settings(&dir).is_empty());

        std::fs::write(settings_path(&dir), "{ not json").unwrap();
        assert!(read_settings(&dir).is_empty());
        std::fs::write(settings_path(&dir), "[1,2,3]").unwrap();
        assert!(read_settings(&dir).is_empty());

        let mut settings = SiteSettings::new();
        settings.insert(
            "zhihu".to_string(),
            SiteConfig {
                seed_url: "https://www.zhihu.com/hot".to_string(),
                max_urls: Some(json!(1)),
                keywords: vec!["自建清单".to_string()],
            },
        );
        write_settings(&dir, &settings).unwrap();
        let read = read_settings(&dir);
        assert_eq!(read["zhihu"].seed_url(), "https://www.zhihu.com/hot");
        assert_eq!(read["zhihu"].max_urls(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_collected_page_becomes_bounded_prompt_bullets() {
        let page = json!({
            "channel": "zhihu",
            "url": "https://www.zhihu.com/hot",
            "title": "知乎热榜",
            "itemCount": 2,
            "items": [
                { "title": "一", "url": "https://www.zhihu.com/question/1" },
                { "title": "二", "url": "https://www.zhihu.com/question/2" },
            ],
        });
        let note = format_note("zhihu", &page, PAGE_CHARS);
        assert!(note.contains("- [collected:zhihu] 知乎热榜 — https://www.zhihu.com/hot"));
        assert!(note.contains("  - 一 — https://www.zhihu.com/question/1"));
        assert!(note.contains("  - 二 — https://www.zhihu.com/question/2"));

        let long = json!({
            "channel": "zhihu",
            "url": "https://www.zhihu.com/hot",
            "title": "知乎热榜",
            "items": [
                { "title": "x".repeat(200), "url": "https://a" },
                { "title": "y", "url": "https://b" },
            ],
        });
        let bounded = format_note("zhihu", &long, 120);
        assert!(bounded.contains("项因长度省略"));
        assert!(bounded.chars().count() <= 120);

        let failed = json!({
            "channel": "zhihu",
            "url": "https://www.zhihu.com/hot",
            "title": "",
            "items": [],
            "error": "no signed-in session for this site",
        });
        let note = format_note("zhihu", &failed, PAGE_CHARS);
        assert!(note.contains("no signed-in session"));

        let empty = json!({
            "channel": "zhihu",
            "url": "https://www.zhihu.com/hot",
            "title": "知乎",
            "items": [],
        });
        assert!(format_note("zhihu", &empty, PAGE_CHARS).contains("没有读到列表项"));
    }

    #[test]
    fn a_detail_page_reaches_the_prompt_with_answers_and_comments() {
        let page = json!({
            "channel": "zhihu",
            "url": "https://www.zhihu.com/question/1",
            "title": "怎样做一人公司?",
            "summary": "12 个回答 / 想找一套能自己跑起来的系统",
            "items": [
                {
                    "kind": "answer",
                    "title": "张三",
                    "url": "https://www.zhihu.com/question/1",
                    "meta": "赞同 82 · 2026-09-18",
                    "excerpt": "我用一套定时任务顶了三个人的活",
                },
                { "kind": "comment", "title": "李四", "url": "", "excerpt": "有联系方式吗" },
            ],
        });
        let note = format_note("zhihu", &page, PAGE_CHARS);
        assert!(
            note.contains("- [collected:zhihu] 怎样做一人公司? — https://www.zhihu.com/question/1")
        );
        assert!(note.contains("  正文：12 个回答"));
        assert!(note.contains("  - 回答｜张三 — https://www.zhihu.com/question/1"));
        assert!(note.contains("    赞同 82 · 2026-09-18"));
        assert!(note.contains("    我用一套定时任务顶了三个人的活"));
        assert!(note.contains("  - 评论｜李四"));
        assert!(note.contains("    有联系方式吗"));
    }

    #[test]
    fn the_injected_page_scripts_are_valid_javascript() {
        for channel in ["zhihu", "juejin", "bilibili", "wechatMp", "wechatChannels"] {
            for script in [list_script(channel), detail_script(channel)] {
                assert!(script.contains("querySelectorAll"), "{channel}");
                assert!(script.contains("location.pathname"), "{channel}");
                assert!(!script.contains("fetch("), "{channel}");
                assert!(!script.contains("XMLHttpRequest"), "{channel}");
            }
        }
        let zhihu_detail = detail_script("zhihu");
        assert!(zhihu_detail.contains("Comment"));
        assert!(zhihu_detail.contains("条评论"));
        assert!(zhihu_detail.contains(".click()"));

        let juejin_detail = detail_script("juejin");
        assert!(juejin_detail.contains(".comment-item"));
        assert!(juejin_detail.contains(".article-content"));

        let bilibili_detail = detail_script("bilibili");
        assert!(bilibili_detail.contains(".video-title"));
        assert!(bilibili_detail.contains(".desc-info-text"));

        let wechat_detail = detail_script("wechatMp");
        assert!(wechat_detail.contains("#activity-name"));
        assert!(wechat_detail.contains("#js_content"));

        let channels_list = list_script("wechatChannels");
        assert!(channels_list.contains(".post-list-body"));
        assert!(channels_list.contains("shadowRoot"));
        assert!(channels_list.contains(".none-post"));
    }
}
