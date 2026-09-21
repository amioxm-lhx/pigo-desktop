// The JSONL backend's conformance run (plan phase 3b acceptance): the same
// storetest suite that gates the SQLite backend, executed against
// session.NewStore. It lives in package session_test (not session) because
// storetest imports session — an in-package test would form an import cycle.
package session_test

import (
	"testing"

	"github.com/smallnest/pigo/internal/session"
	"github.com/smallnest/pigo/internal/session/storetest"
)

func TestJSONLStoreConformance(t *testing.T) {
	storetest.RunConformance(t, func(t *testing.T) session.Store {
		t.Helper()
		s, err := session.NewStore(t.TempDir())
		if err != nil {
			t.Fatalf("NewStore: %v", err)
		}
		return s
	})
}
