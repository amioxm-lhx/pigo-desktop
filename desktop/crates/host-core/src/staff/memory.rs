use super::*;
use std::collections::HashSet;

/// What a memory item asserts. `constraint` and `playbook` are not ordinary
/// rows: see the write rules on `write_memory` and `publish_memory`.
pub const MEMORY_KINDS: [&str; 5] = ["fact", "preference", "playbook", "constraint", "signal"];

/// `employee` items are private to one employee. `shared` items are readable
/// by every employee and are only reachable through the promotion gate.
pub const MEMORY_SCOPES: [&str; 2] = ["employee", "shared"];

pub const MEMORY_STATUSES: [&str; 3] = ["active", "superseded", "retired"];

/// Where an item came from. `user` is the only author allowed to write a
/// `constraint`, and the only one the promotion gate does not second-guess.
pub const MEMORY_SOURCE_KINDS: [&str; 4] = ["user", "interaction", "run", "url"];

const MAX_MEMORY_TEXT_CHARS: usize = 16_384;
const MAX_MEMORY_KEY_CHARS: usize = 256;
const DEFAULT_MEMORY_LIMIT: i64 = 20;
const MAX_MEMORY_LIMIT: i64 = 200;

/// One memory item, as returned to every caller.
///
/// Physical columns are `memory_key` / `content` so the SQL stays clear of
/// keyword-adjacent names; the wire keeps the `key` / `text` names that
/// 03-runtime/20 documents, because the renderer and the workflow editor read
/// them directly.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryItem {
    pub id: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    pub kind: String,
    #[serde(rename = "key")]
    pub memory_key: String,
    #[serde(rename = "text")]
    pub content: String,
    pub source_kind: String,
    pub source_ref: String,
    pub confidence: f64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub employee_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// True when another live item shares this `key` with different text. The
    /// pair is surfaced instead of resolved: unattended runs have no one to
    /// arbitrate, and silently picking one would hide the disagreement.
    pub contested: bool,
}

const MEMORY_SELECT: &str =
    "SELECT id, scope, owner_id, kind, memory_key, content, source_kind, source_ref,
            confidence, status, supersedes_id, employee_id, run_id, created_at, expires_at
     FROM memory_items";

/// Visibility predicate shared by retrieval and by the contested probe, so a
/// caller can never see a disagreement it is not allowed to read.
const VISIBILITY: &str = "status = 'active'
       AND (expires_at IS NULL OR expires_at > ?1)
       AND (?2 IS NULL OR kind = ?2)
       AND ((scope = 'shared') OR (scope = 'employee' AND ?3 IS NOT NULL AND owner_id = ?3))
       AND (?4 IS NULL OR scope = ?4)";

fn memory_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryItem> {
    Ok(MemoryItem {
        id: row.get(0)?,
        scope: row.get(1)?,
        owner_id: row.get(2)?,
        kind: row.get(3)?,
        memory_key: row.get(4)?,
        content: row.get(5)?,
        source_kind: row.get(6)?,
        source_ref: row.get(7)?,
        confidence: row.get(8)?,
        status: row.get(9)?,
        supersedes_id: row.get(10)?,
        employee_id: row.get(11)?,
        run_id: row.get(12)?,
        created_at: ms_to_ts(row.get(13)?),
        expires_at: row.get::<_, Option<i64>>(14)?.map(ms_to_ts),
        contested: false,
    })
}

/// A write request, straight from a run or from the user.
#[derive(Debug, Clone)]
pub struct MemoryWriteInput {
    pub scope: String,
    pub owner_id: Option<String>,
    pub kind: String,
    pub key: String,
    pub text: String,
    pub source_kind: String,
    pub source_ref: String,
    pub confidence: Option<f64>,
    pub employee_id: Option<String>,
    pub run_id: Option<String>,
    pub expires_at: Option<String>,
}

impl MemoryWriteInput {
    pub fn from_json(input: &Value) -> Result<Self> {
        Ok(Self {
            scope: required_str(input, "scope")?,
            owner_id: optional_str(input, "ownerId"),
            kind: required_str(input, "kind")?,
            key: optional_str(input, "key").unwrap_or_default(),
            text: required_str(input, "text")?,
            source_kind: required_str(input, "sourceKind")?,
            source_ref: optional_str(input, "sourceRef").unwrap_or_default(),
            confidence: match input.get("confidence") {
                None | Some(Value::Null) => None,
                Some(value) => Some(
                    value
                        .as_f64()
                        .ok_or_else(|| anyhow!("confidence must be a number"))?,
                ),
            },
            employee_id: optional_str(input, "employeeId"),
            run_id: optional_str(input, "runId"),
            expires_at: optional_str(input, "expiresAt"),
        })
    }

    fn confidence(&self) -> f64 {
        self.confidence.unwrap_or(0.5)
    }

    fn expires_at_ms(&self) -> Option<i64> {
        self.expires_at.as_deref().map(ts_to_ms)
    }

    /// Reject an item that cannot be trusted or read back, before any write.
    fn validate(&self, require_shared: bool) -> Result<()> {
        if !MEMORY_KINDS.contains(&self.kind.as_str()) {
            bail!("INVALID_ARGUMENT: unknown memory kind {}", self.kind);
        }
        if !MEMORY_SCOPES.contains(&self.scope.as_str()) {
            bail!("INVALID_ARGUMENT: unknown memory scope {}", self.scope);
        }
        if !MEMORY_SOURCE_KINDS.contains(&self.source_kind.as_str()) {
            bail!(
                "INVALID_ARGUMENT: unknown memory source kind {}",
                self.source_kind
            );
        }
        let text_length = self.text.chars().count();
        if text_length == 0 || text_length > MAX_MEMORY_TEXT_CHARS {
            bail!("LIMIT_EXCEEDED: memory text must be 1..={MAX_MEMORY_TEXT_CHARS} characters");
        }
        if self.key.chars().count() > MAX_MEMORY_KEY_CHARS {
            bail!("LIMIT_EXCEEDED: memory key must be at most {MAX_MEMORY_KEY_CHARS} characters");
        }
        if self.scope == "employee" && self.owner_id.is_none() {
            bail!("INVALID_ARGUMENT: an employee-scoped memory item needs ownerId");
        }
        if require_shared && self.scope != "shared" {
            bail!("INVALID_ARGUMENT: publish_memory writes shared items only");
        }
        // Only the user may write a constraint. A run that could author one
        // could widen its own limits, and nobody is watching unattended runs.
        if self.kind == "constraint" && self.source_kind != "user" {
            bail!("PERMISSION_DENIED: only the user may write a constraint");
        }
        if self.kind == "fact" && self.source_ref.trim().is_empty() {
            bail!("INVALID_ARGUMENT: a fact needs a sourceRef");
        }
        if let Some(confidence) = self.confidence {
            if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
                bail!("INVALID_ARGUMENT: confidence must be between 0 and 1");
            }
        }
        Ok(())
    }
}

fn insert_memory(
    db: &Database,
    input: &MemoryWriteInput,
    supersedes_id: Option<&str>,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    db.conn().execute(
        "INSERT INTO memory_items
           (id, scope, owner_id, kind, memory_key, content, source_kind, source_ref,
            confidence, status, supersedes_id, employee_id, run_id, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'active', ?10, ?11, ?12, ?13, ?14)",
        params![
            id,
            &input.scope,
            &input.owner_id,
            &input.kind,
            &input.key,
            &input.text,
            &input.source_kind,
            &input.source_ref,
            input.confidence(),
            supersedes_id,
            &input.employee_id,
            &input.run_id,
            now_ms(),
            input.expires_at_ms(),
        ],
    )?;
    Ok(id)
}

fn get_memory(db: &Database, id: &str) -> Result<Option<MemoryItem>> {
    let item = db
        .conn()
        .prepare_cached(&format!("{MEMORY_SELECT} WHERE id = ?1"))?
        .query_row(params![id], memory_from_row)
        .optional()?;
    Ok(item)
}

/// Trim, lowercase, and collapse whitespace so two spellings of one claim
/// compare equal without a model in the loop.
fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The promotion gate: deterministic code, never a model judgement, because an
/// unattended promotion decision has to be reproducible.
///
/// User-authored items pass through: the user is the authority the gate would
/// otherwise defer to.
fn pass_gate(db: &Database, input: &MemoryWriteInput, now: i64) -> Result<()> {
    if input.source_kind == "user" {
        return Ok(());
    }
    if input.source_ref.trim().is_empty() {
        bail!("INVALID_ARGUMENT: publishing to shared memory requires a sourceRef");
    }

    let mut stmt = db.conn().prepare_cached(
        "SELECT content FROM memory_items
         WHERE scope = 'shared' AND kind = ?1 AND status = 'active'
           AND (expires_at IS NULL OR expires_at > ?2)",
    )?;
    let live = stmt
        .query_map(params![input.kind, now], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Only a genuinely equivalent claim is a duplicate. Same key with different
    // text is the contested case, and that has to get in: the gate exists to
    // stop the same sentence being promoted twice, not to pick a winner between
    // two live disagreements.
    let candidate = normalize(&input.text);
    for content in &live {
        if normalize(content) == candidate {
            bail!("CONFLICT: an equivalent shared memory item already exists");
        }
    }

    // A live constraint owns its key: nothing else may be promoted under it.
    // Contradiction is otherwise a semantic question, and the gate must stay
    // deterministic, so the check is structural rather than interpretive.
    if !input.key.is_empty() && input.kind != "constraint" {
        let blocked: Option<String> = db
            .conn()
            .prepare_cached(
                "SELECT memory_key FROM memory_items
                 WHERE scope = 'shared' AND kind = 'constraint' AND status = 'active'
                   AND memory_key = ?1 AND (expires_at IS NULL OR expires_at > ?2)",
            )?
            .query_row(params![input.key, now], |row| row.get(0))
            .optional()?;
        if blocked.is_some() {
            bail!(
                "PERMISSION_DENIED: a live constraint blocks publishing key {}",
                input.key
            );
        }
    }
    Ok(())
}

/// Write an employee-private item. Runs use this directly.
///
/// A `playbook` is rejected here on purpose: a tactic only becomes shared
/// knowledge by passing the gate, so a run cannot mint one for itself.
pub fn write_memory(db: &Database, input: MemoryWriteInput) -> Result<MemoryItem> {
    input.validate(false)?;
    if input.scope != "employee" {
        bail!("INVALID_ARGUMENT: write_memory writes employee-scoped items; use publish_memory");
    }
    if input.kind == "playbook" {
        bail!("PERMISSION_DENIED: a playbook is written by the memory gate only");
    }
    let id = insert_memory(db, &input, None)?;
    get_memory(db, &id)?.ok_or_else(|| anyhow!("memory item {id} disappeared after insert"))
}

/// Promote an item into shared memory through the gate.
pub fn publish_memory(db: &Database, input: MemoryWriteInput) -> Result<MemoryItem> {
    input.validate(true)?;
    let now = now_ms();
    pass_gate(db, &input, now)?;
    let id = insert_memory(db, &input, None)?;
    get_memory(db, &id)?.ok_or_else(|| anyhow!("memory item {id} disappeared after insert"))
}

/// Which items a caller may read.
#[derive(Debug, Clone)]
pub struct MemoryFilter {
    /// `None` reads shared items plus the caller's own employee items.
    pub scope: Option<String>,
    pub owner_id: Option<String>,
    pub kind: Option<String>,
    pub limit: i64,
    pub now_ms: Option<i64>,
}

impl Default for MemoryFilter {
    fn default() -> Self {
        Self {
            scope: None,
            owner_id: None,
            kind: None,
            limit: DEFAULT_MEMORY_LIMIT,
            now_ms: None,
        }
    }
}

impl MemoryFilter {
    /// Everything one employee may read: shared items plus its own.
    pub fn for_employee(employee_id: &str) -> Self {
        Self {
            owner_id: Some(employee_id.to_string()),
            ..Self::default()
        }
    }

    /// Only what every employee may read.
    pub fn shared() -> Self {
        Self {
            scope: Some("shared".to_string()),
            ..Self::default()
        }
    }

    pub fn from_json(input: &Value) -> Result<Self> {
        let limit = match input.get("limit") {
            None | Some(Value::Null) => DEFAULT_MEMORY_LIMIT,
            Some(value) => value
                .as_i64()
                .ok_or_else(|| anyhow!("limit must be an integer"))?,
        };
        Ok(Self {
            scope: optional_str(input, "scope"),
            owner_id: optional_str(input, "ownerId"),
            kind: optional_str(input, "kind"),
            limit,
            now_ms: None,
        })
    }

    fn validate(&self) -> Result<()> {
        if let Some(scope) = self.scope.as_deref() {
            if !MEMORY_SCOPES.contains(&scope) {
                bail!("INVALID_ARGUMENT: unknown memory scope {scope}");
            }
            if scope == "employee" && self.owner_id.is_none() {
                bail!("INVALID_ARGUMENT: reading employee memory needs ownerId");
            }
        }
        if let Some(kind) = self.kind.as_deref() {
            if !MEMORY_KINDS.contains(&kind) {
                bail!("INVALID_ARGUMENT: unknown memory kind {kind}");
            }
        }
        Ok(())
    }
}

/// Read live items, ordered by confidence then recency, bounded by `limit`.
///
/// Superseded, retired, and expired rows never come back: an answer must be
/// traceable to something that is still believed.
pub fn query_memory(db: &Database, filter: &MemoryFilter) -> Result<Vec<MemoryItem>> {
    filter.validate()?;
    let now = filter.now_ms.unwrap_or_else(now_ms);
    let limit = filter.limit.clamp(1, MAX_MEMORY_LIMIT);
    let mut stmt = db.conn().prepare_cached(&format!(
        "{MEMORY_SELECT} WHERE {VISIBILITY}
         ORDER BY confidence DESC, created_at DESC, id ASC
         LIMIT ?5"
    ))?;
    let items = stmt
        .query_map(
            params![now, filter.kind, filter.owner_id, filter.scope, limit],
            memory_from_row,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    mark_contested(db, filter, now, items)
}

/// Flag items whose `key` is claimed by more than one live item with
/// different text. One extra probe over the visible keys keeps the page
/// bounded while still detecting disagreement outside the page.
fn mark_contested(
    db: &Database,
    filter: &MemoryFilter,
    now: i64,
    mut items: Vec<MemoryItem>,
) -> Result<Vec<MemoryItem>> {
    if !items.iter().any(|item| !item.memory_key.is_empty()) {
        return Ok(items);
    }
    let mut stmt = db.conn().prepare_cached(&format!(
        "SELECT memory_key FROM memory_items
         WHERE {VISIBILITY} AND memory_key <> ''
         GROUP BY memory_key HAVING COUNT(DISTINCT content) > 1"
    ))?;
    let contested = stmt
        .query_map(
            params![now, filter.kind, filter.owner_id, filter.scope],
            |row| row.get::<_, String>(0),
        )?
        .collect::<rusqlite::Result<HashSet<_>>>()?;
    for item in &mut items {
        item.contested = !item.memory_key.is_empty() && contested.contains(&item.memory_key);
    }
    Ok(items)
}

/// Replace an item without losing it: the previous row stays, and both rows
/// carry the link.
///
/// `supersedes_id` therefore reads as "the other end of this replacement" --
/// the new row points back at what it replaced, the old row points forward at
/// what replaced it, and either direction is enough to reconstruct the chain.
pub fn supersede_memory(
    db: &Database,
    old_id: &str,
    input: MemoryWriteInput,
) -> Result<MemoryItem> {
    input.validate(false)?;
    if input.kind == "playbook" && input.scope != "shared" {
        bail!("PERMISSION_DENIED: a playbook is written by the memory gate only");
    }
    let previous =
        get_memory(db, old_id)?.ok_or_else(|| anyhow!("memory item {old_id} does not exist"))?;
    if previous.status != "active" {
        bail!(
            "CONFLICT: memory item {old_id} is {} and cannot be superseded",
            previous.status
        );
    }
    // A replacement stays where the item it replaces lived: otherwise a run
    // could quietly move a shared claim into its own private scope, or hand one
    // employee's memory to another.
    if previous.scope != input.scope {
        bail!("CONFLICT: a replacement must keep the scope of the item it supersedes");
    }
    if input.scope == "employee" && previous.owner_id != input.owner_id {
        bail!("PERMISSION_DENIED: a replacement must stay with the same employee");
    }
    if input.scope == "shared" {
        pass_gate(db, &input, now_ms())?;
    }

    let tx = db.conn().unchecked_transaction()?;
    let new_id = insert_memory(db, &input, Some(old_id))?;
    tx.execute(
        "UPDATE memory_items SET status = 'superseded', supersedes_id = ?2
         WHERE id = ?1 AND status = 'active'",
        params![old_id, new_id],
    )?;
    tx.commit().context("commit memory supersede")?;
    get_memory(db, &new_id)?.ok_or_else(|| anyhow!("memory item {new_id} disappeared after insert"))
}
/// Retire a live item. The row stays for provenance; retrieval stops returning
/// it.
///
/// `author` is the source kind of the caller. Retiring a constraint is a user
/// act for the same reason writing one is: removing a limit is loosening it, and
/// a run that could retire its own constraint would be widening its own limits
/// by another road.
pub fn retire_memory(db: &Database, id: &str, author: &str) -> Result<bool> {
    if !MEMORY_SOURCE_KINDS.contains(&author) {
        bail!("INVALID_ARGUMENT: unknown memory source kind {author}");
    }
    let item = get_memory(db, id)?.ok_or_else(|| anyhow!("memory item {id} does not exist"))?;
    if item.status != "active" {
        return Ok(false);
    }
    if item.kind == "constraint" && author != "user" {
        bail!("PERMISSION_DENIED: only the user may retire a constraint");
    }
    db.conn().execute(
        "UPDATE memory_items SET status = 'retired' WHERE id = ?1 AND status = 'active'",
        params![id],
    )?;
    Ok(true)
}
