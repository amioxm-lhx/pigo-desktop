package runtime

import (
	"encoding/json"
	"testing"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
)

// TestSubagentEventMapperStampsDelegationAttribution pins the contract the
// transcript relies on (D201, ADR 0062): a delegate's rows cross the event
// stream attributed to the Task call that spawned them, and its lifecycle
// events stay out of the parent turn.
func TestSubagentEventMapperStampsDelegationAttribution(t *testing.T) {
	emitter := &captureEmitter{ch: make(chan protocol.EventEnvelope, 16)}
	session := subagentTestSession((&scriptedStream{}).Stream, nil)
	session.emitter = emitter
	mapper := newSubagentEventMapper(session, "turn-1", "task-call-1", "explorer")

	now := time.Now().UnixMilli()
	assistant := agentcore.AssistantMessage{
		RoleField:  agentcore.RoleAssistant,
		Content:    agentcore.ContentList{agentcore.NewTextContent("delegate answer")},
		StopReason: agentcore.StopReasonEndTurn,
		Timestamp:  now,
	}
	mapper.handle(agentcore.MessageStartEvent{Message: assistant})
	mapper.handle(agentcore.MessageEndEvent{Message: assistant})
	mapper.handle(agentcore.ToolExecutionStartEvent{
		ToolCallID: "read-1",
		ToolName:   "Read",
		Args:       map[string]any{"path": "notes.md"},
	})
	mapper.handle(agentcore.ToolExecutionEndEvent{
		ToolCallID: "read-1",
		ToolName:   "Read",
		Result: agentcore.AgentToolResult{
			Content: agentcore.ContentList{agentcore.NewTextContent("notes")},
		},
	})
	// Parent turn lifecycle belongs to the parent; nothing here may leak into it.
	mapper.handle(agentcore.TurnStartEvent{})
	mapper.handle(agentcore.AgentStartEvent{})
	mapper.handle(agentcore.AgentEndEvent{})

	envelopes := drainEnvelopes(emitter)
	if len(envelopes) != 4 {
		t.Fatalf("delegate events = %d, want 4: %#v", len(envelopes), envelopes)
	}
	for _, envelope := range envelopes {
		if envelope.ParentToolCallID != "task-call-1" || envelope.AgentName != "explorer" {
			t.Fatalf("envelope attribution = %q/%q", envelope.ParentToolCallID, envelope.AgentName)
		}
		if envelope.TurnID != "turn-1" {
			t.Fatalf("envelope turn = %q", envelope.TurnID)
		}
	}

	end, ok := envelopes[1].Event.(protocol.MessageEndEvent)
	if !ok {
		t.Fatalf("second delegate event = %T", envelopes[1].Event)
	}
	if end.Message.Role != protocol.RoleAssistant || end.Message.Content != "delegate answer" {
		t.Fatalf("delegate row = %#v", end.Message)
	}
	if end.Message.ParentToolCallID != "task-call-1" || end.Message.AgentName != "explorer" {
		t.Fatalf("message attribution = %q/%q", end.Message.ParentToolCallID, end.Message.AgentName)
	}

	start, ok := envelopes[2].Event.(protocol.ToolStartEvent)
	if !ok {
		t.Fatalf("third delegate event = %T", envelopes[2].Event)
	}
	if start.ToolName != "Read" || start.ToolCallID != "read-1" {
		t.Fatalf("delegate tool start = %#v", start)
	}
	var args map[string]string
	if err := json.Unmarshal(start.Args, &args); err != nil || args["path"] != "notes.md" {
		t.Fatalf("delegate tool args = %s", start.Args)
	}

	toolEnd, ok := envelopes[3].Event.(protocol.ToolEndEvent)
	if !ok {
		t.Fatalf("fourth delegate event = %T", envelopes[3].Event)
	}
	if toolEnd.ToolCallID != "read-1" || toolEnd.IsError {
		t.Fatalf("delegate tool end = %#v", toolEnd)
	}
}

func drainEnvelopes(emitter *captureEmitter) []protocol.EventEnvelope {
	var envelopes []protocol.EventEnvelope
	for {
		select {
		case envelope := <-emitter.ch:
			envelopes = append(envelopes, envelope)
		default:
			return envelopes
		}
	}
}
