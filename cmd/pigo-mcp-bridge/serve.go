// This file implements the plugin-protocol SERVER side of the bridge — the
// role pigo's own plugin client (internal/plugin) talks to. internal/jsonrpc
// only ships a client, so the stdin NDJSON serve loop lives here. It also owns
// the aggregation logic: connected MCP servers → one plugin Manifest whose
// tools are namespaced mcp_<server>_<tool> and routed back to the right server
// on tools/call.
package main

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"sync"

	"github.com/smallnest/pigo/internal/jsonrpc"
	"github.com/smallnest/pigo/internal/plugin"
)

// JSON-RPC error codes the bridge answers with (server-side only).
const (
	codeParseError     = -32700
	codeInvalidRequest = -32600
	codeMethodNotFound = -32601
	codeInvalidParams  = -32602
	codeServerError    = -32000
)

// bridge wires the plugin protocol to the configured MCP servers.
type bridge struct {
	cfg      []ServerConfig
	servers  []*mcpServer // index-aligned with cfg; a nil entry = that server failed to connect
	index    map[string]toolRoute
	manifest plugin.Manifest
	logf     func(string, ...any)
}

// toolRoute maps one exposed tool name back to the owning server and the
// server's original (un-namespaced) tool name.
type toolRoute struct {
	server *mcpServer
	tool   string
}

func newBridge(cfg []ServerConfig, logf func(string, ...any)) *bridge {
	return &bridge{cfg: cfg, logf: logf}
}

// connectAll connects every configured server in parallel. Failures are
// logged and leave a nil slot — the bridge still serves the servers that came
// up (fault tolerance, docs §2.2).
func (b *bridge) connectAll() {
	b.servers = make([]*mcpServer, len(b.cfg))
	var wg sync.WaitGroup
	for i, cfg := range b.cfg {
		wg.Add(1)
		go func(i int, cfg ServerConfig) {
			defer wg.Done()
			s, err := connectMCP(cfg, os.Stderr, b.logf)
			if err != nil {
				b.logf("server %q unavailable: %v", cfg.ID, err)
				return
			}
			b.servers[i] = s
			b.logf("server %q connected: %d tools", cfg.ID, len(s.tools))
		}(i, cfg)
	}
	wg.Wait()
}

// buildManifest aggregates every connected server's tools into the plugin
// Manifest pigo sees on initialize, and builds the call-routing index. Tool
// names are mcp_<safe(server)>_<safe(tool)> (mirroring the desktop client's
// namespace rule); collisions after sanitization and the bridge-wide tool cap
// are resolved first-wins with a logged warning, because pigo drops duplicate
// registry entries silently.
func (b *bridge) buildManifest() {
	tools := make([]plugin.ToolSpec, 0, 16)
	b.index = make(map[string]toolRoute)
	seen := make(map[string]string)

loop:
	for _, s := range b.servers {
		if s == nil {
			continue
		}
		for _, t := range s.tools {
			if len(tools) >= maxTools {
				b.logf("exposed tool cap %d reached: server %q tool %q and everything after is NOT exposed", maxTools, s.id, t.Name)
				break loop
			}
			name := "mcp_" + safeToken(s.id) + "_" + safeToken(t.Name)
			if prev, dup := seen[name]; dup {
				b.logf("tool name collision: server %q tool %q maps to %q already owned by server %q — keeping the first", s.id, t.Name, name, prev)
				continue
			}
			seen[name] = s.id
			tools = append(tools, plugin.ToolSpec{
				Name:        name,
				Description: t.Description,
				Schema:      safeSchema(t.InputSchema),
			})
			b.index[name] = toolRoute{server: s, tool: t.Name}
		}
	}

	b.manifest = plugin.Manifest{
		Name:    "pigo-mcp-bridge",
		Version: bridgeVersion,
		Tools:   tools,
	}
}

// safeSchema guarantees the schema pigo's ToolRegistry receives is a JSON
// object. pigo silently drops tools whose schema fails schema compilation, so
// a malformed or missing MCP inputSchema degrades to a permissive object
// schema instead of the tool quietly disappearing (docs §1.1).
func safeSchema(schema json.RawMessage) json.RawMessage {
	var obj map[string]any
	if len(schema) > 0 && json.Unmarshal(schema, &obj) == nil && obj != nil {
		return schema
	}
	return json.RawMessage(`{"type":"object"}`)
}

// closeAll tears down every connected MCP server subprocess. Safe to call
// twice (deferred in main plus explicit on shutdown).
func (b *bridge) closeAll() {
	for _, s := range b.servers {
		if s != nil {
			_ = s.client.Close()
		}
	}
}

// toolCall routes one plugin tools/call to the owning MCP server. Unknown
// tools (truncated, collided, or belonging to a server that never connected)
// and malformed params return error results rather than protocol errors so
// the agent sees a readable tool failure.
func (b *bridge) toolCall(params json.RawMessage) plugin.CallResult {
	var p plugin.CallParams
	if err := json.Unmarshal(params, &p); err != nil {
		return plugin.CallResult{
			Content: fmt.Sprintf("mcp: decode tools/call params: %v", err),
			IsError: true,
		}
	}
	route, ok := b.index[p.Name]
	if !ok {
		return plugin.CallResult{
			Content: fmt.Sprintf("mcp: unknown tool %q (it may have been truncated, name-collided, or its server failed to connect)", p.Name),
			IsError: true,
		}
	}
	ctx, cancel := context.WithTimeout(context.Background(), callTimeout)
	defer cancel()
	return route.server.callTool(ctx, route.tool, p.Arguments)
}

// incomingRequest is one plugin-protocol message read from stdin.
type incomingRequest struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params"`
}

// outgoingResponse is one reply written to stdout. ID echoes the raw request
// id verbatim (number or string) so pigo's pending-call map correlates it.
type outgoingResponse struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Result  any             `json:"result,omitempty"`
	Error   *rpcError       `json:"error,omitempty"`
}

// rpcError is the JSON-RPC error object.
type rpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

func rpcErrf(code int, format string, args ...any) *rpcError {
	return &rpcError{Code: code, Message: fmt.Sprintf(format, args...)}
}

// serve reads plugin-protocol messages from r until a shutdown notification or
// EOF, answering requests and tearing the MCP servers down on the way out.
// Requests are dispatched inline (the plugin protocol in practice is
// sequential: pigo runs plugin tools sequentially), so responses stay ordered
// and no extra synchronization is needed.
func (b *bridge) serve(r io.Reader, w io.Writer) error {
	out := bufio.NewWriter(w)
	writeResponse := func(id json.RawMessage, result any, rpcErr *rpcError) {
		resp := outgoingResponse{JSONRPC: jsonrpc.Version, ID: id, Result: result, Error: rpcErr}
		data, err := json.Marshal(resp)
		if err != nil {
			// Our result types are plain JSON; marshaling cannot realistically
			// fail. Fall back to a minimal error response if it ever does.
			data, _ = json.Marshal(outgoingResponse{
				JSONRPC: jsonrpc.Version, ID: id,
				Error: rpcErrf(codeServerError, "marshal response: %v", err),
			})
		}
		_, _ = out.Write(data)
		_ = out.WriteByte('\n')
		_ = out.Flush()
	}

	sc := bufio.NewScanner(r)
	// Manifests and tool-call payloads can be large; match the 16MB line cap
	// internal/jsonrpc uses on the client side.
	sc.Buffer(make([]byte, 0, 64*1024), 16*1024*1024)
	for sc.Scan() {
		line := bytes.TrimSpace(sc.Bytes())
		if len(line) == 0 {
			continue
		}
		var req incomingRequest
		if err := json.Unmarshal(line, &req); err != nil {
			writeResponse(json.RawMessage("null"), nil, rpcErrf(codeParseError, "parse request: %v", err))
			continue
		}

		if req.Method == "shutdown" {
			// Notification from pigo's Plugin.Close; tear down and exit so the
			// child disappears and pigo's Close does not wait out its grace.
			b.closeAll()
			if len(req.ID) != 0 {
				writeResponse(req.ID, nil, nil)
			}
			return nil
		}
		// A notification with no id needs no reply; the bridge declares no
		// event subscriptions, so the only expected notification is shutdown
		// (handled above) — ignore anything else.
		if len(req.ID) == 0 {
			continue
		}

		var result any
		var rpcErr *rpcError
		switch req.Method {
		case "initialize":
			result = b.manifest
		case "tools/call":
			result = b.toolCall(req.Params)
		default:
			rpcErr = rpcErrf(codeMethodNotFound, "method %q not supported by pigo-mcp-bridge", req.Method)
		}
		writeResponse(req.ID, result, rpcErr)
	}
	if err := sc.Err(); err != nil {
		b.closeAll()
		return fmt.Errorf("read stdin: %w", err)
	}
	// stdin EOF: pigo closed the pipe; shut down cleanly.
	b.closeAll()
	return nil
}
