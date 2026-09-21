// The Store contract: the persistence surface every session backend
// satisfies. The JSONL file store (NewStore) is the original implementation
// and remains the default; internal/session/sqlite adds an authoritative
// normalized-table backend. Consumers hold the interface, so switching
// backends is a construction-site change, not a call-site change — the
// method set below is exactly what repl/tui/headless/serve/dream already use.
//
// The contract deliberately speaks pigo's own types (SessionHeader, Entry,
// agentcore.MessageList): the tree-shaped session model crosses the boundary
// intact, and each backend decides its physical encoding (JSONL lines,
// normalized tables). The dependency direction stays inward — storage defines
// the contract, callers adapt to it.
package session

import (
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
)

// Store persists sessions. Every method is safe for concurrent use.
type Store interface {
	// Dir returns the backend's root directory (the directory passed to its
	// constructor).
	Dir() string

	// Save writes header and messages to a fresh session file, overwriting
	// any existing session with the same id. The header's Version is forced
	// to SchemaVersion. Save is the whole-session write; Append adds messages
	// to an existing session.
	Save(header SessionHeader, messages agentcore.MessageList) error

	// SaveEntries writes header plus the given entries verbatim — preserving
	// each entry's id/parentId — to a fresh session, overwriting any existing
	// one for header.ID. It persists an already-known tree (what Fork needs).
	SaveEntries(header SessionHeader, entries []Entry) error

	// Load reads a session and returns its header and messages in file
	// order. It is the linear view; LoadEntries exposes the tree metadata.
	Load(id string) (SessionHeader, agentcore.MessageList, error)

	// LoadEntries reads a session and returns its header plus the tree
	// entries (id/parentId + message) in file order.
	LoadEntries(id string) (SessionHeader, []Entry, error)

	// List returns the headers of all sessions, most recently updated first.
	List() ([]SessionHeader, error)

	// Append adds messages to an existing session and bumps its UpdatedAt.
	// If the session does not exist it is an error — use Save to create one.
	Append(id string, updatedAt time.Time, messages agentcore.MessageList) error

	// AppendBranch appends messages as a chain descending from
	// parentLeafID, preserving every existing entry (and therefore any other
	// branches). It returns the id of the new leaf. If the session does not
	// yet exist it is created.
	AppendBranch(header SessionHeader, parentLeafID string, messages agentcore.MessageList) (string, error)

	// Fork creates a new session holding the linear path from the source's
	// root down to leafID, copied verbatim, with ParentSession recorded.
	Fork(sourceID, leafID string, now time.Time) (SessionHeader, []Entry, error)

	// Export writes the session as a portable JSONL file to outPath and
	// returns the entry count.
	Export(id, outPath string) (int, error)

	// Import reads a portable JSONL file into a fresh session and returns
	// its header and entries.
	Import(inPath string, now time.Time) (SessionHeader, []Entry, error)

	// ExportShare writes the session in a share-friendly format (markdown /
	// html) to outPath and returns the entry count.
	ExportShare(id, outPath, format string, opts ShareOptions) (int, error)

	// ReleaseSession drops this store's write lease on id, if the backend
	// maintains one (the JSONL backend does; a database-backed store is its
	// own arbiter and makes this a no-op). Idempotent.
	ReleaseSession(id string) error
}
