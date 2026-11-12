package desktop

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/im"
)

// Polling cadence. The desktop control catalog has no event stream: the app
// pushes its own events into the renderer, not to us. Polling is the honest
// trade — a card that trails the app by a second, in exchange for not adding a
// subscription channel to the shell.
const (
	defaultPollInterval = 900 * time.Millisecond
	// transcriptEvery is how many polls pass between transcript reads. Reading
	// the tail means opening the session file, so it is cheaper to lag it.
	transcriptEvery = 3
	// runStartGrace keeps a fresh run from being reported as finished before
	// the sidecar has flipped its status. A turn that has not started yet looks
	// exactly like a turn that never will.
	runStartGrace = 8 * time.Second
)

// activityPhases maps the app's activity codes onto something a chat card can
// show. Unknown codes fall through to a generic label rather than leaking
// implementation words into the conversation.
var activityPhases = map[string]string{
	"starting":      "启动中",
	"waiting-model": "模型响应中",
	"compacting":    "压缩上下文",
}

// session implements im.AgentSession over the control catalog.
type session struct {
	c  *Client
	id string
}

func (s *session) ID() string { return s.id }

// Busy asks the app rather than caching: a run can be stopped from the UI, and
// a stale "running" answer would make /status lie.
func (s *session) Busy() bool {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	st, err := s.status(ctx)
	return err == nil && st.IsRunning
}

func (s *session) Prompt(ctx context.Context, text string) error {
	_, err := s.c.Invoke(ctx, "agent/prompt", map[string]any{
		"sessionId": s.id,
		"content":   text,
	})
	if err != nil {
		if isConflict(err) {
			return im.ErrRunActive
		}
		return err
	}
	return nil
}

func (s *session) Steer(text string) error {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	_, err := s.c.Invoke(ctx, "agent/steer", map[string]any{
		"sessionId": s.id,
		"content":   text,
	})
	return err
}

func (s *session) Abort() {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	_, _ = s.c.Invoke(ctx, "agent/stop", map[string]any{"sessionId": s.id})
}

// SetModel asks the app to reconfigure the session. session/configure requires
// the current mode, so it is read first instead of assumed: a session in plan
// mode must not be silently flipped to agent mode by a model switch.
func (s *session) SetModel(model string) error {
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	raw, err := s.c.Invoke(ctx, "session/get", map[string]any{"id": s.id, "messageLimit": 1})
	if err != nil {
		return err
	}
	var payload sessionPayload
	if err := json.Unmarshal(raw, &payload); err != nil {
		return fmt.Errorf("desktop session/get: %w", err)
	}
	if payload.Session == nil {
		return fmt.Errorf("会话 %s 在桌面端已不存在", s.id)
	}
	mode := payload.Session.Mode
	if mode == "" {
		mode = "agent"
	}
	_, err = s.c.Invoke(ctx, "session/configure", map[string]any{
		"id":      s.id,
		"mode":    mode,
		"modelId": model,
	})
	return err
}

// Close detaches without touching the conversation: the session belongs to the
// app, and a chat rebinding must never delete what the user sees on screen.
func (s *session) Close() error { return nil }

func (s *session) Subscribe() (im.EventStream, error) {
	st := &stream{
		sess:     s,
		interval: defaultPollInterval,
		ch:       make(chan agentcore.AgentEvent, 32),
		done:     make(chan struct{}),
	}
	go st.loop()
	return st, nil
}

// statusResult mirrors protocol.AgentStatusResult.
type statusResult struct {
	IsRunning bool `json:"isRunning"`
	Activity  *struct {
		Phase  string `json:"phase"`
		Reason string `json:"reason"`
	} `json:"activity"`
}

func (s *session) status(ctx context.Context) (statusResult, error) {
	raw, err := s.c.Invoke(ctx, "agent/status", map[string]any{"sessionId": s.id})
	if err != nil {
		return statusResult{}, err
	}
	var env struct {
		Status statusResult `json:"status"`
	}
	if err := json.Unmarshal(raw, &env); err != nil {
		return statusResult{}, fmt.Errorf("desktop agent/status: %w", err)
	}
	return env.Status, nil
}

// tailMessages reads the last few UI messages. Only the assistant text matters
// here: the card shows narration, and tools have no names in the status feed.
func (s *session) tailMessages(ctx context.Context, limit int) ([]uiMessage, error) {
	raw, err := s.c.Invoke(ctx, "session/get", map[string]any{
		"id":           s.id,
		"messageLimit": limit,
	})
	if err != nil {
		return nil, err
	}
	var payload struct {
		Session *struct {
			Messages []uiMessage `json:"messages"`
		} `json:"session"`
	}
	if err := json.Unmarshal(raw, &payload); err != nil {
		return nil, fmt.Errorf("desktop session/get: %w", err)
	}
	if payload.Session == nil {
		return nil, fmt.Errorf("会话 %s 在桌面端已不存在", s.id)
	}
	return payload.Session.Messages, nil
}

// uiMessage is the transcript row the app renders.
type uiMessage struct {
	ID      string `json:"id"`
	Role    string `json:"role"`
	Content string `json:"content"`
}

// stream synthesizes agentcore events from polling. It only emits what Router
// consumes: tool-ish steps for progress, assistant text for narration, and one
// AgentEndEvent to close the run.
type stream struct {
	sess     *session
	interval time.Duration
	ch       chan agentcore.AgentEvent
	done     chan struct{}
	stopOnce sync.Once
}

func (st *stream) Events() <-chan agentcore.AgentEvent { return st.ch }

func (st *stream) Close() {
	st.stopOnce.Do(func() {
		close(st.done)
		close(st.ch)
	})
}

func (st *stream) loop() {
	defer st.Close()

	ticker := time.NewTicker(st.interval)
	defer ticker.Stop()

	started := time.Now()
	var (
		seenRunning bool
		lastPhase   string
		lastText    string
		ticks       int
	)

	for {
		select {
		case <-st.done:
			return
		case <-ticker.C:
		}

		ticks++
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		status, err := st.sess.status(ctx)
		if err != nil {
			cancel()
			// A transient control failure must not end the card: the run is
			// still going in the app. Keep polling until the URL answers again
			// or Router's own run timeout fires.
			continue
		}

		if status.IsRunning {
			seenRunning = true
			phase := st.phaseLabel(status)
			if phase != lastPhase {
				if lastPhase != "" {
					st.emit(agentcore.ToolExecutionEndEvent{ToolName: lastPhase})
				}
				st.emit(agentcore.ToolExecutionStartEvent{ToolName: phase})
				lastPhase = phase
			}
			if ticks%transcriptEvery == 0 || lastText == "" {
				if text := st.readText(ctx, lastText); text != "" && text != lastText {
					lastText = text
					st.emit(agentcore.MessageEndEvent{Message: assistantMessage(text)})
				}
			}
			cancel()
			continue
		}

		// Not running. Either the run has not started yet (agent/prompt is
		// asynchronous) or it just finished.
		if !seenRunning && time.Since(started) < runStartGrace {
			cancel()
			continue
		}

		text := st.readText(ctx, lastText)
		cancel()
		if text != "" && text != lastText {
			lastText = text
			st.emit(agentcore.MessageEndEvent{Message: assistantMessage(text)})
		}
		msgs := []agentcore.AgentMessage{}
		if lastText != "" {
			msgs = append(msgs, assistantMessage(lastText))
		}
		st.emit(agentcore.AgentEndEvent{Messages: msgs})
		return
	}
}

// readText returns the latest assistant text that differs from previous.
func (st *stream) readText(ctx context.Context, previous string) string {
	msgs, err := st.sess.tailMessages(ctx, 6)
	if err != nil {
		return previous
	}
	for i := len(msgs) - 1; i >= 0; i-- {
		if msgs[i].Role != "assistant" {
			continue
		}
		if txt := strings.TrimSpace(msgs[i].Content); txt != "" {
			return txt
		}
	}
	return previous
}

func (st *stream) phaseLabel(status statusResult) string {
	if status.Activity != nil {
		if label, ok := activityPhases[status.Activity.Phase]; ok {
			return label
		}
		if status.Activity.Reason != "" {
			return status.Activity.Reason
		}
	}
	return "运行中"
}

func (st *stream) emit(ev agentcore.AgentEvent) {
	select {
	case st.ch <- ev:
	case <-st.done:
	}
}

func assistantMessage(text string) agentcore.AssistantMessage {
	return agentcore.AssistantMessage{
		RoleField: "assistant",
		Content:   agentcore.ContentList{agentcore.TextContent{Type: "text", Text: text}},
	}
}

// isConflict reports whether err is the app refusing a second concurrent run.
func isConflict(err error) bool {
	var typed *Error
	if errors.As(err, &typed) {
		return strings.EqualFold(typed.Code, "CONFLICT")
	}
	msg := strings.ToLower(err.Error())
	return strings.Contains(msg, "already") && strings.Contains(msg, "run")
}
