package main

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"

	"github.com/smallnest/pigo/internal/plugin"
)

// runServe drives the bridge serve loop over in-memory stdin and parses the
// NDJSON responses back out.
func runServe(t *testing.T, b *bridge, input string) ([]parsedResponse, error) {
	t.Helper()
	var out bytes.Buffer
	err := b.serve(strings.NewReader(input), &out)
	var responses []parsedResponse
	for _, line := range strings.Split(strings.TrimSpace(out.String()), "\n") {
		if strings.TrimSpace(line) == "" {
			continue
		}
		var resp parsedResponse
		if err := json.Unmarshal([]byte(line), &resp); err != nil {
			t.Fatalf("unparseable response line %q: %v", line, err)
		}
		responses = append(responses, resp)
	}
	return responses, err
}

// parsedResponse is the decode-side mirror of outgoingResponse (whose Result
// is an encode-side any).
type parsedResponse struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Result  json.RawMessage `json:"result"`
	Error   *rpcError       `json:"error"`
}

func newTestBridge(t *testing.T, fc *fakeRPC) *bridge {
	t.Helper()
	b := newBridge(nil, func(string, ...any) {})
	b.servers = []*mcpServer{
		{
			id:     "echo",
			client: fc,
			tools: []mcpTool{{
				Name:        "echo",
				Description: "Echo the given text back.",
				InputSchema: json.RawMessage(`{"type":"object","properties":{"text":{"type":"string"}}}`),
			}},
		},
	}
	b.buildManifest()
	return b
}

func TestServeEndToEnd(t *testing.T) {
	fc := newFakeRPC()
	fc.results["tools/call"] = json.RawMessage(`{"content":[{"type":"text","text":"echo: hello"}]}`)
	b := newTestBridge(t, fc)

	input := strings.Join([]string{
		`{"jsonrpc":"2.0","id":1,"method":"initialize"}`,
		`{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"mcp_echo_echo","arguments":{"text":"hello"}}}`,
		`{"jsonrpc":"2.0","method":"shutdown"}`,
	}, "\n")

	responses, err := runServe(t, b, input)
	if err != nil {
		t.Fatalf("serve: %v", err)
	}
	if len(responses) != 2 {
		t.Fatalf("got %d responses, want 2 (shutdown is a notification): %+v", len(responses), responses)
	}

	// initialize → manifest with the namespaced tool.
	var m plugin.Manifest
	if err := json.Unmarshal(responses[0].Result, &m); err != nil {
		t.Fatalf("decode manifest: %v", err)
	}
	if m.Name != "pigo-mcp-bridge" || len(m.Tools) != 1 || m.Tools[0].Name != "mcp_echo_echo" {
		t.Errorf("manifest wrong: %+v", m)
	}

	// tools/call → forwarded and mapped back into CallResult.
	var cr plugin.CallResult
	if err := json.Unmarshal(responses[1].Result, &cr); err != nil {
		t.Fatalf("decode call result: %v", err)
	}
	if cr.Content != "echo: hello" || cr.IsError {
		t.Errorf("call result wrong: %+v", cr)
	}
	// The MCP server received its own tool name with verbatim arguments.
	var sent mcpCallParams
	if err := json.Unmarshal(fc.lastParams["tools/call"], &sent); err != nil {
		t.Fatal(err)
	}
	if sent.Name != "echo" || string(sent.Arguments) != `{"text":"hello"}` {
		t.Errorf("forwarded params wrong: %+v", sent)
	}
	// shutdown notification closed the MCP server.
	if !fc.closed {
		t.Errorf("shutdown must close the MCP server client")
	}
}

func TestServeResponseEnvelope(t *testing.T) {
	fc := newFakeRPC()
	b := newTestBridge(t, fc)

	input := `{"jsonrpc":"2.0","id":7,"method":"initialize"}`
	var out bytes.Buffer
	if err := b.serve(strings.NewReader(input), &out); err != nil {
		t.Fatal(err)
	}
	line := strings.TrimSpace(out.String())
	if !strings.Contains(line, `"jsonrpc":"2.0"`) || !strings.Contains(line, `"id":7`) {
		t.Errorf("response must echo jsonrpc version and raw id, got %q", line)
	}
}

func TestServeUnknownMethodAndParseError(t *testing.T) {
	fc := newFakeRPC()
	b := newTestBridge(t, fc)

	input := strings.Join([]string{
		`{"jsonrpc":"2.0","id":1,"method":"commands/call","params":{"name":"x"}}`,
		`this is not json`,
	}, "\n")
	responses, err := runServe(t, b, input)
	if err != nil {
		t.Fatalf("serve must keep going after a parse error: %v", err)
	}
	if len(responses) != 2 {
		t.Fatalf("got %d responses, want 2: %+v", len(responses), responses)
	}
	if responses[0].Error == nil || responses[0].Error.Code != codeMethodNotFound {
		t.Errorf("unknown method must yield -32601, got %+v", responses[0])
	}
	if responses[1].Error == nil || responses[1].Error.Code != codeParseError {
		t.Errorf("parse error must yield -32700 with null id, got %+v", responses[1])
	}
	if string(responses[1].ID) != "null" {
		t.Errorf("parse-error response id = %s, want null", responses[1].ID)
	}
}

func TestServeStdinEOF(t *testing.T) {
	fc := newFakeRPC()
	b := newTestBridge(t, fc)
	if _, err := runServe(t, b, ""); err != nil {
		t.Fatalf("serve on empty stdin: %v", err)
	}
	if !fc.closed {
		t.Errorf("stdin EOF must close the MCP server client")
	}
}

func TestServeRequestShapedShutdown(t *testing.T) {
	fc := newFakeRPC()
	b := newTestBridge(t, fc)
	responses, err := runServe(t, b, `{"jsonrpc":"2.0","id":9,"method":"shutdown"}`)
	if err != nil {
		t.Fatalf("serve: %v", err)
	}
	if len(responses) != 1 {
		t.Fatalf("shutdown request must be answered before exit, got %+v", responses)
	}
	if !fc.closed {
		t.Errorf("shutdown must close the MCP server client")
	}
}

// TestServeLargePayload guards the 16MB stdin line cap: a schema line bigger
// than the default bufio limit must still parse.
func TestServeLargePayload(t *testing.T) {
	fc := newFakeRPC()
	b := newBridge(nil, func(string, ...any) {})
	big := strings.Repeat("x", 2*1024*1024)
	b.servers = []*mcpServer{
		{id: "big", client: fc, tools: []mcpTool{{Name: "big", Description: big}}},
	}
	b.buildManifest()

	responses, err := runServe(t, b, `{"jsonrpc":"2.0","id":1,"method":"initialize"}`)
	if err != nil {
		t.Fatalf("serve: %v", err)
	}
	if len(responses) != 1 {
		t.Fatalf("responses = %d", len(responses))
	}
	var m plugin.Manifest
	if err := json.Unmarshal(responses[0].Result, &m); err != nil {
		t.Fatalf("decode manifest: %v", err)
	}
	if len(m.Tools) != 1 || len(m.Tools[0].Description) != len(big) {
		t.Errorf("large description truncated: %d bytes", len(m.Tools[0].Description))
	}
}
