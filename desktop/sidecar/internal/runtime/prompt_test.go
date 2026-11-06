package runtime

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
)

func TestBuildSystemPromptRendersPluginSkills(t *testing.T) {
	params := protocol.PromptParams{
		PluginSkills: []protocol.PluginSkill{
			{ID: "deploy", Name: "Deploy", Description: "Ship safely."},
			{ID: "notes", Name: "Notes"},
		},
	}
	prompt := BuildSystemPrompt(params)
	for _, want := range []string{
		"# Skills",
		"`Skill` tool",
		"- `deploy` — Deploy: Ship safely.",
		"- `notes` — Notes",
	} {
		if !strings.Contains(prompt, want) {
			t.Errorf("prompt missing %q:\n%s", want, prompt)
		}
	}
	if empty := BuildSystemPrompt(protocol.PromptParams{}); strings.Contains(empty, "# Skills") {
		t.Error("an empty skill catalog must not render a Skills section")
	}
}

func TestBuildSystemPromptUsesResolvedDesktopContext(t *testing.T) {
	params := protocol.PromptParams{
		Mode:        "plan",
		ProjectPath: "/work/app",
		ProjectInstructions: json.RawMessage(`{"entries":[
			{"path":"/work/app/AGENTS.md","content":"rule one"},
			{"path":"/work/app/docs/RULES.md","content":"rule two"}
		]}`),
		ProjectMemory: "remember the deploy command",
		ScratchDir:    "/tmp/pi-scratch",
		CommandShell:  protocol.CommandShell{ID: "pwsh", Label: "PowerShell", Dialect: "powershell"},
	}

	prompt := BuildSystemPrompt(params)
	for _, want := range []string{
		"You are Pi Desktop's coding agent",
		"Collaboration:",
		"Searching and reading:",
		"Call tools through the native tool-call interface only",
		"Editing workflow:",
		"Shell commands run through PowerShell (pwsh)",
		"$env:PI_SCRATCH_DIR",
		"/tmp/pi-scratch",
		"You are in Plan mode",
		"The active workspace root is /work/app.",
		"# Project instructions (/work/app/AGENTS.md)",
		"rule one",
		"# Project instructions (/work/app/docs/RULES.md)",
		"rule two",
		"# Project memory",
		"remember the deploy command",
	} {
		if !strings.Contains(prompt, want) {
			t.Errorf("prompt missing %q:\n%s", want, prompt)
		}
	}
	for _, forbidden := range []string{
		// pigo's BuildSystemPrompt injects these; the sidecar must not.
		"Environment:",
		"Working directory:",
		"- OS:",
		"You are in Agent mode",
	} {
		if strings.Contains(prompt, forbidden) {
			t.Errorf("prompt unexpectedly contains %q", forbidden)
		}
	}
	if strings.Index(prompt, "Collaboration:") > strings.Index(prompt, "You are in Plan mode") {
		t.Error("mode framing must come after collaboration guidance")
	}
	if strings.Index(prompt, "# Project instructions") > strings.Index(prompt, "# Project memory") {
		t.Error("instructions must come before project memory")
	}
}

func TestBuildSystemPromptRendersInstructionSource(t *testing.T) {
	prompt := BuildSystemPrompt(protocol.PromptParams{
		ProjectInstructions: json.RawMessage(`{"entries":[{"source":"AGENTS.md","content":"source rule"}]}`),
	})
	if !strings.Contains(prompt, "# Project instructions (AGENTS.md)") {
		t.Fatalf("prompt missing source label:\n%s", prompt)
	}
	if !strings.Contains(prompt, "source rule") {
		t.Fatalf("prompt missing instruction body:\n%s", prompt)
	}
}

func TestBuildSystemPromptModeFraming(t *testing.T) {
	cases := map[string]string{
		"agent": "You are in Agent mode",
		"plan":  "You are in Plan mode",
		"goal":  "You are in Goal mode",
	}
	for mode, want := range cases {
		prompt := BuildSystemPrompt(protocol.PromptParams{Mode: mode})
		if !strings.Contains(prompt, want) {
			t.Errorf("mode %q prompt missing %q", mode, want)
		}
	}
}

func TestBuildSystemPromptShellDialects(t *testing.T) {
	cases := []struct {
		name     string
		shell    protocol.CommandShell
		scratch  string
		variable string
	}{
		{name: "posix", shell: protocol.CommandShell{ID: "bash", Dialect: "posix"}, variable: "$PI_SCRATCH_DIR"},
		{name: "powershell", shell: protocol.CommandShell{ID: "pwsh", Dialect: "powershell"}, variable: "$env:PI_SCRATCH_DIR"},
		{name: "cmd", shell: protocol.CommandShell{ID: "cmd", Dialect: "cmd"}, variable: "%PI_SCRATCH_DIR%"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			prompt := BuildSystemPrompt(protocol.PromptParams{CommandShell: tc.shell, ScratchDir: "/tmp/scratch"})
			if !strings.Contains(prompt, tc.variable) {
				t.Errorf("prompt missing %q", tc.variable)
			}
			if !strings.Contains(prompt, "/tmp/scratch") {
				t.Error("prompt missing scratch directory")
			}
		})
	}
}
