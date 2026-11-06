package runtime

import (
	"context"
	"errors"
	"fmt"
	"io"
	"log"
	"sync/atomic"
	"testing"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
)

func TestClassifyProviderFailures(t *testing.T) {
	cases := []struct {
		message   string
		code      string
		retriable bool
	}{
		{"transport: upstream 429: slow down", "PROVIDER_RATE_LIMITED", true},
		{"transport: upstream 401: invalid api key", "PROVIDER_UNAUTHORIZED", false},
		{"transport: upstream 500: boom", "PROVIDER_ERROR", true},
		{"context length exceeded", "CONTEXT_TOO_LARGE", false},
		{"transport: upstream 400: context window exceeded", "CONTEXT_TOO_LARGE", false},
		{"transport: upstream 400: malformed request", "PROVIDER_ERROR", false},
		{"connection refused", "NETWORK_ERROR", true},
		{"request timed out", "TIMEOUT", true},
		{"model does not exist", "MODEL_NOT_CONFIGURED", false},
	}
	for _, testCase := range cases {
		classified := classifyError(testCase.message, nil, false)
		if classified.Code != testCase.code || classified.Retriable != testCase.retriable {
			t.Errorf("classify(%q) = %s/%v, want %s/%v", testCase.message, classified.Code, classified.Retriable, testCase.code, testCase.retriable)
		}
		if classified.Message == "" || len([]rune(classified.Message)) > maxErrorMessageChars {
			t.Errorf("message bounds violated: %q", classified.Message)
		}
	}
	if classified := classifyError("provider exploded", nil, true); classified.Code != "TURN_ABORTED" {
		t.Errorf("aborted classification = %s", classified.Code)
	}
}

func withFastRetries(t *testing.T) {
	t.Helper()
	oldDelay, oldWait := retryDelayHook, retryWaitHook
	retryDelayHook = func(ClassifiedError, int) int { return 0 }
	retryWaitHook = func(context.Context, time.Duration) bool { return true }
	t.Cleanup(func() {
		retryDelayHook = oldDelay
		retryWaitHook = oldWait
	})
}

func drainEvents(t *testing.T, emitter *captureEmitter) []protocol.EventEnvelope {
	t.Helper()
	var events []protocol.EventEnvelope
	timeout := time.After(5 * time.Second)
	for {
		select {
		case envelope := <-emitter.ch:
			events = append(events, envelope)
			if _, ok := envelope.Event.(protocol.AgentEndEvent); ok {
				return events
			}
		case <-timeout:
			t.Fatal("timed out waiting for agent_end")
		}
	}
}

func TestProviderRetryRecoversSetupFailure(t *testing.T) {
	withFastRetries(t)
	var calls int32
	stream := func(ctx context.Context, model string, llm provider.LlmContext, cfg provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		if atomic.AddInt32(&calls, 1) == 1 {
			return nil, errors.New("transport: upstream 429: slow down")
		}
		return textStream{}.Stream(ctx, model, llm, cfg)
	}

	params := historyTestParams("u1", "hello")
	params.SessionID = "session-retry"
	emitter := &captureEmitter{ch: make(chan protocol.EventEnvelope, 64)}
	session, err := NewSession(context.Background(), &fakeHost{}, emitter, log.New(io.Discard, "", 0), params, stream, provider.Model{Provider: "desktop", ID: "test-model"})
	if err != nil {
		t.Fatalf("NewSession: %v", err)
	}
	if err := session.Prompt("turn-retry", "u1", "hello"); err != nil {
		t.Fatalf("Prompt: %v", err)
	}

	events := drainEvents(t, emitter)
	var retrying *protocol.AgentActivity
	for _, envelope := range events {
		status, ok := envelope.Event.(protocol.StatusEvent)
		if !ok || status.Status.Activity == nil {
			continue
		}
		if status.Status.Activity.Phase == "retrying" {
			retrying = status.Status.Activity
		}
	}
	if retrying == nil {
		t.Fatal("no retrying activity was emitted")
	}
	if retrying.Attempt != 1 || retrying.Error == nil || retrying.Error.Code != "PROVIDER_RATE_LIMITED" {
		t.Fatalf("retrying activity = %#v", retrying)
	}
	if got := atomic.LoadInt32(&calls); got != 2 {
		t.Fatalf("provider calls = %d, want 2", got)
	}
}

func TestProviderRetryLeavesAuthFailureTerminal(t *testing.T) {
	withFastRetries(t)
	var calls int32
	stream := func(context.Context, string, provider.LlmContext, provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		atomic.AddInt32(&calls, 1)
		return nil, errors.New("transport: upstream 401: invalid api key")
	}
	params := historyTestParams("u1", "hello")
	params.SessionID = "session-auth"
	emitter := &captureEmitter{ch: make(chan protocol.EventEnvelope, 64)}
	session, err := NewSession(context.Background(), &fakeHost{}, emitter, log.New(io.Discard, "", 0), params, stream, provider.Model{Provider: "desktop", ID: "test-model"})
	if err != nil {
		t.Fatalf("NewSession: %v", err)
	}
	if err := session.Prompt("turn-auth", "u1", "hello"); err != nil {
		t.Fatalf("Prompt: %v", err)
	}
	events := drainEvents(t, emitter)
	var failure *protocol.AppError
	for _, envelope := range events {
		if event, ok := envelope.Event.(protocol.ErrorEvent); ok {
			errCopy := event.Error
			failure = &errCopy
		}
	}
	if failure == nil || failure.Code != "PROVIDER_UNAUTHORIZED" {
		t.Fatalf("error = %#v", failure)
	}
	if got := atomic.LoadInt32(&calls); got != 1 {
		t.Fatalf("auth failure retried %d times", got)
	}
}

func TestOverflowCompactionRetriesTheRequest(t *testing.T) {
	host := &checkpointHost{fakeHost: &fakeHost{sessionMessages: longTranscript(10)}}
	var calls int32
	stream := func(ctx context.Context, model string, llm provider.LlmContext, cfg provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		switch atomic.AddInt32(&calls, 1) {
		case 1:
			return nil, errors.New("transport: upstream 400: context length exceeded")
		default:
			return textStream{}.Stream(ctx, model, llm, cfg)
		}
	}
	emitter := &captureEmitter{ch: make(chan protocol.EventEnvelope, 128)}
	params := historyTestParams("u11", "next")
	params.SessionID = "session-overflow"
	session, err := NewSession(context.Background(), host, emitter, log.New(io.Discard, "", 0), params, stream,
		provider.Model{Provider: "desktop", ID: "test-model", ContextWindow: 1200})
	if err != nil {
		t.Fatalf("NewSession: %v", err)
	}
	if err := session.Prompt("turn-overflow", "u11", "next"); err != nil {
		t.Fatalf("Prompt: %v", err)
	}

	events := drainEvents(t, emitter)
	var overflow *protocol.CompactionEndEvent
	for _, envelope := range events {
		if event, ok := envelope.Event.(protocol.CompactionEndEvent); ok && event.Reason == "overflow" {
			overflow = &event
		}
	}
	if overflow == nil {
		t.Fatal("no overflow compaction event")
	}
	if !overflow.OK || !overflow.WillRetry || overflow.Mark == nil {
		t.Fatalf("overflow event = %#v", overflow)
	}
	if got := atomic.LoadInt32(&calls); got != 3 {
		t.Fatalf("provider calls = %d, want failed request + summary + retry", got)
	}
	call := host.lastCompaction(t)
	if call.Compaction.Summary == "" || call.Compaction.ThroughMessageID == "" {
		t.Fatalf("checkpoint = %#v", call.Compaction)
	}
	session.mu.Lock()
	head := session.agentCtx.Messages[0]
	session.mu.Unlock()
	first, ok := head.(agentcore.CompactionMessage)
	if !ok || first.Summary != call.Compaction.Summary {
		t.Fatalf("live context head = %#v", head)
	}
}

func TestManualCompactInstallsCheckpoint(t *testing.T) {
	host := &checkpointHost{fakeHost: &fakeHost{sessionMessages: []historyMessage{
		{ID: "u1", Role: "user", Content: "hello", CreatedAt: "2026-11-03T09:00:00Z"},
		{ID: "a1", Role: "assistant", Content: "hi", CreatedAt: "2026-11-03T09:00:01Z"},
	}}}
	emitter := &captureEmitter{ch: make(chan protocol.EventEnvelope, 64)}
	params := historyTestParams("", "")
	params.SessionID = "session-manual"
	session, err := NewSession(context.Background(), host, emitter, log.New(io.Discard, "", 0), params, textStream{}.Stream,
		provider.Model{Provider: "desktop", ID: "test-model", ContextWindow: 100000})
	if err != nil {
		t.Fatalf("NewSession: %v", err)
	}
	if err := session.Compact(context.Background()); err != nil {
		t.Fatalf("Compact: %v", err)
	}

	var ended *protocol.CompactionEndEvent
	for {
		select {
		case envelope := <-emitter.ch:
			if event, ok := envelope.Event.(protocol.CompactionEndEvent); ok && event.Reason == "manual" {
				ended = &event
			}
		case <-time.After(100 * time.Millisecond):
			goto done
		}
	}
done:
	if ended == nil || !ended.OK || ended.Mark == nil {
		t.Fatalf("manual compaction event = %#v", ended)
	}
	if len(session.agentCtx.Messages) != 1 {
		t.Fatalf("compacted context = %d messages, want the checkpoint only", len(session.agentCtx.Messages))
	}
	checkpoint, ok := session.agentCtx.Messages[0].(agentcore.CompactionMessage)
	if !ok || checkpoint.Summary == "" {
		t.Fatalf("checkpoint = %#v", session.agentCtx.Messages[0])
	}
	call := host.lastCompaction(t)
	if call.Compaction.ThroughMessageID != "a1" {
		t.Fatalf("throughMessageId = %q", call.Compaction.ThroughMessageID)
	}
	if fmt.Sprint(call.Compaction.Details["retainedTailMode"]) != compactionRetentionCompletedTurn {
		t.Fatalf("details = %#v", call.Compaction.Details)
	}
}
