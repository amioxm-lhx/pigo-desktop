// BrowserPreview: open a workspace HTML file in the desktop's embedded browser
// preview.
//
// The tool is sidecar-local, like the mode tools: the Tauri shell owns the
// surface and answers `tools.execute` for it, so host-core never runs the call
// and its tools.list catalogue never names it. The shell serves the file over a
// loopback preview endpoint and opens the renderer's browser tab, which
// live-reloads when the page changes; when that service cannot start it falls
// back to the system browser.
package runtime

import (
	"context"
	"encoding/json"
	"errors"
	"strings"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
)

// BrowserPreviewToolName is the model-facing name of the browser preview.
const BrowserPreviewToolName = "BrowserPreview"

const browserPreviewDescription = "Open a workspace HTML file in PI-Agent-Go-Desktop's embedded browser preview. `path` is workspace-relative (e.g. \"demo/index.html\"). The preview live-reloads when the page or its sibling assets change, so call once per page. It does not provide screenshots, viewport controls, DOM interaction, keyboard simulation or reduced-motion emulation; use project-provided browser or E2E tooling for those checks."

var browserPreviewSchema = json.RawMessage(`{
  "type": "object",
  "properties": {
    "path": {
      "type": "string",
      "description": "File to preview; workspace-relative."
    },
    "file_path": {
      "type": "string",
      "description": "Alias for path."
    }
  },
  "additionalProperties": false
}`)

type browserPreviewTool struct {
	session *Session
	errors  *subagentToolErrors
}

func (t *browserPreviewTool) Name() string        { return BrowserPreviewToolName }
func (t *browserPreviewTool) Description() string { return browserPreviewDescription }
func (t *browserPreviewTool) Schema() json.RawMessage {
	return append(json.RawMessage(nil), browserPreviewSchema...)
}
func (t *browserPreviewTool) ExecutionMode() agentcore.ToolExecutionMode {
	return agentcore.ToolExecutionSequential
}

func (t *browserPreviewTool) Execute(ctx context.Context, id string, args json.RawMessage, _ agentcore.ToolUpdateFunc) (agentcore.AgentToolResult, error) {
	path, err := browserPreviewPath(args)
	if err != nil {
		return t.failure(id, "BROWSER_PREVIEW_INVALID", err.Error()), nil
	}
	if t.session == nil || t.session.host == nil {
		return t.failure(id, "HOST_UNAVAILABLE", "BrowserPreview needs the desktop host."), nil
	}
	normalized, marshalErr := json.Marshal(map[string]string{"path": path})
	if marshalErr != nil {
		return t.failure(id, "BROWSER_PREVIEW_INVALID", "BrowserPreview could not encode its arguments."), nil
	}
	request := protocol.ToolExecuteParams{
		SessionID:  t.session.sessionID,
		TurnID:     t.session.CurrentTurnID(),
		ToolCallID: id,
		ToolName:   BrowserPreviewToolName,
		Args:       normalized,
		Mode:       t.session.Mode(),
	}
	var reply protocol.ToolExecuteResult
	if callErr := t.session.host.CallInto(ctx, "tools.execute", request, &reply); callErr != nil {
		return t.failure(id, "BROWSER_PREVIEW_FAILED", "BrowserPreview failed: "+callErr.Error()), nil
	}
	content, details, isError := toolResultContent(reply, t.session.model.SupportsImages)
	if isError {
		t.markFailed(id)
	}
	return agentcore.AgentToolResult{Content: content, Details: details}, nil
}

// browserPreviewPath folds the `file_path` alias onto `path` and validates the
// workspace-relative shape before the shell ever sees it. The shell remains the
// authority for existence, containment and the HTML extension.
func browserPreviewPath(args json.RawMessage) (string, error) {
	if len(args) == 0 {
		return "", errors.New("BrowserPreview requires a workspace-relative `path`.")
	}
	var record map[string]any
	if err := json.Unmarshal(args, &record); err != nil {
		return "", errors.New("BrowserPreview arguments are not an object.")
	}
	path, _ := record["path"].(string)
	if strings.TrimSpace(path) == "" {
		path, _ = record["file_path"].(string)
	}
	return normalizeBrowserPreviewPath(path)
}

// normalizeBrowserPreviewPath accepts one workspace-relative path: forward or
// Windows separators, no absolute or drive prefix, and no `..` segment. Empty
// and `.` segments are dropped, so `./demo//index.html` means `demo/index.html`.
func normalizeBrowserPreviewPath(raw string) (string, error) {
	trimmed := strings.TrimSpace(raw)
	if trimmed == "" {
		return "", errors.New("BrowserPreview requires a workspace-relative `path`.")
	}
	if strings.ContainsRune(trimmed, 0) {
		return "", errors.New("BrowserPreview `path` contains an invalid character.")
	}
	normalized := strings.ReplaceAll(trimmed, "\\", "/")
	if strings.HasPrefix(normalized, "/") {
		return "", errors.New("BrowserPreview `path` must be workspace-relative, not absolute.")
	}
	if hasWindowsDrivePrefix(normalized) {
		return "", errors.New("BrowserPreview `path` must be workspace-relative, not a drive path.")
	}
	segments := make([]string, 0, strings.Count(normalized, "/")+1)
	for _, segment := range strings.Split(normalized, "/") {
		switch segment {
		case "", ".":
			continue
		case "..":
			return "", errors.New("BrowserPreview `path` must stay inside the workspace (no `..`).")
		}
		segments = append(segments, segment)
	}
	if len(segments) == 0 {
		return "", errors.New("BrowserPreview requires a workspace-relative file path.")
	}
	return strings.Join(segments, "/"), nil
}

// hasWindowsDrivePrefix reports whether a normalized path starts with `X:`.
// A colon later in a segment is a legal POSIX file name and stays allowed.
func hasWindowsDrivePrefix(path string) bool {
	if len(path) < 2 || path[1] != ':' {
		return false
	}
	first := path[0]
	return (first >= 'a' && first <= 'z') || (first >= 'A' && first <= 'Z')
}

func (t *browserPreviewTool) failure(toolCallID, code, message string) agentcore.AgentToolResult {
	t.markFailed(toolCallID)
	return agentcore.AgentToolResult{
		Content: agentcore.ContentList{agentcore.NewTextContent(message)},
		Details: map[string]any{"errorCode": code},
	}
}

func (t *browserPreviewTool) markFailed(toolCallID string) {
	if t.errors != nil {
		t.errors.mark(toolCallID)
		return
	}
	if t.session != nil {
		t.session.markToolError(toolCallID)
	}
}
