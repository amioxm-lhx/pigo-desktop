package mcp

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestNamespace pins the frozen naming rule: mcp_<safe(server)>_<safe(tool)>,
// with empty components spelled "unnamed" (docs §3.3; the bridge spells it "_").
func TestNamespace(t *testing.T) {
	cases := []struct{ server, tool, want string }{
		{"filesystem", "read_file", "mcp_filesystem_read_file"},
		{"my server", "read-file", "mcp_my_server_read_file"},
		{"jp", "a b", "mcp_jp_a_b"},
		{"jp", "!!!", "mcp_jp____"}, // separator + three replaced runes = four underscores
		{"", "read", "mcp_unnamed_read"},
		{"srv", "", "mcp_srv_unnamed"},
		{"", "", "mcp_unnamed_unnamed"},
	}
	for _, c := range cases {
		if got := Namespace(c.server, c.tool); got != c.want {
			t.Errorf("Namespace(%q, %q) = %q, want %q", c.server, c.tool, got, c.want)
		}
	}
}

// newTestClient builds a Client over a fake rpc serving one initialize +
// tools/list sequence with the given tool names.
func newTestClient(t *testing.T, id string, tools []ToolDecl) *Client {
	t.Helper()
	decls := make([]any, len(tools))
	for i, d := range tools {
		var schema any
		if len(d.InputSchema) > 0 {
			if err := json.Unmarshal(d.InputSchema, &schema); err != nil {
				t.Fatalf("bad test schema: %v", err)
			}
		}
		decls[i] = map[string]any{"name": d.Name, "description": d.Description, "inputSchema": schema}
	}
	rpc := &fakeRPC{handlers: map[string]func(json.RawMessage) (any, error){
		"initialize": func(json.RawMessage) (any, error) {
			return map[string]any{"protocolVersion": ProtocolVersion}, nil
		},
		"tools/list": func(json.RawMessage) (any, error) {
			return map[string]any{"tools": decls}, nil
		},
	}}
	c, err := newClient(id, rpc, nil)
	if err != nil {
		t.Fatalf("newClient(%s): %v", id, err)
	}
	return c
}

// TestManagerRegisterDropsAndDedupes: schema sanitize failures and compile
// failures drop the tool with a warning (never silently), name collisions keep
// the first registration, and valid tools register in order.
func TestManagerRegisterDropsAndDedupes(t *testing.T) {
	var logs []string
	m := NewManager(func(format string, args ...any) {
		logs = append(logs, fmt.Sprintf(format, args...))
	}, nil)
	client := newTestClient(t, "srv", []ToolDecl{
		{Name: "good", Description: "ok", InputSchema: json.RawMessage(`{"type":"object","properties":{"a":{"type":"string"}}}`)},
		{Name: "external-ref", InputSchema: json.RawMessage(`{"type":"object","properties":{"a":{"$ref":"https://evil.example/x"}}}`)},
		{Name: "non-object-schema", InputSchema: json.RawMessage(`["nope"]`)},
		{Name: "dup", InputSchema: json.RawMessage(`{"type":"object"}`)},
		{Name: "dup", InputSchema: json.RawMessage(`{"type":"object"}`)},
	})
	for _, decl := range client.Tools() {
		m.register(client, "srv", decl)
	}
	names := make([]string, 0, len(m.tools))
	for _, tool := range m.tools {
		names = append(names, tool.Name())
	}
	want := []string{"mcp_srv_good", "mcp_srv_dup"}
	if strings.Join(names, ",") != strings.Join(want, ",") {
		t.Errorf("registered = %v, want %v", names, want)
	}
	joined := strings.Join(logs, "\n")
	for _, fragment := range []string{"points outside the document", "top level must be an object", "already-registered"} {
		if !strings.Contains(joined, fragment) {
			t.Errorf("missing warning %q in:\n%s", fragment, joined)
		}
	}
	// The surviving tools are side-effectful and sequential (docs §3.7.3).
	if len(m.tools) > 0 {
		tool := m.tools[0]
		if se, ok := tool.(interface{ HasSideEffects() bool }); !ok || !se.HasSideEffects() {
			t.Errorf("mcp tool %s does not declare side effects", tool.Name())
		}
	}
}

// TestConnectAllDisabled: the master switch and the empty result must never
// spawn a subprocess (docs §6: --no-tools/--no-mcp launch nothing).
func TestConnectAllDisabled(t *testing.T) {
	if mgr := ConnectAll(Servers{Disabled: true, ConfigPath: "D:/whatever.toml"}, "", false, nil, func(string, ...any) {}); mgr != nil {
		t.Error("Disabled=true must return nil without reading config or connecting")
	}
	if mgr := ConnectAll(Servers{}, "", false, nil, func(string, ...any) {}); mgr != nil {
		t.Error("no servers must return nil")
	}
}

// TestResolveExplicitConfigReplacesFileServers: --mcp-config is the global
// layer wholesale (CLI > file), while the trusted project layer merges on top.
func TestResolveExplicitConfigReplacesFileServers(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "mcp.toml")
	if err := os.WriteFile(path, []byte(`
[[server]]
id = "explicit"
command = "explicit-server"
`), 0o644); err != nil {
		t.Fatal(err)
	}
	var warns []string
	got := Resolve(Servers{ConfigPath: path, Servers: []Server{{ID: "from-config-toml", Command: "x"}}}, tmp, false, func(f string, a ...any) {
		warns = append(warns, fmt.Sprintf(f, a...))
	})
	if len(got) != 1 || got[0].ID != "explicit" {
		t.Errorf("explicit layer = %+v, want only the --mcp-config server", got)
	}
}

// TestResolveProjectLayerTrustGate is the fail-closed check (docs §3.7.1): the
// project's .pigo/mcp.toml is only merged when trusted=true.
func TestResolveProjectLayerTrustGate(t *testing.T) {
	proj := t.TempDir()
	if err := os.MkdirAll(filepath.Join(proj, ".pigo"), 0o755); err != nil {
		t.Fatal(err)
	}
	projCfg := filepath.Join(proj, ".pigo", "mcp.toml")
	if err := os.WriteFile(projCfg, []byte(`
[[server]]
id = "project"
command = "project-server"
`), 0o644); err != nil {
		t.Fatal(err)
	}
	// Untrusted: project server absent.
	if got := Resolve(Servers{}, proj, false, nil); len(got) != 0 {
		t.Errorf("untrusted project layer loaded: %+v", got)
	}
	// Trusted: project server present.
	got := Resolve(Servers{}, proj, true, nil)
	if len(got) != 1 || got[0].ID != "project" {
		t.Errorf("trusted project layer = %+v, want the project server", got)
	}
}

// writeProjectLayer drops an mcp.toml under <proj>/.pigo so the project tier
// has something to merge.
func writeProjectLayer(t *testing.T, proj, body string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Join(proj, ".pigo"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(proj, ".pigo", "mcp.toml"), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
}

// TestResolveProjectLayerOverridesGlobalById pins the cross-layer rule: one id
// resolves to one server, and the project definition wins. Appending both used
// to connect the same id twice, yielding duplicate mcp_<server>_<tool> names.
func TestResolveProjectLayerOverridesGlobalById(t *testing.T) {
	proj := t.TempDir()
	writeProjectLayer(t, proj, `
[[server]]
id = "files"
command = "project-files"
`)
	settings := Servers{Servers: []Server{
		{ID: "files", Command: "global-files"},
		{ID: "keep", Command: "global-keep"},
	}}
	var warns []string
	got := Resolve(settings, proj, true, func(f string, a ...any) {
		warns = append(warns, fmt.Sprintf(f, a...))
	})
	if len(got) != 2 {
		t.Fatalf("resolved %d servers (%+v), want 2 — the shared id must not be duplicated", len(got), got)
	}
	if got[0].ID != "files" || got[0].Command != "project-files" {
		t.Errorf("files entry = %+v, want the project definition in the global slot", got[0])
	}
	if got[1].ID != "keep" || got[1].Command != "global-keep" {
		t.Errorf("keep entry = %+v, want the untouched global server", got[1])
	}
	if len(warns) != 1 || !strings.Contains(warns[0], "overrides global server") {
		t.Errorf("warnings = %q, want one override notice", warns)
	}
}

// TestResolveProjectLayerAppendsNewIds keeps the additive half of the contract:
// ids only present in the project layer are appended after the global ones.
func TestResolveProjectLayerAppendsNewIds(t *testing.T) {
	proj := t.TempDir()
	writeProjectLayer(t, proj, `
[[server]]
id = "extra"
command = "project-extra"
`)
	got := Resolve(Servers{Servers: []Server{{ID: "files", Command: "global-files"}}}, proj, true, nil)
	if len(got) != 2 || got[0].ID != "files" || got[1].ID != "extra" {
		t.Errorf("resolved = %+v, want [files extra]", got)
	}
}

// TestResolveProjectFileEnabledSwitch: the project file's own enabled=false
// master switch must be honoured, exactly like the global layer's.
func TestResolveProjectFileEnabledSwitch(t *testing.T) {
	proj := t.TempDir()
	writeProjectLayer(t, proj, `
enabled = false
[[server]]
id = "off"
command = "project-off"
`)
	if got := Resolve(Servers{Servers: []Server{{ID: "files", Command: "global-files"}}}, proj, true, nil); len(got) != 1 || got[0].ID != "files" {
		t.Errorf("resolved = %+v, want only the global server", got)
	}
}

// TestResolveFileEnabledSwitch: a standalone file with enabled=false
// contributes no servers even though its entries are valid.
func TestResolveFileEnabledSwitch(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "mcp.toml")
	if err := os.WriteFile(path, []byte(`
enabled = false
[[server]]
id = "off"
command = "x"
`), 0o644); err != nil {
		t.Fatal(err)
	}
	if got := Resolve(Servers{ConfigPath: path}, tmp, false, nil); len(got) != 0 {
		t.Errorf("file enabled=false still yielded servers: %+v", got)
	}
}

// TestResolveDefaultPath: with no explicit path and no file servers, the
// bridge-compatible default (~/.pigo/mcp.toml, overridable by PIGO_MCP_CONFIG /
// PIGO_HOME) is read.
func TestResolveDefaultPath(t *testing.T) {
	home := t.TempDir()
	t.Setenv("PIGO_HOME", home)
	t.Setenv("PIGO_MCP_CONFIG", "")
	if err := os.WriteFile(filepath.Join(home, "mcp.toml"), []byte(`
[[server]]
id = "legacy"
command = "legacy-server"
`), 0o644); err != nil {
		t.Fatal(err)
	}
	got := Resolve(Servers{}, "", false, nil)
	if len(got) != 1 || got[0].ID != "legacy" {
		t.Errorf("default layer = %+v, want the bridge-era legacy server", got)
	}
}

// TestManagerCloseIdempotent: Close disconnects everything and a second Close
// is a no-op.
func TestManagerCloseIdempotent(t *testing.T) {
	m := NewManager(nil, nil)
	c := newTestClient(t, "close-me", nil)
	m.Clients = append(m.Clients, c)
	if err := m.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}
	if err := m.Close(); err != nil {
		t.Fatalf("second Close: %v", err)
	}
	if len(m.Clients) != 0 {
		t.Error("Clients not cleared")
	}
}
