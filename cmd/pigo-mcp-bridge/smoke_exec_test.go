package main

import (
	"bytes"
	"context"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/smallnest/pigo/internal/plugin"
)

// TestExecSmokeBridgeEndToEnd runs the full plugin-protocol conversation
// against the REAL bridge executable and a REAL MCP echo server — the exact
// path pigo's plugin discovery takes (internal/plugin.Load → initialize →
// tools/call → shutdown). It is gated behind PIGO_BRIDGE_EXEC_SMOKE=1 because
// it builds and spawns executables; plain `go test ./...` skips it.
//
// The built exes land in <repo>/build (never committed; smoke artifacts only).
func TestExecSmokeBridgeEndToEnd(t *testing.T) {
	if os.Getenv("PIGO_BRIDGE_EXEC_SMOKE") != "1" {
		t.Skip("set PIGO_BRIDGE_EXEC_SMOKE=1 to run the executable smoke test")
	}
	if runtime.GOOS != "windows" {
		t.Skip("smoke exes are built with windows paths; run on Windows")
	}
	root, err := filepath.Abs(filepath.Join("..", ".."))
	if err != nil {
		t.Fatal(err)
	}
	bridgeExe := filepath.Join(root, "build", "pigo-mcp-bridge.exe")
	echoExe := filepath.Join(root, "build", "echomcp.exe")
	for _, tgt := range []struct{ out, pkg string }{
		{bridgeExe, "./cmd/pigo-mcp-bridge"},
		{echoExe, "./cmd/pigo-mcp-bridge/testdata/echomcp"},
	} {
		cmd := exec.Command("go", "build", "-o", tgt.out, tgt.pkg)
		cmd.Dir = root
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("go build %s: %v\n%s", tgt.pkg, err, out)
		}
	}

	t.Setenv("PIGO_MCP_CONFIG", filepath.Join(root, "build", "mcp-smoke.toml"))

	var stderr bytes.Buffer
	p, err := plugin.Load(bridgeExe, nil, &stderr)
	if err != nil {
		t.Fatalf("plugin.Load: %v\nbridge stderr:\n%s", err, stderr.String())
	}
	defer p.Close()

	if p.Manifest.Name != "pigo-mcp-bridge" {
		t.Errorf("manifest name = %q", p.Manifest.Name)
	}
	if len(p.Manifest.Tools) != 1 || p.Manifest.Tools[0].Name != "mcp_echo_echo" {
		t.Fatalf("manifest tools = %+v, want [mcp_echo_echo]", p.Manifest.Tools)
	}
	if !strings.Contains(stderr.String(), `server "echo" connected: 1 tools`) {
		t.Errorf("bridge stderr missing connect log:\n%s", stderr.String())
	}

	// Invoke the exposed tool the way the agent does and check the mapped
	// MCP text content comes back.
	tool := p.Tools()[0]
	result, err := tool.Execute(context.Background(), "smoke-1", json.RawMessage(`{"text":"hello"}`), nil)
	if err != nil {
		t.Fatalf("tool.Execute: %v", err)
	}
	blob, err := json.Marshal(result)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(blob), "echo: hello") {
		t.Errorf("tool result missing echoed text: %s", blob)
	}

	// Close sends the shutdown notification and must not hang: the bridge
	// exits and its MCP server child is torn down.
	if err := p.Close(); err != nil {
		t.Errorf("Close: %v", err)
	}
}
