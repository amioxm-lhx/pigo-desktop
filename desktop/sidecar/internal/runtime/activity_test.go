package runtime

import (
	"context"
	"encoding/json"
	"io"
	"log"
	"strings"
	"testing"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
)

func TestActivityStatusEvents(t *testing.T) {
	session, emitter := steerTestSession(t)
	session.mu.Lock()
	session.running = true
	session.currentTurn = "turn-activity"
	session.mu.Unlock()
	t.Cleanup(func() {
		session.mu.Lock()
		session.running = false
		session.currentTurn = ""
		session.mu.Unlock()
	})

	session.setActivity("waiting-model", "")
	event := waitStatusEvent(t, emitter)
	if event.Status.Activity == nil || event.Status.Activity.Phase != "waiting-model" {
		t.Fatalf("status activity = %#v", event.Status.Activity)
	}
	if status := session.Status(); status.Activity == nil || status.Activity.Phase != "waiting-model" {
		t.Fatalf("session status = %#v", status)
	}

	session.setActivity("waiting-model", "")
	select {
	case unexpected := <-emitter.ch:
		t.Fatalf("unchanged activity emitted %#v", unexpected.Event)
	case <-time.After(100 * time.Millisecond):
	}

	session.clearActivity()
	event = waitStatusEvent(t, emitter)
	if event.Status.Activity != nil {
		t.Fatalf("cleared activity = %#v", event.Status.Activity)
	}
}

func TestExecuteApprovedPlanStartsInternalTurn(t *testing.T) {
	params := historyTestParams("u1", "start")
	params.SessionID = "session-plan-exec"
	emitter := &captureEmitter{ch: make(chan protocol.EventEnvelope, 64)}
	session, err := NewSession(
		context.Background(),
		&fakeHost{},
		emitter,
		log.New(io.Discard, "", 0),
		params,
		textStream{}.Stream,
		provider.Model{Provider: "desktop", ID: "test-model"},
	)
	if err != nil {
		t.Fatalf("NewSession: %v", err)
	}

	execution := protocol.PlanExecution{
		ID:         "exec-1",
		ProposalID: "proposal-1",
		SessionID:  "session-plan-exec",
		Kind:       KindPlan,
		Plan:       "# Approved\n\nDo the thing.",
		Title:      "Approved plan",
		Question:   "Approve?",
		Artifact:   &protocol.PlanArtifact{RelativePath: ".pi/plans/exec-1.md", SHA256: "abc", SizeBytes: 10},
		State:      "running",
	}
	if err := session.ExecuteApprovedPlan(execution, "turn-exec"); err != nil {
		t.Fatalf("ExecuteApprovedPlan: %v", err)
	}

	timeout := time.After(3 * time.Second)
	for {
		select {
		case envelope := <-emitter.ch:
			if _, ok := envelope.Event.(protocol.AgentEndEvent); ok {
				if session.Running() {
					t.Fatal("session still running after agent_end")
				}
				return
			}
		case <-timeout:
			t.Fatal("timed out waiting for agent_end")
		}
	}
}

func TestExecuteApprovedPlanRejectsBadBinding(t *testing.T) {
	session, _ := steerTestSession(t)
	execution := protocol.PlanExecution{SessionID: "another-session", Kind: KindPlan, Plan: "x"}
	if err := session.ExecuteApprovedPlan(execution, "turn-exec"); err == nil {
		t.Fatal("wrong session should be rejected")
	}
	if err := session.ExecuteApprovedPlan(protocol.PlanExecution{SessionID: session.sessionID, Kind: KindPlan}, ""); err == nil {
		t.Fatal("empty turn id should be rejected")
	}
}

func TestPlanExecutionInstruction(t *testing.T) {
	plan := planExecutionInstruction(protocol.PlanExecution{
		Kind:     KindPlan,
		Plan:     "# Plan",
		Title:    "Title",
		Question: "Approve?",
		Artifact: &protocol.PlanArtifact{RelativePath: "plans/p.md"},
	})
	for _, want := range []string{"Execute the approved plan now", "Title", "Approve?", "plans/p.md", "<approved-plan-markdown>", "# Plan"} {
		if !strings.Contains(plan, want) {
			t.Fatalf("plan instruction missing %q:\n%s", want, plan)
		}
	}

	goal := planExecutionInstruction(protocol.PlanExecution{Kind: KindGoal, Plan: "# Goal"})
	if !strings.Contains(goal, "approved goal contract") || !strings.Contains(goal, "<approved-goal-markdown>") {
		t.Fatalf("goal instruction = %s", goal)
	}
}

type textStream struct{}

func (textStream) Stream(ctx context.Context, _ string, _ provider.LlmContext, _ provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
	stream := provider.NewAssistantMessageEventStream(4)
	go func() {
		defer stream.Close()
		now := time.Now().UnixMilli()
		final := agentcore.AssistantMessage{
			RoleField:  agentcore.RoleAssistant,
			Content:    agentcore.ContentList{agentcore.NewTextContent("done")},
			StopReason: agentcore.StopReasonEndTurn,
			Timestamp:  now,
		}
		_ = stream.Emit(ctx, provider.StreamStartEvent{Partial: agentcore.AssistantMessage{RoleField: agentcore.RoleAssistant, Timestamp: now}})
		_ = stream.Emit(ctx, provider.StreamTextEvent{Partial: final})
		_ = stream.Emit(ctx, provider.StreamDoneEvent{Message: final})
	}()
	return stream, nil
}

func waitStatusEvent(t *testing.T, emitter *captureEmitter) protocol.StatusEvent {
	t.Helper()
	select {
	case envelope := <-emitter.ch:
		event, ok := envelope.Event.(protocol.StatusEvent)
		if !ok {
			t.Fatalf("event = %T, want status", envelope.Event)
		}
		return event
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for status event")
	}
	return protocol.StatusEvent{}
}

func TestExecuteApprovedPlanProtocolParams(t *testing.T) {
	raw := json.RawMessage(`{
		"sessionId":"s","mode":"agent","turnId":"turn","provider":{"id":"p","modelId":"m","authKind":"none"},
		"execution":{"id":"e","sessionId":"s","kind":"goal","plan":"# Goal","title":"T","question":"Q","artifact":{"relativePath":"a","sha256":"x","sizeBytes":1,"state":"running"}}
	}`)
	var parsed protocol.ExecuteApprovedPlanParams
	if err := json.Unmarshal(raw, &parsed); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if parsed.SessionID != "s" || parsed.Execution == nil || parsed.Execution.ID != "e" || parsed.Execution.Kind != KindGoal {
		t.Fatalf("parsed = %#v", parsed)
	}
}
