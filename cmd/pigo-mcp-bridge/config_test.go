package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestLoadConfig(t *testing.T) {
	path := filepath.Join(t.TempDir(), "mcp.toml")
	content := `
[[server]]
id = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "D:/notes"]
env = { NODE_NO_WARNINGS = "1" }

[[server]]
id = "sqlite"
command = "uvx"
args = ["mcp-server-sqlite", "--db", "D:/data/app.db"]
`
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	servers, err := loadConfig(path)
	if err != nil {
		t.Fatalf("loadConfig: %v", err)
	}
	if len(servers) != 2 {
		t.Fatalf("got %d servers, want 2", len(servers))
	}
	if servers[0].ID != "filesystem" || servers[0].Command != "npx" || len(servers[0].Args) != 3 {
		t.Errorf("filesystem server parsed wrong: %+v", servers[0])
	}
	if servers[0].Env["NODE_NO_WARNINGS"] != "1" {
		t.Errorf("env not parsed: %+v", servers[0].Env)
	}
	if servers[1].ID != "sqlite" {
		t.Errorf("second server wrong: %+v", servers[1])
	}
}

func TestLoadConfigMissingFile(t *testing.T) {
	servers, err := loadConfig(filepath.Join(t.TempDir(), "absent.toml"))
	if err != nil {
		t.Fatalf("missing config must not be an error, got: %v", err)
	}
	if len(servers) != 0 {
		t.Fatalf("want zero servers, got %d", len(servers))
	}
}

func TestLoadConfigInvalid(t *testing.T) {
	cases := map[string]string{
		"malformed":  "this is not toml [[",
		"missing id": "[[server]]\ncommand = \"foo\"\n",
		"missing command": `[[server]]
id = "x"
`,
		"duplicate id": `[[server]]
id = "x"
command = "a"

[[server]]
id = "x"
command = "b"
`,
	}
	for name, content := range cases {
		path := filepath.Join(t.TempDir(), "mcp.toml")
		if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
		if servers, err := loadConfig(path); err == nil {
			t.Errorf("%s: want error, got %+v", name, servers)
		}
	}
}

func TestBuildEnv(t *testing.T) {
	// A variable that must NOT cross into the child env, whatever it holds.
	t.Setenv("PIGO_BRIDGE_SECRET_TEST", "leak-me")
	t.Setenv("PATH", "/custom/bin")
	env := buildEnv(map[string]string{
		"NODE_NO_WARNINGS": "1",
		"PATH":             "/declared/path", // declared overrides allowlist
	})

	got := make(map[string]string)
	for _, kv := range env {
		k, v, _ := strings.Cut(kv, "=")
		got[k] = v
	}

	if _, leak := got["PIGO_BRIDGE_SECRET_TEST"]; leak {
		t.Errorf("secret env var leaked into child env: %v", got)
	}
	if got["PATH"] != "/declared/path" {
		t.Errorf("declared env must override allowlist, PATH = %q", got["PATH"])
	}
	if got["NODE_NO_WARNINGS"] != "1" {
		t.Errorf("declared env missing: %v", got)
	}
	// Every allowlisted variable that exists in the parent must be forwarded.
	for _, k := range envAllowlist {
		if _, ok := os.LookupEnv(k); ok {
			if _, ok := got[k]; !ok {
				t.Errorf("allowlisted parent var %q not forwarded", k)
			}
		}
	}
}
