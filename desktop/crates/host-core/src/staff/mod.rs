#![allow(dead_code)]

//! Staff employees: durable employees, their credential profiles, their run
//! records, their workflows, and the memory they accumulate (ADR 0252, spec
//! 03-runtime/20).
//!
//! Three rules shape this module:
//!
//! - A credential is *referenced*, never inlined. An employee points at a
//!   credential profile, and the profile points at a SecretStore ref, so no
//!   employee row, run row, RPC reply, or log line carries key material and a
//!   key can be rotated in one place.
//! - Memory is append-only and carries provenance. Unattended runs make an
//!   unverifiable claim expensive to notice and impossible to trace, so an item
//!   records who wrote it, which run, and what source it came from.
//! - A workflow is immutable per version, and a run pins one. An unattended run
//!   has to be able to say which revision it followed, or "why did it post
//!   that" has no answer.
//!
//! The host owns ordering, gating, scheduling, and memory semantics for all of
//! this. Driving an actual agent turn stays with the desktop host process, which
//! is the piece that stays resident while the app is open.

pub(crate) use anyhow::{anyhow, bail, Context, Result};
pub(crate) use rusqlite::{params, OptionalExtension};
pub(crate) use serde::{Deserialize, Serialize};
pub(crate) use serde_json::Value;
pub(crate) use std::path::Path;
pub(crate) use uuid::Uuid;

pub(crate) use crate::db::{ms_to_ts, now_ms, ts_to_ms, Database, SCHEMA_VERSION};
pub(crate) use crate::secrets::SecretStore;

mod advice;
mod analysis;
mod decisions;
mod contents;
mod credentials;
mod data;
mod employees;
mod leads;
mod report;
mod memory;
mod migration;
mod presets;
mod rpc;
mod schedule;
mod watch;
mod workflow;

#[allow(unused_imports)]
pub use decisions::{
    assign_directive, board_brief, claim_directive, directive_brief, dispatch_scope_of,
    list_decisions,
    record_decision, BOARD_ACTIONS, DIRECTIVE_VERBS, DEFAULT_MAX_DIRECTIVES_PER_DAY,
};

pub(crate) use migration::{
    migrate, migrate_boards, migrate_managers, migrate_run_reports, migrate_run_results,
};
pub(crate) use rpc::handle;
#[allow(unused_imports)]
pub(crate) use schedule::{claim_due, recover, start_run, Schedule};

// The domain surface is consumed one slice at a time -- RPC today, the residency
// loop next -- so an export here may legitimately have no caller yet.
#[allow(unused_imports)]
pub use credentials::{
    create_profile, delete_profile, get_profile, list_profiles, resolve_profile_secret,
    secret_ref_for_profile, set_profile_secret, update_profile, CredentialProfile,
};
#[allow(unused_imports)]
pub use employees::{
    begin_run, begin_run_for, create_employee, delete_employee, finish_run, get_employee, get_run,
    list_employees, list_runs, refresh_schedule, resolve_binding_for_run, resolve_run_binding,
    run_directive_id, set_run_step, update_employee, Employee, EmployeeRun, RunBinding,
    EMPLOYEE_LAYERS, EMPLOYEE_STATUSES,
};
#[allow(unused_imports)]
pub use memory::{
    publish_memory, query_memory, retire_memory, supersede_memory, write_memory, MemoryFilter,
    MemoryItem, MemoryWriteInput, MEMORY_KINDS, MEMORY_SCOPES, MEMORY_SOURCE_KINDS,
    MEMORY_STATUSES,
};
#[allow(unused_imports)]
pub use workflow::{
    adopt_graph, create_workflow, delete_workflow, get_graph, get_workflow, list_versions,
    list_workflows, parse_graph, update_workflow, Graph, MemoryPlan, Workflow, WorkflowStep,
    WorkflowVersion, NODE_KINDS, SOURCE_CHANNELS, WORKFLOW_STATUSES,
};
#[allow(unused_imports)]
pub use presets::{install_presets, list_presets, presets, Preset, PRESET_IDS};
#[allow(unused_imports)]
pub use leads::{
    get_lead, lead_stage_counts, list_leads, move_lead, upsert_lead, Lead, LEAD_OPEN_STAGES,
    LEAD_STAGES,
};
#[allow(unused_imports)]
pub use contents::{
    content_status_counts, get_content, list_contents, set_content_status, upsert_content, Content,
    CONTENT_OPEN_STATUSES, CONTENT_STATUSES,
};
#[allow(unused_imports)]
pub use report::{
    day_bounds, list_metrics, local_day, report_summary, upsert_metric, Metric, METRIC_DIMENSIONS,
};
// The return leg of the loop: what the world did after we spoke.
#[allow(unused_imports)]
pub use watch::{
    fingerprint_of, ingest_watch, watch_summary, watch_targets, WatchTarget,
    DEFAULT_WATCH_TARGETS, MAX_WATCH_TARGETS,
};

// The ledger behind the data page: what each station holds, and what a
// cleanup the owner picked would take.
#[allow(unused_imports)]
pub use data::{
    data_stats, prune, PRUNE_RULES, RETENTION_CONTENTS_DAYS, RETENTION_DISPATCH_DAYS,
    RETENTION_LEADS_DAYS, RETENTION_METRICS_DAYS, RETENTION_RUNS_KEPT,
};

#[allow(unused_imports)]
pub use analysis::derive_suggestions;
#[allow(unused_imports)]
pub use advice::{
    create_suggestion, decide_suggestion, get_directive, get_suggestion, list_directives,
    list_suggestions, update_directive, Directive, Evidence, Suggestion, DIRECTIVE_STATUSES,
    SUGGESTION_BOARDS, SUGGESTION_STATUSES,
};

/// Staff tables, shared by fresh creation and the v16 -> v17 migration.
pub(crate) const SCHEMA: &str = include_str!("schema.sql");

/// What the staff domain needs from the host.
///
/// A run opens a session in the user's workspace, so the workspace path travels
/// with the call instead of being read from a global.
pub(crate) struct StaffContext<'a> {
    pub db: &'a Database,
    pub secrets: &'a SecretStore,
    pub workspace: Option<&'a str>,
}

/// Trimmed required string from a camelCase request object.
pub(crate) fn required_str(params: &Value, key: &str) -> Result<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("INVALID_ARGUMENT: {key} is required"))
}

/// Trimmed optional string; an empty or whitespace-only value reads as absent.
pub(crate) fn optional_str(params: &Value, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests;
