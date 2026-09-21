// This file is the workspace-boundary policy (the PathGuard): the single
// place every file tool's path-traversal guard lives. resolveWithin and
// resolveWithinAny moved here from search_tool.go unchanged (pure migration);
// PathGuard wraps the same policy in a runtime-mutable form — roots can be
// widened mid-run with AllowTree (the escape hatch for trusted out-of-tree
// directories, e.g. the compaction/memory root) and rejections name the
// allowed roots so the model can self-correct its path on the next attempt.
package agenttool

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"github.com/smallnest/pigo/internal/agentcore"
)

// resolveWithin resolves p against root and verifies it stays within it. It is
// the single workspace-boundary policy shared by every file tool: the search
// tools call it directly, and ReadTool/WriteTool/EditTool.resolvePath delegate
// to it, so the path-traversal guard lives in exactly one place.
func resolveWithin(root, p string) (string, error) {
	if root == "" {
		wd, err := os.Getwd()
		if err != nil {
			return "", fmt.Errorf("cannot determine working directory: %w", err)
		}
		root = wd
	}
	absRoot, err := filepath.Abs(root)
	if err != nil {
		return "", fmt.Errorf("invalid root: %w", err)
	}
	var full string
	if filepath.IsAbs(p) {
		full = filepath.Clean(p)
	} else {
		full = filepath.Join(absRoot, p)
	}
	rel, err := filepath.Rel(absRoot, full)
	if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return "", fmt.Errorf("path %q is outside the workspace root", p)
	}
	return full, nil
}

// resolveWithinAny resolves p against the first root that contains it, trying
// roots in order. It exists so the file tools can additionally permit trusted
// out-of-workspace roots (the skills directory) whose absolute SKILL.md paths
// pigo itself advertises in the system prompt: without this, the workspace guard
// would reject the very paths the model is instructed to read or author. Empty
// roots are skipped; if none contain p, the standard workspace-escape error is
// returned.
func resolveWithinAny(roots []string, p string) (string, error) {
	var lastErr error
	for _, root := range roots {
		if root == "" {
			continue
		}
		full, err := resolveWithin(root, p)
		if err == nil {
			return full, nil
		}
		lastErr = err
	}
	if lastErr != nil {
		return "", lastErr
	}
	// No usable roots supplied: fall back to the default (cwd) policy.
	return resolveWithin("", p)
}

// PathGuard is the runtime-mutable workspace boundary. It applies the same
// resolveWithinAny policy over its root set, with two additions over the
// static tool fields (Root/ExtraRoots):
//
//   - AllowTree widens the root set mid-run (the escape hatch: a trusted
//     out-of-tree directory the system itself needs the model to reach, such
//     as the memory/compaction root, can be admitted without rebuilding the
//     tool set).
//   - Guard rejects with the allowed roots named, so the model sees which
//     directory to anchor its path against instead of a bare "outside the
//     workspace root".
//
// A PathGuard is safe for concurrent use. It is deliberately an observable
// boundary, not a sandbox: the bash tool guards only the cd targets and
// redirection paths it can extract from a command (best effort).
type PathGuard struct {
	mu    sync.RWMutex
	roots []string
}

// NewPathGuard builds a guard admitting the given roots, in priority order.
// An empty root list behaves like the static tools' empty Root: the current
// working directory is the boundary.
func NewPathGuard(roots ...string) *PathGuard {
	return &PathGuard{roots: append([]string(nil), roots...)}
}

// AllowTree widens the guard with additional trusted root trees at runtime.
// Duplicates (exact string matches) are ignored. Empty roots are dropped.
func (g *PathGuard) AllowTree(roots ...string) {
	g.mu.Lock()
	defer g.mu.Unlock()
	for _, root := range roots {
		if strings.TrimSpace(root) == "" {
			continue
		}
		duplicate := false
		for _, existing := range g.roots {
			if existing == root {
				duplicate = true
				break
			}
		}
		if !duplicate {
			g.roots = append(g.roots, root)
		}
	}
}

// Roots returns a snapshot of the current root set (in priority order).
func (g *PathGuard) Roots() []string {
	g.mu.RLock()
	defer g.mu.RUnlock()
	return append([]string(nil), g.roots...)
}

// Guard resolves p against the first root that contains it (resolveWithinAny
// semantics). A path outside every root is rejected with the allowed roots
// named, so the rejection is self-correcting: the model can anchor its next
// attempt against the directory the message shows.
func (g *PathGuard) Guard(p string) (string, error) {
	g.mu.RLock()
	roots := append([]string(nil), g.roots...)
	g.mu.RUnlock()

	full, err := resolveWithinAny(roots, p)
	if err != nil {
		return "", fmt.Errorf("path %q is outside the workspace root%s %s", p, pluralRoots(roots), formatRootList(roots))
	}
	return full, nil
}

// pluralRoots returns the plural marker for the root list ("s" when more than
// one root is admissible).
func pluralRoots(roots []string) string {
	if len(roots) > 1 {
		return "s"
	}
	return ""
}

// formatRootList renders the admissible roots for a rejection message:
// "/a" for one, "[/a, /b]" for several, "" when the set is empty (the guard
// then falls back to the cwd policy, and the cwd is named by the inner
// resolveWithin error).
func formatRootList(roots []string) string {
	if len(roots) == 0 {
		return ""
	}
	if len(roots) == 1 {
		return roots[0]
	}
	return "[" + strings.Join(roots, ", ") + "]"
}

// guardOrRoots is the shared resolvePath seam for the file tools: a non-nil
// guard is the live boundary; otherwise the static root (+ extra roots)
// applies exactly as before the guard existed, so callers that do not wire a
// PathGuard keep byte-identical behavior.
func guardOrRoots(guard *PathGuard, root string, extras []string, p string) (string, error) {
	if guard != nil {
		return guard.Guard(p)
	}
	if len(extras) == 0 {
		return resolveWithin(root, p)
	}
	return resolveWithinAny(append([]string{root}, extras...), p)
}

// GuardFromTools returns the shared PathGuard wired into a tool set, or nil
// when no tool carries one (e.g. tools are disabled). It mirrors
// MemoryRootFromTools: run assembly reaches back into the built-in tool set
// to register trusted trees (AllowTree) on the guard the file tools actually
// enforce, rather than keeping a parallel copy of the root set.
func GuardFromTools(tools []agentcore.AgentTool) *PathGuard {
	for _, t := range tools {
		switch tool := t.(type) {
		case *ReadTool:
			if tool.Guard != nil {
				return tool.Guard
			}
		case *WriteTool:
			if tool.Guard != nil {
				return tool.Guard
			}
		case *BashTool:
			if tool.Guard != nil {
				return tool.Guard
			}
		}
	}
	return nil
}
