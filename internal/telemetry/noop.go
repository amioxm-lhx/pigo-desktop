// NoopSink is the zero-cost Sink: it accepts every event and discards it.
// It is the "off" position of the telemetry contract — handy as an explicit
// default in wiring code that prefers a non-nil sink, and as the baseline a
// conformance run always passes (telemetry/testing).
package telemetry

import (
	"github.com/smallnest/pigo/internal/agentcore"
)

// NoopSink discards observed events. It implements Sink with neither
// retention nor side effects; use it where a Sink value is required but
// metrics should go nowhere.
type NoopSink struct{}

// Observe implements Sink; the zero value is ready to use.
func (NoopSink) Observe(agentcore.TelemetryEvent) {}

// Close is a no-op and always succeeds. It is idempotent by construction.
func (NoopSink) Close() error { return nil }
