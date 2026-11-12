package im

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestBindingStoreRoundTrip(t *testing.T) {
	path := filepath.Join(t.TempDir(), "im", "bindings.json")
	store, err := NewBindingStore(path)
	if err != nil {
		t.Fatalf("NewBindingStore: %v", err)
	}
	now := time.Date(2026, 11, 12, 9, 0, 0, 0, time.UTC)
	if err := store.Put("p2p:c1", Binding{SessionID: "s1", Cwd: "/tmp/p", CreatedAt: now, LastActive: now}); err != nil {
		t.Fatalf("Put: %v", err)
	}

	// A second store over the same file must see the binding: it is the whole
	// point of persisting them.
	reloaded, err := NewBindingStore(path)
	if err != nil {
		t.Fatalf("reload: %v", err)
	}
	got, ok := reloaded.Get("p2p:c1")
	if !ok {
		t.Fatal("binding missing after reload")
	}
	if got.SessionID != "s1" || got.Cwd != "/tmp/p" {
		t.Fatalf("binding = %+v, want session s1 in /tmp/p", got)
	}
}

func TestBindingStoreDeleteAndKeyOrder(t *testing.T) {
	store, err := NewBindingStore("")
	if err != nil {
		t.Fatalf("NewBindingStore: %v", err)
	}
	base := time.Date(2026, 11, 12, 9, 0, 0, 0, time.UTC)
	store.Put("p2p:old", Binding{SessionID: "s-old", LastActive: base})
	store.Put("p2p:new", Binding{SessionID: "s-new", LastActive: base.Add(time.Hour)})

	keys := store.Keys()
	if len(keys) != 2 || keys[0] != "p2p:new" {
		t.Fatalf("Keys() = %v, want newest first", keys)
	}
	if err := store.Delete("p2p:old"); err != nil {
		t.Fatalf("Delete: %v", err)
	}
	if _, ok := store.Get("p2p:old"); ok {
		t.Fatal("binding still present after Delete")
	}
	// Deleting an absent key is a no-op, not an error: /new on a fresh chat
	// must not fail.
	if err := store.Delete("p2p:never"); err != nil {
		t.Fatalf("Delete absent: %v", err)
	}
}

func TestBindingStoreMalformedFileIsAnError(t *testing.T) {
	path := filepath.Join(t.TempDir(), "bindings.json")
	if err := os.WriteFile(path, []byte("{not json"), 0o600); err != nil {
		t.Fatalf("seed: %v", err)
	}
	if _, err := NewBindingStore(path); err == nil {
		t.Fatal("expected an error for malformed bindings, got nil")
	}
}

func TestBindingStoreMissingFileIsEmpty(t *testing.T) {
	store, err := NewBindingStore(filepath.Join(t.TempDir(), "nope", "bindings.json"))
	if err != nil {
		t.Fatalf("NewBindingStore: %v", err)
	}
	if len(store.List()) != 0 {
		t.Fatalf("expected empty store, got %v", store.List())
	}
}

func TestBindingStoreTouchPreservesSession(t *testing.T) {
	store, _ := NewBindingStore("")
	created := time.Date(2026, 11, 12, 9, 0, 0, 0, time.UTC)
	store.Put("p2p:c1", Binding{SessionID: "s1", Model: "m", CreatedAt: created, LastActive: created})

	later := created.Add(2 * time.Hour)
	if err := store.Touch("p2p:c1", later); err != nil {
		t.Fatalf("Touch: %v", err)
	}
	got, _ := store.Get("p2p:c1")
	if !got.LastActive.Equal(later) {
		t.Fatalf("LastActive = %v, want %v", got.LastActive, later)
	}
	if got.SessionID != "s1" || got.Model != "m" {
		t.Fatalf("Touch clobbered the binding: %+v", got)
	}
}
