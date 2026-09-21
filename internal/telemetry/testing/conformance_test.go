// Conformance wiring: both shipped implementations (NoopSink and MemorySink)
// must pass the suite. This doubles as the suite's own smoke test — a
// regression here means the suite, not a backend, changed behavior.
package testing

import (
	"testing"

	"github.com/smallnest/pigo/internal/telemetry"
)

func TestNoopSinkConformance(t *testing.T) {
	RunConformance(t, func() telemetry.Sink { return telemetry.NoopSink{} })
}

func TestMemorySinkConformance(t *testing.T) {
	RunConformance(t, func() telemetry.Sink { return telemetry.NewMemorySink() })
}
