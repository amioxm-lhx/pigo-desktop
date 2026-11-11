// Package mcp implements pigo's native MCP (Model Context Protocol) client
// (US-014/US-015): connecting local MCP servers, listing their tools, and
// exposing them as agentcore.AgentTool values. The wire protocol is JSON-RPC
// 2.0 over stdio (via internal/jsonrpc) with the protocol version pinned to
// 2024-11-05, matching the desktop client
// (desktop/apps/desktop-tauri/src-tauri/src/mcp.rs) so both stacks behave
// identically. See docs/mcp-local-integration.md §3 for the design contract.
//
// The frozen conventions shared with cmd/pigo-mcp-bridge (方案 A) are: the
// mcp.toml entry shape (id/transport/command/args/env/url/enabled), the
// mcp_<server>_<tool> namespace, the env allowlist, and the 64-tools-per-server
// cap. They must not change without updating the bridge too.
package mcp

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"strings"

	"github.com/BurntSushi/toml"
)

// Budget limits, aligned with the desktop client (mcp_servers.rs:12-16) and
// docs/mcp-local-integration.md §3.6.
const (
	// MaxArgs caps the args array per server entry.
	MaxArgs = 64
	// MaxEnvEntries caps the declared env map per server entry.
	MaxEnvEntries = 64
)

// Server is one MCP server entry. The shape is frozen together with the
// bridge's mcp.toml (cmd/pigo-mcp-bridge/config.go): the same file loads in
// both, so P0 bridge users keep their config when upgrading to the native
// client. Transport defaults to stdio; http arrives with the HTTP transport.
type Server struct {
	ID string `toml:"id"`
	// Transport is "stdio" (default) or "http".
	Transport string `toml:"transport"`
	// Command is the stdio launch executable: a bare name resolved on PATH at
	// launch, or a path. Path traversal ("..") is rejected at load time.
	Command string   `toml:"command"`
	Args    []string `toml:"args"`
	// Env is the ONLY environment beyond the allowlist the server subprocess
	// receives. Nothing else is inherited — in particular API keys are not
	// leaked to third-party servers (docs §3.7.2).
	Env map[string]string `toml:"env"`
	// URL is the http-transport endpoint. Empty for stdio servers.
	URL string `toml:"url"`
	// AllowInsecureFlag is the allow_insecure opt-in: permit plain http:// to a
	// non-loopback host. The default (nil = false) is https:// only, with
	// http:// allowed for loopback endpoints (docs §3.7.4).
	AllowInsecureFlag *bool `toml:"allow_insecure"`
	// Enabled is a per-server switch; nil counts as true.
	Enabled *bool `toml:"enabled"`
}

// IsEnabled reports whether the server entry is switched on (nil = true).
func (s Server) IsEnabled() bool { return s.Enabled == nil || *s.Enabled }

// Stdio reports whether the entry uses the stdio transport.
func (s Server) Stdio() bool { return s.Transport == "" || s.Transport == "stdio" }

// validate checks one entry. A missing file yields no entries, so anything
// present is user intent and must parse strictly.
func (s Server) validate() error {
	if strings.TrimSpace(s.ID) == "" {
		return fmt.Errorf("server entry missing id")
	}
	if len(s.Args) > MaxArgs {
		return fmt.Errorf("server %q: %d args exceeds the limit of %d", s.ID, len(s.Args), MaxArgs)
	}
	if len(s.Env) > MaxEnvEntries {
		return fmt.Errorf("server %q: %d env entries exceeds the limit of %d", s.ID, len(s.Env), MaxEnvEntries)
	}
	if s.Stdio() {
		if strings.TrimSpace(s.Command) == "" {
			return fmt.Errorf("server %q: stdio server missing command", s.ID)
		}
	} else {
		if s.Transport != "http" {
			return fmt.Errorf("server %q: unknown transport %q (supported: stdio, http)", s.ID, s.Transport)
		}
		if strings.TrimSpace(s.URL) == "" {
			return fmt.Errorf("server %q: http server missing url", s.ID)
		}
		if err := checkEndpointSecurity(s.URL, s.AllowInsecure()); err != nil {
			return fmt.Errorf("server %q: %w", s.ID, err)
		}
	}
	if containsTraversal(s.Command) {
		return fmt.Errorf("server %q: command %q must not contain %q", s.ID, s.Command, "..")
	}
	return nil
}

// containsTraversal reports whether a path-like string climbs directories.
func containsTraversal(p string) bool {
	if p == "" {
		return false
	}
	for _, part := range strings.FieldsFunc(p, func(r rune) bool {
		return r == '/' || r == '\\'
	}) {
		if part == ".." {
			return true
		}
	}
	return false
}

// File is the on-disk shape of a standalone mcp.toml. The same shape is
// embedded as the [mcp] table of config.toml by internal/cli/config (there the
// servers array key is "servers" → [[mcp.servers]]).
type File struct {
	// Enabled is a file-level master switch; nil counts as true.
	Enabled *bool `toml:"enabled"`
	Servers []Server `toml:"server"`
}

// IsEnabled reports whether the whole file is switched on (nil = true).
func (f File) IsEnabled() bool { return f.Enabled == nil || *f.Enabled }

// EnabledServers returns the on entries in file order, each validated. An
// invalid entry is dropped with a warning rather than failing the whole file —
// one broken server must not take down the others (mirrors plugin loading).
func (f File) EnabledServers(warnf func(string, ...any)) []Server {
	var out []Server
	seen := make(map[string]bool, len(f.Servers))
	for _, s := range f.Servers {
		if !s.IsEnabled() {
			continue
		}
		if err := s.validate(); err != nil {
			if warnf != nil {
				warnf("pigo: mcp: skipping server: %v", err)
			}
			continue
		}
		if seen[s.ID] {
			if warnf != nil {
				warnf("pigo: mcp: skipping duplicate server id %q", s.ID)
			}
			continue
		}
		seen[s.ID] = true
		out = append(out, s)
	}
	return out
}

// LoadFile reads a standalone mcp.toml. A missing file is not an error — it
// yields a zero File (no servers), so MCP can be configured purely through
// config.toml or enabled lazily. A malformed file IS an error so it surfaces
// loudly; per-entry problems are downgraded to skips by EnabledServers.
func LoadFile(path string) (File, error) {
	if path == "" {
		return File{}, nil
	}
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return File{}, nil
		}
		return File{}, fmt.Errorf("mcp: read %s: %w", path, err)
	}
	var f File
	if err := toml.Unmarshal(data, &f); err != nil {
		return File{}, fmt.Errorf("mcp: parse %s: %w", path, err)
	}
	return f, nil
}

// DefaultConfigPath resolves the standalone mcp.toml the native client loads
// when neither --mcp-config nor config.toml [mcp] provided servers: the SAME
// file the P0 bridge reads, so a bridge-era config keeps working. $PIGO_MCP_CONFIG
// overrides (the bridge's override env), then $PIGO_HOME/mcp.toml, then
// ~/.pigo/mcp.toml. An empty return means the path could not be resolved at all.
func DefaultConfigPath() string {
	if p := os.Getenv("PIGO_MCP_CONFIG"); p != "" {
		return p
	}
	dir := os.Getenv("PIGO_HOME")
	if dir == "" {
		home, err := os.UserHomeDir()
		if err != nil {
			return ""
		}
		dir = filepath.Join(home, ".pigo")
	}
	return filepath.Join(dir, "mcp.toml")
}

// ProjectConfigPath is the trust-gated per-project layer: <cwd>/.pigo/mcp.toml.
// Callers must only load it when the directory is trusted (run.Trusted), so a
// cloned repository cannot launch arbitrary commands (docs §3.7.1).
func ProjectConfigPath(cwd string) string {
	return filepath.Join(cwd, ".pigo", "mcp.toml")
}

// envAllowlist is the only parent environment MCP server subprocesses receive,
// mirroring the bridge and the desktop client (ADR 0038 decision 6): PATH and
// process basics cross so a bare command name can still launch; everything
// else — API keys included — does not. Config-declared env entries are applied
// on top and override the allowlist. This allowlist is NOT optional:
// jsonrpc.Config.Env == nil would make the child inherit the whole parent
// environment, leaking secrets like OPENAI_API_KEY to third-party servers.
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

// BuildEnv constructs a server subprocess environment: allowlisted parent
// variables overlaid with the config-declared ones, emitted in sorted order for
// determinism (and to avoid duplicate keys in the child's env block).
func BuildEnv(declared map[string]string) []string {
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
