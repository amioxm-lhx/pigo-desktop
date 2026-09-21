// Package telemetry defines the pluggable backend contract for run-level
// telemetry (observability). The runtime already surfaces structured metrics
// as an agentcore.TelemetryEvent on the event stream just before agent_end;
// this package adds the second, backend-facing half: a Sink receives that
// same event so metrics can be exported to any destination (in-process
// dashboard, file, OTEL collector, ...) without the runtime knowing about
// any of them.
//
// The package depends only on agentcore (the shared event vocabulary) — no
// third-party telemetry SDK — so wiring it in is always cheap. The
// agentcore.TelemetryEvent shape is the single source of truth: sinks must
// not assume a different schema, and the frozen runtime.TelemetrySummary
// export document continues to derive from the same event unchanged.
package telemetry

import (
	"github.com/smallnest/pigo/internal/agentcore"
)

// Sink receives run-level telemetry events. Observe is called by the runtime
// once per run, with the same agentcore.TelemetryEvent the event stream
// carries; it is synchronous and must not block for long (the run's finish
// path calls it before closing the stream), so slow backends should buffer
// internally and flush on Close. Observe never returns an error: delivery
// problems are the sink's own concern (typically logged, never propagated —
// telemetry must not change run behavior). Implementations must be safe for
// concurrent use.
//
// Close releases the sink's resources. It must be idempotent: calling it
// more than once returns a nil error and does not panic. After Close,
// Observe must remain safe to call (the event may be dropped); retained
// events stay readable on sinks that expose them.
type Sink interface {
	Observe(ev agentcore.TelemetryEvent)
	Close() error
}

// RetentionSink is the optional capability interface for sinks that retain
// observed events in memory. The conformance suite (telemetry/testing) uses
// it, when implemented, to additionally verify ordering, field fidelity and
// losslessness — the properties only a retaining sink can exhibit. Sinks that
// forward-and-drop (like NoopSink) simply do not implement it.
type RetentionSink interface {
	Sink

	// Events returns a copy of the events observed (and retained) so far, in
	// observation order.
	Events() []agentcore.TelemetryEvent
}
