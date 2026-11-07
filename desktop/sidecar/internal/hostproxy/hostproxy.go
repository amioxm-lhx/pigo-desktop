// Package hostproxy sends the reverse RPC calls the sidecar makes back to
// Electron main. Every host capability (tool catalog, tool execution, provider
// auth) is one host.proxy request; Electron main validates it against its
// allowlist and forwards it to host-core.
package hostproxy

import (
	"context"
	"encoding/json"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/jsonrpc"
	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
)

// Allowed is the host method allowlist. The Tauri shell accepts exactly these
// host.proxy targets; keeping the local list in sync turns an unsupported
// method into an immediate error instead of a rejected round trip.
var Allowed = map[string]bool{
	"provider.resolveAuth":     true,
	"tools.execute":            true,
	"tools.abort":              true,
	"tools.list":               true,
	"session.get":              true,
	"session.list":             true,
	"session.appendMessage":    true,
	"session.appendCompaction": true,
	"session.replaceMessages":  true,
	"workspace.get":            true,
	"plans.enter":              true,
	"plans.submit":             true,
	"plans.pending":            true,
	"plans.abort":              true,
	"app.health":               true,
	"session.rename":           true,
	"session.create":           true,
	"session.fork":             true,
}

const (
	// DefaultRPCTimeout is the deadline for ordinary host calls.
	DefaultRPCTimeout = 130 * time.Second
	// BashRPCTimeout covers the permission wait plus the command itself.
	BashRPCTimeout = 190 * time.Second
)

type proxyParams struct {
	Method string `json:"method"`
	Params any    `json:"params"`
}

// Client proxies host calls over the sidecar connection.
type Client struct {
	conn *jsonrpc.Conn
}

// New creates a client over an established sidecar connection.
func New(conn *jsonrpc.Conn) *Client {
	return &Client{conn: conn}
}

// Call sends one host request and returns its raw result.
func (c *Client) Call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	if !Allowed[method] {
		return nil, jsonrpc.Errorf(jsonrpc.CodeMethodNotFound, "host method not allowed from sidecar: %s", method)
	}

	deadline := DefaultRPCTimeout
	if method == "tools.execute" && isBash(params) {
		deadline = BashRPCTimeout
	}
	callCtx, cancel := context.WithTimeout(ctx, deadline)
	defer cancel()

	return c.conn.Call(callCtx, protocol.MethodHostProxy, proxyParams{Method: method, Params: params})
}

// CallInto sends one host request and decodes its result into out.
func (c *Client) CallInto(ctx context.Context, method string, params any, out any) error {
	raw, err := c.Call(ctx, method, params)
	if err != nil {
		return err
	}
	if out == nil {
		return nil
	}
	if len(raw) == 0 || string(raw) == "null" {
		return nil
	}
	return json.Unmarshal(raw, out)
}

func isBash(params any) bool {
	switch input := params.(type) {
	case protocol.ToolExecuteParams:
		return input.ToolName == "Bash"
	case *protocol.ToolExecuteParams:
		return input != nil && input.ToolName == "Bash"
	case map[string]any:
		name, _ := input["toolName"].(string)
		return name == "Bash"
	default:
		return false
	}
}
