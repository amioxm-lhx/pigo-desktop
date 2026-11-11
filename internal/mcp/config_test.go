package mcp

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
)

// TestLoadFileParsesServers covers the frozen mcp.toml shape: [[server]]
// entries with id/transport/command/args/env/url/enabled, plus the file-level
// enabled switch.
func TestLoadFileParsesServers(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "mcp.toml")
	content := `
enabled = true

[[server]]
id = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "D:/notes"]
env = { NODE_NO_WARNINGS = "1" }

[[server]]
id = "local-http"
transport = "http"
url = "http://127.0.0.1:8765/mcp"

[[server]]
id = "off"
command = "true"
enabled = false
`
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	f, err := LoadFile(path)
	if err != nil {
		t.Fatalf("LoadFile: %v", err)
	}
	if !f.IsEnabled() {
		t.Error("file enabled = false, want default-on")
	}
	if len(f.Servers) != 3 {
		t.Fatalf("servers = %d, want 3", len(f.Servers))
	}
	if f.Servers[0].Command != "npx" || len(f.Servers[0].Args) != 3 || f.Servers[0].Env["NODE_NO_WARNINGS"] != "1" {
		t.Errorf("stdio entry parsed wrong: %+v", f.Servers[0])
	}
	if !f.Servers[0].Stdio() {
		t.Error("entry without transport must default to stdio")
	}
	if f.Servers[1].Stdio() || f.Servers[1].URL != "http://127.0.0.1:8765/mcp" {
		t.Errorf("http entry parsed wrong: %+v", f.Servers[1])
	}
	if f.Servers[2].IsEnabled() {
		t.Error("enabled=false entry must report disabled")
	}
}

// TestLoadFileMissingAndMalformed: a missing file is "no servers" (nil error);
// a malformed file is an error so it surfaces loudly.
func TestLoadFileMissingAndMalformed(t *testing.T) {
	if f, err := LoadFile(filepath.Join(t.TempDir(), "absent.toml")); err != nil || len(f.Servers) != 0 {
		t.Errorf("missing file = (%+v, %v), want zero file and no error", f, err)
	}
	if _, err := LoadFile(""); err != nil {
		t.Errorf("empty path = %v, want nil error", err)
	}
	bad := filepath.Join(t.TempDir(), "bad.toml")
	if err := os.WriteFile(bad, []byte("not [valid toml"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := LoadFile(bad); err == nil {
		t.Error("malformed file must be an error")
	}
}

// TestEnabledServersValidation covers per-entry skips: invalid entries and
// duplicate ids are dropped with a warning, disabled entries are filtered.
func TestEnabledServersValidation(t *testing.T) {
	var warns []string
	warnf := func(format string, args ...any) { warns = append(warns, strings.TrimSpace(fmt.Sprintf(format, args...))) }
	f := File{Servers: []Server{
		{ID: "ok", Command: "true"},
		{ID: "", Command: "true"},              // missing id
		{ID: "no-cmd"},                         // stdio without command
		{ID: "trav", Command: "../evil.exe"},   // traversal
		{ID: "http", Transport: "http", URL: "http://127.0.0.1:1/mcp"},
		{ID: "bad-transport", Transport: "grpc"},
		{ID: "dup", Command: "a"},
		{ID: "dup", Command: "b"},
	}}
	got := f.EnabledServers(warnf)
	if len(got) != 3 {
		t.Fatalf("enabled servers = %d (%+v), want 3 (ok, http, first dup)", len(got), got)
	}
	if got[0].ID != "ok" || got[1].ID != "http" || got[2].Command != "a" {
		t.Errorf("unexpected survivors: %+v", got)
	}
	if len(warns) != 5 {
		t.Errorf("warnings = %d (%q), want 5", len(warns), warns)
	}
}

// TestServerValidateLimits covers the desktop-aligned budgets: MAX_ARGS 64 and
// MAX_ENV_ENTRIES 64.
func TestServerValidateLimits(t *testing.T) {
	s := Server{ID: "x", Command: "true", Args: make([]string, MaxArgs+1)}
	if err := s.validate(); err == nil || !strings.Contains(err.Error(), "args") {
		t.Errorf("args over limit: err = %v", err)
	}
	s.Args = nil
	s.Env = map[string]string{}
	for i := 0; i <= MaxEnvEntries; i++ {
		s.Env[fmt.Sprintf("K%02d", i)] = "v"
	}
	if err := s.validate(); err == nil || !strings.Contains(err.Error(), "env") {
		t.Errorf("env over limit: err = %v", err)
	}
}

// TestDefaultConfigPath covers the resolution chain: PIGO_MCP_CONFIG override,
// then $PIGO_HOME/mcp.toml, then ~/.pigo/mcp.toml.
func TestDefaultConfigPath(t *testing.T) {
	t.Setenv("PIGO_MCP_CONFIG", "D:/explicit/mcp.toml")
	if got := DefaultConfigPath(); got != "D:/explicit/mcp.toml" {
		t.Errorf("PIGO_MCP_CONFIG override ignored: %q", got)
	}
	t.Setenv("PIGO_MCP_CONFIG", "")
	home := t.TempDir()
	t.Setenv("PIGO_HOME", home)
	if got := DefaultConfigPath(); got != filepath.Join(home, "mcp.toml") {
		t.Errorf("PIGO_HOME path = %q, want %q", got, filepath.Join(home, "mcp.toml"))
	}
	t.Setenv("PIGO_HOME", "")
	t.Setenv("USERPROFILE", home)
	if got := DefaultConfigPath(); got != filepath.Join(home, ".pigo", "mcp.toml") {
		t.Errorf("home path = %q, want %q", got, filepath.Join(home, ".pigo", "mcp.toml"))
	}
}

// TestBuildEnvIsolation is the most security-critical assertion in the package
// (docs §6): the child environment must NOT contain arbitrary parent secrets
// (OPENAI_API_KEY is the canonical leak), must carry the PATH allowlist, and
// must apply config-declared entries on top.
func TestBuildEnvIsolation(t *testing.T) {
	t.Setenv("OPENAI_API_KEY", "sk-leak-me")
	t.Setenv("SECRET_TOKEN", "leak-me-too")
	env := BuildEnv(map[string]string{"NODE_NO_WARNINGS": "1", "MY_TOKEN": "declared"})
	if len(env) == 0 {
		t.Fatal("env is empty")
	}
	joined := strings.Join(env, "\n")
	if strings.Contains(joined, "sk-leak-me") || strings.Contains(joined, "leak-me-too") {
		t.Errorf("parent secrets leaked into child env:\n%s", joined)
	}
	for _, kv := range env {
		k, _, _ := strings.Cut(kv, "=")
		switch k {
		case "OPENAI_API_KEY", "SECRET_TOKEN":
			t.Errorf("secret key %q present in child env", k)
		}
	}
	if !hasEnv(env, "PATH") {
		t.Error("PATH missing from child env")
	}
	if !hasEnv(env, "NODE_NO_WARNINGS") || !hasEnv(env, "MY_TOKEN") {
		t.Errorf("declared env entries missing: %v", env)
	}
	// Determinism: sorted keys, no duplicates.
	keys := make([]string, 0, len(env))
	for _, kv := range env {
		k, _, _ := strings.Cut(kv, "=")
		keys = append(keys, k)
	}
	if !sort.StringsAreSorted(keys) {
		t.Errorf("env keys not sorted: %v", keys)
	}
	seen := map[string]bool{}
	for _, k := range keys {
		if seen[k] {
			t.Errorf("duplicate env key %q", k)
		}
		seen[k] = true
	}
}

func hasEnv(env []string, key string) bool {
	for _, kv := range env {
		if k, _, _ := strings.Cut(kv, "="); k == key {
			return true
		}
	}
	return false
}

// TestProjectConfigPath pins the project layer location: <cwd>/.pigo/mcp.toml.
func TestProjectConfigPath(t *testing.T) {
	if got := ProjectConfigPath("D:/work"); got != filepath.Join("D:/work", ".pigo", "mcp.toml") {
		t.Errorf("ProjectConfigPath = %q", got)
	}
}