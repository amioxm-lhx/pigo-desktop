package runtime

import (
	"context"
	"errors"
	"testing"

	"github.com/smallnest/pigo/desktop/sidecar/internal/jsonrpc"
)

func TestRebuildRequiresSessionID(t *testing.T) {
	m := historyTestManager(&fakeHost{})
	if _, err := m.Rebuild(context.Background(), "  "); err == nil {
		t.Fatal("expected agent.rebuild to reject an empty session id")
	}
}

func TestRebuildWithoutLiveRuntimeIsANoOp(t *testing.T) {
	m := historyTestManager(&fakeHost{})
	rebuilt, err := m.Rebuild(context.Background(), "no-runtime")
	if err != nil {
		t.Fatalf("rebuild without a runtime: %v", err)
	}
	if rebuilt {
		t.Fatal("no live runtime should not report a rebuild")
	}
	if m.Count() != 0 {
		t.Fatalf("rebuild created %d runtimes", m.Count())
	}
}

func TestRebuildRejectsRunningSession(t *testing.T) {
	m := historyTestManager(&fakeHost{})
	params := historyTestParams("u1", "hello")
	session, err := m.RuntimeFor(context.Background(), params)
	if err != nil {
		t.Fatalf("build runtime: %v", err)
	}

	session.mu.Lock()
	session.running = true
	session.mu.Unlock()

	_, err = m.Rebuild(context.Background(), params.SessionID)
	var rpcErr *jsonrpc.Error
	if !errors.As(err, &rpcErr) {
		t.Fatalf("expected a JSON-RPC error, got %v", err)
	}
	data, ok := rpcErr.Data.(map[string]string)
	if !ok || data["errorCode"] != "AGENT_BUSY" {
		t.Fatalf("expected AGENT_BUSY, got %v", err)
	}
	if m.Get(params.SessionID) != session {
		t.Fatal("a busy session must not be disposed by /rebuild")
	}
}

func TestRebuildReloadsTranscriptAndClearsTurnIdentity(t *testing.T) {
	host := &fakeHost{sessionMessages: []historyMessage{
		{ID: "u1", Role: "user", Content: "hello", CreatedAt: "2026-11-07T01:00:00Z"},
	}}
	m := historyTestManager(host)
	params := historyTestParams("u1", "hello")
	params.TurnID = "turn-1"
	first, err := m.RuntimeFor(context.Background(), params)
	if err != nil {
		t.Fatalf("build runtime: %v", err)
	}

	// Host-side rewrite: the live runtime still only knows the old context.
	host.sessionMessages = []historyMessage{
		{ID: "u1", Role: "user", Content: "hello", CreatedAt: "2026-11-07T01:00:00Z"},
		{ID: "a1", Role: "assistant", Content: "hi", CreatedAt: "2026-11-07T01:00:01Z"},
		{ID: "u2", Role: "user", Content: "next", CreatedAt: "2026-11-07T01:00:02Z"},
	}

	rebuilt, err := m.Rebuild(context.Background(), params.SessionID)
	if err != nil {
		t.Fatalf("rebuild: %v", err)
	}
	if !rebuilt {
		t.Fatal("expected a live runtime to be replaced")
	}
	second := m.Get(params.SessionID)
	if second == nil || second == first {
		t.Fatal("rebuild must install a fresh runtime")
	}
	if len(second.agentCtx.Messages) <= len(first.agentCtx.Messages) {
		t.Fatalf(
			"rebuilt context did not pick up the host transcript: %d -> %d",
			len(first.agentCtx.Messages),
			len(second.agentCtx.Messages),
		)
	}
	// Carrying the old turn identity would make snapshotFromDetail drop the
	// last transcript row as if it were an unpersisted current prompt.
	if second.params.TurnID != "" || second.params.Content != "" || second.params.UserMessageID != "" {
		t.Fatalf("rebuild leaked turn identity: %+v", second.params)
	}
	if !second.params.RefreshHistory {
		t.Fatal("rebuild should mark the runtime as history-refreshed")
	}
}
