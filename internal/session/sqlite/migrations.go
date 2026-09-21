// Forward-only schema migrations: each step is a named set of DDL statements
// applied inside its own transaction, with the applied version recorded in
// schema_migrations. Open runs every pending step before the store is
// handed out, so a database file always matches the binary's expectations —
// the hook SessionHeader.Version never had for the JSONL format.
package sqlite

import (
	"database/sql"
	"fmt"
)

const schemaMigrations = `CREATE TABLE IF NOT EXISTS schema_migrations (
	version    INTEGER PRIMARY KEY,
	name       TEXT NOT NULL,
	applied_at TEXT NOT NULL
)`

// migration is one schema step. Steps run strictly in version order and are
// never edited after shipping — a change is always a new step appended at
// the end (migrations are history, not code to refactor).
type migration struct {
	name  string
	stmts []string
}

var migrations = []migration{
	{
		name: "initial normalized schema",
		stmts: []string{
			schemaSessions,
			schemaEntries,
			schemaEntriesIndex,
		},
	},
}

// migrate brings db up to the latest schema version.
func migrate(db *sql.DB) error {
	if _, err := db.Exec(schemaMigrations); err != nil {
		return fmt.Errorf("session/sqlite: prepare migrations table: %w", err)
	}
	var current int
	if err := db.QueryRow(`SELECT COALESCE(MAX(version), 0) FROM schema_migrations`).Scan(&current); err != nil {
		return fmt.Errorf("session/sqlite: read schema version: %w", err)
	}
	for v := current; v < len(migrations); v++ {
		step := migrations[v]
		err := func() error {
			tx, err := db.Begin()
			if err != nil {
				return fmt.Errorf("session/sqlite: begin migration %d: %w", v+1, err)
			}
			for _, stmt := range step.stmts {
				if _, err := tx.Exec(stmt); err != nil {
					tx.Rollback()
					return fmt.Errorf("session/sqlite: migration %d (%s): %w", v+1, step.name, err)
				}
			}
			if _, err := tx.Exec(`INSERT INTO schema_migrations (version, name, applied_at) VALUES (?,?,datetime('now'))`, v+1, step.name); err != nil {
				tx.Rollback()
				return fmt.Errorf("session/sqlite: record migration %d: %w", v+1, err)
			}
			return tx.Commit()
		}()
		if err != nil {
			return err
		}
	}
	return nil
}
