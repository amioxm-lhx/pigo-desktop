package runtime

import (
	"context"
	"encoding/json"
	"strings"
	"testing"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
)

type askToolCallResult struct {
	result agentcore.AgentToolResult
	err    error
}

func runAskTool(t *testing.T, session *Session, ctx context.Context, callID, args string) <-chan askToolCallResult {
	t.Helper()
	done := make(chan askToolCallResult, 1)
	tool := newAskTool(session)
	go func() {
		result, err := tool.Execute(ctx, callID, json.RawMessage(args), nil)
		done <- askToolCallResult{result: result, err: err}
	}()
	return done
}

func waitAskToolRequest(t *testing.T, emitter *captureEmitter) protocol.AskToolRequestEvent {
	t.Helper()
	select {
	case envelope := <-emitter.ch:
		event, ok := envelope.Event.(protocol.AskToolRequestEvent)
		if !ok {
			t.Fatalf("event = %T, want asktool_request", envelope.Event)
		}
		return event
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for asktool_request")
	}
	return protocol.AskToolRequestEvent{}
}

func askToolTestSession(t *testing.T) (*Session, *captureEmitter) {
	t.Helper()
	session, emitter := steerTestSession(t)
	session.mu.Lock()
	session.running = true
	session.currentTurn = "turn-ask"
	session.mu.Unlock()
	t.Cleanup(func() {
		session.mu.Lock()
		session.running = false
		session.currentTurn = ""
		session.mu.Unlock()
	})
	return session, emitter
}

func TestAskToolResolvesAnswers(t *testing.T) {
	session, emitter := askToolTestSession(t)
	done := runAskTool(t, session, context.Background(), "call-ask", `{"questions":[
		{"question":"Pick one","options":["A","B"]},
		{"question":"Pick many","options":["x","y","z"],"multiSelect":true}
	]}`)

	request := waitAskToolRequest(t, emitter)
	if request.Request.SessionID != session.sessionID || request.Request.ToolCallID != "call-ask" {
		t.Fatalf("request = %#v", request.Request)
	}
	if len(request.Request.Questions) != 2 || request.Request.Questions[0].Question != "Pick one" {
		t.Fatalf("questions = %#v", request.Request.Questions)
	}

	resolved, err := session.ResolveAskTool(protocol.AskToolParams{
		SessionID: session.sessionID,
		RequestID: request.Request.RequestID,
		Answers: []json.RawMessage{
			json.RawMessage(`["B"]`),
			json.RawMessage(`["x","y"]`),
		},
	})
	if err != nil || !resolved.OK {
		t.Fatalf("ResolveAskTool = %+v, %v", resolved, err)
	}

	select {
	case call := <-done:
		if call.err != nil {
			t.Fatalf("asktool Execute: %v", call.err)
		}
		text := agentcore.ContentToText(call.result.Content)
		if text != "Pick one：B\n---\nPick many：x、y" {
			t.Fatalf("asktool output = %q", text)
		}
		details, ok := call.result.Details.(askToolDetails)
		if !ok || len(details.Answers) != 2 || len(details.Answers[0]) != 1 || len(details.Answers[1]) != 2 {
			t.Fatalf("asktool details = %#v", call.result.Details)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("asktool did not return after resolution")
	}
}

func TestAskToolRejectsInvalidAnswersAndKeepsRequestPending(t *testing.T) {
	session, emitter := askToolTestSession(t)
	done := runAskTool(t, session, context.Background(), "call-ask", `{"questions":[
		{"question":"Pick one","options":["A","B"]}
	]}`)
	request := waitAskToolRequest(t, emitter)

	if _, err := session.ResolveAskTool(protocol.AskToolParams{
		SessionID: session.sessionID,
		RequestID: request.Request.RequestID,
		Answers:   nil,
	}); err == nil {
		t.Fatal("resolve with no answers should fail")
	}
	if _, err := session.ResolveAskTool(protocol.AskToolParams{
		SessionID: session.sessionID,
		RequestID: request.Request.RequestID,
		Answers:   []json.RawMessage{json.RawMessage(`["A","B"]`)},
	}); err == nil {
		t.Fatal("resolve with two answers for a single-select question should fail")
	}

	if _, err := session.ResolveAskTool(protocol.AskToolParams{
		SessionID: session.sessionID,
		RequestID: request.Request.RequestID,
		Answers:   []json.RawMessage{json.RawMessage(`["A"]`)},
	}); err != nil {
		t.Fatalf("valid resolve: %v", err)
	}
	if call := <-done; call.err != nil || agentcore.ContentToText(call.result.Content) != "Pick one：A" {
		t.Fatalf("asktool result = %#v, %v", call.result, call.err)
	}
}

func TestAskToolCancelReturnsSkippedAnswers(t *testing.T) {
	session, emitter := askToolTestSession(t)
	ctx, cancel := context.WithCancel(context.Background())
	done := runAskTool(t, session, ctx, "call-ask", `{"questions":[
		{"question":"Pick","options":["A"]}
	]}`)
	request := waitAskToolRequest(t, emitter)
	cancel()

	select {
	case call := <-done:
		if call.err != nil {
			t.Fatalf("canceled asktool Execute: %v", call.err)
		}
		if text := agentcore.ContentToText(call.result.Content); text != "Pick：" {
			t.Fatalf("canceled asktool output = %q", text)
		}
		details, ok := call.result.Details.(askToolDetails)
		if !ok || len(details.Answers) != 1 || details.Answers[0] != nil {
			t.Fatalf("canceled asktool details = %#v", call.result.Details)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("asktool did not return after cancellation")
	}

	if _, err := session.ResolveAskTool(protocol.AskToolParams{
		SessionID: session.sessionID,
		RequestID: request.Request.RequestID,
		Answers:   []json.RawMessage{json.RawMessage(`["A"]`)},
	}); err == nil {
		t.Fatal("a canceled asktool should no longer be pending")
	}
}

func TestAskToolInvalidArguments(t *testing.T) {
	session, _ := askToolTestSession(t)
	tool := newAskTool(session)
	result, err := tool.Execute(context.Background(), "call-invalid", json.RawMessage(`{"questions":[]}`), nil)
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}
	if text := agentcore.ContentToText(result.Content); text != askToolInvalidText {
		t.Fatalf("invalid output = %q", text)
	}
	if !session.consumeToolError("call-invalid") {
		t.Fatal("invalid asktool should be marked as a failed tool call")
	}
}

func TestNormalizeAskQuestions(t *testing.T) {
	questions := normalizeAskQuestions(map[string]any{
		"questions": []any{
			map[string]any{
				"question":    " Pick ",
				"options":     []any{" A ", "A", "", "B"},
				"multiSelect": true,
			},
		},
	})
	if len(questions) != 1 {
		t.Fatalf("questions = %#v", questions)
	}
	if questions[0].Question != "Pick" || questions[0].Options[0] != "A" ||
		questions[0].Options[1] != "B" || !questions[0].MultiSelect {
		t.Fatalf("question = %#v", questions[0])
	}

	for _, invalid := range []any{
		map[string]any{"questions": "no"},
		map[string]any{"questions": []any{map[string]any{"question": "", "options": []any{"A"}}}},
		map[string]any{"questions": []any{map[string]any{"question": "Q", "options": []any{}}}},
		map[string]any{"questions": []any{map[string]any{"question": "Q", "options": []any{"A"}}, "bad"}},
	} {
		if got := normalizeAskQuestions(invalid); got != nil {
			t.Fatalf("invalid args accepted: %#v", got)
		}
	}
}

func TestFormatAskToolOutput(t *testing.T) {
	output := formatAskToolOutput(
		[]protocol.AskToolQuestion{{Question: "Q1"}, {Question: "Q2"}},
		[][]string{{"A", "B"}, nil},
	)
	if !strings.Contains(output, "Q1：A、B\n---\nQ2：") {
		t.Fatalf("output = %q", output)
	}
}
