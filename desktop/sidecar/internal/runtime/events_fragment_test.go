package runtime

import (
	"context"
	"encoding/json"
	"io"
	"log"
	"sync"
	"testing"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
)

// startlessStream mimics an OpenAI-compatible gateway: deltas begin the
// response with no StreamStartEvent, and a tool call precedes the answer.
type startlessStream struct {
	mu    sync.Mutex
	calls int
}

func (s *startlessStream) Stream(ctx context.Context, _ string, _ provider.LlmContext, _ provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
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
				RoleField: agentcore.RoleAssistant,
				Content: agentcore.ContentList{
					agentcore.NewTextContent("先读一下文件"),
					agentcore.NewToolCallContent("call-1", "Read", json.RawMessage(`{"path":"notes.md"}`)),
				},
				StopReason: agentcore.StopReasonToolUse,
				Timestamp:  now,
			}
			_ = stream.Emit(ctx, provider.StreamToolCallEvent{Partial: partial})
			_ = stream.Emit(ctx, provider.StreamDoneEvent{Message: partial})
			return
		}
		final := agentcore.AssistantMessage{
			RoleField:  agentcore.RoleAssistant,
			Content:    agentcore.ContentList{agentcore.NewTextContent("文件里写的是 hello")},
			StopReason: agentcore.StopReasonEndTurn,
			Timestamp:  now,
		}
		_ = stream.Emit(ctx, provider.StreamTextEvent{Partial: final})
		_ = stream.Emit(ctx, provider.StreamDoneEvent{Message: final})
	}()

	return stream, nil
}

// A turn is several assistant messages, one per model response. The sidecar
// synthesizes the opening row when the provider sends no start event, and that
// row must carry a new id: the host appends by id, so reusing the id of the
// fragment that just closed drops the newer snapshot on reload while the
// renderer overwrites the older row instead of showing a new one.
func TestStartlessProviderKeepsEachFragmentOnItsOwnRow(t *testing.T) {
	host := &fakeHost{}
	emitter := &captureEmitter{ch: make(chan protocol.EventEnvelope, 128)}
	stream := &startlessStream{}
	manager := NewManager(Options{
		Host:     host,
		Emitter:  emitter,
		Logger:   log.New(io.Discard, "", 0),
		StreamFn: stream.Stream,
		Model:    provider.Model{Provider: "desktop", ID: "test-model"},
	})
	params := protocol.PromptParams{
		SessionID: "session-startless",
		Mode:      "agent",
		TurnID:    "turn-startless",
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
	if err := session.Prompt("turn-startless", "user-1", "read notes"); err != nil {
		t.Fatalf("Prompt: %v", err)
	}

	ended := map[string]string{}
	var order []string
	timeout := time.After(3 * time.Second)
	for done := false; !done; {
		select {
		case envelope := <-emitter.ch:
			switch event := envelope.Event.(type) {
			case protocol.MessageEndEvent:
				if _, seen := ended[event.Message.ID]; !seen {
					order = append(order, event.Message.ID)
				}
				ended[event.Message.ID] = event.Message.Content
			case protocol.AgentEndEvent:
				done = true
			}
		case <-timeout:
			t.Fatalf("timed out waiting for agent_end; rows so far: %v", ended)
		}
	}

	if len(order) != 2 {
		t.Fatalf("assistant rows = %d (%v), want one per model response", len(order), ended)
	}
	if order[0] == order[1] {
		t.Fatalf("both fragments share id %q; the second one would be dropped", order[0])
	}
	if got := ended[order[0]]; got != "先读一下文件" {
		t.Errorf("first row content = %q, want the narration", got)
	}
	if got := ended[order[1]]; got != "文件里写的是 hello" {
		t.Errorf("second row content = %q, want the answer", got)
	}
}
