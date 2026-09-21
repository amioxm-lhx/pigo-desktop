// Bidirectional migration between the JSONL and SQLite backends (plan
// phase 3b): the JSONL v3 format stays the exchange/share format, the
// database becomes the authoritative store, so moving a session history
// across is a first-class operation, not a hand-edit. Both directions copy
// the entry trees verbatim (ids/parentIds/timestamps preserved), which is
// what makes a round-trip lossless.
package sqlite

import (
	"fmt"

	"github.com/smallnest/pigo/internal/session"
)

// ImportJSONL copies every session from the JSONL backend src into this
// database (one transaction per session, so a failure mid-way leaves the
// already-copied sessions intact and the failed one untouched). Sessions
// that already exist here are replaced. It returns the number of sessions
// imported.
func (s *Store) ImportJSONL(src session.Store) (int, error) {
	headers, err := src.List()
	if err != nil {
		return 0, fmt.Errorf("session/sqlite: import: list source: %w", err)
	}
	imported := 0
	for _, h := range headers {
		_, entries, err := src.LoadEntries(h.ID)
		if err != nil {
			return imported, fmt.Errorf("session/sqlite: import: load %s: %w", h.ID, err)
		}
		if err := s.SaveEntries(h, entries); err != nil {
			return imported, fmt.Errorf("session/sqlite: import: save %s: %w", h.ID, err)
		}
		imported++
	}
	return imported, nil
}

// ExportJSONL copies every session from this database into the JSONL
// backend dst (one file per session, replaced if it exists). It returns the
// number of sessions exported.
func (s *Store) ExportJSONL(dst session.Store) (int, error) {
	headers, err := s.List()
	if err != nil {
		return 0, fmt.Errorf("session/sqlite: export: list source: %w", err)
	}
	exported := 0
	for _, h := range headers {
		_, entries, err := s.LoadEntries(h.ID)
		if err != nil {
			return exported, fmt.Errorf("session/sqlite: export: load %s: %w", h.ID, err)
		}
		if err := dst.SaveEntries(h, entries); err != nil {
			return exported, fmt.Errorf("session/sqlite: export: save %s: %w", h.ID, err)
		}
		exported++
	}
	return exported, nil
}
