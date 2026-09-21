// MemorySink unit tests: the details beyond the conformance suite — the
// drop-after-close behavior made observable, and the copy-out semantics of
// Events (mutating the returned slice must not corrupt the sink).
package telemetry

import (
	"reflect"
	"testing"

	"github.com/smallnest/pigo/internal/agentcore"
)

func TestMemorySinkObserveAndEvents(t *testing.T) {
	s := NewMemorySink()
	ev := agentcore.TelemetryEvent{
		Turns:           3,
		ToolDurationsMs: map[string]agentcore.ToolTiming{"read": {Count: 2, TotalMs: 50}},
	}
	s.Observe(ev)
	s.Observe(agentcore.TelemetryEvent{Turns: 1})

	got := s.Events()
	if len(got) != 2 || got[0].Turns != 3 || got[1].Turns != 1 {
		t.Fatalf("Events() = %+v, want the two observed events in order", got)
	}
	if !reflect.DeepEqual(got[0], ev) {
		t.Fatalf("first retained event = %+v, want %+v", got[0], ev)
	}
}

func TestMemorySinkEventsReturnsCopy(t *testing.T) {
	s := NewMemorySink()
	s.Observe(agentcore.TelemetryEvent{Turns: 1})

	got := s.Events()
	got[0].Turns = 99 // mutating the copy must not reach the sink

	if again := s.Events(); again[0].Turns != 1 {
		t.Fatalf("Events() must return a copy; sink now reports %+v", again[0])
	}
}

func TestMemorySinkCloseDropsLaterEvents(t *testing.T) {
	s := NewMemorySink()
	s.Observe(agentcore.TelemetryEvent{Turns: 1})
	if err := s.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}
	s.Observe(agentcore.TelemetryEvent{Turns: 2}) // dropped: sink is closed

	got := s.Events()
	if len(got) != 1 {
		t.Fatalf("retained %d events after close+observe, want 1 (post-close drops)", len(got))
	}
	if got[0].Turns != 1 {
		t.Fatalf("retained event = %+v, want the pre-close one", got[0])
	}
}

func TestMemorySinkImplementsRetentionSink(t *testing.T) {
	var _ RetentionSink = NewMemorySink()
	var _ Sink = NoopSink{}
}
