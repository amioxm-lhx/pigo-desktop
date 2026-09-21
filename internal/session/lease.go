// Session write leases (plan phases 2b + 3a): the concurrency precondition
// for multi-frontend access. Three cooperating layers keep session writes
// from interleaving —
//
//   - an in-process lease registry (exclusive/shared semantics over a
//     map + mutex): two Store instances in one process (the CLI and the
//     remotecontrol server, say) cannot silently overwrite each other's
//     writes anymore;
//   - a cross-process advisory lock on a sidecar "<id>.jsonl.lock" file
//     (flock where available): a second pigo process on the same machine
//     gets a clear error instead of interleaving JSONL writes;
//   - a per-session write-transaction mutex (phase 3a): the lease says WHO
//     may write, the transaction mutex serializes the write sections of
//     concurrent goroutines holding that same lease — the read-modify-write
//     behind Append/AppendBranch runs atomically, so concurrent appends
//     queue up instead of losing each other's updates.
//
// The lease is acquired lazily on the first write of a session and held
// until ReleaseSession (or process exit — the kernel drops flock when the
// fd closes, and the in-process registry dies with the process). Reads
// never take a lease: atomicWrite's tmp+rename already guarantees a reader
// sees either the old or the new file, never a torn one.
//
// This is arbitration, not sandboxing: a lease holder can still write
// garbage — the guarantee is exactly one writer at a time.
package session

import (
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"
)

// ErrLeaseHeld is returned when a write cannot proceed because another
// owner holds the session's write lease (in-process or cross-process). The
// message names the holder when known so the losing side can surface
// actionable context.
var ErrLeaseHeld = errors.New("session: write lease held by another owner")

// leaseManager arbitrates per-key leases within one process. Keys are
// store-dir + session-id, so two stores on different directories never
// collide. Exclusive and shared follow the usual rules: at most one
// exclusive holder, any number of shared holders, exclusive excludes all.
// Re-acquisition by the current holder is a no-op success.
type leaseManager struct {
	mu       sync.Mutex
	sessions map[string]*leaseState
}

type leaseState struct {
	exclusive string // owner token of the exclusive holder, "" if none
	shared    map[string]bool
}

func newLeaseManager() *leaseManager {
	return &leaseManager{sessions: make(map[string]*leaseState)}
}

// acquire takes (or keeps) a lease on key for owner. exclusive=true wants
// the sole write right; false wants one of the shared slots.
func (m *leaseManager) acquire(key, owner string, exclusive bool) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	st := m.sessions[key]
	if st == nil {
		st = &leaseState{shared: make(map[string]bool)}
		m.sessions[key] = st
	}
	if st.exclusive != "" {
		if st.exclusive == owner {
			return nil // already the exclusive holder
		}
		return fmt.Errorf("%w: %s", ErrLeaseHeld, st.exclusive)
	}
	if exclusive {
		if len(st.shared) == 0 {
			st.exclusive = owner
			return nil
		}
		if len(st.shared) == 1 && st.shared[owner] {
			// The only shared holder upgrades itself to exclusive.
			delete(st.shared, owner)
			st.exclusive = owner
			return nil
		}
		return fmt.Errorf("%w: %d shared holder(s)", ErrLeaseHeld, len(st.shared))
	}
	st.shared[owner] = true
	return nil
}

// release drops owner's hold on key (whichever kind it held).
func (m *leaseManager) release(key, owner string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	st := m.sessions[key]
	if st == nil {
		return
	}
	if st.exclusive == owner {
		st.exclusive = ""
	}
	delete(st.shared, owner)
	if st.exclusive == "" && len(st.shared) == 0 {
		delete(m.sessions, key)
	}
}

// processLeases is the process-wide registry shared by every Store the
// process builds — the in-process half of the two-layer lease.
var processLeases = newLeaseManager()

// leaseKey scopes a lease to this store's directory + session id, so the
// same session id under different roots is two unrelated leases.
func (s *jsonlStore) leaseKey(id string) string { return s.dir + "\x00" + id }

// claimWrite acquires both lease layers for a write to id: the in-process
// exclusive lease first (cheap, exact), then the cross-process flock. On
// flock failure the in-process lease is rolled back so a failed claim
// leaves no residue. Idempotent per (store, session): already-held claims
// are free.
func (s *jsonlStore) claimWrite(id string) error {
	key := s.leaseKey(id)
	if err := processLeases.acquire(key, s.owner, true); err != nil {
		return err
	}
	if err := s.flockSession(id); err != nil {
		processLeases.release(key, s.owner)
		return err
	}
	return nil
}

// lockWriteTxn holds id's write-transaction mutex and returns its unlock
// func. It is the phase-3a refinement of the lease: claimWrite arbitrates
// ownership (fast failure for non-holders), this serializes the write
// sections of same-owner goroutines — concurrent Append/AppendBranch calls
// queue here instead of racing their load-modify-save cycles against each
// other and losing whole batches.
//
// The per-session mutexes stay registered for the store's lifetime: a
// waiter may still reference one after ReleaseSession, and removing the
// entry then would hand a concurrent caller a fresh mutex (and a broken
// guarantee). A few dozen bytes per session id is a fine price.
func (s *jsonlStore) lockWriteTxn(id string) func() {
	s.txnMu.Lock()
	m := s.txns[id]
	if m == nil {
		m = &sync.Mutex{}
		s.txns[id] = m
	}
	s.txnMu.Unlock()
	m.Lock()
	return m.Unlock
}

// flockSession takes the cross-process advisory lock on the sidecar lock
// file, keeping the fd open for the lease's lifetime (the kernel releases
// flock when it closes). Already-held locks are free.
func (s *jsonlStore) flockSession(id string) error {
	s.lockMu.Lock()
	defer s.lockMu.Unlock()
	if _, ok := s.lockFiles[id]; ok {
		return nil
	}
	path := filepath.Join(s.dir, FileName(id)+".lock")
	f, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0o644)
	if err != nil {
		return fmt.Errorf("session: open lock file %s: %w", path, err)
	}
	if err := tryFlock(f); err != nil {
		f.Close()
		return fmt.Errorf("%w: cross-process lock on %s: %w", ErrLeaseHeld, FileName(id), err)
	}
	s.lockFiles[id] = f
	return nil
}

// ReleaseSession drops this store's write lease on id (both layers). After
// it returns, another owner — in this process or another — can claim the
// session. Idempotent; unknown ids are ignored. The store keeps working.
func (s *jsonlStore) ReleaseSession(id string) error {
	s.lockMu.Lock()
	f, ok := s.lockFiles[id]
	if ok {
		delete(s.lockFiles, id)
	}
	s.lockMu.Unlock()
	if ok {
		if err := f.Close(); err != nil { // closes fd → kernel drops flock
			return fmt.Errorf("session: release lock for %s: %w", id, err)
		}
	}
	processLeases.release(s.leaseKey(id), s.owner)
	return nil
}

// newOwnerID mints a fresh owner token for a Store instance, in the same
// 8-hex style as entry ids.
func newOwnerID() string {
	var b [4]byte
	if _, err := rand.Read(b[:]); err != nil {
		// crypto/rand.Read never fails in practice; fall back to a
		// timestamp-derived token like newEntryID does.
		return fmt.Sprintf("%08x", time.Now().UnixNano()&0xffffffff)
	}
	return hex.EncodeToString(b[:])
}
