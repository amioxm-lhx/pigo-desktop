package mcp

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"sync"
	"testing"
)

// fakeRPC is an in-process rpcClient standing in for an MCP server. It records
// every method it is asked and replies from a handler table, so protocol
// behavior (handshake shape, pagination, call mapping, resources) can be
// asserted without spawning a subprocess.
type fakeRPC struct {
	mu        sync.Mutex
	methods   []string
	handlers  map[string]func(params json.RawMessage) (any, error)
	notifyErr error
	callErr   error
	closed    bool
}

func (f *fakeRPC) Call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	f.mu.Lock()
	f.methods = append(f.methods, method)
	h := f.handlers[method]
	callErr := f.callErr
	f.mu.Unlock()
	if callErr != nil {
		return nil, callErr
	}
	raw, _ := json.Marshal(params)
	if h == nil {
		return nil, fmt.Errorf("fakeRPC: no handler for %q", method)
	}
	res, err := h(raw)
	if err != nil {
		return nil, err
	}
	out, _ := json.Marshal(res)
	return out, nil
}

func (f *fakeRPC) Notify(method string, params any) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.methods = append(f.methods, method)
	return f.notifyErr
}

func (f *fakeRPC) Close() error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.closed = true
	return nil
}

func (f *fakeRPC) saw(method string) bool {
	f.mu.Lock()
	defer f.mu.Unlock()
	for _, m := range f.methods {
		if m == method {
			return true
		}
	}
	return false
}

// TestClientHandshake pins the initialize request shape (protocol version
// 2024-11-05, pigo clientInfo) and the initialized notification.
func TestClientHandshake(t *testing.T) {
	rpc := &fakeRPC{handlers: map[string]func(json.RawMessage) (any, error){
		"initialize": func(params json.RawMessage) (any, error) {
			var p map[string]any
			if err := json.Unmarshal(params, &p); err != nil {
				return nil, err
			}
			if p["protocolVersion"] != ProtocolVersion {
				t.Errorf("protocolVersion = %v, want %s", p["protocolVersion"], ProtocolVersion)
			}
			info, _ := p["clientInfo"].(map[string]any)
			if info == nil || info["name"] != clientName {
				t.Errorf("clientInfo = %v, want name %s", p["clientInfo"], clientName)
			}
			return map[string]any{"protocolVersion": ProtocolVersion, "capabilities": map[string]any{}}, nil
		},
		"tools/list": func(json.RawMessage) (any, error) {
			return map[string]any{"tools": []any{}}, nil
		},
	}}
	var logs []string
	c, err := newClient("echo", rpc, func(format string, args ...any) {
		logs = append(logs, fmt.Sprintf(format, args...))
	})
	if err != nil {
		t.Fatalf("newClient: %v", err)
	}
	if c.ID() != "echo" {
		t.Errorf("ID = %q", c.ID())
	}
	if !rpc.saw("notifications/initialized") {
		t.Error("initialized notification not sent")
	}
	if len(logs) != 0 {
		t.Errorf("unexpected mismatch logs: %v", logs)
	}
}

// TestClientVersionMismatchLogged: a server speaking a different protocol
// version logs and proceeds (the desktop client's behavior).
func TestClientVersionMismatchLogged(t *testing.T) {
	rpc := &fakeRPC{handlers: map[string]func(json.RawMessage) (any, error){
		"initialize": func(json.RawMessage) (any, error) {
			return map[string]any{"protocolVersion": "1999-01-01"}, nil
		},
		"tools/list": func(json.RawMessage) (any, error) {
			return map[string]any{"tools": []any{}}, nil
		},
	}}
	var logs []string
	if _, err := newClient("old", rpc, func(format string, args ...any) {
		logs = append(logs, fmt.Sprintf(format, args...))
	}); err != nil {
		t.Fatalf("newClient: %v", err)
	}
	if len(logs) != 1 || !strings.Contains(logs[0], "1999-01-01") {
		t.Errorf("mismatch not logged: %v", logs)
	}
}

// TestClientListToolsPagination follows a two-page cursor and verifies the
// merged list; a server that never stops paging stops at the page cap.
func TestClientListToolsPagination(t *testing.T) {
	page := 0
	rpc := &fakeRPC{handlers: map[string]func(json.RawMessage) (any, error){
		"initialize": func(json.RawMessage) (any, error) {
			return map[string]any{"protocolVersion": ProtocolVersion}, nil
		},
		"tools/list": func(params json.RawMessage) (any, error) {
			page++
			tool := map[string]any{"name": fmt.Sprintf("t%d", page), "inputSchema": map[string]any{"type": "object"}}
			if page == 1 {
				return map[string]any{"tools": []any{tool}, "nextCursor": "page2"}, nil
			}
			return map[string]any{"tools": []any{tool}}, nil
		},
	}}
	c, err := newClient("paged", rpc, nil)
	if err != nil {
		t.Fatalf("newClient: %v", err)
	}
	tools := c.Tools()
	if len(tools) != 2 || tools[0].Name != "t1" || tools[1].Name != "t2" {
		t.Errorf("tools = %+v, want [t1 t2]", tools)
	}
}

// TestClientListToolsNeverEnding: a server that keeps sending cursors stops
// after maxListPages instead of looping forever.
func TestClientListToolsNeverEnding(t *testing.T) {
	rpc := &fakeRPC{handlers: map[string]func(json.RawMessage) (any, error){
		"initialize": func(json.RawMessage) (any, error) {
			return map[string]any{"protocolVersion": ProtocolVersion}, nil
		},
		"tools/list": func(json.RawMessage) (any, error) {
			return map[string]any{"tools": []any{}, "nextCursor": "more"}, nil
		},
	}}
	var logs []string
	c, err := newClient("loop", rpc, func(format string, args ...any) {
		logs = append(logs, fmt.Sprintf(format, args...))
	})
	if err != nil {
		t.Fatalf("newClient: %v", err)
	}
	count := 0
	rpc.mu.Lock()
	count = len(rpc.methods)
	rpc.mu.Unlock()
	if count > 1+maxListPages+1 { // initialize + pages (+ notifications slack)
		t.Errorf("listTools looped past the page cap: %d rpc calls", count)
	}
	if len(c.Notices()) == 0 {
		t.Error("pagination cap produced no notice")
	}
}

// TestClientToolCap: a server exposing more than MaxToolsPerServer tools is
// truncated with a notice.
func TestClientToolCap(t *testing.T) {
	tools := make([]any, MaxToolsPerServer+10)
	for i := range tools {
		tools[i] = map[string]any{"name": fmt.Sprintf("t%d", i), "inputSchema": map[string]any{"type": "object"}}
	}
	rpc := &fakeRPC{handlers: map[string]func(json.RawMessage) (any, error){
		"initialize": func(json.RawMessage) (any, error) {
			return map[string]any{"protocolVersion": ProtocolVersion}, nil
		},
		"tools/list": func(json.RawMessage) (any, error) {
			return map[string]any{"tools": tools}, nil
		},
	}}
	c, err := newClient("big", rpc, nil)
	if err != nil {
		t.Fatalf("newClient: %v", err)
	}
	if len(c.Tools()) != MaxToolsPerServer {
		t.Errorf("tools = %d, want capped at %d", len(c.Tools()), MaxToolsPerServer)
	}
	if len(c.Notices()) == 0 {
		t.Error("cap produced no notice")
	}
}

// TestClientCallToolMapping covers tools/call forwarding and reply decoding,
// including isError and structuredContent passthrough.
func TestClientCallToolMapping(t *testing.T) {
	rpc := &fakeRPC{handlers: map[string]func(json.RawMessage) (any, error){
		"initialize": func(json.RawMessage) (any, error) {
			return map[string]any{"protocolVersion": ProtocolVersion}, nil
		},
		"tools/list": func(json.RawMessage) (any, error) {
			return map[string]any{"tools": []any{}}, nil
		},
		"tools/call": func(params json.RawMessage) (any, error) {
			var p callParams
			if err := json.Unmarshal(params, &p); err != nil {
				return nil, err
			}
			if p.Name != "echo" {
				t.Errorf("call name = %q, want echo", p.Name)
			}
			var args map[string]any
			if err := json.Unmarshal(p.Arguments, &args); err != nil {
				return nil, err
			}
			if args["text"] != "hi" {
				t.Errorf("arguments = %v, want text=hi", args)
			}
			return map[string]any{
				"content":           []map[string]any{{"type": "text", "text": "echo: hi"}},
				"isError":           true,
				"structuredContent": map[string]any{"n": 1},
			}, nil
		},
	}}
	c, err := newClient("echo", rpc, nil)
	if err != nil {
		t.Fatalf("newClient: %v", err)
	}
	res, err := c.CallTool(context.Background(), "echo", json.RawMessage(`{"text":"hi"}`))
	if err != nil {
		t.Fatalf("CallTool: %v", err)
	}
	if !res.IsError {
		t.Error("isError lost")
	}
	if len(res.Content) != 1 || res.Content[0].Type != "text" || res.Content[0].Text != "echo: hi" {
		t.Errorf("content = %+v", res.Content)
	}
	if string(res.Structured) == "" {
		t.Error("structuredContent lost")
	}
}

// TestClientResources covers the minimal US-015 surface: paginated
// resources/list and resources/read.
func TestClientResources(t *testing.T) {
	rpc := &fakeRPC{handlers: map[string]func(json.RawMessage) (any, error){
		"initialize": func(json.RawMessage) (any, error) {
			return map[string]any{"protocolVersion": ProtocolVersion}, nil
		},
		"tools/list": func(json.RawMessage) (any, error) {
			return map[string]any{"tools": []any{}}, nil
		},
		"resources/list": func(params json.RawMessage) (any, error) {
			var p map[string]string
			_ = json.Unmarshal(params, &p)
			if p["cursor"] == "" {
				return map[string]any{"resources": []map[string]any{{"uri": "file:///a.txt", "name": "a"}}, "nextCursor": "c2"}, nil
			}
			return map[string]any{"resources": []map[string]any{{"uri": "file:///b.txt", "name": "b"}}}, nil
		},
		"resources/read": func(params json.RawMessage) (any, error) {
			var p map[string]string
			_ = json.Unmarshal(params, &p)
			if p["uri"] != "file:///a.txt" {
				t.Errorf("read uri = %q", p["uri"])
			}
			return map[string]any{"contents": []map[string]any{{"type": "text", "text": "data"}}}, nil
		},
	}}
	c, err := newClient("res", rpc, nil)
	if err != nil {
		t.Fatalf("newClient: %v", err)
	}
	res, err := c.ListResources(context.Background())
	if err != nil {
		t.Fatalf("ListResources: %v", err)
	}
	if len(res) != 2 || res[0].URI != "file:///a.txt" || res[1].URI != "file:///b.txt" {
		t.Errorf("resources = %+v", res)
	}
	blocks, err := c.ReadResource(context.Background(), "file:///a.txt")
	if err != nil {
		t.Fatalf("ReadResource: %v", err)
	}
	if len(blocks) != 1 || blocks[0].Text != "data" {
		t.Errorf("contents = %+v", blocks)
	}
}

// TestClientClose: Close reaches the transport (tearing the subprocess down).
func TestClientClose(t *testing.T) {
	rpc := &fakeRPC{handlers: map[string]func(json.RawMessage) (any, error){
		"initialize": func(json.RawMessage) (any, error) {
			return map[string]any{"protocolVersion": ProtocolVersion}, nil
		},
		"tools/list": func(json.RawMessage) (any, error) {
			return map[string]any{"tools": []any{}}, nil
		},
	}}
	c, err := newClient("close", rpc, nil)
	if err != nil {
		t.Fatalf("newClient: %v", err)
	}
	if err := c.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}
	rpc.mu.Lock()
	closed := rpc.closed
	rpc.mu.Unlock()
	if !closed {
		t.Error("transport Close not called")
	}
}
