package runtime

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"sync"
	"testing"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
)

// fakeHost serves the minimal host contract: a Read tool, a successful
// execution, and a no-op auth resolution.
type fakeHost struct {
	mu       sync.Mutex
	executed []string
	// sessionMessages is the transcript session.get returns.
	sessionMessages []historyMessage
}

func (h *fakeHost) Call(_ context.Context, method string, params any) (json.RawMessage, error) {
	switch method {
	case "session.get":
		reply := sessionDetail{Session: &sessionView{Messages: append([]historyMessage(nil), h.sessionMessages...)}}
		return json.Marshal(reply)
	case "tools.list":
		reply := catalogResponse{Tools: []protocol.ToolDefinition{{
			Name:        "Read",
			Description: "Read a file",
			Parameters:  json.RawMessage(`{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}`),
		}}}
		return json.Marshal(reply)
	case "tools.execute":
		request, _ := params.(protocol.ToolExecuteParams)
		h.mu.Lock()
		h.executed = append(h.executed, request.ToolName)
		h.mu.Unlock()
		return json.Marshal(protocol.ToolExecuteResult{
			OK:      true,
			Content: json.RawMessage(`"hello from host"`),
		})
	default:
		return nil, fmt.Errorf("unexpected host method %q", method)
	}
}

func (h *fakeHost) CallInto(ctx context.Context, method string, params any, out any) error {
	raw, err := h.Call(ctx, method, params)
	if err != nil {
		return err
	}
	if out == nil {
		return nil
	}
	return json.Unmarshal(raw, out)
}

func (h *fakeHost) executedTools() []string {
	h.mu.Lock()
	defer h.mu.Unlock()
	out := append([]string(nil), h.executed...)
	return out
}

type captureEmitter struct {
	ch chan protocol.EventEnvelope
}

func (e *captureEmitter) Emit(envelope protocol.EventEnvelope) error {
	e.ch <- envelope
	return nil
}

// scriptedStream emits a tool call on the first request and a final text on
// the request that follows the tool result.
type scriptedStream struct {
	mu    sync.Mutex
	calls int
}

func (s *scriptedStream) Stream(ctx context.Context, _ string, _ provider.LlmContext, _ provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
	s.mu.Lock()
	call := s.calls
	s.calls++
	s.mu.Unlock()

	stream := provider.NewAssistantMessageEventStream(4)
	go func() {
		defer stream.Close()
		now := time.Now().UnixMilli()
		if call == 0 {
			partial := agentcore.AssistantMessage{
				RoleField:  agentcore.RoleAssistant,
				Content:    agentcore.ContentList{agentcore.NewToolCallContent("call-1", "Read", json.RawMessage(`{"path":"notes.md"}`))},
				StopReason: agentcore.StopReasonToolUse,
				Timestamp:  now,
			}
			_ = stream.Emit(ctx, provider.StreamStartEvent{Partial: agentcore.AssistantMessage{RoleField: agentcore.RoleAssistant, Timestamp: now}})
			_ = stream.Emit(ctx, provider.StreamToolCallEvent{Partial: partial})
			_ = stream.Emit(ctx, provider.StreamDoneEvent{Message: partial})
			return
		}

		final := agentcore.AssistantMessage{
			RoleField:  agentcore.RoleAssistant,
			Content:    agentcore.ContentList{agentcore.NewTextContent("done")},
			StopReason: agentcore.StopReasonEndTurn,
			Timestamp:  now,
		}
		_ = stream.Emit(ctx, provider.StreamStartEvent{Partial: agentcore.AssistantMessage{RoleField: agentcore.RoleAssistant, Timestamp: now}})
		_ = stream.Emit(ctx, provider.StreamTextEvent{Partial: final})
		_ = stream.Emit(ctx, provider.StreamDoneEvent{Message: final})
	}()
	return stream, nil
}

func TestPromptStreamsDesktopEventsAndRunsHostTool(t *testing.T) {
	host := &fakeHost{}
	emitter := &captureEmitter{ch: make(chan protocol.EventEnvelope, 128)}
	stream := &scriptedStream{}
	manager := NewManager(Options{
		Host:     host,
		Emitter:  emitter,
		Logger:   log.New(io.Discard, "", 0),
		StreamFn: stream.Stream,
		Model:    provider.Model{Provider: "desktop", ID: "test-model"},
	})

	params := protocol.PromptParams{
		SessionID: "session-1",
		Mode:      "agent",
		TurnID:    "turn-1",
		Provider: protocol.ProviderConfig{
			ID:       "desktop",
			ModelID:  "test-model",
			APIStyle: "openai",
			BaseURL:  "http://example.test",
			AuthKind: "none",
		},
		Content: "read notes",
	}
	session, err := manager.RuntimeFor(context.Background(), params)
	if err != nil {
		t.Fatalf("RuntimeFor: %v", err)
	}
	if err := session.Prompt("turn-1", "user-1", "read notes"); err != nil {
		t.Fatalf("Prompt: %v", err)
	}

	seen := map[string]bool{}
	var messageIDs []string
	timeout := time.After(3 * time.Second)
	for done := false; !done; {
		select {
		case envelope := <-emitter.ch:
			name := eventName(envelope.Event)
			seen[name] = true
			if end, ok := envelope.Event.(protocol.AgentEndEvent); ok {
				messageIDs = end.MessageIDs
				done = true
			}
		case <-timeout:
			t.Fatalf("timed out waiting for agent_end; events seen: %v", seen)
		}
	}

	for _, name := range []string{
		protocol.TypeAgentStart,
		protocol.TypeTurnStart,
		protocol.TypeMessageStart,
		protocol.TypeToolStart,
		protocol.TypeToolEnd,
		protocol.TypeMessageEnd,
		protocol.TypeTurnEnd,
		protocol.TypeAgentEnd,
	} {
		if !seen[name] {
			t.Errorf("missing event %q; seen: %v", name, seen)
		}
	}
	if len(messageIDs) < 2 {
		t.Errorf("agent_end messageIds = %v, want at least 2 assistant rows", messageIDs)
	}
	if executed := host.executedTools(); len(executed) != 1 || executed[0] != "Read" {
		t.Errorf("host executed tools = %v, want [Read]", executed)
	}
}

// blockingStream starts a response and then waits for cancellation, so an abort
// leaves a streaming row open.
type blockingStream struct{}

func (blockingStream) Stream(ctx context.Context, _ string, _ provider.LlmContext, _ provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
	stream := provider.NewAssistantMessageEventStream(4)
	go func() {
		defer stream.Close()
		now := time.Now().UnixMilli()
		partial := agentcore.AssistantMessage{
			RoleField:  agentcore.RoleAssistant,
			Content:    agentcore.ContentList{agentcore.NewTextContent("partial")},
			StopReason: agentcore.StopReasonEndTurn,
			Timestamp:  now,
		}
		_ = stream.Emit(ctx, provider.StreamStartEvent{Partial: agentcore.AssistantMessage{RoleField: agentcore.RoleAssistant, Timestamp: now}})
		_ = stream.Emit(ctx, provider.StreamTextEvent{Partial: partial})
		<-ctx.Done()
	}()
	return stream, nil
}

func TestAbortClosesStreamingRow(t *testing.T) {
	host := &fakeHost{}
	emitter := &captureEmitter{ch: make(chan protocol.EventEnvelope, 128)}
	manager := NewManager(Options{
		Host:     host,
		Emitter:  emitter,
		Logger:   log.New(io.Discard, "", 0),
		StreamFn: blockingStream{}.Stream,
		Model:    provider.Model{Provider: "desktop", ID: "test-model"},
	})
	params := protocol.PromptParams{
		SessionID: "session-abort",
		Mode:      "agent",
		Provider: protocol.ProviderConfig{
			ID:       "desktop",
			ModelID:  "test-model",
			APIStyle: "openai",
			BaseURL:  "http://example.test",
			AuthKind: "none",
		},
		Content: "start a long response",
	}
	session, err := manager.RuntimeFor(context.Background(), params)
	if err != nil {
		t.Fatalf("RuntimeFor: %v", err)
	}
	if err := session.Prompt("turn-abort", "user-abort", "start a long response"); err != nil {
		t.Fatalf("Prompt: %v", err)
	}

	timeout := time.After(3 * time.Second)
	started := false
	abortedRow := false
	for {
		select {
		case envelope := <-emitter.ch:
			switch event := envelope.Event.(type) {
			case protocol.MessageStartEvent:
				if !started {
					started = true
					session.Abort()
				}
			case protocol.MessageEndEvent:
				if event.Message.Status == protocol.StatusAborted {
					abortedRow = true
				}
			case protocol.AgentEndEvent:
				if !started {
					t.Fatal("agent_end arrived before message_start")
				}
				if !abortedRow {
					t.Fatal("agent_end arrived without an aborted message_end")
				}
				return
			}
		case <-timeout:
			t.Fatalf("timed out waiting for agent_end (started=%v abortedRow=%v)", started, abortedRow)
		}
	}
}

func eventName(event any) string {
	switch event.(type) {
	case protocol.AgentStartEvent:
		return protocol.TypeAgentStart
	case protocol.AgentEndEvent:
		return protocol.TypeAgentEnd
	case protocol.TurnStartEvent:
		return protocol.TypeTurnStart
	case protocol.TurnEndEvent:
		return protocol.TypeTurnEnd
	case protocol.MessageStartEvent:
		return protocol.TypeMessageStart
	case protocol.MessageUpdateEvent:
		return protocol.TypeMessageUpdate
	case protocol.MessageEndEvent:
		return protocol.TypeMessageEnd
	case protocol.ToolStartEvent:
		return protocol.TypeToolStart
	case protocol.ToolUpdateEvent:
		return protocol.TypeToolUpdate
	case protocol.ToolEndEvent:
		return protocol.TypeToolEnd
	case protocol.ErrorEvent:
		return protocol.TypeError
	case protocol.CompactionStartEvent:
		return protocol.TypeCompactionStart
	case protocol.CompactionEndEvent:
		return protocol.TypeCompactionEnd
	default:
		return fmt.Sprintf("%T", event)
	}
}
