package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"testing"
	"time"
)

// fakeRPC is an in-process rpcClient standing in for a spawned MCP server.
type fakeRPC struct {
	mu         sync.Mutex
	results    map[string]json.RawMessage // method -> canned result
	calls      []string
	lastParams map[string]json.RawMessage
	closed     bool
}

func newFakeRPC() *fakeRPC {
	return &fakeRPC{
		results:    make(map[string]json.RawMessage),
		lastParams: make(map[string]json.RawMessage),
	}
}

func (f *fakeRPC) Call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.calls = append(f.calls, method)
	if params != nil {
		raw, err := json.Marshal(params)
		if err != nil {
			return nil, err
		}
		f.lastParams[method] = raw
	}
	if raw, ok := f.results[method]; ok {
		return raw, nil
	}
	return nil, fmt.Errorf("unexpected call %q", method)
}

func (f *fakeRPC) Notify(method string, params any) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.calls = append(f.calls, method)
	return nil
}

func (f *fakeRPC) Close() error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.closed = true
	return nil
}

func TestMCPHandshake(t *testing.T) {
	fc := newFakeRPC()
	fc.results["initialize"] = json.RawMessage(`{"protocolVersion":"2024-11-05","capabilities":{},"serverInfo":{"name":"fake","version":"1"}}`)
	s := &mcpServer{id: "srv", client: fc}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if err := s.handshake(ctx, func(string, ...any) {}); err != nil {
		t.Fatalf("handshake: %v", err)
	}
	joined := strings.Join(fc.calls, ",")
	if !strings.Contains(joined, "initialize") {
		t.Errorf("initialize not called: %v", fc.calls)
	}
	if !strings.Contains(joined, "notifications/initialized") {
		t.Errorf("initialized notification missing: %v", fc.calls)
	}
	// clientInfo identifies the bridge.
	var sent map[string]any
	if err := json.Unmarshal(fc.lastParams["initialize"], &sent); err != nil {
		t.Fatal(err)
	}
	if sent["protocolVersion"] != mcpProtocolVersion {
		t.Errorf("protocolVersion = %v, want %v", sent["protocolVersion"], mcpProtocolVersion)
	}
}

func TestMCPListToolsPagination(t *testing.T) {
	fc := newFakeRPC()
	fc.results["initialize"] = json.RawMessage(`{"protocolVersion":"2024-11-05"}`)
	s := &mcpServer{id: "srv", client: &pagedFake{fc: fc}}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	quiet := func(string, ...any) {}
	if err := s.handshake(ctx, quiet); err != nil {
		t.Fatalf("handshake: %v", err)
	}
	if err := s.listTools(ctx); err != nil {
		t.Fatalf("listTools: %v", err)
	}
	if len(s.tools) != 2 || s.tools[0].Name != "echo" || s.tools[1].Name != "second" {
		t.Fatalf("tools across pages wrong: %+v", s.tools)
	}
	// pagedFake enforces that the second call carries cursor=page2 (it errors
	// otherwise); reaching here with both pages proves the pagination loop.
	if len(fc.calls) == 0 || fc.calls[0] != "initialize" {
		t.Errorf("call sequence wrong: %v", fc.calls)
	}
}

// pagedFake serves two tools/list pages, then any other canned result.
type pagedFake struct {
	fc    *fakeRPC
	pages int
}

func (p *pagedFake) Call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	if method == "tools/list" {
		p.pages++
		if p.pages == 1 {
			return json.RawMessage(`{"tools":[{"name":"echo","description":"Echoes.","inputSchema":{"type":"object"}}],"nextCursor":"page2"}`), nil
		}
		if params == nil {
			return nil, errors.New("expected cursor params on second tools/list call")
		}
		raw, _ := json.Marshal(params)
		var m map[string]string
		if json.Unmarshal(raw, &m) != nil || m["cursor"] != "page2" {
			return nil, fmt.Errorf("second tools/list must carry cursor=page2, got %s", raw)
		}
		return json.RawMessage(`{"tools":[{"name":"second"}]}`), nil
	}
	return p.fc.Call(ctx, method, params)
}

func (p *pagedFake) Notify(method string, params any) error { return p.fc.Notify(method, params) }
func (p *pagedFake) Close() error                           { return p.fc.Close() }

func TestCallToolMapping(t *testing.T) {
	fc := newFakeRPC()
	fc.results["tools/call"] = json.RawMessage(`{
		"content": [
			{"type":"text","text":"line one"},
			{"type":"image","data":"QUJD","mimeType":"image/png"},
			{"type":"resource_link","uri":"file:///x"},
			{"type":"holo"}
		]
	}`)
	s := &mcpServer{id: "srv", client: fc}
	res := s.callTool(context.Background(), "multi", json.RawMessage(`{"q":1}`))
	if res.IsError {
		t.Errorf("unexpected isError: %+v", res)
	}
	want := "line one\n[image: image/png, ~3 bytes]\n[resource link: file:///x]\n[unsupported content type: holo]"
	if res.Content != want {
		t.Errorf("content = %q, want %q", res.Content, want)
	}
	// Arguments forwarded verbatim under the server-side tool name.
	var sent mcpCallParams
	if err := json.Unmarshal(fc.lastParams["tools/call"], &sent); err != nil {
		t.Fatal(err)
	}
	if sent.Name != "multi" || string(sent.Arguments) != `{"q":1}` {
		t.Errorf("forwarded params wrong: %+v", sent)
	}
}

func TestCallToolIsErrorAndFallbacks(t *testing.T) {
	fc := newFakeRPC()
	fc.results["tools/call"] = json.RawMessage(`{"content":[],"isError":true,"structuredContent":{"result":42}}`)
	s := &mcpServer{id: "srv", client: fc}
	res := s.callTool(context.Background(), "bad", json.RawMessage(`null`))
	if !res.IsError {
		t.Errorf("isError must be passed through, got %+v", res)
	}
	if res.Content != `{"result":42}` {
		t.Errorf("structuredContent fallback = %q", res.Content)
	}
	// null arguments are omitted from the forwarded params.
	if raw := fc.lastParams["tools/call"]; strings.Contains(string(raw), "arguments") {
		t.Errorf("null arguments should be omitted, got %s", raw)
	}

	// Transport error degrades to an error result, not a panic.
	res = s.callTool(context.Background(), "boom", nil)
	if !res.IsError || res.Content == "" {
		t.Errorf("transport failure must yield error result, got %+v", res)
	}
}

func TestSafeToken(t *testing.T) {
	cases := map[string]string{
		"filesystem": "filesystem",
		"a-b.c/d":    "a_b_c_d",
		"":           "_",
		"中文":         "__",
	}
	for in, want := range cases {
		if got := safeToken(in); got != want {
			t.Errorf("safeToken(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestResolveCommandWindows(t *testing.T) {
	if runtime.GOOS != "windows" {
		t.Skip("windows-only resolution rules")
	}
	dir := t.TempDir()
	batch := filepath.Join(dir, "fakenv.cmd")
	if err := os.WriteFile(batch, []byte("@echo off\r\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", dir)

	// Bare name resolving to a .cmd is routed through cmd /c, preserving args.
	cmd, args := resolveCommand("fakenv", []string{"-y"})
	if cmd != "cmd" || len(args) != 3 || args[0] != "/c" || args[1] != batch || args[2] != "-y" {
		t.Errorf("resolve fakenv = (%q, %v), want (cmd, [/c %s -y])", cmd, args, batch)
	}

	// A real executable passes through unchanged.
	cmd, _ = resolveCommand("C:\\Tools\\server.exe", nil)
	if cmd != "C:\\Tools\\server.exe" {
		t.Errorf("exe must pass through, got %q", cmd)
	}

	// An unknown bare name is left alone (jsonrpc surfaces the launch error).
	cmd, _ = resolveCommand("definitely-not-on-path-xyz", nil)
	if cmd != "definitely-not-on-path-xyz" {
		t.Errorf("unknown command must pass through, got %q", cmd)
	}
}
