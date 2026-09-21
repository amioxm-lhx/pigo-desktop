// Tests for the run-end telemetry sink delivery (plan phase 1c): a registered
// Sink receives exactly the same TelemetryEvent the event stream carries just
// before agent_end, while the default (nil sink) path is unchanged.
package runtime

import (
	"context"
	"reflect"
	"testing"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
	"github.com/smallnest/pigo/internal/telemetry"
)

// runWithSink drives a single-turn run (no tool calls) with the given sink
// registered, returning the stream-carried TelemetryEvent and the sink's
// retained events.
func runWithSink(t *testing.T, sink telemetry.Sink) (agentcore.TelemetryEvent, []agentcore.TelemetryEvent) {
	t.Helper()
	final := agentcore.AssistantMessage{
		RoleField:  agentcore.RoleAssistant,
		Content:    agentcore.ContentList{agentcore.NewTextContent("done")},
		StopReason: agentcore.StopReasonEndTurn,
	}
	cfg := RunConfig{
		LoopConfig: LoopConfig{
			Model: "fake",
			Stream: fakeStream([]provider.AssistantMessageEvent{
				provider.StreamStartEvent{},
				provider.StreamDoneEvent{Message: final},
			}),
		},
		TelemetrySink: sink,
	}
	agentCtx := &agentcore.AgentContext{Messages: agentcore.MessageList{
		agentcore.UserMessage{RoleField: agentcore.RoleUser, Content: agentcore.ContentList{agentcore.NewTextContent("hi")}},
	}}

	var streamEvent agentcore.TelemetryEvent
	sawTelemetry := false
	stream := StartRun(context.Background(), agentCtx, cfg)
	if _, err := DrainStream(context.Background(), stream, StreamHandler{
		OnEvent: func(ev agentcore.AgentEvent) {
			if te, ok := ev.(agentcore.TelemetryEvent); ok {
				streamEvent = te
				sawTelemetry = true
			}
		},
	}); err != nil {
		t.Fatalf("DrainStream: %v", err)
	}
	if !sawTelemetry {
		t.Fatal("run did not emit a TelemetryEvent on the stream")
	}

	var retained []agentcore.TelemetryEvent
	if rs, ok := sink.(telemetry.RetentionSink); ok {
		retained = rs.Events()
	}
	return streamEvent, retained
}

func TestTelemetrySinkReceivesRunSummary(t *testing.T) {
	sink := telemetry.NewMemorySink()
	streamEvent, retained := runWithSink(t, sink)

	if len(retained) != 1 {
		t.Fatalf("sink retained %d events, want exactly 1 per run", len(retained))
	}
	if !reflect.DeepEqual(retained[0], streamEvent) {
		t.Fatalf("sink event differs from the stream-carried event:\n sink %+v\nstream %+v", retained[0], streamEvent)
	}
	if retained[0].Turns != 1 {
		t.Fatalf("sink event Turns = %d, want 1 (single-turn run)", retained[0].Turns)
	}
}

func TestTelemetrySinkDefaultNilUnchanged(t *testing.T) {
	// The default (nil) path must keep working — this is the zero-behavior-
	// change guarantee every existing run already relies on.
	streamEvent, _ := runWithSink(t, nil)
	if streamEvent.Turns != 1 {
		t.Fatalf("stream telemetry Turns = %d, want 1", streamEvent.Turns)
	}
}

func TestTelemetrySinkNoopExplicitlyWired(t *testing.T) {
	// Wiring telemetry.NoopSink explicitly must behave like nil: the stream
	// still carries the event, nothing breaks.
	streamEvent, _ := runWithSink(t, telemetry.NoopSink{})
	if streamEvent.Turns != 1 {
		t.Fatalf("stream telemetry Turns = %d, want 1", streamEvent.Turns)
	}
}
