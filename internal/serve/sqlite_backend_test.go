// Serve over the SQLite backend: the phase-3b "callers unchanged" proof —
// the service layer is constructed with a sqlite store instead of a JSONL
// store and the whole create→prompt→persist→reopen flow works with zero
// serve-side changes. serve never learns which backend it holds; only the
// construction site differs.
package serve_test

import (
	"context"
	"path/filepath"
	"testing"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
	"github.com/smallnest/pigo/internal/runtime"
	"github.com/smallnest/pigo/internal/serve"
	"github.com/smallnest/pigo/internal/session"
	"github.com/smallnest/pigo/internal/session/sqlite"
)

func TestServeOverSqliteBackend(t *testing.T) {
	db, err := sqlite.Open(filepath.Join(t.TempDir(), "sessions.db"))
	if err != nil {
		t.Fatalf("sqlite.Open: %v", err)
	}
	defer db.Close()

	svc, err := serve.NewService(db, func(h session.SessionHeader, th agentcore.ThinkingLevel) runtime.RunConfig {
		return runtime.RunConfig{
			LoopConfig: runtime.LoopConfig{Model: h.Model, Provider: h.Provider, Stream: textTurn("sqlite reply")},
		}
	})
	if err != nil {
		t.Fatalf("NewService over sqlite: %v", err)
	}

	ctx := context.Background()
	rt, err := svc.CreateSession(ctx, serve.SessionConfig{Model: "fake", Provider: "p1"})
	if err != nil {
		t.Fatalf("CreateSession: %v", err)
	}
	if err := rt.Prompt(ctx, "hello"); err != nil {
		t.Fatalf("Prompt: %v", err)
	}
	if err := rt.WaitIdle(ctx); err != nil {
		t.Fatalf("WaitIdle: %v", err)
	}
	snap, err := rt.Snapshot(ctx)
	if err != nil {
		t.Fatalf("Snapshot: %v", err)
	}
	if len(snap.Entries) != 2 {
		t.Fatalf("entries = %d, want 2", len(snap.Entries))
	}

	// Reopen from the database: the continuation context survived the
	// backend switch.
	rt2, err := svc.OpenSession(ctx, rt.ID())
	if err != nil {
		t.Fatalf("OpenSession: %v", err)
	}
	snap2, err := rt2.Snapshot(ctx)
	if err != nil {
		t.Fatalf("Snapshot 2: %v", err)
	}
	if len(snap2.Entries) != 2 || snap2.LeafID != snap.LeafID {
		t.Fatalf("reopened = %d entries, leaf %q; want 2, %q", len(snap2.Entries), snap2.LeafID, snap.LeafID)
	}
}

// textTurn returns a StreamFn answering every call with one plain text turn.
func textTurn(text string) provider.StreamFn {
	return func(ctx context.Context, model string, llm provider.LlmContext, cfg provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		final := agentcore.AssistantMessage{
			RoleField:  agentcore.RoleAssistant,
			Content:    agentcore.ContentList{agentcore.NewTextContent(text)},
			StopReason: agentcore.StopReasonEndTurn,
		}
		s := provider.NewAssistantMessageEventStream(0)
		go func() {
			_ = s.Emit(ctx, provider.StreamDoneEvent{Message: final})
			s.Close()
		}()
		return s, nil
	}
}
