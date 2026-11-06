use super::*;
use serde_json::json;

/// The columns an opportunity card may sit in, in board order.
///
/// The set is closed for the same reason node kinds are: a stage the console
/// cannot draw is a card that disappears, and a card that disappears is a
/// person nobody answers. An owner moves cards between these; a run only ever
/// files one at the front.
pub const LEAD_STAGES: [&str; 8] = [
    "new",
    "qualified",
    "contacted",
    "replied",
    "talking",
    "won",
    "lost",
    "dropped",
];

/// The stages that still mean somebody is worth working.
pub const LEAD_OPEN_STAGES: [&str; 5] = ["new", "qualified", "contacted", "replied", "talking"];

const MAX_LEAD_TEXT_CHARS: usize = 8_192;
const DEFAULT_LEAD_LIMIT: i64 = 200;
const MAX_LEAD_LIMIT: i64 = 1_000;

/// One opportunity on the business board.
///
/// The card exists because somebody said something we can answer, so `quote`
/// and `url` are the point of the row and everything else is filing. Both
/// travel with the card: a claim about demand that cannot be clicked back to
/// the sentence it came from is an opinion, not a lead.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lead {
    pub id: String,
    pub title: String,
    pub site: String,
    pub url: String,
    pub author: String,
    pub quote: String,
    pub topic: String,
    /// One line on how to open, written when the card was filed.
    pub angle: String,
    pub stage: String,
    pub score: i64,
    pub tags: Vec<String>,
    pub note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub employee_id: Option<String>,
    /// The run that filed this card, so the board can cite its evidence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// The employee currently responsible for moving this card.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contacted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replied_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub won_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

const LEAD_SELECT: &str = "SELECT id, title, site, url, author, quote, topic, angle, stage,
            score, tags_json, note, employee_id, run_id, owner_id,
            contacted_at, replied_at, won_at, created_at, updated_at
     FROM staff_leads";

fn lead_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Lead> {
    let tags: String = row.get(10)?;
    Ok(Lead {
        id: row.get(0)?,
        title: row.get(1)?,
        site: row.get(2)?,
        url: row.get(3)?,
        author: row.get(4)?,
        quote: row.get(5)?,
        topic: row.get(6)?,
        angle: row.get(7)?,
        stage: row.get(8)?,
        score: row.get(9)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        note: row.get(11)?,
        employee_id: row.get(12)?,
        run_id: row.get(13)?,
        owner_id: row.get(14)?,
        contacted_at: row.get::<_, Option<i64>>(15)?.map(ms_to_ts),
        replied_at: row.get::<_, Option<i64>>(16)?.map(ms_to_ts),
        won_at: row.get::<_, Option<i64>>(17)?.map(ms_to_ts),
        created_at: ms_to_ts(row.get(18)?),
        updated_at: ms_to_ts(row.get(19)?),
    })
}

/// A list of strings, or nothing when the field is absent.
fn string_list(input: &Value, key: &str) -> Result<Vec<String>> {
    match input.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(value) => value
            .as_array()
            .ok_or_else(|| anyhow!("INVALID_ARGUMENT: {key} must be a list"))?
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .map(str::to_string)
                    .ok_or_else(|| anyhow!("INVALID_ARGUMENT: {key} holds a non-string entry"))
            })
            .collect(),
    }
}

/// The stage a caller named, checked against the closed set.
fn stage_of(input: &Value) -> Result<String> {
    let stage = required_str(input, "stage")?;
    if !LEAD_STAGES.contains(&stage.as_str()) {
        bail!("INVALID_ARGUMENT: unknown lead stage {stage}");
    }
    Ok(stage)
}

/// A caller's confidence in a lead, clamped to the 0..=100 the board draws.
fn score_of(input: &Value, fallback: i64) -> Result<i64> {
    match input.get("score") {
        None | Some(Value::Null) => Ok(fallback),
        Some(value) => {
            let score = value
                .as_i64()
                .ok_or_else(|| anyhow!("INVALID_ARGUMENT: score must be an integer"))?;
            Ok(score.clamp(0, 100))
        }
    }
}

/// A milestone timestamp: set when the flag is on, cleared when it is off,
/// and left alone when the field is absent.
fn milestone(input: &Value, key: &str, existing: Option<i64>, now: i64) -> Option<i64> {
    match input.get(key) {
        Some(Value::Bool(true)) => Some(now),
        Some(Value::Bool(false)) => None,
        _ => existing,
    }
}

fn bound(text: String) -> String {
    if text.chars().count() <= MAX_LEAD_TEXT_CHARS {
        return text;
    }
    text.chars().take(MAX_LEAD_TEXT_CHARS).collect()
}

fn ms_of(value: &Option<String>) -> Option<i64> {
    value.as_deref().map(ts_to_ms)
}

/// Cards the board shows, most recently moved first.
pub fn list_leads(db: &Database, filter: &Value) -> Result<Vec<Lead>> {
    let limit = match filter.get("limit") {
        None | Some(Value::Null) => DEFAULT_LEAD_LIMIT,
        Some(value) => value
            .as_i64()
            .ok_or_else(|| anyhow!("INVALID_ARGUMENT: limit must be an integer"))?
            .clamp(1, MAX_LEAD_LIMIT),
    };
    let stage = optional_str(filter, "stage");
    let site = optional_str(filter, "site");
    let run_id = optional_str(filter, "runId");
    let sql = format!(
        "{LEAD_SELECT}
         WHERE (?1 IS NULL OR stage = ?1)
           AND (?2 IS NULL OR site = ?2)
           AND (?3 IS NULL OR run_id = ?3)
         ORDER BY updated_at DESC, id
         LIMIT ?4"
    );
    let mut stmt = db.conn().prepare(&sql)?;
    let rows = stmt.query_map(params![stage, site, run_id, limit], lead_from_row)?;
    let mut leads = Vec::new();
    for row in rows {
        leads.push(row?);
    }
    Ok(leads)
}

pub fn get_lead(db: &Database, id: &str) -> Result<Option<Lead>> {
    let sql = format!("{LEAD_SELECT} WHERE id = ?1");
    let mut stmt = db.conn().prepare(&sql)?;
    Ok(stmt.query_row(params![id], lead_from_row).optional()?)
}

/// File a card, or update the one whose id was given.
///
/// A run files cards and never moves them: the stage is the owner's answer to
/// "do I want this", so a run that could set it would be claiming a decision
/// nobody made. A run's write therefore keeps whatever stage it finds.
pub fn upsert_lead(db: &Database, input: &Value) -> Result<Lead> {
    let title = required_str(input, "title")?;
    let from_run = input.get("runId").and_then(Value::as_str).is_some();
    let existing = match optional_str(input, "id") {
        Some(id) => get_lead(db, &id)?,
        None => None,
    };
    let lead_id = match &existing {
        Some(lead) => lead.id.clone(),
        None => Uuid::new_v4().to_string(),
    };
    let stage = match &existing {
        Some(lead) if from_run => lead.stage.clone(),
        _ => match input.get("stage") {
            None | Some(Value::Null) => {
                existing.as_ref().map(|l| l.stage.clone()).unwrap_or_else(|| "new".to_string())
            }
            Some(_) => stage_of(input)?,
        },
    };
    let score = score_of(input, existing.as_ref().map(|l| l.score).unwrap_or(0))?;
    let tags = match input.get("tags") {
        None => existing
            .as_ref()
            .map(|lead| lead.tags.clone())
            .unwrap_or_default(),
        Some(_) => string_list(input, "tags")?,
    };
    let now = now_ms();
    let carried = |key: &str, fallback: Option<String>| -> Option<String> {
        match input.get(key) {
            None => fallback,
            Some(Value::Null) => None,
            Some(_) => optional_str(input, key),
        }
    };
    let text = |key: &str, fallback: String| -> String {
        match input.get(key) {
            None => fallback,
            Some(Value::Null) => String::new(),
            Some(_) => optional_str(input, key).unwrap_or_default(),
        }
    };

    db.conn().execute(
        "INSERT INTO staff_leads
           (id, title, site, url, author, quote, topic, angle, stage, score, tags_json, note,
            employee_id, run_id, owner_id, contacted_at, replied_at, won_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
         ON CONFLICT(id) DO UPDATE SET
           title = ?2, site = ?3, url = ?4, author = ?5, quote = ?6, topic = ?7, angle = ?8,
           stage = ?9, score = ?10, tags_json = ?11, note = ?12, employee_id = ?13, run_id = ?14,
           owner_id = ?15, contacted_at = ?16, replied_at = ?17, won_at = ?18, updated_at = ?20",
        params![
            lead_id,
            bound(title),
            text("site", existing.as_ref().map(|l| l.site.clone()).unwrap_or_default()),
            text("url", existing.as_ref().map(|l| l.url.clone()).unwrap_or_default()),
            text("author", existing.as_ref().map(|l| l.author.clone()).unwrap_or_default()),
            bound(text("quote", existing.as_ref().map(|l| l.quote.clone()).unwrap_or_default())),
            text("topic", existing.as_ref().map(|l| l.topic.clone()).unwrap_or_default()),
            bound(text("angle", existing.as_ref().map(|l| l.angle.clone()).unwrap_or_default())),
            stage,
            score,
            serde_json::to_string(&tags)?,
            bound(text("note", existing.as_ref().map(|l| l.note.clone()).unwrap_or_default())),
            carried("employeeId", existing.as_ref().and_then(|l| l.employee_id.clone())),
            carried("runId", existing.as_ref().and_then(|l| l.run_id.clone())),
            carried("ownerId", existing.as_ref().and_then(|l| l.owner_id.clone())),
            milestone(input, "contacted", ms_of(&existing.as_ref().and_then(|l| l.contacted_at.clone())), now),
            milestone(input, "replied", ms_of(&existing.as_ref().and_then(|l| l.replied_at.clone())), now),
            milestone(input, "won", ms_of(&existing.as_ref().and_then(|l| l.won_at.clone())), now),
            existing
                .as_ref()
                .map(|lead| ts_to_ms(&lead.created_at))
                .unwrap_or(now),
            now,
        ],
    )?;
    get_lead(db, &lead_id)?.ok_or_else(|| anyhow!("NOT_FOUND: lead {lead_id} vanished after write"))
}

/// Move a card to another column.
///
/// The move is the owner's, and the timestamps it sets are the ones the daily
/// board counts: reaching "contacted" is the moment we spoke, "replied" the
/// moment they did, "won" the moment it became revenue.
pub fn move_lead(db: &Database, input: &Value) -> Result<Option<Lead>> {
    let lead_id = required_str(input, "id")?;
    let Some(current) = get_lead(db, &lead_id)? else {
        return Ok(None);
    };
    let stage = stage_of(input)?;
    let note = optional_str(input, "note").unwrap_or(current.note);
    let owner_id = match input.get("ownerId") {
        None => current.owner_id,
        Some(Value::Null) => None,
        Some(_) => optional_str(input, "ownerId"),
    };
    let now = now_ms();
    db.conn().execute(
        "UPDATE staff_leads SET stage = ?2, note = ?3, owner_id = ?4,
           contacted_at = ?5, replied_at = ?6, won_at = ?7, updated_at = ?8
         WHERE id = ?1",
        params![
            lead_id,
            stage,
            note,
            owner_id,
            if stage == "contacted" { Some(now) } else { ms_of(&current.contacted_at) },
            if stage == "replied" { Some(now) } else { ms_of(&current.replied_at) },
            if stage == "won" { Some(now) } else { ms_of(&current.won_at) },
            now,
        ],
    )?;
    get_lead(db, &lead_id)
}

/// How many cards sit in each column, for the board's headers.
pub fn lead_stage_counts(db: &Database) -> Result<Value> {
    let mut stmt = db
        .conn()
        .prepare("SELECT stage, COUNT(*) FROM staff_leads GROUP BY stage")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut counts = serde_json::Map::new();
    for stage in LEAD_STAGES {
        counts.insert(stage.to_string(), json!(0));
    }
    for row in rows {
        let (stage, count) = row?;
        counts.insert(stage, json!(count));
    }
    Ok(Value::Object(counts))
}
