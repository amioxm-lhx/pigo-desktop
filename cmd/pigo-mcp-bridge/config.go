// This file loads the bridge's own configuration: which local MCP servers to
// expose to pigo and how to launch them. It deliberately has nothing to do
// with pigo's own config.toml (internal/cli/config) — the bridge is a plugin
// process, not part of the pigo binary, so it owns a separate TOML file.
package main

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"strings"

	"github.com/BurntSushi/toml"
)

// ConfigPath resolves the bridge config file. $PIGO_MCP_CONFIG overrides,
// otherwise the default is ~/.pigo/mcp.toml. An empty return means the path
// could not be resolved at all (treated as "no config").
func ConfigPath() string {
	if p := os.Getenv("PIGO_MCP_CONFIG"); p != "" {
		return p
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return ""
	}
	return filepath.Join(home, ".pigo", "mcp.toml")
}

// ServerConfig is one [[server]] entry: a local stdio MCP server to bridge.
// Env values declared here are the ONLY extra variables passed to the server
// subprocess beyond the allowlist in buildEnv.
type ServerConfig struct {
	ID      string            `toml:"id"`
	Command string            `toml:"command"`
	Args    []string          `toml:"args"`
	Env     map[string]string `toml:"env"`
}

// fileConfig is the on-disk shape of mcp.toml.
type fileConfig struct {
	Servers []ServerConfig `toml:"server"`
}

// loadConfig reads and validates the bridge config. A missing file is not an
// error — it yields zero servers, so the bridge can be installed before any
// server is configured (pigo then loads a tool-less plugin). A malformed file
// or an invalid server entry is an error so it surfaces loudly.
func loadConfig(path string) ([]ServerConfig, error) {
	if path == "" {
		return nil, nil
	}
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("read: %w", err)
	}
	var fc fileConfig
	if err := toml.Unmarshal(data, &fc); err != nil {
		return nil, fmt.Errorf("parse: %w", err)
	}
	seen := make(map[string]bool, len(fc.Servers))
	for i := range fc.Servers {
		s := &fc.Servers[i]
		if strings.TrimSpace(s.ID) == "" {
			return nil, fmt.Errorf("server #%d: missing id", i+1)
		}
		if strings.TrimSpace(s.Command) == "" {
			return nil, fmt.Errorf("server %q: missing command", s.ID)
		}
		if seen[s.ID] {
			return nil, fmt.Errorf("duplicate server id %q", s.ID)
		}
		seen[s.ID] = true
	}
	return fc.Servers, nil
}

// envAllowlist is the only parent environment the bridge forwards to MCP
// server subprocesses, mirroring the desktop client
// (desktop/apps/desktop-tauri/src-tauri/src/mcp.rs) and ADR 0038 §6: PATH and
// process basics cross so a bare command name can still launch, everything
// else (API keys included) does not. Config-declared env entries are applied
// on top and override the allowlist.
//
// This whitelist is NOT optional: jsonrpc.Config.Env == nil makes the child
// inherit the bridge's whole parent environment, which would leak secrets like
// OPENAI_API_KEY to arbitrary third-party MCP servers.
var envAllowlist = []string{
	"PATH",
	"HOME",
	"TMPDIR",
	"TEMP",
	"TMP",
	"LANG",
	"LC_ALL",
	"SystemRoot",
	"WINDIR",
	"ComSpec",
	"USERPROFILE",
}

// buildEnv constructs the child environment: allowlisted parent variables,
// overlaid with the config-declared ones, emitted in sorted order for
// determinism (and to avoid duplicate keys in the child's env block).
func buildEnv(declared map[string]string) []string {
	parent := make(map[string]string)
	for _, kv := range os.Environ() {
		k, v, _ := strings.Cut(kv, "=")
		parent[k] = v
	}
	merged := make(map[string]string)
	for _, k := range envAllowlist {
		if v, ok := lookupParent(parent, k); ok {
			merged[k] = v
		}
	}
	for k, v := range declared {
		merged[k] = v
	}
	keys := make([]string, 0, len(merged))
	for k := range merged {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	out := make([]string, 0, len(keys))
	for _, k := range keys {
		out = append(out, k+"="+merged[k])
	}
	return out
}

// lookupParent finds an allowlisted variable in the parent environment. On
// Windows variable names are case-insensitive, so a parent spelling like
// "windir" still matches the allowlisted "WINDIR" (emitted in canonical
// spelling); elsewhere matching is exact.
func lookupParent(parent map[string]string, key string) (string, bool) {
	if v, ok := parent[key]; ok {
		return v, true
	}
	if runtime.GOOS != "windows" {
		return "", false
	}
	for k, v := range parent {
		if strings.EqualFold(k, key) {
			return v, true
		}
	}
	return "", false
}
