// The normalized schema and its transactional read/write primitives: one
// row per session (sessions), one row per entry (entries) with a per-session
// sequence number carrying the JSONL file's line order. All writes go through
// *sql.Tx so a session's header and entries move atomically; the tree shape
// (id/parentId) is preserved verbatim, and the message column stores exactly
// the JSON the JSONL backend puts inside entryWire.message — same codec,
// different container.
package sqlite

import (
	"crypto/rand"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/session"
)

const schemaSessions = `
CREATE TABLE IF NOT EXISTS sessions (
	id                 TEXT PRIMARY KEY,
	version            INTEGER NOT NULL,
	created_at         TEXT NOT NULL,
	updated_at         TEXT NOT NULL,
	model              TEXT NOT NULL DEFAULT '',
	provider           TEXT NOT NULL DEFAULT '',
	system_prompt      TEXT NOT NULL DEFAULT '',
	parent_session     TEXT NOT NULL DEFAULT '',
	cwd                TEXT NOT NULL DEFAULT '',
	context_from       TEXT NOT NULL DEFAULT '',
	context_watermark  INTEGER NOT NULL DEFAULT 0
)`

const schemaEntries = `
CREATE TABLE IF NOT EXISTS entries (
	session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
	id          TEXT NOT NULL,
	parent_id   TEXT NOT NULL DEFAULT '',
	timestamp   TEXT NOT NULL,
	message     TEXT NOT NULL,
	seq         INTEGER NOT NULL,
	PRIMARY KEY (session_id, id)
)`

const schemaEntriesIndex = `CREATE INDEX IF NOT EXISTS entries_session_seq ON entries(session_id, seq)`

const (
	insertSessionSQL = `INSERT INTO sessions
		(id, version, created_at, updated_at, model, provider, system_prompt, parent_session, cwd, context_from, context_watermark)
		VALUES (?,?,?,?,?,?,?,?,?,?,?)
		ON CONFLICT(id) DO UPDATE SET
			version           = excluded.version,
			created_at        = excluded.created_at,
			updated_at        = excluded.updated_at,
			model             = excluded.model,
			provider          = excluded.provider,
			system_prompt     = excluded.system_prompt,
			parent_session    = excluded.parent_session,
			cwd               = excluded.cwd,
			context_from      = excluded.context_from,
			context_watermark = excluded.context_watermark`

	deleteEntriesSQL = `DELETE FROM entries WHERE session_id = ?`

	insertEntrySQL = `INSERT INTO entries (session_id, id, parent_id, timestamp, message, seq) VALUES (?,?,?,?,?,?)`

	selectHeaderSQL = `SELECT id, version, created_at, updated_at, model, provider, system_prompt, parent_session, cwd, context_from, context_watermark FROM sessions WHERE id = ?`

	selectEntriesSQL = `SELECT id, parent_id, timestamp, message FROM entries WHERE session_id = ? ORDER BY seq`

	listSessionsSQL = `SELECT id, version, created_at, updated_at, model, provider, system_prompt, parent_session, cwd, context_from, context_watermark FROM sessions ORDER BY updated_at DESC, id`

	maxSeqSQL = `SELECT COALESCE(MAX(seq), -1) FROM entries WHERE session_id = ?`

	tailEntrySQL = `SELECT id FROM entries WHERE session_id = ? ORDER BY seq DESC LIMIT 1`
)

// rowScanner is the shared scan surface of *sql.Row and *sql.Rows.
type rowScanner interface{ Scan(dest ...any) error }

// formatTime renders a timestamp in the JSONL-compatible RFC 3339 (UTC,
// nanosecond precision) form.
func formatTime(t time.Time) string { return t.UTC().Format(time.RFC3339Nano) }

// parseTime reads a stored timestamp back.
func parseTime(s string) (time.Time, error) {
	t, err := time.Parse(time.RFC3339Nano, s)
	if err != nil {
		return time.Time{}, fmt.Errorf("session/sqlite: parse timestamp %q: %w", s, err)
	}
	return t, nil
}

// headerToArgs flattens a header into the insert argument order.
func headerToArgs(h session.SessionHeader) []any {
	return []any{
		h.ID, h.Version, formatTime(h.CreatedAt), formatTime(h.UpdatedAt),
		h.Model, h.Provider, h.SystemPrompt, h.ParentSession, h.Cwd,
		h.ContextFrom, h.ContextWatermark,
	}
}

// scanHeader decodes a header row (per selectHeaderSQL's column order).
func scanHeader(sc rowScanner) (session.SessionHeader, error) {
	var h session.SessionHeader
	var createdAt, updatedAt string
	if err := sc.Scan(&h.ID, &h.Version, &createdAt, &updatedAt, &h.Model, &h.Provider,
		&h.SystemPrompt, &h.ParentSession, &h.Cwd, &h.ContextFrom, &h.ContextWatermark); err != nil {
		return session.SessionHeader{}, err
	}
	var err error
	if h.CreatedAt, err = parseTime(createdAt); err != nil {
		return session.SessionHeader{}, err
	}
	if h.UpdatedAt, err = parseTime(updatedAt); err != nil {
		return session.SessionHeader{}, err
	}
	return h, nil
}

// scanEntry decodes an entry row (per selectEntriesSQL's column order).
func scanEntry(sc rowScanner) (session.Entry, error) {
	var e session.Entry
	var timestamp, message string
	if err := sc.Scan(&e.ID, &e.ParentID, &timestamp, &message); err != nil {
		return session.Entry{}, err
	}
	var err error
	if e.Timestamp, err = parseTime(timestamp); err != nil {
		return session.Entry{}, err
	}
	if e.Message, err = decodeMessage([]byte(message)); err != nil {
		return session.Entry{}, err
	}
	return e, nil
}

// decodeMessage decodes the message column with agentcore.MessageList's
// role-discriminated decoder — the same logic session.Entry.UnmarshalJSON
// applies to entryWire.message, so both backends decode identical bytes to
// identical values.
func decodeMessage(raw []byte) (agentcore.Message, error) {
	var one agentcore.MessageList
	if err := json.Unmarshal([]byte("["+string(raw)+"]"), &one); err != nil {
		return nil, fmt.Errorf("session/sqlite: decode message: %w", err)
	}
	if len(one) != 1 {
		return nil, fmt.Errorf("session/sqlite: message decoded to %d values, want 1", len(one))
	}
	return one[0], nil
}

// saveSessionTx upserts the session row.
func saveSessionTx(tx *sql.Tx, h session.SessionHeader) error {
	if _, err := tx.Exec(insertSessionSQL, headerToArgs(h)...); err != nil {
		return fmt.Errorf("session/sqlite: upsert session %s: %w", h.ID, err)
	}
	return nil
}

// saveEntriesTx replaces a whole session: upsert the header, drop the old
// entries, insert the given ones with seq 0..n-1. It is the transactional
// core of Save and SaveEntries.
func saveEntriesTx(tx *sql.Tx, h session.SessionHeader, entries []session.Entry) error {
	if err := saveSessionTx(tx, h); err != nil {
		return err
	}
	if _, err := tx.Exec(deleteEntriesSQL, h.ID); err != nil {
		return fmt.Errorf("session/sqlite: clear entries %s: %w", h.ID, err)
	}
	for i, e := range entries {
		if err := insertEntryTx(tx, h.ID, e, i); err != nil {
			return err
		}
	}
	return nil
}

// insertEntryTx writes one entry row at the given sequence position.
func insertEntryTx(tx *sql.Tx, sessionID string, e session.Entry, seq int) error {
	mb, err := json.Marshal(e.Message)
	if err != nil {
		return fmt.Errorf("session/sqlite: encode message %s/%s: %w", sessionID, e.ID, err)
	}
	if _, err := tx.Exec(insertEntrySQL, sessionID, e.ID, e.ParentID, formatTime(e.Timestamp), string(mb), seq); err != nil {
		return fmt.Errorf("session/sqlite: insert entry %s/%s: %w", sessionID, e.ID, err)
	}
	return nil
}

// loadHeaderTx reads one session's header. found=false means the session
// does not exist (no error).
func loadHeaderTx(tx *sql.Tx, id string) (session.SessionHeader, bool, error) {
	h, err := scanHeader(tx.QueryRow(selectHeaderSQL, id))
	if err == sql.ErrNoRows {
		return session.SessionHeader{}, false, nil
	}
	if err != nil {
		return session.SessionHeader{}, false, fmt.Errorf("session/sqlite: load header %s: %w", id, err)
	}
	return h, true, nil
}

// tailParentTx returns the id of the session's highest-seq entry, or "" for
// an empty session — the parent a linear Append chains onto.
func tailParentTx(tx *sql.Tx, id string) (string, error) {
	var parent string
	err := tx.QueryRow(tailEntrySQL, id).Scan(&parent)
	if err == sql.ErrNoRows {
		return "", nil
	}
	if err != nil {
		return "", fmt.Errorf("session/sqlite: tail entry %s: %w", id, err)
	}
	return parent, nil
}

// appendBranchTx chains the messages onto parentLeafID at the session's
// current sequence tail, upserting the header (UpdatedAt as the caller set
// it) and returning the new leaf id.
func appendBranchTx(tx *sql.Tx, header session.SessionHeader, parentLeafID string, messages agentcore.MessageList) (string, error) {
	if err := saveSessionTx(tx, header); err != nil {
		return "", err
	}
	var maxSeq int
	if err := tx.QueryRow(maxSeqSQL, header.ID).Scan(&maxSeq); err != nil {
		return "", fmt.Errorf("session/sqlite: max seq %s: %w", header.ID, err)
	}
	now := time.Now().UTC()
	parent := parentLeafID
	leaf := parentLeafID
	seq := maxSeq + 1
	for _, m := range messages {
		e := session.Entry{ID: newEntryID(), ParentID: parent, Timestamp: now, Message: m}
		if err := insertEntryTx(tx, header.ID, e, seq); err != nil {
			return "", err
		}
		parent = e.ID
		leaf = e.ID
		seq++
	}
	return leaf, nil
}

// newEntryID mints a fresh 8-hex-character entry id — the same style as the
// session package's private generator (duplicated rather than exported:
// the id format is an internal convention of the storage layer, not part of
// any package's public surface).
func newEntryID() string {
	var b [4]byte
	if _, err := rand.Read(b[:]); err != nil {
		return fmt.Sprintf("%08x", time.Now().UnixNano()&0xffffffff)
	}
	return hex.EncodeToString(b[:])
}
