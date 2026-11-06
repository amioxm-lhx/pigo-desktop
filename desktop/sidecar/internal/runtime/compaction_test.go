package runtime

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
)

// checkpointHost serves a transcript and records the checkpoint the runtime
// installs through session.appendCompaction.
type checkpointHost struct {
	*fakeHost

	mu          sync.Mutex
	compactions []compactionCall
	failAppend  error
}

type compactionCall struct {
	SessionID  string           `json:"sessionId"`
	Compaction compactionRecord `json:"compaction"`
}

func (h *checkpointHost) Call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	if method != "session.appendCompaction" {
		return h.fakeHost.Call(ctx, method, params)
	}
	h.mu.Lock()
	defer h.mu.Unlock()
	if h.failAppend != nil {
		return nil, h.failAppend
	}
	raw, err := json.Marshal(params)
	if err != nil {
		return nil, err
	}
	var call compactionCall
	if err := json.Unmarshal(raw, &call); err != nil {
		return nil, err
	}
	h.compactions = append(h.compactions, call)
	return json.Marshal(map[string]any{"ok": true})
}

func (h *checkpointHost) CallInto(ctx context.Context, method string, params any, out any) error {
	raw, err := h.Call(ctx, method, params)
	if err != nil {
		return err
	}
	if out == nil {
		return nil
	}
	return json.Unmarshal(raw, out)
}

func (h *checkpointHost) lastCompaction(t *testing.T) compactionCall {
	t.Helper()
	h.mu.Lock()
	defer h.mu.Unlock()
	if len(h.compactions) == 0 {
		t.Fatal("no compaction checkpoint was persisted")
	}
	return h.compactions[len(h.compactions)-1]
}

// longTranscript returns enough user rows to push a small window over its
// compaction threshold.
func longTranscript(rows int) []historyMessage {
	messages := make([]historyMessage, 0, rows+1)
	for index := 1; index <= rows; index++ {
		messages = append(messages, historyMessage{
			ID:        fmt.Sprintf("u%d", index),
			Role:      "user",
			Content:   strings.Repeat("x", 400),
			CreatedAt: fmt.Sprintf("2026-11-02T10:00:%02dZ", index),
		})
	}
	return append(messages, historyMessage{
		ID:        "u11",
		Role:      "user",
		Content:   "next",
		CreatedAt: "2026-11-02T10:01:00Z",
	})
}

func newCompactionSession(t *testing.T, host Host, emitter Emitter, window int) *Session {
	t.Helper()
	params := historyTestParams("u11", "next")
	params.SessionID = "session-compact"
	session, err := NewSession(
		context.Background(),
		host,
		emitter,
		log.New(io.Discard, "", 0),
		params,
		textStream{}.Stream,
		provider.Model{Provider: "desktop", ID: "test-model", ContextWindow: window},
	)
	if err != nil {
		t.Fatalf("NewSession: %v", err)
	}
	return session
}

func runUntilAgentEnd(t *testing.T, session *Session, emitter *captureEmitter, turnID string) []protocol.EventEnvelope {
	t.Helper()
	if err := session.Prompt(turnID, "u11", "next"); err != nil {
		t.Fatalf("Prompt: %v", err)
	}
	var events []protocol.EventEnvelope
	timeout := time.After(5 * time.Second)
	for {
		select {
		case envelope := <-emitter.ch:
			events = append(events, envelope)
			if _, ok := envelope.Event.(protocol.AgentEndEvent); ok {
				return events
			}
		case <-timeout:
			t.Fatal("timed out waiting for agent_end")
		}
	}
}

func TestAutoCompactionPersistsHostCheckpoint(t *testing.T) {
	host := &checkpointHost{fakeHost: &fakeHost{sessionMessages: longTranscript(10)}}
	emitter := &captureEmitter{ch: make(chan protocol.EventEnvelope, 128)}
	session := newCompactionSession(t, host, emitter, 1200)

	events := runUntilAgentEnd(t, session, emitter, "turn-compact")

	started := false
	var ended *protocol.CompactionEndEvent
	for _, envelope := range events {
		switch event := envelope.Event.(type) {
		case protocol.CompactionStartEvent:
			if started || event.Reason != "threshold" {
				t.Fatalf("start event = %#v", event)
			}
			started = true
		case protocol.CompactionEndEvent:
			if !started {
				t.Fatal("compaction_end arrived before compaction_start")
			}
			if ended != nil {
				t.Fatal("duplicate compaction_end")
			}
			ended = &event
		}
	}
	if !started || ended == nil {
		t.Fatalf("compaction events missing: %d events", len(events))
	}
	if !ended.OK || ended.Error != nil {
		t.Fatalf("compaction failed: %#v", ended)
	}
	if ended.Mark == nil {
		t.Fatalf("compaction_end has no mark: %#v", ended)
	}
	if ended.Mark.Generation != 1 || ended.Mark.ThroughMessageID != "u8" {
		t.Fatalf("mark = %#v", ended.Mark)
	}
	if ended.Mark.SummaryTokens <= 0 || ended.Mark.ID == "" {
		t.Fatalf("mark = %#v", ended.Mark)
	}
	if ended.FirstKeptMessageID != "u9" {
		t.Fatalf("firstKeptMessageId = %q", ended.FirstKeptMessageID)
	}

	call := host.lastCompaction(t)
	if call.SessionID != "session-compact" {
		t.Fatalf("sessionId = %q", call.SessionID)
	}
	record := call.Compaction
	if record.ID != ended.Mark.ID || record.ThroughMessageID != "u8" {
		t.Fatalf("record = %#v", record)
	}
	if record.FirstKeptMessageID != "u9" {
		t.Fatalf("firstKept = %q", record.FirstKeptMessageID)
	}
	if !strings.Contains(record.Summary, "done") || record.TokensBefore <= 0 {
		t.Fatalf("record = %#v", record)
	}
	if len(record.RetainedTail) != 0 {
		t.Fatalf("retainedTail = %#v, want an empty array", record.RetainedTail)
	}
	if record.ProviderID != "desktop" || record.ModelID != "test-model" {
		t.Fatalf("record binding = %q/%q", record.ProviderID, record.ModelID)
	}
	if generation := checkpointGeneration(record.Details); generation != 1 {
		t.Fatalf("generation = %d", generation)
	}
	if mode, _ := record.Details["retainedTailMode"].(string); mode != compactionRetentionCompletedTurn {
		t.Fatalf("retainedTailMode = %v", record.Details["retainedTailMode"])
	}
}

func TestAutoCompactionReportsPersistenceFailure(t *testing.T) {
	host := &checkpointHost{
		fakeHost:   &fakeHost{sessionMessages: longTranscript(10)},
		failAppend: errors.New("disk full"),
	}
	emitter := &captureEmitter{ch: make(chan protocol.EventEnvelope, 128)}
	session := newCompactionSession(t, host, emitter, 1200)

	events := runUntilAgentEnd(t, session, emitter, "turn-compact")

	var ended *protocol.CompactionEndEvent
	for _, envelope := range events {
		if event, ok := envelope.Event.(protocol.CompactionEndEvent); ok {
			ended = &event
		}
	}
	if ended == nil {
		t.Fatal("compaction_end missing")
	}
	if ended.OK {
		t.Fatalf("compaction should report the failed checkpoint: %#v", ended)
	}
	if ended.Error == nil || ended.Error.Code != "CONTEXT_COMPACTION_FAILED" {
		t.Fatalf("error = %#v", ended.Error)
	}
	if !strings.Contains(ended.Error.Message, "disk full") {
		t.Fatalf("error message = %q", ended.Error.Message)
	}
	if ended.Mark != nil {
		t.Fatalf("failed checkpoint carries a mark: %#v", ended.Mark)
	}
}

func TestCompactionCheckpointReplaysIntoNextRuntime(t *testing.T) {
	host := &checkpointHost{fakeHost: &fakeHost{sessionMessages: longTranscript(10)}}
	emitter := &captureEmitter{ch: make(chan protocol.EventEnvelope, 128)}
	session := newCompactionSession(t, host, emitter, 1200)
	runUntilAgentEnd(t, session, emitter, "turn-compact")

	call := host.lastCompaction(t)
	rows := longTranscript(10)
	replayHost := historyHostWithCompaction(
		&fakeHost{sessionMessages: rows},
		&historyCompaction{
			Summary:          call.Compaction.Summary,
			ThroughMessageID: call.Compaction.ThroughMessageID,
			TokensBefore:     call.Compaction.TokensBefore,
			RetainedTail:     []json.RawMessage{},
			Details:          call.Compaction.Details,
			CreatedAt:        call.Compaction.CreatedAt,
		},
	)
	replayed, err := NewSession(
		context.Background(),
		replayHost,
		nil,
		log.New(io.Discard, "", 0),
		historyTestParams("u11", "next"),
		textStream{}.Stream,
		provider.Model{Provider: "desktop", ID: "test-model"},
	)
	if err != nil {
		t.Fatalf("NewSession: %v", err)
	}
	messages := replayed.agentCtx.Messages
	if len(messages) != 3 {
		t.Fatalf("replayed context = %d messages, want summary + u9 + u10", len(messages))
	}
	checkpoint, ok := messages[0].(agentcore.CompactionMessage)
	if !ok || checkpoint.Summary != call.Compaction.Summary {
		t.Fatalf("messages[0] = %#v", messages[0])
	}
	if _, ok := messages[1].(agentcore.UserMessage); !ok {
		t.Fatalf("messages[1] = %#v, want the first retained row", messages[1])
	}
	if replayed.compactionGeneration != 1 {
		t.Fatalf("generation = %d", replayed.compactionGeneration)
	}
}

func TestCompactionSettingsStayValidForSmallWindows(t *testing.T) {
	settings := compactionSettingsForWindow(1200)
	if !settings.Enabled || settings.ReserveTokens != 600 || settings.KeepRecentTokens != 200 {
		t.Fatalf("settings = %#v", settings)
	}
	if settings := compactionSettingsForWindow(0); settings.Enabled {
		t.Fatalf("unknown window enabled compaction: %#v", settings)
	}
	if settings := compactionSettingsForWindow(6); !settings.Enabled ||
		settings.ReserveTokens <= 0 || settings.KeepRecentTokens <= 0 ||
		settings.KeepRecentTokens > 6-settings.ReserveTokens {
		t.Fatalf("tiny window settings = %#v", settings)
	}
}

func TestCompactionBoundaryRefusesDivergentContext(t *testing.T) {
	hostMessages, hostIDs := historyToMessagesWithIDs(longTranscript(10), provider.Model{})
	live := append(agentcore.MessageList(nil), hostMessages...)
	live[2] = agentcore.UserMessage{
		RoleField: agentcore.RoleUser,
		Content:   agentcore.ContentList{agentcore.NewTextContent("rewritten")},
	}
	if _, _, ok := compactionBoundary(hostMessages, hostIDs, live, 5); ok {
		t.Fatal("a rewritten transcript must not anchor a checkpoint")
	}
	if _, _, ok := compactionBoundary(hostMessages, hostIDs, hostMessages, len(hostMessages)+1); ok {
		t.Fatal("an out-of-range cut must not anchor a checkpoint")
	}
	if through, firstKept, ok := compactionBoundary(hostMessages, hostIDs, hostMessages, 5); !ok {
		t.Fatal("a matching transcript should anchor a checkpoint")
	} else if through != "u5" || firstKept != "u6" {
		t.Fatalf("boundary = %q/%q", through, firstKept)
	}
	// A summary covering the whole transcript anchors on its last row and keeps
	// nothing after it.
	if through, firstKept, ok := compactionBoundary(hostMessages, hostIDs, hostMessages, len(hostMessages)); !ok {
		t.Fatal("a full-context compaction should anchor on the last row")
	} else if through != "u11" || firstKept != "" {
		t.Fatalf("full boundary = %q/%q", through, firstKept)
	}
}
