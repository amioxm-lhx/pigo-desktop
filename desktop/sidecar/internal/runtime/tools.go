package runtime

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
)

// catalogResponse is the tools.list reply.
type catalogResponse struct {
	Tools []protocol.ToolDefinition `json:"tools"`
}

// hostToolNamesByMode is the host tool set each mode exposes. The catalog
// itself is owned by host-core; this table keeps the mode boundary explicit.
var hostToolNamesByMode = map[string]map[string]bool{
	"agent": {"Read": true, "Write": true, "Edit": true, "Bash": true, "Glob": true, "Grep": true},
	"plan":  {"Read": true, "Glob": true, "Grep": true, "Bash": true},
	"goal":  {"Read": true, "Glob": true, "Grep": true, "Bash": true},
}

var toolParamAliases = map[string]map[string]string{
	"Read":  {"file_path": "path"},
	"Write": {"file_path": "path"},
	"Edit":  {"file_path": "path"},
	"Glob":  {"query": "pattern"},
	"Grep":  {"query": "pattern"},
}

func fetchCatalog(ctx context.Context, host Host) ([]protocol.ToolDefinition, error) {
	if host == nil {
		return nil, nil
	}
	var reply catalogResponse
	if err := host.CallInto(ctx, "tools.list", map[string]any{}, &reply); err != nil {
		return nil, err
	}
	return reply.Tools, nil
}

func buildHostTools(ctx context.Context, session *Session) ([]agentcore.AgentTool, []protocol.ToolDefinition, error) {
	catalog, err := fetchCatalog(ctx, session.host)
	if err != nil {
		return nil, nil, fmt.Errorf("tools.list: %w", err)
	}
	allowed := hostToolNamesByMode[session.mode]
	if allowed == nil {
		allowed = hostToolNamesByMode["agent"]
	}

	tools := make([]agentcore.AgentTool, 0, len(catalog))
	for _, definition := range catalog {
		name := strings.TrimSpace(definition.Name)
		if name == "" || !allowed[name] {
			continue
		}
		tools = append(tools, &hostTool{
			session:    session,
			definition: definition,
			sequential: name == "Bash" || name == "Write" || name == "Edit",
		})
	}
	tools = append(tools, buildPluginTools(session)...)
	return tools, catalog, nil
}

// buildPluginTools wraps the prompt's plugin contributions in the same
// hostTool proxy used for built-in tools: execution stays in host-core's
// permission flow, which dispatches `plugin_*` calls to the desktop runner.
//
// Contract modes expose only tools that declare plan-safe actions. Host-core
// admits exactly those (ADR 0211) and the desktop runtime then gates the
// per-action list; a plugin tool without the declaration stays hidden.
func buildPluginTools(session *Session) []agentcore.AgentTool {
	contractMode := session.mode == "plan" || session.mode == "goal"
	tools := make([]agentcore.AgentTool, 0, len(session.params.PluginTools))
	for _, pluginTool := range session.params.PluginTools {
		name := strings.TrimSpace(pluginTool.Name)
		// Plugin tools and user-owned MCP tools share the desktop-dispatched
		// namespaces; host-core gates both and answers both through tools.execute.
		if name == "" || (!strings.HasPrefix(name, "plugin_") && !strings.HasPrefix(name, "mcp_")) {
			continue
		}
		if contractMode && len(pluginTool.PlanSafeActions) == 0 {
			continue
		}
		parameters := pluginTool.Parameters
		if !json.Valid(parameters) || len(bytes.TrimSpace(parameters)) == 0 {
			parameters = json.RawMessage(`{"type":"object","properties":{}}`)
		}
		tools = append(tools, &hostTool{
			session: session,
			definition: protocol.ToolDefinition{
				Name:        name,
				Description: pluginTool.Description,
				Risk:        pluginTool.Risk,
				Parameters:  parameters,
			},
			declaredRisk:    pluginTool.Risk,
			planSafeActions: pluginTool.PlanSafeActions,
		})
	}
	return tools
}

// hostToolByName builds a child delegate's host tool. It is independent of the
// parent mode's allowed set: a definition that declares Write may use it even
// when the parent session is in a contract mode, while host-core still applies
// the session mode and the delegate's permission scope to the call.
func hostToolByName(session *Session, catalog []protocol.ToolDefinition, name, permissionScope string) *hostTool {
	for _, definition := range catalog {
		if strings.TrimSpace(definition.Name) != name {
			continue
		}
		return &hostTool{
			session:         session,
			definition:      definition,
			sequential:      name == "Bash" || name == "Write" || name == "Edit",
			permissionScope: permissionScope,
		}
	}
	// Plugin tools live in the parent's registered set rather than the host
	// catalog. `tools: inherit` may select them; the clone keeps the plugin's
	// declared risk and plan-safe actions while the delegate's permission
	// scope replaces the parent's.
	if strings.HasPrefix(name, "plugin_") || strings.HasPrefix(name, "mcp_") {
		if parent, ok := session.liveTools[name].(*hostTool); ok {
			clone := *parent
			clone.session = session
			clone.permissionScope = permissionScope
			clone.errors = nil
			return &clone
		}
	}
	return nil
}

// hostTool wraps one host-core tool. Every call goes back through
// host.proxy tools.execute; the sidecar never executes a host tool itself.
type hostTool struct {
	session    *Session
	definition protocol.ToolDefinition
	sequential bool
	// declaredRisk/planSafeActions travel with `tools.execute` so host-core
	// applies the plugin's own risk declaration instead of defaulting to
	// medium, and so plan-safe actions are honoured when forwarded.
	declaredRisk    string
	planSafeActions []string
	permissionScope string
	// errors, when set, is a delegate's own failure set instead of the parent
	// session's per-turn bookkeeping.
	errors *subagentToolErrors
}

func (t *hostTool) Name() string        { return strings.TrimSpace(t.definition.Name) }
func (t *hostTool) Description() string { return t.definition.Description }
func (t *hostTool) Schema() json.RawMessage {
	return append(json.RawMessage(nil), t.definition.Parameters...)
}

func (t *hostTool) ExecutionMode() agentcore.ToolExecutionMode {
	if t.sequential {
		return agentcore.ToolExecutionSequential
	}
	return agentcore.ToolExecutionParallel
}

func (t *hostTool) Execute(ctx context.Context, id string, args json.RawMessage, _ agentcore.ToolUpdateFunc) (agentcore.AgentToolResult, error) {
	if t.session == nil || t.session.host == nil {
		return agentcore.AgentToolResult{}, errors.New("host channel is unavailable")
	}

	normalized := normalizeToolArguments(t.Name(), args)
	request := protocol.ToolExecuteParams{
		SessionID:  t.session.sessionID,
		TurnID:     t.session.CurrentTurnID(),
		ToolCallID: id,
		ToolName:   t.Name(),
		Args:       normalized,
		Mode:       t.session.Mode(),
	}
	if t.permissionScope != "" && t.permissionScope != "inherit" {
		request.PermissionScope = t.permissionScope
	}
	if t.declaredRisk != "" {
		request.DeclaredRisk = t.declaredRisk
	}
	if len(t.planSafeActions) > 0 {
		request.PlanSafeActions = append([]string(nil), t.planSafeActions...)
	}
	if t.Name() == "Bash" {
		request.ExpectedCommandShellID = t.session.shell.ID
		request.ExpectedCommandShellDialect = t.session.shell.Dialect
		if timeoutMs, ok := bashTimeoutMs(normalized); ok {
			request.TimeoutMs = &timeoutMs
		}
	}

	var reply protocol.ToolExecuteResult
	if err := t.session.host.CallInto(ctx, "tools.execute", request, &reply); err != nil {
		return agentcore.AgentToolResult{}, err
	}

	content, details, isError := toolResultContent(reply, t.session.model.SupportsImages)
	if isError {
		if t.errors != nil {
			t.errors.mark(id)
		} else {
			t.session.markToolError(id)
		}
	}
	return agentcore.AgentToolResult{Content: content, Details: details}, nil
}

func normalizeToolArguments(toolName string, args json.RawMessage) json.RawMessage {
	aliases := toolParamAliases[toolName]
	if len(aliases) == 0 || len(args) == 0 {
		return args
	}
	var record map[string]any
	if err := json.Unmarshal(args, &record); err != nil || record == nil {
		return args
	}
	present := false
	for alias := range aliases {
		if _, ok := record[alias]; ok {
			present = true
			break
		}
	}
	if !present {
		return args
	}
	next := make(map[string]any, len(record))
	for name, value := range record {
		if _, isAlias := aliases[name]; isAlias {
			continue
		}
		next[name] = value
	}
	for alias, canonical := range aliases {
		value, ok := record[alias]
		if !ok {
			continue
		}
		if _, taken := next[canonical]; !taken {
			next[canonical] = value
		}
	}
	encoded, err := json.Marshal(next)
	if err != nil {
		return args
	}
	return encoded
}

func bashTimeoutMs(args json.RawMessage) (int64, bool) {
	var record map[string]any
	if err := json.Unmarshal(args, &record); err != nil {
		return 0, false
	}
	switch typed := record["timeoutMs"].(type) {
	case float64:
		if typed > 0 {
			return int64(typed), true
		}
	case int64:
		if typed > 0 {
			return typed, true
		}
	case int:
		if typed > 0 {
			return int64(typed), true
		}
	}
	return 0, false
}

func toolResultContent(reply protocol.ToolExecuteResult, supportsImages bool) (agentcore.ContentList, any, bool) {
	isError := reply.IsError || !reply.OK || reply.Denied
	if len(reply.Content) == 0 {
		return agentcore.ContentList{agentcore.NewTextContent("")}, nil, isError
	}

	var text string
	if err := json.Unmarshal(reply.Content, &text); err == nil {
		return agentcore.ContentList{agentcore.NewTextContent(text)}, text, isError
	}

	var object map[string]any
	if err := json.Unmarshal(reply.Content, &object); err == nil && object != nil {
		if images, ok := object["images"].([]any); ok {
			body, _ := object["text"].(string)
			if body == "" {
				body = prettyJSON(withoutKey(object, "images"))
			}
			content := agentcore.ContentList{agentcore.NewTextContent(body)}
			count := 0
			if supportsImages {
				for _, raw := range images {
					image, ok := raw.(map[string]any)
					if !ok {
						continue
					}
					data, _ := image["data"].(string)
					mimeType, _ := image["mimeType"].(string)
					if data == "" || mimeType == "" {
						continue
					}
					content = append(content, agentcore.NewImageContent(data, mimeType))
					count++
				}
			}
			details := withoutKey(object, "images")
			details["imageCount"] = count
			return content, details, isError
		}
		return agentcore.ContentList{agentcore.NewTextContent(prettyJSON(object))}, object, isError
	}

	var value any
	if err := json.Unmarshal(reply.Content, &value); err == nil {
		return agentcore.ContentList{agentcore.NewTextContent(prettyJSON(value))}, value, isError
	}
	return agentcore.ContentList{agentcore.NewTextContent(string(reply.Content))}, string(reply.Content), isError
}

func withoutKey(source map[string]any, key string) map[string]any {
	out := make(map[string]any, len(source))
	for name, value := range source {
		if name == key {
			continue
		}
		out[name] = value
	}
	return out
}

func prettyJSON(value any) string {
	encoded, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		return ""
	}
	return string(encoded)
}
