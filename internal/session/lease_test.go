// Tests for the write-lease layer (plan phases 2b + 3a): the in-process
// exclusive/shared semantics, the cross-process flock conflict, the phase-2b
// acceptance case — two owners hammering the same session concurrently see
// clear errors instead of silently interleaving JSONL writes — and the
// phase-3a acceptance case: same-owner concurrent appends serialize inside
// one write transaction each, so every batch lands.
package session

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
)

func TestLeaseManagerSemantics(t *testing.T) {
	m := newLeaseManager()

	// Exclusive excludes another exclusive.
	if err := m.acquire("k", "a", true); err != nil {
		t.Fatalf("a exclusive: %v", err)
	}
	if err := m.acquire("k", "b", true); !errors.Is(err, ErrLeaseHeld) {
		t.Fatalf("b exclusive vs a = %v, want ErrLeaseHeld", err)
	}
	// Exclusive excludes shared.
	if err := m.acquire("k", "b", false); !errors.Is(err, ErrLeaseHeld) {
		t.Fatalf("b shared vs a-exclusive = %v, want ErrLeaseHeld", err)
	}
	// Holder re-acquisition is free.
	if err := m.acquire("k", "a", true); err != nil {
		t.Fatalf("a re-acquire: %v", err)
	}

	m.release("k", "a")

	// Shared holders coexist.
	if err := m.acquire("k", "b", false); err != nil {
		t.Fatalf("b shared: %v", err)
	}
	if err := m.acquire("k", "c", false); err != nil {
		t.Fatalf("c shared: %v", err)
	}
	// But exclude a new exclusive.
	if err := m.acquire("k", "a", true); !errors.Is(err, ErrLeaseHeld) {
		t.Fatalf("a exclusive vs shared = %v, want ErrLeaseHeld", err)
	}

	// The sole shared holder upgrades itself.
	m.release("k", "c")
	if err := m.acquire("k", "b", true); err != nil {
		t.Fatalf("b upgrade to exclusive: %v", err)
	}

	// Full release empties the registry entry (re-acquire works).
	m.release("k", "b")
	if err := m.acquire("k", "a", true); err != nil {
		t.Fatalf("a after full release: %v", err)
	}

	// Unrelated keys never collide.
	if err := m.acquire("other", "a", true); err != nil {
		t.Fatalf("other key: %v", err)
	}
}

func TestStoreWriteLeaseExclusiveInProcess(t *testing.T) {
	dir := t.TempDir()
	store1, err := NewStore(dir)
	if err != nil {
		t.Fatalf("NewStore1: %v", err)
	}
	store2, err := NewStore(dir)
	if err != nil {
		t.Fatalf("NewStore2: %v", err)
	}
	t.Cleanup(func() { _ = store1.ReleaseSession("s1"); _ = store2.ReleaseSession("s1") })

	header := SessionHeader{ID: "s1", CreatedAt: time.Now(), UpdatedAt: time.Now()}
	msgs := agentcore.MessageList{agentcore.UserMessage{RoleField: agentcore.RoleUser}}

	if err := store1.Save(header, msgs); err != nil {
		t.Fatalf("store1 Save: %v", err)
	}
	// The second owner — same process, same dir — is rejected with the
	// sentinel error, not a silent overwrite.
	err = store2.Save(header, msgs)
	if !errors.Is(err, ErrLeaseHeld) {
		t.Fatalf("store2 Save = %v, want ErrLeaseHeld", err)
	}
	if err := store2.Append("s1", time.Now(), msgs); !errors.Is(err, ErrLeaseHeld) {
		t.Fatalf("store2 Append = %v, want ErrLeaseHeld", err)
	}

	// Reads stay open to everyone.
	if _, _, err := store2.Load("s1"); err != nil {
		t.Fatalf("store2 Load on leased session: %v", err)
	}

	// After the holder releases, the other owner can take over.
	if err := store1.ReleaseSession("s1"); err != nil {
		t.Fatalf("ReleaseSession: %v", err)
	}
	if err := store2.Save(header, msgs); err != nil {
		t.Fatalf("store2 Save after release: %v", err)
	}
}

func TestTryFlockExcludesSecondDescriptor(t *testing.T) {
	// flock is per open-file-description: two fds in one process conflict
	// exactly like two processes do, so this tests the cross-process layer
	// without forking.
	path := filepath.Join(t.TempDir(), "x.lock")
	f1, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0o644)
	if err != nil {
		t.Fatalf("open1: %v", err)
	}
	defer f1.Close()
	if err := tryFlock(f1); err != nil {
		t.Fatalf("first flock: %v", err)
	}

	f2, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0o644)
	if err != nil {
		t.Fatalf("open2: %v", err)
	}
	defer f2.Close()
	if err := tryFlock(f2); err == nil {
		t.Fatal("second flock on the same file must fail")
	}

	// Closing the first fd drops the lock; a new take succeeds.
	f1.Close()
	if err := tryFlock(f2); err != nil {
		t.Fatalf("flock after first holder closed: %v", err)
	}
}

// TestConcurrentDoubleWrite is the phase-2b acceptance case: two owners
// (two Store instances, simulating the CLI and the phone remote) write the
// same session concurrently. Exactly one wins; the loser gets ErrLeaseHeld
// every time; the file is never torn.
func TestConcurrentDoubleWrite(t *testing.T) {
	dir := t.TempDir()
	store1, err := NewStore(dir)
	if err != nil {
		t.Fatalf("NewStore1: %v", err)
	}
	store2, err := NewStore(dir)
	if err != nil {
		t.Fatalf("NewStore2: %v", err)
	}
	t.Cleanup(func() { _ = store1.ReleaseSession("race"); _ = store2.ReleaseSession("race") })

	header := SessionHeader{ID: "race", CreatedAt: time.Now(), UpdatedAt: time.Now()}
	msgs := agentcore.MessageList{agentcore.UserMessage{RoleField: agentcore.RoleUser, Content: agentcore.ContentList{agentcore.NewTextContent("hi")}}}

	// Seed the session so both sides append (the overwrite-heavy path).
	if err := store1.Save(header, msgs); err != nil {
		t.Fatalf("seed Save: %v", err)
	}

	const rounds = 50
	var rejected int64
	var wg sync.WaitGroup
	write := func(s Store, tag string) {
		defer wg.Done()
		for i := 0; i < rounds; i++ {
			msg := agentcore.UserMessage{
				RoleField: agentcore.RoleUser,
				Content:   agentcore.ContentList{agentcore.NewTextContent(fmt.Sprintf("%s-%d", tag, i))},
			}
			if _, err := s.AppendBranch(header, "", agentcore.MessageList{msg}); err != nil {
				if errors.Is(err, ErrLeaseHeld) {
					atomic.AddInt64(&rejected, 1)
					continue
				}
				t.Errorf("unexpected error from %s: %v", tag, err)
				return
			}
		}
	}
	wg.Add(2)
	go write(store1, "cli")
	go write(store2, "phone")
	wg.Wait()

	if rejected == 0 {
		t.Fatal("the losing owner saw no lease rejections — the lease layer is not arbitrating")
	}

	// The file is intact: it loads, and every line decodes.
	_, entries, err := store1.LoadEntries("race")
	if err != nil {
		t.Fatalf("final LoadEntries after concurrent writes: %v", err)
	}
	// Seed message + at least the winner's writes.
	if len(entries) < 2 {
		t.Fatalf("entries = %d, want >= 2", len(entries))
	}
}

// waitGroupDone returns a channel that closes when wg completes, so callers
// can select against a deadline and turn a deadlock into a fast failure.
func waitGroupDone(wg *sync.WaitGroup) <-chan struct{} {
	done := make(chan struct{})
	go func() {
		wg.Wait()
		close(done)
	}()
	return done
}

// TestConcurrentAppendSerialized is the phase-3a acceptance case: one owner
// (one Store — e.g. the REPL goroutine and the remotecontrol bridge sharing
// a process) fires concurrent Appends at the same session. The write lease
// lets them all through (same owner); the per-session write-transaction
// mutex serializes each load-modify-save, so every batch lands. Without it,
// two Appends reading the same "old" file would each write their own version
// and whole batches would silently vanish.
func TestConcurrentAppendSerialized(t *testing.T) {
	store, err := NewStore(t.TempDir())
	if err != nil {
		t.Fatalf("NewStore: %v", err)
	}
	header := SessionHeader{ID: "serial", CreatedAt: time.Now(), UpdatedAt: time.Now()}
	seed := agentcore.MessageList{agentcore.UserMessage{
		RoleField: agentcore.RoleUser,
		Content:   agentcore.ContentList{agentcore.NewTextContent("seed")},
	}}
	if err := store.Save(header, seed); err != nil {
		t.Fatalf("seed Save: %v", err)
	}

	const workers, batches = 8, 10
	var wg sync.WaitGroup
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func(w int) {
			defer wg.Done()
			for i := 0; i < batches; i++ {
				msg := agentcore.UserMessage{
					RoleField: agentcore.RoleUser,
					Content:   agentcore.ContentList{agentcore.NewTextContent(fmt.Sprintf("w%d-%d", w, i))},
				}
				if err := store.Append("serial", time.Now(), agentcore.MessageList{msg}); err != nil {
					t.Errorf("append w%d-%d: %v", w, i, err)
					return
				}
			}
		}(w)
	}
	select {
	case <-waitGroupDone(&wg):
	case <-time.After(30 * time.Second):
		t.Fatal("concurrent appends deadlocked — transaction lock re-entered?")
	}

	// Every batch landed: seed + workers×batches entries, all decodable.
	_, entries, err := store.LoadEntries("serial")
	if err != nil {
		t.Fatalf("final LoadEntries: %v", err)
	}
	if want := 1 + workers*batches; len(entries) != want {
		t.Fatalf("entries = %d, want %d (batches lost to unsynchronized writes)", len(entries), want)
	}
	seen := make(map[string]bool, len(entries))
	for _, e := range entries {
		if seen[e.ID] {
			t.Fatalf("duplicate entry id %s — writes interleaved", e.ID)
		}
		seen[e.ID] = true
	}
}

// TestConcurrentAppendBranchSerialized is the tree-shaped counterpart: the
// same-owner concurrency case serve actually produces (one store, many
// SessionRuntimes finishing runs on the same session). Concurrent branch
// appends serialize, so every chain lands intact.
func TestConcurrentAppendBranchSerialized(t *testing.T) {
	store, err := NewStore(t.TempDir())
	if err != nil {
		t.Fatalf("NewStore: %v", err)
	}
	header := SessionHeader{ID: "tree", CreatedAt: time.Now(), UpdatedAt: time.Now()}
	if _, err := store.AppendBranch(header, "", agentcore.MessageList{agentcore.UserMessage{
		RoleField: agentcore.RoleUser,
		Content:   agentcore.ContentList{agentcore.NewTextContent("root")},
	}}); err != nil {
		t.Fatalf("seed AppendBranch: %v", err)
	}

	const workers, msgs = 8, 10
	var wg sync.WaitGroup
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func(w int) {
			defer wg.Done()
			batch := make(agentcore.MessageList, msgs)
			for i := range batch {
				batch[i] = agentcore.UserMessage{
					RoleField: agentcore.RoleUser,
					Content:   agentcore.ContentList{agentcore.NewTextContent(fmt.Sprintf("w%d-%d", w, i))},
				}
			}
			if _, err := store.AppendBranch(header, "", batch); err != nil {
				t.Errorf("branch append w%d: %v", w, err)
			}
		}(w)
	}
	select {
	case <-waitGroupDone(&wg):
	case <-time.After(30 * time.Second):
		t.Fatal("concurrent branch appends deadlocked — transaction lock re-entered?")
	}

	_, entries, err := store.LoadEntries("tree")
	if err != nil {
		t.Fatalf("final LoadEntries: %v", err)
	}
	if want := 1 + workers*msgs; len(entries) != want {
		t.Fatalf("entries = %d, want %d (branches lost to unsynchronized writes)", len(entries), want)
	}
}
