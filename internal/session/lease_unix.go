//go:build !windows

// Cross-process lease locking via flock(2) (BSD/macOS/Linux): the sidecar
// lock file is locked exclusively, non-blocking, for the lease's lifetime —
// the fd stays open until ReleaseSession (or process exit, when the kernel
// drops the lock automatically).
package session

import (
	"fmt"
	"os"
	"syscall"
)

// tryFlock takes an exclusive, non-blocking advisory lock on f. It returns
// an error (wrapped errno) when another holder — in this or any other
// process — already holds it.
func tryFlock(f *os.File) error {
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		return fmt.Errorf("flock: %w", err)
	}
	return nil
}
