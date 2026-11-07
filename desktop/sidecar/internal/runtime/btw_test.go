package runtime

import (
	"context"
	"encoding/json"
	"io"
	"log"
	"strings"
	"sync"
	"testing"

	"github.com/smallnest/pigo/desktop/sidecar/internal/jsonrpc"
	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
)

type btwTestEmitter struct {
	mu        sync.Mutex
	envelopes []protocol.EventEnvelope
}

func (e *btwTestEmitter) Emit(envelope protocol.EventEnvelope) error {
	e.mu.Lock()
	defer e.mu.Unlock()
	e.envelopes = append(e.envelopes, envelope)
	return nil
}

func (e *btwTestEmitter) all() []protocol.EventEnvelope {
	e.mu.Lock()
	defer e.mu.Unlock()
	return append([]protocol.EventEnvelope(nil), e.envelopes...)
}

// btwTestHost records every host call. A side question must not open a turn,
// append a message or install a checkpoint, so most of these tests assert
// that the list stays empty.
type btwTestHost struct {
	mu    sync.Mutex
	calls []string
}

func (h *btwTestHost) Call(_ context.Context, method string, _ any) (json.RawMessage, error) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.calls = append(h.calls, method)
	return json.RawMessage(`{}`), nil
}

func (h *btwTestHost) CallInto(_ context.Context, method string, _ any, _ any) error {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.calls = append(h.calls, method)
	return nil
}

func (h *btwTestHost) all() []string {
	h.mu.Lock()
	defer h.mu.Unlock()
	return append([]string(nil), h.calls...)
}

func btwTestRunner() (*SideRunner, *btwTestEmitter, *btwTestHost) {
	host := &btwTestHost{}
	emitter := &btwTestEmitter{}
	runner := NewSideRunner(host, log.New(io.Discard, "", 0))
	runner.SetEmitter(emitter)
	return runner, emitter, host
}

func btwTestAssistant(text string) agentcore.AssistantMessage {
	return agentcore.AssistantMessage{
		RoleField:  agentcore.RoleAssistant,
		Content:    agentcore.ContentList{agentcore.NewTextContent(text)},
		StopReason: agentcore.StopReasonEndTurn,
	}
}

func TestBtwAskRejectsIncompleteParamsWithoutHostCalls(t *testing.T) {
	cases := []struct {
		name   string
		params protocol.BtwParams
	}{
		{
			name:   "missing session id",
			params: protocol.BtwParams{Question: "what is this?"},
		},
		{
			name:   "missing question",
			params: protocol.BtwParams{SessionID: "session-1"},
		},
		{
			name: "unconfigured model",
			params: protocol.BtwParams{
				SessionID: "session-1",
				Question:  "what is this?",
			},
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			runner, emitter, host := btwTestRunner()
			if _, err := runner.Ask(context.Background(), tc.params); err == nil {
				t.Fatal("invalid /btw request must be rejected")
			}
			if calls := host.all(); len(calls) != 0 {
				t.Fatalf("rejected request touched the host: %v", calls)
			}
			if events := emitter.all(); len(events) != 0 {
				t.Fatalf("rejected request emitted %d events", len(events))
			}
			if got := runner.Count(); got != 0 {
				t.Fatalf("runner still tracks %d side runs", got)
			}
		})
	}
}

// The isolation invariant: every /btw envelope is marked side-scoped, carries
// its own side id in both the envelope and the payload, and never uses one of
// the transcript event types the shell persists.
func TestBtwEventsAreScopedRendererOnly(t *testing.T) {
	runner, emitter, _ := btwTestRunner()
	mapper := &btwMapper{runner: runner, sessionID: "session-1", sideID: "side-1"}

	mapper.onStart(agentcore.MessageStartEvent{Message: btwTestAssistant("")})
	mapper.onUpdate(agentcore.MessageUpdateEvent{Message: btwTestAssistant("Hello")})
	mapper.onUpdate(agentcore.MessageUpdateEvent{Message: btwTestAssistant("Hello world")})
	mapper.onEnd(agentcore.MessageEndEvent{Message: btwTestAssistant("Hello world")})

	events := emitter.all()
	if len(events) != 3 {
		t.Fatalf("got %d side events, want 3", len(events))
	}
	for i, envelope := range events {
		if envelope.Scope != EventScopeSide {
			t.Errorf("event %d scope = %q, want %q", i, envelope.Scope, EventScopeSide)
		}
		if envelope.SideID != "side-1" || envelope.TurnID != "side-1" {
			t.Errorf("event %d ids = sideId %q turnId %q", i, envelope.SideID, envelope.TurnID)
		}
		if envelope.SessionID != "session-1" {
			t.Errorf("event %d session = %q", i, envelope.SessionID)
		}
		switch envelope.Event.(type) {
		case protocol.BtwDeltaEvent, protocol.BtwEndEvent, protocol.BtwErrorEvent, protocol.BtwStartEvent:
		default:
			t.Errorf("event %d leaked a transcript event type: %T", i, envelope.Event)
		}
	}
	first, ok := events[0].Event.(protocol.BtwDeltaEvent)
	if !ok || first.Text != "Hello" {
		t.Fatalf("first delta = %#v", events[0].Event)
	}
	second, ok := events[1].Event.(protocol.BtwDeltaEvent)
	if !ok || second.Text != " world" || second.Reset {
		t.Fatalf("second delta = %#v", events[1].Event)
	}
	last, ok := events[2].Event.(protocol.BtwEndEvent)
	if !ok || last.Text != "Hello world" || last.Aborted {
		t.Fatalf("terminal event = %#v", events[2].Event)
	}
	if !mapper.terminal {
		t.Fatal("a finished side answer must be terminal")
	}
	// A late turn_end must not reopen a closed side thread.
	mapper.onTurnEnd(agentcore.TurnEndEvent{Message: btwTestAssistant("ignored")})
	if len(emitter.all()) != 3 {
		t.Fatal("late events reopened a terminal side thread")
	}
}

func TestBtwSystemPromptHasNoToolSurface(t *testing.T) {
	params := protocol.PromptParams{
		ProjectPath:         "/tmp/workspace",
		ProjectInstructions: json.RawMessage(`{"entries":[{"source":"AGENTS.md","content":"Use tabs."}]}`),
		ProjectMemory:       "prefers dark mode",
	}
	prompt := btwSystemPrompt(params)
	for _, want := range []string{
		"No tools are available",
		"never added to the main conversation",
		"/tmp/workspace",
		"Use tabs.",
		"prefers dark mode",
	} {
		if !strings.Contains(prompt, want) {
			t.Errorf("system prompt missing %q:\n%s", want, prompt)
		}
	}
}

func TestBtwCancelStopsOnlyItsOwnRun(t *testing.T) {
	runner, _, _ := btwTestRunner()
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	runner.runs["side-1"] = cancel
	if runner.Count() != 1 {
		t.Fatalf("count = %d, want 1", runner.Count())
	}
	if !runner.Cancel("side-1") {
		t.Fatal("Cancel must report a known side run")
	}
	if ctx.Err() == nil {
		t.Fatal("Cancel did not cancel the run context")
	}
	if runner.Cancel("unknown") {
		t.Fatal("Cancel must not report an unknown side run")
	}
}

// The Rust shell builds a wrapped request; this guards the field names both
// sides agreed on so a rename cannot silently strip the provider binding.
func TestBtwParamsDecodeFromShellRequest(t *testing.T) {
	raw := json.RawMessage(`{
		"sessionId": "session-1",
		"sideId": "side-1",
		"question": "why did we fork?",
		"binding": {
			"sessionId": "session-1",
			"turnId": "",
			"content": "",
			"userMessageId": "",
			"projectPath": "/tmp/workspace",
			"provider": {"id": "openai", "modelId": "gpt-test", "authKind": "none"}
		}
	}`)
	parsed, err := jsonrpc.Decode[protocol.BtwParams](raw)
	if err != nil {
		t.Fatalf("Decode: %v", err)
	}
	if parsed.SessionID != "session-1" || parsed.SideID != "side-1" || parsed.Question != "why did we fork?" {
		t.Fatalf("decoded params = %+v", parsed)
	}
	if parsed.Binding.Provider.ModelID != "gpt-test" {
		t.Fatalf("binding model = %q", parsed.Binding.Provider.ModelID)
	}
	if parsed.Binding.TurnID != "" || parsed.Binding.Content != "" {
		t.Fatal("a side question must not carry a durable turn")
	}
}
