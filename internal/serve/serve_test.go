// Tests for the serve layer: the full headless-shaped flow — create session,
// subscribe, prompt, await, persist, reopen, continue — driven by scripted
// streams, plus the lifecycle edges (concurrent prompt, steer, abort,
// dispose) and the factory-facing knobs (SetModel/SetThinking).
package serve

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/agenttool"
	"github.com/smallnest/pigo/internal/provider"
	"github.com/smallnest/pigo/internal/runtime"
	"github.com/smallnest/pigo/internal/session"
)

// textTurnStream returns a StreamFn whose every call completes one plain
// text turn (no tool calls), so a run always settles after one assistant
// message.
func textTurnStream(text string) provider.StreamFn {
	return func(ctx context.Context, model string, llm provider.LlmContext, cfg provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		final := agentcore.AssistantMessage{
			RoleField:  agentcore.RoleAssistant,
			Content:    agentcore.ContentList{agentcore.NewTextContent(text)},
			StopReason: agentcore.StopReasonEndTurn,
		}
		s := provider.NewAssistantMessageEventStream(0)
		go func() {
			_ = s.Emit(ctx, provider.StreamDoneEvent{Message: final})
			s.Close()
		}()
		return s, nil
	}
}

// recordingStream wraps fn, recording the LlmContext messages of each call
// so tests can assert what the conversation actually fed the model.
func recordingStream(fn provider.StreamFn, seen *[]agentcore.MessageList) provider.StreamFn {
	return func(ctx context.Context, model string, llm provider.LlmContext, cfg provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		*seen = append(*seen, llm.Messages)
		return fn(ctx, model, llm, cfg)
	}
}

// gatedTool is an AgentTool whose execution blocks until release is closed,
// giving a test a deterministic window to steer mid-run.
type gatedTool struct{ release chan struct{} }

func (g gatedTool) Name() string            { return "gated" }
func (g gatedTool) Description() string     { return "gated test tool" }
func (g gatedTool) Schema() json.RawMessage { return nil }
func (g gatedTool) ExecutionMode() agentcore.ToolExecutionMode {
	return agentcore.ToolExecutionParallel
}
func (g gatedTool) Execute(ctx context.Context, id string, args json.RawMessage, onUpdate agentcore.ToolUpdateFunc) (agentcore.AgentToolResult, error) {
	<-g.release
	return agentcore.AgentToolResult{Content: agentcore.ContentList{agentcore.NewTextContent("gated done")}}, nil
}

// newTestService builds a Service over a temp store whose factory returns
// the given stream, recording the headers and thinking levels it saw.
func newTestService(t *testing.T, stream provider.StreamFn) (Service, session.Store, *[]session.SessionHeader, *[]agentcore.ThinkingLevel) {
	t.Helper()
	store, err := session.NewStore(t.TempDir())
	if err != nil {
		t.Fatalf("NewStore: %v", err)
	}
	var headers []session.SessionHeader
	var thinkings []agentcore.ThinkingLevel
	svc, err := NewService(store, func(h session.SessionHeader, th agentcore.ThinkingLevel) runtime.RunConfig {
		headers = append(headers, h)
		thinkings = append(thinkings, th)
		return runtime.RunConfig{
			LoopConfig: runtime.LoopConfig{Model: h.Model, Provider: h.Provider, Stream: stream},
		}
	})
	if err != nil {
		t.Fatalf("NewService: %v", err)
	}
	return svc, store, &headers, &thinkings
}

// drainUntilAgentEnd reads the subscription until the run's agent_end event
// arrives (the subscription itself stays open across runs).
func drainUntilAgentEnd(t *testing.T, sub *Subscription) []agentcore.AgentEvent {
	t.Helper()
	var got []agentcore.AgentEvent
	for ev := range sub.Events() {
		got = append(got, ev)
		if _, ok := ev.(agentcore.AgentEndEvent); ok {
			return got
		}
	}
	t.Fatal("subscription closed before agent_end")
	return got
}

// eventKinds maps events to their type strings for sequence assertions.
func eventKinds(evs []agentcore.AgentEvent) []string {
	out := make([]string, len(evs))
	for i, ev := range evs {
		out[i] = ev.EventType()
	}
	return out
}

func TestCreatePromptPersistE2E(t *testing.T) {
	svc, store, headers, _ := newTestService(t, textTurnStream("done!"))

	rt, err := svc.CreateSession(context.Background(), SessionConfig{
		Model: "fake", Provider: "p1", SystemPrompt: "sys", Cwd: "/w",
	})
	if err != nil {
		t.Fatalf("CreateSession: %v", err)
	}
	sub, err := rt.Subscribe()
	if err != nil {
		t.Fatalf("Subscribe: %v", err)
	}

	if err := rt.Prompt(context.Background(), "hello"); err != nil {
		t.Fatalf("Prompt: %v", err)
	}
	events := drainUntilAgentEnd(t, sub)

	// The run rides the stream untouched: agent_start → … → telemetry → agent_end.
	want := []string{
		agentcore.EventAgentStart, agentcore.EventTurnStart, agentcore.EventMessageEnd,
		agentcore.EventTurnEnd, agentcore.EventTelemetry, agentcore.EventAgentEnd,
	}
	got := eventKinds(events)
	if !equalStrings(got, want) {
		t.Fatalf("event kinds = %v, want %v", got, want)
	}

	if err := rt.WaitIdle(context.Background()); err != nil {
		t.Fatalf("WaitIdle: %v", err)
	}
	if rt.GetPhase() != PhaseIdle {
		t.Fatalf("phase = %v, want idle", rt.GetPhase())
	}

	snap, err := rt.Snapshot(context.Background())
	if err != nil {
		t.Fatalf("Snapshot: %v", err)
	}
	if len(snap.Entries) != 2 { // user prompt + assistant reply
		t.Fatalf("snapshot entries = %d, want 2", len(snap.Entries))
	}
	if snap.LeafID == "" {
		t.Fatal("snapshot leaf must be set after a persisted run")
	}
	if snap.Header.Model != "fake" || snap.Header.SystemPrompt != "sys" {
		t.Fatalf("snapshot header = %+v", snap.Header)
	}

	// The session really landed on disk and is listed.
	listed, err := svc.ListSessions(context.Background())
	if err != nil || len(listed) != 1 || listed[0].ID != rt.ID() {
		t.Fatalf("ListSessions = %v, %v; want exactly the created session", listed, err)
	}
	if h, _, err := store.Load(rt.ID()); err != nil || h.ID != rt.ID() {
		t.Fatalf("store.Load: %v, %+v", err, h)
	}

	// The factory saw the session's own header (system prompt included).
	if len(*headers) != 1 || (*headers)[0].SystemPrompt != "sys" {
		t.Fatalf("factory headers = %+v", *headers)
	}
}

func equalStrings(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func TestOpenSessionContinuesConversation(t *testing.T) {
	var seen []agentcore.MessageList
	stream := recordingStream(textTurnStream("done!"), &seen)
	svc, _, _, _ := newTestService(t, stream)

	rt, _ := svc.CreateSession(context.Background(), SessionConfig{Model: "fake", Provider: "p1"})
	if err := rt.Prompt(context.Background(), "first"); err != nil {
		t.Fatalf("Prompt: %v", err)
	}
	if err := rt.WaitIdle(context.Background()); err != nil {
		t.Fatalf("WaitIdle: %v", err)
	}

	// Reopen: the runtime must resume exactly where the first run persisted.
	rt2, err := svc.OpenSession(context.Background(), rt.ID())
	if err != nil {
		t.Fatalf("OpenSession: %v", err)
	}
	snap, _ := rt2.Snapshot(context.Background())
	if len(snap.Entries) != 2 || snap.LeafID == "" {
		t.Fatalf("reopened snapshot = %d entries, leaf %q", len(snap.Entries), snap.LeafID)
	}
	msgs := snap.Messages()
	if len(msgs) != 2 {
		t.Fatalf("snapshot messages = %d, want 2", len(msgs))
	}

	if err := rt2.Prompt(context.Background(), "second"); err != nil {
		t.Fatalf("Prompt 2: %v", err)
	}
	if err := rt2.WaitIdle(context.Background()); err != nil {
		t.Fatalf("WaitIdle 2: %v", err)
	}

	// The second run's model request started from the persisted history: the
	// conversation it saw was [user first, assistant done, user second].
	if len(seen) != 2 {
		t.Fatalf("stream called %d times, want 2", len(seen))
	}
	var kinds []string
	for _, m := range seen[1] {
		kinds = append(kinds, m.Role())
	}
	if !equalStrings(kinds, []string{"user", "assistant", "user"}) {
		t.Fatalf("second-run roles = %v, want [user assistant user]", kinds)
	}
	snap2, _ := rt2.Snapshot(context.Background())
	if len(snap2.Entries) != 4 {
		t.Fatalf("entries after second run = %d, want 4", len(snap2.Entries))
	}
}

// blockingStream returns a StreamFn that never completes a turn until its
// context is cancelled, for abort/dispose tests.
func blockingStream() provider.StreamFn {
	return func(ctx context.Context, model string, llm provider.LlmContext, cfg provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		s := provider.NewAssistantMessageEventStream(0)
		go func() {
			<-ctx.Done()
			s.Close()
		}()
		return s, nil
	}
}

func TestPromptWhileRunningRejectedAndAbort(t *testing.T) {
	svc, _, _, _ := newTestService(t, blockingStream())
	rt, _ := svc.CreateSession(context.Background(), SessionConfig{Model: "fake"})

	if err := rt.Prompt(context.Background(), "go"); err != nil {
		t.Fatalf("Prompt: %v", err)
	}
	if rt.GetPhase() != PhaseRunning {
		t.Fatal("phase should be running")
	}
	if err := rt.Prompt(context.Background(), "again"); !errors.Is(err, ErrRunActive) {
		t.Fatalf("second Prompt err = %v, want ErrRunActive", err)
	}
	if err := rt.Steer("late"); err != nil {
		t.Fatalf("Steer while running: %v", err)
	}

	rt.Abort()
	if err := rt.WaitIdle(context.Background()); err != nil {
		t.Fatalf("WaitIdle after abort: %v", err)
	}
	if rt.GetPhase() != PhaseIdle {
		t.Fatal("phase should be idle after abort")
	}

	// The aborted run still persisted what it produced (at least the prompt).
	snap, err := rt.Snapshot(context.Background())
	if err != nil {
		t.Fatalf("Snapshot: %v", err)
	}
	if len(snap.Entries) < 1 {
		t.Fatalf("aborted run persisted %d entries, want >= 1", len(snap.Entries))
	}
}

func TestSteerInjectsBetweenTurns(t *testing.T) {
	// Script: call 1 answers with a gated tool call; call 2+ answers with text.
	calls := 0
	var seen []agentcore.MessageList
	release := make(chan struct{})
	stream := func(ctx context.Context, model string, llm provider.LlmContext, cfg provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		seen = append(seen, llm.Messages)
		calls++
		var msg agentcore.AssistantMessage
		if calls == 1 {
			msg = agentcore.AssistantMessage{
				RoleField:  agentcore.RoleAssistant,
				StopReason: agentcore.StopReasonToolUse,
				Content:    agentcore.ContentList{agentcore.NewToolCallContent("c1", "gated", json.RawMessage(`{}`))},
			}
		} else {
			msg = agentcore.AssistantMessage{
				RoleField:  agentcore.RoleAssistant,
				StopReason: agentcore.StopReasonEndTurn,
				Content:    agentcore.ContentList{agentcore.NewTextContent("done")},
			}
		}
		s := provider.NewAssistantMessageEventStream(0)
		go func() {
			_ = s.Emit(ctx, provider.StreamDoneEvent{Message: msg})
			s.Close()
		}()
		return s, nil
	}

	store, err := session.NewStore(t.TempDir())
	if err != nil {
		t.Fatalf("NewStore: %v", err)
	}
	reg := agenttool.NewToolRegistry()
	if err := reg.Register(gatedTool{release: release}); err != nil {
		t.Fatalf("register tool: %v", err)
	}
	svc, err := NewService(store, func(h session.SessionHeader, th agentcore.ThinkingLevel) runtime.RunConfig {
		return runtime.RunConfig{
			LoopConfig: runtime.LoopConfig{Model: h.Model, Provider: h.Provider, Stream: stream},
			Batch:      agenttool.BatchConfig{ToolExecutorConfig: agenttool.ToolExecutorConfig{Registry: reg}},
		}
	})
	if err != nil {
		t.Fatalf("NewService: %v", err)
	}

	rt, _ := svc.CreateSession(context.Background(), SessionConfig{Model: "fake"})
	if err := rt.Prompt(context.Background(), "start"); err != nil {
		t.Fatalf("Prompt: %v", err)
	}

	// The run is parked inside the gated tool: steer now, deterministically.
	if err := rt.Steer("steer me"); err != nil {
		t.Fatalf("Steer: %v", err)
	}
	close(release)

	if err := rt.WaitIdle(context.Background()); err != nil {
		t.Fatalf("WaitIdle: %v", err)
	}

	// The second model call saw the steering message after the tool result.
	if len(seen) != 2 {
		t.Fatalf("stream called %d times, want 2", len(seen))
	}
	var steerSeen bool
	for _, m := range seen[1] {
		if u, ok := m.(agentcore.UserMessage); ok && strings.Contains(agentcore.ContentToText(u.Content), "steer me") {
			steerSeen = true
		}
	}
	if !steerSeen {
		t.Fatalf("second turn did not see the steering message: %+v", seen[1])
	}

	// Everything persisted: user, tool-call, tool-result, steer, final answer.
	snap, _ := rt.Snapshot(context.Background())
	if len(snap.Entries) != 5 {
		t.Fatalf("entries = %d, want 5: %+v", len(snap.Entries), snap.Entries)
	}
}

func TestDisposeClosesEverything(t *testing.T) {
	svc, _, _, _ := newTestService(t, blockingStream())
	rt, _ := svc.CreateSession(context.Background(), SessionConfig{Model: "fake"})
	sub, err := rt.Subscribe()
	if err != nil {
		t.Fatalf("Subscribe: %v", err)
	}
	if err := rt.Prompt(context.Background(), "go"); err != nil {
		t.Fatalf("Prompt: %v", err)
	}

	if err := rt.Dispose(); err != nil {
		t.Fatalf("Dispose: %v", err)
	}
	if err := rt.Dispose(); err != nil {
		t.Fatalf("Dispose must be idempotent: %v", err)
	}
	if rt.GetPhase() != PhaseDisposed {
		t.Fatal("phase should be disposed")
	}

	// The subscription channel closed (range loop ends).
	timeout := time.After(5 * time.Second)
	for {
		select {
		case _, ok := <-sub.Events():
			if !ok {
				goto closed
			}
		case <-timeout:
			t.Fatal("subscription channel was not closed by Dispose")
		}
	}
closed:
	if _, err := rt.Subscribe(); !errors.Is(err, ErrDisposed) {
		t.Fatalf("Subscribe after dispose = %v, want ErrDisposed", err)
	}
	if err := rt.Prompt(context.Background(), "x"); !errors.Is(err, ErrDisposed) {
		t.Fatalf("Prompt after dispose = %v, want ErrDisposed", err)
	}
	if _, err := rt.Snapshot(context.Background()); !errors.Is(err, ErrDisposed) {
		t.Fatalf("Snapshot after dispose = %v, want ErrDisposed", err)
	}
	if err := rt.Steer("x"); !errors.Is(err, ErrDisposed) {
		t.Fatalf("Steer after dispose = %v, want ErrDisposed", err)
	}
	if err := rt.WaitIdle(context.Background()); !errors.Is(err, ErrDisposed) {
		t.Fatalf("WaitIdle after dispose = %v, want ErrDisposed", err)
	}

	// The disposed run was aborted without persisting: nothing on disk.
	list, err := svc.ListSessions(context.Background())
	if err == nil && len(list) != 0 {
		t.Fatalf("disposed run persisted %d sessions, want 0", len(list))
	}
}

func TestListModels(t *testing.T) {
	svc, _, _, _ := newTestService(t, textTurnStream("x"))
	models, err := svc.ListModels(context.Background())
	if err != nil || len(models) == 0 {
		t.Fatalf("ListModels = %v, %v; want non-empty", models, err)
	}
	if models[0].Provider != "openrouter" || models[0].ID == "" {
		t.Fatalf("first model = %+v, want the catalog head", models[0])
	}

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := svc.ListModels(ctx); err == nil {
		t.Fatal("cancelled context should error")
	}
}

func TestSetModelSetThinkingVisibleToFactory(t *testing.T) {
	svc, _, headers, thinkings := newTestService(t, textTurnStream("done"))
	rt, _ := svc.CreateSession(context.Background(), SessionConfig{Model: "m1", Provider: "p1"})

	if err := rt.SetModel("m2"); err != nil {
		t.Fatalf("SetModel: %v", err)
	}
	if err := rt.SetThinking("high"); err != nil {
		t.Fatalf("SetThinking: %v", err)
	}
	if err := rt.Prompt(context.Background(), "go"); err != nil {
		t.Fatalf("Prompt: %v", err)
	}
	if err := rt.WaitIdle(context.Background()); err != nil {
		t.Fatalf("WaitIdle: %v", err)
	}

	if len(*headers) != 1 || (*headers)[0].Model != "m2" {
		t.Fatalf("factory saw model %v, want m2", *headers)
	}
	if len(*thinkings) != 1 || (*thinkings)[0] != "high" {
		t.Fatalf("factory saw thinking %v, want high", *thinkings)
	}
	// The header that lands on disk carries the switched model too.
	snap, _ := rt.Snapshot(context.Background())
	if snap.Header.Model != "m2" {
		t.Fatalf("persisted header model = %q, want m2", snap.Header.Model)
	}
}

func TestSnapshotMessagesProjection(t *testing.T) {
	svc, _, _, _ := newTestService(t, textTurnStream("reply"))
	rt, _ := svc.CreateSession(context.Background(), SessionConfig{Model: "fake"})
	if err := rt.Prompt(context.Background(), "question"); err != nil {
		t.Fatalf("Prompt: %v", err)
	}
	if err := rt.WaitIdle(context.Background()); err != nil {
		t.Fatalf("WaitIdle: %v", err)
	}
	snap, _ := rt.Snapshot(context.Background())
	msgs := snap.Messages()
	if len(msgs) != 2 {
		t.Fatalf("messages = %d, want 2", len(msgs))
	}
	if u, ok := msgs[0].(agentcore.UserMessage); !ok || agentcore.ContentToText(u.Content) != "question" {
		t.Fatalf("msgs[0] = %+v, want the user prompt", msgs[0])
	}
	if a, ok := msgs[1].(agentcore.AssistantMessage); !ok || agentcore.ContentToText(a.Content) != "reply" {
		t.Fatalf("msgs[1] = %+v, want the assistant reply", msgs[1])
	}
}
