// Tests for the PathGuard workspace boundary: multi-root resolution, runtime
// widening (AllowTree), self-correcting rejections that name the allowed
// roots, concurrency, and the bash tool's best-effort command guard.
package agenttool

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"

	"github.com/smallnest/pigo/internal/agentcore"
)

func TestPathGuardResolvesWithinRoots(t *testing.T) {
	root := t.TempDir()
	extra := t.TempDir()
	outside := t.TempDir()

	g := NewPathGuard(root, extra)
	if got := g.Roots(); len(got) != 2 || got[0] != root || got[1] != extra {
		t.Fatalf("roots = %v, want [%s %s]", got, root, extra)
	}

	// A relative path joins against the first root that contains it.
	full, err := g.Guard("notes.txt")
	if err != nil || full != filepath.Join(root, "notes.txt") {
		t.Fatalf("Guard(notes.txt) = %q, %v", full, err)
	}
	// An absolute path inside the extra root is admitted.
	full, err = g.Guard(filepath.Join(extra, "SKILL.md"))
	if err != nil || full != filepath.Join(extra, "SKILL.md") {
		t.Fatalf("Guard(extra file) = %q, %v", full, err)
	}
	// A relative path escapes when it walks out of every root.
	if _, err := g.Guard("../escape.txt"); err == nil {
		t.Fatal("Guard(../escape.txt) should reject")
	}
	// An absolute path in a third directory is rejected.
	if _, err := g.Guard(filepath.Join(outside, "x")); err == nil {
		t.Fatal("Guard(outside) should reject")
	}
}

func TestPathGuardRejectionNamesRoots(t *testing.T) {
	root := t.TempDir()
	extra := t.TempDir()

	single := NewPathGuard(root)
	_, err := single.Guard("../out.txt")
	if err == nil {
		t.Fatal("expected a rejection")
	}
	if !strings.Contains(err.Error(), root) {
		t.Fatalf("rejection should name the root %s: %v", root, err)
	}
	// The message keeps the historical "outside the workspace root" wording so
	// existing assertions (model-visible and test-side) stay compatible.
	if !strings.Contains(err.Error(), "outside the workspace root") {
		t.Fatalf("rejection lost the canonical wording: %v", err)
	}

	multi := NewPathGuard(root, extra)
	_, err = multi.Guard("../out.txt")
	if err == nil {
		t.Fatal("expected a rejection")
	}
	if !strings.Contains(err.Error(), root) || !strings.Contains(err.Error(), extra) {
		t.Fatalf("rejection should name both roots: %v", err)
	}
	if !strings.Contains(err.Error(), "["+root+", "+extra+"]") {
		t.Fatalf("multi-root rejection should list them: %v", err)
	}
}

func TestPathGuardAllowTreeWidensAtRuntime(t *testing.T) {
	root := t.TempDir()
	trusted := t.TempDir()

	g := NewPathGuard(root)
	target := filepath.Join(trusted, "checkpoint.md")
	if _, err := g.Guard(target); err == nil {
		t.Fatal("before AllowTree the trusted path must be rejected")
	}

	g.AllowTree(trusted)
	full, err := g.Guard(target)
	if err != nil || full != target {
		t.Fatalf("after AllowTree: %q, %v", full, err)
	}
	if got := g.Roots(); len(got) != 2 {
		t.Fatalf("roots = %v, want 2", got)
	}

	// Duplicates and empties are ignored; the root set stays tight.
	g.AllowTree(trusted, "", "   ")
	if got := g.Roots(); len(got) != 2 {
		t.Fatalf("roots after duplicate AllowTree = %v, want 2", got)
	}
}

func TestPathGuardConcurrentWidenAndGuard(t *testing.T) {
	root := t.TempDir()
	g := NewPathGuard(root)

	var wg sync.WaitGroup
	roots := make([]string, 8)
	for i := range roots {
		roots[i] = t.TempDir()
	}
	for i := 0; i < 8; i++ {
		wg.Add(2)
		go func() {
			defer wg.Done()
			g.AllowTree(roots...)
		}()
		go func() {
			defer wg.Done()
			_, _ = g.Guard("some/relative/path.txt") // any outcome; must not race
		}()
	}
	wg.Wait()
	if got := g.Roots(); len(got) != 9 {
		t.Fatalf("roots = %d entries, want 9 (root + 8)", len(got))
	}
}

func TestGuardOrRootsStaticFallback(t *testing.T) {
	root := t.TempDir()
	extra := t.TempDir()

	// nil guard + no extras: the original single-root behavior.
	full, err := guardOrRoots(nil, root, nil, "a.txt")
	if err != nil || full != filepath.Join(root, "a.txt") {
		t.Fatalf("static single root: %q, %v", full, err)
	}
	// nil guard + extras: the original resolveWithinAny behavior.
	full, err = guardOrRoots(nil, root, []string{extra}, filepath.Join(extra, "s.md"))
	if err != nil || full != filepath.Join(extra, "s.md") {
		t.Fatalf("static multi root: %q, %v", full, err)
	}
	// The old wording is preserved on the static path.
	_, err = guardOrRoots(nil, root, nil, "../x")
	if err == nil || !strings.Contains(err.Error(), "outside the workspace root") {
		t.Fatalf("static rejection wording changed: %v", err)
	}
}

func TestReadToolUsesSharedGuard(t *testing.T) {
	root := t.TempDir()
	trusted := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "in.txt"), []byte("inside"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(trusted, "SKILL.md"), []byte("skill"), 0o644); err != nil {
		t.Fatal(err)
	}

	g := NewPathGuard(root)
	tool := &ReadTool{Root: root, Guard: g}
	args, _ := json.Marshal(map[string]any{"path": "in.txt"})
	res, err := tool.Execute(context.Background(), "call-1", args, nil)
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}
	if !strings.Contains(resultText(res), "inside") {
		t.Fatalf("read inside root failed: %s", resultText(res))
	}

	// Before widening the trusted tree is out of bounds; after AllowTree the
	// same tool instance admits it — the point of the shared runtime guard.
	args, _ = json.Marshal(map[string]any{"path": filepath.Join(trusted, "SKILL.md")})
	res, _ = tool.Execute(context.Background(), "call-2", args, nil)
	if !strings.Contains(resultText(res), "outside the workspace root") {
		t.Fatalf("expected a boundary rejection, got: %s", resultText(res))
	}
	g.AllowTree(trusted)
	res, err = tool.Execute(context.Background(), "call-3", args, nil)
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}
	if !strings.Contains(resultText(res), "skill") {
		t.Fatalf("read after AllowTree failed: %s", resultText(res))
	}
}

func TestBashToolGuardBlocksEscapeAndNamesRoot(t *testing.T) {
	root := t.TempDir()
	g := NewPathGuard(root)
	tool := &BashTool{Dir: root, Guard: g}

	// A redirection outside the root is rejected before anything spawns, and
	// the message names the allowed root so the model can self-correct.
	for _, command := range []string{
		"echo hi > /tmp/leaked.txt",
		"echo hi >> /tmp/leaked.txt",
		"cat /etc/passwd > /tmp/leaked.txt",
		"cd /etc && cat passwd",
		"cd ../../outside",
		"some-cmd 2> /tmp/err.txt",
		"some-cmd < /etc/passwd",
	} {
		args, _ := json.Marshal(map[string]any{"command": command})
		res, err := tool.Execute(context.Background(), "call-1", args, nil)
		if err != nil {
			t.Fatalf("command %q: unexpected Go error: %v", command, err)
		}
		text := resultText(res)
		if !strings.Contains(text, "outside the workspace root") || !strings.Contains(text, root) {
			t.Fatalf("command %q should be rejected naming the root, got: %s", command, text)
		}
	}

	// /dev/null is idiomatic and never an exfiltration path: admitted.
	for _, command := range []string{
		"echo hi > /dev/null",
		"some-cmd 2> /dev/null",
		"some-cmd &> /dev/null",
	} {
		args, _ := json.Marshal(map[string]any{"command": command})
		res, err := tool.Execute(context.Background(), "call-2", args, nil)
		if err == nil && strings.Contains(resultText(res), "outside the workspace root") {
			t.Fatalf("command %q should pass the device-file allowance, got: %s", command, resultText(res))
		}
	}

	// Expansions and quoted paths cannot be resolved statically: they are not
	// guarded (best-effort boundary), so the command runs rather than being
	// wrongly rejected on an unresolvable token.
	for _, command := range []string{
		"echo hi > \"$OUT\"",
		"echo hi > ~/notes.txt",
		"cd $HOME",
	} {
		args, _ := json.Marshal(map[string]any{"command": command})
		res, err := tool.Execute(context.Background(), "call-3", args, nil)
		if err == nil && strings.Contains(resultText(res), "outside the workspace root") {
			t.Fatalf("command %q should not be statically guarded, got: %s", command, resultText(res))
		}
	}
}

func TestBashToolGuardWorksWithinRoot(t *testing.T) {
	root := t.TempDir()
	g := NewPathGuard(root)
	tool := &BashTool{Dir: root, Guard: g}

	args, _ := json.Marshal(map[string]any{"command": "echo pigo-guard-ok > out.txt && cat out.txt"})
	res, err := tool.Execute(context.Background(), "call-1", args, nil)
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}
	if !strings.Contains(resultText(res), "pigo-guard-ok") {
		t.Fatalf("in-root redirection failed: %s", resultText(res))
	}

	// cd into a subdirectory then write relatively: both resolve inside root.
	if err := os.Mkdir(filepath.Join(root, "sub"), 0o755); err != nil {
		t.Fatal(err)
	}
	args, _ = json.Marshal(map[string]any{"command": "cd sub && echo nested > inner.txt && cat inner.txt"})
	res, err = tool.Execute(context.Background(), "call-2", args, nil)
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}
	if !strings.Contains(resultText(res), "nested") {
		t.Fatalf("cd + write inside root failed: %s", resultText(res))
	}
}

func TestGuardableCommandPaths(t *testing.T) {
	cases := []struct {
		command string
		want    []string
	}{
		{"cd sub", []string{"sub"}},
		{"cd /etc", []string{"/etc"}},
		{"echo x > out.txt", []string{"out.txt"}},
		{"echo x >>out.txt", nil}, // no whitespace before the operand: not extracted
		{"cmd 2> err.log", []string{"err.log"}},
		{"cmd < in.txt", []string{"in.txt"}},
		{"cmd &> both.log", []string{"both.log"}},
		{"cd a && cd b", []string{"a", "b"}},
		{"cd \"my dir\"", nil}, // broken quoting: skipped
		{"cd $HOME", nil},
		{"cd ~", nil},
		{"cd -", nil},
		{"ls -la", nil},
	}
	for _, tc := range cases {
		got := guardableCommandPaths(tc.command)
		if fmt.Sprint(got) != fmt.Sprint(tc.want) {
			t.Errorf("guardableCommandPaths(%q) = %v, want %v", tc.command, got, tc.want)
		}
	}
}

func TestGuardFromTools(t *testing.T) {
	g := NewPathGuard("/w")
	tools := []agentcore.AgentTool{
		&GrepTool{Root: "/w"}, // no guard: skipped
		&ReadTool{Root: "/w", Guard: g},
	}
	if got := GuardFromTools(tools); got != g {
		t.Fatalf("GuardFromTools = %v, want the read tool's guard", got)
	}
	if got := GuardFromTools([]agentcore.AgentTool{&GrepTool{Root: "/w"}}); got != nil {
		t.Fatalf("GuardFromTools without a wired guard = %v, want nil", got)
	}
	if got := GuardFromTools(nil); got != nil {
		t.Fatalf("GuardFromTools(nil) = %v, want nil", got)
	}
}
