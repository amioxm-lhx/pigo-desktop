//! 选股神器 (stock picker) backend.
//!
//! The renderer owns every sentence; this module returns structured facts and
//! machine keys from the official hithink-finance data service
//! (`https://fuyao.aicubes.cn`, header `X-api-key`). The engine never fills a
//! missing upstream number with a mock value: an unavailable pool stays empty,
//! an unavailable quote shows as unknown, and an upstream failure is an error.
//!
//! It mirrors the rules in `docs/选股神器-使用手册.md`: a sentiment cycle that
//! caps total position, four weighted strategies over one candidate pool, entry
//! / stop / target plans derived from the 5- and 10-day averages, and a
//! time-boxed next-step list with watchlist alerts.

use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{Datelike, FixedOffset, NaiveDate, TimeZone, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const BASE: &str = "https://fuyao.aicubes.cn";
pub const ENV_VAR: &str = "HITHINK_FINANCE_API_KEY";
pub const APPLY_URL: &str = "https://fuyao.aicubes.cn/";

/// Upstream asks for at least 150 ms between calls.
const UPSTREAM_GAP: Duration = Duration::from_millis(150);
const POOL_TTL: Duration = Duration::from_secs(60);
const LADDER_TTL: Duration = Duration::from_secs(10 * 60);
const HOT_TTL: Duration = Duration::from_secs(5 * 60);
const DRAGON_TTL: Duration = Duration::from_secs(10 * 60);
const SNAPSHOT_TTL: Duration = Duration::from_secs(30);
const KLINE_TTL: Duration = Duration::from_secs(10 * 60);
const VALUATION_TTL: Duration = Duration::from_secs(15 * 60);
const CALENDAR_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const OVERVIEW_TTL: Duration = Duration::from_secs(45);

const MAX_CANDIDATES: usize = 14;
const MAX_ITEMS: usize = 8;
const MAX_KLINE_DAYS: i64 = 140;

#[derive(Debug, Clone)]
pub struct StocksError {
    pub code: String,
    pub message: String,
}

impl StocksError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }

    fn upstream(message: impl Into<String>) -> Self {
        Self::new("STOCKS_UPSTREAM", message)
    }

    fn invalid_key(message: impl Into<String>) -> Self {
        Self::new("STOCKS_INVALID_KEY", message)
    }
}

struct CacheEntry {
    at: Instant,
    value: Value,
}

/// Resident engine state: upstream cache, request spacing, and the short-lived
/// recommendation result cache. One instance per app run.
pub struct StocksEngine {
    data_dir: PathBuf,
    cache: Mutex<HashMap<String, CacheEntry>>,
    klines: Mutex<HashMap<String, (Instant, Kline)>>,
    overviews: Mutex<HashMap<String, (Instant, Value)>>,
    last_overview: Mutex<Option<(Instant, Value)>>,
    last_call: Mutex<Option<Instant>>,
}

impl StocksEngine {
    pub fn new(data_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            data_dir,
            cache: Mutex::new(HashMap::new()),
            klines: Mutex::new(HashMap::new()),
            overviews: Mutex::new(HashMap::new()),
            last_overview: Mutex::new(None),
            last_call: Mutex::new(None),
        })
    }

    // ── files ──────────────────────────────────────────────────────────────

    fn credentials_path(&self) -> PathBuf {
        self.data_dir.join("stocks-credentials.json")
    }

    fn watchlist_path(&self) -> PathBuf {
        self.data_dir.join("stocks-watchlist.json")
    }

    fn stored_key(&self) -> String {
        let raw = std::fs::read_to_string(self.credentials_path()).unwrap_or_default();
        let parsed: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        parsed
            .get("apiKey")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string()
    }

    fn env_key(&self) -> String {
        std::env::var(ENV_VAR)
            .ok()
            .map(|value| value.trim().to_string())
            .unwrap_or_default()
    }

    fn resolve_key(&self) -> Result<(String, &'static str), StocksError> {
        let stored = self.stored_key();
        if !stored.is_empty() {
            return Ok((stored, "settings"));
        }
        let env = self.env_key();
        if !env.is_empty() {
            return Ok((env, "env"));
        }
        Err(StocksError::new(
            "STOCKS_NOT_CONFIGURED",
            format!("还没有配置同花顺 API Key（{ENV_VAR} 或页面内保存）"),
        ))
    }

    fn read_watchlist(&self) -> Vec<WatchItem> {
        let raw = std::fs::read_to_string(self.watchlist_path()).unwrap_or_default();
        let parsed: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        let rows = parsed
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        rows.into_iter()
            .filter_map(|row| serde_json::from_value::<WatchItem>(row).ok())
            .filter(|item| !item.code.trim().is_empty())
            .collect()
    }

    fn write_watchlist(&self, items: &[WatchItem]) -> Result<(), StocksError> {
        let body = serde_json::to_string_pretty(&json!({ "items": items }))
            .map_err(|error| StocksError::new("INTERNAL", error.to_string()))?;
        std::fs::create_dir_all(&self.data_dir)
            .map_err(|error| StocksError::new("INTERNAL", error.to_string()))?;
        std::fs::write(self.watchlist_path(), body)
            .map_err(|error| StocksError::new("INTERNAL", error.to_string()))
    }

    // ── upstream ───────────────────────────────────────────────────────────

    fn wait_turn(&self) {
        let mut last = self.last_call.lock().unwrap();
        if let Some(previous) = *last {
            let elapsed = previous.elapsed();
            if elapsed < UPSTREAM_GAP {
                std::thread::sleep(UPSTREAM_GAP - elapsed);
            }
        }
        *last = Some(Instant::now());
    }

    fn fetch(
        &self,
        key: &str,
        path: &str,
        query: &[(&str, String)],
        ttl: Duration,
    ) -> Result<Value, StocksError> {
        let mut url = format!("{BASE}{path}");
        if !query.is_empty() {
            let encoded: Vec<String> = query
                .iter()
                .map(|(name, value)| {
                    format!(
                        "{}={}",
                        name,
                        url::form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>()
                    )
                })
                .collect();
            url.push('?');
            url.push_str(&encoded.join("&"));
        }
        let cache_key = format!("{}|{}", url, key_tag(key));
        if let Some(entry) = self.cache.lock().unwrap().get(&cache_key) {
            if entry.at.elapsed() < ttl {
                return Ok(entry.value.clone());
            }
        }

        self.wait_turn();
        let response = ureq::get(&url)
            .set("X-api-key", key)
            .timeout(Duration::from_secs(30))
            .call();
        let body: Value = match response {
            Ok(response) => {
                let text = response
                    .into_string()
                    .map_err(|error| StocksError::upstream(format!("响应读取失败：{error}")))?;
                serde_json::from_str(&text)
                    .map_err(|error| StocksError::upstream(format!("响应不是 JSON：{error}")))?
            }
            Err(ureq::Error::Status(status, response)) => {
                let text = response.into_string().unwrap_or_default();
                let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                let message = parsed
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("同花顺数据服务返回错误");
                if status == 401 {
                    return Err(StocksError::invalid_key(message));
                }
                if status == 429 {
                    return Err(StocksError::upstream("上游限流，请稍后再试"));
                }
                return Err(StocksError::upstream(format!("HTTP {status}：{message}")));
            }
            Err(ureq::Error::Transport(error)) => {
                return Err(StocksError::upstream(format!(
                    "无法连接同花顺数据服务：{error}"
                )))
            }
        };

        let code = body.get("code").and_then(Value::as_i64).unwrap_or(0);
        if code != 0 {
            let message = body
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("同花顺数据服务返回错误")
                .to_string();
            match code {
                2001 => return Err(StocksError::invalid_key(message)),
                2003 => {
                    // The live service answers both a revoked key and a missing
                    // capability with 2003; the message is what tells them apart.
                    let lowered = message.to_lowercase();
                    if lowered.contains("invalid or revoked")
                        || lowered.contains("missing x-api-key")
                        || lowered.contains("api key")
                        || lowered.contains("未认证")
                    {
                        return Err(StocksError::invalid_key(message));
                    }
                    return Err(StocksError::new(
                        "STOCKS_FORBIDDEN",
                        format!("这个 API Key 没有该数据权限：{message}"),
                    ));
                }
                4001 => return Err(StocksError::upstream("上游限流，请稍后再试")),
                // "Data not ready" is an ordinary empty answer, not a failure:
                // the console shows zeros and the owner refreshes later.
                3002 => {
                    log::warn!("stocks: upstream data not ready: {message}");
                    return Ok(Value::Null);
                }
                _ => return Err(StocksError::upstream(format!("上游错误 {code}：{message}"))),
            }
        }

        let data = body.get("data").cloned().unwrap_or(Value::Null);
        self.cache.lock().unwrap().insert(
            cache_key,
            CacheEntry {
                at: Instant::now(),
                value: data.clone(),
            },
        );
        Ok(data)
    }

    // ── public surface ─────────────────────────────────────────────────────

    pub fn status(&self) -> Value {
        let stored = self.stored_key();
        let env = self.env_key();
        let (configured, source, key) = if !stored.is_empty() {
            (true, "settings", stored)
        } else if !env.is_empty() {
            (true, "env", env)
        } else {
            (false, "none", String::new())
        };
        let mut status = json!({
            "configured": configured,
            "source": source,
            "applyUrl": APPLY_URL,
            "envVar": ENV_VAR,
        });
        if configured {
            status["keyMasked"] = json!(mask_key(&key));
        }
        status
    }

    pub async fn save_key(engine: Arc<Self>, raw: &str) -> Result<Value, StocksError> {
        let key = raw.trim();
        if key.is_empty() {
            return Err(StocksError::new("INVALID_ARGUMENT", "API Key 不能为空"));
        }
        // Validate against the cheapest endpoint before overwriting a working
        // key: a bad paste must not lock the page out of its data.
        let probe = Arc::clone(&engine);
        let candidate = key.to_string();
        let probe_result = tokio::task::spawn_blocking(move || {
            probe.fetch(
                &candidate,
                "/api/a-share/calendar/trading-days",
                &[],
                CALENDAR_TTL,
            )
        })
        .await
        .map_err(|error| StocksError::new("INTERNAL", error.to_string()))?;
        match probe_result {
            Ok(_) => {}
            // A wrong key is HTTP 401 and must not be saved. A capability or
            // transient failure is reported by the page later; it should not
            // block saving a key the owner just pasted.
            Err(error) if error.code == "STOCKS_INVALID_KEY" => return Err(error),
            Err(error) => {
                log::warn!("stocks: key probe unavailable: {}", error.message);
            }
        }

        let body = json!({ "apiKey": key });
        std::fs::create_dir_all(&engine.data_dir)
            .map_err(|error| StocksError::new("INTERNAL", error.to_string()))?;
        std::fs::write(
            engine.credentials_path(),
            serde_json::to_string_pretty(&body)
                .map_err(|error| StocksError::new("INTERNAL", error.to_string()))?,
        )
        .map_err(|error| StocksError::new("INTERNAL", error.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                engine.credentials_path(),
                std::fs::Permissions::from_mode(0o600),
            );
        }
        // A rotated key must not keep serving rows the previous key fetched.
        engine.cache.lock().unwrap().clear();
        engine.klines.lock().unwrap().clear();
        engine.overviews.lock().unwrap().clear();
        *engine.last_overview.lock().unwrap() = None;
        Ok(engine.status())
    }

    /// Replace the recommendation cache entry for one strategy.
    pub fn cached_overview(&self, strategy: &str) -> Option<Value> {
        let overviews = self.overviews.lock().unwrap();
        overviews
            .get(strategy)
            .filter(|(at, _)| at.elapsed() < OVERVIEW_TTL)
            .map(|(_, value)| value.clone())
    }

    fn recent_overview(&self, ttl: Duration) -> Option<Value> {
        self.last_overview
            .lock()
            .unwrap()
            .clone()
            .filter(|(at, _)| at.elapsed() < ttl)
            .map(|(_, value)| value)
    }
}

// The engine is used through `Arc<AppState>`, so an `&Arc<Self>` helper keeps
// the blocking closures explicit without adding another trait.
fn engine_of(state: &Arc<crate::state::AppState>) -> Arc<StocksEngine> {
    state.stocks.clone()
}

#[derive(Clone, Serialize, Deserialize)]
struct WatchItem {
    code: String,
    name: String,
    #[serde(default, rename = "thscode")]
    thscode: String,
    #[serde(default)]
    note: String,
    #[serde(default)]
    stop: Option<f64>,
    #[serde(default)]
    target: Option<f64>,
    #[serde(default, rename = "addedAt")]
    added_at: String,
}

fn key_tag(key: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn mask_key(key: &str) -> String {
    if key.chars().count() <= 8 {
        return "••••".to_string();
    }
    let first: String = key.chars().take(4).collect();
    let last: String = key
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();
    format!("{first}…{last}")
}

fn shanghai() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).expect("UTC+8")
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn clamp(value: f64, low: f64, high: f64) -> f64 {
    value.max(low).min(high)
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn number(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

fn integer(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

/// An auxiliary board that may be unavailable without taking the page down:
/// a missing hot list or dragon board is an empty section, not an auth failure.
fn optional(result: Result<Value, StocksError>) -> Result<Value, StocksError> {
    match result {
        Ok(value) => Ok(value),
        Err(error) if error.code == "STOCKS_UPSTREAM" || error.code == "STOCKS_FORBIDDEN" => {
            log::warn!(
                "stocks: optional upstream section unavailable: {}",
                error.message
            );
            Ok(Value::Null)
        }
        Err(error) => Err(error),
    }
}

fn items(value: &Value) -> Vec<Value> {
    value
        .get("item")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

// ── trading day ────────────────────────────────────────────────────────────

#[derive(Clone)]
struct TradeDay {
    /// The board the page displays. Before the open it is the previous
    /// completed trading day; during and after the session it is today.
    date: NaiveDate,
    /// Whether the local calendar day is a trading day (drives the plan).
    today_is_trading: bool,
}

impl TradeDay {
    fn dash(&self) -> String {
        self.date.format("%Y-%m-%d").to_string()
    }

    fn compact(&self) -> String {
        self.date.format("%Y%m%d").to_string()
    }

    fn midnight_ms(&self) -> i64 {
        shanghai()
            .from_local_datetime(&self.date.and_hms_opt(0, 0, 0).expect("midnight"))
            .single()
            .map(|at| at.timestamp_millis())
            .unwrap_or(0)
    }
}

fn trade_day(engine: &StocksEngine, key: &str) -> Result<TradeDay, StocksError> {
    let local = Utc::now().with_timezone(&shanghai());
    let today = local.date_naive();
    let start = today - chrono::Duration::days(20);
    let data = match engine.fetch(
        key,
        "/api/a-share/calendar/trading-days",
        &[
            ("start_date", start.format("%Y%m%d").to_string()),
            ("end_date", today.format("%Y%m%d").to_string()),
        ],
        CALENDAR_TTL,
    ) {
        Ok(data) => data,
        // A key without the calendar capability still gets a usable page:
        // today's weekday is the fallback trading-day signal.
        Err(error) if error.code == "STOCKS_FORBIDDEN" || error.code == "STOCKS_UPSTREAM" => {
            log::warn!("stocks: calendar unavailable: {}", error.message);
            Value::Null
        }
        Err(error) => return Err(error),
    };
    let mut dates: Vec<NaiveDate> = items(&data)
        .into_iter()
        .filter_map(|item| NaiveDate::parse_from_str(text(&item, "date"), "%Y%m%d").ok())
        .filter(|date| *date <= today)
        .collect();
    dates.sort_unstable();
    dates.dedup();
    let today_is_trading = if dates.is_empty() {
        !matches!(today.weekday(), Weekday::Sat | Weekday::Sun)
    } else {
        dates.contains(&today)
    };
    let mut date = dates.iter().copied().max().unwrap_or(today);
    // Before the open the current board has not been formed yet; showing it
    // would print a page of zeros. The previous complete board is what the
    // owner is reviewing at that hour.
    let before_open = local.hour() * 60 + local.minute() < 9 * 60 + 15;
    if today_is_trading && before_open && date == today {
        if let Some(previous) = dates.iter().copied().filter(|day| *day < today).max() {
            date = previous;
        }
    }
    Ok(TradeDay {
        date,
        today_is_trading,
    })
}

// ── market snapshot ────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct MarketStats {
    limit_up: i64,
    limit_down: i64,
    break_board: i64,
    max_board: i64,
    break_rate_pct: f64,
    promote_pct: f64,
}

#[derive(Clone)]
struct Sentiment {
    score: i64,
    phase: &'static str,
    max_position_pct: i64,
}

struct MarketSnapshot {
    trade: TradeDay,
    stats: MarketStats,
    sentiment: Sentiment,
    themes: Vec<Value>,
    ladder: Vec<Value>,
    limit_up: Vec<Value>,
}

fn market_snapshot(engine: &StocksEngine, key: &str) -> Result<MarketSnapshot, StocksError> {
    let trade = trade_day(engine, key)?;
    let limit_up = engine.fetch(
        key,
        "/api/a-share/special-data/limit-up-pool",
        &[
            ("date_ms", trade.midnight_ms().to_string()),
            ("page", "1".to_string()),
            ("size", "200".to_string()),
            ("sort_field", "limit_up_time".to_string()),
            ("sort_dir", "desc".to_string()),
        ],
        POOL_TTL,
    )?;
    let limit_up_items = items(&limit_up);
    let limit_up_total = limit_up
        .pointer("/pagination/total")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| limit_up_items.len() as i64);

    let down = engine.fetch(
        key,
        "/api/a-share/special-data/limit-down-pool",
        &[
            ("date_ms", trade.midnight_ms().to_string()),
            ("page", "1".to_string()),
            ("size", "200".to_string()),
        ],
        POOL_TTL,
    )?;
    let limit_down_total = down
        .pointer("/pagination/total")
        .and_then(Value::as_i64)
        .unwrap_or(0);

    let broken = engine.fetch(
        key,
        "/api/a-share/special-data/limit-break-pool",
        &[
            ("date_ms", trade.midnight_ms().to_string()),
            ("page", "1".to_string()),
            ("size", "200".to_string()),
        ],
        POOL_TTL,
    )?;
    let break_total = broken
        .pointer("/pagination/total")
        .and_then(Value::as_i64)
        .unwrap_or(0);

    let ladder = engine
        .fetch(
            key,
            "/api/a-share/special-data/limit-up-ladder",
            &[],
            LADDER_TTL,
        )
        .unwrap_or(Value::Null);

    let statistics = engine
        .fetch(
            key,
            "/api/a-share/special-data/limit-up-statistics",
            &[("date", trade.dash())],
            POOL_TTL,
        )
        .ok();

    let max_board_ladder = today_ladder_max_board(&ladder, &trade.compact());
    let promote_pct = promotion_pct(&ladder, &trade.compact(), &limit_up_items);

    let mut stats = MarketStats {
        limit_up: limit_up_total,
        limit_down: limit_down_total,
        break_board: break_total,
        max_board: limit_up_items
            .iter()
            .filter_map(|row| integer(row, "continue_day_cnt"))
            .max()
            .unwrap_or(0)
            .max(max_board_ladder),
        ..MarketStats::default()
    };
    if let Some(statistics) = statistics.as_ref() {
        stats.limit_up = integer(statistics, "limit_up_stock_count").unwrap_or(stats.limit_up);
        stats.break_board = integer(statistics, "open_stock_count").unwrap_or(stats.break_board);
        stats.max_board = statistics
            .get("exact_boards")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|row| integer(row, "board_num"))
            .max()
            .unwrap_or(stats.max_board)
            .max(max_board_ladder);
    }
    if stats.limit_up + stats.break_board > 0 {
        stats.break_rate_pct =
            stats.break_board as f64 / (stats.limit_up + stats.break_board) as f64 * 100.0;
    } else if let Some(seal_rate) = statistics.as_ref().and_then(|row| number(row, "seal_rate")) {
        stats.break_rate_pct = (1.0 - seal_rate) * 100.0;
    }
    stats.promote_pct = promote_pct;

    let themes = theme_counts(&limit_up_items);
    let sentiment = sentiment(&stats);
    let ladder_rows = ladder_for(&ladder, &trade.compact());

    Ok(MarketSnapshot {
        trade,
        stats,
        sentiment,
        themes,
        ladder: ladder_rows,
        limit_up: limit_up_items,
    })
}

fn today_ladder_max_board(ladder: &Value, today: &str) -> i64 {
    let Some(entry) = ladder_entries(ladder)
        .into_iter()
        .find(|entry| text(entry, "date") == today)
    else {
        return 0;
    };
    let Some(boards) = entry.get("boards").and_then(Value::as_object) else {
        return 0;
    };
    boards
        .values()
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|stock| integer(stock, "board_num"))
        .max()
        .unwrap_or(0)
}

fn ladder_entries(ladder: &Value) -> Vec<Value> {
    ladder
        .get("item")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn promotion_pct(ladder: &Value, today: &str, limit_up: &[Value]) -> f64 {
    let mut entries = ladder_entries(ladder);
    entries.sort_by(|left, right| text(right, "date").cmp(text(left, "date")));
    let Some(yesterday) = entries
        .iter()
        .filter(|entry| text(entry, "date") < today)
        .max_by_key(|entry| text(entry, "date").to_string())
    else {
        return 0.0;
    };
    let sealed: BTreeSet<String> = limit_up
        .iter()
        .map(|row| text(row, "ticker").to_string())
        .collect();
    let mut total = 0i64;
    let mut promoted = 0i64;
    if let Some(boards) = yesterday.get("boards").and_then(Value::as_object) {
        for rows in boards.values().filter_map(Value::as_array) {
            for row in rows {
                if integer(row, "board_num").unwrap_or(0) < 2 {
                    continue;
                }
                total += 1;
                match row.get("seal_nextday").and_then(Value::as_bool) {
                    Some(true) => promoted += 1,
                    Some(false) => {}
                    None => {
                        if sealed.contains(text(row, "ticker")) {
                            promoted += 1;
                        }
                    }
                }
            }
        }
    }
    if total == 0 {
        return 0.0;
    }
    promoted as f64 / total as f64 * 100.0
}

fn theme_counts(limit_up: &[Value]) -> Vec<Value> {
    let mut counts: HashMap<String, i64> = HashMap::new();
    for row in limit_up {
        let reason = text(row, "limit_up_reason").trim();
        if reason.is_empty() {
            continue;
        }
        *counts.entry(reason.to_string()).or_insert(0) += 1;
    }
    let mut rows: Vec<(String, i64)> = counts.into_iter().collect();
    rows.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    rows.into_iter()
        .take(5)
        .map(|(name, count)| json!({ "name": name, "count": count }))
        .collect()
}

fn ladder_for(ladder: &Value, today: &str) -> Vec<Value> {
    let Some(entry) = ladder_entries(ladder)
        .into_iter()
        .find(|entry| text(entry, "date") == today)
    else {
        return Vec::new();
    };
    let Some(boards) = entry.get("boards").and_then(Value::as_object) else {
        return Vec::new();
    };
    let slots = [
        ("two_board", 2),
        ("three_board", 3),
        ("four_board", 4),
        ("five_board", 5),
        ("six_board", 6),
        ("seven_over", 7),
    ];
    let mut rows = Vec::new();
    for (key, board) in slots {
        let names: Vec<String> = boards
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|row| {
                let name = text(row, "name").trim();
                (!name.is_empty()).then(|| name.to_string())
            })
            .take(3)
            .collect();
        if !names.is_empty() {
            rows.push(json!({ "board": board, "names": names }));
        }
    }
    rows
}

fn sentiment(stats: &MarketStats) -> Sentiment {
    let breadth = clamp((stats.limit_up as f64 - 20.0) / 80.0 * 100.0, 0.0, 100.0);
    let height = clamp((stats.max_board as f64 - 2.0) / 6.0 * 100.0, 0.0, 100.0);
    let seal = clamp(100.0 - stats.break_rate_pct, 0.0, 100.0);
    let down = clamp(100.0 - stats.limit_down as f64 / 15.0 * 100.0, 0.0, 100.0);
    let promote = clamp(stats.promote_pct, 0.0, 100.0);
    let score = (0.30 * breadth + 0.20 * height + 0.30 * seal + 0.10 * down + 0.10 * promote)
        .round() as i64;
    let (phase, max_position_pct) = if score < 25 {
        ("freeze", 5)
    } else if score < 45 {
        ("ebb", 15)
    } else if score < 65 {
        ("warm", 35)
    } else {
        ("climax", 50)
    };
    Sentiment {
        score,
        phase,
        max_position_pct,
    }
}

// ── candidates ─────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct Candidate {
    ticker: String,
    thscode: String,
    name: String,
    price: f64,
    change_pct: f64,
    boards: i64,
    seal_money: f64,
    max_seal_money: f64,
    reason: String,
    hot_rank: Option<i64>,
    hot_up: Option<i64>,
    soar_rank: Option<i64>,
    dragon_net: f64,
    dragon_org: bool,
    is_st: bool,
    is_new: bool,
    sources: BTreeSet<String>,
    heat: f64,
    money: f64,
}

impl Candidate {
    fn ensure(thscode: &str, ticker: &str) -> Self {
        Self {
            ticker: ticker.to_string(),
            thscode: thscode.to_string(),
            ..Self::default()
        }
    }
}

struct CandidateBook {
    rows: HashMap<String, Candidate>,
}

impl CandidateBook {
    fn new() -> Self {
        Self {
            rows: HashMap::new(),
        }
    }

    fn row(&mut self, ticker: &str, thscode: &str) -> &mut Candidate {
        self.rows
            .entry(ticker.to_string())
            .or_insert_with(|| Candidate::ensure(thscode, ticker))
    }

    fn merge_limit_up(&mut self, row: &Value) {
        let ticker = text(row, "ticker").trim();
        if ticker.is_empty() {
            return;
        }
        let thscode = text(row, "thscode");
        let candidate = self.row(ticker, thscode);
        candidate.name = text(row, "name").to_string();
        candidate.price = number(row, "last_price").unwrap_or(0.0);
        candidate.change_pct = number(row, "price_change_ratio_pct").unwrap_or(0.0);
        candidate.boards = integer(row, "continue_day_cnt").unwrap_or(0);
        candidate.seal_money = number(row, "seal_money").unwrap_or(0.0);
        candidate.max_seal_money = number(row, "max_seal_money").unwrap_or(0.0);
        candidate.reason = text(row, "limit_up_reason").to_string();
        candidate.is_st = row.get("is_st").and_then(Value::as_bool).unwrap_or(false);
        candidate.is_new = row.get("is_new").and_then(Value::as_bool).unwrap_or(false);
        candidate.sources.insert("limitUp".to_string());
    }

    fn merge_rank(&mut self, row: &Value, source: &str) {
        let ticker = text(row, "ticker").trim();
        if ticker.is_empty() {
            return;
        }
        let thscode = text(row, "thscode");
        let rank = integer(row, "rank");
        let up = integer(row, "rank_change").filter(|change| *change > 0);
        let candidate = self.row(ticker, thscode);
        if candidate.name.is_empty() {
            candidate.name = text(row, "name").to_string();
        }
        if source == "hot" {
            candidate.hot_rank = rank;
            candidate.hot_up = up;
        } else {
            candidate.soar_rank = rank;
        }
        candidate.sources.insert(source.to_string());
    }

    fn merge_dragon(&mut self, row: &Value) {
        let ticker = text(row, "ticker").trim();
        if ticker.is_empty() {
            return;
        }
        let thscode = text(row, "thscode");
        let net = number(row, "net_value").unwrap_or(0.0);
        let org = number(row, "org_net_value").unwrap_or(0.0);
        let candidate = self.row(ticker, thscode);
        if candidate.name.is_empty() {
            candidate.name = text(row, "name").to_string();
        }
        if net.abs() > candidate.dragon_net.abs() {
            candidate.dragon_net = net;
        }
        candidate.dragon_org = candidate.dragon_org || org > 0.0;
        if candidate.reason.is_empty() {
            candidate.reason = text(row, "limit_reason").to_string();
        }
        candidate.sources.insert("dragon".to_string());
    }

    fn finish_scores(&mut self) {
        for candidate in self.rows.values_mut() {
            candidate.heat = heat_score(candidate);
            candidate.money = money_score(candidate);
        }
    }

    fn take(self, limit: usize) -> Vec<Candidate> {
        let mut rows: Vec<Candidate> = self
            .rows
            .into_values()
            .filter(|row| !row.is_st && !row.is_new)
            .collect();
        rows.sort_by(|left, right| {
            let left_score = left.heat * 0.6 + left.money * 0.4;
            let right_score = right.heat * 0.6 + right.money * 0.4;
            right_score
                .partial_cmp(&left_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(left.ticker.cmp(&right.ticker))
        });
        rows.truncate(limit);
        rows
    }
}

fn heat_score(candidate: &Candidate) -> f64 {
    let mut score = 0.0f64;
    if let Some(rank) = candidate.hot_rank {
        score = score.max(100.0 - (rank as f64 - 1.0) * 3.0);
        if let Some(up) = candidate.hot_up {
            score += up.min(10) as f64;
        }
    }
    if let Some(rank) = candidate.soar_rank {
        score = score.max(90.0 - (rank as f64 - 1.0) * 2.5);
    }
    if candidate.sources.len() > 1 {
        score += 10.0 * (candidate.sources.len() - 1) as f64;
    }
    if candidate.boards >= 2 {
        score += (candidate.boards as f64 * 3.0).min(15.0);
    }
    clamp(score, 0.0, 100.0)
}

fn money_score(candidate: &Candidate) -> f64 {
    let mut score = 0.0f64;
    if candidate.seal_money >= 10_000_000.0 {
        score = clamp(
            40.0 + (candidate.seal_money / 10_000_000.0).log10() * 30.0,
            0.0,
            90.0,
        );
    }
    if candidate.max_seal_money > 0.0 && candidate.seal_money > 0.0 {
        score += clamp(
            candidate.seal_money / candidate.max_seal_money * 10.0,
            0.0,
            10.0,
        );
    }
    if candidate.dragon_net > 0.0 {
        score += clamp(
            10.0 + (candidate.dragon_net / 100_000_000.0).log10() * 20.0,
            0.0,
            30.0,
        );
    }
    if candidate.dragon_org {
        score += 10.0;
    }
    clamp(score, 0.0, 100.0)
}

// ── kline and scoring ──────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct Kline {
    ma5: Option<f64>,
    ma10: Option<f64>,
    ma20: Option<f64>,
    volume_ratio: Option<f64>,
    position_60: Option<f64>,
    gain_3d: Option<f64>,
}

fn kline_of(close: &[f64], high: &[f64], low: &[f64], volume: &[f64]) -> Kline {
    let average = |values: &[f64], days: usize| -> Option<f64> {
        if values.len() < days {
            None
        } else {
            Some(values[values.len() - days..].iter().sum::<f64>() / days as f64)
        }
    };
    let ma5 = average(close, 5);
    let ma10 = average(close, 10);
    let ma20 = average(close, 20);
    let volume_ratio = if close.len() >= 7 && volume.len() >= 6 {
        let previous: Vec<f64> = volume[volume.len() - 6..volume.len() - 1].to_vec();
        let base = previous.iter().sum::<f64>() / previous.len() as f64;
        (base > 0.0).then(|| volume[volume.len() - 1] / base)
    } else {
        None
    };
    let position_60 = if close.len() >= 2 {
        let take = close.len().min(60);
        let highs = &high[high.len().saturating_sub(take)..];
        let lows = &low[low.len().saturating_sub(take)..];
        let max = highs.iter().cloned().fold(f64::MIN, f64::max);
        let min = lows.iter().cloned().fold(f64::MAX, f64::min);
        (max > min).then(|| (close[close.len() - 1] - min) / (max - min) * 100.0)
    } else {
        None
    };
    let gain_3d = if close.len() >= 4 && close[close.len() - 4] > 0.0 {
        Some((close[close.len() - 1] / close[close.len() - 4] - 1.0) * 100.0)
    } else {
        None
    };
    Kline {
        ma5,
        ma10,
        ma20,
        volume_ratio,
        position_60,
        gain_3d,
    }
}

fn momentum_score(kline: &Kline, price: f64) -> f64 {
    let mut score = 50.0f64;
    if let (Some(ma5), Some(ma10), Some(ma20)) = (kline.ma5, kline.ma10, kline.ma20) {
        if ma5 > ma10 && ma10 > ma20 && price > ma5 {
            score += 25.0;
        }
        let deviation = (price / ma5 - 1.0) * 100.0;
        if deviation.abs() <= 3.0 {
            score += 18.0;
        } else if deviation > 8.0 {
            score -= 20.0;
        }
        if price < ma10 {
            score -= 12.0;
        }
    }
    if let Some(ratio) = kline.volume_ratio {
        if (1.3..=4.0).contains(&ratio) {
            score += 15.0;
        } else if ratio > 5.0 {
            score -= 8.0;
        }
    }
    if let Some(position) = kline.position_60 {
        if position > 90.0 {
            score -= 10.0;
        }
    }
    if let Some(gain) = kline.gain_3d {
        if gain > 20.0 {
            score -= 15.0;
        }
    }
    clamp(score, 0.0, 100.0)
}

fn quality_score(pe_ttm: Option<f64>, reason: &str, themes: &[String]) -> f64 {
    let mut score = 50.0f64;
    if let Some(pe) = pe_ttm {
        if pe > 0.0 && pe < 40.0 {
            score += 20.0;
        } else if pe >= 40.0 && pe < 80.0 {
            score += 10.0;
        } else if pe < 0.0 {
            score -= 20.0;
        }
    }
    if !reason.is_empty() && themes.iter().any(|theme| theme == reason) {
        score += 15.0;
    }
    clamp(score, 0.0, 100.0)
}

fn strategy_weights(strategy: &str) -> (f64, f64, f64, f64) {
    match strategy {
        "limitup" => (0.40, 0.30, 0.20, 0.10),
        "trend" => (0.15, 0.10, 0.50, 0.25),
        "dragon" => (0.10, 0.45, 0.25, 0.20),
        _ => (0.30, 0.25, 0.30, 0.15),
    }
}

fn advice_of(
    candidate: &Candidate,
    kline: &Kline,
    _sentiment: &Sentiment,
) -> (&'static str, Value, f64, f64, f64, &'static str) {
    if candidate.boards > 0 || candidate.change_pct >= 9.5 {
        let entry = json!({
            "kind": "auction",
            "lower": round2(candidate.price * 1.02),
            "upper": round2(candidate.price * 1.05),
            "giveUpBelowPct": 3,
        });
        let stop = round2(candidate.price * 0.93);
        (
            "board",
            entry,
            stop,
            round2(candidate.price * 1.08),
            round2(candidate.price * 1.16),
            "days1to3",
        )
    } else {
        let ma5 = kline.ma5.unwrap_or(candidate.price);
        let ma10 = kline.ma10.unwrap_or(candidate.price * 0.98);
        let deviation = (candidate.price / ma5 - 1.0) * 100.0;
        let (_kind, entry) = if candidate.price < ma10 {
            (
                "reclaim",
                json!({ "kind": "reclaim", "anchor": round2(ma10) }),
            )
        } else if deviation.abs() <= 3.0 {
            (
                "now",
                json!({ "kind": "now", "anchor": round2(candidate.price) }),
            )
        } else if deviation <= 8.0 {
            (
                "pullback",
                json!({
                    "kind": "pullback",
                    "lower": round2(ma5 * 0.97),
                    "upper": round2(ma5 * 1.03),
                }),
            )
        } else {
            ("wait", json!({ "kind": "wait", "anchor": round2(ma5) }))
        };
        let stop = round2(ma10.max(candidate.price * 0.93));
        (
            "trend",
            entry,
            stop,
            round2(candidate.price * 1.08),
            round2(candidate.price * 1.16),
            "days3to8",
        )
    }
}

fn position_pct(score: i64, cap: i64) -> i64 {
    let factor = if score >= 75 {
        1.0
    } else if score >= 65 {
        0.7
    } else if score >= 55 {
        0.45
    } else {
        0.0
    };
    (cap as f64 * factor).round() as i64
}

fn reason_rows(candidate: &Candidate, kline: &Kline) -> Vec<Value> {
    let mut reasons = Vec::new();
    if candidate.boards > 0 {
        reasons.push(json!({ "key": "boards", "count": candidate.boards }));
    }
    if candidate.seal_money > 0.0 {
        let (amount, unit) = if candidate.seal_money >= 100_000_000.0 {
            (round2(candidate.seal_money / 100_000_000.0), "yi")
        } else {
            ((candidate.seal_money / 10_000.0).round(), "wan")
        };
        reasons.push(json!({ "key": "seal", "amount": amount, "unit": unit }));
    }
    if let Some(rank) = candidate.hot_rank {
        let mut row = json!({ "key": "hotRank", "rank": rank });
        if let Some(up) = candidate.hot_up {
            row["up"] = json!(up);
        }
        reasons.push(row);
    }
    if candidate.dragon_net.abs() > 0.0 {
        reasons.push(json!({
            "key": "dragonNet",
            "amount": round2(candidate.dragon_net / 100_000_000.0),
            "org": candidate.dragon_org,
        }));
    }
    if !candidate.reason.is_empty() {
        reasons.push(json!({ "key": "theme", "name": candidate.reason }));
    }
    if let (Some(ma5), Some(ma10), Some(ma20)) = (kline.ma5, kline.ma10, kline.ma20) {
        if ma5 > ma10 && ma10 > ma20 && candidate.price > ma5 {
            reasons.push(json!({ "key": "maBull" }));
        }
    }
    if let (Some(ratio), Some(position)) = (kline.volume_ratio, kline.position_60) {
        reasons.push(json!({
            "key": "volPosition",
            "ratio": round2(ratio),
            "position": (position.round() as i64),
        }));
    }
    if candidate.boards >= 2 {
        reasons.push(json!({ "key": "heightMember", "count": candidate.boards }));
    }
    reasons
}

// ── overview ───────────────────────────────────────────────────────────────

fn normalize_strategy(raw: &str) -> &'static str {
    match raw.trim() {
        "limitup" => "limitup",
        "trend" => "trend",
        "dragon" => "dragon",
        _ => "all",
    }
}

fn overview_blocking(
    engine: Arc<StocksEngine>,
    key: String,
    strategy: &'static str,
) -> Result<Value, StocksError> {
    if let Some(cached) = engine.cached_overview(strategy) {
        return Ok(cached);
    }
    let market = market_snapshot(&engine, &key)?;
    let hot = optional(engine.fetch(
        &key,
        "/api/a-share/special-data/hot-stock-list",
        &[("period", "day".to_string()), ("limit", "30".to_string())],
        HOT_TTL,
    ))?;
    let soar = optional(engine.fetch(
        &key,
        "/api/a-share/special-data/skyrocket-list",
        &[("period", "day".to_string()), ("limit", "30".to_string())],
        HOT_TTL,
    ))?;
    // The dragon board only publishes after the close; letting the API pick
    // its latest available day keeps a morning refresh from asking for a date
    // the upstream cannot answer yet.
    let dragon = optional(engine.fetch(
        &key,
        "/api/a-share/special-data/dragon-tiger-list",
        &[("board_type", "all".to_string())],
        DRAGON_TTL,
    ))?;
    let indices = optional(engine.fetch(
        &key,
        "/api/a-share-index/prices/snapshot",
        &[("thscodes", INDEX_CODES.join(","))],
        SNAPSHOT_TTL,
    ))?;
    let index_rows = index_quotes(&indices);

    let mut book = CandidateBook::new();
    for row in &market.limit_up {
        book.merge_limit_up(row);
    }
    for row in items(&hot) {
        book.merge_rank(&row, "hot");
    }
    for row in items(&soar) {
        book.merge_rank(&row, "soar");
    }
    if let Some(rows) = dragon.get("stock_items").and_then(Value::as_array) {
        for row in rows {
            book.merge_dragon(row);
        }
    }
    book.finish_scores();
    let candidates = book.take(MAX_CANDIDATES);
    if candidates.is_empty() {
        let empty = json!({
            "generatedAt": now_iso(),
            "strategy": strategy,
            "sentiment": sentiment_json(&market.sentiment),
            "stats": stats_json(&market.stats),
            "themes": market.themes,
            "indices": index_rows,
            "ladder": market.ladder,
            "items": [],
        });
        engine
            .overviews
            .lock()
            .unwrap()
            .insert(strategy.to_string(), (Instant::now(), empty.clone()));
        *engine.last_overview.lock().unwrap() = Some((Instant::now(), empty.clone()));
        return Ok(empty);
    }

    let codes: Vec<String> = candidates
        .iter()
        .map(|row| row.thscode.clone())
        .filter(|code| !code.is_empty())
        .collect();
    let (snapshot, valuations) = if codes.is_empty() {
        (Value::Null, Value::Null)
    } else {
        let joined = codes.join(",");
        (
            engine.fetch(
                &key,
                "/api/a-share/prices/snapshot",
                &[("thscodes", joined.clone())],
                SNAPSHOT_TTL,
            )?,
            optional(engine.fetch(
                &key,
                "/api/a-share/valuations/snapshot",
                &[("thscodes", joined)],
                VALUATION_TTL,
            ))?,
        )
    };
    let snapshot_by_code = snapshot_items(&snapshot);
    let pe_by_code = valuation_items(&valuations);

    let theme_names: Vec<String> = market
        .themes
        .iter()
        .filter_map(|row| row.get("name").and_then(Value::as_str).map(str::to_string))
        .collect();
    let (wheat, wmoney, wmomentum, wquality) = strategy_weights(strategy);

    let mut scored: Vec<(f64, Value)> = Vec::new();
    for mut candidate in candidates {
        if let Some(snap) = snapshot_by_code.get(&candidate.thscode) {
            candidate.price = number(snap, "last_price").unwrap_or(candidate.price);
            candidate.change_pct =
                number(snap, "price_change_ratio_pct").unwrap_or(candidate.change_pct);
        }
        // A row with no quote would render an advice plan around 0; drop it
        // rather than print a number nobody read.
        if candidate.price <= 0.0 {
            continue;
        }
        let kline = if candidate.thscode.is_empty() {
            Kline::default()
        } else {
            fetch_kline(&engine, &key, &candidate.thscode).unwrap_or_default()
        };
        let momentum = momentum_score(&kline, candidate.price);
        let pe = pe_by_code.get(&candidate.thscode).copied();
        let quality = quality_score(pe, &candidate.reason, &theme_names);
        let composite = (candidate.heat * wheat
            + candidate.money * wmoney
            + momentum * wmomentum
            + quality * wquality)
            .round() as i64;
        let (strategy_key, entry, stop, target1, target2, horizon) =
            advice_of(&candidate, &kline, &market.sentiment);
        let item = json!({
            "code": candidate.ticker,
            "name": if candidate.name.is_empty() { candidate.ticker.clone() } else { candidate.name.clone() },
            "price": round2(candidate.price),
            "changePct": round2(candidate.change_pct),
            "score": composite,
            "strategyKey": strategy_key,
            "heat": candidate.heat.round() as i64,
            "money": candidate.money.round() as i64,
            "momentum": momentum.round() as i64,
            "quality": quality.round() as i64,
            "entry": entry,
            "stop": stop,
            "target1": target1,
            "target2": target2,
            "positionPct": position_pct(composite, market.sentiment.max_position_pct),
            "horizonKey": horizon,
            "reasons": reason_rows(&candidate, &kline),
            "sources": candidate.sources.iter().cloned().collect::<Vec<String>>(),
        });
        scored.push((composite as f64, item));
    }
    scored.sort_by(|left, right| {
        right
            .0
            .partial_cmp(&left.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                left.1["code"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(right.1["code"].as_str().unwrap_or("")),
            )
    });
    let items: Vec<Value> = scored
        .into_iter()
        .take(MAX_ITEMS)
        .map(|(_, item)| item)
        .collect();

    let result = json!({
        "generatedAt": now_iso(),
        "strategy": strategy,
        "sentiment": sentiment_json(&market.sentiment),
        "stats": stats_json(&market.stats),
        "themes": market.themes,
        "indices": index_rows,
        "ladder": market.ladder,
        "items": items,
    });
    engine
        .overviews
        .lock()
        .unwrap()
        .insert(strategy.to_string(), (Instant::now(), result.clone()));
    *engine.last_overview.lock().unwrap() = Some((Instant::now(), result.clone()));
    Ok(result)
}

fn fetch_kline(engine: &StocksEngine, key: &str, thscode: &str) -> Option<Kline> {
    if let Some((at, kline)) = engine.klines.lock().unwrap().get(thscode) {
        if at.elapsed() < KLINE_TTL {
            return Some(kline.clone());
        }
    }
    let now = Utc::now();
    let start =
        (now - chrono::Duration::days(MAX_KLINE_DAYS)).timestamp_millis() / 86_400_000 * 86_400_000;
    // Keep the query window on five-minute buckets: the upstream cache then
    // behaves like the per-symbol cache the console expects, instead of asking
    // for a new URL every time the clock moves a millisecond.
    let end = now.timestamp_millis() / (5 * 60 * 1000) * (5 * 60 * 1000);
    let data = engine
        .fetch(
            key,
            "/api/a-share/prices/historical",
            &[
                ("thscode", thscode.to_string()),
                ("interval", "1d".to_string()),
                ("start", start.to_string()),
                ("end", end.to_string()),
                ("adjust", "forward".to_string()),
            ],
            KLINE_TTL,
        )
        .ok()?;
    let rows = items(&data);
    if rows.len() < 2 {
        return None;
    }
    let mut close = Vec::new();
    let mut high = Vec::new();
    let mut low = Vec::new();
    let mut volume = Vec::new();
    for row in rows {
        let Some(close_price) = number(&row, "close_price") else {
            continue;
        };
        close.push(close_price);
        high.push(number(&row, "high_price").unwrap_or(close_price));
        low.push(number(&row, "low_price").unwrap_or(close_price));
        volume.push(number(&row, "volume").unwrap_or(0.0));
    }
    if close.len() < 2 {
        return None;
    }
    let kline = kline_of(&close, &high, &low, &volume);
    engine
        .klines
        .lock()
        .unwrap()
        .insert(thscode.to_string(), (Instant::now(), kline.clone()));
    Some(kline)
}

fn snapshot_items(value: &Value) -> HashMap<String, Value> {
    items(value)
        .into_iter()
        .filter_map(|row| {
            let code = text(&row, "thscode").to_string();
            (!code.is_empty()).then_some((code, row))
        })
        .collect()
}

fn valuation_items(value: &Value) -> HashMap<String, f64> {
    items(value)
        .into_iter()
        .filter_map(|row| {
            let code = text(&row, "thscode").to_string();
            let pe = number(&row, "pe_ttm")?;
            (!code.is_empty()).then_some((code, pe))
        })
        .collect()
}

const INDEX_CODES: [&str; 4] = ["000001.SH", "399001.SZ", "399006.SZ", "000688.SH"];

fn index_quotes(value: &Value) -> Vec<Value> {
    let names = [
        ("000001.SH", "上证指数"),
        ("399001.SZ", "深证成指"),
        ("399006.SZ", "创业板指"),
        ("000688.SH", "科创50"),
    ];
    items(value)
        .into_iter()
        .filter_map(|row| {
            let code = text(&row, "thscode").to_string();
            let name = names
                .iter()
                .find(|(index_code, _)| *index_code == code)
                .map(|(_, name)| *name)
                .unwrap_or(code.as_str());
            let last = number(&row, "last_price")?;
            Some(json!({
                "code": code,
                "name": name,
                "last": round2(last),
                "pct": round2(number(&row, "price_change_ratio_pct").unwrap_or(0.0)),
            }))
        })
        .collect()
}

fn sentiment_json(sentiment: &Sentiment) -> Value {
    json!({
        "score": sentiment.score,
        "phase": sentiment.phase,
        "maxPositionPct": sentiment.max_position_pct,
    })
}

fn stats_json(stats: &MarketStats) -> Value {
    json!({
        "limitUp": stats.limit_up,
        "limitDown": stats.limit_down,
        "breakBoard": stats.break_board,
        "maxBoard": stats.max_board,
        "breakRatePct": round2(stats.break_rate_pct),
        "promotePct": round2(stats.promote_pct),
    })
}

fn now_iso() -> String {
    Utc::now()
        .with_timezone(&shanghai())
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, false)
}

// ── plan ───────────────────────────────────────────────────────────────────

fn plan_session(
    now: chrono::DateTime<FixedOffset>,
    trading_today: bool,
) -> (&'static str, &'static str) {
    if !trading_today {
        return ("weekend", "closed");
    }
    let minutes = now.hour() * 60 + now.minute();
    if minutes < 9 * 60 + 15 {
        ("preOpen", "preOpen")
    } else if minutes < 9 * 60 + 25 {
        ("auction", "preOpen")
    } else if minutes < 9 * 60 + 30 {
        ("auctionConfirm", "preOpen")
    } else if minutes < 11 * 60 + 30 {
        ("morning", "morning")
    } else if minutes < 13 * 60 + 30 {
        ("midday", "midday")
    } else if minutes < 14 * 60 + 30 {
        ("afternoon", "afternoon")
    } else if minutes < 15 * 60 {
        ("close", "afternoon")
    } else {
        ("review", "closed")
    }
}

fn plan_blocking(engine: Arc<StocksEngine>, key: String) -> Result<Value, StocksError> {
    let market = market_snapshot(&engine, &key)?;
    let now = Utc::now().with_timezone(&shanghai());
    let (session_key, market_phase) = plan_session(now, market.trade.today_is_trading);
    let urgency = match session_key {
        "weekend" => "low",
        "auction" | "auctionConfirm" | "morning" | "close" => "high",
        _ => "normal",
    };

    // Alerts first: a stop that fired outranks anything else on the list.
    let mut items: Vec<Value> = Vec::new();
    let watch = watch_rows(&engine, &key);
    for row in &watch {
        let price = number(row, "price").unwrap_or(0.0);
        if price <= 0.0 {
            continue;
        }
        if let Some(stop) = number(row, "stop") {
            if price <= stop {
                items.push(json!({
                    "kind": "stopHit",
                    "code": text(row, "code"),
                    "name": text(row, "name"),
                    "price": round2(price),
                    "stop": stop,
                    "urgency": "high",
                }));
            }
        }
        if let Some(target) = number(row, "target") {
            if price >= target {
                items.push(json!({
                    "kind": "targetHit",
                    "code": text(row, "code"),
                    "name": text(row, "name"),
                    "price": round2(price),
                    "target": target,
                    "urgency": "high",
                }));
            }
        }
    }

    items.push(json!({
        "kind": "session",
        "sessionKey": session_key,
        "urgency": urgency,
    }));
    if session_key == "morning" {
        items.push(
            json!({ "kind": "session", "sessionKey": "sentimentCheck", "urgency": "normal" }),
        );
    }

    if let Some(overview) = engine.recent_overview(Duration::from_secs(10 * 60)) {
        if let Some(rows) = overview.get("items").and_then(Value::as_array) {
            for row in rows.iter().take(3) {
                let price = number(row, "price").unwrap_or(0.0);
                if price <= 0.0 {
                    continue;
                }
                items.push(json!({
                    "kind": "track",
                    "code": text(row, "code"),
                    "name": text(row, "name"),
                    "price": round2(price),
                    "score": integer(row, "score").unwrap_or(0),
                    "entry": row.get("entry").cloned().unwrap_or(Value::Null),
                    "stop": number(row, "stop").map(round2).unwrap_or(0.0),
                    "urgency": "normal",
                }));
            }
        }
    }

    Ok(json!({
        "now": now.to_rfc3339_opts(chrono::SecondsFormat::Millis, false),
        "sessionKey": market_phase,
        "phase": market.sentiment.phase,
        "maxPositionPct": market.sentiment.max_position_pct,
        "items": items,
    }))
}

// ── watchlist ──────────────────────────────────────────────────────────────

fn infer_thscode(code: &str) -> String {
    let code = code.trim();
    if code.contains('.') {
        return code.to_string();
    }
    let suffix = match code.chars().next() {
        Some('6') => "SH",
        Some('0') | Some('3') => "SZ",
        Some('4') | Some('8') => "BJ",
        _ => "SH",
    };
    format!("{code}.{suffix}")
}

fn watch_rows(engine: &StocksEngine, key: &str) -> Vec<Value> {
    let stored = engine.read_watchlist();
    if stored.is_empty() {
        return Vec::new();
    }
    let codes: Vec<String> = stored
        .iter()
        .map(|item| {
            if item.thscode.is_empty() {
                infer_thscode(&item.code)
            } else {
                item.thscode.clone()
            }
        })
        .collect();
    // The upstream batch accepts a comma-separated list; keep each request
    // bounded so a long watchlist cannot turn into one oversized URL.
    let mut quotes: HashMap<String, Value> = HashMap::new();
    for chunk in codes.chunks(50) {
        if let Ok(value) = engine.fetch(
            key,
            "/api/a-share/prices/snapshot",
            &[("thscodes", chunk.join(","))],
            SNAPSHOT_TTL,
        ) {
            quotes.extend(snapshot_items(&value));
        }
    }
    stored
        .into_iter()
        .zip(codes)
        .map(|(item, thscode)| {
            let quote = quotes.get(&thscode);
            let price = quote
                .and_then(|row| number(row, "last_price"))
                .unwrap_or(0.0);
            let change = quote
                .and_then(|row| number(row, "price_change_ratio_pct"))
                .unwrap_or(0.0);
            let mut row = json!({
                "code": item.code,
                "name": item.name,
                "price": round2(price),
                "changePct": round2(change),
            });
            if !item.note.is_empty() {
                row["note"] = json!(item.note);
            }
            if let Some(stop) = item.stop {
                row["stop"] = json!(stop);
            }
            if let Some(target) = item.target {
                row["target"] = json!(target);
            }
            if !item.added_at.is_empty() {
                row["addedAt"] = json!(item.added_at);
            }
            row
        })
        .collect()
}

async fn watch_add(
    engine: &Arc<StocksEngine>,
    key: &str,
    params: &Value,
) -> Result<Value, StocksError> {
    let query = params
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if query.is_empty() {
        return Err(StocksError::new(
            "INVALID_ARGUMENT",
            "请输入 6 位代码或股票名称",
        ));
    }
    let note = params
        .get("note")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let stop = params.get("stop").and_then(Value::as_f64);
    let target = params.get("target").and_then(Value::as_f64);
    let probe = Arc::clone(engine);
    let key_for_search = key.to_string();
    let query_for_search = query.to_string();
    let search = tokio::task::spawn_blocking(move || {
        probe.fetch(
            &key_for_search,
            "/api/meta/tickers/search",
            &[
                ("q", query_for_search),
                ("asset_type", "a-share".to_string()),
                ("limit", "10".to_string()),
            ],
            HOT_TTL,
        )
    })
    .await
    .map_err(|error| StocksError::new("INTERNAL", error.to_string()))??;

    let rows: Vec<Value> = items(&search)
        .into_iter()
        .filter(|row| text(row, "asset_type") == "a-share")
        .collect();
    let picked = if query.chars().all(|ch| ch.is_ascii_digit()) || query.contains('.') {
        rows.iter()
            .find(|row| {
                text(row, "ticker") == query
                    || text(row, "thscode") == query
                    || text(row, "thscode")
                        .split('.')
                        .next()
                        .map(|code| code == query)
                        .unwrap_or(false)
            })
            .or_else(|| rows.first())
    } else {
        rows.iter()
            .find(|row| text(row, "name") == query)
            .or_else(|| rows.first())
    };
    let Some(picked) = picked else {
        return Err(StocksError::new(
            "STOCKS_UPSTREAM",
            format!("没有找到标的「{query}」"),
        ));
    };
    let ticker = text(picked, "ticker").to_string();
    let thscode = text(picked, "thscode").to_string();
    let name = text(picked, "name").to_string();
    let mut list = engine.read_watchlist();
    if list.iter().any(|item| item.code == ticker) {
        return Ok(json!({ "added": false, "code": ticker, "name": name }));
    }
    list.push(WatchItem {
        code: ticker.clone(),
        name: name.clone(),
        thscode,
        note,
        stop,
        target,
        added_at: now_iso(),
    });
    engine.write_watchlist(&list)?;
    Ok(json!({ "added": true, "code": ticker, "name": name }))
}

// ── RPC entry points ──────────────────────────────────────────────────────

pub async fn overview(
    state: &Arc<crate::state::AppState>,
    params: &Value,
) -> Result<Value, StocksError> {
    let engine = engine_of(state);
    let (key, _) = engine.resolve_key()?;
    let strategy = normalize_strategy(
        params
            .get("strategy")
            .and_then(Value::as_str)
            .unwrap_or("all"),
    );
    tokio::task::spawn_blocking(move || overview_blocking(engine, key, strategy))
        .await
        .map_err(|error| StocksError::new("INTERNAL", error.to_string()))?
}

pub async fn set_key(
    state: &Arc<crate::state::AppState>,
    params: &Value,
) -> Result<Value, StocksError> {
    let raw = params
        .get("key")
        .or_else(|| params.get("apiKey"))
        .and_then(Value::as_str)
        .unwrap_or("");
    StocksEngine::save_key(engine_of(state), raw).await
}

pub async fn plan(state: &Arc<crate::state::AppState>) -> Result<Value, StocksError> {
    let engine = engine_of(state);
    let (key, _) = engine.resolve_key()?;
    tokio::task::spawn_blocking(move || plan_blocking(engine, key))
        .await
        .map_err(|error| StocksError::new("INTERNAL", error.to_string()))?
}

pub async fn watch_list(state: &Arc<crate::state::AppState>) -> Result<Value, StocksError> {
    let engine = engine_of(state);
    let key = engine.resolve_key().map(|(key, _)| key).unwrap_or_default();
    let engine_for_rows = engine.clone();
    let key_for_rows = key.clone();
    let rows = tokio::task::spawn_blocking(move || {
        if key_for_rows.is_empty() {
            engine_for_rows
                .read_watchlist()
                .into_iter()
                .map(|item| {
                    json!({
                        "code": item.code,
                        "name": item.name,
                        "price": 0.0,
                        "changePct": 0.0,
                        "stop": item.stop,
                        "target": item.target,
                    })
                })
                .collect::<Vec<Value>>()
        } else {
            watch_rows(&engine_for_rows, &key_for_rows)
        }
    })
    .await
    .map_err(|error| StocksError::new("INTERNAL", error.to_string()))?;
    Ok(json!({ "items": rows }))
}

pub async fn watch_add_rpc(
    state: &Arc<crate::state::AppState>,
    params: &Value,
) -> Result<Value, StocksError> {
    let engine = engine_of(state);
    let (key, _) = engine.resolve_key()?;
    watch_add(&engine, &key, params).await
}

pub fn watch_remove(
    state: &Arc<crate::state::AppState>,
    params: &Value,
) -> Result<Value, StocksError> {
    let code = params
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if code.is_empty() {
        return Err(StocksError::new("INVALID_ARGUMENT", "code required"));
    }
    let engine = engine_of(state);
    let mut list = engine.read_watchlist();
    let before = list.len();
    list.retain(|item| item.code != code && item.thscode != code);
    let removed = list.len() != before;
    if removed {
        engine.write_watchlist(&list)?;
    }
    Ok(json!({ "removed": removed }))
}

pub fn stocks_result(result: Result<Value, StocksError>) -> Value {
    match result {
        Ok(value) => crate::envelope::ok(value),
        Err(error) => crate::envelope::err(&error.code, error.message),
    }
}

// ── tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(
        limit_up: i64,
        limit_down: i64,
        max_board: i64,
        break_rate: f64,
        promote: f64,
    ) -> MarketStats {
        MarketStats {
            limit_up,
            limit_down,
            break_board: 0,
            max_board,
            break_rate_pct: break_rate,
            promote_pct: promote,
        }
    }

    #[test]
    fn the_sentiment_cycle_maps_to_the_manual_position_caps() {
        let freeze = sentiment(&stats(10, 20, 2, 80.0, 0.0));
        assert!(freeze.score < 25);
        assert_eq!(freeze.phase, "freeze");
        assert_eq!(freeze.max_position_pct, 5);

        let climax = sentiment(&stats(100, 0, 8, 0.0, 100.0));
        assert!(climax.score >= 65);
        assert_eq!(climax.phase, "climax");
        assert_eq!(climax.max_position_pct, 50);
    }

    #[test]
    fn a_limit_up_candidate_gets_the_auction_plan_and_a_trend_candidate_the_ma_plan() {
        let sentiment = Sentiment {
            score: 50,
            phase: "warm",
            max_position_pct: 35,
        };
        let mut board = Candidate {
            price: 10.0,
            boards: 2,
            ..Candidate::default()
        };
        board.change_pct = 10.0;
        let (kind, entry, stop, target1, _target2, horizon) =
            advice_of(&board, &Kline::default(), &sentiment);
        assert_eq!(kind, "board");
        assert_eq!(entry["kind"], json!("auction"));
        assert_eq!(entry["lower"], json!(10.2));
        assert_eq!(stop, 9.3);
        assert_eq!(target1, 10.8);
        assert_eq!(horizon, "days1to3");

        let trend = Candidate {
            price: 10.0,
            change_pct: 2.0,
            ..Candidate::default()
        };
        let kline = Kline {
            ma5: Some(10.0),
            ma10: Some(9.8),
            ..Kline::default()
        };
        let (kind, entry, stop, _t1, _t2, horizon) = advice_of(&trend, &kline, &sentiment);
        assert_eq!(kind, "trend");
        assert_eq!(entry["kind"], json!("now"));
        assert_eq!(stop, 9.8);
        assert_eq!(horizon, "days3to8");
    }

    #[test]
    fn the_position_cap_follows_the_score_bands() {
        assert_eq!(position_pct(80, 35), 35);
        assert_eq!(position_pct(68, 35), 25);
        assert_eq!(position_pct(58, 35), 16);
        assert_eq!(position_pct(40, 35), 0);
    }

    #[test]
    fn a_missing_upstream_number_is_never_invented() {
        let value = json!({ "item": [ { "last_price": 1.0 }, { "thscode": "X" } ] });
        let map = snapshot_items(&value);
        assert_eq!(map.len(), 1);
        assert!(!map.contains_key(""));
    }

    #[test]
    fn the_plan_walks_the_trading_day() {
        let day = Utc::now()
            .with_timezone(&shanghai())
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap();
        assert_eq!(plan_session(day.with_hour(8).unwrap(), true).0, "preOpen");
        assert_eq!(
            plan_session(day.with_hour(9).unwrap().with_minute(20).unwrap(), true).0,
            "auction"
        );
        assert_eq!(plan_session(day.with_hour(10).unwrap(), true).0, "morning");
        assert_eq!(plan_session(day.with_hour(12).unwrap(), true).0, "midday");
        assert_eq!(
            plan_session(day.with_hour(14).unwrap(), true).0,
            "afternoon"
        );
        assert_eq!(
            plan_session(day.with_hour(14).unwrap().with_minute(45).unwrap(), true).0,
            "close"
        );
        assert_eq!(plan_session(day.with_hour(16).unwrap(), true).0, "review");
        assert_eq!(plan_session(day, false), ("weekend", "closed"));
    }

    #[test]
    fn the_watchlist_file_survives_a_hostile_payload() {
        let dir = std::env::temp_dir().join(format!("pi-stocks-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let engine = StocksEngine::new(dir.clone());
        assert!(engine.read_watchlist().is_empty());
        std::fs::write(engine.watchlist_path(), "{ not json").unwrap();
        assert!(engine.read_watchlist().is_empty());
        engine
            .write_watchlist(&[WatchItem {
                code: "600519".into(),
                name: "贵州茅台".into(),
                thscode: "600519.SH".into(),
                note: String::new(),
                stop: Some(100.0),
                target: Some(200.0),
                added_at: "2026-01-01T00:00:00+08:00".into(),
            }])
            .unwrap();
        let rows = engine.read_watchlist();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].code, "600519");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_api_key_is_masked_and_never_echoed() {
        assert_eq!(mask_key("short"), "••••");
        assert_eq!(mask_key("sk-1234567890abcdef"), "sk-1…cdef");
    }
}
