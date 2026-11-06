use super::*;
use crate::db::create_migration_backup;

/// v16 -> v17: add the staff tables (ADR 0252).
///
/// The caller runs this for any database at or below v16 rather than exactly
/// v16, because the v15 step stamps the newest `user_version` when it commits.
/// Every statement is `IF NOT EXISTS`, so a database that arrives here already
/// carrying the tables is a no-op instead of an error.
pub(crate) fn migrate(conn: &rusqlite::Connection, path: &Path) -> Result<()> {
    create_migration_backup(conn, path, 16)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(SCHEMA)?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit().context("commit staff migration")?;
    Ok(())
}

/// v17 -> v18: record what each run step produced.
///
/// A run's steps carry a one-line result in the workflow's own terms, and the
/// run log is the only place an unattended run reports to. Without the column
/// a finished run says how far it got but never what it gathered.
pub(crate) fn migrate_run_results(conn: &rusqlite::Connection, path: &Path) -> Result<()> {
    create_migration_backup(conn, path, 17)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        "ALTER TABLE employee_runs ADD COLUMN results_json TEXT NOT NULL DEFAULT '{}';",
    )?;
    tx.pragma_update(None, "user_version", 18)?;
    tx.commit().context("commit run results migration")?;
    Ok(())
}

/// v18 -> v19: keep each step's full reply, not just its one-line result.
///
/// The collected material -- posts, quotes, links, the step's own working
/// notes -- lives in the step's final message, and the run log is where an
/// operator reads it. Without the column the row shows the verdict ("kept 2
/// of 12 candidates") but never the evidence behind it.
/// v19 -> v20: the boards the middle managers read.
///
/// The employees already produce the evidence; what was missing was anywhere
/// to put what it turns into -- the opportunity cards, the drafts, the daily
/// numbers, and the proposals an owner accepts or rejects. Every statement in
/// the shared schema is `IF NOT EXISTS`, so this runs the whole file and only
/// the new tables land.
pub(crate) fn migrate_boards(conn: &rusqlite::Connection, path: &Path) -> Result<()> {
    create_migration_backup(conn, path, 19)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(SCHEMA)?;
    tx.pragma_update(None, "user_version", 20)?;
    tx.commit().context("commit boards migration")?;
    Ok(())
}

/// v20 -> v21: the middle managers (ADR 0254).
///
/// Two things were missing. The roster had no rank, so nothing could tell a
/// worker from the manager that hands work out; and a decision a manager makes
/// had nowhere to live, so its reasoning was only visible as the cards it
/// eventually caused. The columns are added one at a time because ALTER TABLE
/// has no `IF NOT EXISTS` -- a database that arrived here at 16 already carries
/// them from the schema file, and adding them twice is the failure this
/// guard exists to prevent.
pub(crate) fn migrate_managers(conn: &rusqlite::Connection, path: &Path) -> Result<()> {
    create_migration_backup(conn, path, 20)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(SCHEMA)?;
    add_column(&tx, "employees", "layer", "TEXT NOT NULL DEFAULT 'worker'")?;
    add_column(&tx, "employees", "board", "TEXT NOT NULL DEFAULT ''")?;
    add_column(&tx, "employee_runs", "directive_id", "TEXT")?;
    add_column(&tx, "staff_directives", "issuer_employee_id", "TEXT")?;
    add_column(&tx, "staff_directives", "reason_run_id", "TEXT")?;
    add_column(&tx, "staff_directives", "deadline_at", "INTEGER")?;
    add_column(&tx, "staff_directives", "priority", "INTEGER NOT NULL DEFAULT 0")?;
    add_column(&tx, "staff_directives", "claimed_run_id", "TEXT")?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit().context("commit managers migration")?;
    Ok(())
}

/// Add a column only when the table does not already carry it.
///
/// The schema file is the source of truth and every table in it is created
/// `IF NOT EXISTS`, but SQLite has no such form for a column, so a migration
/// that has to add one reads the table's own description first.
fn add_column(
    tx: &rusqlite::Transaction<'_>,
    table: &str,
    column: &str,
    ddl: &str,
) -> Result<()> {
    let mut stmt = tx.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut held = false;
    for row in rows {
        if row? == column {
            held = true;
            break;
        }
    }
    drop(stmt);
    if !held {
        tx.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {ddl};"))?;
    }
    Ok(())
}

pub(crate) fn migrate_run_reports(conn: &rusqlite::Connection, path: &Path) -> Result<()> {
    create_migration_backup(conn, path, 18)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        "ALTER TABLE employee_runs ADD COLUMN reports_json TEXT NOT NULL DEFAULT '{}';",
    )?;
    tx.pragma_update(None, "user_version", 19)?;
    tx.commit().context("commit run reports migration")?;
    Ok(())
}
