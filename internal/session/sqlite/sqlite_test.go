// The SQLite backend's test battery (plan phase 3b): the shared conformance
// suite, migration idempotence, the bidirectional JSONL transfer round trip,
// and the same-owner concurrent-append serialization the JSONL backend
// proves via its write-transaction mutex — here via the single pooled
// connection.
package sqlite

import (
	"fmt"
	"path/filepath"
	"sync"
	"testing"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/session"
	"github.com/smallnest/pigo/internal/session/storetest"
)

func openTestStore(t *testing.T) *Store {
	t.Helper()
	s, err := Open(filepath.Join(t.TempDir(), "sessions.db"))
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	t.Cleanup(func() { _ = s.Close() })
	return s
}

func TestConformance(t *testing.T) {
	storetest.RunConformance(t, func(t *testing.T) session.Store {
		t.Helper()
		return openTestStore(t)
	})
}

// TestOpenIsMigrationIdempotent reopens an existing database: the second
// Open must find the schema at version and apply nothing.
func TestOpenIsMigrationIdempotent(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sessions.db")
	s1, err := Open(path)
	if err != nil {
		t.Fatalf("Open 1: %v", err)
	}
	if err := s1.Save(session.SessionHeader{ID: "keep", CreatedAt: time.Now(), UpdatedAt: time.Now()},
		agentcore.MessageList{agentcore.UserMessage{RoleField: agentcore.RoleUser}}); err != nil {
		t.Fatalf("Save: %v", err)
	}
	if err := s1.Close(); err != nil {
		t.Fatalf("Close 1: %v", err)
	}

	s2, err := Open(path)
	if err != nil {
		t.Fatalf("Open 2 (migrations must be idempotent): %v", err)
	}
	defer s2.Close()
	if _, entries, err := s2.LoadEntries("keep"); err != nil || len(entries) != 1 {
		t.Fatalf("after reopen: entries=%d err=%v, want 1/nil", len(entries), err)
	}
}

// TestTransferRoundTripJSONLSQLite is the phase-3b bidirectional migration
// acceptance: a branched session history moves JSONL → SQLite → JSONL with
// every entry (ids, parentIds, timestamps, messages) intact.
func TestTransferRoundTripJSONLSQLite(t *testing.T) {
	jsonl1, err := session.NewStore(t.TempDir())
	if err != nil {
		t.Fatalf("NewStore 1: %v", err)
	}
	ts := time.Date(2026, 5, 1, 12, 0, 0, 0, time.UTC)

	// Two sessions; one carries a real tree (root plus two sibling branches).
	hA := session.SessionHeader{ID: "alpha", CreatedAt: ts, UpdatedAt: ts, Model: "m1", Cwd: "/a"}
	if _, err := jsonl1.AppendBranch(hA, "", agentcore.MessageList{userMsg("root")}); err != nil {
		t.Fatalf("root: %v", err)
	}
	_, entriesA, _ := jsonl1.LoadEntries("alpha")
	root := entriesA[0].ID
	if _, err := jsonl1.AppendBranch(hA, root, agentcore.MessageList{userMsg("left"), assistantMsg("leaf L")}); err != nil {
		t.Fatalf("left: %v", err)
	}
	if _, err := jsonl1.AppendBranch(hA, root, agentcore.MessageList{userMsg("right")}); err != nil {
		t.Fatalf("right: %v", err)
	}
	hB := session.SessionHeader{ID: "beta", CreatedAt: ts, UpdatedAt: ts.Add(time.Hour), Model: "m2"}
	if _, err := jsonl1.AppendBranch(hB, "", agentcore.MessageList{userMsg("only")}); err != nil {
		t.Fatalf("beta: %v", err)
	}
	_, wantA, err := jsonl1.LoadEntries("alpha")
	if err != nil {
		t.Fatalf("load alpha: %v", err)
	}

	db := openTestStore(t)
	n, err := db.ImportJSONL(jsonl1)
	if err != nil || n != 2 {
		t.Fatalf("ImportJSONL = %d, %v; want 2, nil", n, err)
	}

	// The tree survived the hop into the database.
	_, gotA, err := db.LoadEntries("alpha")
	if err != nil {
		t.Fatalf("sqlite LoadEntries: %v", err)
	}
	if len(gotA) != len(wantA) {
		t.Fatalf("alpha entries = %d, want %d", len(gotA), len(wantA))
	}
	for i := range wantA {
		if gotA[i].ID != wantA[i].ID || gotA[i].ParentID != wantA[i].ParentID ||
			!gotA[i].Timestamp.Equal(wantA[i].Timestamp) {
			t.Fatalf("alpha entry[%d] = %+v, want %+v", i, gotA[i], wantA[i])
		}
	}

	// And back out into a fresh JSONL store.
	jsonl2, err := session.NewStore(t.TempDir())
	if err != nil {
		t.Fatalf("NewStore 2: %v", err)
	}
	n, err = db.ExportJSONL(jsonl2)
	if err != nil || n != 2 {
		t.Fatalf("ExportJSONL = %d, %v; want 2, nil", n, err)
	}
	_, backA, err := jsonl2.LoadEntries("alpha")
	if err != nil {
		t.Fatalf("exported LoadEntries: %v", err)
	}
	if len(backA) != len(wantA) {
		t.Fatalf("exported alpha entries = %d, want %d", len(backA), len(wantA))
	}
	for i := range wantA {
		if backA[i].ID != wantA[i].ID || backA[i].ParentID != wantA[i].ParentID {
			t.Fatalf("exported alpha entry[%d] = %+v, want %+v", i, backA[i], wantA[i])
		}
	}
}

// TestConcurrentAppendBranchSerialized mirrors the JSONL backend's
// phase-3a case: one store, many goroutines appending branches to the same
// session — every batch lands.
func TestConcurrentAppendBranchSerialized(t *testing.T) {
	s := openTestStore(t)
	header := session.SessionHeader{ID: "tree", CreatedAt: time.Now(), UpdatedAt: time.Now()}
	if _, err := s.AppendBranch(header, "", agentcore.MessageList{userMsg("root")}); err != nil {
		t.Fatalf("seed: %v", err)
	}

	const workers, msgs = 8, 10
	var wg sync.WaitGroup
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func(w int) {
			defer wg.Done()
			batch := make(agentcore.MessageList, msgs)
			for i := range batch {
				batch[i] = userMsg(fmt.Sprintf("w%d-%d", w, i))
			}
			if _, err := s.AppendBranch(header, "", batch); err != nil {
				t.Errorf("branch append w%d: %v", w, err)
			}
		}(w)
	}
	done := make(chan struct{})
	go func() { wg.Wait(); close(done) }()
	select {
	case <-done:
	case <-time.After(30 * time.Second):
		t.Fatal("concurrent branch appends deadlocked")
	}

	_, entries, err := s.LoadEntries("tree")
	if err != nil {
		t.Fatalf("LoadEntries: %v", err)
	}
	if want := 1 + workers*msgs; len(entries) != want {
		t.Fatalf("entries = %d, want %d (batches lost to unsynchronized writes)", len(entries), want)
	}
}

func userMsg(text string) agentcore.UserMessage {
	return agentcore.UserMessage{
		RoleField: agentcore.RoleUser,
		Content:   agentcore.ContentList{agentcore.NewTextContent(text)},
	}
}

func assistantMsg(text string) agentcore.AssistantMessage {
	return agentcore.AssistantMessage{
		RoleField:  agentcore.RoleAssistant,
		Content:    agentcore.ContentList{agentcore.NewTextContent(text)},
		StopReason: agentcore.StopReasonEndTurn,
	}
}
