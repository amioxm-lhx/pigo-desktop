use super::*;
use serde_json::json;

/// Where a draft is in its life.
///
/// A draft is written by an employee, approved by the owner, and published by
/// whoever holds the account. Splitting `approved` from `published` is the
/// whole point of the board: the owner's decision and the act of speaking are
/// different events, and only one of them needs a human.
///
/// `queued` is the gap between those two events when the act of speaking is a
/// worker's job. The owner answers "say it", the piece is handed to the poster,
/// and the poster publishes it on its own round -- so a piece can be decided
/// without anyone waiting on the owner to paste an address, and it is still not
/// counted as said until the poster has said it.
pub const CONTENT_STATUSES: [&str; 6] =
    ["draft", "review", "approved", "queued", "published", "retired"];

/// The statuses a card can be moved between by hand, in board order.
pub const CONTENT_OPEN_STATUSES: [&str; 3] = ["draft", "review", "approved"];

const MAX_CONTENT_BODY_CHARS: usize = 32_768;
const DEFAULT_CONTENT_LIMIT: i64 = 200;
const MAX_CONTENT_LIMIT: i64 = 1_000;

/// One thing we intend to say somewhere.
///
/// A content row outlives the run that drafted it: the draft is edited,
/// approved, published, and then measured. `views` and `replies` are facts
/// about the world rather than intentions, which is why they are columns the
/// daily board reads back rather than prose in a memory item.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub id: String,
    pub title: String,
    pub channel: String,
    pub topic: String,
    pub body: String,
    /// The slot this draft fills in an A/B pair, e.g. "A" or "B".
    pub variant: String,
    pub status: String,
    /// The card this draft answers, when it was written for a specific person.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lead_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub employee_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    pub published_url: String,
    pub views: i64,
    pub replies: i64,
    pub created_at: String,
    pub updated_at: String,
}

const CONTENT_SELECT: &str = "SELECT id, title, channel, topic, body, variant, status, lead_id,
            employee_id, run_id, approved_at, published_at, published_url, views, replies,
            created_at, updated_at
     FROM staff_contents";

fn content_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Content> {
    Ok(Content {
        id: row.get(0)?,
        title: row.get(1)?,
        channel: row.get(2)?,
        topic: row.get(3)?,
        body: row.get(4)?,
        variant: row.get(5)?,
        status: row.get(6)?,
        lead_id: row.get(7)?,
        employee_id: row.get(8)?,
        run_id: row.get(9)?,
        approved_at: row.get::<_, Option<i64>>(10)?.map(ms_to_ts),
        published_at: row.get::<_, Option<i64>>(11)?.map(ms_to_ts),
        published_url: row.get(12)?,
        views: row.get(13)?,
        replies: row.get(14)?,
        created_at: ms_to_ts(row.get(15)?),
        updated_at: ms_to_ts(row.get(16)?),
    })
}

fn status_of(input: &Value) -> Result<String> {
    let status = required_str(input, "status")?;
    if !CONTENT_STATUSES.contains(&status.as_str()) {
        bail!("INVALID_ARGUMENT: unknown content status {status}");
    }
    Ok(status)
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

fn bound(text: String) -> String {
    if text.chars().count() <= MAX_CONTENT_BODY_CHARS {
        return text;
    }
    text.chars().take(MAX_CONTENT_BODY_CHARS).collect()
}

fn ms_of(value: &Option<String>) -> Option<i64> {
    value.as_deref().map(ts_to_ms)
}

/// The drafts the board shows, most recently touched first.
pub fn list_contents(db: &Database, filter: &Value) -> Result<Vec<Content>> {
    let limit = match filter.get("limit") {
        None | Some(Value::Null) => DEFAULT_CONTENT_LIMIT,
        Some(value) => value
            .as_i64()
            .ok_or_else(|| anyhow!("INVALID_ARGUMENT: limit must be an integer"))?
            .clamp(1, MAX_CONTENT_LIMIT),
    };
    let status = optional_str(filter, "status");
    let channel = optional_str(filter, "channel");
    let topic = optional_str(filter, "topic");
    let lead_id = optional_str(filter, "leadId");
    let run_id = optional_str(filter, "runId");
    let sql = format!(
        "{CONTENT_SELECT}
         WHERE (?1 IS NULL OR status = ?1)
           AND (?2 IS NULL OR channel = ?2)
           AND (?3 IS NULL OR topic = ?3)
           AND (?4 IS NULL OR lead_id = ?4)
           AND (?5 IS NULL OR run_id = ?5)
         ORDER BY updated_at DESC, id
         LIMIT ?6"
    );
    let mut stmt = db.conn().prepare(&sql)?;
    let rows = stmt.query_map(
        params![status, channel, topic, lead_id, run_id, limit],
        content_from_row,
    )?;
    let mut contents = Vec::new();
    for row in rows {
        contents.push(row?);
    }
    Ok(contents)
}

pub fn get_content(db: &Database, id: &str) -> Result<Option<Content>> {
    let sql = format!("{CONTENT_SELECT} WHERE id = ?1");
    let mut stmt = db.conn().prepare(&sql)?;
    Ok(stmt.query_row(params![id], content_from_row).optional()?)
}

/// Draft or revise a piece of writing.
///
/// Approval is deliberately not reachable from here: `set_content_status` is
/// the owner's door and this one is the writing door, so no run can approve
/// its own draft by writing it back with a new status.
pub fn upsert_content(db: &Database, input: &Value) -> Result<Content> {
    let title = required_str(input, "title")?;
    let existing = match optional_str(input, "id") {
        Some(id) => get_content(db, &id)?,
        None => None,
    };
    let content_id = match &existing {
        Some(content) => content.id.clone(),
        None => Uuid::new_v4().to_string(),
    };
    let now = now_ms();
    let from_run = input.get("runId").and_then(Value::as_str).is_some();
    // A run writes drafts and never approves its own work: whatever status it
    // asks for, an unattended run leaves the piece where a human still has to
    // look at it. Approving is the owner's door, and this is not it.
    let asked = optional_str(input, "status")
        .map(|_| status_of(input))
        .transpose()?
        .unwrap_or_else(|| {
            existing
                .as_ref()
                .map(|content| content.status.clone())
                .unwrap_or_else(|| "draft".to_string())
        });
    let status = match &existing {
        Some(content) if from_run => content.status.clone(),
        None if from_run => "draft".to_string(),
        _ if from_run && asked != "draft" && asked != "review" => "draft".to_string(),
        _ => asked,
    };
    let text = |key: &str, fallback: String| -> String {
        match input.get(key) {
            None => fallback,
            Some(Value::Null) => String::new(),
            Some(_) => optional_str(input, key).unwrap_or_default(),
        }
    };
    let carried = |key: &str, fallback: Option<String>| -> Option<String> {
        match input.get(key) {
            None => fallback,
            Some(Value::Null) => None,
            Some(_) => optional_str(input, key),
        }
    };
    let views = count_of(input, "views", existing.as_ref().map(|c| c.views).unwrap_or(0))?;
    let replies = count_of(input, "replies", existing.as_ref().map(|c| c.replies).unwrap_or(0))?;

    db.conn().execute(
        "INSERT INTO staff_contents
           (id, title, channel, topic, body, variant, status, lead_id, employee_id, run_id,
            approved_at, published_at, published_url, views, replies, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
         ON CONFLICT(id) DO UPDATE SET
           title = ?2, channel = ?3, topic = ?4, body = ?5, variant = ?6, status = ?7,
           lead_id = ?8, employee_id = ?9, run_id = ?10, approved_at = ?11, published_at = ?12,
           published_url = ?13, views = ?14, replies = ?15, updated_at = ?17",
        params![
            content_id,
            bound(title),
            text("channel", existing.as_ref().map(|c| c.channel.clone()).unwrap_or_default()),
            text("topic", existing.as_ref().map(|c| c.topic.clone()).unwrap_or_default()),
            bound(text("body", existing.as_ref().map(|c| c.body.clone()).unwrap_or_default())),
            text("variant", existing.as_ref().map(|c| c.variant.clone()).unwrap_or_default()),
            status,
            carried("leadId", existing.as_ref().and_then(|c| c.lead_id.clone())),
            carried("employeeId", existing.as_ref().and_then(|c| c.employee_id.clone())),
            carried("runId", existing.as_ref().and_then(|c| c.run_id.clone())),
            ms_of(&existing.as_ref().and_then(|c| c.approved_at.clone())),
            ms_of(&existing.as_ref().and_then(|c| c.published_at.clone())),
            text(
                "publishedUrl",
                existing.as_ref().map(|c| c.published_url.clone()).unwrap_or_default()
            ),
            views,
            replies,
            existing
                .as_ref()
                .map(|content| ts_to_ms(&content.created_at))
                .unwrap_or(now),
            now,
        ],
    )?;
    get_content(db, &content_id)?
        .ok_or_else(|| anyhow!("NOT_FOUND: content {content_id} vanished after write"))
}

/// Move a draft to another status: the owner's door.
///
/// Approving stamps the moment the owner said yes, publishing stamps the
/// moment it went out, and both are what the daily board counts.
pub fn set_content_status(db: &Database, input: &Value) -> Result<Option<Content>> {
    let content_id = required_str(input, "id")?;
    let Some(current) = get_content(db, &content_id)? else {
        return Ok(None);
    };
    let status = status_of(input)?;
    let now = now_ms();
    // Handing a piece over is part of approving it, not a second approval:
    // the moment the owner said yes is the moment they said it, and the
    // poster's round must not be able to move it.
    let approved = match status.as_str() {
        "approved" | "queued" | "published" => ms_of(&current.approved_at).or(Some(now)),
        _ => ms_of(&current.approved_at),
    };
    let published = if status == "published" {
        ms_of(&current.published_at).or(Some(now))
    } else {
        ms_of(&current.published_at)
    };
    let body = optional_str(input, "body").map(bound).unwrap_or(current.body);
    let published_url = optional_str(input, "publishedUrl").unwrap_or(current.published_url);
    let channel = optional_str(input, "channel").unwrap_or(current.channel);
    db.conn().execute(
        "UPDATE staff_contents SET status = ?2, body = ?3, published_url = ?4, channel = ?5,
           approved_at = ?6, published_at = ?7, updated_at = ?8
         WHERE id = ?1",
        params![
            content_id,
            status,
            body,
            published_url,
            channel,
            approved,
            published,
            now
        ],
    )?;
    get_content(db, &content_id)
}

/// How many drafts sit in each status, for the board's headers.
pub fn content_status_counts(db: &Database) -> Result<Value> {
    let mut stmt = db
        .conn()
        .prepare("SELECT status, COUNT(*) FROM staff_contents GROUP BY status")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut counts = serde_json::Map::new();
    for status in CONTENT_STATUSES {
        counts.insert(status.to_string(), json!(0));
    }
    for row in rows {
        let (status, count) = row?;
        counts.insert(status, json!(count));
    }
    Ok(Value::Object(counts))
}
