package runtime

import (
	"encoding/json"
	"testing"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
)

func pluginToolParams(tools []protocol.PluginTool) protocol.PromptParams {
	return protocol.PromptParams{PluginTools: tools}
}

func TestBuildPluginToolsProxiesForcedNames(t *testing.T) {
	session := &Session{
		mode: "agent",
		params: pluginToolParams([]protocol.PluginTool{
			{
				Name:        "plugin_demo_hello_echo_text",
				Description: "Echo",
				Risk:        "low",
				Parameters:  json.RawMessage(`{"type":"object","properties":{"text":{"type":"string"}}}`),
				PluginID:    "demo.hello",
			},
			// A name outside the plugin prefix is ignored rather than exposed.
			{Name: "Read", Description: "shadow"},
		}),
	}
	tools := buildPluginTools(session)
	if len(tools) != 1 {
		t.Fatalf("expected one plugin tool, got %d", len(tools))
	}
	if tools[0].Name() != "plugin_demo_hello_echo_text" {
		t.Fatalf("unexpected name %q", tools[0].Name())
	}
	if string(tools[0].Schema()) != `{"type":"object","properties":{"text":{"type":"string"}}}` {
		t.Fatalf("schema not carried through: %s", tools[0].Schema())
	}
}

func TestBuildPluginToolsFallBackToAnEmptySchema(t *testing.T) {
	session := &Session{
		mode: "agent",
		params: pluginToolParams([]protocol.PluginTool{{
			Name:       "plugin_demo_bad_schema",
			Parameters: json.RawMessage(`not json`),
		}}),
	}
	tools := buildPluginTools(session)
	if len(tools) != 1 {
		t.Fatalf("expected one plugin tool, got %d", len(tools))
	}
	var schema map[string]any
	if err := json.Unmarshal(tools[0].Schema(), &schema); err != nil {
		t.Fatalf("fallback schema is not JSON: %v", err)
	}
	if schema["type"] != "object" {
		t.Fatalf("unexpected fallback schema %v", schema)
	}
}

func TestHostToolByNameClonesAnInheritedPluginTool(t *testing.T) {
	parent := &hostTool{
		definition: protocol.ToolDefinition{
			Name:        "plugin_demo_echo",
			Description: "Echo",
			Risk:        "low",
			Parameters:  json.RawMessage(`{"type":"object"}`),
		},
		declaredRisk:    "low",
		planSafeActions: []string{"read"},
	}
	session := &Session{
		mode:      "agent",
		liveTools: map[string]agentcore.AgentTool{"plugin_demo_echo": parent},
	}
	child := hostToolByName(session, nil, "plugin_demo_echo", "accept-edits")
	if child == nil {
		t.Fatal("inherited plugin tool was not resolved")
	}
	if child.declaredRisk != "low" || len(child.planSafeActions) != 1 {
		t.Fatalf("plugin declaration lost in clone: %+v", child)
	}
	if child.permissionScope != "accept-edits" || child.session != session {
		t.Fatalf("delegate scope not applied: %+v", child)
	}
}

func TestBuildPluginToolsAreHiddenInContractModes(t *testing.T) {
	for _, mode := range []string{"plan", "goal"} {
		hidden := &Session{
			mode: mode,
			params: pluginToolParams([]protocol.PluginTool{{
				Name: "plugin_demo_hello_echo_text",
			}}),
		}
		if tools := buildPluginTools(hidden); len(tools) != 0 {
			t.Fatalf("%s exposed %d plugin tools", mode, len(tools))
		}
		exposed := &Session{
			mode: mode,
			params: pluginToolParams([]protocol.PluginTool{{
				Name:            "plugin_demo_safe_tool",
				Risk:            "low",
				PlanSafeActions: []string{"read"},
			}}),
		}
		if tools := buildPluginTools(exposed); len(tools) != 1 {
			t.Fatalf("%s hid a plan-safe plugin tool", mode)
		}
	}
}
