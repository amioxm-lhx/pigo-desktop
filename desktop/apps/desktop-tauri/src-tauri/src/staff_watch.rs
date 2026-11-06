//! The return leg: what happened after the team spoke (spec 03-runtime/20 §15).
//!
//! host-core owns which published pieces are worth revisiting and what a
//! reading does to the boards (`staff.watch.list` / `staff.watch.ingest`).
//! This module is the resident half: every fifteen minutes it hands each
//! published URL to the site collector, which reads the page the site rendered
//! and returns the piece's reply count and the comments under it. The same pass
//! runs when the owner presses "read now".
//!
//! The page is the source of truth for a count, so a number that went down is
//! written down rather than subtracted, and a reply becomes a card exactly once
//! (host-core keys it by site/author/quote). A page that cannot be read is
//! reported, never counted as "nobody answered".

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::AppHandle;

use crate::state::AppState;

/// How often the resident loop revisits published pieces.
pub const INTERVAL_MS: u64 = 15 * 60 * 1000;
/// How many pieces one pass reads; host-core rotates the rest in over time.
const TARGET_LIMIT: i64 = 8;
/// One page may hand this many comments to the lead board.
const MAX_REPLY_ITEMS: usize = 20;
const MAX_LINES: usize = 16;

pub struct Watch {
    resident: AtomicBool,
    running: AtomicBool,
    next_at: Mutex<String>,
    last: Mutex<Option<Value>>,
    last_error: Mutex<Option<String>>,
    /// One pass at a time: a manual read and the timer must not fight over the
    /// same pages, and two passes would file the same reply twice.
    pass: tokio::sync::Mutex<()>,
}

impl Watch {
    pub fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            resident: AtomicBool::new(false),
            running: AtomicBool::new(false),
            next_at: Mutex::new(next_at()),
            last: Mutex::new(None),
            last_error: Mutex::new(None),
            pass: tokio::sync::Mutex::new(()),
        })
    }
}

/// Start the resident loop once boot has the host and sidecar handshaked.
pub fn start(app: AppHandle, state: std::sync::Arc<AppState>) {
    state.watch.resident.store(true, Ordering::Relaxed);
    log::info!("staff watch resident loop every {INTERVAL_MS}ms");
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(INTERVAL_MS)).await;
            *state.watch.next_at.lock().unwrap() = next_at();
            let _guard = state.watch.pass.lock().await;
            run_pass(&app, &state, "timer").await;
        }
    });
}

pub fn status(state: &AppState) -> Value {
    let mut status = json!({
        "resident": state.watch.resident.load(Ordering::Relaxed),
        "intervalMs": INTERVAL_MS,
        "running": state.watch.running.load(Ordering::Relaxed),
        "nextAt": state.watch.next_at.lock().unwrap().clone(),
    });
    if let Some(last) = state.watch.last.lock().unwrap().clone() {
        status["last"] = last;
    }
    if let Some(error) = state.watch.last_error.lock().unwrap().clone() {
        status["lastError"] = json!(error);
    }
    status
}

/// `staff/watch/run`: the same pass the timer runs, requested by the owner.
pub async fn run_now(app: &AppHandle, state: &std::sync::Arc<AppState>) -> Value {
    let _guard = state.watch.pass.lock().await;
    run_pass(app, state, "manual").await
}

struct PassSummary {
    targets: i64,
    found: i64,
    new_replies: i64,
    leads: i64,
    lines: Vec<String>,
}

async fn run_pass(app: &AppHandle, state: &std::sync::Arc<AppState>, trigger: &str) -> Value {
    state.watch.running.store(true, Ordering::Relaxed);
    let summary = collect_pass(app, state).await;
    let outcome = match summary {
        Ok(summary) => {
            *state.watch.last_error.lock().unwrap() = None;
            json!({
                "at": now_iso(),
                "trigger": trigger,
                "targets": summary.targets,
                "found": summary.found,
                "newReplies": summary.new_replies,
                "leads": summary.leads,
                "lines": summary.lines,
            })
        }
        Err(error) => {
            *state.watch.last_error.lock().unwrap() = Some(error.clone());
            json!({
                "at": now_iso(),
                "trigger": trigger,
                "targets": 0,
                "found": 0,
                "newReplies": 0,
                "leads": 0,
                "lines": [format!("巡检失败：{error}")],
            })
        }
    };
    *state.watch.last.lock().unwrap() = Some(outcome.clone());
    state.watch.running.store(false, Ordering::Relaxed);
    outcome
}

async fn collect_pass(
    app: &AppHandle,
    state: &std::sync::Arc<AppState>,
) -> Result<PassSummary, String> {
    let listing = state
        .host
        .call("staff.watch.list", json!({ "limit": TARGET_LIMIT }))
        .await
        .map_err(|error| error.to_string())?;
    let targets = listing
        .get("targets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut summary = PassSummary {
        targets: targets.len() as i64,
        found: 0,
        new_replies: 0,
        leads: 0,
        lines: Vec::new(),
    };

    for target in targets {
        let title = text(&target, "title");
        let content_id = text(&target, "contentId");
        let url = text(&target, "url");
        let fingerprint = text(&target, "fingerprint");

        // The address names the site; the draft's channel label does not.
        let Some(spec) = crate::staff_sites::site_by_url(&url) else {
            summary
                .lines
                .push(format!("《{title}》跳过：这个地址不属于已接入的站点"));
            continue;
        };
        if !crate::staff_sites::site_signed_in(app, spec.channel).await {
            summary.lines.push(format!(
                "《{title}》跳过：这台机器没有 {} 登录会话",
                spec.channel
            ));
            continue;
        }

        let extraction =
            crate::staff_sites::read_published(app, spec.channel, &url, &fingerprint).await;
        let extraction = match extraction {
            Ok(extraction) => extraction,
            Err(error) => {
                summary
                    .lines
                    .push(format!("《{title}》读不到：{}", error.message));
                continue;
            }
        };

        let found_here = extraction
            .get("found")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let params = ingest_params(&content_id, &extraction);
        match state.host.call("staff.watch.ingest", params).await {
            Ok(reply) => {
                if found_here {
                    summary.found += 1;
                }
                let new_replies = reply.get("newReplies").and_then(Value::as_i64).unwrap_or(0);
                let leads = reply
                    .get("leads")
                    .and_then(Value::as_array)
                    .map(|rows| rows.len() as i64)
                    .unwrap_or(0);
                summary.new_replies += new_replies;
                summary.leads += leads;
                if found_here {
                    let replies = reply.get("replies").and_then(Value::as_i64).unwrap_or(0);
                    summary.lines.push(format!(
                        "《{title}》读到 {replies} 条回应，新增 {new_replies}，建卡 {leads}"
                    ));
                } else {
                    let reason = extraction
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("第一页上没有找到我们发布的内容");
                    summary.lines.push(format!("《{title}》未读到：{reason}"));
                }
            }
            Err(error) => summary.lines.push(format!("《{title}》写入失败：{error}")),
        }
    }

    if summary.lines.len() > MAX_LINES {
        let omitted = summary.lines.len() - MAX_LINES;
        summary.lines.truncate(MAX_LINES);
        summary.lines.push(format!("另有 {omitted} 条未列出"));
    }
    Ok(summary)
}

/// What the collector read, as the shape `staff.watch.ingest` accepts.
fn ingest_params(content_id: &str, extraction: &Value) -> Value {
    let found = extraction
        .get("found")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut params = json!({ "contentId": content_id, "found": found });
    if !found {
        params["reason"] = json!(extraction
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("第一页上没有找到我们发布的内容"));
        return params;
    }
    for key in ["replies", "views", "likes"] {
        if let Some(number) = extraction
            .get(key)
            .and_then(Value::as_i64)
            .filter(|n| *n >= 0)
        {
            params[key] = json!(number);
        }
    }
    if let Some(comment_id) = extraction
        .get("commentId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    {
        params["commentId"] = json!(comment_id);
    }
    let items: Vec<Value> = extraction
        .get("items")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(sanitize_reply)
                .take(MAX_REPLY_ITEMS)
                .collect()
        })
        .unwrap_or_default();
    params["items"] = json!(items);
    params
}

fn sanitize_reply(row: &Value) -> Option<Value> {
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
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn next_at() -> String {
    (chrono::Utc::now() + chrono::Duration::milliseconds(INTERVAL_MS as i64))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_found_page_is_sent_with_the_numbers_and_comments_it_read() {
        let extraction = json!({
            "found": true,
            "replies": 3,
            "views": 1200,
            "likes": 5,
            "items": [
                { "author": "张三", "text": "有联系方式吗", "at": "2026-09-18" },
                { "author": "", "text": "   " },
            ],
        });
        let params = ingest_params("fa56208e", &extraction);
        assert_eq!(params["contentId"], json!("fa56208e"));
        assert_eq!(params["found"], json!(true));
        assert_eq!(params["replies"], json!(3));
        assert_eq!(params["views"], json!(1200));
        assert_eq!(params["likes"], json!(5));
        let items = params["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["author"], json!("张三"));
        assert_eq!(items[0]["text"], json!("有联系方式吗"));
    }

    #[test]
    fn a_page_without_our_text_is_reported_and_writes_no_numbers() {
        let params = ingest_params("fa56208e", &json!({ "found": false }));
        assert_eq!(params["found"], json!(false));
        assert_eq!(params["reason"], json!("第一页上没有找到我们发布的内容"));
        assert!(params.get("replies").is_none());
        assert!(params.get("items").is_none());
    }

    #[test]
    fn one_page_hands_at_most_the_lead_boards_ceiling_of_comments() {
        let rows: Vec<Value> = (0..MAX_REPLY_ITEMS + 5)
            .map(|index| json!({ "author": format!("u{index}"), "text": format!("reply {index}") }))
            .collect();
        let params = ingest_params("c1", &json!({ "found": true, "items": rows }));
        assert_eq!(params["items"].as_array().unwrap().len(), MAX_REPLY_ITEMS);
    }

    #[test]
    fn the_next_slot_moves_forward_from_now() {
        let next = next_at();
        let parsed = chrono::DateTime::parse_from_rfc3339(&next).expect("iso");
        let now = chrono::Utc::now();
        assert!(parsed.to_utc() >= now + chrono::Duration::minutes(14));
        assert!(parsed.to_utc() <= now + chrono::Duration::minutes(16));
    }
}
