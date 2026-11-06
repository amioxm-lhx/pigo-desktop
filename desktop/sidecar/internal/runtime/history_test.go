package runtime

import (
	"context"
	"encoding/json"
	"io"
	"log"
	"testing"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
)

func historyTestManager(host *fakeHost) *Manager {
	return NewManager(Options{
		Host:     host,
		Emitter:  &captureEmitter{ch: make(chan protocol.EventEnvelope, 16)},
		Logger:   log.New(io.Discard, "", 0),
		StreamFn: (&scriptedStream{}).Stream,
		Model:    provider.Model{Provider: "desktop", ID: "test-model"},
	})
}

func historyTestParams(userMessageID, content string) protocol.PromptParams {
	return protocol.PromptParams{
		SessionID:     "session-history",
		Mode:          "agent",
		Provider:      protocol.ProviderConfig{ID: "desktop", ModelID: "test-model", APIStyle: "openai", BaseURL: "http://example.test", AuthKind: "none"},
		UserMessageID: userMessageID,
		Content:       content,
	}
}

func TestHistoryRestoresTranscriptAndDropsCurrentPrompt(t *testing.T) {
	host := &fakeHost{sessionMessages: []historyMessage{
		{ID: "u1", Role: "user", Content: "hello", CreatedAt: "2026-09-15T10:00:00Z"},
		{ID: "a1", Role: "assistant", Content: "hi", Thinking: "think", CreatedAt: "2026-09-15T10:00:01Z"},
		{ID: "t1", Role: "tool", Content: "file data", ToolCallID: "call-1", ToolName: "Read", ToolArgs: json.RawMessage(`{"path":"notes.md"}`), CreatedAt: "2026-09-15T10:00:02Z"},
		// Failed assistant rows and subagent rows never enter the parent context.
		{ID: "aerr", Role: "assistant", Content: "boom", Status: "error", CreatedAt: "2026-09-15T10:00:03Z"},
		{ID: "sub", Role: "tool", Content: "subagent result", ToolCallID: "call-2", ToolName: "Read", ParentToolCallID: "parent-1", CreatedAt: "2026-09-15T10:00:04Z"},
		// Electron main persists the current prompt row last.
		{ID: "u2", Role: "user", Content: "next", CreatedAt: "2026-09-15T10:00:05Z"},
	}}

	session, err := historyTestManager(host).RuntimeFor(context.Background(), historyTestParams("u2", "next"))
	if err != nil {
		t.Fatalf("RuntimeFor: %v", err)
	}
	messages := session.agentCtx.Messages
	if len(messages) != 3 {
		t.Fatalf("history = %d messages, want 3: %#v", len(messages), messages)
	}

	user, ok := messages[0].(agentcore.UserMessage)
	if !ok || agentcore.ContentToText(user.Content) != "hello" {
		t.Fatalf("messages[0] = %#v, want the user 'hello' row", messages[0])
	}
	assistant, ok := messages[1].(agentcore.AssistantMessage)
	if !ok {
		t.Fatalf("messages[1] = %T, want assistant", messages[1])
	}
	if text := agentcore.ContentToText(assistant.Content); text != "hi" {
		t.Fatalf("assistant text = %q", text)
	}
	if thinking := assistantThinking(assistant); thinking != "think" {
		t.Fatalf("assistant thinking = %q", thinking)
	}
	calls := assistant.ToolCalls()
	if len(calls) != 1 || calls[0].Name != "Read" || calls[0].ID != "call-1" {
		t.Fatalf("assistant tool calls = %#v", calls)
	}
	result, ok := messages[2].(agentcore.ToolResultMessage)
	if !ok || result.ToolCallID != "call-1" || agentcore.ContentToText(result.Content) != "file data" {
		t.Fatalf("messages[2] = %#v, want the Read tool result", messages[2])
	}
}

func TestRetainedTailReplaysLatestUserMessage(t *testing.T) {
	tail := []json.RawMessage{
		json.RawMessage(`{"role":"assistant","content":[{"type":"text","text":"ignored"}]}`),
		json.RawMessage(`{"role":"user","content":"follow-up","timestamp":1}`),
		json.RawMessage(`{"role":"user","content":[{"type":"text","text":"from blocks"}],"timestamp":2}`),
	}
	messages := retainedTailMessages(&historyCompaction{
		RetainedTail: tail,
		Details:      map[string]any{"retainedTailMode": "active_turn"},
	})
	if len(messages) != 1 {
		t.Fatalf("retained tail = %d messages, want one", len(messages))
	}
	user, ok := messages[0].(agentcore.UserMessage)
	if !ok || agentcore.ContentToText(user.Content) != "from blocks" {
		t.Fatalf("retained tail = %#v, want the latest user block message", messages[0])
	}

	if messages := retainedTailMessages(&historyCompaction{
		RetainedTail: tail,
		Details:      map[string]any{"retainedTailMode": "completed_turn"},
	}); len(messages) != 0 {
		t.Fatalf("completed-turn tail = %#v, want none", messages)
	}

	// A record without the marker is treated as an active turn.
	if messages := retainedTailMessages(&historyCompaction{RetainedTail: tail}); len(messages) != 1 {
		t.Fatalf("unmarked tail = %#v, want the latest user message", messages)
	}
}

func TestTruncateHistory(t *testing.T) {
	rows := []historyMessage{
		{ID: "u1", Role: "user", Content: "one"},
		{ID: "a1", Role: "assistant", Content: "two"},
		{ID: "u2", Role: "user", Content: "three"},
	}

	if got := truncateHistory(rows, "u1", nil); len(got) != 0 {
		t.Fatalf("truncate from the first row = %d rows, want 0", len(got))
	}
	if got := truncateHistory(rows, "u2", nil); len(got) != 2 || got[1].ID != "a1" {
		t.Fatalf("truncate from u2 = %#v", got)
	}
	// A boundary the host already removed leaves the transcript untouched.
	if got := truncateHistory(rows, "gone", nil); len(got) != 3 {
		t.Fatalf("unknown id truncated the transcript: %#v", got)
	}
	limit := 2
	if got := truncateHistory(rows, "", &limit); len(got) != 2 || got[1].ID != "a1" {
		t.Fatalf("truncateBefore 2 = %#v", got)
	}
}

func TestRuntimeRefreshRebuildsLiveSession(t *testing.T) {
	host := &fakeHost{sessionMessages: []historyMessage{
		{ID: "u1", Role: "user", Content: "hello", CreatedAt: "2026-09-15T10:00:00Z"},
		{ID: "a1", Role: "assistant", Content: "hi", CreatedAt: "2026-09-15T10:00:01Z"},
	}}
	manager := historyTestManager(host)
	base := historyTestParams("u9", "new")

	first, err := manager.RuntimeFor(context.Background(), base)
	if err != nil {
		t.Fatalf("first RuntimeFor: %v", err)
	}
	reused, err := manager.RuntimeFor(context.Background(), base)
	if err != nil {
		t.Fatalf("reused RuntimeFor: %v", err)
	}
	if reused != first {
		t.Fatal("an unchanged binding must reuse the live runtime")
	}

	refresh := base
	refresh.RefreshHistory = true
	refreshed, err := manager.RuntimeFor(context.Background(), refresh)
	if err != nil {
		t.Fatalf("refresh RuntimeFor: %v", err)
	}
	if refreshed == first {
		t.Fatal("refreshHistory must rebuild the runtime")
	}
	if len(refreshed.agentCtx.Messages) != 2 {
		t.Fatalf("refreshed history = %d messages, want 2", len(refreshed.agentCtx.Messages))
	}

	truncatedParams := base
	truncatedParams.TruncateFromMessageID = "a1"
	truncated, err := manager.RuntimeFor(context.Background(), truncatedParams)
	if err != nil {
		t.Fatalf("truncate RuntimeFor: %v", err)
	}
	if truncated == refreshed {
		t.Fatal("truncateFromMessageId must rebuild the runtime")
	}
	if len(truncated.agentCtx.Messages) != 1 {
		t.Fatalf("truncated history = %d messages, want 1: %#v", len(truncated.agentCtx.Messages), truncated.agentCtx.Messages)
	}
	if user, ok := truncated.agentCtx.Messages[0].(agentcore.UserMessage); !ok || agentcore.ContentToText(user.Content) != "hello" {
		t.Fatalf("truncated history = %#v, want the retained u1 row", truncated.agentCtx.Messages)
	}
	if manager.Count() != 1 {
		t.Fatalf("manager count = %d, want 1", manager.Count())
	}
}

func TestRuntimeRefreshPicksUpExternalTranscriptChange(t *testing.T) {
	host := &fakeHost{sessionMessages: []historyMessage{
		{ID: "u1", Role: "user", Content: "hello", CreatedAt: "2026-09-15T10:00:00Z"},
		{ID: "a1", Role: "assistant", Content: "hi", CreatedAt: "2026-09-15T10:00:01Z"},
	}}
	manager := historyTestManager(host)
	base := historyTestParams("u9", "new")

	session, err := manager.RuntimeFor(context.Background(), base)
	if err != nil {
		t.Fatalf("RuntimeFor: %v", err)
	}
	if len(session.agentCtx.Messages) != 2 {
		t.Fatalf("initial history = %d messages", len(session.agentCtx.Messages))
	}

	// Another writer appended a turn; the live runtime must adopt it without
	// being rebuilt.
	host.sessionMessages = append(host.sessionMessages,
		historyMessage{ID: "u2", Role: "user", Content: "later", CreatedAt: "2026-09-15T10:00:02Z"},
		historyMessage{ID: "a2", Role: "assistant", Content: "answer", CreatedAt: "2026-09-15T10:00:03Z"},
	)
	reused, err := manager.RuntimeFor(context.Background(), base)
	if err != nil {
		t.Fatalf("RuntimeFor after append: %v", err)
	}
	if reused != session {
		t.Fatal("an automatic history refresh must reuse the live runtime")
	}
	if len(reused.agentCtx.Messages) != 4 {
		t.Fatalf("refreshed history = %d messages, want 4", len(reused.agentCtx.Messages))
	}

	// A rewrite in place (same length, different content) is a replacement, not
	// a lagging writer.
	host.sessionMessages[1] = historyMessage{ID: "a1", Role: "assistant", Content: "rewritten", CreatedAt: "2026-09-15T10:00:01Z"}
	if _, err := manager.RuntimeFor(context.Background(), base); err != nil {
		t.Fatalf("RuntimeFor after rewrite: %v", err)
	}
	assistant, ok := reused.agentCtx.Messages[1].(agentcore.AssistantMessage)
	if !ok || agentcore.ContentToText(assistant.Content) != "rewritten" {
		t.Fatalf("rewritten history = %#v, want the host content", reused.agentCtx.Messages[1])
	}
}

func TestRuntimeRefreshKeepsLiveContextWhenHostIsBehind(t *testing.T) {
	host := &fakeHost{sessionMessages: []historyMessage{
		{ID: "u1", Role: "user", Content: "hello", CreatedAt: "2026-09-15T10:00:00Z"},
		{ID: "a1", Role: "assistant", Content: "hi", CreatedAt: "2026-09-15T10:00:01Z"},
		{ID: "u2", Role: "user", Content: "later", CreatedAt: "2026-09-15T10:00:02Z"},
		{ID: "a2", Role: "assistant", Content: "answer", CreatedAt: "2026-09-15T10:00:03Z"},
	}}
	manager := historyTestManager(host)
	base := historyTestParams("u9", "new")
	session, err := manager.RuntimeFor(context.Background(), base)
	if err != nil {
		t.Fatalf("RuntimeFor: %v", err)
	}
	if len(session.agentCtx.Messages) != 4 {
		t.Fatalf("initial history = %d messages", len(session.agentCtx.Messages))
	}

	// The host has not persisted the newest turn yet. Its rows are a prefix of
	// the live context, so the live context must win.
	host.sessionMessages = host.sessionMessages[:2]
	if _, err := manager.RuntimeFor(context.Background(), base); err != nil {
		t.Fatalf("RuntimeFor with lagging host: %v", err)
	}
	if len(session.agentCtx.Messages) != 4 {
		t.Fatalf("lagging host shrank the live context to %d messages", len(session.agentCtx.Messages))
	}
}

func TestMessageSignatureIgnoresUnpersistedImageBytes(t *testing.T) {
	host := agentcore.UserMessage{
		RoleField: agentcore.RoleUser,
		Content:   agentcore.ContentList{agentcore.NewTextContent("look")},
	}
	live := agentcore.UserMessage{
		RoleField: agentcore.RoleUser,
		Content: agentcore.ContentList{
			agentcore.NewTextContent("look"),
			agentcore.NewImageContent("aGVsbG8=", "image/png"),
		},
	}
	if messageSignature(host) != messageSignature(live) {
		t.Fatal("an image-bearing live prompt must match the text-only host row")
	}
	changed := agentcore.UserMessage{
		RoleField: agentcore.RoleUser,
		Content:   agentcore.ContentList{agentcore.NewTextContent("different")},
	}
	if messageSignature(host) == messageSignature(changed) {
		t.Fatal("different user text must not compare equal")
	}
}

func TestHistoryRestoresActiveCompactionCheckpoint(t *testing.T) {
	host := &fakeHost{sessionMessages: []historyMessage{
		{ID: "u1", Role: "user", Content: "old question", CreatedAt: "2026-09-15T10:00:00Z"},
		{ID: "a1", Role: "assistant", Content: "old answer", CreatedAt: "2026-09-15T10:00:01Z"},
		{ID: "t1", Role: "tool", Content: "old tool", ToolCallID: "call-1", ToolName: "Read", CreatedAt: "2026-09-15T10:00:02Z"},
		{ID: "u2", Role: "user", Content: "next", CreatedAt: "2026-09-15T10:00:03Z"},
	}}
	// A checkpoint through t1 collapses everything before it.
	detailCompaction := &historyCompaction{
		Summary:          "the earlier context",
		ThroughMessageID: "t1",
		TokensBefore:     4200,
		RetainedTail:     []json.RawMessage{json.RawMessage(`{"role":"user","content":"follow-up","timestamp":3}`)},
		Details:          map[string]any{"retainedTailMode": "active_turn"},
		CreatedAt:        "2026-09-15T10:00:02.500Z",
	}

	session, err := NewSession(context.Background(), historyHostWithCompaction(host, detailCompaction), nil, log.New(io.Discard, "", 0), historyTestParams("u2", "next"), (&scriptedStream{}).Stream, provider.Model{Provider: "desktop", ID: "test-model"})
	if err != nil {
		t.Fatalf("NewSession: %v", err)
	}
	messages := session.agentCtx.Messages
	if len(messages) != 2 {
		t.Fatalf("history = %d messages, want checkpoint + retained tail", len(messages))
	}
	checkpoint, ok := messages[0].(agentcore.CompactionMessage)
	if !ok || checkpoint.Summary != "the earlier context" || checkpoint.TokensBefore != 4200 {
		t.Fatalf("messages[0] = %#v, want the compaction checkpoint", messages[0])
	}
	user, ok := messages[1].(agentcore.UserMessage)
	if !ok || agentcore.ContentToText(user.Content) != "follow-up" {
		t.Fatalf("messages[1] = %#v, want the retained user message", messages[1])
	}
}

type compactionHost struct {
	*fakeHost
	compaction *historyCompaction
}

func (h compactionHost) Call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	if method == "session.get" {
		reply := sessionDetail{Session: &sessionView{
			Messages:   append([]historyMessage(nil), h.sessionMessages...),
			Compaction: h.compaction,
		}}
		return json.Marshal(reply)
	}
	return h.fakeHost.Call(ctx, method, params)
}

func (h compactionHost) CallInto(ctx context.Context, method string, params any, out any) error {
	raw, err := h.Call(ctx, method, params)
	if err != nil {
		return err
	}
	if out == nil {
		return nil
	}
	return json.Unmarshal(raw, out)
}

func historyHostWithCompaction(host *fakeHost, compaction *historyCompaction) Host {
	return compactionHost{fakeHost: host, compaction: compaction}
}
