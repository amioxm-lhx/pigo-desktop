package runtime

import (
	"encoding/json"
	"strings"
	"testing"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
)

// marshalSummary renders the export document compactly for key assertions.
func marshalSummary(s TelemetrySummary) (string, error) {
	b, err := json.Marshal(s)
	return string(b), err
}

// containsKey reports whether the compact JSON contains a literal key fragment.
func containsKey(doc, fragment string) bool {
	return strings.Contains(doc, fragment)
}

// TestTelemetryMCPAudit covers the §3.7.6 audit trail: tool_execution_end
// events whose tool name carries the frozen mcp_ namespace fold into a
// separate MCPTools slice of the summary event — server id, tool name, and
// duration, never payloads — while builtin tools stay out of it.
func TestTelemetryMCPAudit(t *testing.T) {
	acc := newTelemetry()
	acc.now = func() time.Time { return time.Unix(0, 0) }
	// One mcp tool call: start at t0, end at t0+150ms.
	acc.observe(agentcore.ToolExecutionStartEvent{ToolCallID: "c1", ToolName: "mcp_files_read"})
	acc.now = func() time.Time { return time.Unix(0, 150_000_000) }
	acc.observe(agentcore.ToolExecutionEndEvent{ToolCallID: "c1", ToolName: "mcp_files_read"})
	// A second call to the same mcp tool, and one builtin tool that must NOT
	// enter the MCP slice.
	acc.observe(agentcore.ToolExecutionStartEvent{ToolCallID: "c2", ToolName: "mcp_files_read"})
	acc.now = func() time.Time { return time.Unix(0, 350_000_000) }
	acc.observe(agentcore.ToolExecutionEndEvent{ToolCallID: "c2", ToolName: "mcp_files_read"})
	acc.observe(agentcore.ToolExecutionStartEvent{ToolCallID: "c3", ToolName: "read"})
	acc.now = func() time.Time { return time.Unix(0, 400_000_000) }
	acc.observe(agentcore.ToolExecutionEndEvent{ToolCallID: "c3", ToolName: "read"})

	sum := acc.summary()
	if got := sum.MCPTools["mcp_files_read"]; got.Count != 2 || got.TotalMs != 350 {
		t.Errorf("MCPTools[mcp_files_read] = %+v, want count 2 / 350ms", got)
	}
	if _, ok := sum.MCPTools["read"]; ok {
		t.Error("builtin tool leaked into MCPTools")
	}
	if got := sum.ToolDurationsMs["read"]; got.Count != 1 || got.TotalMs != 50 {
		t.Errorf("builtin timing broken: %+v", got)
	}
	if got := sum.ToolDurationsMs["mcp_files_read"]; got.Count != 2 {
		t.Errorf("mcp timing missing from the general slice: %+v", got)
	}
}

// TestTelemetrySummaryMCPToolsOmitted pins the additive export field: an event
// without MCP calls produces a document without the mcp_tools key
// (byte-identical to the pre-MCP golden); an event with MCP calls projects it.
func TestTelemetrySummaryMCPToolsOmitted(t *testing.T) {
	plain := TelemetrySummaryFromEvent(agentcore.TelemetryEvent{
		Turns:           1,
		ToolDurationsMs: map[string]agentcore.ToolTiming{"read": {Count: 1, TotalMs: 5}},
	})
	doc, err := marshalSummary(plain)
	if err != nil {
		t.Fatal(err)
	}
	if containsKey(doc, "mcp_tools") {
		t.Errorf("mcp_tools present on an MCP-free document: %s", doc)
	}
	withMCP := TelemetrySummaryFromEvent(agentcore.TelemetryEvent{
		Turns:           1,
		ToolDurationsMs: map[string]agentcore.ToolTiming{"mcp_fs_read": {Count: 2, TotalMs: 12}},
		MCPTools:        map[string]agentcore.ToolTiming{"mcp_fs_read": {Count: 2, TotalMs: 12}},
	})
	doc, err = marshalSummary(withMCP)
	if err != nil {
		t.Fatal(err)
	}
	if !containsKey(doc, `"mcp_tools"`) {
		t.Errorf("mcp_tools missing from an MCP-bearing document: %s", doc)
	}
	if !containsKey(doc, `"mcp_fs_read"`) {
		t.Errorf("namespaced tool missing from mcp_tools: %s", doc)
	}
}
