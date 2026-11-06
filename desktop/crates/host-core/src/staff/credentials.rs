use super::*;

/// Where a profile's API key lives. Kept in its own namespace so a profile
/// secret can never collide with, or be read through, a provider secret ref.
pub fn secret_ref_for_profile(profile_id: &str) -> String {
    format!("secret:profile:{profile_id}:api_key")
}

/// A named account an employee runs on.
///
/// Several employees may share one profile (a company key) or each hold their
/// own. The key itself never appears here -- only `secret_ref`, which only
/// `SecretStore` can resolve.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialProfile {
    pub id: String,
    pub label: String,
    pub provider_id: String,
    pub secret_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    pub has_secret: bool,
    pub created_at: String,
    pub updated_at: String,
}

const PROFILE_SELECT: &str =
    "SELECT id, label, provider_id, secret_ref, base_url, created_at, updated_at
     FROM credential_profiles";

fn profile_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CredentialProfile> {
    Ok(CredentialProfile {
        id: row.get(0)?,
        label: row.get(1)?,
        provider_id: row.get(2)?,
        secret_ref: row.get(3)?,
        base_url: row.get(4)?,
        has_secret: false,
        created_at: ms_to_ts(row.get(5)?),
        updated_at: ms_to_ts(row.get(6)?),
    })
}

fn with_secret_state(secrets: &SecretStore, mut profile: CredentialProfile) -> CredentialProfile {
    profile.has_secret = secrets.has(&profile.secret_ref);
    profile
}

fn provider_exists(db: &Database, provider_id: &str) -> Result<()> {
    let exists: bool = db.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM providers WHERE id = ?1)",
        params![provider_id],
        |row| row.get(0),
    )?;
    if !exists {
        bail!("provider {provider_id} does not exist");
    }
    Ok(())
}

/// Record the profile as the owner of its secret ref.
///
/// Provider secrets are indexed with `owner_kind = 'provider'`; a profile must
/// not reuse that, or deleting a provider could clear a profile's key.
fn upsert_secret_meta(
    db: &Database,
    secret_ref: &str,
    profile_id: &str,
    backend: &str,
) -> Result<()> {
    db.conn()
        .prepare_cached(
            "INSERT INTO secrets_meta (secret_ref, owner_kind, owner_id, kind, backend, updated_at)
             VALUES (?1, 'credential_profile', ?2, 'api_key', ?3, ?4)
             ON CONFLICT(secret_ref) DO UPDATE SET
               updated_at = excluded.updated_at, backend = excluded.backend,
               owner_kind = excluded.owner_kind, owner_id = excluded.owner_id",
        )?
        .execute(params![secret_ref, profile_id, backend, now_ms()])?;
    Ok(())
}

/// Create a profile. `secretValue` is optional so a profile can be prepared
/// before its key is entered; the employee then fails closed at run time.
pub fn create_profile(
    db: &Database,
    secrets: &SecretStore,
    params_json: &Value,
) -> Result<CredentialProfile> {
    let label = required_str(params_json, "label")?;
    let provider_id = required_str(params_json, "providerId")?;
    provider_exists(db, &provider_id)?;

    let id = Uuid::new_v4().to_string();
    let secret_ref = secret_ref_for_profile(&id);
    let base_url = optional_str(params_json, "baseUrl");
    let now = now_ms();

    db.conn().execute(
        "INSERT INTO credential_profiles
           (id, label, provider_id, secret_ref, base_url, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![id, label, provider_id, secret_ref, base_url, now],
    )?;

    if let Some(value) = optional_str(params_json, "secretValue") {
        set_profile_secret(db, secrets, &id, &value)?;
    }

    get_profile(db, secrets, &id)?
        .ok_or_else(|| anyhow!("credential profile {id} disappeared after insert"))
}

pub fn list_profiles(db: &Database, secrets: &SecretStore) -> Result<Vec<CredentialProfile>> {
    let mut stmt = db
        .conn()
        .prepare_cached(&format!("{PROFILE_SELECT} ORDER BY created_at ASC"))?;
    let rows = stmt.query_map([], profile_from_row)?;
    let profiles = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(profiles
        .into_iter()
        .map(|profile| with_secret_state(secrets, profile))
        .collect())
}

pub fn get_profile(
    db: &Database,
    secrets: &SecretStore,
    id: &str,
) -> Result<Option<CredentialProfile>> {
    let profile = db
        .conn()
        .prepare_cached(&format!("{PROFILE_SELECT} WHERE id = ?1"))?
        .query_row(params![id], profile_from_row)
        .optional()?;
    Ok(profile.map(|profile| with_secret_state(secrets, profile)))
}

/// Update the label, base URL, or provider of a profile. A provider change is
/// allowed because the secret is per profile, not per provider.
pub fn update_profile(
    db: &Database,
    secrets: &SecretStore,
    id: &str,
    params_json: &Value,
) -> Result<Option<CredentialProfile>> {
    let Some(current) = get_profile(db, secrets, id)? else {
        return Ok(None);
    };

    let label = optional_str(params_json, "label").unwrap_or(current.label);
    let provider_id = optional_str(params_json, "providerId").unwrap_or(current.provider_id);
    provider_exists(db, &provider_id)?;
    let base_url = match params_json.get("baseUrl") {
        // An explicit null clears the override; an absent key keeps it.
        Some(Value::Null) => None,
        Some(_) => optional_str(params_json, "baseUrl"),
        None => current.base_url,
    };

    db.conn().execute(
        "UPDATE credential_profiles
         SET label = ?2, provider_id = ?3, base_url = ?4, updated_at = ?5
         WHERE id = ?1",
        params![id, label, provider_id, base_url, now_ms()],
    )?;

    if let Some(value) = optional_str(params_json, "secretValue") {
        set_profile_secret(db, secrets, id, &value)?;
    }

    get_profile(db, secrets, id)
}

/// Write the profile's API key. Returns `None` when the profile is gone.
pub fn set_profile_secret(
    db: &Database,
    secrets: &SecretStore,
    id: &str,
    value: &str,
) -> Result<Option<CredentialProfile>> {
    let Some(profile) = get_profile(db, secrets, id)? else {
        return Ok(None);
    };
    let backend = secrets.set(&profile.secret_ref, value)?;
    // The metadata row trails the ciphertext: a crash in between leaves a
    // readable secret with a stale index, which only costs a rewrite.
    upsert_secret_meta(db, &profile.secret_ref, id, &backend)?;
    get_profile(db, secrets, id)
}

/// Refuse to delete a profile an employee still runs on, so a scheduled
/// employee can never lose its credentials silently.
pub fn delete_profile(db: &Database, secrets: &SecretStore, id: &str) -> Result<bool> {
    let Some(profile) = get_profile(db, secrets, id)? else {
        return Ok(false);
    };

    let referenced: i64 = db.conn().query_row(
        "SELECT COUNT(*) FROM employees WHERE credential_profile_id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    if referenced > 0 {
        bail!(
            "credential profile {} is referenced by {referenced} employee(s); reassign them first",
            profile.label
        );
    }

    secrets.delete(&profile.secret_ref)?;
    let tx = db.conn().unchecked_transaction()?;
    tx.execute(
        "DELETE FROM secrets_meta WHERE secret_ref = ?1",
        params![profile.secret_ref],
    )?;
    tx.execute("DELETE FROM credential_profiles WHERE id = ?1", params![id])?;
    tx.commit().context("commit credential profile delete")?;
    Ok(true)
}

/// Resolve the key for a run. Fails closed with a readable error: a scheduled
/// employee that cannot authenticate must not start.
pub fn resolve_profile_secret(
    db: &Database,
    secrets: &SecretStore,
    profile_id: &str,
) -> Result<String> {
    let profile = get_profile(db, secrets, profile_id)?
        .ok_or_else(|| anyhow!("credential profile {profile_id} does not exist"))?;
    secrets
        .get(&profile.secret_ref)?
        .ok_or_else(|| anyhow!("credential profile {} has no API key", profile.label))
}
