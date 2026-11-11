// This file implements the MCP-client side of the bridge: launching each
// configured MCP server as a subprocess, performing the MCP initialize
// handshake, listing its tools, and forwarding tool calls. It reuses
// internal/jsonrpc (the same transport pigo itself uses against this bridge)
// as the subprocess JSON-RPC client; the MCP methods it speaks are exactly the
// ones the desktop client pins (docs/mcp-local-integration.md §1.3).
package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"

	"github.com/smallnest/pigo/internal/jsonrpc"
	"github.com/smallnest/pigo/internal/plugin"
)

// mcpProtocolVersion is the MCP protocol version this bridge pins, matching
// the desktop client.
const mcpProtocolVersion = "2024-11-05"

const (
	// connectTimeout bounds one server's launch + initialize + tools/list. It
	// must stay comfortably under pigo's 10s plugin handshake (internal/plugin
	// initTimeout): pigo's initialize response only comes back once connectAll
	// has finished, so a slower server would get the whole bridge skipped.
	connectTimeout = 8 * time.Second
	// callTimeout bounds one forwarded tools/call. Generous because MCP tools
	// can legitimately be slow, but bounded so a hung server cannot block the
	// agent loop forever.
	callTimeout = 120 * time.Second
	// maxListTools caps the paginated tools/list fetch per server so a
	// misbehaving server cannot loop cursors forever.
	maxListTools = 256
	// maxTools caps the bridge-wide tool count exposed to pigo: MCP servers
	// routinely declare dozens of tools, and an unbounded list floods the
	// agent's context (docs §2.2).
	maxTools = 64
)

// rpcClient is the MCP-server-facing transport, abstracted so tests can inject
// an in-process fake instead of a spawned subprocess. *jsonrpc.Client
// satisfies it.
type rpcClient interface {
	Call(ctx context.Context, method string, params any) (json.RawMessage, error)
	Notify(method string, params any) error
	Close() error
}

// mcpServer is one connected MCP server: its client plus the tool list
// discovered at connect time.
type mcpServer struct {
	id     string
	client rpcClient
	tools  []mcpTool
}

// mcpTool is one entry of a tools/list page.
type mcpTool struct {
	Name        string          `json:"name"`
	Description string          `json:"description"`
	InputSchema json.RawMessage `json:"inputSchema"`
}

// connectMCP launches the MCP server subprocess, handshakes, and lists its
// tools. The whole sequence must fit within connectTimeout (see above).
// childStderr receives the server's stderr so its logs flow into the bridge's
// stderr (which pigo captures) instead of being lost.
func connectMCP(cfg ServerConfig, childStderr io.Writer, logf func(string, ...any)) (*mcpServer, error) {
	command, args := resolveCommand(cfg.Command, cfg.Args)
	client, err := jsonrpc.NewClient(jsonrpc.Config{
		Command: command,
		Args:    args,
		Env:     buildEnv(cfg.Env),
		Stderr:  childStderr,
	})
	if err != nil {
		return nil, fmt.Errorf("launch %q: %w", cfg.Command, err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), connectTimeout)
	defer cancel()

	s := &mcpServer{id: cfg.ID, client: client}
	if err := s.handshake(ctx, logf); err != nil {
		_ = client.Close()
		return nil, err
	}
	if err := s.listTools(ctx); err != nil {
		_ = client.Close()
		return nil, err
	}
	return s, nil
}

// handshake performs the MCP initialize round trip and the
// notifications/initialized follow-up. A protocol-version mismatch is logged
// but not fatal — the desktop client likewise pins one version and proceeds.
func (s *mcpServer) handshake(ctx context.Context, logf func(string, ...any)) error {
	raw, err := s.client.Call(ctx, "initialize", map[string]any{
		"protocolVersion": mcpProtocolVersion,
		"capabilities":    map[string]any{},
		"clientInfo":      map[string]any{"name": "pigo-mcp-bridge", "version": bridgeVersion},
	})
	if err != nil {
		return fmt.Errorf("initialize: %w", err)
	}
	var info struct {
		ProtocolVersion string `json:"protocolVersion"`
	}
	if json.Unmarshal(raw, &info) == nil && info.ProtocolVersion != "" && info.ProtocolVersion != mcpProtocolVersion {
		logf("server %q speaks MCP protocol %q, bridge speaks %q — proceeding anyway", s.id, info.ProtocolVersion, mcpProtocolVersion)
	}
	return s.client.Notify("notifications/initialized", nil)
}

// listTools fetches the server's full tool list, following cursor pagination
// until exhausted (or the fetch cap is hit).
func (s *mcpServer) listTools(ctx context.Context) error {
	cursor := ""
	for {
		var params any
		if cursor != "" {
			params = map[string]string{"cursor": cursor}
		}
		raw, err := s.client.Call(ctx, "tools/list", params)
		if err != nil {
			return fmt.Errorf("tools/list: %w", err)
		}
		var page struct {
			Tools      []mcpTool `json:"tools"`
			NextCursor string    `json:"nextCursor"`
		}
		if err := json.Unmarshal(raw, &page); err != nil {
			return fmt.Errorf("decode tools/list: %w", err)
		}
		s.tools = append(s.tools, page.Tools...)
		if len(s.tools) >= maxListTools {
			s.tools = s.tools[:maxListTools]
			break
		}
		if page.NextCursor == "" {
			break
		}
		cursor = page.NextCursor
	}
	return nil
}

// mcpCallParams is the tools/call request the bridge sends to the server.
// Arguments are the agent-provided JSON object, forwarded verbatim.
type mcpCallParams struct {
	Name      string          `json:"name"`
	Arguments json.RawMessage `json:"arguments,omitempty"`
}

// mcpCallResult is the tools/call reply. Exactly the surface the bridge maps
// into the plugin protocol's flat CallResult.
type mcpCallResult struct {
	Content    []mcpContent    `json:"content"`
	IsError    bool            `json:"isError"`
	Structured json.RawMessage `json:"structuredContent"`
}

// mcpContent is one content block. MCP content blocks are a tagged union;
// only the fields the bridge renders are decoded.
type mcpContent struct {
	Type     string               `json:"type"` // "text" | "image" | "audio" | "resource_link" | "resource" | ...
	Text     string               `json:"text"`
	Data     string               `json:"data"` // image/audio: base64 payload
	MIMEType string               `json:"mimeType"`
	URI      string               `json:"uri"` // resource_link
	Resource *mcpEmbeddedResource `json:"resource"`
}

// mcpEmbeddedResource is the resource block embedded by a "resource" content.
type mcpEmbeddedResource struct {
	URI      string `json:"uri"`
	MIMEType string `json:"mimeType"`
	Text     string `json:"text"`
}

// callTool forwards one tool invocation to the server and maps the reply into
// the plugin protocol's flat CallResult. Every failure mode (transport error,
// undecodable reply) degrades to an error result — never a panic, never a
// hang beyond callTimeout.
func (s *mcpServer) callTool(ctx context.Context, tool string, args json.RawMessage) plugin.CallResult {
	params := mcpCallParams{Name: tool}
	if trimmed := bytes.TrimSpace(args); len(trimmed) > 0 && !bytes.Equal(trimmed, []byte("null")) {
		params.Arguments = trimmed
	}
	raw, err := s.client.Call(ctx, "tools/call", params)
	if err != nil {
		return plugin.CallResult{
			Content: fmt.Sprintf("mcp %s/%s: call failed: %v", s.id, tool, err),
			IsError: true,
		}
	}
	var res mcpCallResult
	if err := json.Unmarshal(raw, &res); err != nil {
		return plugin.CallResult{
			Content: fmt.Sprintf("mcp %s/%s: decode result: %v", s.id, tool, err),
			IsError: true,
		}
	}
	return plugin.CallResult{Content: renderMCPContent(res), IsError: res.IsError}
}

// renderMCPContent flattens the MCP content-block list into the single text
// string the plugin protocol can carry. Text blocks are concatenated with
// newlines; binary blocks (image/audio) degrade to a text placeholder because
// the plugin protocol has no binary channel; a structuredContent-only reply is
// passed through as raw JSON.
func renderMCPContent(res mcpCallResult) string {
	parts := make([]string, 0, len(res.Content))
	for _, c := range res.Content {
		switch c.Type {
		case "text":
			parts = append(parts, c.Text)
		case "image", "audio":
			// ~ decoded size: base64 is 4 chars per 3 bytes.
			parts = append(parts, fmt.Sprintf("[%s: %s, ~%d bytes]", c.Type, c.MIMEType, len(c.Data)*3/4))
		case "resource_link":
			parts = append(parts, fmt.Sprintf("[resource link: %s]", c.URI))
		case "resource":
			if c.Resource != nil && c.Resource.Text != "" {
				parts = append(parts, fmt.Sprintf("[resource %s]\n%s", c.Resource.URI, c.Resource.Text))
			} else if c.Resource != nil {
				parts = append(parts, fmt.Sprintf("[resource: %s (%s)]", c.Resource.URI, c.Resource.MIMEType))
			}
		default:
			parts = append(parts, fmt.Sprintf("[unsupported content type: %s]", c.Type))
		}
	}
	s := strings.Join(parts, "\n")
	if s == "" && len(res.Structured) > 0 {
		s = string(res.Structured)
	}
	return s
}

// resolveCommand adapts Windows launcher quirks (docs §2.4 item 3, the most
// common local-MCP failure point on Windows): os/exec does not resolve
// PATHEXT, so a bare "npx" would fail to launch, and CreateProcess cannot exec
// .cmd/.bat batch files directly. When the configured command has no
// extension, look it up on PATH; if it resolves to (or already is) a batch
// file, route it through "cmd /c <path>". Everything else — non-Windows, real
// executables, a user-written "cmd /c ..." — passes through unchanged.
func resolveCommand(command string, args []string) (string, []string) {
	if runtime.GOOS != "windows" {
		return command, args
	}
	ext := strings.ToLower(filepath.Ext(command))
	path := command
	if ext == "" {
		resolved, err := exec.LookPath(command)
		if err != nil || resolved == "" {
			return command, args // let jsonrpc surface the launch error
		}
		path = resolved
		ext = strings.ToLower(filepath.Ext(path))
	}
	if ext == ".cmd" || ext == ".bat" {
		return "cmd", append([]string{"/c", path}, args...)
	}
	return path, args
}

// safeToken maps a server id or tool name onto the [A-Za-z0-9_] alphabet used
// for plugin tool names, mirroring the desktop client's namespace rule
// (mcp.rs safe_token): every other rune becomes '_'.
func safeToken(s string) string {
	var b strings.Builder
	b.Grow(len(s))
	for _, r := range s {
		switch {
		case r >= 'a' && r <= 'z', r >= 'A' && r <= 'Z', r >= '0' && r <= '9', r == '_':
			b.WriteRune(r)
		default:
			b.WriteByte('_')
		}
	}
	if b.Len() == 0 {
		return "_"
	}
	return b.String()
}
