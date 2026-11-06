use super::*;
use chrono::{Offset, TimeZone};
use serde_json::json;

/// What a row's label counts. Every number the daily board ranks is one of
/// these three, so "which topic is working" and "which channel is working" are
/// the same query with a different word.
pub const METRIC_DIMENSIONS: [&str; 3] = ["topic", "channel", "content"];

/// How many days of history the board reads back by default.
const DEFAULT_TREND_DAYS: i64 = 14;
const MAX_TREND_DAYS: i64 = 120;
const DEFAULT_METRIC_LIMIT: i64 = 2_000;
const MAX_METRIC_LIMIT: i64 = 20_000;

/// One day's numbers for one label.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metric {
    pub id: String,
    /// The local calendar day, `YYYY-MM-DD`.
    pub day: String,
    pub dimension: String,
    pub label: String,
    pub views: i64,
    pub replies: i64,
    pub leads: i64,
    pub inquiries: i64,
    pub deals: i64,
    pub created_at: String,
    pub updated_at: String,
}

const METRIC_SELECT: &str = "SELECT id, day, dimension, label, views, replies, leads, inquiries,
            deals, created_at, updated_at
     FROM staff_metrics";

fn metric_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Metric> {
    Ok(Metric {
        id: row.get(0)?,
        day: row.get(1)?,
        dimension: row.get(2)?,
        label: row.get(3)?,
        views: row.get(4)?,
        replies: row.get(5)?,
        leads: row.get(6)?,
        inquiries: row.get(7)?,
        deals: row.get(8)?,
        created_at: ms_to_ts(row.get(9)?),
        updated_at: ms_to_ts(row.get(10)?),
    })
}

/// The offset the owner's clock sits at, in minutes.
///
/// A day is a local day: a 09:00 post and the reply it earns at 23:00 belong to
/// the same board, and a run that filed them against UTC would split the pair
/// across two. The offset is read when the board is queried rather than stored,
/// so a machine that travels does not keep reporting yesterday.
fn local_offset_minutes() -> i64 {
    i64::from(chrono::Local::now().offset().fix().local_minus_utc()) / 60
}

/// The local day a moment falls in.
pub fn local_day(ms: i64) -> String {
    let offset = chrono::FixedOffset::east_opt((local_offset_minutes() * 60) as i32)
        .unwrap_or_else(|| chrono::FixedOffset::east_opt(0).unwrap());
    chrono::Utc
        .timestamp_millis_opt(ms)
        .single()
        .map(|utc| utc.with_timezone(&offset).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".to_string())
}

/// The half-open millisecond range a local day covers.
pub fn day_bounds(day: &str) -> Result<(i64, i64)> {
    let date = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map_err(|_| anyhow!("INVALID_ARGUMENT: day must be YYYY-MM-DD, got {day}"))?;
    let midnight = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| anyhow!("INVALID_ARGUMENT: {day} has no midnight"))?;
    let start = midnight.and_utc().timestamp_millis() - local_offset_minutes() * 60_000;
    Ok((start, start + 86_400_000))
}

/// How many days back a request asked for.
fn trend_days(input: &Value) -> Result<i64> {
    match input.get("days") {
        None | Some(Value::Null) => Ok(DEFAULT_TREND_DAYS),
        Some(value) => Ok(value
            .as_i64()
            .ok_or_else(|| anyhow!("INVALID_ARGUMENT: days must be an integer"))?
            .clamp(1, MAX_TREND_DAYS)),
    }
}

fn dimension_of(input: &Value) -> Result<String> {
    let dimension = required_str(input, "dimension")?;
    if !METRIC_DIMENSIONS.contains(&dimension.as_str()) {
        bail!("INVALID_ARGUMENT: unknown metric dimension {dimension}");
    }
    Ok(dimension)
}

fn count_of(input: &Value, key: &str, fallback: i64) -> Result<i64> {
    match input.get(key) {
        None | Some(Value::Null) => Ok(fallback),
        Some(value) => Ok(value
            .as_i64()
            .ok_or_else(|| anyhow!("INVALID_ARGUMENT: {key} must be an integer"))?
            .max(0)),
    }
}

/// Rows the board reads back, newest day first.
pub fn list_metrics(db: &Database, filter: &Value) -> Result<Vec<Metric>> {
    let limit = match filter.get("limit") {
        None | Some(Value::Null) => DEFAULT_METRIC_LIMIT,
        Some(value) => value
            .as_i64()
            .ok_or_else(|| anyhow!("INVALID_ARGUMENT: limit must be an integer"))?
            .clamp(1, MAX_METRIC_LIMIT),
    };
    let dimension = optional_str(filter, "dimension");
    let since = optional_str(filter, "since");
    let day = optional_str(filter, "day");
    let sql = format!(
        "{METRIC_SELECT}
         WHERE (?1 IS NULL OR dimension = ?1)
           AND (?2 IS NULL OR day >= ?2)
           AND (?3 IS NULL OR day = ?3)
         ORDER BY day DESC, views DESC, label
         LIMIT ?4"
    );
    let mut stmt = db.conn().prepare(&sql)?;
    let rows = stmt.query_map(params![dimension, since, day, limit], metric_from_row)?;
    let mut metrics = Vec::new();
    for row in rows {
        metrics.push(row?);
    }
    Ok(metrics)
}

/// Record one day's numbers for one label.
///
/// The same day and label overwrite rather than accumulate: a sweep reads the
/// count a site prints, and adding two readings of the same counter would
/// double it. A caller that means to add has to read first.
pub fn upsert_metric(db: &Database, input: &Value) -> Result<Metric> {
    let day = match optional_str(input, "day") {
        Some(day) => day,
        None => local_day(now_ms()),
    };
    day_bounds(&day)?;
    let dimension = match input.get("dimension") {
        None | Some(Value::Null) => "topic".to_string(),
        Some(_) => dimension_of(input)?,
    };
    let label = required_str(input, "label")?;
    let now = now_ms();
    db.conn().execute(
        "INSERT INTO staff_metrics
           (id, day, dimension, label, views, replies, leads, inquiries, deals, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
         ON CONFLICT(day, dimension, label) DO UPDATE SET
           views = ?5, replies = ?6, leads = ?7, inquiries = ?8, deals = ?9, updated_at = ?10",
        params![
            Uuid::new_v4().to_string(),
            day,
            dimension,
            label,
            count_of(input, "views", 0)?,
            count_of(input, "replies", 0)?,
            count_of(input, "leads", 0)?,
            count_of(input, "inquiries", 0)?,
            count_of(input, "deals", 0)?,
            now,
        ],
    )?;
    let sql = format!("{METRIC_SELECT} WHERE day = ?1 AND dimension = ?2 AND label = ?3");
    let mut stmt = db.conn().prepare(&sql)?;
    Ok(stmt.query_row(params![day, dimension, label], metric_from_row)?)
}

/// One day's funnel, read straight off the rows the boards hold.
///
/// Nothing here is stored as a total: every number is a count of cards that
/// already exist, so the board cannot disagree with the board it summarizes.
fn day_funnel(db: &Database, day: &str) -> Result<Value> {
    let (start, end) = day_bounds(day)?;
    let count = |sql: &str, column: &str| -> Result<i64> {
        Ok(db
            .conn()
            .query_row(sql, params![start, end], |row| row.get::<_, i64>(0))
            .with_context(|| format!("count {column} for {day}"))?)
    };
    let leads = count(
        "SELECT COUNT(*) FROM staff_leads WHERE created_at >= ?1 AND created_at < ?2",
        "leads filed",
    )?;
    let contacted = count(
        "SELECT COUNT(*) FROM staff_leads WHERE contacted_at >= ?1 AND contacted_at < ?2",
        "leads contacted",
    )?;
    let replied = count(
        "SELECT COUNT(*) FROM staff_leads WHERE replied_at >= ?1 AND replied_at < ?2",
        "leads replied",
    )?;
    let won = count(
        "SELECT COUNT(*) FROM staff_leads WHERE won_at >= ?1 AND won_at < ?2",
        "leads won",
    )?;
    let approved = count(
        "SELECT COUNT(*) FROM staff_contents WHERE approved_at >= ?1 AND approved_at < ?2",
        "contents approved",
    )?;
    let published = count(
        "SELECT COUNT(*) FROM staff_contents WHERE published_at >= ?1 AND published_at < ?2",
        "contents published",
    )?;
    let (views, replies_seen) = db.conn().query_row(
        "SELECT COALESCE(SUM(views), 0), COALESCE(SUM(replies), 0) FROM staff_metrics WHERE day = ?1",
        params![day],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
    )?;
    Ok(json!({
        "day": day,
        "leads": leads,
        "contacted": contacted,
        "replied": replied,
        "won": won,
        "approved": approved,
        "published": published,
        "views": views,
        "replies": replies_seen,
    }))
}

/// Everything the daily board draws, in one answer.
///
/// The console asks once: which day it is, what happened that day, what the
/// day before looked like, which topics rank highest, and how the last two
/// weeks moved. Splitting these into four calls would let the page render a
/// ranking that contradicts its own funnel.
pub fn report_summary(db: &Database, input: &Value) -> Result<Value> {
    let days = trend_days(input)?;
    let today = match optional_str(input, "day") {
        Some(day) => day,
        None => local_day(now_ms()),
    };
    let (start, _) = day_bounds(&today)?;
    let yesterday = local_day(start - 1);

    let topic_since = local_day(start - (days - 1) * 86_400_000);
    let mut stmt = db.conn().prepare(
        "SELECT day,
                COALESCE(SUM(views), 0), COALESCE(SUM(replies), 0),
                COALESCE(SUM(leads), 0), COALESCE(SUM(inquiries), 0), COALESCE(SUM(deals), 0)
           FROM staff_metrics
          WHERE day >= ?1
          GROUP BY day
          ORDER BY day",
    )?;
    let rows = stmt.query_map(params![topic_since], |row| {
        Ok(json!({
            "day": row.get::<_, String>(0)?,
            "views": row.get::<_, i64>(1)?,
            "replies": row.get::<_, i64>(2)?,
            "leads": row.get::<_, i64>(3)?,
            "inquiries": row.get::<_, i64>(4)?,
            "deals": row.get::<_, i64>(5)?,
        }))
    })?;
    let mut trend = Vec::new();
    for row in rows {
        trend.push(row?);
    }

    let mut stmt = db.conn().prepare(
        "SELECT label,
                COALESCE(SUM(views), 0), COALESCE(SUM(replies), 0),
                COALESCE(SUM(leads), 0), COALESCE(SUM(inquiries), 0), COALESCE(SUM(deals), 0)
           FROM staff_metrics
          WHERE dimension = 'topic' AND day >= ?1
          GROUP BY label
          ORDER BY SUM(views) DESC, SUM(replies) DESC, label
          LIMIT 20",
    )?;
    let rows = stmt.query_map(params![topic_since], |row| {
        Ok(json!({
            "label": row.get::<_, String>(0)?,
            "views": row.get::<_, i64>(1)?,
            "replies": row.get::<_, i64>(2)?,
            "leads": row.get::<_, i64>(3)?,
            "inquiries": row.get::<_, i64>(4)?,
            "deals": row.get::<_, i64>(5)?,
        }))
    })?;
    let mut topics = Vec::new();
    for row in rows {
        topics.push(row?);
    }

    let mut stmt = db
        .conn()
        .prepare("SELECT stage, COUNT(*) FROM staff_leads GROUP BY stage")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut pipeline = serde_json::Map::new();
    for (stage, count) in rows.flatten() {
        pipeline.insert(stage, json!(count));
    }

    Ok(json!({
        "day": today,
        "today": day_funnel(db, &today)?,
        "yesterday": day_funnel(db, &yesterday)?,
        "topics": topics,
        "trend": trend,
        "pipeline": Value::Object(pipeline),
        "days": days,
    }))
}
