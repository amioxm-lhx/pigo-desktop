//! The return leg: what happened after we spoke.
//!
//! Everything above this file moves one way -- a sweep reads a site, an
//! employee writes a draft, the owner approves it, and whoever holds the
//! account publishes it. The numbers that decide what to do next (which topic
//! is worth another piece, whether a stranger answered) were facts nobody
//! collected: the console asked the owner to type them in, and the boards that
//! judge a topic read `staff_metrics`, which had no writer at all. A company
//! whose only feedback is a form its owner forgets to fill is open-loop.
//!
//! So this file owns two things:
//!
//! - Which published pieces are worth looking at again (`watch_targets`), and
//!   what a collector should look for on the page -- a fingerprint of the body
//!   we published, because the site never told us our own comment id.
//! - Turning what the page says into rows (`ingest_watch`): the piece's own
//!   reply count, today's trend for its topic, and one card per stranger who
//!   answered, because an answer from a stranger is a lead whoever wrote it.
//!
//! Two rules keep this honest. The page is the source of truth for a count, so
//! a number that went down is written down rather than subtracted (a deleted
//! reply is not negative demand). And a reply becomes a card exactly once: the
//! card's own (site, author, quote) is the key, so a watch that runs every ten
//! minutes does not fill the board with the same person.

use super::*;
use serde_json::json;

/// How many characters of the published body identify it on the page.
///
/// Long enough that two comments under the same article cannot collide, short
/// enough to survive the site trimming or re-wrapping the tail.
const FINGERPRINT_CHARS: usize = 20;

/// How many targets one list call hands back. A watch pass loads each of these
/// pages one at a time, so this is also the price of a pass.
pub const DEFAULT_WATCH_TARGETS: i64 = 8;
pub const MAX_WATCH_TARGETS: i64 = 50;

/// One published piece, as a collector needs to hear about it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchTarget {
    pub content_id: String,
    pub title: String,
    pub channel: String,
    pub url: String,
    pub topic: String,
    /// The head of the body, whitespace collapsed: what to look for.
    pub fingerprint: String,
    /// The reply count the board already holds, for the "did it move" line.
    pub replies: i64,
    pub published_at: Option<String>,
}

/// Whitespace-collapsed head of a body. The page renders our paragraph breaks
/// as spaces and newlines interchangeably, so the only stable part of a text is
/// its characters in order.
pub fn fingerprint_of(body: &str) -> String {
    body.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(FINGERPRINT_CHARS)
        .collect()
}

fn target_limit(input: &Value) -> Result<i64> {
    let limit = match input.get("limit") {
        None | Some(Value::Null) => DEFAULT_WATCH_TARGETS,
        Some(value) => value
            .as_i64()
            .ok_or_else(|| anyhow!("INVALID_ARGUMENT: limit must be an integer"))?,
    };
    Ok(limit.clamp(1, MAX_WATCH_TARGETS))
}

/// Published pieces that carry a URL, oldest-updated first.
///
/// A piece with no URL cannot be looked at: the owner published it somewhere
/// this company was never told about. Those rows are still listed by the
/// console -- they are the ones whose numbers have to be typed in by hand.
pub fn watch_targets(db: &Database, input: &Value) -> Result<Vec<WatchTarget>> {
    let limit = target_limit(input)?;
    let mut stmt = db.conn().prepare(
        "SELECT id, title, channel, topic, body, published_url, replies, published_at
           FROM staff_contents
          WHERE status = 'published'
            AND TRIM(published_url) <> ''
          ORDER BY COALESCE(published_at, 0) ASC, updated_at ASC
          LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        let body: String = row.get(4)?;
        Ok(WatchTarget {
            content_id: row.get(0)?,
            title: row.get(1)?,
            channel: row.get(2)?,
            topic: row.get(3)?,
            url: row.get(5)?,
            fingerprint: fingerprint_of(&body),
            replies: row.get(6)?,
            published_at: row.get::<_, Option<i64>>(7)?.map(ms_to_ts),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// What the console says about the watch: how much of it there is to do.
pub fn watch_summary(db: &Database, input: &Value) -> Result<Value> {
    let limit = target_limit(input)?;
    let published: i64 = db.conn().query_row(
        "SELECT COUNT(*) FROM staff_contents WHERE status = 'published'",
        [],
        |row| row.get(0),
    )?;
    let watchable: i64 = db.conn().query_row(
        "SELECT COUNT(*) FROM staff_contents
          WHERE status = 'published' AND TRIM(published_url) <> ''",
        [],
        |row| row.get(0),
    )?;
    let replies: i64 = db.conn().query_row(
        "SELECT COALESCE(SUM(replies), 0) FROM staff_contents
          WHERE status = 'published' AND TRIM(published_url) <> ''",
        [],
        |row| row.get(0),
    )?;
    Ok(json!({
        "published": published,
        "watchable": watchable,
        "unwatchable": published - watchable,
        "replies": replies,
        "targets": watch_targets(db, input)?.len(),
        "limit": limit,
    }))
}

/// Today's trend row for one topic, moved by a delta rather than replaced.
///
/// The board sums a topic over several days, so a day's row has to be that
/// day's movement. Writing the running total instead would make three days of
/// history read as three times the same number.
fn accumulate_metric(
    db: &Database,
    day: &str,
    label: &str,
    views_delta: i64,
    replies_delta: i64,
) -> Result<()> {
    if views_delta == 0 && replies_delta == 0 {
        return Ok(());
    }
    let now = now_ms();
    db.conn().execute(
        "INSERT INTO staff_metrics
           (id, day, dimension, label, views, replies, leads, inquiries, deals, created_at, updated_at)
         VALUES (?1, ?2, 'topic', ?3, ?4, ?5, 0, 0, 0, ?6, ?6)
         ON CONFLICT(day, dimension, label) DO UPDATE SET
           views = views + ?4, replies = replies + ?5, updated_at = ?6",
        params![
            Uuid::new_v4().to_string(),
            day,
            label,
            views_delta,
            replies_delta,
            now
        ],
    )?;
    Ok(())
}

/// The card a reply would have filed, if it is already on the board.
///
/// The key is the stranger's own words: the same person answering twice with
/// the same sentence is one answer, and a watch that runs all day must not turn
/// it into ten cards. A reply that was later edited into different words is a
/// second card, which is the honest reading -- we cannot tell an edit from a
/// new sentence, and filing both is cheaper than losing one.
fn reply_lead_id(db: &Database, site: &str, author: &str, quote: &str) -> Result<Option<String>> {
    let mut stmt = db.conn().prepare(
        "SELECT id FROM staff_leads
          WHERE site = ?1 AND author = ?2 AND quote = ?3
          LIMIT 1",
    )?;
    let mut rows = stmt.query(params![site, author, quote])?;
    Ok(match rows.next()? {
        Some(row) => Some(row.get(0)?),
        None => None,
    })
}

fn items_of(input: &Value) -> Result<Vec<Value>> {
    match input.get("items") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(value) => value
            .as_array()
            .map(|items| items.clone())
            .ok_or_else(|| anyhow!("INVALID_ARGUMENT: items must be a list")),
    }
}

/// What a collector found on the page, written down.
///
/// `found: false` is an ordinary answer, not an error: a comment can be pushed
/// off the first page, folded behind a "more comments" link, or deleted by the
/// site. Nothing is written in that case, because a zero that was never read is
/// worse than a number that is missing -- it reads as "nobody answered".
pub fn ingest_watch(db: &Database, input: &Value) -> Result<Value> {
    let content_id = required_str(input, "contentId")?;
    let content = get_content(db, &content_id)?
        .ok_or_else(|| anyhow!("NOT_FOUND: content {content_id}"))?;
    let found = input.get("found").and_then(Value::as_bool).unwrap_or(false);
    if !found {
        return Ok(json!({
            "contentId": content_id,
            "found": false,
            "newReplies": 0,
            "leads": [],
            "reason": optional_str(input, "reason").unwrap_or_else(|| "not on the first page".to_string()),
        }));
    }

    let reported = match input.get("replies") {
        Some(value) => value
            .as_i64()
            .ok_or_else(|| anyhow!("INVALID_ARGUMENT: replies must be an integer"))?
            .max(0),
        None => content.replies,
    };
    let views = match input.get("views") {
        Some(value) => value
            .as_i64()
            .ok_or_else(|| anyhow!("INVALID_ARGUMENT: views must be an integer"))?
            .max(0),
        None => content.views,
    };
    let now = now_ms();
    db.conn().execute(
        "UPDATE staff_contents SET replies = ?2, views = ?3, updated_at = ?4 WHERE id = ?1",
        params![content_id, reported, views, now],
    )?;

    let replies_delta = (reported - content.replies).max(0);
    let views_delta = (views - content.views).max(0);
    let topic = content.topic.trim().to_string();
    if !topic.is_empty() {
        accumulate_metric(
            db,
            &local_day(now),
            &topic,
            views_delta,
            replies_delta,
        )?;
    }

    let site = if content.channel.trim().is_empty() {
        "manual".to_string()
    } else {
        content.channel.clone()
    };
    let mut filed: Vec<String> = Vec::new();
    for item in items_of(input)? {
        let author = optional_str(&item, "author").unwrap_or_else(|| "知乎用户".to_string());
        let text = optional_str(&item, "text").unwrap_or_default();
        let quote: String = text.chars().take(280).collect();
        if quote.is_empty() {
            continue;
        }
        if reply_lead_id(db, &site, &author, &quote)?.is_some() {
            continue;
        }
        let at = optional_str(&item, "at");
        let lead = upsert_lead(
            db,
            &json!({
                "title": format!("{} 回复了《{}》", author, content.title),
                "site": site,
                "url": content.published_url,
                "author": author,
                "quote": quote,
                "topic": content.topic,
                "angle": "对方主动回了话，值得先回一句再谈别的。",
                "stage": "replied",
                "score": 70,
                "tags": ["客户回音", "自动采集"],
                "replied": true,
                "note": format!(
                    "回音巡检自动建卡：{} 在《{}》下回复了我们的稿子{}{}",
                    author,
                    content.title,
                    if let Some(at) = &at { format!("（{}）", at) } else { String::new() },
                    if content.published_url.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", content.published_url)
                    }
                ),
            }),
        )?;
        filed.push(lead.id);
    }

    Ok(json!({
        "contentId": content_id,
        "found": true,
        "commentId": optional_str(input, "commentId"),
        "likes": match input.get("likes") {
            Some(value) => value.as_i64().unwrap_or(0).max(0),
            None => 0,
        },
        "replies": reported,
        "newReplies": replies_delta,
        "leads": filed,
    }))
}
