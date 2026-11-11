// This file aggregates multiple MCP servers into one tool set (docs §3.1
// registry.go, mirroring internal/plugin/manager.go's aggregation role):
// connecting each configured server, capping its tool count, sanitizing and
// compile-checking each schema, namespacing names, and deduplicating
// collisions loudly. A server that fails to connect is skipped with a warning
// — one broken server must not take down the run.
package mcp

import (
	"io"
	"sync"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/agenttool"
)

// Manager is the connected-server set for one run. Connect is called during
// run assembly (single-goroutine); Tools/Close are read by the run and its
// shutdown path.
type Manager struct {
	// Logf receives one-line warnings (skipped servers, dropped tools).
	Logf func(string, ...any)
	// Stderr, when non-nil, receives each server subprocess's stderr.
	Stderr io.Writer

	mu    sync.Mutex
	tools []agentcore.AgentTool
	seen  map[string]bool
	// Clients holds one Client per successfully connected server, in connect
	// order. Resource access (resources/list + resources/read) goes through
	// these.
	Clients []*Client
}

// NewManager returns an empty Manager. logf may be nil (warnings dropped).
func NewManager(logf func(string, ...any), stderr io.Writer) *Manager {
	if logf == nil {
		logf = func(string, ...any) {}
	}
	return &Manager{Logf: logf, Stderr: stderr, seen: map[string]bool{}}
}

// warnf logs through Logf, prefixing pigo's convention.
func (m *Manager) warnf(format string, args ...any) {
	m.Logf("pigo: mcp: "+format, args...)
}

// Connect connects one server (stdio subprocess or Streamable HTTP) and
// registers its tools. The returned error is the connection failure (caller
// logs and continues with the remaining servers); per-tool problems inside a
// connected server are warned here and skipped instead of failing the server.
func (m *Manager) Connect(server Server) error {
	if !server.IsEnabled() {
		return nil
	}
	client, err := Dial(server, m.Stderr, m.Logf)
	if err != nil {
		return err
	}
	for _, notice := range client.Notices() {
		m.warnf("%s", notice)
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	m.Clients = append(m.Clients, client)
	for _, decl := range client.Tools() {
		m.register(client, server.ID, decl)
	}
	return nil
}

// register adapts one tool declaration: sanitize schema, compile-check it,
// namespace, and dedupe. Any failure warns and skips the tool — registration
// must never be silent (docs §1.1's ToolRegistry pitfall).
func (m *Manager) register(client *Client, serverID string, decl ToolDecl) {
	sanitized, err := SanitizeSchema(decl.InputSchema)
	if err != nil {
		m.warnf("server %q: tool %q dropped: %v", serverID, decl.Name, err)
		return
	}
	name := Namespace(serverID, decl.Name)
	if _, err := agenttool.CompileSchema(name, sanitized); err != nil {
		m.warnf("server %q: tool %q dropped: schema does not compile: %v", serverID, decl.Name, err)
		return
	}
	if m.seen[name] {
		m.warnf("server %q: tool %q maps to already-registered name %q; keeping the first", serverID, decl.Name, name)
		return
	}
	m.seen[name] = true
	m.tools = append(m.tools, newTool(client, serverID, decl, sanitized))
}

// Tools returns the registered MCP tools in connection order.
func (m *Manager) Tools() []agentcore.AgentTool {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.tools
}

// Close disconnects every connected server. The first error is returned after
// all closes are attempted; it is safe to call once.
func (m *Manager) Close() error {
	m.mu.Lock()
	clients := m.Clients
	m.Clients = nil
	m.mu.Unlock()
	var firstErr error
	for _, c := range clients {
		if err := c.Close(); err != nil && firstErr == nil {
			firstErr = err
		}
	}
	return firstErr
}

// Resolve gathers the effective server list for one run (docs §3.2 item 2 and
// §3.7.1): the explicit layer (--mcp-config, or the config.toml [mcp] tier it
// overrides, or the bridge-compatible default file) plus the trust-gated
// project layer. disabled or empty settings yield nil.
//
// trusted must come from run.Trusted(cwd) by the caller — the project layer is
// fail-closed and only merges when the directory is trusted, so a cloned repo
// cannot launch arbitrary commands.
func Resolve(settings Servers, cwd string, trusted bool, warnf func(string, ...any)) []Server {
	if warnf == nil {
		warnf = func(string, ...any) {}
	}
	var file File
	switch {
	case settings.ConfigPath != "":
		f, err := LoadFile(settings.ConfigPath)
		if err != nil {
			warnf("%v", err)
		}
		file = f
	case len(settings.Servers) > 0:
		file = File{Servers: settings.Servers}
	default:
		f, err := LoadFile(DefaultConfigPath())
		if err != nil {
			warnf("%v", err)
		}
		file = f
	}
	var out []Server
	if file.IsEnabled() {
		out = append(out, file.EnabledServers(warnf)...)
	}
	// Project layer: always attempted when trusted, in every branch — it is the
	// per-project addition, like hooks (docs §3.7.1).
	if trusted && cwd != "" {
		f, err := LoadFile(ProjectConfigPath(cwd))
		if err != nil {
			warnf("%v", err)
		}
		out = append(out, f.EnabledServers(warnf)...)
	}
	return out
}

// Servers is the resolved settings passed to Resolve (and, through run.MCPSettings,
// from cmd/pigo): the [mcp] tier of config.toml plus the --mcp-config flag.
type Servers struct {
	// Disabled is the master switch (--no-mcp or [mcp] enabled=false).
	Disabled bool
	// ConfigPath is --mcp-config; when set it replaces Servers as the global
	// layer (CLI > config file, docs §3.2 item 1).
	ConfigPath string
	// Servers is the config.toml [mcp] servers tier.
	Servers []Server
}

// ConnectAll connects every server in order through one Manager, warning (not
// failing) on individual connection errors, and logs each notice. It is the
// one-call setup used by run assembly.
func ConnectAll(settings Servers, cwd string, trusted bool, stderr io.Writer, warnf func(string, ...any)) *Manager {
	if settings.Disabled || warnf == nil {
		return nil
	}
	servers := Resolve(settings, cwd, trusted, warnf)
	if len(servers) == 0 {
		return nil
	}
	m := NewManager(warnf, stderr)
	connected := 0
	for _, s := range servers {
		if err := m.Connect(s); err != nil {
			warnf("pigo: mcp: server %q: %v (skipped)", s.ID, err)
			continue
		}
		connected++
	}
	if connected == 0 {
		return nil
	}
	return m
}
