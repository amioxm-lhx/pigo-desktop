package runtime

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
)

func skillTestSession(t *testing.T, skills []protocol.PluginSkill) *Session {
	t.Helper()
	return &Session{
		params:     protocol.PromptParams{PluginSkills: skills},
		toolErrors: map[string]bool{},
	}
}

func TestSkillToolLoadsDocumentByID(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "deploy.md")
	body := "# Deploy\n\nRun the release checklist."
	if err := os.WriteFile(path, []byte(body), 0o600); err != nil {
		t.Fatalf("write skill: %v", err)
	}
	session := skillTestSession(t, []protocol.PluginSkill{{
		ID: "deploy", Name: "Deploy", Description: "Ship safely.", Path: path,
	}})
	tool := newSkillTool(session)
	result, err := tool.Execute(context.Background(), "call-1", json.RawMessage(`{"id":"deploy"}`), nil)
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}
	if got := strings.TrimSpace(agentcore.ContentToText(result.Content)); got != body {
		t.Fatalf("skill body = %q", got)
	}
	if session.toolErrors["call-1"] {
		t.Fatal("a successful skill load must not mark the tool failed")
	}
}

func TestSkillToolRejectsUnknownID(t *testing.T) {
	session := skillTestSession(t, []protocol.PluginSkill{{ID: "deploy", Name: "Deploy"}})
	tool := newSkillTool(session)
	result, err := tool.Execute(context.Background(), "call-2", json.RawMessage(`{"id":"missing"}`), nil)
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}
	details, ok := result.Details.(map[string]any)
	if !ok || details["errorCode"] != "SKILL_NOT_FOUND" {
		t.Fatalf("details = %#v", result.Details)
	}
	if !session.toolErrors["call-2"] {
		t.Fatal("an unknown skill must mark the tool failed")
	}
}
