package runtime

import (
	"encoding/json"
	"strings"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
)

// The prompt is assembled here, from Electron main's resolved context only.
// pigo's own BuildSystemPrompt is deliberately not used: it would append a
// host environment block and read AGENTS.md from the sidecar's filesystem,
// duplicating the desktop's instruction chain and leaking machine state into
// the model context.

const identityPrompt = "You are Pi Desktop's coding agent, running locally in the user's project. Prefer concise, actionable answers and use the provided tools when they help."

const collaborationPrompt = "Collaboration: answer in the language the user writes in. Before each batch of tool calls, write one short sentence in the same assistant message saying what you are about to do. The user only sees your visible text, so put the conclusion there rather than only in your reasoning. Make the final message self-contained: the outcome, what changed, and anything still open. Carry the work through end to end; when you hit a blocker, try to clear it yourself and report what you tried."

const searchReadPrompt = "Searching and reading: prefer the Read, Grep, and Glob tools over shell cat, sed, head, grep, or find. Read only regular text files; use Glob when the path is uncertain or a directory must be listed. Scope searches with the tool's own path/include/limit parameters and do not re-run a search whose answer you already have. Prefer workspace-relative paths."

const nativeToolCallPrompt = "Call tools through the native tool-call interface only. Never write a tool call as assistant text and never emit a multi_tool_use.parallel or {\"tool_uses\": [...]} wrapper: there is no such tool here, and a call written as prose does not run."

const editingWorkflowPrompt = "Editing workflow: use Edit for one small line-anchored change and Write for a coherent whole-file rewrite. Do not invoke shell apply_patch, git apply, or hand-edited diff files. After a failed edit, re-read the target and retry with corrected content; never issue concurrent Write/Edit calls for the same path. Stop after three failed attempts on one path and report the exact mismatch."

const agentModePrompt = "You are in Agent mode. After the user approves a plan or asks for implementation, carry out the work with the available tools and report the result clearly."

const planModePrompt = "You are in Plan mode. Inspect the workspace and relevant context, reason about the requested change, and produce a concrete implementation plan with files, behavior, and validation steps. Do not use Write or Edit. Bash is available under the permission policy but may mutate files, so use it only when it materially helps inspection. Present the plan in your visible text; do not implement changes."

const goalModePrompt = "You are in Goal mode. Negotiate what to achieve before any autonomous work: the desired outcome, acceptance criteria that can be objectively checked after execution, and the boundaries you must not cross. Do not use Write or Edit. Bash may mutate files, so use it only when it materially helps understand the goal. Present the contract in your visible text; do not implement changes until it is approved."

// BuildSystemPrompt composes the session's system prompt from PromptParams.
// Every block is deterministic and comes from Electron main's payload; no
// pigo defaults or on-disk instruction files participate.
func BuildSystemPrompt(params protocol.PromptParams) string {
	parts := []string{identityPrompt, collaborationPrompt, searchReadPrompt, nativeToolCallPrompt, editingWorkflowPrompt}

	if shell := shellPrompt(params.CommandShell, params.ScratchDir); shell != "" {
		parts = append(parts, shell)
	}
	if scratch := strings.TrimSpace(params.ScratchDir); scratch != "" {
		parts = append(parts, "Your scratch directory for this session is `"+scratch+"`. Write temporary and intermediate files there, using absolute paths; only write deliverables into the workspace.")
	}

	parts = append(parts, modePrompt(params.Mode))

	if project := strings.TrimSpace(params.ProjectPath); project != "" {
		parts = append(parts, "The active workspace root is "+project+".")
	}
	if instructions := renderInstructions(params.ProjectInstructions); instructions != "" {
		parts = append(parts, instructions)
	}
	if skills := pluginSkillsPrompt(params.PluginSkills); skills != "" {
		parts = append(parts, skills)
	}
	if memory := strings.TrimSpace(params.ProjectMemory); memory != "" {
		parts = append(parts, "# Project memory\n\n"+memory)
	}
	return strings.Join(nonEmpty(parts), "\n\n")
}

// pluginSkillsPrompt renders the skill catalog. Only id/name/description ship
// up front; the body is loaded through the Skill tool when the model decides a
// skill applies, so a long document costs nothing until it is needed.
func pluginSkillsPrompt(skills []protocol.PluginSkill) string {
	if len(skills) == 0 {
		return ""
	}
	lines := []string{
		"# Skills",
		"",
		"Plugins and the user have taught you the following skills. Each entry is a set of instructions you can load with the `Skill` tool by passing its exact id. When a task matches a skill's description, load the skill first and follow it; do not guess at its content. Load each skill at most once per task.",
		"",
	}
	for _, skill := range skills {
		description := strings.TrimSpace(skill.Description)
		entry := "- `" + skill.ID + "` — " + skill.Name
		if description != "" {
			entry += ": " + description
		}
		lines = append(lines, entry)
	}
	return strings.Join(lines, "\n")
}

func modePrompt(mode string) string {
	switch normalizeMode(mode) {
	case "plan":
		return planModePrompt
	case "goal":
		return goalModePrompt
	default:
		return agentModePrompt
	}
}

func shellPrompt(shell protocol.CommandShell, scratchDir string) string {
	if strings.TrimSpace(shell.ID) == "" {
		return ""
	}
	label := strings.TrimSpace(shell.Label)
	if label == "" {
		label = shell.ID
	}
	syntax := "Use POSIX shell syntax and forward-slash paths."
	switch shell.Dialect {
	case "powershell":
		syntax = "Use native PowerShell syntax and Windows paths."
	case "cmd":
		syntax = "Use native cmd.exe syntax and Windows paths."
	}
	scratchVariable := "$PI_SCRATCH_DIR"
	switch shell.Dialect {
	case "powershell":
		scratchVariable = "$env:PI_SCRATCH_DIR"
	case "cmd":
		scratchVariable = "%PI_SCRATCH_DIR%"
	}
	scratch := "Keep temporary files in the session scratch directory and use " + scratchVariable + " to refer to it."
	if strings.TrimSpace(scratchDir) != "" {
		scratch = "The session scratch directory is `" + strings.TrimSpace(scratchDir) + "`; use " + scratchVariable + " to refer to it."
	}
	return "Shell commands run through " + label + " (" + shell.ID + "). The protocol tool remains named Bash for compatibility. " + syntax + " " + scratch
}

type instructionEntry struct {
	Path    string `json:"path"`
	Source  string `json:"source"`
	Content string `json:"content"`
}

type instructionChain struct {
	Entries []instructionEntry `json:"entries"`
}

// renderInstructions formats the host-resolved instruction files in the order
// Electron main returned them.
func renderInstructions(raw json.RawMessage) string {
	if len(raw) == 0 {
		return ""
	}
	var chain instructionChain
	if err := json.Unmarshal(raw, &chain); err != nil || len(chain.Entries) == 0 {
		return ""
	}
	sections := make([]string, 0, len(chain.Entries))
	for _, entry := range chain.Entries {
		content := strings.TrimSpace(entry.Content)
		if content == "" {
			continue
		}
		header := "# Project instructions"
		if label := firstNonEmpty(entry.Source, entry.Path); label != "" {
			header += " (" + label + ")"
		}
		sections = append(sections, header+"\n\n"+content)
	}
	return strings.Join(sections, "\n\n")
}

func nonEmpty(values []string) []string {
	out := make([]string, 0, len(values))
	for _, value := range values {
		if strings.TrimSpace(value) != "" {
			out = append(out, value)
		}
	}
	return out
}
