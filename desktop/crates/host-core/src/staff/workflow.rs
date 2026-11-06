use super::*;
use serde_json::json;
use std::collections::{HashMap, HashSet};

/// Node kinds a workflow may contain. The set is deliberately closed: a stored
/// graph is executed unattended, so an unknown node is a hole nobody is
/// watching, not a forward-compatible extension point.
pub const NODE_KINDS: [&str; 11] = [
    "trigger",
    "probe",
    "analyze",
    "produce",
    "agent",
    "memory",
    "condition",
    "handoff",
    "humanGate",
    "notify",
    // The one node only a manager may run: it reads a board, judges what it
    // read, and hands work to the workers its own config names (ADR 0254).
    "board",
];

pub const WORKFLOW_STATUSES: [&str; 3] = ["draft", "active", "archived"];
/// Channels a probe step may collect from.
///
/// Closed for the same reason node kinds are: each id has a collector, so an
/// unknown id is a step that would run and quietly collect nothing. The two
/// WeChat channels are included because that is where the Chinese-language
/// conversations are, even though they need a logged-in session rather than a
/// public feed.
pub const SOURCE_CHANNELS: [&str; 5] =
    ["juejin", "zhihu", "bilibili", "wechatMp", "wechatChannels"];

/// Memory operations a memory node may name -- the four the console's node
/// form offers, and the four this file validates.
///
/// The set is closed for the same reason node kinds are, and it is one list
/// read by both the validator and the compiler because the driver answers the
/// four differently: `read` is answered by reading the store itself, the rest
/// by asking the model. The driver once knew only the word `query`, so a graph
/// naming `read` -- including the seeded interest analyst -- fell through to a
/// paid model turn and then recorded what that turn said instead of what the
/// store already knew (run 6e6e5672, ADR 0252).
pub const MEMORY_OPERATIONS: [&str; 4] = ["read", "write", "publish", "retire"];

/// Spellings a graph may still carry, and the operation they mean.
const MEMORY_OPERATION_ALIASES: [(&str, &str); 1] = [("query", "read")];

/// Node kinds that may not appear more than once, or at all, in one graph.
const SINGLETON_KINDS: [&str; 1] = ["trigger"];

const MAX_NODES: usize = 200;
const MAX_EDGES: usize = 600;
const MAX_ID_CHARS: usize = 64;
const MAX_TEXT_CHARS: usize = 4_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub latest_version: i64,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowVersion {
    pub id: String,
    pub workflow_id: String,
    pub version: i64,
    pub node_count: i64,
    pub created_by: String,
    pub created_at: String,
}

/// One executable step, produced by compiling a graph.
///
/// The host owns ordering, gating, and memory semantics; the executor owns
/// wording. That split keeps an unattended run reproducible even though the
/// prompt is assembled elsewhere.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStep {
    pub node_id: String,
    pub kind: String,
    pub title: String,
    pub instruction: String,
    /// The node config as written. The driver renders it into the step prompt so
    /// a node that says `channel: forum` reaches the model as a parameter and
    /// not as a fact only the editor knows.
    pub config: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoff_employee_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemoryPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryPlan {
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone)]
struct Node {
    id: String,
    kind: String,
    title: String,
    instruction: String,
    config: Value,
}

/// A validated graph: every rule below has already been applied.
#[derive(Debug, Clone)]
pub struct Graph {
    nodes: Vec<Node>,
    edges: Vec<(String, String)>,
}

impl Graph {
    pub fn node_count(&self) -> i64 {
        self.nodes.len() as i64
    }

    /// Whether the graph holds a node of this kind. The caller that needs it is
    /// the employee binding: reading a board is a manager's work, and a worker
    /// bound to a graph that reads one has to be refused by name.
    pub fn has_kind(&self, kind: &str) -> bool {
        self.nodes.iter().any(|node| node.kind == kind)
    }

    /// The titles of the nodes of one kind, in declaration order. A manager's
    /// dispatch scope is declared on its board nodes, and the scope check reads
    /// it back from the graph the run is actually pinned to.
    pub fn configs_of(&self, kind: &str) -> Vec<&Value> {
        self.nodes
            .iter()
            .filter(|node| node.kind == kind)
            .map(|node| &node.config)
            .collect()
    }
}

fn text_of(config: &Value, key: &str) -> Option<String> {
    config
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn require_config_text(node: &Node, key: &str) -> Result<String> {
    text_of(&node.config, key).ok_or_else(|| {
        anyhow!(
            "INVALID_ARGUMENT: node {} ({}) needs a {key}",
            node.id,
            node.kind
        )
    })
}

/// The operation a memory node really means, or an error naming the offender.
fn require_memory_operation(node: &Node) -> Result<String> {
    let named = require_config_text(node, "operation")?;
    if MEMORY_OPERATIONS.contains(&named.as_str()) {
        return Ok(named);
    }
    match MEMORY_OPERATION_ALIASES
        .iter()
        .find(|(alias, _)| *alias == named.as_str())
    {
        Some((_, real)) => Ok((*real).to_string()),
        None => bail!(
            "INVALID_ARGUMENT: node {} has unknown memory operation {named} (expected {})",
            node.id,
            MEMORY_OPERATIONS.join(", ")
        ),
    }
}

/// A memory node's scope, checked against the scopes the store actually has, and
/// read by both the validator and the compiler.
///
/// An unknown scope is not a harmless label: the write it asks for is refused
/// at run time, long after the graph was saved, with a message about the
/// store's vocabulary rather than about the node that asked. Failing the save
/// names the node.
fn memory_scope_of(node: &Node) -> Result<Option<String>> {
    let Some(scope) = text_of(&node.config, "scope") else {
        return Ok(None);
    };
    if !MEMORY_SCOPES.contains(&scope.as_str()) {
        bail!(
            "INVALID_ARGUMENT: node {} has unknown memory scope {scope} (expected {})",
            node.id,
            MEMORY_SCOPES.join(" or ")
        );
    }
    Ok(Some(scope))
}

/// A probe names its sources as a list, as one legacy channel label, or both.
///
/// `channel` stays accepted because graphs and seeds written before sources
/// existed still have to open, and a single free-text label is what an owner
/// types when they want one ad-hoc source.
fn require_probe_sources(node: &Node) -> Result<()> {
    let mut named = text_of(&node.config, "channel").is_some();
    if let Some(raw) = node.config.get("sources") {
        let list = raw.as_array().ok_or_else(|| {
            anyhow!("INVALID_ARGUMENT: node {} has a non-list sources", node.id)
        })?;
        if list.is_empty() {
            bail!("INVALID_ARGUMENT: node {} names no probe source", node.id);
        }
        for value in list {
            let channel = value.as_str().unwrap_or_default();
            if !SOURCE_CHANNELS.contains(&channel) {
                bail!(
                    "INVALID_ARGUMENT: node {} has unknown source channel {channel}",
                    node.id
                );
            }
        }
        named = true;
    }
    if !named {
        bail!(
            "INVALID_ARGUMENT: node {} needs a probe channel or sources",
            node.id
        );
    }
    Ok(())
}

fn bounded(label: &str, value: &str, max: usize) -> Result<()> {
    if value.chars().count() > max {
        bail!("LIMIT_EXCEEDED: {label} must be at most {max} characters");
    }
    Ok(())
}

/// Parse and fully validate a stored graph.
///
/// Validation is all-or-nothing: a graph that reaches storage has passed every
/// rule, because a half-valid graph would only fail later, mid-run, with nobody
/// watching.
pub fn parse_graph(value: &Value) -> Result<Graph> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("INVALID_ARGUMENT: graph must be an object"))?;
    let raw_nodes = object
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("INVALID_ARGUMENT: graph.nodes must be an array"))?;
    if raw_nodes.is_empty() {
        bail!("INVALID_ARGUMENT: graph needs at least one node");
    }
    if raw_nodes.len() > MAX_NODES {
        bail!("LIMIT_EXCEEDED: a graph holds at most {MAX_NODES} nodes");
    }

    let mut nodes = Vec::with_capacity(raw_nodes.len());
    let mut seen: HashSet<String> = HashSet::new();
    let mut singletons: HashSet<String> = HashSet::new();
    for raw in raw_nodes {
        let id = required_str(raw, "id")?;
        bounded("a node id", &id, MAX_ID_CHARS)?;
        if !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            bail!("INVALID_ARGUMENT: node id {id} must be alphanumeric, '-' or '_'");
        }
        if !seen.insert(id.clone()) {
            bail!("INVALID_ARGUMENT: duplicate node id {id}");
        }
        let kind = required_str(raw, "kind")?;
        if !NODE_KINDS.contains(&kind.as_str()) {
            bail!("INVALID_ARGUMENT: node {id} has unknown kind {kind}");
        }
        if SINGLETON_KINDS.contains(&kind.as_str()) && !singletons.insert(kind.clone()) {
            bail!("INVALID_ARGUMENT: a graph holds exactly one {kind} node");
        }
        let title = optional_str(raw, "title").unwrap_or_else(|| kind.clone());
        bounded("a node title", &title, MAX_ID_CHARS)?;
        let config = raw.get("config").cloned().unwrap_or_else(|| json!({}));
        if !config.is_object() {
            bail!("INVALID_ARGUMENT: node {id} config must be an object");
        }
        let instruction = optional_str(raw, "instruction")
            .or_else(|| text_of(&config, "instruction"))
            .unwrap_or_default();
        bounded("a node instruction", &instruction, MAX_TEXT_CHARS)?;
        nodes.push(Node {
            id,
            kind,
            title,
            instruction,
            config,
        });
    }

    // Per-kind requirements. These are the fields the executor and the gate
    // read, so a missing one is a run that fails hours later.
    for node in &nodes {
        match node.kind.as_str() {
            "trigger" => {
                let mode = require_config_text(node, "mode")?;
                if !["schedule", "manual", "event"].contains(&mode.as_str()) {
                    bail!(
                        "INVALID_ARGUMENT: node {} has unknown trigger mode {mode}",
                        node.id
                    );
                }
            }
            "probe" => {
                require_probe_sources(node)?;
            }
            "analyze" => {
                require_config_text(node, "focus")?;
            }
            "produce" => {
                require_config_text(node, "artifact")?;
            }
            "agent" => {
                if node.instruction.is_empty() {
                    bail!("INVALID_ARGUMENT: node {} needs an instruction", node.id);
                }
            }
            "memory" => {
                let operation = require_memory_operation(node)?;
                memory_scope_of(node)?;
                if operation == "read" {
                    require_config_text(node, "scope")?;
                } else if operation != "retire" {
                    require_config_text(node, "kind")?;
                }
            }
            "condition" => {
                require_config_text(node, "expression")?;
            }
            "handoff" => {
                require_config_text(node, "employeeId")?;
            }
            "humanGate" => {
                require_config_text(node, "reason")?;
            }
            "notify" => {
                require_config_text(node, "channel")?;
            }
            "board" => {
                let action = require_config_text(node, "action")?;
                if !BOARD_ACTIONS.contains(&action.as_str()) {
                    bail!(
                        "INVALID_ARGUMENT: node {} has unknown board action {action} (expected {})",
                        node.id,
                        BOARD_ACTIONS.join(", ")
                    );
                }
            }
            other => bail!(
                "INVALID_ARGUMENT: node {} has unknown kind {other}",
                node.id
            ),
        }
    }

    let raw_edges = object
        .get("edges")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if raw_edges.len() > MAX_EDGES {
        bail!("LIMIT_EXCEEDED: a graph holds at most {MAX_EDGES} edges");
    }
    let mut edges = Vec::with_capacity(raw_edges.len());
    let mut edge_seen: HashSet<(String, String)> = HashSet::new();
    for raw in &raw_edges {
        let from = required_str(raw, "from")?;
        let to = required_str(raw, "to")?;
        if !seen.contains(&from) {
            bail!("INVALID_ARGUMENT: edge {from} -> {to} starts at an unknown node");
        }
        if !seen.contains(&to) {
            bail!("INVALID_ARGUMENT: edge {from} -> {to} ends at an unknown node");
        }
        if from == to {
            bail!("INVALID_ARGUMENT: node {from} cannot connect to itself");
        }
        if !edge_seen.insert((from.clone(), to.clone())) {
            bail!("INVALID_ARGUMENT: duplicate edge {from} -> {to}");
        }
        edges.push((from, to));
    }

    let graph = Graph { nodes, edges };
    let order = graph.topological_order()?;
    graph.require_reachable(&order)?;
    Ok(graph)
}

impl Graph {
    fn index(&self) -> HashMap<&str, usize> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.as_str(), index))
            .collect()
    }

    /// Kahn's algorithm with the declaration order as the tie-break, so the same
    /// graph always compiles to the same steps.
    pub fn topological_order(&self) -> Result<Vec<String>> {
        let index = self.index();
        let mut indegree = vec![0usize; self.nodes.len()];
        let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); self.nodes.len()];
        for (from, to) in &self.edges {
            let from = index[from.as_str()];
            let to = index[to.as_str()];
            outgoing[from].push(to);
            indegree[to] += 1;
        }
        let mut ready: Vec<usize> = (0..self.nodes.len())
            .filter(|index| indegree[*index] == 0)
            .collect();
        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some(position) = ready.iter().copied().min() {
            ready.retain(|index| *index != position);
            order.push(self.nodes[position].id.clone());
            for next in &outgoing[position] {
                indegree[*next] -= 1;
                if indegree[*next] == 0 {
                    ready.push(*next);
                }
            }
        }
        if order.len() != self.nodes.len() {
            bail!("INVALID_ARGUMENT: the workflow graph has a cycle");
        }
        Ok(order)
    }

    /// Every node has to be reachable from the trigger. An orphaned node is a
    /// step the author believes is running and never does.
    fn require_reachable(&self, order: &[String]) -> Result<()> {
        let trigger = self
            .nodes
            .iter()
            .find(|node| node.kind == "trigger")
            .ok_or_else(|| anyhow!("INVALID_ARGUMENT: a graph needs exactly one trigger node"))?;
        let index = self.index();
        let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); self.nodes.len()];
        for (from, to) in &self.edges {
            outgoing[index[from.as_str()]].push(index[to.as_str()]);
        }
        let mut reached = vec![false; self.nodes.len()];
        let mut stack = vec![index[trigger.id.as_str()]];
        reached[stack[0]] = true;
        while let Some(current) = stack.pop() {
            for next in &outgoing[current] {
                if !reached[*next] {
                    reached[*next] = true;
                    stack.push(*next);
                }
            }
        }
        for (position, node) in self.nodes.iter().enumerate() {
            if !reached[position] {
                bail!(
                    "INVALID_ARGUMENT: node {} is not reachable from the trigger",
                    node.id
                );
            }
        }
        debug_assert_eq!(order.len(), self.nodes.len());
        Ok(())
    }

    /// Compile the graph into the ordered steps a run executes.
    pub fn compile(&self) -> Result<Vec<WorkflowStep>> {
        let order = self.topological_order()?;
        self.require_reachable(&order)?;
        let index = self.index();
        let mut steps = Vec::with_capacity(order.len());
        for id in order {
            let node = &self.nodes[index[id.as_str()]];
            if node.kind == "trigger" {
                continue;
            }
            let memory = if node.kind == "memory" {
                Some(MemoryPlan {
                    operation: require_memory_operation(node)?,
                    scope: memory_scope_of(node)?,
                    kind: text_of(&node.config, "kind"),
                    limit: node.config.get("limit").and_then(Value::as_i64),
                })
            } else {
                None
            };
            steps.push(WorkflowStep {
                node_id: node.id.clone(),
                kind: node.kind.clone(),
                title: node.title.clone(),
                instruction: node.instruction.clone(),
                config: node.config.clone(),
                gate: if node.kind == "humanGate" {
                    Some(require_config_text(node, "reason")?)
                } else {
                    None
                },
                handoff_employee_id: if node.kind == "handoff" {
                    Some(require_config_text(node, "employeeId")?)
                } else {
                    None
                },
                memory,
            });
        }
        Ok(steps)
    }
}

const WORKFLOW_SELECT: &str =
    "SELECT id, name, description, status, latest_version, created_at, updated_at FROM workflows";

fn workflow_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Workflow> {
    Ok(Workflow {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        status: row.get(3)?,
        latest_version: row.get(4)?,
        created_at: ms_to_ts(row.get(5)?),
        updated_at: ms_to_ts(row.get(6)?),
        node_count: None,
    })
}

fn normalize_workflow_status(value: Option<&Value>) -> Result<String> {
    match value.and_then(Value::as_str) {
        None => Ok("draft".to_string()),
        Some(status) if WORKFLOW_STATUSES.contains(&status) => Ok(status.to_string()),
        Some(other) => bail!("INVALID_ARGUMENT: unknown workflow status {other}"),
    }
}

pub fn list_workflows(db: &Database) -> Result<Vec<Workflow>> {
    let mut stmt = db
        .conn()
        .prepare_cached(&format!("{WORKFLOW_SELECT} ORDER BY created_at ASC"))?;
    let rows = stmt.query_map([], workflow_from_row)?;
    let mut workflows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    for workflow in &mut workflows {
        workflow.node_count = Some(node_count(db, &workflow.id, workflow.latest_version)?);
    }
    Ok(workflows)
}

fn node_count(db: &Database, workflow_id: &str, version: i64) -> Result<i64> {
    let count: Option<i64> = db
        .conn()
        .prepare_cached(
            "SELECT node_count FROM workflow_versions WHERE workflow_id = ?1 AND version = ?2",
        )?
        .query_row(params![workflow_id, version], |row| row.get(0))
        .optional()?;
    Ok(count.unwrap_or(0))
}

pub fn get_workflow(db: &Database, id: &str) -> Result<Option<Workflow>> {
    let workflow = db
        .conn()
        .prepare_cached(&format!("{WORKFLOW_SELECT} WHERE id = ?1"))?
        .query_row(params![id], workflow_from_row)
        .optional()?;
    Ok(match workflow {
        Some(mut workflow) => {
            workflow.node_count = Some(node_count(db, &workflow.id, workflow.latest_version)?);
            Some(workflow)
        }
        None => None,
    })
}

pub fn list_versions(db: &Database, workflow_id: &str) -> Result<Vec<WorkflowVersion>> {
    let mut stmt = db.conn().prepare_cached(
        "SELECT id, workflow_id, version, node_count, created_by, created_at
         FROM workflow_versions WHERE workflow_id = ?1 ORDER BY version DESC",
    )?;
    let rows = stmt.query_map(params![workflow_id], |row| {
        Ok(WorkflowVersion {
            id: row.get(0)?,
            workflow_id: row.get(1)?,
            version: row.get(2)?,
            node_count: row.get(3)?,
            created_by: row.get(4)?,
            created_at: ms_to_ts(row.get(5)?),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Load a graph at an exact version. A run resolves its version this way, which
/// is what makes a pinned run reproducible.
pub fn get_graph(
    db: &Database,
    workflow_id: &str,
    version: i64,
) -> Result<Option<(Graph, String)>> {
    let raw: Option<String> = db
        .conn()
        .prepare_cached(
            "SELECT graph_json FROM workflow_versions WHERE workflow_id = ?1 AND version = ?2",
        )?
        .query_row(params![workflow_id, version], |row| row.get(0))
        .optional()?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value: Value = serde_json::from_str(&raw).context("parse stored workflow graph")?;
    Ok(Some((parse_graph(&value)?, raw)))
}

fn insert_version(
    db: &Database,
    workflow_id: &str,
    version: i64,
    graph: &Graph,
    graph_json: &str,
    created_by: &str,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    db.conn().execute(
        "INSERT INTO workflow_versions
           (id, workflow_id, version, graph_json, node_count, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            workflow_id,
            version,
            graph_json,
            graph.node_count(),
            created_by,
            now_ms()
        ],
    )?;
    Ok(id)
}

/// Create a workflow with its first version.
pub fn create_workflow(db: &Database, params: &Value) -> Result<Workflow> {
    let name = required_str(params, "name")?;
    bounded("a workflow name", &name, MAX_ID_CHARS)?;
    let description = optional_str(params, "description").unwrap_or_default();
    bounded("a workflow description", &description, MAX_TEXT_CHARS)?;
    let status = normalize_workflow_status(params.get("status"))?;
    let graph_value = params
        .get("graph")
        .cloned()
        .ok_or_else(|| anyhow!("INVALID_ARGUMENT: graph is required"))?;
    let graph = parse_graph(&graph_value)?;
    let id = Uuid::new_v4().to_string();
    let now = now_ms();

    let tx = db.conn().unchecked_transaction()?;
    tx.execute(
        "INSERT INTO workflows (id, name, description, status, latest_version, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
        params![id, name, description, status, now],
    )?;
    insert_version(db, &id, 1, &graph, &graph_value.to_string(), "user")?;
    tx.commit().context("commit workflow create")?;
    get_workflow(db, &id)?.ok_or_else(|| anyhow!("workflow {id} disappeared after insert"))
}

/// Update metadata, and append an immutable version when the graph changes.
pub fn update_workflow(db: &Database, id: &str, params: &Value) -> Result<Option<Workflow>> {
    let Some(current) = get_workflow(db, id)? else {
        return Ok(None);
    };
    let name = optional_str(params, "name").unwrap_or(current.name);
    bounded("a workflow name", &name, MAX_ID_CHARS)?;
    let description = optional_str(params, "description").unwrap_or(current.description);
    bounded("a workflow description", &description, MAX_TEXT_CHARS)?;
    let status = match params.get("status") {
        None | Some(Value::Null) => current.status.clone(),
        other => normalize_workflow_status(other)?,
    };
    let graph_value = params.get("graph").cloned();
    let graph = match &graph_value {
        Some(value) => Some(parse_graph(value)?),
        None => None,
    };
    let next_version = if graph.is_some() {
        current.latest_version + 1
    } else {
        current.latest_version
    };

    let tx = db.conn().unchecked_transaction()?;
    tx.execute(
        "UPDATE workflows SET name = ?2, description = ?3, status = ?4,
           latest_version = ?5, updated_at = ?6 WHERE id = ?1",
        params![id, name, description, status, next_version, now_ms()],
    )?;
    if let (Some(graph), Some(value)) = (&graph, &graph_value) {
        insert_version(db, id, next_version, graph, &value.to_string(), "user")?;
    }
    tx.commit().context("commit workflow update")?;
    get_workflow(db, id)
}

/// Adopt a blueprint onto a workflow as its next version.
///
/// The owner's history is the reason this is not an in-place edit: the graph
/// that was replaced stays readable at the version it was, so adopting a
/// corrected blueprint is something that can be looked at afterwards rather
/// than trusted. It is also why `created_by` is a parameter -- the author of a
/// seeded blueprint is the build, not the person whose console it landed in,
/// and a version history that names the wrong author is worse than none.
pub fn adopt_graph(
    db: &Database,
    id: &str,
    graph_value: &Value,
    created_by: &str,
) -> Result<Option<Workflow>> {
    let Some(current) = get_workflow(db, id)? else {
        return Ok(None);
    };
    let graph = parse_graph(graph_value)?;
    let next_version = current.latest_version + 1;
    let tx = db.conn().unchecked_transaction()?;
    tx.execute(
        "UPDATE workflows SET latest_version = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, next_version, now_ms()],
    )?;
    insert_version(db, id, next_version, &graph, &graph_value.to_string(), created_by)?;
    tx.commit().context("commit workflow adopt")?;
    get_workflow(db, id)
}

/// Deleting a workflow an employee still runs would strand that employee, so it
/// is refused the same way a referenced credential profile is.
pub fn delete_workflow(db: &Database, id: &str) -> Result<bool> {
    if get_workflow(db, id)?.is_none() {
        return Ok(false);
    }
    let referenced: i64 = db.conn().query_row(
        "SELECT COUNT(*) FROM employees WHERE workflow_id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    if referenced > 0 {
        bail!("CONFLICT: workflow {id} is used by {referenced} employee(s); reassign them first");
    }
    let tx = db.conn().unchecked_transaction()?;
    tx.execute(
        "DELETE FROM workflow_versions WHERE workflow_id = ?1",
        params![id],
    )?;
    tx.execute("DELETE FROM workflows WHERE id = ?1", params![id])?;
    tx.commit().context("commit workflow delete")?;
    Ok(true)
}
