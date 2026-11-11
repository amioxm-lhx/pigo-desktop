package run

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// mcpSetupEnv runs SetupEnv with the given MCP settings in an isolated home
// (fresh trust store, no plugins/skills) and returns the manager plus the
// mcp-prefixed tool names. The provider is never contacted (same contract as
// setupToolNames).
func mcpSetupEnv(t *testing.T, noTools bool, mcpCfg MCPSettings) (env Env, err error) {
	t.Helper()
	t.Setenv("OPENROUTER_API_KEY", "test-key")
	t.Setenv("PIGO_HOME", t.TempDir()) // fresh trust store + empty plugin dir
	t.Setenv("PIGO_MCP_CONFIG", "")
	env, err = SetupEnv("openrouter/free", "", "", "", "", noTools, true /*noSkills*/, "", nil, false /*memEnabled*/, ToolPolicy{}, mcpCfg)
	return env, err
}

// mcpToolNames returns the mcp_-namespaced tools in the set.
func mcpToolNames(env Env) []string {
	var out []string
	for _, t := range env.Tools {
		if strings.HasPrefix(t.Name(), "mcp_") {
			out = append(out, t.Name())
		}
	}
	return out
}

// TestSetupEnvMCPDisabledByFlags pins the "launch nothing" contract (docs §6):
// --no-mcp and --no-tools must leave no MCP manager behind even when a config
// file with servers exists, and the project layer must not be consulted either.
func TestSetupEnvMCPDisabledByFlags(t *testing.T) {
	cwd := t.TempDir()
	if err := os.MkdirAll(filepath.Join(cwd, ".pigo"), 0o755); err != nil {
		t.Fatal(err)
	}
	cfg := filepath.Join(cwd, ".pigo", "mcp.toml")
	if err := os.WriteFile(cfg, []byte("[[server]]\nid = \"x\"\ncommand = \"whatever\"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	t.Chdir(cwd)

	// --no-mcp: manager nil, no mcp tools.
	env, err := mcpSetupEnv(t, false, MCPSettings{Disabled: true})
	if err != nil {
		t.Fatalf("SetupEnv(--no-mcp): %v", err)
	}
	if env.MCP != nil || len(mcpToolNames(env)) != 0 {
		t.Errorf("--no-mcp left MCP active: manager=%v tools=%v", env.MCP, mcpToolNames(env))
	}

	// --no-tools implies --no-mcp.
	env, err = mcpSetupEnv(t, true, MCPSettings{})
	if err != nil {
		t.Fatalf("SetupEnv(--no-tools): %v", err)
	}
	if env.MCP != nil || len(mcpToolNames(env)) != 0 {
		t.Errorf("--no-tools left MCP active: manager=%v tools=%v", env.MCP, mcpToolNames(env))
	}
}

// TestSetupEnvMCPUntrustedProjectLayerSkipped pins the fail-closed trust gate
// end to end: a fresh (untrusted) directory's .pigo/mcp.toml must not be loaded
// — the run's default resolution is empty, so no manager exists and no
// subprocess was ever launched.
func TestSetupEnvMCPUntrustedProjectLayerSkipped(t *testing.T) {
	cwd := t.TempDir()
	if err := os.MkdirAll(filepath.Join(cwd, ".pigo"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(cwd, ".pigo", "mcp.toml"), []byte("[[server]]\nid = \"evil\"\ncommand = \"malicious.exe\"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	t.Chdir(cwd)
	env, err := mcpSetupEnv(t, false, MCPSettings{})
	if err != nil {
		t.Fatalf("SetupEnv: %v", err)
	}
	if env.MCP != nil {
		t.Errorf("untrusted project config was loaded: %+v", env.MCP)
	}
	if len(mcpToolNames(env)) != 0 {
		t.Errorf("untrusted project tools registered: %v", mcpToolNames(env))
	}
}
