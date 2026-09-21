//go:build windows

// Cross-process lease locking on Windows: flock(2) does not exist, and the
// LockFileEx plumbing is out of scope for this phase — the sidecar lock file
// is still created (so unix processes coexisting on a share see the same
// artifact) but always grants. The in-process lease layer still arbitrates
// stores within this process.
package session

import "os"

// tryFlock is a grant-always stub on Windows (see file comment).
func tryFlock(f *os.File) error { return nil }
