// Package testing provides the Sink conformance suite: a battery of
// behavioral checks every telemetry backend must pass, so new backends
// (OTEL, file, self-hosted) can be validated by one line of test wiring
// instead of re-deriving the contract by hand.
//
// The suite is split into two tiers:
//
//   - Universal checks (CloseIdempotent, ObserveAfterCloseSafe,
//     ConcurrentObserveSafe) run for every Sink: not panicking, closing
//     cleanly and surviving concurrency are contract-level properties, so
//     even a drop-everything sink like NoopSink passes them.
//   - Retention checks (Ordering, FieldFidelity, NoLossUnderConcurrency) run
//     only when the sink implements telemetry.RetentionSink, because only a
//     retaining sink can exhibit ordering, fidelity and losslessness.
//
// Usage from a backend's own test:
//
//	func TestConformance(t *testing.T) {
//	    testing.RunConformance(t, func() telemetry.Sink { return NewMySink() })
//	}
//
// The factory builds a fresh, isolated sink per check (subtests get
// independent instances), so no shared state leaks between checks.
package testing

import (
	"reflect"
	"sync"
	"testing"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/telemetry"
)

// RunConformance drives the Sink conformance suite against a factory that
// builds one fresh, isolated sink per check. Every implementation of
// telemetry.Sink must pass; implementations that also implement
// telemetry.RetentionSink are additionally checked for ordering, field
// fidelity and losslessness.
func RunConformance(t *testing.T, newSink func() telemetry.Sink) {
	t.Helper()

	t.Run("CloseIdempotent", func(t *testing.T) {
		s := newSink()
		if err := s.Close(); err != nil {
			t.Fatalf("first Close: %v", err)
		}
		for i := 0; i < 3; i++ {
			if err := s.Close(); err != nil {
				t.Fatalf("Close call %d after close must return nil, got %v", i+2, err)
			}
		}
	})

	t.Run("ObserveAfterCloseSafe", func(t *testing.T) {
		s := newSink()
		if err := s.Close(); err != nil {
			t.Fatalf("Close: %v", err)
		}
		// Must not panic. Whether the event is dropped or retained is an
		// implementation choice; the contract only requires the call to stay
		// safe after Close.
		s.Observe(sampleEvent(1))
	})

	t.Run("ConcurrentObserveSafe", func(t *testing.T) {
		s := newSink()
		var wg sync.WaitGroup
		const workers, perWorker = 4, 25
		for w := 0; w < workers; w++ {
			wg.Add(1)
			go func() {
				defer wg.Done()
				for i := 0; i < perWorker; i++ {
					s.Observe(sampleEvent(i))
				}
			}()
		}
		wg.Wait()
		if err := s.Close(); err != nil {
			t.Fatalf("Close after concurrent Observe: %v", err)
		}
	})

	// Retention checks: only for sinks that keep what they observe.
	if _, ok := newSink().(telemetry.RetentionSink); !ok {
		return
	}

	t.Run("Ordering", func(t *testing.T) {
		s := newSink()
		want := []agentcore.TelemetryEvent{sampleEvent(1), sampleEvent(2), sampleEvent(3)}
		for _, ev := range want {
			s.Observe(ev)
		}
		got := s.(telemetry.RetentionSink).Events()
		if !reflect.DeepEqual(got, want) {
			t.Fatalf("retained events out of order or altered:\n got %+v\nwant %+v", got, want)
		}
	})

	t.Run("FieldFidelity", func(t *testing.T) {
		s := newSink()
		ev := agentcore.TelemetryEvent{
			Turns:              7,
			ToolDurationsMs:    map[string]agentcore.ToolTiming{"write": {Count: 3, TotalMs: 1234}},
			TruncationCount:    2,
			CompactionCount:    1,
			ContextUtilization: 0.5,
			ContextTokens:      4096,
			ContextWindow:      8192,
		}
		s.Observe(ev)
		got := s.(telemetry.RetentionSink).Events()
		if len(got) != 1 {
			t.Fatalf("retained %d events, want 1", len(got))
		}
		if !reflect.DeepEqual(got[0], ev) {
			t.Fatalf("field fidelity broken:\n got %+v\nwant %+v", got[0], ev)
		}
	})

	t.Run("NoLossUnderConcurrency", func(t *testing.T) {
		s := newSink()
		var wg sync.WaitGroup
		const workers, perWorker = 4, 25
		for w := 0; w < workers; w++ {
			wg.Add(1)
			go func() {
				defer wg.Done()
				for i := 0; i < perWorker; i++ {
					s.Observe(sampleEvent(i))
				}
			}()
		}
		wg.Wait()
		if got := len(s.(telemetry.RetentionSink).Events()); got != workers*perWorker {
			t.Fatalf("retained %d events, want %d (loss under concurrency)", got, workers*perWorker)
		}
	})
}

// sampleEvent builds a distinguishable event for ordering/loss checks: the
// turn count encodes the emission index so a misordered or dropped event is
// visible in the retained slice.
func sampleEvent(i int) agentcore.TelemetryEvent {
	return agentcore.TelemetryEvent{Turns: i + 1}
}
