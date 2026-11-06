use super::*;

pub(super) fn migrate(conn: &Connection, path: &Path) -> Result<()> {
    create_migration_backup(conn, path, 15)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(crate::session_collaboration::SCHEMA)?;
    let has_column: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('turn_queue') WHERE name = 'session_message_id')",
        [],
        |row| row.get(0),
    )?;
    if !has_column {
        tx.execute_batch("ALTER TABLE turn_queue ADD COLUMN session_message_id TEXT;")?;
    }
    // Stamp the version this step produces, not the newest one: the chain
    // reads user_version back between steps, and stamping ahead would skip the
    // next step entirely.
    tx.pragma_update(None, "user_version", 16i64)?;
    tx.commit()
        .context("commit session collaboration migration")?;
    Ok(())
}
