// This file adapts one MCP tool declaration to the agentcore.AgentTool
// interface (docs/mcp-local-integration.md §3.5). Mirrors pluginTool
// (internal/plugin/plugin.go:150-196): cross-process, unknown-side-effect,
// sequential execution, and crash isolation — a transport failure degrades to
// an error result, never a Go error, never a panic.
package mcp

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"

	"github.com/smallnest/pigo/internal/agentcore"
)

// Tool is one MCP server tool as the agent sees it.
type Tool struct {
	client  *Client
	server  string
	decl    ToolDecl
	schema  json.RawMessage // sanitized at construction
	namespaced string
}

// newTool sanitizes decl.InputSchema and namespaces the name. A sanitize error
// means the caller drops the tool entirely (never registers it).
func newTool(client *Client, server string, decl ToolDecl, sanitized json.RawMessage) *Tool {
	return &Tool{
		client:     client,
		server:     server,
		decl:       decl,
		schema:     sanitized,
		namespaced: Namespace(server, decl.Name),
	}
}

// Name implements AgentTool: mcp_<safe(server)>_<safe(tool)>, the frozen
// namespace shared with the desktop client.
func (t *Tool) Name() string { return t.namespaced }

// Description implements AgentTool.
func (t *Tool) Description() string { return t.decl.Description }

// Schema implements AgentTool, returning the sanitized schema.
func (t *Tool) Schema() json.RawMessage { return t.schema }

// ExecutionMode implements AgentTool. MCP calls cross a process boundary with
// unknown side effects, so they run sequentially (mirrors plugin.go:172-176).
func (t *Tool) ExecutionMode() agentcore.ToolExecutionMode {
	return agentcore.ToolExecutionSequential
}

// HasSideEffects implements agentcore.SideEffectDeclarer. MCP tools run in a
// separate process with arbitrary effects, so they are trust-gated in
// untrusted directories even though their names are not in the built-in
// SideEffectTools table (docs §3.2 item 5 / §3.7.3).
func (t *Tool) HasSideEffects() bool { return true }

// Execute implements AgentTool by forwarding the call to the server and
// mapping the MCP content blocks into agentcore contents (§3.5):
//
//	text        → TextContent, one per block, in order
//	image       → ImageContent (models that cannot consume images degrade it
//	              downstream, the same way provider layers already handle
//	              built-in image content)
//	audio / resource_link / unknown → text placeholder
//	resource    → text folded in, blob degraded to a placeholder
//
// structuredContent rides in Details; isError is recorded in Details as well
// (never a Go error — an MCP tool error is a result the model can read).
func (t *Tool) Execute(ctx context.Context, id string, args json.RawMessage, onUpdate agentcore.ToolUpdateFunc) (agentcore.AgentToolResult, error) {
	res, err := t.client.CallTool(ctx, t.decl.Name, args)
	if err != nil {
		return agentcore.AgentToolResult{
			Content: agentcore.ContentList{agentcore.NewTextContent(
				fmt.Sprintf("%s: mcp %s/%s call failed: %v", t.namespaced, t.server, t.decl.Name, err))},
		}, nil
	}
	var content agentcore.ContentList
	for _, block := range res.Content {
		content = append(content, mapBlock(block)...)
	}
	if len(content) == 0 && len(res.Structured) > 0 {
		content = append(content, agentcore.NewTextContent(string(res.Structured)))
	}
	var details map[string]any
	if res.IsError || len(res.Structured) > 0 {
		details = map[string]any{}
		if res.IsError {
			details["isError"] = true
		}
		if len(res.Structured) > 0 {
			details["structuredContent"] = json.RawMessage(res.Structured)
		}
	}
	return agentcore.AgentToolResult{Content: content, Details: details}, nil
}

// mapBlock converts one MCP content block to agentcore content.
func mapBlock(block ContentBlock) agentcore.ContentList {
	switch block.Type {
	case "text":
		return agentcore.ContentList{agentcore.NewTextContent(block.Text)}
	case "image":
		return agentcore.ContentList{agentcore.NewImageContent(block.Data, block.MIMEType)}
	case "audio":
		// agentcore has no audio content; degrade to a placeholder.
		return agentcore.ContentList{agentcore.NewTextContent(
			fmt.Sprintf("[audio: %s, ~%d bytes]", block.MIMEType, len(block.Data)*3/4))}
	case "resource_link":
		return agentcore.ContentList{agentcore.NewTextContent(
			fmt.Sprintf("[resource link: %s]", block.URI))}
	case "resource":
		if block.Resource == nil {
			return nil
		}
		if block.Resource.Text != "" {
			return agentcore.ContentList{agentcore.NewTextContent(
				fmt.Sprintf("[resource %s]\n%s", block.Resource.URI, block.Resource.Text))}
		}
		return agentcore.ContentList{agentcore.NewTextContent(
			fmt.Sprintf("[resource: %s (%s)]", block.Resource.URI, block.Resource.MIMEType))}
	default:
		return agentcore.ContentList{agentcore.NewTextContent(
			fmt.Sprintf("[unsupported content type: %s]", block.Type))}
	}
}

// Namespace maps a server id and tool name onto the agent-visible tool name:
// mcp_<safe(server)>_<safe(tool)>, mirroring the desktop client's namespace
// rule (mcp.rs:819-837). safe() keeps [A-Za-z0-9_] and replaces every other
// rune with '_', and an EMPTY component becomes "unnamed" — the native
// spelling per docs §3.3 (the bridge spells it "_"; the two never share tool
// names in one registry because the bridge is a stopgap being replaced).
func Namespace(serverID, toolName string) string {
	return "mcp_" + safeToken(serverID) + "_" + safeToken(toolName)
}

// safeToken maps one namespace component onto [A-Za-z0-9_]; empty → "unnamed".
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
		return "unnamed"
	}
	return b.String()
}
