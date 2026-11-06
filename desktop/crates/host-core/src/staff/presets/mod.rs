/**
 * The employees a fresh console starts with.
 *
 * A one-person company does not begin with an empty org chart. Shipping the
 * roster as blueprints means the console has something real to run on first
 * launch instead of an empty state that only its author knows how to fill.
 *
 * The blueprints split the way the work does: `founding` holds the pair the
 * whole funnel rests on, and `roster` holds the rest of the chain -- the
 * writer, the one who posts, the three who carry a conversation to money.
 * Which of them run, and in what order, stays the owner's decision: a
 * blueprint is data, not a migration, and an owner who edits a seeded employee
 * is editing an ordinary employee. The one exception is asked for by name:
 * `install` with `adoptBlueprint` will hand an existing employee the copy of
 * its blueprint this build ships, as a new version, so a corrected graph can
 * reach the console that was seeded with the broken one.
 *
 * The graphs inside the blueprints are written on the grid the console's own
 * auto-layout produces: three columns to a band at its origin, then the next
 * band below. A blueprint left on the old single row would open one way for an
 * owner who adopted it and another way for one who pressed the layout button,
 * so the two are kept in step here by hand.
 */

use super::*;
use crate::providers::{create_provider, list_providers, ProviderCreateInput};
use serde_json::json;

mod founding;
mod managers;
mod roster;
/// Ids accepted by `staff.presets.install`, in the order the chain runs.
pub const PRESET_IDS: [&str; 9] = [
    "scout",
    "interestAnalyst",
    "leadManager",
    "contentEditor",
    "writer",
    "poster",
    "receptionist",
    "closer",
    "aftersales",
];

/// Everything one seeded employee needs, already in wire shape.
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    /// Names this blueprint shipped under before it was renamed.
    pub legacy_names: &'static [&'static str],
    /// `worker` or `manager`: whether this employee does errands or hands them
    /// out. A manager's blueprint is the only place a `board` node appears.
    pub layer: &'static str,
    /// The board a manager owns; empty for a worker.
    pub board: &'static str,
    pub role: &'static str,
    pub workflow_name: &'static str,
    pub workflow_description: &'static str,
    pub channels: &'static [&'static str],
    pub schedule: Value,
    pub quota: Value,
    pub graph: Value,
}

impl Preset {
    /// Every name an employee of this blueprint may already be under.
    fn held_names(&self) -> Vec<&'static str> {
        let mut names = Vec::with_capacity(1 + self.legacy_names.len());
        names.push(self.name);
        names.extend_from_slice(self.legacy_names);
        names
    }

    /// The name this blueprint is already hired under, if it is.
    ///
    /// Installing is idempotent by employee name and an employee carries no
    /// preset id, so a rename on this side would otherwise read as a brand new
    /// hire and the owner would be given a second employee. The names the
    /// blueprint shipped under before count as the same employee.
    fn hired_as(&self, employees: &[Employee]) -> Option<&'static str> {
        self.held_names()
            .into_iter()
            .find(|name| employees.iter().any(|employee| employee.name == *name))
    }
}

/// The owner's UTC offset in minutes.
///
/// A seeded 09:30 has to mean 09:30 where the owner is, or the first run of a
/// brand new employee lands in the middle of their night. The offset is read
/// when the preset is installed, not stored as a constant.
pub(super) fn local_offset_minutes() -> i64 {
    use chrono::Offset;
    i64::from(chrono::Local::now().offset().fix().local_minus_utc()) / 60
}


/// Every blueprint the console offers, in the order it shows them.
pub fn presets() -> Vec<Preset> {
    let mut all = founding::blueprints();
    all.extend(managers::blueprints());
    all.extend(roster::blueprints());
    all
}
/// Blueprints plus whether an employee of that name already exists.
///
/// An installed blueprint also reports whether the employee is still running
/// the copy it was hired with. A blueprint is data, not a migration, so the
/// console never rewrites an employee on its own -- but an owner cannot adopt
/// a corrected blueprint they cannot see is different, and these graphs are
/// corrected by hand between releases.
pub fn list_presets(db: &Database) -> Result<Value> {
    let existing = list_employees(db)?;
    let mut rows = Vec::new();
    for preset in presets() {
        let held = preset.hired_as(&existing);
        let changed = match held.and_then(|name| existing.iter().find(|e| e.name == name)) {
            Some(employee) => holds_older_blueprint(db, employee, &preset.graph)?,
            None => false,
        };
        rows.push(json!({
            "id": preset.id,
            "name": preset.name,
            "layer": preset.layer,
            "board": preset.board,
            "role": preset.role,
            "workflowName": preset.workflow_name,
            "workflowDescription": preset.workflow_description,
            "channels": preset.channels,
            "schedule": preset.schedule,
            "quota": preset.quota,
            "installed": held.is_some(),
            "blueprintChanged": changed,
        }));
    }
    Ok(json!({ "presets": rows }))
}

/// Whether an installed employee still runs an older copy of its blueprint.
///
/// The two graphs are compared by content rather than by text: the console
/// re-serialises a graph when it saves one, so equal graphs can differ in key
/// order and a text comparison would call every inspected employee stale. A
/// stored graph that will not load counts as changed, because the only way it
/// differs from the blueprint is that it is broken.
fn holds_older_blueprint(db: &Database, employee: &Employee, blueprint: &Value) -> Result<bool> {
    let (Some(workflow_id), Some(version)) = (&employee.workflow_id, employee.workflow_version)
    else {
        return Ok(false);
    };
    let Some((_, raw)) = get_graph(db, workflow_id, version)? else {
        return Ok(false);
    };
    Ok(match serde_json::from_str::<Value>(&raw) {
        Ok(stored) => stored != *blueprint,
        Err(_) => true,
    })
}

/// Create the requested blueprints.
///
/// Idempotent by employee name, not by preset id: an owner may rename a seeded
/// employee, and the second call must still refuse to create a duplicate of
/// whichever name it already finds. A name the blueprint itself shipped under
/// before counts too, so renaming a blueprint never hires a second employee.
pub fn install_presets(
    db: &Database,
    secrets: &SecretStore,
    params: &Value,
) -> Result<Value> {
    let wanted: Vec<String> = match params.get("ids") {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(_) => bail!("INVALID_ARGUMENT: ids must be a list"),
        None => PRESET_IDS.iter().map(|id| id.to_string()).collect(),
    };
    for id in &wanted {
        if !PRESET_IDS.contains(&id.as_str()) {
            bail!("INVALID_ARGUMENT: unknown preset {id}");
        }
    }

    let api_key = optional_str(params, "apiKey");
    let base_url = optional_str(params, "baseUrl");
    let model_id = optional_str(params, "modelId");
    let provider_name =
        optional_str(params, "providerName").unwrap_or_else(|| "默认模型".to_string());
    let start_paused = params
        .get("startPaused")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    // Whether an employee already on payroll may have its blueprint rewritten.
    // Off by default: hiring someone is a different act from editing them.
    let adopt_blueprint = params
        .get("adoptBlueprint")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // The credential is created once and shared: two employees of the same
    // owner are two roles, not two accounts.
    let mut provider_id = None;
    let mut profile_id = None;
    if let Some(key) = api_key.as_deref() {
        let existing = list_providers(db, secrets, true)?
            .into_iter()
            .find(|provider| provider.name == provider_name);
        let provider = match existing {
            Some(provider) => provider,
            None => {
                let models = model_id.as_deref().map(|id| {
                    json!([{
                        "id": id,
                        "contextWindow": 128_000,
                        "maxTokens": 8_192,
                        "thinkingLevels": ["off"],
                    }])
                });
                let input: ProviderCreateInput = serde_json::from_value(json!({
                    "name": provider_name,
                    "type": "openai_compatible",
                    "protocol": "openai",
                    "baseUrl": base_url,
                    "authKind": "api_key",
                    "apiStyle": "chat_completions",
                    "defaultModelId": model_id,
                    "models": models,
                }))?;
                create_provider(db, secrets, input)?
            }
        };
        provider_id = Some(provider.id.clone());
        let label = format!("{provider_name} 凭据");
        let profile = match list_profiles(db, secrets)?
            .into_iter()
            .find(|profile| profile.label == label)
        {
            Some(profile) => profile,
            None => create_profile(
                db,
                secrets,
                &json!({
                    "label": label,
                    "providerId": provider.id,
                    "baseUrl": base_url,
                    "secretValue": key,
                }),
            )?,
        };
        profile_id = Some(profile.id);
    }

    let existing = list_employees(db)?;
    let mut installed = Vec::new();
    let mut updated = Vec::new();
    let mut skipped = Vec::new();
    for preset in presets() {
        if !wanted.iter().any(|id| id == preset.id) {
            continue;
        }
        // The name the owner actually holds is the one reported, so the
        // console's answer matches the roster they are looking at.
        if let Some(held) = preset.hired_as(&existing) {
            // An employee already on payroll is only rewritten when the caller
            // asks: hiring someone and editing them are different acts, and
            // doing the second under the name of the first is how a blueprint
            // becomes a migration. Adopting appends a version rather than
            // editing one, so the graph the employee had been running stays
            // readable afterwards.
            let employee = existing
                .iter()
                .find(|item| item.name == held)
                .ok_or_else(|| anyhow!("employee {held} vanished mid-install"))?;
            if adopt_blueprint {
                if let Some(workflow_id) = employee.workflow_id.clone() {
                    let workflow = adopt_graph(db, &workflow_id, &preset.graph, "preset")?
                        .ok_or_else(|| anyhow!("workflow {workflow_id} vanished mid-install"))?;
                    update_employee(
                        db,
                        &employee.id,
                        &json!({ "workflowVersion": workflow.latest_version }),
                    )?;
                    updated.push(held.to_string());
                    continue;
                }
            }
            skipped.push(held.to_string());
            continue;
        }
        let workflow = create_workflow(
            db,
            &json!({
                "name": preset.workflow_name,
                "description": preset.workflow_description,
                "status": "active",
                "graph": preset.graph,
            }),
        )?;
        let status = if start_paused || profile_id.is_none() {
            "paused"
        } else {
            "running"
        };
        create_employee(
            db,
            &json!({
                "name": preset.name,
                "role": preset.role,
                "layer": preset.layer,
                "board": preset.board,
                "status": status,
                "credentialProfileId": profile_id,
                "providerId": provider_id,
                "modelId": model_id,
                "workflowId": workflow.id,
                "workflowVersion": workflow.latest_version,
                "schedule": preset.schedule,
                "quota": preset.quota,
            }),
        )?;
        installed.push(preset.name.to_string());
    }

    Ok(json!({
        "installed": installed,
        "updated": updated,
        "skipped": skipped,
        "providerId": provider_id,
        "credentialProfileId": profile_id,
    }))
}
