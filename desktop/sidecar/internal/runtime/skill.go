package runtime

import (
	"context"
	"encoding/json"
	"os"
	"strings"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
)

// SkillToolName is the model-facing name of the on-demand skill loader.
const SkillToolName = "Skill"

const skillToolDescription = "Load one skill's instructions by its exact id from the Skills catalog. Use this when a task matches a skill's description, then follow the returned instructions."

// skillDocumentMaxBytes bounds one skill body so a pathological document
// cannot flood the context window.
const skillDocumentMaxBytes = 256 * 1024

var skillToolSchema = json.RawMessage(`{
  "type": "object",
  "properties": {
    "id": {"type": "string"}
  },
  "required": ["id"]
}`)

// skillToolImpl loads a skill document the desktop resolved for this turn.
type skillToolImpl struct {
	session *Session
}

func newSkillTool(session *Session) agentcore.AgentTool {
	return &skillToolImpl{session: session}
}

func (t *skillToolImpl) Name() string        { return SkillToolName }
func (t *skillToolImpl) Description() string { return skillToolDescription }
func (t *skillToolImpl) Schema() json.RawMessage {
	return append(json.RawMessage(nil), skillToolSchema...)
}
func (t *skillToolImpl) ExecutionMode() agentcore.ToolExecutionMode {
	return agentcore.ToolExecutionSequential
}

func (t *skillToolImpl) Execute(ctx context.Context, id string, args json.RawMessage, _ agentcore.ToolUpdateFunc) (agentcore.AgentToolResult, error) {
	var input struct {
		ID string `json:"id"`
	}
	if err := json.Unmarshal(args, &input); err != nil {
		return t.failure(id, "SKILL_INVALID_ARGUMENT", "Skill requires an id."), nil
	}
	skillID := strings.TrimSpace(input.ID)
	if skillID == "" {
		return t.failure(id, "SKILL_INVALID_ARGUMENT", "Skill requires an id."), nil
	}

	skill, ok := t.session.pluginSkill(skillID)
	if !ok {
		return t.failure(id, "SKILL_NOT_FOUND", "No skill with id `"+skillID+"` is available in this session."), nil
	}
	if strings.TrimSpace(skill.Path) == "" {
		return t.failure(id, "SKILL_NOT_AVAILABLE", "The skill `"+skillID+"` has no readable document in this session."), nil
	}
	info, err := os.Stat(skill.Path)
	if err != nil || info.IsDir() {
		return t.failure(id, "SKILL_NOT_AVAILABLE", "The skill document for `"+skillID+"` is not readable."), nil
	}
	if info.Size() > skillDocumentMaxBytes {
		return t.failure(id, "SKILL_TOO_LARGE", "The skill document for `"+skillID+"` exceeds the 256 KiB limit."), nil
	}
	body, err := os.ReadFile(skill.Path)
	if err != nil {
		return t.failure(id, "SKILL_NOT_AVAILABLE", "The skill document for `"+skillID+"` is not readable."), nil
	}
	return agentcore.AgentToolResult{
		Content: agentcore.ContentList{agentcore.NewTextContent(string(body))},
		Details: map[string]any{"skillId": skillID, "name": skill.Name},
	}, nil
}

func (t *skillToolImpl) failure(toolCallID, code, message string) agentcore.AgentToolResult {
	t.session.markToolError(toolCallID)
	return agentcore.AgentToolResult{
		Content: agentcore.ContentList{agentcore.NewTextContent(message)},
		Details: map[string]any{"errorCode": code},
	}
}

// pluginSkill looks up one catalog entry under the session lock.
func (s *Session) pluginSkill(id string) (protocol.PluginSkill, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	for _, skill := range s.params.PluginSkills {
		if strings.EqualFold(skill.ID, id) {
			return skill, true
		}
	}
	return protocol.PluginSkill{}, false
}
