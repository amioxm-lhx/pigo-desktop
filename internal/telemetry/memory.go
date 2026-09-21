// MemorySink is the reference Sink implementation: it retains every observed
// event in memory (in observation order) so tests, examples and tooling can
// inspect exactly what the runtime delivered. It is deliberately boring —
// correct locking, copy-out reads, idempotent Close, drop-after-close — so
// real backends can copy its shape instead of re-deriving the contract, and
// telemetry/testing can hold it up as the canonical conformance target.
package telemetry

import (
	"sync"

	"github.com/smallnest/pigo/internal/agentcore"
)

// MemorySink retains observed telemetry events in memory. The zero value is
// ready to use; use NewMemorySink for a fresh instance. It implements
// RetentionSink.
type MemorySink struct {
	mu     sync.Mutex
	events []agentcore.TelemetryEvent
	closed bool
}

// NewMemorySink returns an empty, open MemorySink.
func NewMemorySink() *MemorySink {
	return &MemorySink{}
}

// Observe appends the event. After Close the event is dropped (the sink is
// closed, not broken: the call stays panic-free per the Sink contract). The
// map field is shared by reference with the caller — the runtime always
// passes a freshly materialized event whose map is a copy of the
// accumulator's state, so no defensive copy is taken here; retainers that
// mutate observed events elsewhere must copy on their side.
func (s *MemorySink) Observe(ev agentcore.TelemetryEvent) {
	if s == nil {
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return
	}
	s.events = append(s.events, ev)
}

// Events returns a copy of the retained events in observation order. It is
// safe to call concurrently with Observe and after Close; the retained
// events stay readable once the sink is closed.
func (s *MemorySink) Events() []agentcore.TelemetryEvent {
	if s == nil {
		return nil
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	out := make([]agentcore.TelemetryEvent, len(s.events))
	copy(out, s.events)
	return out
}

// Close marks the sink closed. It is idempotent: repeated calls return nil.
// Retained events remain available via Events.
func (s *MemorySink) Close() error {
	if s == nil {
		return nil
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	s.closed = true
	return nil
}
