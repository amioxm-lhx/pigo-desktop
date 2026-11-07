use super::*;
use crate::db::migration_backup_path;
use serde_json::json;

fn setup() -> (tempfile::TempDir, Database, SecretStore) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("pi.sqlite")).unwrap();
    let secrets = SecretStore::open(dir.path()).unwrap();
    (dir, db, secrets)
}

/// The graph one of the shipped blueprints carries, by preset id.
fn blueprint_graph(id: &str) -> Value {
    presets()
        .into_iter()
        .find(|preset| preset.id == id)
        .unwrap_or_else(|| panic!("no blueprint {id}"))
        .graph
}

/// The staff RPC surface takes one context instead of loose host handles.
fn ctx<'a>(db: &'a Database, secrets: &'a SecretStore) -> StaffContext<'a> {
    StaffContext {
        db,
        secrets,
        workspace: None,
    }
}

fn provider(db: &Database, id: &str) {
    db.conn()
        .execute(
            "INSERT INTO providers(id,name,created_at,updated_at) VALUES(?1,?2,?3,?3)",
            params![id, format!("Provider {id}"), now_ms()],
        )
        .unwrap();
}

fn profile(
    db: &Database,
    secrets: &SecretStore,
    label: &str,
    provider_id: &str,
    key: &str,
) -> String {
    let created = create_profile(
        db,
        secrets,
        &json!({ "label": label, "providerId": provider_id, "secretValue": key }),
    )
    .unwrap();
    assert!(created.has_secret);
    created.id
}

/// A three-node graph: trigger, probe, then promote what the probe learned.
fn graph() -> Value {
    json!({
        "nodes": [
            { "id": "t1", "kind": "trigger", "config": { "mode": "schedule" } },
            {
                "id": "p1",
                "kind": "probe",
                "title": "Post a probe",
                "instruction": "Post a probe and read the replies.",
                "config": { "channel": "forum" }
            },
            {
                "id": "m1",
                "kind": "memory",
                "config": { "operation": "publish", "scope": "shared", "kind": "signal" }
            }
        ],
        "edges": [
            { "from": "t1", "to": "p1" },
            { "from": "p1", "to": "m1" }
        ]
    })
}

fn seed_workflow(db: &Database, name: &str) -> (String, i64) {
    let created = create_workflow(
        db,
        &json!({ "name": name, "status": "active", "graph": graph() }),
    )
    .unwrap();
    (created.id, created.latest_version)
}

fn employee(db: &Database, name: &str, profile_id: &str, model: &str) -> String {
    let (workflow_id, workflow_version) = seed_workflow(db, &format!("{name} workflow"));
    create_employee(
        db,
        &json!({
            "name": name,
            "role": "promoter",
            "credentialProfileId": profile_id,
            "providerId": "vendor-a",
            "modelId": model,
            "workflowId": workflow_id,
            "workflowVersion": workflow_version,
            "status": "running"
        }),
    )
    .unwrap()
    .id
}

fn private(db: &Database, owner: &str, text: &str) -> MemoryItem {
    write_memory(
        db,
        MemoryWriteInput::from_json(&json!({
            "scope": "employee",
            "ownerId": owner,
            "kind": "signal",
            "text": text,
            "sourceKind": "interaction",
            "sourceRef": "https://example.test/thread/1"
        }))
        .unwrap(),
    )
    .unwrap()
}

fn shared(db: &Database, kind: &str, key: &str, text: &str) -> Result<MemoryItem> {
    publish_memory(
        db,
        MemoryWriteInput::from_json(&json!({
            "scope": "shared",
            "kind": kind,
            "key": key,
            "text": text,
            "sourceKind": "run",
            "sourceRef": "run-1"
        }))
        .unwrap(),
    )
}

#[test]
fn employees_run_on_their_own_profile_and_model_and_the_run_records_the_payer() {
    let (_dir, db, secrets) = setup();
    provider(&db, "vendor-a");
    let company = profile(&db, &secrets, "Company key", "vendor-a", "sk-company");
    let personal = profile(&db, &secrets, "Personal key", "vendor-a", "sk-personal");
    let scout = employee(&db, "Scout", &company, "gpt-5.6-sol");
    let closer = employee(&db, "Closer", &personal, "gpt-6-astra");

    let scout_binding = resolve_run_binding(&db, &secrets, &scout).unwrap();
    let closer_binding = resolve_run_binding(&db, &secrets, &closer).unwrap();
    assert_eq!(scout_binding.api_key.as_deref(), Some("sk-company"));
    assert_eq!(scout_binding.model_id.as_deref(), Some("gpt-5.6-sol"));
    assert_eq!(closer_binding.api_key.as_deref(), Some("sk-personal"));
    assert_eq!(closer_binding.model_id.as_deref(), Some("gpt-6-astra"));
    assert_ne!(
        scout_binding.credential_profile_id,
        closer_binding.credential_profile_id
    );

    let session = crate::sessions::create_session_with_options(
        &db,
        crate::sessions::SessionCreateOptions {
            title: Some("Scout run".into()),
            mode: Some("agent".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let run = begin_run(&db, &scout, Some("schedule"), Some(&session.id)).unwrap();
    assert_eq!(run.credential_profile_id.as_deref(), Some(company.as_str()));
    assert_eq!(run.model_id.as_deref(), Some("gpt-5.6-sol"));
    assert_eq!(run.status, "running");

    // The payer is snapshotted on the run, so reassigning the employee later
    // cannot rewrite which account paid.
    update_employee(&db, &scout, &json!({ "credentialProfileId": personal })).unwrap();
    let runs = list_runs(&db, Some(&scout), 10).unwrap();
    assert_eq!(
        runs[0].credential_profile_id.as_deref(),
        Some(company.as_str())
    );

    assert!(finish_run(&db, &run.id, "succeeded", None).unwrap());
    assert_eq!(
        list_runs(&db, Some(&scout), 10).unwrap()[0].status,
        "succeeded"
    );
}

#[test]
fn no_row_carries_key_material() {
    let (_dir, db, secrets) = setup();
    provider(&db, "vendor-a");
    let profile_id = profile(&db, &secrets, "Company key", "vendor-a", "sk-company");
    let employee_id = employee(&db, "Scout", &profile_id, "gpt-5.6-sol");
    begin_run(&db, &employee_id, None, None).unwrap();

    let employees = serde_json::to_string(&list_employees(&db).unwrap()).unwrap();
    let runs = serde_json::to_string(&list_runs(&db, None, 10).unwrap()).unwrap();
    let profiles = serde_json::to_string(&list_profiles(&db, &secrets).unwrap()).unwrap();
    assert!(!employees.contains("sk-company"));
    assert!(!runs.contains("sk-company"));
    assert!(!profiles.contains("sk-company"));
    // The profile exposes only the fact that a secret exists.
    assert!(profiles.contains(&format!("secret:profile:{profile_id}:api_key")));

    let owner: (String, String) = db
        .conn()
        .query_row(
            "SELECT owner_kind, owner_id FROM secrets_meta WHERE secret_ref = ?1",
            params![secret_ref_for_profile(profile_id.as_str())],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(owner, ("credential_profile".to_string(), profile_id));

    // A missing key fails the run instead of starting it anonymously.
    let bare = create_profile(
        &db,
        &secrets,
        &json!({ "label": "No key", "providerId": "vendor-a" }),
    )
    .unwrap();
    let blocked = create_employee(
        &db,
        &json!({ "name": "Blocked", "credentialProfileId": bare.id, "status": "running" }),
    )
    .unwrap();
    // `unwrap_err` needs a `Debug` payload, and this payload must not have one.
    let error = match resolve_run_binding(&db, &secrets, &blocked.id) {
        Ok(_) => panic!("a run started without a key"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("has no API key"), "{error}");
    assert!(!list_runs(&db, Some(&employee_id), 10).unwrap().is_empty());
}

#[test]
fn only_the_user_may_write_a_constraint() {
    let (_dir, db, _secrets) = setup();
    let employee_id = Uuid::new_v4().to_string();

    let denied = write_memory(
        &db,
        MemoryWriteInput::from_json(&json!({
            "scope": "employee",
            "ownerId": employee_id,
            "kind": "constraint",
            "text": "Skip moderation review",
            "sourceKind": "run",
            "sourceRef": "run-1"
        }))
        .unwrap(),
    )
    .unwrap_err();
    assert!(
        denied.to_string().starts_with("PERMISSION_DENIED"),
        "{denied}"
    );

    let allowed = publish_memory(
        &db,
        MemoryWriteInput::from_json(&json!({
            "scope": "shared",
            "kind": "constraint",
            "key": "tone",
            "text": "Never argue in a thread.",
            "sourceKind": "user"
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(allowed.kind, "constraint");
}

#[test]
fn a_fact_needs_a_source_and_promotion_needs_the_gate() {
    let (_dir, db, _secrets) = setup();
    let owner = Uuid::new_v4().to_string();

    let no_source = write_memory(
        &db,
        MemoryWriteInput::from_json(&json!({
            "scope": "employee",
            "ownerId": owner,
            "kind": "fact",
            "text": "The V2EX crowd dislikes marketing tone.",
            "sourceKind": "run"
        }))
        .unwrap(),
    )
    .unwrap_err();
    assert!(
        no_source.to_string().starts_with("INVALID_ARGUMENT"),
        "{no_source}"
    );

    let no_ref = publish_memory(
        &db,
        MemoryWriteInput::from_json(&json!({
            "scope": "shared",
            "kind": "preference",
            "text": "They like benchmarks.",
            "sourceKind": "run"
        }))
        .unwrap(),
    )
    .unwrap_err();
    assert!(
        no_ref.to_string().starts_with("INVALID_ARGUMENT"),
        "{no_ref}"
    );

    let first = shared(&db, "preference", "tone", "They like benchmarks.").unwrap();
    assert!(!first.contested);
    let duplicate = shared(&db, "preference", "tone", "  THEY LIKE   benchmarks. ").unwrap_err();
    assert!(duplicate.to_string().starts_with("CONFLICT"), "{duplicate}");
    assert_eq!(query_memory(&db, &MemoryFilter::shared()).unwrap().len(), 1);

    // A playbook is not something a run can mint for itself.
    let playbook = write_memory(
        &db,
        MemoryWriteInput::from_json(&json!({
            "scope": "employee",
            "ownerId": owner,
            "kind": "playbook",
            "text": "Post at 9am.",
            "sourceKind": "run",
            "sourceRef": "run-1"
        }))
        .unwrap(),
    )
    .unwrap_err();
    assert!(
        playbook.to_string().starts_with("PERMISSION_DENIED"),
        "{playbook}"
    );
}

#[test]
fn a_live_constraint_blocks_publishing_its_key() {
    let (_dir, db, _secrets) = setup();
    publish_memory(
        &db,
        MemoryWriteInput::from_json(&json!({
            "scope": "shared",
            "kind": "constraint",
            "key": "channel/v2ex",
            "text": "Do not post to V2EX.",
            "sourceKind": "user"
        }))
        .unwrap(),
    )
    .unwrap();

    let blocked = shared(&db, "playbook", "channel/v2ex", "Post a probe at 9am.").unwrap_err();
    assert!(
        blocked.to_string().starts_with("PERMISSION_DENIED"),
        "{blocked}"
    );
}

#[test]
fn private_memory_is_private_and_shared_memory_is_readable_by_everyone() {
    let (_dir, db, _secrets) = setup();
    let scout = Uuid::new_v4().to_string();
    let closer = Uuid::new_v4().to_string();

    private(&db, &scout, "Thread 12 got 40 replies about pricing.");
    shared(
        &db,
        "preference",
        "pricing",
        "Pricing questions close fastest.",
    )
    .unwrap();

    let scout_items = query_memory(&db, &MemoryFilter::for_employee(&scout)).unwrap();
    let closer_items = query_memory(&db, &MemoryFilter::for_employee(&closer)).unwrap();
    assert_eq!(scout_items.len(), 2);
    assert_eq!(closer_items.len(), 1);
    assert_eq!(closer_items[0].scope, "shared");

    // Reading another employee's scope needs its owner id.
    let denied = query_memory(
        &db,
        &MemoryFilter {
            scope: Some("employee".into()),
            ..MemoryFilter::default()
        },
    )
    .unwrap_err();
    assert!(
        denied.to_string().starts_with("INVALID_ARGUMENT"),
        "{denied}"
    );
}

#[test]
fn retrieval_drops_superseded_retired_and_expired_items() {
    let (_dir, db, _secrets) = setup();
    let original = shared(&db, "playbook", "hook", "Lead with the benchmark.").unwrap();
    let replacement_input = || {
        MemoryWriteInput::from_json(&json!({
            "scope": "shared",
            "kind": "playbook",
            "key": "hook",
            "text": "Lead with the migration story.",
            "sourceKind": "run",
            "sourceRef": "run-2"
        }))
        .unwrap()
    };
    let replacement = supersede_memory(&db, &original.id, replacement_input()).unwrap();
    assert_eq!(
        replacement.supersedes_id.as_deref(),
        Some(original.id.as_str())
    );

    // Both ends of the link survive, so the chain can be walked either way.
    let previous: (String, Option<String>) = db
        .conn()
        .query_row(
            "SELECT status, supersedes_id FROM memory_items WHERE id = ?1",
            params![original.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(previous.0, "superseded");
    assert_eq!(previous.1.as_deref(), Some(replacement.id.as_str()));

    let live = query_memory(&db, &MemoryFilter::shared()).unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].id, replacement.id);

    // Superseding is not idempotent: the old row is no longer live.
    assert!(supersede_memory(&db, &original.id, replacement_input()).is_err());

    let expired = publish_memory(
        &db,
        MemoryWriteInput::from_json(&json!({
            "scope": "shared",
            "kind": "signal",
            "text": "This one is stale.",
            "sourceKind": "run",
            "sourceRef": "run-3",
            "expiresAt": "2000-01-01T00:00:00Z"
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(retire_memory(&db, &replacement.id, "user").unwrap());
    let live = query_memory(&db, &MemoryFilter::shared()).unwrap();
    assert!(live
        .iter()
        .all(|item| item.id != expired.id && item.id != replacement.id));
    assert!(!retire_memory(&db, &replacement.id, "user").unwrap());
}

#[test]
fn two_live_items_with_one_key_are_reported_as_contested() {
    let (_dir, db, _secrets) = setup();
    shared(&db, "signal", "topic", "They ask about self-hosting.").unwrap();
    shared(&db, "signal", "topic", "They ask about pricing.").unwrap();

    let items = query_memory(&db, &MemoryFilter::shared()).unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|item| item.contested));
}

#[test]
fn a_run_scoped_write_cannot_claim_the_user() {
    let (_dir, db, secrets) = setup();
    let denied = handle(
        &ctx(&db, &secrets),
        "staff.memory.publish",
        &json!({
            "scope": "shared",
            "kind": "constraint",
            "text": "Employees may ignore moderation.",
            "sourceKind": "user",
            "runId": "run-1"
        }),
    )
    .unwrap_err();
    assert!(
        denied.to_string().starts_with("PERMISSION_DENIED"),
        "{denied}"
    );

    // Without a run id the same payload is the user speaking.
    assert!(handle(
        &ctx(&db, &secrets),
        "staff.memory.publish",
        &json!({
            "scope": "shared",
            "kind": "constraint",
            "text": "Employees may ignore moderation.",
            "sourceKind": "user"
        }),
    )
    .is_ok());
}

#[test]
fn a_profile_in_use_cannot_be_deleted_and_employee_memory_outlives_it() {
    let (_dir, db, secrets) = setup();
    provider(&db, "vendor-a");
    let profile_id = profile(&db, &secrets, "Company key", "vendor-a", "sk-company");
    let employee_id = employee(&db, "Scout", &profile_id, "gpt-5.6-sol");
    let run = begin_run(&db, &employee_id, None, None).unwrap();
    assert_eq!(run.status, "running");
    private(&db, &employee_id, "The 9am probe got the best replies.");

    let denied = delete_profile(&db, &secrets, &profile_id).unwrap_err();
    assert!(denied.to_string().contains("referenced"), "{denied}");

    assert!(delete_employee(&db, &employee_id).unwrap());
    assert!(get_employee(&db, &employee_id).unwrap().is_none());
    // Runs cascade, memory does not: what the employee learned stays readable.
    assert!(list_runs(&db, Some(&employee_id), 10).unwrap().is_empty());
    assert_eq!(
        query_memory(
            &db,
            &MemoryFilter {
                scope: Some("employee".into()),
                owner_id: Some(employee_id.clone()),
                ..MemoryFilter::default()
            }
        )
        .unwrap()
        .len(),
        1
    );
    assert!(delete_profile(&db, &secrets, &profile_id).unwrap());
    assert!(list_profiles(&db, &secrets).unwrap().is_empty());
    assert!(!secrets.has(&secret_ref_for_profile(&profile_id)));
}

#[test]
fn migration_from_v16_adds_the_staff_tables_and_keeps_every_existing_row() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    {
        let db = Database::open(&path).unwrap();
        provider(&db, "vendor-a");
        crate::sessions::create_session_with_options(
            &db,
            crate::sessions::SessionCreateOptions {
                title: Some("Existing conversation".into()),
                mode: Some("agent".into()),
                ..Default::default()
            },
        )
        .unwrap();
        // Rewind to v16: the staff tables did not exist at that version.
        db.conn()
            .execute_batch(
                "DROP INDEX idx_memory_supersedes;
                 DROP INDEX idx_memory_key;
                 DROP INDEX idx_memory_owner;
                 DROP INDEX idx_memory_lookup;
                 DROP TABLE memory_items;
                 DROP INDEX idx_employee_runs;
                 DROP TABLE employee_runs;
                 DROP INDEX idx_employees_status;
                 DROP TABLE employees;
                 DROP INDEX idx_credential_profiles_provider;
                 DROP TABLE credential_profiles;",
            )
            .unwrap();
        db.conn().pragma_update(None, "user_version", 16).unwrap();
    }

    let db = Database::open(&path).unwrap();
    assert_eq!(
        db.conn()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        SCHEMA_VERSION
    );
    assert_eq!(SCHEMA_VERSION, 22);
    for table in [
        "credential_profiles",
        "employees",
        "employee_runs",
        "memory_items",
        "staff_leads",
        "staff_contents",
        "staff_metrics",
        "staff_suggestions",
        "staff_directives",
        "staff_decisions",
    ] {
        let exists: bool = db
            .conn()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                params![table],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists, "{table} missing after migration");
    }
    assert!(list_profiles(&db, &SecretStore::open(dir.path()).unwrap())
        .unwrap()
        .is_empty());
    let sessions: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
        .unwrap();
    assert_eq!(sessions, 1);
    assert!(migration_backup_path(&path, 16).exists());
}

#[test]
fn migration_from_v17_adds_the_run_results_column_and_keeps_every_row() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    let secrets = SecretStore::open(dir.path()).unwrap();
    let (employee_id, run_id) = {
        let db = Database::open(&path).unwrap();
        provider(&db, "vendor-a");
        let profile_id = profile(&db, &secrets, "Company key", "vendor-a", "sk-company");
        let employee_id = employee(&db, "Scout", &profile_id, "gpt-5.6-sol");
        let run = begin_run(&db, &employee_id, Some("manual"), None).unwrap();
        // Rewind to v17: neither run results column existed at that version.
        db.conn()
            .execute_batch(
                "ALTER TABLE employee_runs DROP COLUMN results_json;
                 ALTER TABLE employee_runs DROP COLUMN reports_json;",
            )
            .unwrap();
        db.conn().pragma_update(None, "user_version", 17).unwrap();
        (employee_id, run.id)
    };

    let db = Database::open(&path).unwrap();
    let version: i64 = db
        .conn()
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    assert!(migration_backup_path(&path, 17).exists());

    // The pre-existing run survives and reads as a run with no results yet.
    let run = get_run(&db, &run_id).unwrap().unwrap();
    assert_eq!(run.results, json!({}));
    assert_eq!(run.reports, json!({}));
    assert!(run.step_titles.len() >= 1);

    // A new run records what its steps reported, under one-based numbers.
    let run = begin_run(&db, &employee_id, Some("manual"), None).unwrap();
    assert!(set_run_step(
        &db,
        &run.id,
        1,
        Some("Posted the probe."),
        Some("Visited three channels. Collected: link-a, link-b.\nObservations follow.")
    )
    .unwrap());
    let run = get_run(&db, &run.id).unwrap().unwrap();
    assert_eq!(run.current_step, 1);
    assert_eq!(run.results["1"], "Posted the probe.");
    assert_eq!(run.step_titles[0], "Post a probe");
    assert!(run.reports["1"].as_str().unwrap().contains("link-b"));

    // A bare progress mark still works and touches neither stored column.
    assert!(set_run_step(&db, &run.id, 2, None, None).unwrap());
    let run = get_run(&db, &run.id).unwrap().unwrap();
    assert_eq!(run.current_step, 2);
    assert_eq!(run.results["1"], "Posted the probe.");
    assert!(run.reports["1"].as_str().unwrap().contains("link-b"));
}

#[test]
fn a_v14_database_walks_the_whole_chain_to_the_latest_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    {
        let db = Database::open(&path).unwrap();
        db.conn()
            .execute_batch(
                "DROP INDEX idx_turn_queue_idempotency;
                 DROP INDEX idx_turn_queue_session;
                 DROP TABLE turn_queue;",
            )
            .unwrap();
        db.conn().pragma_update(None, "user_version", 14).unwrap();
    }
    let db = Database::open(&path).unwrap();
    let version: i64 = db
        .conn()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    // Later steps must not be skipped by an earlier step stamping ahead.
    let has_memory: bool = db
        .conn()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='memory_items')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(has_memory);
}

#[test]
fn a_fresh_database_never_runs_the_staff_migration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    drop(Database::open(&path).unwrap());
    drop(Database::open(&path).unwrap());
    assert!(!migration_backup_path(&path, 16).exists());
}

#[test]
fn a_replacement_cannot_change_the_scope_of_what_it_replaces() {
    let (_dir, db, _secrets) = setup();
    let owner = Uuid::new_v4().to_string();
    let shared_item = shared(&db, "preference", "hook", "They like the benchmark.").unwrap();

    let escaped = supersede_memory(
        &db,
        &shared_item.id,
        MemoryWriteInput::from_json(&json!({
            "scope": "employee",
            "ownerId": owner,
            "kind": "preference",
            "text": "They like the benchmark, privately.",
            "sourceKind": "run",
            "sourceRef": "run-9"
        }))
        .unwrap(),
    )
    .unwrap_err();
    assert!(escaped.to_string().starts_with("CONFLICT"), "{escaped}");
    assert_eq!(query_memory(&db, &MemoryFilter::shared()).unwrap().len(), 1);
}

#[test]
fn a_run_cannot_retire_a_constraint_but_the_user_can() {
    let (_dir, db, secrets) = setup();
    let constraint = publish_memory(
        &db,
        MemoryWriteInput::from_json(&json!({
            "scope": "shared",
            "kind": "constraint",
            "key": "channel/v2ex",
            "text": "Do not post to V2EX.",
            "sourceKind": "user"
        }))
        .unwrap(),
    )
    .unwrap();

    let denied = handle(
        &ctx(&db, &secrets),
        "staff.memory.retire",
        &json!({ "id": constraint.id, "runId": "run-1" }),
    )
    .unwrap_err();
    assert!(
        denied.to_string().starts_with("PERMISSION_DENIED"),
        "{denied}"
    );

    assert!(handle(
        &ctx(&db, &secrets),
        "staff.memory.retire",
        &json!({ "id": constraint.id })
    )
    .is_ok());
    assert!(query_memory(&db, &MemoryFilter::shared())
        .unwrap()
        .is_empty());
}

fn scheduled_employee(db: &Database, profile_id: &str, schedule: Value) -> String {
    let (workflow_id, workflow_version) = seed_workflow(db, "Scheduled flow");
    create_employee(
        db,
        &json!({
            "name": "Scout",
            "credentialProfileId": profile_id,
            "providerId": "vendor-a",
            "modelId": "gpt-5.6-sol",
            "workflowId": workflow_id,
            "workflowVersion": workflow_version,
            "schedule": schedule,
            "status": "running"
        }),
    )
    .unwrap()
    .id
}

/// Pull a due stamp into the past so the next claim sees the employee.
fn make_due(db: &Database, employee_id: &str) {
    db.conn()
        .execute(
            "UPDATE employees SET next_run_at = ?2 WHERE id = ?1",
            params![employee_id, now_ms() - 1000],
        )
        .unwrap();
}

fn next_run_at(db: &Database, employee_id: &str) -> Option<i64> {
    db.conn()
        .query_row(
            "SELECT next_run_at FROM employees WHERE id = ?1",
            params![employee_id],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn a_graph_is_validated_before_it_is_stored() {
    let (_dir, db, _secrets) = setup();

    let good = parse_graph(&graph()).unwrap();
    assert_eq!(good.node_count(), 3);
    let steps = good.compile().unwrap();
    // The trigger starts the run; it is not a step the run performs.
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].node_id, "p1");
    assert_eq!(steps[0].kind, "probe");
    assert_eq!(steps[1].kind, "memory");
    let plan = steps[1].memory.as_ref().unwrap();
    assert_eq!(plan.operation, "publish");
    assert_eq!(plan.kind.as_deref(), Some("signal"));

    let cases: Vec<(&str, Value)> = vec![
        (
            "no trigger",
            json!({ "nodes": [ { "id": "a1", "kind": "agent", "instruction": "x" } ] }),
        ),
        (
            "two triggers",
            json!({
                "nodes": [
                    { "id": "t1", "kind": "trigger", "config": { "mode": "manual" } },
                    { "id": "t2", "kind": "trigger", "config": { "mode": "manual" } }
                ]
            }),
        ),
        (
            "unknown kind",
            json!({ "nodes": [ { "id": "t1", "kind": "trigger", "config": { "mode": "manual" } },
                                 { "id": "x1", "kind": "teleport" } ],
                     "edges": [ { "from": "t1", "to": "x1" } ] }),
        ),
        (
            "unreachable",
            json!({ "nodes": [ { "id": "t1", "kind": "trigger", "config": { "mode": "manual" } },
                                 { "id": "a1", "kind": "agent", "instruction": "x" },
                                 { "id": "a2", "kind": "agent", "instruction": "y" } ],
                     "edges": [ { "from": "t1", "to": "a1" } ] }),
        ),
        (
            "cycle",
            json!({ "nodes": [ { "id": "t1", "kind": "trigger", "config": { "mode": "manual" } },
                                 { "id": "a1", "kind": "agent", "instruction": "x" },
                                 { "id": "a2", "kind": "agent", "instruction": "y" } ],
                     "edges": [ { "from": "t1", "to": "a1" }, { "from": "a1", "to": "a2" },
                                 { "from": "a2", "to": "a1" } ] }),
        ),
        (
            "dangling edge",
            json!({ "nodes": [ { "id": "t1", "kind": "trigger", "config": { "mode": "manual" } } ],
                     "edges": [ { "from": "t1", "to": "ghost" } ] }),
        ),
        (
            "probe without a channel",
            json!({ "nodes": [ { "id": "t1", "kind": "trigger", "config": { "mode": "manual" } },
                                 { "id": "p1", "kind": "probe", "config": {} } ],
                     "edges": [ { "from": "t1", "to": "p1" } ] }),
        ),
        (
            "memory without an operation",
            json!({ "nodes": [ { "id": "t1", "kind": "trigger", "config": { "mode": "manual" } },
                                 { "id": "m1", "kind": "memory", "config": { "kind": "signal" } } ],
                     "edges": [ { "from": "t1", "to": "m1" } ] }),
        ),
    ];
    for (label, value) in cases {
        assert!(parse_graph(&value).is_err(), "{label} should not validate");
    }
    assert!(create_workflow(&db, &json!({ "name": "Broken", "graph": { "nodes": [] } })).is_err());

    // The editor asks first, and gets a reason instead of an error.
    let verdict = handle(
        &ctx(&db, &_secrets),
        "staff.workflow.validate",
        &json!({ "graph": { "nodes": [], "edges": [] } }),
    )
    .unwrap();
    assert_eq!(verdict["valid"], false);
    assert!(verdict["error"]
        .as_str()
        .unwrap()
        .contains("at least one node"));
    let verdict = handle(
        &ctx(&db, &_secrets),
        "staff.workflow.validate",
        &json!({ "graph": graph() }),
    )
    .unwrap();
    assert_eq!(verdict["valid"], true);
    assert_eq!(verdict["steps"], 2);
}

#[test]
fn a_workflow_version_is_immutable_and_a_pinned_run_keeps_its_revision() {
    let (_dir, db, secrets) = setup();
    provider(&db, "vendor-a");
    let profile_id = profile(&db, &secrets, "Company key", "vendor-a", "sk-company");
    let workflow = create_workflow(&db, &json!({ "name": "Promote", "graph": graph() })).unwrap();
    assert_eq!(workflow.latest_version, 1);

    let employee_id = create_employee(
        &db,
        &json!({
            "name": "Scout",
            "credentialProfileId": profile_id,
            "workflowId": workflow.id,
            "workflowVersion": 1,
            "status": "running"
        }),
    )
    .unwrap()
    .id;

    // Editing the graph appends a version; version 1 keeps its bytes.
    let mut changed = graph();
    changed["nodes"][1]["config"]["channel"] = json!("video");
    let updated = update_workflow(&db, &workflow.id, &json!({ "graph": changed })).unwrap();
    assert_eq!(updated.unwrap().latest_version, 2);
    let (v1, _) = get_graph(&db, &workflow.id, 1).unwrap().unwrap();
    assert_eq!(v1.compile().unwrap()[0].kind, "probe");

    let run = begin_run(&db, &employee_id, Some("manual"), None).unwrap();
    assert_eq!(run.workflow_version, Some(1));
    assert_eq!(run.step_count, 2);
    assert_eq!(list_versions(&db, &workflow.id).unwrap().len(), 2);

    let denied = delete_workflow(&db, &workflow.id).unwrap_err();
    assert!(denied.to_string().starts_with("CONFLICT"), "{denied}");

    // A pin that does not exist is refused, and so is a half pin.
    assert!(create_employee(
        &db,
        &json!({ "name": "Ghost", "workflowId": workflow.id, "workflowVersion": 9 })
    )
    .is_err());
    assert!(create_employee(&db, &json!({ "name": "Half", "workflowId": workflow.id })).is_err());
}

#[test]
fn an_interval_schedule_is_deterministic_and_jitter_stays_in_range() {
    let every = Schedule::parse(&json!({ "everyMinutes": 60 }))
        .unwrap()
        .unwrap();
    let now = 1_700_000_000_000i64;
    let anchor = now - 90 * 60_000;
    let first = every.next_after(now, anchor, "employee-1").unwrap();
    let second = every.next_after(now, anchor, "employee-1").unwrap();
    assert_eq!(first, second, "the same inputs must schedule the same run");
    assert!(first > now);
    assert_eq!((first - anchor) % (60 * 60_000), 0, "the grid is anchored");

    // Jitter moves the slot, but never outside the window, and it is stable.
    let jittered = Schedule::parse(&json!({ "everyMinutes": 60, "jitterMinutes": 10 }))
        .unwrap()
        .unwrap();
    let shifted = jittered.next_after(now, anchor, "employee-1").unwrap();
    assert!(shifted >= first && shifted < first + 10 * 60_000);
    assert_eq!(
        shifted,
        jittered.next_after(now, anchor, "employee-1").unwrap()
    );

    // Times of day are read in the configured offset, not in UTC.
    let daily = Schedule::parse(&json!({
        "timesOfDay": ["09:00", "21:00"],
        "utcOffsetMinutes": 480
    }))
    .unwrap()
    .unwrap();
    let local_noon = now - (now % 86_400_000) + 12 * 60 * 60_000 - 480 * 60_000;
    let next = daily
        .next_after(local_noon, local_noon, "employee-1")
        .unwrap();
    assert_eq!(
        next,
        local_noon + 9 * 60 * 60_000,
        "the next slot is 21:00 local"
    );

    assert!(Schedule::parse(&json!({ "everyMinutes": 1 })).is_err());
    assert!(Schedule::parse(&json!({ "everyMinutes": 60, "timesOfDay": ["09:00"] })).is_err());
    assert!(Schedule::parse(&json!({ "timesOfDay": ["25:00"] })).is_err());
    assert!(Schedule::parse(&json!({})).unwrap().is_none());
}

#[test]
fn a_scheduled_employee_must_pin_a_workflow() {
    let (_dir, db, _secrets) = setup();
    let denied = create_employee(
        &db,
        &json!({ "name": "Loop", "schedule": { "everyMinutes": 60 }, "status": "running" }),
    )
    .unwrap_err();
    assert!(
        denied.to_string().starts_with("INVALID_ARGUMENT"),
        "{denied}"
    );

    let (workflow_id, version) = seed_workflow(&db, "Pinned");
    let employee_id = create_employee(
        &db,
        &json!({
            "name": "Loop",
            "workflowId": workflow_id,
            "workflowVersion": version,
            "schedule": { "everyMinutes": 60 },
            "status": "running"
        }),
    )
    .unwrap()
    .id;
    assert!(next_run_at(&db, &employee_id).is_some());

    // Dropping the pin while the schedule is still set is refused: an
    // unattended employee without a pinned revision has nothing to follow.
    let denied = update_employee(&db, &employee_id, &json!({ "workflowId": null })).unwrap_err();
    assert!(
        denied.to_string().starts_with("INVALID_ARGUMENT"),
        "{denied}"
    );

    // Clearing the schedule clears the pending slot, and re-adding it is fine.
    update_employee(&db, &employee_id, &json!({ "schedule": {} })).unwrap();
    assert!(next_run_at(&db, &employee_id).is_none());
    update_employee(
        &db,
        &employee_id,
        &json!({ "schedule": { "everyMinutes": 60 } }),
    )
    .unwrap();
    assert!(next_run_at(&db, &employee_id).is_some());

    // A paused employee owns no pending slot.
    update_employee(&db, &employee_id, &json!({ "status": "paused" })).unwrap();
    assert!(next_run_at(&db, &employee_id).is_none());
}

#[test]
fn claiming_a_due_employee_opens_one_session_and_advances_the_slot() {
    let (_dir, db, secrets) = setup();
    provider(&db, "vendor-a");
    let profile_id = profile(&db, &secrets, "Company key", "vendor-a", "sk-company");
    let employee_id = scheduled_employee(&db, &profile_id, json!({ "everyMinutes": 60 }));
    make_due(&db, &employee_id);

    let batch = claim_due(&ctx(&db, &secrets), 8).unwrap();
    assert_eq!(batch.claims.len(), 1);
    assert!(batch.skipped.is_empty());
    let claim = &batch.claims[0];
    assert_eq!(claim.employee_id, employee_id);
    assert_eq!(claim.steps.len(), 2);
    assert!(!claim.session_id.is_empty());
    assert_eq!(claim.workflow_version, 1);
    assert!(crate::sessions::get_session(&db, &claim.session_id)
        .unwrap()
        .is_some());
    assert!(next_run_at(&db, &employee_id).unwrap() > now_ms());

    // Claiming is not repeatable: the slot already moved.
    assert!(claim_due(&ctx(&db, &secrets), 8).unwrap().claims.is_empty());

    // A paused employee is never claimed, even with a stale stamp.
    update_employee(&db, &employee_id, &json!({ "status": "paused" })).unwrap();
    make_due(&db, &employee_id);
    assert!(claim_due(&ctx(&db, &secrets), 8).unwrap().claims.is_empty());

    // An employee whose key is gone is reported, not silently skipped: nobody is
    // watching at 3am, so the reason has to be readable afterwards.
    let bare = create_profile(
        &db,
        &secrets,
        &json!({ "label": "No key yet", "providerId": "vendor-a" }),
    )
    .unwrap();
    let (workflow_id, workflow_version) = seed_workflow(&db, "Unkeyed flow");
    let unkeyed = create_employee(
        &db,
        &json!({
            "name": "Unkeyed",
            "credentialProfileId": bare.id,
            "workflowId": workflow_id,
            "workflowVersion": workflow_version,
            "schedule": { "everyMinutes": 60 },
            "status": "running"
        }),
    )
    .unwrap()
    .id;
    make_due(&db, &unkeyed);
    let batch = claim_due(&ctx(&db, &secrets), 8).unwrap();
    assert!(batch.claims.is_empty());
    assert_eq!(batch.skipped.len(), 1);
    assert!(
        batch.skipped[0].error.contains("API key"),
        "{}",
        batch.skipped[0].error
    );
    assert!(next_run_at(&db, &unkeyed).unwrap() > now_ms());
}

#[test]
fn a_failed_run_backs_off_and_a_quota_caps_a_runaway_schedule() {
    let (_dir, db, secrets) = setup();
    provider(&db, "vendor-a");
    let profile_id = profile(&db, &secrets, "Company key", "vendor-a", "sk-company");
    let employee_id = scheduled_employee(&db, &profile_id, json!({ "everyMinutes": 5 }));
    make_due(&db, &employee_id);

    let batch = claim_due(&ctx(&db, &secrets), 8).unwrap();
    let run_id = batch.claims[0].run_id.clone();
    assert!(schedule::finish_run(&db, &run_id, "failed", Some("PROVIDER_ERROR")).unwrap());
    let after_failure = next_run_at(&db, &employee_id).unwrap();
    assert!(
        after_failure >= now_ms() + 9 * 60_000,
        "first failure waits"
    );

    // The second failure waits longer than the first.
    make_due(&db, &employee_id);
    let batch = claim_due(&ctx(&db, &secrets), 8).unwrap();
    assert!(schedule::finish_run(&db, &batch.claims[0].run_id, "failed", None).unwrap());
    assert!(next_run_at(&db, &employee_id).unwrap() > after_failure);

    // A success returns to the schedule instead of the backoff.
    make_due(&db, &employee_id);
    let batch = claim_due(&ctx(&db, &secrets), 8).unwrap();
    assert!(schedule::finish_run(&db, &batch.claims[0].run_id, "succeeded", None).unwrap());
    let after_success = next_run_at(&db, &employee_id).unwrap();
    assert!(after_success < after_failure);

    // Quota: one run a day, so the employee is held instead of hammering.
    let capped = scheduled_employee(&db, &profile_id, json!({ "everyMinutes": 5 }));
    db.conn()
        .execute(
            "UPDATE employees SET quota_json = ?2 WHERE id = ?1",
            params![capped, json!({ "maxRunsPerDay": 1 }).to_string()],
        )
        .unwrap();
    make_due(&db, &capped);
    assert_eq!(claim_due(&ctx(&db, &secrets), 8).unwrap().claims.len(), 1);
    make_due(&db, &capped);
    let batch = claim_due(&ctx(&db, &secrets), 8).unwrap();
    assert!(batch.claims.iter().all(|claim| claim.employee_id != capped));
    assert!(next_run_at(&db, &capped).unwrap() > now_ms());
}

#[test]
fn boot_recovery_closes_orphaned_runs_and_reschedules() {
    let (_dir, db, secrets) = setup();
    provider(&db, "vendor-a");
    let profile_id = profile(&db, &secrets, "Company key", "vendor-a", "sk-company");
    let employee_id = scheduled_employee(&db, &profile_id, json!({ "everyMinutes": 60 }));
    make_due(&db, &employee_id);
    let run = begin_run(&db, &employee_id, Some("schedule"), None).unwrap();

    // A crash leaves the run live and the employee with no pending slot.
    db.conn()
        .execute(
            "UPDATE employees SET next_run_at = NULL WHERE id = ?1",
            params![employee_id],
        )
        .unwrap();
    assert_eq!(recover(&db).unwrap(), 1);
    let after = get_run(&db, &run.id).unwrap().unwrap();
    assert_eq!(after.status, "aborted");
    assert_eq!(after.error_code.as_deref(), Some("HOST_RESTART"));
    assert!(after.ended_at.is_some());
    assert!(next_run_at(&db, &employee_id).is_some());
    // Recovery is idempotent.
    assert_eq!(recover(&db).unwrap(), 0);
}

#[test]
fn a_run_binding_comes_from_the_run_row_and_carries_its_endpoint() {
    let (_dir, db, secrets) = setup();
    provider(&db, "vendor-a");
    let company = create_profile(
        &db,
        &secrets,
        &json!({
            "label": "Company key",
            "providerId": "vendor-a",
            "baseUrl": "https://gateway.example/v1",
            "secretValue": "sk-company"
        }),
    )
    .unwrap();
    let personal = profile(&db, &secrets, "Personal key", "vendor-a", "sk-personal");
    let scout = employee(&db, "Scout", &company.id, "gpt-5.6-sol");
    let session = crate::sessions::create_session_with_options(
        &db,
        crate::sessions::SessionCreateOptions {
            title: Some("Scout run".into()),
            mode: Some("agent".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let run = begin_run(&db, &scout, Some("schedule"), Some(&session.id)).unwrap();

    // Repointing the employee after the run started must not move the bill.
    update_employee(&db, &scout, &json!({ "credentialProfileId": personal })).unwrap();

    let binding = resolve_binding_for_run(&db, &secrets, &run.id).unwrap();
    assert_eq!(binding.api_key.as_deref(), Some("sk-company"));
    assert_eq!(
        binding.base_url.as_deref(),
        Some("https://gateway.example/v1")
    );
    assert_eq!(binding.model_id.as_deref(), Some("gpt-5.6-sol"));
    assert_eq!(
        binding.credential_profile_id.as_deref(),
        Some(company.id.as_str())
    );

    // Falling back to the provider row binds the provider's own key, and there
    // is no endpoint override to invent for it.
    let lone_ref = crate::secrets::secret_ref_for_provider("vendor-a");
    secrets.set(&lone_ref, "sk-provider").unwrap();
    db.conn()
        .execute(
            "UPDATE providers SET secret_ref = ?2 WHERE id = ?1",
            params!["vendor-a", lone_ref],
        )
        .unwrap();
    let loner = employee(&db, "Loner", &personal, "gpt-5.6-luna");
    db.conn()
        .execute(
            "UPDATE employees SET credential_profile_id = NULL WHERE id = ?1",
            params![loner],
        )
        .unwrap();
    let direct = resolve_run_binding(&db, &secrets, &loner).unwrap();
    assert_eq!(direct.api_key.as_deref(), Some("sk-provider"));
    assert!(direct.base_url.is_none());
}

#[test]
fn the_binding_rpc_answers_a_run_and_refuses_an_unknown_one() {
    let (_dir, db, secrets) = setup();
    provider(&db, "vendor-a");
    let profile_id = profile(&db, &secrets, "Company key", "vendor-a", "sk-company");
    let employee_id = employee(&db, "Scout", &profile_id, "gpt-5.6-sol");
    let run = begin_run(&db, &employee_id, Some("schedule"), None).unwrap();

    let reply = handle(
        &ctx(&db, &secrets),
        "staff.run.binding",
        &json!({ "runId": run.id }),
    )
    .unwrap();
    assert_eq!(reply["apiKey"], json!("sk-company"));
    assert_eq!(reply["modelId"], json!("gpt-5.6-sol"));
    assert_eq!(reply["employeeId"], json!(employee_id));

    let missing = handle(
        &ctx(&db, &secrets),
        "staff.run.binding",
        &json!({ "runId": "nope" }),
    )
    .unwrap_err();
    assert!(missing.to_string().starts_with("NOT_FOUND"));
}

#[test]
fn a_run_without_a_key_refuses_to_bind() {
    let (_dir, db, secrets) = setup();
    provider(&db, "vendor-a");
    let profile_id = profile(&db, &secrets, "Company key", "vendor-a", "sk-company");
    let employee_id = employee(&db, "Scout", &profile_id, "gpt-5.6-sol");
    db.conn()
        .execute(
            "UPDATE employees SET credential_profile_id = NULL WHERE id = ?1",
            params![employee_id],
        )
        .unwrap();
    let run = begin_run(&db, &employee_id, Some("schedule"), None).unwrap();
    // A binding is deliberately not Debug, so the failure is matched, not
    // unwrapped: the compiler is what keeps a key out of a panic payload.
    let refused = match resolve_binding_for_run(&db, &secrets, &run.id) {
        Ok(_) => panic!("a run with no key must not bind"),
        Err(error) => error,
    };
    assert!(refused.to_string().starts_with("PROVIDER_SECRET_MISSING"));
}

#[test]
fn a_claimed_run_opens_a_session_that_cannot_block_on_a_permission_card() {
    let (_dir, db, secrets) = setup();
    provider(&db, "vendor-a");
    let profile_id = profile(&db, &secrets, "Company key", "vendor-a", "sk-company");
    let employee_id = scheduled_employee(&db, &profile_id, json!({ "everyMinutes": 60 }));
    make_due(&db, &employee_id);

    let batch = claim_due(&ctx(&db, &secrets), 4).unwrap();
    assert_eq!(batch.claims.len(), 1);
    let session_id = batch.claims[0].session_id.clone();
    let permission_mode: String = db
        .conn()
        .query_row(
            "SELECT permission_mode FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(permission_mode, "auto");
    assert_eq!(batch.claims[0].steps.len(), 2);
    // A step carries its node parameters, not just its instruction: the driver
    // renders them into the prompt, so the channel reaches the model.
    assert_eq!(batch.claims[0].steps[0].config["channel"], json!("forum"));
}

#[test]
fn a_probe_may_name_the_closed_source_set() {
    let (_dir, db, _secrets) = setup();
    let seen = parse_graph(&json!({
        "nodes": [
            { "id": "t1", "kind": "trigger", "config": { "mode": "manual" } },
            {
                "id": "p1",
                "kind": "probe",
                "config": { "sources": ["wechatMp", "wechatChannels", "juejin"] }
            }
        ],
        "edges": [{ "from": "t1", "to": "p1" }]
    }))
    .unwrap();
    assert_eq!(seen.compile().unwrap().len(), 1);
    let _ = &db;
}

#[test]
fn a_probe_rejects_a_source_nobody_collects() {
    let (_dir, _db, _secrets) = setup();
    let error = parse_graph(&json!({
        "nodes": [
            { "id": "t1", "kind": "trigger", "config": { "mode": "manual" } },
            { "id": "p1", "kind": "probe", "config": { "sources": ["weibo"] } }
        ],
        "edges": [{ "from": "t1", "to": "p1" }]
    }))
    .unwrap_err()
    .to_string();
    assert!(error.contains("unknown source channel weibo"), "{error}");
}

#[test]
fn a_probe_needs_some_named_source() {
    let (_dir, _db, _secrets) = setup();
    let error = parse_graph(&json!({
        "nodes": [
            { "id": "t1", "kind": "trigger", "config": { "mode": "manual" } },
            { "id": "p1", "kind": "probe", "config": { "sources": [] } }
        ],
        "edges": [{ "from": "t1", "to": "p1" }]
    }))
    .unwrap_err()
    .to_string();
    assert!(error.contains("names no probe source"), "{error}");
}

#[test]
fn a_legacy_single_channel_still_opens() {
    let (_dir, _db, _secrets) = setup();
    let seen = parse_graph(&json!({
        "nodes": [
            { "id": "t1", "kind": "trigger", "config": { "mode": "manual" } },
            { "id": "p1", "kind": "probe", "config": { "channel": "forum" } }
        ],
        "edges": [{ "from": "t1", "to": "p1" }]
    }))
    .unwrap();
    assert_eq!(seen.compile().unwrap().len(), 1);
}

#[test]
fn a_memory_read_keeps_the_word_the_graph_wrote() {
    let (_dir, _db, _secrets) = setup();
    let seen = parse_graph(&json!({
        "nodes": [
            { "id": "t1", "kind": "trigger", "config": { "mode": "schedule" } },
            {
                "id": "m1",
                "kind": "memory",
                "instruction": "Read what the team already knows.",
                "config": { "operation": "read", "scope": "shared", "kind": "signal" }
            }
        ],
        "edges": [{ "from": "t1", "to": "m1" }]
    }))
    .unwrap();
    let steps = seen.compile().unwrap();
    let memory = steps[0].memory.as_ref().unwrap();
    assert_eq!(memory.operation, "read");
    assert_eq!(memory.scope.as_deref(), Some("shared"));
}

#[test]
fn the_older_spelling_of_a_memory_read_compiles_to_a_read() {
    let (_dir, _db, _secrets) = setup();
    // The driver used to know only "query" while the console wrote "read", so
    // the seeded interest analyst paid for a model turn on every run and then
    // recorded that turn's prose as its signal (run 6e6e5672).
    let seen = parse_graph(&json!({
        "nodes": [
            { "id": "t1", "kind": "trigger", "config": { "mode": "schedule" } },
            {
                "id": "m1",
                "kind": "memory",
                "config": { "operation": "query", "scope": "shared" }
            }
        ],
        "edges": [{ "from": "t1", "to": "m1" }]
    }))
    .unwrap();
    let steps = seen.compile().unwrap();
    assert_eq!(steps[0].memory.as_ref().unwrap().operation, "read");
}

#[test]
fn a_memory_node_rejects_an_operation_nobody_implements() {
    let (_dir, _db, _secrets) = setup();
    let error = parse_graph(&json!({
        "nodes": [
            { "id": "t1", "kind": "trigger", "config": { "mode": "schedule" } },
            { "id": "m1", "kind": "memory", "config": { "operation": "reed", "scope": "shared" } }
        ],
        "edges": [{ "from": "t1", "to": "m1" }]
    }))
    .unwrap_err()
    .to_string();
    assert!(error.contains("unknown memory operation reed"), "{error}");
}

#[test]
fn a_memory_node_rejects_a_scope_the_store_does_not_have() {
    let (_dir, _db, _secrets) = setup();
    let error = parse_graph(&json!({
        "nodes": [
            { "id": "t1", "kind": "trigger", "config": { "mode": "schedule" } },
            {
                "id": "m1",
                "kind": "memory",
                "config": { "operation": "publish", "scope": "global", "kind": "signal" }
            }
        ],
        "edges": [{ "from": "t1", "to": "m1" }]
    }))
    .unwrap_err()
    .to_string();
    assert!(error.contains("unknown memory scope global"), "{error}");
}

/// Where the console's auto-layout puts the first node of a fresh graph.
const LAYOUT_ORIGIN: (i64, i64) = (40, 32);
/// Columns one band holds before the layout wraps onto the next one.
const LAYOUT_BAND_COLUMNS: usize = 3;

/// The blueprints ship their own coordinates, and the console has a button that
/// lays a graph out. Both have to agree: a blueprint left on the old single row
/// would open one way for an owner who adopted it and another way for one who
/// pressed the button.
#[test]
fn every_shipped_blueprint_is_laid_out_in_bands() {
    for preset in presets() {
        let nodes = preset.graph["nodes"].as_array().expect("nodes").clone();
        let mut rows: std::collections::BTreeMap<i64, usize> = std::collections::BTreeMap::new();
        for node in &nodes {
            let y = node["position"]["y"]
                .as_i64()
                .unwrap_or_else(|| panic!("{} ships a node with no row", preset.id));
            *rows.entry(y).or_default() += 1;
        }
        assert!(rows.len() > 1, "{} still ships a single row", preset.id);
        for (row, count) in &rows {
            assert!(
                *count <= LAYOUT_BAND_COLUMNS,
                "{} puts {count} nodes on row {row}",
                preset.id
            );
        }
        let first = &nodes[0]["position"];
        assert_eq!(
            (first["x"].as_i64(), first["y"].as_i64()),
            (Some(LAYOUT_ORIGIN.0), Some(LAYOUT_ORIGIN.1)),
            "{} does not start where the layout button would put it",
            preset.id
        );
    }
}

#[test]
fn installing_the_presets_staffs_the_company_once() {
    let (_dir, db, secrets) = setup();
    let params = json!({
        "apiKey": "sk-preset-test",
        "baseUrl": "https://api.deepseek.com",
        "modelId": "deepseek-flash",
        "providerName": "预设模型",
    });
    let first = install_presets(&db, &secrets, &params).unwrap();
    assert_eq!(
        first["installed"],
        json!(["侦察员", "选题员", "线索主管", "内容主编", "笔杆子", "地推员", "接线员", "跟单员", "售后员"])
    );
    let profile_id = first["credentialProfileId"].as_str().unwrap().to_string();
    assert!(!first.to_string().contains("sk-preset-test"));

    // Every seeded employee runs the workflow it was hired for, on its own clock.
    let employees = list_employees(&db).unwrap();
    assert_eq!(employees.len(), 9);
    let scout = employees.iter().find(|e| e.name == "侦察员").unwrap();
    assert_eq!(scout.status, "running");
    assert_eq!(scout.credential_profile_id.as_deref(), Some(profile_id.as_str()));
    assert!(scout.next_run_at.is_some(), "a running employee is scheduled");
    let analyst = employees.iter().find(|e| e.name == "选题员").unwrap();
    assert_eq!(analyst.status, "running");
    assert!(analyst.next_run_at.is_some());
    let workflows = list_workflows(&db).unwrap();
    assert_eq!(workflows.len(), 9);
    assert!(workflows.iter().all(|w| w.latest_version == 1));
    // Every blueprint compiles: a seeded graph that cannot be compiled would
    // be an employee the console shows and the driver silently skips.
    for employee in &employees {
        let steps = get_graph(
            &db,
            employee.workflow_id.as_deref().unwrap(),
            employee.workflow_version.unwrap(),
        )
        .unwrap()
        .unwrap()
        .0
        .compile()
        .unwrap();
        assert!(steps.len() >= 4, "{} has too few steps", employee.name);
    }

    // The scout sweeps the sites that have a collector; naming a site nobody
    // can read would send the employee off to fetch it by hand.
    let scout_graph = get_graph(&db, scout.workflow_id.as_deref().unwrap(), 1)
        .unwrap()
        .unwrap()
        .0;
    let steps = scout_graph.compile().unwrap();
    let scan = steps
        .iter()
        .find(|step| step.kind == "probe")
        .unwrap();
    let sources = scan.config["sources"].as_array().unwrap();
    assert_eq!(sources, &vec![json!("zhihu")]);
    // The sweep is scoped in the graph, not left to the model to widen.
    assert!(scan.instruction.contains("[collected:"));
    assert!(scan.instruction.contains("skipped"));

    // A second call is a no-op, not a second company.
    let again = install_presets(&db, &secrets, &params).unwrap();
    assert_eq!(again["installed"], json!([]));
    assert_eq!(
        again["skipped"],
        json!(["侦察员", "选题员", "线索主管", "内容主编", "笔杆子", "地推员", "接线员", "跟单员", "售后员"])
    );
    assert_eq!(list_employees(&db).unwrap().len(), 9);
    assert_eq!(list_workflows(&db).unwrap().len(), 9);
}

#[test]
fn a_blueprint_that_was_renamed_does_not_hire_a_second_employee() {
    let (_dir, db, secrets) = setup();
    // The roster a console holds when it installed the blueprints under their
    // old names: the rename must not read as a brand new hire.
    create_employee(
        &db,
        &json!({ "name": "侦察探针", "role": "外勤", "status": "paused" }),
    )
    .unwrap();
    let result = install_presets(&db, &secrets, &json!({ "apiKey": "sk-preset-test" })).unwrap();
    assert_eq!(
        result["installed"],
        json!(["选题员", "线索主管", "内容主编", "笔杆子", "地推员", "接线员", "跟单员", "售后员"])
    );
    assert_eq!(result["skipped"], json!(["侦察探针"]));
    assert_eq!(list_employees(&db).unwrap().len(), 9);

    // And the blueprint reports itself as hired under that older name.
    let listed = list_presets(&db).unwrap();
    let scout = listed["presets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|preset| preset["id"] == "scout")
        .unwrap();
    assert_eq!(scout["installed"], json!(true));
}

#[test]
fn the_seeded_scout_files_what_it_finds_where_the_seeded_analyst_reads_it() {
    let (_dir, db, secrets) = setup();
    install_presets(&db, &secrets, &json!({ "apiKey": "sk-preset-test" })).unwrap();
    let employees = list_employees(&db).unwrap();
    let scout = employees.iter().find(|e| e.name == "侦察员").unwrap();
    let analyst = employees.iter().find(|e| e.name == "选题员").unwrap();
    let steps = |employee: &Employee| {
        get_graph(
            &db,
            employee.workflow_id.as_deref().unwrap(),
            employee.workflow_version.unwrap(),
        )
        .unwrap()
        .unwrap()
        .0
        .compile()
        .unwrap()
    };

    // The scout's judgement leaves its own run, so it can reach the employee
    // whose whole job is to read it.
    let scout_steps = steps(scout);
    let filed = scout_steps
        .iter()
        .find(|step| step.kind == "memory")
        .expect("the scout files what it judged");
    let write = filed.memory.as_ref().unwrap();
    assert_eq!(write.operation, "publish");
    assert_eq!(write.scope.as_deref(), Some("shared"));

    // After the judgement that produced it, before the replies drafted from it.
    let position = |node: &str| {
        scout_steps
            .iter()
            .position(|step| step.node_id == node)
            .unwrap()
    };
    assert!(position("triage") < position("file"));
    assert!(position("file") < position("draft"));

    // The analyst's recall reads that same store for that same kind of item,
    // before the step that mines it: one headline, two employees.
    let analyst_steps = steps(analyst);
    let recall = analyst_steps
        .iter()
        .find(|step| step.kind == "memory" && step.memory.as_ref().unwrap().operation == "read")
        .expect("the analyst reads what the team knows");
    let read = recall.memory.as_ref().unwrap();
    assert_eq!(read.scope.as_deref(), Some("shared"));
    assert_eq!(read.kind, write.kind);
    assert!(
        analyst_steps.iter().position(|step| step.node_id == "recall")
            < analyst_steps.iter().position(|step| step.node_id == "mine")
    );
}

#[test]
fn a_preset_without_a_credential_installs_paused() {
    let (_dir, db, secrets) = setup();
    let result = install_presets(&db, &secrets, &json!({ "ids": ["scout"] })).unwrap();
    assert_eq!(result["installed"], json!(["侦察员"]));
    assert!(result["credentialProfileId"].is_null());
    let employees = list_employees(&db).unwrap();
    assert_eq!(employees.len(), 1);
    assert_eq!(employees[0].status, "paused");
    assert!(employees[0].next_run_at.is_none());
}

#[test]
fn an_unknown_preset_id_is_rejected() {
    let (_dir, db, secrets) = setup();
    let error = install_presets(&db, &secrets, &json!({ "ids": ["ceo"] }))
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown preset ceo"), "{error}");
}

#[test]
fn migration_from_v19_adds_the_boards_and_keeps_every_row() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    let secrets = SecretStore::open(dir.path()).unwrap();
    let (employee_id, run_id) = {
        let db = Database::open(&path).unwrap();
        provider(&db, "vendor-a");
        let profile_id = profile(&db, &secrets, "Company key", "vendor-a", "sk-company");
        let employee_id = employee(&db, "Scout", &profile_id, "gpt-5.6-sol");
        let run = begin_run(&db, &employee_id, Some("manual"), None).unwrap();
        // Rewind to v19: the boards did not exist at that version.
        db.conn()
            .execute_batch(
                "DROP TABLE staff_directives;
                 DROP TABLE staff_suggestions;
                 DROP TABLE staff_metrics;
                 DROP TABLE staff_contents;
                 DROP TABLE staff_leads;",
            )
            .unwrap();
        db.conn().pragma_update(None, "user_version", 19).unwrap();
        (employee_id, run.id)
    };

    let db = Database::open(&path).unwrap();
    let version: i64 = db
        .conn()
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    assert!(migration_backup_path(&path, 19).exists());

    // The run that predates the boards survives the rewrite.
    let run = get_run(&db, &run_id).unwrap().unwrap();
    assert_eq!(run.employee_id, employee_id);

    // And the boards are usable on the upgraded file.
    let lead = upsert_lead(
        &db,
        &json!({ "title": "私有部署能不能落地", "site": "v2ex", "score": 90 }),
    )
    .unwrap();
    assert_eq!(lead.stage, "new");
    assert_eq!(list_leads(&db, &json!({})).unwrap().len(), 1);
}

#[test]
fn a_card_moves_through_the_columns_and_records_when_it_did() {
    let (_dir, db, _secrets) = setup();
    let lead = upsert_lead(
        &db,
        &json!({
            "title": "老板让 AI 拍板",
            "site": "zhihu",
            "url": "https://example.test/q/1",
            "quote": "我研究过，这根本不可行",
            "score": 140,
        }),
    )
    .unwrap();
    // A score is clamped to the range the board draws.
    assert_eq!(lead.score, 100);
    assert!(lead.contacted_at.is_none());

    let moved = move_lead(&db, &json!({ "id": lead.id, "stage": "contacted" }))
        .unwrap()
        .unwrap();
    assert_eq!(moved.stage, "contacted");
    assert!(moved.contacted_at.is_some(), "reaching out stamps the moment");

    // A run files cards but never moves them: the stage is the owner's answer,
    // and a run that could set it would be claiming a decision nobody made.
    let revised = upsert_lead(
        &db,
        &json!({
            "id": moved.id,
            "title": moved.title,
            "stage": "won",
            "runId": "00000000-0000-4000-8000-000000000000",
        }),
    )
    .unwrap();
    assert_eq!(revised.stage, "contacted");

    let counts = lead_stage_counts(&db).unwrap();
    assert_eq!(counts["contacted"], json!(1));
    assert_eq!(counts["new"], json!(0));

    let error = move_lead(&db, &json!({ "id": lead.id, "stage": "somewhere" }))
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown lead stage somewhere"), "{error}");
}

#[test]
fn a_draft_is_written_by_a_run_and_approved_by_the_owner() {
    let (_dir, db, _secrets) = setup();
    let drafted = upsert_content(
        &db,
        &json!({
            "title": "数据不出本地怎么讲",
            "channel": "v2ex",
            "variant": "A",
            "body": "先给做法，再问一句。",
            "status": "published",
            "runId": "00000000-0000-4000-8000-000000000000",
        }),
    )
    .unwrap();
    // A run may draft, not approve: its write-back keeps the status it finds.
    assert_eq!(drafted.status, "draft");
    assert!(drafted.approved_at.is_none());

    let approved = set_content_status(&db, &json!({ "id": drafted.id, "status": "approved" }))
        .unwrap()
        .unwrap();
    assert_eq!(approved.status, "approved");
    assert!(approved.approved_at.is_some());
    assert!(approved.published_at.is_none());

    let published = set_content_status(
        &db,
        &json!({ "id": drafted.id, "status": "published", "publishedUrl": "https://example.test/1" }),
    )
    .unwrap()
    .unwrap();
    assert_eq!(published.published_url, "https://example.test/1");
    // Approving stamped the moment it happened; publishing does not move it.
    assert_eq!(published.approved_at, approved.approved_at);
    assert!(published.published_at.is_some());
}

#[test]
fn handing_a_piece_to_the_poster_is_not_the_same_as_saying_it() {
    let (_dir, db, _secrets) = setup();
    let drafted = upsert_content(&db, &json!({ "title": "交给地推员", "body": "x" })).unwrap();
    let approved = set_content_status(&db, &json!({ "id": drafted.id, "status": "approved" }))
        .unwrap()
        .unwrap();

    // The owner's second click: the piece is out of their hands, and it is
    // still not out in the world.
    let queued = set_content_status(&db, &json!({ "id": drafted.id, "status": "queued" }))
        .unwrap()
        .unwrap();
    assert_eq!(queued.status, "queued");
    assert_eq!(queued.approved_at, approved.approved_at);
    assert!(queued.published_at.is_none());
    assert_eq!(queued.published_url, "");

    // The poster's own report of what it did: this is the only write that
    // counts as said, and it carries the address the watch reads back from.
    let published = set_content_status(
        &db,
        &json!({ "id": drafted.id, "status": "published", "publishedUrl": "https://example.test/2" }),
    )
    .unwrap()
    .unwrap();
    assert_eq!(published.status, "published");
    assert!(published.published_at.is_some());
    assert_eq!(published.approved_at, approved.approved_at);
}

#[test]
fn the_daily_board_reads_its_numbers_off_the_rows_it_summarizes() {
    let (_dir, db, _secrets) = setup();
    let today = local_day(now_ms());
    let lead = upsert_lead(&db, &json!({ "title": "卡片", "score": 80 })).unwrap();
    move_lead(&db, &json!({ "id": lead.id, "stage": "contacted" })).unwrap();
    upsert_mettoday_helper(&db, &today);

    let summary = report_summary(&db, &json!({ "days": 7 })).unwrap();
    assert_eq!(summary["day"], json!(today));
    assert_eq!(summary["today"]["leads"], json!(1));
    assert_eq!(summary["today"]["contacted"], json!(1));
    assert_eq!(summary["today"]["views"], json!(120));
    assert_eq!(summary["topics"][0]["label"], json!("数据不出本地"));
    // A second reading of the same counter overwrites rather than adds.
    upsert_metric(
        &db,
        &json!({ "day": today, "label": "数据不出本地", "views": 130, "replies": 9, "leads": 1, "inquiries": 2, "deals": 0 }),
    )
    .unwrap();
    let summary = report_summary(&db, &json!({ "days": 7 })).unwrap();
    assert_eq!(summary["today"]["views"], json!(130));
    assert_eq!(summary["trend"].as_array().unwrap().len(), 1);
}

fn upsert_mettoday_helper(db: &Database, day: &str) {
    upsert_metric(
        db,
        &json!({ "day": day, "label": "数据不出本地", "views": 120, "replies": 8, "leads": 1, "inquiries": 2, "deals": 0 }),
    )
    .unwrap();
}

#[test]
fn a_board_raises_a_proposal_from_the_rows_and_the_owner_turns_it_into_work() {
    let (_dir, db, secrets) = setup();
    provider(&db, "vendor-a");
    let profile_id = profile(&db, &secrets, "Company key", "vendor-a", "sk-company");
    let employee_id = employee(&db, "Scout", &profile_id, "gpt-5.6-sol");
    let run = begin_run(&db, &employee_id, Some("manual"), None).unwrap();

    // A card a run filed and nobody touched, older than the cold threshold.
    let lead = upsert_lead(
        &db,
        &json!({
            "title": "能不能不把代码传上去",
            "site": "v2ex",
            "quote": "我不想把公司的代码传到别人服务器",
            "score": 85,
            "employeeId": employee_id,
            "runId": run.id,
        }),
    )
    .unwrap();
    db.conn()
        .execute(
            "UPDATE staff_leads SET created_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![lead.id, now_ms() - 48 * 3_600_000],
        )
        .unwrap();

    let raised = derive_suggestions(&db, &json!({})).unwrap();
    let cold = raised
        .iter()
        .find(|item| item.board == "leads")
        .expect("a cold card is the first thing a board raises");
    assert!(cold.title.contains("1 张"), "{}", cold.title);
    // The citation points back at the run that filed the card.
    assert_eq!(cold.evidence[0].run_id.as_deref(), Some(run.id.as_str()));
    assert_eq!(cold.evidence[0].employee_name.as_deref(), Some("Scout"));
    assert!(cold.evidence[0]
        .quote
        .as_deref()
        .unwrap()
        .contains("不想把公司的代码传到别人服务器"));

    // Asking again while it is still open does not stack a duplicate.
    let again = derive_suggestions(&db, &json!({})).unwrap();
    assert!(again.is_empty(), "{again:?}");

    // Accepting it is what creates work, and the work names who gets it.
    let decided = decide_suggestion(
        &db,
        &json!({
            "id": cold.id,
            "decision": "accepted",
            "directive": { "employeeId": employee_id, "verb": "挑一遍待触达的卡" },
        }),
    )
    .unwrap()
    .unwrap();
    assert_eq!(decided.status, "accepted");
    let directive = decided.directive.expect("accepting a proposal creates work");
    assert_eq!(directive.status, "queued");
    assert_eq!(directive.employee_id.as_deref(), Some(employee_id.as_str()));
    assert_eq!(directive.verb, "挑一遍待触达的卡");

    // Deciding twice is refused rather than silently overwriting the answer.
    let error = decide_suggestion(&db, &json!({ "id": cold.id, "decision": "rejected" }))
        .unwrap_err()
        .to_string();
    assert!(error.contains("already decided"), "{error}");

    // Reporting back closes the proposal with the same sentence.
    let done = update_directive(
        &db,
        &json!({ "id": directive.id, "status": "done", "resultNote": "标了 1 张待触达" }),
    )
    .unwrap()
    .unwrap();
    assert_eq!(done.status, "done");
    assert!(done.ended_at.is_some());
    let closed = get_suggestion(&db, &cold.id).unwrap().unwrap();
    assert_eq!(closed.status, "done");
    assert_eq!(closed.result_note, "标了 1 张待触达");

    // With the condition still true and the proposal answered, the board is
    // free to raise it again: the same problem returning is news.
    let raised = derive_suggestions(&db, &json!({})).unwrap();
    assert!(raised.iter().any(|item| item.board == "leads"));
}

#[test]
fn a_rejected_proposal_keeps_its_citations() {
    let (_dir, db, _secrets) = setup();
    let lead = upsert_lead(&db, &json!({ "title": "卡片", "score": 10 })).unwrap();
    db.conn()
        .execute(
            "UPDATE staff_leads SET created_at = ?2 WHERE id = ?1",
            params![lead.id, now_ms() - 72 * 3_600_000],
        )
        .unwrap();
    let raised = derive_suggestions(&db, &json!({})).unwrap();
    let cold = raised.iter().find(|item| item.board == "leads").unwrap();
    let decided = decide_suggestion(
        &db,
        &json!({ "id": cold.id, "decision": "rejected", "note": "这批不算" }),
    )
    .unwrap()
    .unwrap();
    assert_eq!(decided.status, "rejected");
    assert!(decided.directive.is_none(), "a rejection orders nothing");
    assert!(!decided.evidence.is_empty(), "the citations survive the no");
    assert_eq!(decided.result_note, "这批不算");
}
#[test]
fn an_employee_reports_when_it_runs_an_older_copy_of_its_blueprint() {
    let (_dir, db, secrets) = setup();
    install_presets(&db, &secrets, &json!({ "apiKey": "sk-preset-test" })).unwrap();

    // Freshly hired, every employee runs the copy it was hired with, so the
    // console has nothing to offer.
    let hired = list_presets(&db).unwrap();
    assert!(hired["presets"]
        .as_array()
        .unwrap()
        .iter()
        .all(|preset| preset["blueprintChanged"] == json!(false)));

    // An owner widens the sweep window. That is a real edit, and it must read
    // as one: the console offers a stale blueprint to adopt, not a fresh one.
    let scout = list_employees(&db)
        .unwrap()
        .into_iter()
        .find(|employee| employee.name == "侦察员")
        .unwrap();
    let workflow_id = scout.workflow_id.clone().unwrap();
    let mut edited = blueprint_graph("scout");
    edited["nodes"][1]["config"]["window"] = json!("7d");
    update_workflow(&db, &workflow_id, &json!({ "graph": edited })).unwrap();
    // Saving a graph and running it are two steps in the console too, so the
    // employee is pointed at the revision before anything reads it as current.
    update_employee(&db, &scout.id, &json!({ "workflowVersion": 2 })).unwrap();

    let listed = list_presets(&db).unwrap();
    let rows = listed["presets"].as_array().unwrap();
    let scout_row = rows.iter().find(|row| row["id"] == json!("scout")).unwrap();
    assert_eq!(scout_row["blueprintChanged"], json!(true));
    assert_eq!(scout_row["name"], json!("侦察员"));
    // One employee's edit says nothing about the other six.
    assert_eq!(
        rows.iter()
            .filter(|row| row["blueprintChanged"] == json!(true))
            .count(),
        1
    );
}

#[test]
fn adopting_a_corrected_blueprint_appends_a_version_and_keeps_the_old_one() {
    let (_dir, db, secrets) = setup();
    install_presets(&db, &secrets, &json!({ "apiKey": "sk-preset-test" })).unwrap();
    let scout = list_employees(&db)
        .unwrap()
        .into_iter()
        .find(|employee| employee.name == "侦察员")
        .unwrap();
    let workflow_id = scout.workflow_id.clone().unwrap();

    let mut edited = blueprint_graph("scout");
    edited["nodes"][1]["config"]["window"] = json!("7d");
    update_workflow(&db, &workflow_id, &json!({ "graph": edited })).unwrap();
    // Saving a graph and running it are two steps in the console too, so the
    // employee is pointed at the revision before anything reads it as current.
    update_employee(&db, &scout.id, &json!({ "workflowVersion": 2 })).unwrap();

    // Hiring is idempotent by design, so installing again reports the employee
    // as held and rewrites nothing: the edit is the owner's, not a stale copy.
    let plain = install_presets(&db, &secrets, &json!({ "ids": ["scout"] })).unwrap();
    assert_eq!(plain["updated"], json!([]));
    assert_eq!(plain["skipped"], json!(["侦察员"]));
    assert_eq!(get_workflow(&db, &workflow_id).unwrap().unwrap().latest_version, 2);
    let held = list_employees(&db)
        .unwrap()
        .into_iter()
        .find(|employee| employee.id == scout.id)
        .unwrap();
    assert_eq!(held.workflow_version, Some(2), "the owner still runs their edit");

    // Asking for it is what moves them onto this build's blueprint.
    let adopted = install_presets(
        &db,
        &secrets,
        &json!({ "ids": ["scout"], "adoptBlueprint": true }),
    )
    .unwrap();
    assert_eq!(adopted["updated"], json!(["侦察员"]));
    assert_eq!(adopted["skipped"], json!([]));
    assert_eq!(adopted["installed"], json!([]));

    let workflow = get_workflow(&db, &workflow_id).unwrap().unwrap();
    assert_eq!(workflow.latest_version, 3);
    let moved = list_employees(&db)
        .unwrap()
        .into_iter()
        .find(|employee| employee.id == scout.id)
        .unwrap();
    assert_eq!(moved.workflow_version, Some(3));
    // The blueprint is now what the employee runs, byte for byte.
    let (_, raw) = get_graph(&db, &workflow_id, 3).unwrap().unwrap();
    assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), blueprint_graph("scout"));
    // And the edit is still there to look at, which is the point of appending
    // rather than editing.
    let (_, previous) = get_graph(&db, &workflow_id, 2).unwrap().unwrap();
    assert!(previous.contains("7d"), "{previous}");
    // The version says who wrote it, and the author is the build.
    let versions = list_versions(&db, &workflow_id).unwrap();
    let latest = versions.iter().find(|version| version.version == 3).unwrap();
    assert_eq!(latest.created_by, "preset");
}
#[test]
fn a_draft_a_run_filed_is_what_the_content_board_asks_about() {
    let (_dir, db, _secrets) = setup();
    // A run files its draft at `draft`. Nothing has moved it on, so the board
    // is waiting on the owner -- same as if the run had said `review`.
    upsert_content(
        &db,
        &json!({
            "title": "数据不出本地怎么讲",
            "channel": "v2ex",
            "body": "先给做法，再问一句。",
            "status": "draft",
            "runId": "00000000-0000-4000-8000-000000000000",
        }),
    )
    .unwrap();

    let raised = derive_suggestions(&db, &json!({})).unwrap();
    let waiting = raised
        .iter()
        .find(|item| item.board == "contents")
        .expect("a draft nobody approved is what the board is waiting on");
    assert!(waiting.title.contains("1 篇稿子"), "{}", waiting.title);
    assert_eq!(waiting.evidence.len(), 1);
    assert!(waiting.evidence[0].label.contains("v2ex"), "{}", waiting.evidence[0].label);

    // Once it is approved the board stops asking, and the paper trail starts.
    let stored = list_contents(&db, &json!({})).unwrap();
    set_content_status(
        &db,
        &json!({ "id": stored[0].id, "status": "approved", "decidedBy": "owner" }),
    )
    .unwrap();
    let after = derive_suggestions(&db, &json!({})).unwrap();
    assert!(after.iter().all(|item| item.board != "contents"));
}
#[test]
fn the_topic_proposal_counts_deals_as_deals() {
    let (_dir, db, _secrets) = setup();
    let day = local_day(now_ms());
    // Two topics, one clearly ahead, and leads deliberately different from
    // deals: the line that reads them out has to name each one correctly.
    upsert_metric(
        &db,
        &json!({ "day": day, "dimension": "topic", "label": "领先话题", "views": 900, "replies": 20, "leads": 7, "inquiries": 5, "deals": 3 }),
    )
    .unwrap();
    upsert_metric(
        &db,
        &json!({ "day": day, "dimension": "topic", "label": "陪跑话题", "views": 100, "replies": 2, "leads": 1, "inquiries": 1, "deals": 0 }),
    )
    .unwrap();

    let raised = derive_suggestions(&db, &json!({})).unwrap();
    let proposal = raised
        .iter()
        .find(|item| item.board == "report")
        .expect("the topic outrunning the others is what the report is waiting on");
    assert!(proposal.title.contains("领先话题"), "{}", proposal.title);
    assert!(proposal.impact.ends_with("3 单"), "{}", proposal.impact);
    assert!(proposal.impact.contains("5 次咨询"), "{}", proposal.impact);
}
#[test]
fn an_accepted_proposal_names_a_worker_who_can_do_the_work() {
    let (_dir, db, secrets) = setup();
    install_presets(&db, &secrets, &json!({ "apiKey": "sk-preset-test" })).unwrap();

    // A card nobody picked up belongs to the receptionist: re-triaging it is a
    // run, and the employee that can run it is on the roster.
    let lead = upsert_lead(&db, &json!({ "title": "冷掉的卡片", "score": 80 })).unwrap();
    db.conn()
        .execute(
            "UPDATE staff_leads SET created_at = ?2 WHERE id = ?1",
            params![lead.id, now_ms() - 72 * 3_600_000],
        )
        .unwrap();
    let cold = derive_suggestions(&db, &json!({}))
        .unwrap()
        .into_iter()
        .find(|item| item.board == "leads")
        .unwrap();
    assert!(cold.directive.is_none(), "an open proposal has ordered nothing yet");
    let decided = decide_suggestion(
        &db,
        &json!({ "id": cold.id, "decision": "accepted", "note": "先跟这两张" }),
    )
    .unwrap()
    .unwrap();
    let order = decided.directive.expect("accepting orders the work");
    assert_eq!(order.employee_name, "接线员");
    assert!(
        order.employee_id.is_some(),
        "a named worker has an id to hand the run to"
    );
    assert!(order.verb.contains("按分诊标准"), "{}", order.verb);
    assert_ne!(
        order.verb, decided.action,
        "the card's line is written to the owner; the order is written to the worker"
    );

    // A draft waiting to be approved is the owner's own act -- only the owner
    // may move a card to approved -- so the order names nobody.
    upsert_content(
        &db,
        &json!({
            "title": "等定稿的稿子",
            "channel": "v2ex",
            "body": "先给做法，再问一句。",
            "status": "draft",
            "runId": "00000000-0000-4000-8000-000000000000",
        }),
    )
    .unwrap();
    let waiting = derive_suggestions(&db, &json!({}))
        .unwrap()
        .into_iter()
        .find(|item| item.board == "contents")
        .unwrap();
    let decided = decide_suggestion(&db, &json!({ "id": waiting.id, "decision": "accepted" }))
        .unwrap()
        .unwrap();
    let order = decided.directive.expect("accepting orders the work");
    assert_eq!(order.employee_name, "");
    assert!(order.employee_id.is_none());

    // The owner may still send it somewhere else, without restating the verb.
    let other = derive_suggestions(&db, &json!({}))
        .unwrap()
        .into_iter()
        .find(|item| item.board == "report");
    if let Some(proposal) = other {
        let moved = decide_suggestion(
            &db,
            &json!({
                "id": proposal.id,
                "decision": "accepted",
                "directive": { "employeeId": null, "employeeName": "我自己" },
            }),
        )
        .unwrap()
        .unwrap();
        let order = moved.directive.unwrap();
        assert_eq!(order.employee_name, "我自己");
        assert!(order.employee_id.is_none());
        assert!(!order.verb.is_empty(), "the board's verb survives the override");
    }
}

// ---------------------------------------------------------------------------
// The middle managers (ADR 0254)
// ---------------------------------------------------------------------------

/// Seat the whole company, key and all, the way a fresh console does.
fn staff_the_company(db: &Database, secrets: &SecretStore) {
    install_presets(
        db,
        secrets,
        &json!({
            "apiKey": "sk-manager-test",
            "baseUrl": "https://api.deepseek.com",
            "modelId": "deepseek-flash",
            "providerName": "主管模型",
        }),
    )
    .unwrap();
}

fn by_name(db: &Database, name: &str) -> Employee {
    list_employees(db)
        .unwrap()
        .into_iter()
        .find(|employee| employee.name == name)
        .unwrap_or_else(|| panic!("no employee {name}"))
}

/// The loop the two managers exist for: a judgement becomes an order, the
/// order wakes the worker, and what the worker reported comes back to the
/// console as the order's own result.
#[test]
fn a_manager_judgement_becomes_an_order_the_scheduler_actually_runs() {
    let (_dir, db, secrets) = setup();
    staff_the_company(&db, &secrets);

    let manager = by_name(&db, "线索主管");
    assert_eq!(manager.layer, "manager");
    assert_eq!(manager.board, "leads");
    let writer = by_name(&db, "笔杆子");
    assert_eq!(writer.layer, "worker");

    // What the manager may hand out is declared in its own blueprint, not in
    // the code that runs it.
    let scope = dispatch_scope_of(&db, &manager.id).unwrap();
    assert_eq!(scope["targets"], json!(["笔杆子", "地推员", "接线员", "跟单员"]));
    assert_eq!(scope["verbs"], json!(["produce", "outreach", "reply", "followup"]));
    assert_eq!(scope["maxPerDay"], json!(5));
    assert_eq!(scope["spentToday"], json!(0));

    let decision = record_decision(
        &db,
        &json!({
            "employeeId": manager.id,
            "board": "leads",
            "title": "先动 V2EX 那个问权限边界的",
            "verdict": "他缺的是权限边界，不是模型能力",
            "inputs": ["- [线索][new·88分] 私有部署怎么不重做权限"],
            "dispatched": [{
                "to": "笔杆子",
                "verb": "produce",
                "task": "写一条留言：讲清权限边界怎么不动现有系统",
            }],
        }),
    )
    .unwrap();
    assert_eq!(decision.dispatched.len(), 1);
    assert!(decision.note.is_empty(), "{}", decision.note);

    // Queued, not run inline: the manager asked, the scheduler decides when.
    let queued = list_directives(&db, &json!({ "issuerEmployeeId": manager.id })).unwrap();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].status, "queued");
    assert_eq!(queued[0].employee_id.as_deref(), Some(writer.id.as_str()));
    assert_eq!(queued[0].verb, "produce");
    assert_eq!(queued[0].board, "leads");
    assert!(queued[0].deadline_at.is_some(), "an order carries its window");
    assert_eq!(
        queued[0].reason_run_id.as_deref(),
        decision.run_id.as_deref()
    );

    // The next tick picks the order up and opens the worker's own run.
    let batch = claim_due(&ctx(&db, &secrets), 8).unwrap();
    let claim = batch
        .claims
        .iter()
        .find(|claim| claim.employee_id == writer.id)
        .expect("the order woke the writer");
    assert_eq!(claim.trigger, "directive");
    let brief = claim.directive.as_ref().expect("the order rides with the run");
    assert_eq!(brief["verb"], json!("produce"));
    assert_eq!(brief["issuer"], json!("线索主管"));
    assert!(brief["task"].as_str().unwrap().contains("权限边界"));

    let running = get_directive(&db, &queued[0].id).unwrap().unwrap();
    assert_eq!(running.status, "running");
    assert_eq!(running.claimed_run_id.as_deref(), Some(claim.run_id.as_str()));

    // Finishing the run closes the order in the worker's own words.
    set_run_step(
        &db,
        &claim.run_id,
        1,
        Some("写了三个开头，投在 V2EX 那个帖子里"),
        None,
    )
    .unwrap();
    super::schedule::finish_run(&db, &claim.run_id, "succeeded", None).unwrap();
    let closed = get_directive(&db, &queued[0].id).unwrap().unwrap();
    assert_eq!(closed.status, "done");
    assert!(closed.result_note.contains("V2EX"), "{}", closed.result_note);
}

#[test]
fn a_manager_may_only_dispatch_what_its_blueprint_declares() {
    let (_dir, db, secrets) = setup();
    staff_the_company(&db, &secrets);
    let manager = by_name(&db, "线索主管");
    let writer = by_name(&db, "笔杆子");

    // A worker the blueprint never named.
    let refused = assign_directive(
        &db,
        &json!({
            "issuerEmployeeId": manager.id,
            "employeeName": "选题员",
            "verb": "produce",
            "task": "顺手也写一篇",
        }),
    )
    .unwrap_err()
    .to_string();
    assert!(refused.contains("授权"), "{refused}");

    // A kind of work the blueprint never named.
    let refused = assign_directive(
        &db,
        &json!({
            "issuerEmployeeId": manager.id,
            "employeeName": "笔杆子",
            "verb": "service",
            "task": "交付之后顺手回访",
        }),
    )
    .unwrap_err()
    .to_string();
    assert!(refused.contains("service"), "{refused}");

    // A worker has no authority to hand anything out at all.
    let refused = assign_directive(
        &db,
        &json!({
            "issuerEmployeeId": writer.id,
            "employeeName": "跟单员",
            "verb": "followup",
            "task": "帮我跟一下这条",
        }),
    )
    .unwrap_err()
    .to_string();
    assert!(refused.contains("不是中层"), "{refused}");

    // Nor to write a judgement: a decision is a manager's output.
    let refused = record_decision(&db, &json!({ "employeeId": writer.id, "title": "我觉得该发" }))
        .unwrap_err()
        .to_string();
    assert!(refused.contains("不是中层"), "{refused}");

    // A manager's board node is the only place a manager's powers come from, so
    // a worker cannot be bound to one.
    let graph = json!({
        "nodes": [
            { "id": "t1", "kind": "trigger", "config": { "mode": "schedule" } },
            { "id": "b1", "kind": "board", "config": { "action": "read", "board": "leads" } }
        ],
        "edges": [{ "from": "t1", "to": "b1" }]
    });
    let workflow = create_workflow(
        &db,
        &json!({ "name": "别人的看板", "status": "active", "graph": graph }),
    )
    .unwrap();
    let refused = create_employee(
        &db,
        &json!({
            "name": "越权的人",
            "workflowId": workflow.id,
            "workflowVersion": workflow.latest_version,
        }),
    )
    .unwrap_err()
    .to_string();
    assert!(refused.contains("only a manager"), "{refused}");
}

#[test]
fn a_manager_hands_out_five_orders_a_day_and_the_next_one_is_refused() {
    let (_dir, db, secrets) = setup();
    staff_the_company(&db, &secrets);
    let manager = by_name(&db, "线索主管");

    for index in 0..5 {
        assign_directive(
            &db,
            &json!({
                "issuerEmployeeId": manager.id,
                "employeeName": "笔杆子",
                "verb": "produce",
                "task": format!("第 {index} 件活：写一条留言"),
            }),
        )
        .unwrap();
    }
    assert_eq!(dispatch_scope_of(&db, &manager.id).unwrap()["spentToday"], json!(5));

    // The sixth is refused, and the refusal is a sentence the owner can read.
    let refused = assign_directive(
        &db,
        &json!({
            "issuerEmployeeId": manager.id,
            "employeeName": "笔杆子",
            "verb": "produce",
            "task": "再来一件",
        }),
    )
    .unwrap_err()
    .to_string();
    assert!(refused.contains("上限"), "{refused}");

    // A judgement whose orders were all refused still lands: the owner sees
    // what the manager concluded and why it could not act on it.
    let decision = record_decision(
        &db,
        &json!({
            "employeeId": manager.id,
            "board": "leads",
            "title": "今天还有两条值得动",
            "dispatched": [{ "to": "笔杆子", "verb": "produce", "task": "第六条" }],
        }),
    )
    .unwrap();
    assert!(decision.dispatched.is_empty());
    assert!(decision.note.contains("上限"), "{}", decision.note);
    assert_eq!(list_directives(&db, &json!({})).unwrap().len(), 5);
}

#[test]
fn an_order_that_missed_its_deadline_is_closed_instead_of_run_late() {
    let (_dir, db, secrets) = setup();
    staff_the_company(&db, &secrets);
    let manager = by_name(&db, "线索主管");
    let writer = by_name(&db, "笔杆子");

    let order = assign_directive(
        &db,
        &json!({
            "issuerEmployeeId": manager.id,
            "employeeName": "笔杆子",
            "verb": "produce",
            "task": "回那条昨天的问题",
            "deadlineHours": 1,
        }),
    )
    .unwrap();
    // Push the window into the past rather than waiting an hour for it.
    db.conn()
        .execute(
            "UPDATE staff_directives SET deadline_at = ?2 WHERE id = ?1",
            params![order.id, now_ms() - 1_000],
        )
        .unwrap();

    let batch = claim_due(&ctx(&db, &secrets), 8).unwrap();
    assert!(
        batch.claims.iter().all(|claim| claim.employee_id != writer.id),
        "a stale order must not wake anyone"
    );
    let expired = get_directive(&db, &order.id).unwrap().unwrap();
    assert_eq!(expired.status, "failed");
    assert!(expired.result_note.contains("超过期限"), "{}", expired.result_note);
    // And the manager is not stuck waiting on it: the window closed, and the
    // console says so instead of showing a queued order nobody will run.
}

#[test]
fn migration_from_v20_adds_the_manager_columns_and_keeps_every_row() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    let secrets = SecretStore::open(dir.path()).unwrap();
    let (employee_id, run_id, order_id) = {
        let db = Database::open(&path).unwrap();
        provider(&db, "vendor-a");
        let profile_id = profile(&db, &secrets, "Company key", "vendor-a", "sk-company");
        let employee_id = employee(&db, "Scout", &profile_id, "gpt-5.6-sol");
        let run = begin_run(&db, &employee_id, Some("manual"), None).unwrap();
        let suggestion = create_suggestion(
            &db,
            &json!({ "board": "leads", "title": "先动这一条" }),
        )
        .unwrap();
        let decided = decide_suggestion(
            &db,
            &json!({
                "id": suggestion.id,
                "decision": "accepted",
                "directive": {
                    "employeeId": employee_id,
                    "employeeName": "Scout",
                    "verb": "produce",
                },
            }),
        )
        .unwrap()
        .unwrap();
        let order_id = decided.directive.unwrap().id;
        // Rewind to the v20 shape: rank, the order's issuer and the run's
        // directive are all new, and SQLite has no "ADD COLUMN IF NOT EXISTS".
        db.conn()
            .execute_batch(
                "DROP TABLE staff_decisions;
                 DROP INDEX idx_staff_directives_queued;
                 ALTER TABLE staff_directives DROP COLUMN issuer_employee_id;
                 ALTER TABLE staff_directives DROP COLUMN reason_run_id;
                 ALTER TABLE staff_directives DROP COLUMN deadline_at;
                 ALTER TABLE staff_directives DROP COLUMN priority;
                 ALTER TABLE staff_directives DROP COLUMN claimed_run_id;
                 ALTER TABLE employee_runs DROP COLUMN directive_id;
                 ALTER TABLE employees DROP COLUMN layer;
                 ALTER TABLE employees DROP COLUMN board;",
            )
            .unwrap();
        db.conn().pragma_update(None, "user_version", 20).unwrap();
        (employee_id, run.id, order_id)
    };

    let db = Database::open(&path).unwrap();
    let version: i64 = db
        .conn()
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    assert!(migration_backup_path(&path, 20).exists());

    // Every row that predates the managers survives, and a column added later
    // reads as what an older row means rather than as null.
    let worker = get_employee(&db, &employee_id).unwrap().unwrap();
    assert_eq!(worker.layer, "worker");
    assert_eq!(worker.board, "");
    let run = get_run(&db, &run_id).unwrap().unwrap();
    assert!(run.directive_id.is_none());
    let order = get_directive(&db, &order_id).unwrap().unwrap();
    assert!(order.issuer_employee_id.is_none());
    assert_eq!(order.priority, 0);

    // And the new surfaces work on the upgraded file.
    let manager = create_employee(
        &db,
        &json!({ "name": "主管", "layer": "manager", "board": "leads" }),
    )
    .unwrap();
    assert_eq!(manager.board, "leads");
    assert!(list_decisions(&db, &json!({})).unwrap().is_empty());
    let recorded = record_decision(
        &db,
        &json!({ "employeeId": manager.id, "board": "leads", "title": "先动第一条" }),
    )
    .unwrap();
    assert_eq!(recorded.board, "leads");
}

#[test]
fn the_watch_lists_only_published_pieces_that_carry_a_url() {
    let (_dir, db, _secrets) = setup();
    let unpublished = upsert_content(
        &db,
        &json!({ "title": "还没批", "body": "先给做法，再问一句。", "status": "draft" }),
    )
    .unwrap();
    let published = upsert_content(
        &db,
        &json!({ "title": "已经发出去了", "channel": "zhihu", "topic": "冷启动", "body": "你说要在做一个方向的服务、想找几个人深度聊聊。", "status": "draft" }),
    )
    .unwrap();
    // The owner published it by hand and never told the console where.
    set_content_status(&db, &json!({ "id": published.id, "status": "published" })).unwrap();
    let listed = watch_targets(&db, &json!({})).unwrap();
    assert!(listed.is_empty(), "a published piece with no URL is not watchable");

    set_content_status(
        &db,
        &json!({ "id": published.id, "status": "published", "publishedUrl": "https://example.test/p/1" }),
    )
    .unwrap();
    let listed = watch_targets(&db, &json!({})).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].content_id, published.id);
    assert_eq!(listed[0].url, "https://example.test/p/1");
    assert_eq!(listed[0].topic, "冷启动");
    // The fingerprint is the head of the body with its whitespace collapsed:
    // that is what a collector looks for, because the site never told us our
    // own comment id.
    assert_eq!(fingerprint_of("  你说要在做一个方向的服务、想找几个人深度聊聊。"), "你说要在做一个方向的服务、想找几个人深度");
    assert_eq!(fingerprint_of("  你说要在做一个方向的服务、想找几个人深度聊聊。").chars().count(), 20);
    assert!(listed[0].fingerprint.starts_with("你说要在做一个方向的服务"));
    assert!(!unpublished.id.is_empty());

    let summary = watch_summary(&db, &json!({})).unwrap();
    assert_eq!(summary["published"], json!(1));
    assert_eq!(summary["watchable"], json!(1));
    assert_eq!(summary["unwatchable"], json!(0));
}

#[test]
fn a_stranger_answering_becomes_one_card_however_often_the_watch_runs() {
    let (_dir, db, _secrets) = setup();
    let today = local_day(now_ms());
    let content = upsert_content(
        &db,
        &json!({ "title": "回复见微观澜", "channel": "zhihu", "topic": "冷启动", "body": "你说要在做一个方向的服务、想找几个人深度聊聊。" }),
    )
    .unwrap();
    set_content_status(
        &db,
        &json!({ "id": content.id, "status": "published", "publishedUrl": "https://example.test/p/1" }),
    )
    .unwrap();

    let first = ingest_watch(
        &db,
        &json!({
            "contentId": content.id,
            "found": true,
            "commentId": "11581503149",
            "likes": 3,
            "replies": 1,
            "items": [{ "author": "路人甲", "text": "这个思路我试过，确实有人回。", "at": "3 分钟前" }],
        }),
    )
    .unwrap();
    assert_eq!(first["found"], json!(true));
    assert_eq!(first["newReplies"], json!(1));
    assert_eq!(first["leads"].as_array().unwrap().len(), 1);

    // The page is the source of truth for the count, and the card is filed once.
    let stored = get_content(&db, &content.id).unwrap().unwrap();
    assert_eq!(stored.replies, 1);
    let leads = list_leads(&db, &json!({ "stage": "replied" })).unwrap();
    assert_eq!(leads.len(), 1);
    assert_eq!(leads[0].author, "路人甲");
    assert_eq!(leads[0].site, "zhihu");
    assert!(leads[0].replied_at.is_some());
    let metrics = list_metrics(&db, &json!({ "dimension": "topic" })).unwrap();
    let trend = metrics.iter().find(|metric| metric.label == "冷启动").unwrap();
    assert_eq!(trend.replies, 1);
    assert_eq!(trend.day, today);

    // A second pass ten minutes later sees the same one reply.
    let second = ingest_watch(
        &db,
        &json!({
            "contentId": content.id,
            "found": true,
            "replies": 1,
            "items": [{ "author": "路人甲", "text": "这个思路我试过，确实有人回。", "at": "3 分钟前" }],
        }),
    )
    .unwrap();
    assert_eq!(second["newReplies"], json!(0));
    assert_eq!(second["leads"].as_array().unwrap().len(), 0);
    assert_eq!(list_leads(&db, &json!({ "stage": "replied" })).unwrap().len(), 1);
    let metrics = list_metrics(&db, &json!({ "dimension": "topic" })).unwrap();
    let trend = metrics.iter().find(|metric| metric.label == "冷启动").unwrap();
    // The day's row is a movement, not a running total: reading the same count
    // twice must not count the reply twice.
    assert_eq!(trend.replies, 1);

    // A second person does file a second card, and the trend moves by one.
    let third = ingest_watch(
        &db,
        &json!({
            "contentId": content.id,
            "found": true,
            "replies": 2,
            "items": [
                { "author": "路人乙", "text": "请问你们是怎么找到这些人的？" },
                { "author": "路人甲", "text": "这个思路我试过，确实有人回。" },
            ],
        }),
    )
    .unwrap();
    assert_eq!(third["newReplies"], json!(1));
    assert_eq!(third["leads"].as_array().unwrap().len(), 1);
    let metrics = list_metrics(&db, &json!({ "dimension": "topic" })).unwrap();
    let trend = metrics.iter().find(|metric| metric.label == "冷启动").unwrap();
    assert_eq!(trend.replies, 2);
}

#[test]
fn a_page_that_did_not_show_our_comment_writes_nothing() {
    let (_dir, db, _secrets) = setup();
    let content = upsert_content(
        &db,
        &json!({ "title": "被挤到后面的那条", "channel": "zhihu", "topic": "冷启动", "body": "这是一条会被翻页藏起来的留言。" }),
    )
    .unwrap();
    set_content_status(
        &db,
        &json!({ "id": content.id, "status": "published", "publishedUrl": "https://example.test/p/2" }),
    )
    .unwrap();

    let result = ingest_watch(
        &db,
        &json!({ "contentId": content.id, "found": false, "reason": "comment not on the first page" }),
    )
    .unwrap();
    assert_eq!(result["found"], json!(false));
    assert_eq!(result["newReplies"], json!(0));

    // Nothing was written: a zero nobody read would read as "nobody answered".
    let stored = get_content(&db, &content.id).unwrap().unwrap();
    assert_eq!(stored.replies, 0);
    assert!(list_metrics(&db, &json!({ "dimension": "topic" })).unwrap().is_empty());
    assert!(list_leads(&db, &json!({})).unwrap().is_empty());
}


/* --------------------------------------------------------------- the ledger -- */

/// Age a row past a retention window, the way a month of real use would.
fn age_row(db: &Database, table: &str, id: &str, days: i64) {
    let at = now_ms() - days * 86_400_000;
    db.conn()
        .execute(
            &format!("UPDATE {table} SET created_at = ?2, updated_at = ?2 WHERE id = ?1"),
            params![id, at],
        )
        .unwrap();
}

/// A card the cleanup would want: dropped at triage, and long past retention.
fn dead_card(db: &Database, title: &str) -> Lead {
    let lead = upsert_lead(db, &json!({ "title": title, "score": 20 })).unwrap();
    move_lead(db, &json!({ "id": lead.id, "stage": "dropped" })).unwrap();
    age_row(db, "staff_leads", &lead.id, RETENTION_LEADS_DAYS + 1);
    get_lead(db, &lead.id).unwrap().unwrap()
}

/// A piece nobody wants any more, carrying whatever still points at it.
fn retired_draft(
    db: &Database,
    title: &str,
    lead_id: Option<&str>,
    run_id: Option<&str>,
) -> Content {
    let content = upsert_content(
        db,
        &json!({ "title": title, "body": "一段没人要的稿子，写出来就是为了占地方。" }),
    )
    .unwrap();
    db.conn()
        .execute(
            "UPDATE staff_contents SET status = 'retired', lead_id = ?2, run_id = ?3 WHERE id = ?1",
            params![content.id, lead_id, run_id],
        )
        .unwrap();
    age_row(db, "staff_contents", &content.id, RETENTION_CONTENTS_DAYS + 1);
    get_content(db, &content.id).unwrap().unwrap()
}

/// One stage's line in the ledger.
fn stage<'a>(stats: &'a Value, id: &str) -> &'a Value {
    stats["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == json!(id))
        .unwrap_or_else(|| panic!("no stage {id}"))
}

#[test]
fn the_ledger_reports_what_a_cleanup_would_really_take() {
    let (_dir, db, secrets) = setup();
    provider(&db, "vendor-a");
    let profile_id = profile(&db, &secrets, "Company key", "vendor-a", "sk-company");
    let employee_id = employee(&db, "Scout", &profile_id, "gpt-5.6-sol");
    let run = begin_run(&db, &employee_id, Some("manual"), None).unwrap();

    // Two dead cards, one of which a draft still answers.
    let dead = dead_card(&db, "没人回的第一条");
    let answered = dead_card(&db, "还有人写过的第二条");
    let live = upsert_lead(&db, &json!({ "title": "今天的卡", "score": 70 })).unwrap();

    // Three pieces nobody wants, one draft still in hand, one out in the world.
    let refused = retired_draft(&db, "被驳回的稿子", Some(&answered.id), None);
    let cited = retired_draft(&db, "被中层引用过的稿子", None, Some(&run.id));
    retired_draft(&db, "没人要的稿子", None, None);
    let writing = upsert_content(&db, &json!({ "title": "还在写的" })).unwrap();
    let published = upsert_content(&db, &json!({ "title": "发出去的那篇" })).unwrap();
    set_content_status(
        &db,
        &json!({ "id": published.id, "status": "published", "publishedUrl": "https://example.test/p/1" }),
    )
    .unwrap();

    // A proposal citing the run that wrote the third piece. That citation is
    // the only reason anyone would open it again, so the cleanup must not.
    create_suggestion(
        &db,
        &json!({
            "board": "contents",
            "title": "这稿子还留着当证据",
            "evidence": [{ "label": "运行记录", "runId": run.id }],
        }),
    )
    .unwrap();

    let stats = data_stats(&db, &json!({})).unwrap();

    // 巡站 keeps nothing, and says so rather than reporting a zero that reads
    // like a fault.
    assert_eq!(stats["stages"][0]["id"], json!("scan"));
    assert_eq!(stage(&stats, "scan")["rows"], json!(0));

    assert_eq!(stage(&stats, "leads")["rows"], json!(3));
    assert_eq!(stage(&stats, "leads")["cleanable"], json!(1));
    assert_eq!(stage(&stats, "contents")["rows"], json!(1));
    assert_eq!(stage(&stats, "contents")["cleanable"], json!(1));
    // Three pieces nobody wants are still rows on disk, and the ledger says so
    // rather than printing a zero over a table full of text.
    assert_eq!(stage(&stats, "contents")["parkedRows"], json!(3));
    assert!(stage(&stats, "contents")["parkedBytes"].as_i64().unwrap() > 0);
    // A cleanup only ever offers part of what is parked, never more than it.
    assert!(
        stage(&stats, "contents")["cleanable"].as_i64().unwrap()
            <= stage(&stats, "contents")["parkedRows"].as_i64().unwrap()
    );
    assert_eq!(stage(&stats, "publish")["rows"], json!(1));
    assert_eq!(stage(&stats, "publish")["cleanable"], json!(0));
    assert!(stage(&stats, "runs")["rows"].as_i64().unwrap() >= 1);
    assert!(stats["database"]["bytes"].as_i64().unwrap() > 0);
    // The rows really do weigh something: a byte count of zero would mean the
    // measurement stopped working, not that the text is empty.
    assert!(stage(&stats, "leads")["cleanableBytes"].as_i64().unwrap() > 0);

    // The window is the owner's to set, and a nonsense one is refused.
    let tight = data_stats(&db, &json!({ "leadDays": 1 })).unwrap();
    assert_eq!(tight["retention"]["leads"], json!(1));
    let error = data_stats(&db, &json!({ "leadDays": "soon" }))
        .unwrap_err()
        .to_string();
    assert!(error.contains("INVALID_ARGUMENT"), "{error}");

    // ---- measuring changes nothing ------------------------------------------
    let dry = prune(&db, &json!({ "dryRun": true, "rules": ["leads-dead", "contents-retired"] }))
        .unwrap();
    assert_eq!(dry["taken"][0]["rows"], json!(1));
    assert_eq!(dry["taken"][1]["rows"], json!(1));
    // The promise and the measurement are the same number, from the same code.
    assert_eq!(dry["taken"][0]["bytes"], stage(&stats, "leads")["cleanableBytes"]);
    assert_eq!(dry["taken"][1]["bytes"], stage(&stats, "contents")["cleanableBytes"]);
    assert!(get_lead(&db, &dead.id).unwrap().is_some());
    assert!(get_content(&db, &refused.id).unwrap().is_some());

    // ---- and confirming takes exactly that ---------------------------------
    let taken =
        prune(&db, &json!({ "dryRun": false, "rules": ["leads-dead", "contents-retired"] }))
            .unwrap();
    assert_eq!(taken["taken"][0]["rows"], json!(1));
    assert_eq!(taken["taken"][1]["rows"], json!(1));
    assert!(get_lead(&db, &dead.id).unwrap().is_none());
    // The card a draft answers, the piece a citation rests on, the piece that
    // went out into the world: all three stay.
    assert!(get_lead(&db, &answered.id).unwrap().is_some());
    assert!(get_lead(&db, &live.id).unwrap().is_some());
    assert!(get_content(&db, &refused.id).unwrap().is_some());
    assert!(get_content(&db, &cited.id).unwrap().is_some());
    assert!(get_content(&db, &writing.id).unwrap().is_some());
    assert!(get_content(&db, &published.id).unwrap().is_some());
}

#[test]
fn the_run_cleanup_takes_the_report_and_leaves_the_steps() {
    let (_dir, db, secrets) = setup();
    provider(&db, "vendor-a");
    let profile_id = profile(&db, &secrets, "Company key", "vendor-a", "sk-company");
    let employee_id = employee(&db, "Scout", &profile_id, "gpt-5.6-sol");

    // Thirty-two runs: the two oldest are the only ones past the window.
    let mut ids = Vec::new();
    for index in 0..32 {
        let run = begin_run(&db, &employee_id, Some("manual"), None).unwrap();
        set_run_step(&db, &run.id, 1, Some("读了三页"), Some("整份回复，就是占地方的那部分")).unwrap();
        finish_run(&db, &run.id, "succeeded", None).unwrap();
        db.conn()
            .execute(
                "UPDATE employee_runs SET started_at = ?2 WHERE id = ?1",
                params![run.id, now_ms() - (32 - index) * 60_000],
            )
            .unwrap();
        ids.push(run.id);
    }

    let stats = data_stats(&db, &json!({})).unwrap();
    assert_eq!(stage(&stats, "runs")["cleanable"], json!(2));

    let taken = prune(&db, &json!({ "dryRun": false, "rules": ["runs-reports"] })).unwrap();
    assert_eq!(taken["taken"][0]["rows"], json!(2));

    let reports = |id: &str| -> (String, String) {
        db.conn()
            .query_row(
                "SELECT results_json, reports_json FROM employee_runs WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap()
    };

    // The stale run keeps its row and its one-line steps -- that line is what a
    // board cites -- and loses only the bulky reply nobody reads twice.
    let (results, reports_json) = reports(&ids[0]);
    assert_eq!(reports_json, "{}");
    assert!(results.contains("读了三页"), "{results}");

    // The most recent run is untouched: thirty is the window in which "why did
    // it post that" still gets asked.
    let (_, recent) = reports(&ids[31]);
    assert_ne!(recent, "{}");

    // A second pass has nothing left to take, which is what makes the count on
    // the page a promise rather than a guess.
    let again = data_stats(&db, &json!({})).unwrap();
    assert_eq!(stage(&again, "runs")["cleanable"], json!(0));
}

#[test]
fn an_expired_fact_is_retired_rather_than_deleted() {
    let (_dir, db, _secrets) = setup();
    db.conn()
        .execute(
            "INSERT INTO memory_items
               (id, scope, kind, memory_key, content, source_kind, status, created_at, expires_at)
             VALUES ('m-old', 'employee', 'fact', '价格', '三千八', 'observation', 'active', ?1, ?2)",
            params![now_ms() - 10 * 86_400_000, now_ms() - 86_400_000],
        )
        .unwrap();

    let stats = data_stats(&db, &json!({})).unwrap();
    assert_eq!(stage(&stats, "memory")["cleanable"], json!(1));

    let taken = prune(&db, &json!({ "dryRun": false, "rules": ["memory-expired"] })).unwrap();
    assert_eq!(taken["taken"][0]["rows"], json!(1));

    // What it said is still the record of why something was done, so the row
    // survives and only its status moves.
    let (content, status): (String, String) = db
        .conn()
        .query_row(
            "SELECT content, status FROM memory_items WHERE id = 'm-old'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "retired");
    assert_eq!(content, "三千八");

    // The count and the byte size describe the same rows, and a fact that
    // stopped being live is still on disk rather than gone from the ledger.
    let stats = data_stats(&db, &json!({})).unwrap();
    assert_eq!(stage(&stats, "memory")["rows"], json!(0));
    assert_eq!(stage(&stats, "memory")["bytes"], json!(0));
    assert_eq!(stage(&stats, "memory")["parkedRows"], json!(1));
    assert_eq!(
        stage(&stats, "memory")["parkedBytes"],
        json!("三千八".len() as i64 + "价格".len() as i64)
    );
}

#[test]
fn the_dispatch_cleanup_takes_a_proposal_and_the_order_it_became() {
    let (_dir, db, secrets) = setup();
    provider(&db, "vendor-a");
    let profile_id = profile(&db, &secrets, "Company key", "vendor-a", "sk-company");
    let employee_id = employee(&db, "Scout", &profile_id, "gpt-5.6-sol");

    let raised = create_suggestion(
        &db,
        &json!({ "board": "leads", "title": "有一批卡凉了", "action": "挑一遍" }),
    )
    .unwrap();
    let decided = decide_suggestion(
        &db,
        &json!({
            "id": raised.id,
            "decision": "accepted",
            "directive": { "employeeId": employee_id, "verb": "挑一遍待触达的卡" },
        }),
    )
    .unwrap()
    .unwrap();
    let directive = decided.directive.unwrap();
    update_directive(&db, &json!({ "id": directive.id, "status": "done" })).unwrap();

    // Freshly decided, it is still the record of what the middle manager saw.
    let stats = data_stats(&db, &json!({})).unwrap();
    assert_eq!(stage(&stats, "dispatch")["cleanable"], json!(0));

    age_row(
        &db,
        "staff_suggestions",
        &raised.id,
        RETENTION_DISPATCH_DAYS + 1,
    );
    age_row(
        &db,
        "staff_directives",
        &directive.id,
        RETENTION_DISPATCH_DAYS + 1,
    );

    let stats = data_stats(&db, &json!({})).unwrap();
    assert_eq!(stage(&stats, "dispatch")["cleanable"], json!(2));

    let taken = prune(&db, &json!({ "dryRun": false, "rules": ["dispatch-answered"] })).unwrap();
    assert_eq!(taken["taken"][0]["rows"], json!(2));
    assert!(get_suggestion(&db, &raised.id).unwrap().is_none());
    assert!(get_directive(&db, &directive.id).unwrap().is_none());
}

#[test]
fn an_unknown_cleanup_rule_is_refused() {
    let (_dir, db, _secrets) = setup();
    let error = prune(&db, &json!({ "dryRun": false, "rules": ["delete-everything"] }))
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown prune rule"), "{error}");

    let error = prune(&db, &json!({ "dryRun": true }))
        .unwrap_err()
        .to_string();
    assert!(error.contains("rules is required"), "{error}");
}
