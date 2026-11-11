package mcp

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
)

// TestCheckEndpointSecurity pins the §3.7.4 gate: https always allowed; plain
// http only for loopback hosts or an explicit allow_insecure opt-in; other
// schemes refused.
func TestCheckEndpointSecurity(t *testing.T) {
	cases := []struct {
		url        string
		insecure   bool
		wantErrSub string // empty = allowed
	}{
		{"https://mcp.example.com/mcp", false, ""},
		{"http://127.0.0.1:8765/mcp", false, ""},
		{"http://127.7.7.7:8765/mcp", false, ""}, // 127/8 is loopback
		{"http://[::1]:8765/mcp", false, ""},
		{"http://localhost:8765/mcp", false, ""},
		{"http://mcp.example.com/mcp", false, "use https:// or set allow_insecure"},
		{"http://mcp.example.com/mcp", true, ""}, // explicit opt-in
		{"ftp://mcp.example.com", false, "unsupported scheme"},
		{"ftp://mcp.example.com", true, "unsupported scheme"}, // opt-in does not widen the scheme list
	}
	for _, c := range cases {
		err := checkEndpointSecurity(c.url, c.insecure)
		if c.wantErrSub == "" {
			if err != nil {
				t.Errorf("checkEndpointSecurity(%q, insecure=%v) = %v, want allowed", c.url, c.insecure, err)
			}
			continue
		}
		if err == nil || !strings.Contains(err.Error(), c.wantErrSub) {
			t.Errorf("checkEndpointSecurity(%q, insecure=%v) = %v, want error containing %q", c.url, c.insecure, err, c.wantErrSub)
		}
	}
}

// TestServerValidateHTTPGate: the endpoint gate also runs at config-validate
// time, so a non-loopback plain-http entry is rejected before any dialing.
func TestServerValidateHTTPGate(t *testing.T) {
	s := Server{ID: "remote", Transport: "http", URL: "http://mcp.example.com/mcp"}
	if err := s.validate(); err == nil || !strings.Contains(err.Error(), "allow_insecure") {
		t.Errorf("validate = %v, want the §3.7.4 gate error", err)
	}
	insecure := true
	s.AllowInsecureFlag = &insecure
	if err := s.validate(); err != nil {
		t.Errorf("validate with allow_insecure = %v, want allowed", err)
	}
}

// httpTestServer is a minimal in-test MCP server over Streamable HTTP: it
// issues a session id at initialize, asserts the session header on later
// calls, and can stream any reply as SSE instead of plain JSON.
type httpTestServer struct {
	t        *testing.T
	srv      *httptest.Server
	mu       sync.Mutex
	session  string
	asSSE    string // method name whose reply is streamed as SSE; empty = never
	requests []string
}

func newHTTPTestServer(t *testing.T, asSSE string) *httpTestServer {
	t.Helper()
	h := &httpTestServer{t: t, asSSE: asSSE}
	h.srv = httptest.NewServer(http.HandlerFunc(h.handle))
	t.Cleanup(h.srv.Close)
	return h
}

func (h *httpTestServer) handle(w http.ResponseWriter, r *http.Request) {
	body, _ := io.ReadAll(r.Body)
	var env struct {
		ID     json.RawMessage `json:"id"`
		Method string          `json:"method"`
	}
	_ = json.Unmarshal(body, &env)
	h.mu.Lock()
	h.requests = append(h.requests, env.Method)
	session := h.session
	h.mu.Unlock()

	// Header assertions apply to JSON-RPC requests only; the DELETE of Close()
	// carries neither header.
	if env.Method != "" {
		if env.Method != "initialize" && session != "" {
			if got := r.Header.Get("Mcp-Session-Id"); got != session {
				h.t.Errorf("%s: session header = %q, want %q", env.Method, got, session)
			}
		}
		if r.Header.Get("MCP-Protocol-Version") != ProtocolVersion {
			h.t.Errorf("%s: missing MCP-Protocol-Version header", env.Method)
		}
		accept := r.Header.Get("Accept")
		if !strings.Contains(accept, "text/event-stream") || !strings.Contains(accept, "application/json") {
			h.t.Errorf("%s: Accept = %q, want both json and event-stream", env.Method, accept)
		}
	}

	reply := func(result any) {
		env2 := map[string]any{"jsonrpc": "2.0", "id": json.RawMessage(env.ID), "result": result}
		payload, _ := json.Marshal(env2)
		// SSE reply when the method matches; the stream carries the reply and
		// closes, per the single-response Streamable HTTP shape.
		if h.asSSEFor(env.Method) {
			w.Header().Set("Content-Type", "text/event-stream")
			fmt.Fprintf(w, "event: message\ndata: %s\n\n", payload)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write(payload)
	}

	switch env.Method {
	case "initialize":
		w.Header().Set("Mcp-Session-Id", "sess-123")
		h.mu.Lock()
		h.session = "sess-123"
		h.mu.Unlock()
		reply(map[string]any{"protocolVersion": ProtocolVersion, "capabilities": map[string]any{}})
	case "notifications/initialized":
		w.WriteHeader(http.StatusAccepted)
	case "tools/list":
		reply(map[string]any{"tools": []map[string]any{{
			"name":        "echo",
			"description": "Echo back",
			"inputSchema": map[string]any{"type": "object", "properties": map[string]any{"text": map[string]any{"type": "string"}}},
		}}})
	case "tools/call":
		reply(map[string]any{"content": []map[string]any{{"type": "text", "text": "http echo"}}, "isError": false})
	case "resources/list":
		reply(map[string]any{"resources": []map[string]any{{"uri": "file:///x", "name": "x"}}})
	case "resources/read":
		reply(map[string]any{"contents": []map[string]any{{"type": "text", "text": "x-body"}}})
	default:
		w.WriteHeader(http.StatusBadRequest)
	}
}

func (h *httpTestServer) asSSEFor(method string) bool {
	h.mu.Lock()
	defer h.mu.Unlock()
	return h.asSSE == method
}

func (h *httpTestServer) saw(method string) bool {
	h.mu.Lock()
	defer h.mu.Unlock()
	for _, m := range h.requests {
		if m == method {
			return true
		}
	}
	return false
}

// TestClientOverHTTP exercises the whole client over Streamable HTTP:
// handshake + session header, tools/list, tools/call, resources, and the
// DELETE on Close.
func TestClientOverHTTP(t *testing.T) {
	h := newHTTPTestServer(t, "")
	cfg := Server{ID: "echo-http", Transport: "http", URL: h.srv.URL}
	c, err := Dial(cfg, nil, nil)
	if err != nil {
		t.Fatalf("Dial: %v", err)
	}
	tools := c.Tools()
	if len(tools) != 1 || tools[0].Name != "echo" {
		t.Fatalf("tools = %+v", tools)
	}
	res, err := c.CallTool(context.Background(), "echo", json.RawMessage(`{"text":"hi"}`))
	if err != nil {
		t.Fatalf("CallTool: %v", err)
	}
	if len(res.Content) != 1 || res.Content[0].Text != "http echo" {
		t.Errorf("content = %+v", res.Content)
	}
	rs, err := c.ListResources(context.Background())
	if err != nil {
		t.Fatalf("ListResources: %v", err)
	}
	if len(rs) != 1 || rs[0].URI != "file:///x" {
		t.Errorf("resources = %+v", rs)
	}
	if err := c.Close(); err != nil {
		t.Errorf("Close: %v", err)
	}
	for _, method := range []string{"initialize", "notifications/initialized", "tools/list", "tools/call", "resources/list"} {
		if !h.saw(method) {
			t.Errorf("server never saw %s", method)
		}
	}
}

// TestClientOverHTTPSSE: the same flow when the server streams its reply as
// SSE instead of plain JSON — the client must find its id inside the stream.
func TestClientOverHTTPSSE(t *testing.T) {
	h := newHTTPTestServer(t, "tools/call")
	cfg := Server{ID: "echo-http", Transport: "http", URL: h.srv.URL}
	c, err := Dial(cfg, nil, nil)
	if err != nil {
		t.Fatalf("Dial: %v", err)
	}
	res, err := c.CallTool(context.Background(), "echo", json.RawMessage(`{}`))
	if err != nil {
		t.Fatalf("CallTool (SSE reply): %v", err)
	}
	if len(res.Content) != 1 || res.Content[0].Text != "http echo" {
		t.Errorf("content = %+v", res.Content)
	}
}

// TestDialHTTPGateFailsFast: a non-loopback plain-http endpoint fails at dial
// time — before any network I/O — with the gate message.
func TestDialHTTPGateFailsFast(t *testing.T) {
	cfg := Server{ID: "remote", Transport: "http", URL: "http://mcp.example.com/mcp"}
	_, err := Dial(cfg, nil, nil)
	if err == nil || !strings.Contains(err.Error(), "allow_insecure") {
		t.Errorf("Dial = %v, want the gate error without any connection attempt", err)
	}
}
