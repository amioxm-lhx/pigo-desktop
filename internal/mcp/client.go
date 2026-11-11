// This file implements the MCP protocol client: launching a stdio server,
// performing the initialize handshake, listing tools (cursor-paginated), and
// forwarding tool calls. The rpc transport is abstracted behind rpcClient so
// tests can inject an in-process fake; *jsonrpc.Client satisfies it in
// production. resources/list + resources/read are implemented as the minimal
// US-015 acceptance surface.
package mcp

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
)

// ProtocolVersion is the MCP protocol version pigo pins, matching the desktop
// client (mcp.rs:26). A server that replies with a different version is logged
// and used anyway (the desktop client does the same).
const ProtocolVersion = "2024-11-05"

const (
	// initTimeout bounds one server's launch + initialize + tools/list.
	initTimeout = 10 * time.Second
	// callTimeout bounds one tools/call. Generous because MCP tools can
	// legitimately be slow, but bounded so a hung server cannot block the agent
	// loop forever.
	callTimeout = 60 * time.Second
	// maxListPages caps paginated list fetches (tools/list, resources/list) so a
	// misbehaving server cannot loop cursors forever.
	maxListPages = 8
	// MaxToolsPerServer caps the tools exposed per server; the rest is truncated
	// with a warning (desktop MAX_TOOLS_PER_SERVER).
	MaxToolsPerServer = 64
)

// clientName identifies pigo in the initialize handshake.
const clientName = "pigo"

// rpcClient is the server-facing transport, abstracted for tests.
type rpcClient interface {
	Call(ctx context.Context, method string, params any) (json.RawMessage, error)
	Notify(method string, params any) error
	Close() error
}

// ToolDecl is one entry of a tools/list page.
type ToolDecl struct {
	Name        string          `json:"name"`
	Description string          `json:"description"`
	InputSchema json.RawMessage `json:"inputSchema"`
}

// ContentBlock is one MCP content block. MCP content blocks are a tagged
// union; only the fields pigo renders are decoded.
type ContentBlock struct {
	Type     string            `json:"type"` // "text" | "image" | "audio" | "resource_link" | "resource" | ...
	Text     string            `json:"text"`
	Data     string            `json:"data"` // image/audio: base64 payload
	MIMEType string            `json:"mimeType"`
	URI      string            `json:"uri"` // resource_link
	Resource *EmbeddedResource `json:"resource"`
}

// EmbeddedResource is the resource carried by a "resource" content block.
type EmbeddedResource struct {
	URI      string `json:"uri"`
	MIMEType string `json:"mimeType"`
	Text     string `json:"text"`
}

// CallResult is the tools/call reply.
type CallResult struct {
	Content    []ContentBlock  `json:"content"`
	IsError    bool            `json:"isError"`
	Structured json.RawMessage `json:"structuredContent"`
}

// Resource is one entry of a resources/list page.
type Resource struct {
	URI         string `json:"uri"`
	Name        string `json:"name"`
	Description string `json:"description"`
	MIMEType    string `json:"mimeType"`
}

// ResourceContents is the reply of resources/read: the resource's contents as
// content blocks (text or blob, base64).
type ResourceContents struct {
	Contents []ContentBlock `json:"contents"`
}

// Client is one connected MCP server: its rpc client plus the tool list
// discovered at connect time.
type Client struct {
	id     string
	rpc    rpcClient
	tools  []ToolDecl
	truncs []string // human-readable truncation notices, surfaced by the Manager
}

// Dial connects one MCP server: stdio servers launch a subprocess and speak
// NDJSON over its pipes; http servers speak Streamable HTTP (transport_http.go).
// Both then perform the initialize handshake and list tools. The whole
// sequence must fit within initTimeout. stderr, when non-nil, receives a stdio
// server's stderr so its logs flow into pigo's instead of being lost.
func Dial(cfg Server, stderr io.Writer, logf func(string, ...any)) (*Client, error) {
	var rpc rpcClient
	if cfg.Stdio() {
		command, args := resolveCommand(cfg.Command, cfg.Args)
		jrpc, err := jsonrpc.NewClient(jsonrpc.Config{
			Command: command,
			Args:    args,
			Env:     BuildEnv(cfg.Env),
			Stderr:  stderr,
		})
		if err != nil {
			return nil, fmt.Errorf("mcp: launch %q: %w", cfg.Command, err)
		}
		rpc = jrpc
	} else {
		hrpc, err := dialHTTP(cfg)
		if err != nil {
			return nil, err
		}
		rpc = hrpc
	}
	c, err := newClient(cfg.ID, rpc, logf)
	if err != nil {
		_ = rpc.Close()
		return nil, err
	}
	return c, nil
}

// newClient handshakes and lists tools over an established rpc transport. The
// test seam: callers inject a fake rpcClient instead of a subprocess.
func newClient(id string, rpc rpcClient, logf func(string, ...any)) (*Client, error) {
	c := &Client{id: id, rpc: rpc}
	ctx, cancel := context.WithTimeout(context.Background(), initTimeout)
	defer cancel()

	raw, err := rpc.Call(ctx, "initialize", map[string]any{
		"protocolVersion": ProtocolVersion,
		"capabilities":    map[string]any{},
		"clientInfo":      map[string]any{"name": clientName, "version": "1"},
	})
	if err != nil {
		return nil, fmt.Errorf("mcp %q: initialize: %w", id, err)
	}
	var info struct {
		ProtocolVersion string `json:"protocolVersion"`
	}
	if json.Unmarshal(raw, &info) == nil && info.ProtocolVersion != "" && info.ProtocolVersion != ProtocolVersion && logf != nil {
		logf("pigo: mcp %q speaks protocol %q, pigo speaks %q — proceeding anyway", id, info.ProtocolVersion, ProtocolVersion)
	}
	if err := rpc.Notify("notifications/initialized", nil); err != nil {
		return nil, fmt.Errorf("mcp %q: initialized notification: %w", id, err)
	}
	if err := c.listTools(ctx); err != nil {
		return nil, err
	}
	return c, nil
}

// ID returns the server id this client connected to.
func (c *Client) ID() string { return c.id }

// Tools returns the tool list discovered at connect time, already capped at
// MaxToolsPerServer.
func (c *Client) Tools() []ToolDecl { return c.tools }

// Notices returns truncation/limit warnings gathered during connection, for
// the caller to log once.
func (c *Client) Notices() []string { return c.truncs }

// listTools fetches the server's full tool list, following cursor pagination
// until exhausted, the page cap is hit, or the per-server tool cap is reached.
func (c *Client) listTools(ctx context.Context) error {
	cursor := ""
	for page := 0; page < maxListPages; page++ {
		var params any
		if cursor != "" {
			params = map[string]string{"cursor": cursor}
		}
		raw, err := c.rpc.Call(ctx, "tools/list", params)
		if err != nil {
			return fmt.Errorf("mcp %q: tools/list: %w", c.id, err)
		}
		var pg struct {
			Tools      []ToolDecl `json:"tools"`
			NextCursor string     `json:"nextCursor"`
		}
		if err := json.Unmarshal(raw, &pg); err != nil {
			return fmt.Errorf("mcp %q: decode tools/list: %w", c.id, err)
		}
		c.tools = append(c.tools, pg.Tools...)
		if len(c.tools) >= MaxToolsPerServer {
			c.tools = c.tools[:MaxToolsPerServer]
			c.truncs = append(c.truncs, fmt.Sprintf("mcp %q: tool list truncated at %d tools", c.id, MaxToolsPerServer))
			return nil
		}
		if pg.NextCursor == "" {
			return nil
		}
		cursor = pg.NextCursor
	}
	c.truncs = append(c.truncs, fmt.Sprintf("mcp %q: tools/list pagination stopped after %d pages (server kept sending cursors)", c.id, maxListPages))
	return nil
}

// callParams is the tools/call request. Arguments are the agent-provided JSON
// object, forwarded verbatim.
type callParams struct {
	Name      string          `json:"name"`
	Arguments json.RawMessage `json:"arguments,omitempty"`
}

// CallTool forwards one tool invocation to the server. A nil-deadline context
// gets the default callTimeout. Transport errors surface as a returned error —
// the Tool adapter degrades them to an error result so a dead server cannot
// take down the agent loop.
func (c *Client) CallTool(ctx context.Context, name string, args json.RawMessage) (CallResult, error) {
	if _, ok := ctx.Deadline(); !ok {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, callTimeout)
		defer cancel()
	}
	params := callParams{Name: name}
	if trimmed := bytes.TrimSpace(args); len(trimmed) > 0 && !bytes.Equal(trimmed, []byte("null")) {
		params.Arguments = trimmed
	}
	raw, err := c.rpc.Call(ctx, "tools/call", params)
	if err != nil {
		return CallResult{}, err
	}
	var res CallResult
	if err := json.Unmarshal(raw, &res); err != nil {
		return CallResult{}, fmt.Errorf("mcp %q: decode result for %q: %w", c.id, name, err)
	}
	return res, nil
}

// ListResources performs resources/list, following cursor pagination up to the
// page cap. Servers without the resources capability return an error, which
// the caller treats as "no resources".
func (c *Client) ListResources(ctx context.Context) ([]Resource, error) {
	var out []Resource
	cursor := ""
	for page := 0; page < maxListPages; page++ {
		var params any
		if cursor != "" {
			params = map[string]string{"cursor": cursor}
		}
		raw, err := c.rpc.Call(ctx, "resources/list", params)
		if err != nil {
			return nil, err
		}
		var pg struct {
			Resources  []Resource `json:"resources"`
			NextCursor string     `json:"nextCursor"`
		}
		if err := json.Unmarshal(raw, &pg); err != nil {
			return nil, fmt.Errorf("mcp %q: decode resources/list: %w", c.id, err)
		}
		out = append(out, pg.Resources...)
		if pg.NextCursor == "" {
			return out, nil
		}
		cursor = pg.NextCursor
	}
	return out, fmt.Errorf("mcp %q: resources/list pagination did not settle after %d pages", c.id, maxListPages)
}

// ReadResource performs resources/read for one URI and returns the resource's
// content blocks.
func (c *Client) ReadResource(ctx context.Context, uri string) ([]ContentBlock, error) {
	raw, err := c.rpc.Call(ctx, "resources/read", map[string]string{"uri": uri})
	if err != nil {
		return nil, err
	}
	var rc ResourceContents
	if err := json.Unmarshal(raw, &rc); err != nil {
		return nil, fmt.Errorf("mcp %q: decode resources/read: %w", c.id, err)
	}
	return rc.Contents, nil
}

// Close shuts the server down: closes stdin (the MCP graceful-shutdown signal)
// and, after the transport's grace period, kills the process. Idempotent.
func (c *Client) Close() error { return c.rpc.Close() }

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
