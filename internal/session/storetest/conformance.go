// Package storetest is the shared conformance suite for session.Store
// backends (plan phase 3b): every implementation — the JSONL file store, the
// SQLite normalized store — runs the same suite, so the contract means the
// same thing everywhere. The cases cover exactly the semantics the frontends
// rely on (serve, headless, repl, webhook), including the os.ErrNotExist
// signal AppendBranch's create-fresh path depends on.
package storetest

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/session"
)

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

// RunConformance runs the full backend suite. newStore must return a fresh,
// empty store over disposable storage.
func RunConformance(t *testing.T, newStore func(t *testing.T) session.Store) {
	t.Helper()

	t.Run("SaveLoadRoundTrip", func(t *testing.T) {
		s := newStore(t)
		created, updated := time.Date(2026, 1, 2, 3, 4, 5, 678900000, time.UTC), time.Date(2026, 2, 3, 4, 5, 6, 789000000, time.UTC)
		header := session.SessionHeader{
			Version: session.SchemaVersion, ID: "rt", CreatedAt: created, UpdatedAt: updated,
			Model: "m", Provider: "p", SystemPrompt: "sys", Cwd: "/w", ParentSession: "orig",
		}
		msgs := agentcore.MessageList{userMsg("hello"), assistantMsg("hi"), userMsg("bye")}
		if err := s.Save(header, msgs); err != nil {
			t.Fatalf("Save: %v", err)
		}

		h, loaded, err := s.Load("rt")
		if err != nil {
			t.Fatalf("Load: %v", err)
		}
		if h.Version != session.SchemaVersion || h.Model != "m" || h.Provider != "p" ||
			h.SystemPrompt != "sys" || h.Cwd != "/w" || h.ParentSession != "orig" {
			t.Fatalf("header round trip = %+v", h)
		}
		if !h.CreatedAt.Equal(created) || !h.UpdatedAt.Equal(updated) {
			t.Fatalf("timestamps = %v/%v, want %v/%v", h.CreatedAt, h.UpdatedAt, created, updated)
		}
		if len(loaded) != 3 || agentcore.ContentToText(loaded[0].(agentcore.UserMessage).Content) != "hello" {
			t.Fatalf("messages = %+v", loaded)
		}

		_, entries, err := s.LoadEntries("rt")
		if err != nil {
			t.Fatalf("LoadEntries: %v", err)
		}
		if len(entries) != 3 {
			t.Fatalf("entries = %d, want 3", len(entries))
		}
		if entries[0].ParentID != "" || entries[1].ParentID != entries[0].ID || entries[2].ParentID != entries[1].ID {
			t.Fatalf("entry chain broken: %+v", entries)
		}
	})

	t.Run("SaveEntriesPreservesTree", func(t *testing.T) {
		s := newStore(t)
		ts := time.Date(2026, 3, 1, 0, 0, 0, 0, time.UTC)
		entries := []session.Entry{
			{ID: "a", ParentID: "", Timestamp: ts, Message: userMsg("root")},
			{ID: "b", ParentID: "a", Timestamp: ts, Message: assistantMsg("mid")},
			{ID: "c", ParentID: "a", Timestamp: ts, Message: userMsg("sibling")},
		}
		header := session.SessionHeader{ID: "tree", CreatedAt: ts, UpdatedAt: ts}
		if err := s.SaveEntries(header, entries); err != nil {
			t.Fatalf("SaveEntries: %v", err)
		}
		_, got, err := s.LoadEntries("tree")
		if err != nil {
			t.Fatalf("LoadEntries: %v", err)
		}
		if len(got) != 3 {
			t.Fatalf("entries = %d, want 3", len(got))
		}
		for i := range entries {
			if got[i].ID != entries[i].ID || got[i].ParentID != entries[i].ParentID {
				t.Fatalf("entry[%d] = %+v, want %+v (tree metadata must survive verbatim)", i, got[i], entries[i])
			}
			if !got[i].Timestamp.Equal(entries[i].Timestamp) {
				t.Fatalf("entry[%d] timestamp = %v, want %v", i, got[i].Timestamp, entries[i].Timestamp)
			}
		}
	})

	t.Run("AppendGrowsExistingSession", func(t *testing.T) {
		s := newStore(t)
		ts := time.Now().UTC()
		if err := s.Save(session.SessionHeader{ID: "grow", CreatedAt: ts, UpdatedAt: ts},
			agentcore.MessageList{userMsg("one")}); err != nil {
			t.Fatalf("Save: %v", err)
		}
		later := ts.Add(time.Hour)
		if err := s.Append("grow", later, agentcore.MessageList{assistantMsg("two"), userMsg("three")}); err != nil {
			t.Fatalf("Append: %v", err)
		}
		h, msgs, err := s.Load("grow")
		if err != nil {
			t.Fatalf("Load: %v", err)
		}
		if len(msgs) != 3 {
			t.Fatalf("messages = %d, want 3", len(msgs))
		}
		if !h.UpdatedAt.Equal(later) {
			t.Fatalf("UpdatedAt = %v, want %v", h.UpdatedAt, later)
		}
	})

	t.Run("AppendMissingSessionErrors", func(t *testing.T) {
		s := newStore(t)
		err := s.Append("nope", time.Now(), agentcore.MessageList{userMsg("x")})
		if err == nil {
			t.Fatal("Append to a missing session must fail")
		}
		if !errors.Is(err, os.ErrNotExist) {
			t.Fatalf("Append missing = %v, want os.ErrNotExist", err)
		}
	})

	t.Run("AppendBranchCreatesThenGrows", func(t *testing.T) {
		s := newStore(t)
		ts := time.Now().UTC()
		header := session.SessionHeader{ID: "br", CreatedAt: ts, UpdatedAt: ts, Model: "m"}
		leaf1, err := s.AppendBranch(header, "", agentcore.MessageList{userMsg("root"), assistantMsg("a1")})
		if err != nil {
			t.Fatalf("AppendBranch create: %v", err)
		}
		if leaf1 == "" {
			t.Fatal("leaf must be the last appended entry id")
		}
		// A sibling branch off the same root: both must coexist.
		leaf2, err := s.AppendBranch(header, "", agentcore.MessageList{userMsg("other branch")})
		if err != nil {
			t.Fatalf("AppendBranch sibling: %v", err)
		}
		_, entries, err := s.LoadEntries("br")
		if err != nil {
			t.Fatalf("LoadEntries: %v", err)
		}
		if len(entries) != 3 {
			t.Fatalf("entries = %d, want 3 (both branches preserved)", len(entries))
		}
		if leaf2 == leaf1 {
			t.Fatal("sibling leaf must differ")
		}
	})

	t.Run("ListOrdersByUpdatedAtDesc", func(t *testing.T) {
		s := newStore(t)
		base := time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC)
		for i, id := range []string{"old", "newer", "newest"} {
			ts := base.Add(time.Duration(i) * time.Hour)
			if err := s.Save(session.SessionHeader{ID: id, CreatedAt: ts, UpdatedAt: ts},
				agentcore.MessageList{userMsg(id)}); err != nil {
				t.Fatalf("Save %s: %v", id, err)
			}
		}
		listed, err := s.List()
		if err != nil {
			t.Fatalf("List: %v", err)
		}
		if len(listed) != 3 {
			t.Fatalf("listed = %d, want 3", len(listed))
		}
		if listed[0].ID != "newest" || listed[1].ID != "newer" || listed[2].ID != "old" {
			t.Fatalf("order = %s %s %s, want newest newer old", listed[0].ID, listed[1].ID, listed[2].ID)
		}
	})

	t.Run("ForkCopiesPathVerbatim", func(t *testing.T) {
		s := newStore(t)
		ts := time.Now().UTC()
		header := session.SessionHeader{ID: "src", CreatedAt: ts, UpdatedAt: ts}
		// root → left → left2, plus root → right (a sibling).
		if _, err := s.AppendBranch(header, "", agentcore.MessageList{userMsg("root")}); err != nil {
			t.Fatalf("root: %v", err)
		}
		_, entries, _ := s.LoadEntries("src")
		root := entries[0].ID
		if _, err := s.AppendBranch(header, root, agentcore.MessageList{userMsg("left"), assistantMsg("left2")}); err != nil {
			t.Fatalf("left: %v", err)
		}
		_, entries, _ = s.LoadEntries("src")
		left2 := entries[2].ID // root, left, left2
		if _, err := s.AppendBranch(header, root, agentcore.MessageList{userMsg("right")}); err != nil {
			t.Fatalf("right: %v", err)
		}

		newHeader, forked, err := s.Fork("src", left2, ts.Add(time.Minute))
		if err != nil {
			t.Fatalf("Fork: %v", err)
		}
		if newHeader.ParentSession != "src" {
			t.Fatalf("ParentSession = %q, want src", newHeader.ParentSession)
		}
		if len(forked) != 3 { // root, left, left2 — the path only, sibling dropped
			t.Fatalf("forked entries = %d, want 3", len(forked))
		}
		_, reloaded, err := s.LoadEntries(newHeader.ID)
		if err != nil {
			t.Fatalf("LoadEntries fork: %v", err)
		}
		if len(reloaded) != len(forked) {
			t.Fatalf("persisted fork = %d entries, want %d", len(reloaded), len(forked))
		}
	})

	t.Run("ExportImportRoundTrip", func(t *testing.T) {
		s := newStore(t)
		ts := time.Now().UTC()
		header := session.SessionHeader{ID: "exp", CreatedAt: ts, UpdatedAt: ts, Model: "m"}
		if _, err := s.AppendBranch(header, "", agentcore.MessageList{userMsg("q"), assistantMsg("a")}); err != nil {
			t.Fatalf("AppendBranch: %v", err)
		}
		_, entries, _ := s.LoadEntries("exp")

		out := filepath.Join(t.TempDir(), "export.jsonl")
		n, err := s.Export("exp", out)
		if err != nil {
			t.Fatalf("Export: %v", err)
		}
		if n != len(entries) {
			t.Fatalf("Export wrote %d, want %d", n, len(entries))
		}

		now := ts.Add(time.Minute)
		newHeader, imported, err := s.Import(out, now)
		if err != nil {
			t.Fatalf("Import: %v", err)
		}
		if newHeader.ParentSession != "exp" {
			t.Fatalf("Import ParentSession = %q, want exp", newHeader.ParentSession)
		}
		if len(imported) != len(entries) {
			t.Fatalf("imported = %d entries, want %d", len(imported), len(entries))
		}
		for i := range entries {
			if imported[i].ID != entries[i].ID || imported[i].ParentID != entries[i].ParentID {
				t.Fatalf("imported[%d] = %+v, want %+v (lossless round trip)", i, imported[i], entries[i])
			}
		}
	})

	t.Run("ReleaseSessionIdempotent", func(t *testing.T) {
		s := newStore(t)
		if err := s.ReleaseSession("unknown"); err != nil {
			t.Fatalf("ReleaseSession unknown = %v, want nil", err)
		}
		if err := s.ReleaseSession("unknown"); err != nil {
			t.Fatalf("second ReleaseSession = %v, want nil (idempotent)", err)
		}
	})

	t.Run("LoadEntriesMissingWrapsNotExists", func(t *testing.T) {
		s := newStore(t)
		_, _, err := s.LoadEntries("absent")
		if err == nil {
			t.Fatal("LoadEntries of a missing session must fail")
		}
		if !errors.Is(err, os.ErrNotExist) {
			t.Fatalf("LoadEntries missing = %v, want os.ErrNotExist (AppendBranch's create path keys on it)", err)
		}
	})
}
