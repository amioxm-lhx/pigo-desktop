package runtime

import (
	"fmt"
	"path/filepath"

	"github.com/smallnest/pigo/internal/dream"
	"github.com/smallnest/pigo/internal/memory"
)

// MemoryReport is the read-only answer to /memory: the persistent-memory store
// state plus the per-scope entry counts, rendered by the composer info card.
//
// Unlike the CLI's memstatus.RunMemory, this report carries no context-window or
// checkpoint section. The renderer already holds the live transcript, the
// compaction rows and the last reported usage, so those figures stay in the
// renderer and the sidecar only answers for what it alone can open: the memory
// index.
type MemoryReport struct {
	Enabled bool `json:"enabled"`
	// Root is the memory root directory (global/projects/sessions layout).
	Root string `json:"root,omitempty"`
	// DBPath is the SQLite index stored next to that root.
	DBPath string `json:"dbPath,omitempty"`
	// Counts is keyed by scope ("global" / "projects" / "sessions" / "cc").
	Counts map[string]int `json:"counts"`
	Total  int            `json:"total"`
	// Indexed and Pruned are the reconcile deltas applied before counting, so
	// the card can explain why the numbers moved.
	Indexed int `json:"indexed"`
	Pruned  int `json:"pruned"`
	// Error reports a non-fatal open/reconcile failure. The store section then
	// renders as unavailable instead of failing the whole request.
	Error string `json:"error,omitempty"`
}

// MemoryRootDir resolves the persistent-memory root exactly as the CLI and the
// dream runner do: $PIGO_HOME/memory, else ~/.pigo/memory. It returns "" when
// neither resolves.
func MemoryRootDir() string {
	return dream.ResolveMemoryRoot()
}

// BuildMemoryReport opens the memory index under root, reconciles the Markdown
// files into it, and returns the per-scope counts. Reconciling first matches the
// CLI's behaviour (counts reflect files on disk, not a stale index) and is
// idempotent, so a report never lies after an external file edit.
//
// An open failure is reported through the Error field, not as a returned error:
// /memory is a diagnostic, and "the index cannot be opened" is itself the
// diagnostic the user needs to see.
func BuildMemoryReport(root string) MemoryReport {
	if root == "" {
		root = MemoryRootDir()
	}
	report := MemoryReport{Enabled: true, Root: root, Counts: map[string]int{}}
	if root == "" {
		report.Enabled = false
		report.Error = "cannot resolve the memory root (no home directory and no PIGO_HOME)"
		return report
	}
	report.DBPath = filepath.Join(root, "index.db")

	store, err := memory.Open(report.DBPath, root, "")
	if err != nil {
		report.Error = err.Error()
		return report
	}
	defer func() { _ = store.Close() }()

	if reconciled, err := store.Reconcile(); err == nil {
		report.Indexed = reconciled.Indexed
		report.Pruned = reconciled.Pruned
	} else {
		report.Error = fmt.Sprintf("reconcile: %v", err)
	}

	counts, err := store.CountByScope()
	if err != nil {
		if report.Error == "" {
			report.Error = fmt.Sprintf("count: %v", err)
		}
		return report
	}
	for scope, n := range counts {
		report.Counts[string(scope)] = n
		report.Total += n
	}
	return report
}
