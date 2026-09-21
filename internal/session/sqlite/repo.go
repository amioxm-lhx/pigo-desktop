// Package sqlite is the normalized-table session backend (plan phase 3b):
// sessions and entries live as authoritative rows in one SQLite database
// instead of one JSONL file per session. It satisfies session.Store with the
// same method semantics, so callers switch backends at the construction site
// and nowhere else.
//
// Concurrency model: the connection pool is capped at one connection, which
// serializes every statement — the database is the arbiter, so the JSONL
// backend's lease/transaction machinery (lease.go) has no equivalent here.
// WAL journaling keeps that single connection crash-safe and lets a reader
// process work alongside. Concurrent Append/AppendBranch calls queue on the
// connection exactly like they queue on the JSONL write-transaction mutex;
// arrival-order serialization is all the contract promises.
//
// The dependency direction is inward: this package knows session (types,
// PathToLeaf, the JSONL exchange codecs) and agentcore; nothing in session
// knows this package.
package sqlite

import (
	"database/sql"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	_ "modernc.org/sqlite" // registers the "sqlite" driver (pure Go, no CGO)

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/session"
)

// Compile-time proof that the sqlite backend satisfies the contract.
var _ session.Store = (*Store)(nil)

// Store is the SQLite-backed session.Store. Construct with Open; Close it
// when done (Close is deliberately outside the session.Store interface —
// the JSONL backend has no equivalent, so hosts opt in per backend).
type Store struct {
	db   *sql.DB
	dir  string // the database file's parent directory (Dir)
	path string // the database file path
}

// Open opens (creating if needed) the database at path and runs pending
// migrations. The parent directory is created when missing. The returned
// Store is safe for concurrent use.
func Open(path string) (*Store, error) {
	if path == "" {
		return nil, fmt.Errorf("session/sqlite: database path must not be empty")
	}
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return nil, fmt.Errorf("session/sqlite: create %s: %w", dir, err)
	}
	// WAL for crash safety; a busy timeout so a second process touching the
	// same database fails loudly instead of spinning; foreign keys so the
	// entries→sessions reference is enforced by the engine.
	dsn := "file:" + path + "?_pragma=journal_mode(WAL)&_pragma=busy_timeout(5000)&_pragma=foreign_keys(ON)"
	db, err := sql.Open("sqlite", dsn)
	if err != nil {
		return nil, fmt.Errorf("session/sqlite: open %s: %w", path, err)
	}
	// One connection serializes every statement: SQLite has a single writer,
	// and queueing in the pool avoids SQLITE_BUSY between this process's own
	// goroutines entirely. Session-store traffic is millisecond-scale; the
	// simplicity is worth more than read concurrency here.
	db.SetMaxOpenConns(1)
	db.SetMaxIdleConns(1)
	db.SetConnMaxLifetime(0)
	if err := migrate(db); err != nil {
		db.Close()
		return nil, err
	}
	return &Store{db: db, dir: dir, path: path}, nil
}

// Close closes the database. The store is unusable afterwards.
func (s *Store) Close() error {
	if s == nil || s.db == nil {
		return nil
	}
	return s.db.Close()
}

// Dir returns the database file's parent directory.
func (s *Store) Dir() string { return s.dir }

// Path returns the database file path (the sqlite-specific accessor; Dir
// mirrors session.Store's directory notion).
func (s *Store) Path() string { return s.path }

// withTx runs fn inside one transaction, rolling back on error.
func (s *Store) withTx(fn func(tx *sql.Tx) error) error {
	tx, err := s.db.Begin()
	if err != nil {
		return fmt.Errorf("session/sqlite: begin: %w", err)
	}
	if err := fn(tx); err != nil {
		tx.Rollback()
		return err
	}
	if err := tx.Commit(); err != nil {
		return fmt.Errorf("session/sqlite: commit: %w", err)
	}
	return nil
}

// Save writes header and messages as a fresh session (a new linear entry
// chain), replacing any existing session with the same id. It mirrors the
// JSONL Save semantics, including the Version force.
func (s *Store) Save(header session.SessionHeader, messages agentcore.MessageList) error {
	header.Version = session.SchemaVersion
	if header.ID == "" {
		return fmt.Errorf("session/sqlite: header ID must not be empty")
	}
	now := time.Now().UTC()
	entries := make([]session.Entry, 0, len(messages))
	parentID := ""
	for _, m := range messages {
		e := session.Entry{ID: newEntryID(), ParentID: parentID, Timestamp: now, Message: m}
		entries = append(entries, e)
		parentID = e.ID
	}
	return s.withTx(func(tx *sql.Tx) error {
		return saveEntriesTx(tx, header, entries)
	})
}

// SaveEntries writes header plus the given entries verbatim — ids and
// parentIds preserved — replacing any existing session with the same id.
func (s *Store) SaveEntries(header session.SessionHeader, entries []session.Entry) error {
	header.Version = session.SchemaVersion
	if header.ID == "" {
		return fmt.Errorf("session/sqlite: header ID must not be empty")
	}
	return s.withTx(func(tx *sql.Tx) error {
		return saveEntriesTx(tx, header, entries)
	})
}

// Load reads a session and returns its header and messages in insertion
// order (the linear view of the entry tree).
func (s *Store) Load(id string) (session.SessionHeader, agentcore.MessageList, error) {
	header, entries, err := s.LoadEntries(id)
	if err != nil {
		return session.SessionHeader{}, nil, err
	}
	msgs := make(agentcore.MessageList, len(entries))
	for i, e := range entries {
		msgs[i] = e.Message
	}
	return header, msgs, nil
}

// LoadEntries reads a session and returns its header plus the tree entries
// in insertion order. A missing session reports an error wrapping
// os.ErrNotExist — the same signal the JSONL backend gives, and the one
// AppendBranch relies on to detect the create-fresh path.
func (s *Store) LoadEntries(id string) (session.SessionHeader, []session.Entry, error) {
	var header session.SessionHeader
	found := false
	err := s.withRead(func(tx *sql.Tx) error {
		h, ok, err := loadHeaderTx(tx, id)
		if err != nil || !ok {
			return err
		}
		header, found = h, true
		return nil
	})
	if err != nil {
		return session.SessionHeader{}, nil, err
	}
	if !found {
		return session.SessionHeader{}, nil, fmt.Errorf("session/sqlite: open %s: %w", id, os.ErrNotExist)
	}
	rows, err := s.db.Query(selectEntriesSQL, id)
	if err != nil {
		return session.SessionHeader{}, nil, fmt.Errorf("session/sqlite: query entries %s: %w", id, err)
	}
	defer rows.Close()
	var entries []session.Entry
	for rows.Next() {
		e, err := scanEntry(rows)
		if err != nil {
			return session.SessionHeader{}, nil, fmt.Errorf("session/sqlite: scan entry %s: %w", id, err)
		}
		entries = append(entries, e)
	}
	if err := rows.Err(); err != nil {
		return session.SessionHeader{}, nil, fmt.Errorf("session/sqlite: entries %s: %w", id, err)
	}
	return header, entries, nil
}

// withRead runs fn in a transaction it then rolls back — a read-only helper
// that reuses the single pooled connection without holding a write lock.
func (s *Store) withRead(fn func(tx *sql.Tx) error) error {
	tx, err := s.db.Begin()
	if err != nil {
		return fmt.Errorf("session/sqlite: begin: %w", err)
	}
	if err := fn(tx); err != nil {
		tx.Rollback()
		return err
	}
	return tx.Rollback()
}

// List returns the headers of all sessions, most recently updated first.
func (s *Store) List() ([]session.SessionHeader, error) {
	rows, err := s.db.Query(listSessionsSQL)
	if err != nil {
		return nil, fmt.Errorf("session/sqlite: list sessions: %w", err)
	}
	defer rows.Close()
	var headers []session.SessionHeader
	for rows.Next() {
		h, err := scanHeader(rows)
		if err != nil {
			return nil, fmt.Errorf("session/sqlite: scan header: %w", err)
		}
		headers = append(headers, h)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("session/sqlite: list sessions: %w", err)
	}
	return headers, nil
}

// Append adds messages to an existing session and bumps its UpdatedAt. A
// missing session is an error — use Save to create one. Unlike the JSONL
// backend (which rewrites the whole chain with fresh ids), existing entries
// keep their ids and the new messages grow a chain off the current tail:
// both satisfy the contract (count and content), and preserving ids is the
// more faithful tree behavior.
func (s *Store) Append(id string, updatedAt time.Time, messages agentcore.MessageList) error {
	return s.withTx(func(tx *sql.Tx) error {
		header, ok, err := loadHeaderTx(tx, id)
		if err != nil {
			return err
		}
		if !ok {
			return fmt.Errorf("session/sqlite: append %s: %w", id, os.ErrNotExist)
		}
		header.UpdatedAt = updatedAt
		parent, err := tailParentTx(tx, id)
		if err != nil {
			return err
		}
		_, err = appendBranchTx(tx, header, parent, messages)
		return err
	})
}

// AppendBranch appends messages as a chain descending from parentLeafID,
// preserving every existing entry (and therefore any other branches). It
// returns the id of the new leaf. A session that does not exist yet is
// created with the given header.
func (s *Store) AppendBranch(header session.SessionHeader, parentLeafID string, messages agentcore.MessageList) (string, error) {
	if header.ID == "" {
		return "", fmt.Errorf("session/sqlite: header ID must not be empty")
	}
	header.Version = session.SchemaVersion
	var leaf string
	err := s.withTx(func(tx *sql.Tx) error {
		existing, ok, err := loadHeaderTx(tx, header.ID)
		if err != nil {
			return err
		}
		if ok {
			// The caller's header wins (mirrors the JSONL rewrite), but keep
			// CreatedAt from the stored row when the caller left it zero —
			// serve passes the stored header back, so this is usually equal.
			if header.CreatedAt.IsZero() {
				header.CreatedAt = existing.CreatedAt
			}
		} else if header.CreatedAt.IsZero() {
			header.CreatedAt = time.Now().UTC()
		}
		l, err := appendBranchTx(tx, header, parentLeafID, messages)
		if err != nil {
			return err
		}
		leaf = l
		return nil
	})
	if err != nil {
		return "", err
	}
	return leaf, nil
}

// Fork creates a new session holding the linear path from the source's root
// down to leafID, copied verbatim, with ParentSession recorded — the same
// shape as the JSONL Fork.
func (s *Store) Fork(sourceID, leafID string, now time.Time) (session.SessionHeader, []session.Entry, error) {
	srcHeader, entries, err := s.LoadEntries(sourceID)
	if err != nil {
		return session.SessionHeader{}, nil, err
	}
	path := session.PathToLeaf(entries, leafID)
	newHeader := session.SessionHeader{
		ID:            session.NewID(now),
		CreatedAt:     now,
		UpdatedAt:     now,
		Model:         srcHeader.Model,
		Provider:      srcHeader.Provider,
		SystemPrompt:  srcHeader.SystemPrompt,
		ParentSession: sourceID,
	}
	if err := s.SaveEntries(newHeader, path); err != nil {
		return session.SessionHeader{}, nil, err
	}
	return newHeader, path, nil
}

// Export writes the session to outPath: ".html"/".htm" produces the
// self-contained HTML transcript, anything else the JSONL exchange format.
// It reuses the session package's codecs, so an export is byte-compatible
// with a JSONL-backend export.
func (s *Store) Export(id, outPath string) (int, error) {
	header, entries, err := s.LoadEntries(id)
	if err != nil {
		return 0, err
	}
	f, err := os.Create(outPath)
	if err != nil {
		return 0, fmt.Errorf("session/sqlite: create export %s: %w", outPath, err)
	}
	defer f.Close()
	ext := strings.ToLower(filepath.Ext(outPath))
	if ext == ".html" || ext == ".htm" {
		err = session.WriteHTML(f, header, entries)
	} else {
		err = session.WriteJSONL(f, header, entries)
	}
	if err != nil {
		return 0, err
	}
	if err := f.Close(); err != nil {
		return 0, fmt.Errorf("session/sqlite: finalize export %s: %w", outPath, err)
	}
	return len(entries), nil
}

// Import reads a JSONL export at inPath and materializes it as a fresh
// session (new id, original id recorded as ParentSession, entries verbatim),
// exactly like the JSONL backend's Import.
func (s *Store) Import(inPath string, now time.Time) (session.SessionHeader, []session.Entry, error) {
	f, err := os.Open(inPath)
	if err != nil {
		return session.SessionHeader{}, nil, fmt.Errorf("session/sqlite: open import %s: %w", inPath, err)
	}
	defer f.Close()
	srcHeader, entries, err := session.ReadJSONL(f)
	if err != nil {
		return session.SessionHeader{}, nil, err
	}
	newHeader := session.SessionHeader{
		ID:            session.NewID(now),
		CreatedAt:     now,
		UpdatedAt:     now,
		Model:         srcHeader.Model,
		Provider:      srcHeader.Provider,
		SystemPrompt:  srcHeader.SystemPrompt,
		ParentSession: srcHeader.ID,
	}
	if err := s.SaveEntries(newHeader, entries); err != nil {
		return session.SessionHeader{}, nil, err
	}
	return newHeader, entries, nil
}

// ExportShare writes the session in the share-friendly format (markdown /
// jsonl / html), reusing the session package's redaction-aware renderers.
func (s *Store) ExportShare(id, outPath, format string, opts session.ShareOptions) (int, error) {
	header, entries, err := s.LoadEntries(id)
	if err != nil {
		return 0, err
	}
	normalizedFormat := strings.ToLower(strings.TrimPrefix(format, "."))
	if normalizedFormat == "json" || normalizedFormat == "jsonl" {
		if opts.Redact {
			return 0, fmt.Errorf("redaction does not apply to the lossless JSONL export; use --no-redact for a resumable archive or the md format for sharing")
		}
	}
	f, err := os.Create(outPath)
	if err != nil {
		return 0, fmt.Errorf("session/sqlite: create share export %s: %w", outPath, err)
	}
	defer f.Close()
	var writeErr error
	switch normalizedFormat {
	case "md", "markdown":
		writeErr = session.WriteMarkdown(f, header, entries, opts)
	case "json", "jsonl":
		writeErr = session.WriteJSONL(f, header, entries)
	case "html", "htm":
		writeErr = session.WriteHTML(f, header, entries)
	default:
		writeErr = fmt.Errorf("unknown share format %q (want md, json, or html)", format)
	}
	if writeErr != nil {
		return 0, writeErr
	}
	if err := f.Close(); err != nil {
		return 0, fmt.Errorf("session/sqlite: close share export %s: %w", outPath, err)
	}
	return len(entries), nil
}

// ReleaseSession is a no-op: the database is its own write arbiter (the
// single-connection pool plus SQLite's file locking), so there is no lease
// to drop. It exists to satisfy the shared contract.
func (s *Store) ReleaseSession(id string) error { return nil }
