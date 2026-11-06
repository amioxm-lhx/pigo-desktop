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

type modeHost struct {
	*fakeHost
	proposal     *protocol.PlanProposal
	submitStatus string
	enterCalls   []map[string]any
	submitCalls  []map[string]any
}

func (h *modeHost) Call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	switch method {
	case "plans.enter":
		if record, ok := params.(map[string]any); ok {
			h.enterCalls = append(h.enterCalls, record)
		}
		return json.Marshal(map[string]any{"ok": true, "state": "planning", "kind": "plan"})
	case "plans.submit":
		if record, ok := params.(map[string]any); ok {
			h.submitCalls = append(h.submitCalls, record)
		}
		status := h.submitStatus
		if status == "" {
			status = "pending"
		}
		return json.Marshal(map[string]any{"status": status, "proposal": h.proposal})
	default:
		return h.fakeHost.Call(ctx, method, params)
	}
}

func (h *modeHost) CallInto(ctx context.Context, method string, params any, out any) error {
	raw, err := h.Call(ctx, method, params)
	if err != nil {
		return err
	}
	if out == nil {
		return nil
	}
	return json.Unmarshal(raw, out)
}

func modeTestSession(t *testing.T, host *modeHost, mode string) (*Session, *captureEmitter) {
	t.Helper()
	params := historyTestParams("u1", "start")
	params.SessionID = "session-mode"
	params.Mode = mode
	emitter := &captureEmitter{ch: make(chan protocol.EventEnvelope, 4)}
	session, err := NewSession(
		context.Background(),
		host,
		emitter,
		log.New(io.Discard, "", 0),
		params,
		(&scriptedStream{}).Stream,
		provider.Model{Provider: "desktop", ID: "test-model"},
	)
	if err != nil {
		t.Fatalf("NewSession: %v", err)
	}
	session.mu.Lock()
	session.running = true
	session.currentTurn = "turn-mode"
	session.mu.Unlock()
	t.Cleanup(func() {
		session.mu.Lock()
		session.running = false
		session.currentTurn = ""
		session.mu.Unlock()
	})
	return session, emitter
}

func TestEnterPlanModeCommitsAndPublishesPlanningState(t *testing.T) {
	host := &modeHost{fakeHost: &fakeHost{}}
	session, emitter := modeTestSession(t, host, "agent")

	result, err := newEnterModeTool(session, KindPlan).Execute(context.Background(), "call-enter", json.RawMessage(`{}`), nil)
	if err != nil {
		t.Fatalf("EnterPlanMode: %v", err)
	}
	if text := agentcore.ContentToText(result.Content); text == "" {
		t.Fatal("empty EnterPlanMode result")
	}
	if session.Mode() != KindPlan {
		t.Fatalf("mode = %q, want plan", session.Mode())
	}
	if !strings.Contains(session.agentCtx.SystemPrompt, "You are in Plan mode") {
		t.Fatalf("system prompt did not switch to plan:\n%s", session.agentCtx.SystemPrompt)
	}
	if len(host.enterCalls) != 1 || host.enterCalls[0]["kind"] != KindPlan ||
		host.enterCalls[0]["turnId"] != "turn-mode" || host.enterCalls[0]["toolCallId"] != "call-enter" {
		t.Fatalf("plans.enter calls = %#v", host.enterCalls)
	}

	event := waitPlanningState(t, emitter)
	if event.State != planningStatePlanning || event.Kind != KindPlan {
		t.Fatalf("planning state = %#v", event)
	}
}

func TestSubmitPlanPublishesProposalAndTerminates(t *testing.T) {
	proposal := &protocol.PlanProposal{
		ID:         "proposal-1",
		SessionID:  "session-mode",
		TurnID:     "turn-mode",
		ToolCallID: "call-submit",
		Kind:       KindPlan,
		Title:      "Plan title",
		Markdown:   "# Plan\n\nDo the thing.",
		Plan:       "# Plan\n\nDo the thing.",
		Question:   "Approve?",
		Artifact:   &protocol.PlanArtifact{RelativePath: ".pi/plans/plan.md", SHA256: "abc", SizeBytes: 12},
		Version:    1,
		Status:     "pending",
	}
	host := &modeHost{fakeHost: &fakeHost{}, proposal: proposal}
	session, emitter := modeTestSession(t, host, KindPlan)

	result, err := newSubmitContractTool(session, KindPlan).Execute(context.Background(), "call-submit", json.RawMessage(`{
		"title":"Plan title",
		"markdown":"# Plan\n\nDo the thing.",
		"question":"Approve?"
	}`), nil)
	if err != nil {
		t.Fatalf("SubmitPlan: %v", err)
	}
	if result.Terminate == nil || !*result.Terminate {
		t.Fatalf("SubmitPlan terminate = %#v", result.Terminate)
	}
	details, ok := result.Details.(map[string]any)
	if !ok || details["proposal"] == nil {
		t.Fatalf("SubmitPlan details = %#v", result.Details)
	}
	if len(host.submitCalls) != 1 || host.submitCalls[0]["kind"] != KindPlan ||
		host.submitCalls[0]["turnId"] != "turn-mode" || host.submitCalls[0]["toolCallId"] != "call-submit" {
		t.Fatalf("plans.submit calls = %#v", host.submitCalls)
	}

	status := session.Status()
	if status.PlanningState != planningStateAwaitingApproval || status.PendingPlanID != "proposal-1" {
		t.Fatalf("status = %#v", status)
	}
	event := waitPlanningState(t, emitter)
	if event.State != planningStateAwaitingApproval || event.Kind != KindPlan ||
		event.ProposalID != "proposal-1" || event.Proposal == nil {
		t.Fatalf("planning state = %#v", event)
	}
	if event.Proposal.Title != "Plan title" || event.Proposal.Artifact == nil {
		t.Fatalf("planning proposal = %#v", event.Proposal)
	}
}

func TestSubmitPlanRejectsInvalidArguments(t *testing.T) {
	host := &modeHost{fakeHost: &fakeHost{}}
	session, _ := modeTestSession(t, host, KindPlan)
	result, err := newSubmitContractTool(session, KindPlan).Execute(context.Background(), "call-bad", json.RawMessage(`{"title":"","markdown":"","question":""}`), nil)
	if err != nil {
		t.Fatalf("SubmitPlan: %v", err)
	}
	if result.Terminate == nil || *result.Terminate {
		t.Fatalf("invalid submit terminate = %#v, want false", result.Terminate)
	}
	if !session.consumeToolError("call-bad") {
		t.Fatal("invalid submit should be marked as a failed tool call")
	}
	if len(host.submitCalls) != 0 {
		t.Fatalf("invalid submit reached host: %#v", host.submitCalls)
	}
}

func TestSubmitPlanRejectsInvalidHostProposal(t *testing.T) {
	host := &modeHost{fakeHost: &fakeHost{}}
	session, _ := modeTestSession(t, host, KindPlan)
	result, err := newSubmitContractTool(session, KindPlan).Execute(context.Background(), "call-submit", json.RawMessage(`{
		"title":"Plan title","markdown":"# Plan","question":"Approve?"
	}`), nil)
	if err != nil {
		t.Fatalf("SubmitPlan: %v", err)
	}
	if result.Terminate == nil || !*result.Terminate {
		t.Fatalf("invalid proposal terminate = %#v, want true", result.Terminate)
	}
	if !session.consumeToolError("call-submit") {
		t.Fatal("invalid proposal should be marked as a failed tool call")
	}
}

func TestModeToolAvailability(t *testing.T) {
	host := &modeHost{fakeHost: &fakeHost{}}
	agentSession, _ := modeTestSession(t, host, "agent")
	if decision := agentSession.beforeToolCall(context.Background(), agentcore.AgentToolCall{Name: SubmitPlanTool}); decision == nil || !decision.Block {
		t.Fatal("SubmitPlan should be blocked in Agent mode")
	}
	if decision := agentSession.beforeToolCall(context.Background(), agentcore.AgentToolCall{Name: EnterPlanModeTool}); decision != nil && decision.Block {
		t.Fatal("EnterPlanMode should be allowed in Agent mode")
	}

	planSession, _ := modeTestSession(t, host, KindPlan)
	if decision := planSession.beforeToolCall(context.Background(), agentcore.AgentToolCall{Name: SubmitPlanTool}); decision != nil && decision.Block {
		t.Fatal("SubmitPlan should be allowed in Plan mode")
	}
	if decision := planSession.beforeToolCall(context.Background(), agentcore.AgentToolCall{Name: "Write"}); decision == nil || !decision.Block {
		t.Fatal("Write should be blocked in Plan mode")
	}
	if decision := planSession.beforeToolCall(context.Background(), agentcore.AgentToolCall{Name: EnterPlanModeTool}); decision == nil || !decision.Block {
		t.Fatal("EnterPlanMode should be blocked outside Agent mode")
	}
}

func waitPlanningState(t *testing.T, emitter *captureEmitter) protocol.PlanningStateEvent {
	t.Helper()
	select {
	case envelope := <-emitter.ch:
		event, ok := envelope.Event.(protocol.PlanningStateEvent)
		if !ok {
			t.Fatalf("event = %T, want planning_state", envelope.Event)
		}
		return event
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for planning_state")
	}
	return protocol.PlanningStateEvent{}
}
