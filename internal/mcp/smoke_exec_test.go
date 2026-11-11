// Gated end-to-end smoke test: builds the echo MCP server fixture and runs the
// real client against it — launch, handshake, tools/list, tools/call, project
// layer trust gate, and clean shutdown. Following the bridge's smoke pattern
// (cmd/pigo-mcp-bridge/smoke_exec_test.go), it is skipped unless
// PIGO_MCP_EXEC_SMOKE=1 because go test inside a sandbox cannot always exec
// freshly built subprocesses.
//
// go test ./internal/mcp -run TestExecSmoke -count=1 -tags=smoketests
package mcp

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/smallnest/pigo/internal/agentcore"
)

// agentcoreTextContent narrows the Content interface to the text block the
// echo fixture produces.
type agentcoreTextContent = agentcore.TextContent

func TestExecSmokeEchoServer(t *testing.T) {
	if os.Getenv("PIGO_MCP_EXEC_SMOKE") != "1" {
		t.Skip("set PIGO_MCP_EXEC_SMOKE=1 to run the real-exec MCP smoke test")
	}
	// Build the echo fixture.
	exe := filepath.Join(t.TempDir(), "echomcp"+binSuffix())
	build := exec.Command("go", "build", "-o", exe, "../../cmd/pigo-mcp-bridge/testdata/echomcp")
	if out, err := build.CombinedOutput(); err != nil {
		t.Fatalf("build fixture: %v\n%s", err, out)
	}

	// Project layer with the echo server; marked trusted by pointing trust at
	// this directory through the store-less test path: Resolve accepts the
	// trusted flag directly, mirroring run.Trusted's decision.
	proj := t.TempDir()
	if err := os.MkdirAll(filepath.Join(proj, ".pigo"), 0o755); err != nil {
		t.Fatal(err)
	}
	projCfg := filepath.Join(proj, ".pigo", "mcp.toml")
	if err := os.WriteFile(projCfg, []byte("[[server]]\nid = \"echo\"\ncommand = \""+filepath.ToSlash(exe)+"\"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	var logs []string
	mgr := ConnectAll(Servers{}, proj, true /*trusted*/, nil, func(format string, args ...any) {
		logs = append(logs, strings.TrimSpace(fmt.Sprintf(format, args...)))
	})
	if mgr == nil {
		t.Fatalf("manager is nil; logs: %s", strings.Join(logs, "\n"))
	}
	tools := mgr.Tools()
	if len(tools) != 1 {
		t.Fatalf("tools = %d, want 1 (echo); logs: %s", len(tools), strings.Join(logs, "\n"))
	}
	tool := tools[0]
	if tool.Name() != "mcp_echo_echo" {
		t.Errorf("tool name = %q, want mcp_echo_echo", tool.Name())
	}
	if se, ok := tool.(interface{ HasSideEffects() bool }); !ok || !se.HasSideEffects() {
		t.Error("mcp tool must declare side effects")
	}
	// Execute degrades errors to results, so a Go error here is always a bug.
	result, err := tool.Execute(context.Background(), "smoke-1", json.RawMessage(`{"text":"hi"}`), nil)
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}
	if len(result.Content) == 0 {
		t.Fatalf("empty content: %+v", result)
	}
	if txt, ok := result.Content[0].(agentcoreTextContent); !ok || !strings.Contains(txt.Text, "echo: hi") {
		t.Errorf("content[0] = %#v, want text containing \"echo: hi\"", result.Content[0])
	}
	if err := mgr.Close(); err != nil {
		t.Errorf("Close: %v", err)
	}
}

// binSuffix adds the Windows executable extension on Windows.
func binSuffix() string {
	if os.PathSeparator == '\\' {
		return ".exe"
	}
	return ""
}
