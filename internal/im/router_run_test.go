package im

import (
	"context"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
)

// fakeStream feeds events the way a carrier would: the run ends by closing the
// channel, exactly like serve's subscription does.
type fakeStream struct {
	ch chan agentcore.AgentEvent
}

func (s *fakeStream) Events() <-chan agentcore.AgentEvent { return s.ch }

// Close is a no-op: the producer owns the channel, and the router calls Close on
// a path where the producer may already have finished.
func (s *fakeStream) Close() {}

// fakeSession is one carrier-agnostic session. It proves the router no longer
// needs serve: a host that can prompt and stream is a host the channel can drive.
type fakeSession struct {
	id   string
	mu   sync.Mutex
	sent []string
	str  *fakeStream
	// reply is what the "agent" says, so the final card is assertable.
	reply string
	// busy, when true, makes Prompt answer ErrRunActive once.
	busy bool
}

func (s *fakeSession) ID() string { return s.id }

func (s *fakeSession) Busy() bool { return false }

func (s *fakeSession) Prompt(_ context.Context, text string) error {
	s.mu.Lock()
	s.sent = append(s.sent, text)
	busy := s.busy
	s.busy = false
	stream, reply := s.str, s.reply
	s.mu.Unlock()
	if busy {
		return ErrRunActive
	}
	go func() {
		stream.ch <- agentcore.ToolExecutionStartEvent{ToolName: "bash"}
		stream.ch <- agentcore.AgentEndEvent{Messages: []agentcore.AgentMessage{assistant(reply)}}
		close(stream.ch)
	}()
	return nil
}

func (s *fakeSession) Steer(text string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.sent = append(s.sent, "steer:"+text)
	return nil
}

func (s *fakeSession) Abort() {}

func (s *fakeSession) SetModel(string) error { return nil }

func (s *fakeSession) Subscribe() (EventStream, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.str = &fakeStream{ch: make(chan agentcore.AgentEvent, 8)}
	return s.str, nil
}

func (s *fakeSession) Close() error { return nil }

func (s *fakeSession) prompts() []string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return append([]string(nil), s.sent...)
}

// fakeHost hands out one session and records what the router asked for.
type fakeHost struct {
	session *fakeSession
}

func (h *fakeHost) CreateSession(context.Context, SessionSpec) (AgentSession, error) {
	return h.session, nil
}

func (h *fakeHost) OpenSession(context.Context, string) (AgentSession, error) {
	return h.session, nil
}

func (h *fakeHost) ListSessions(context.Context) ([]SessionSummary, error) { return nil, nil }

func assistant(text string) agentcore.AssistantMessage {
	return agentcore.AssistantMessage{
		RoleField: "assistant",
		Content:   agentcore.ContentList{agentcore.TextContent{Type: "text", Text: text}},
	}
}

// TestRouterDrivesAnyHost is the regression test for the carrier split: the run
// path must work against a plain AgentSession, not just serve's runtime.
func TestRouterDrivesAnyHost(t *testing.T) {
	store, err := NewBindingStore("")
	if err != nil {
		t.Fatalf("NewBindingStore: %v", err)
	}
	sess := &fakeSession{id: "s1", reply: "查到了：配置错在第 3 行"}
	host := &fakeHost{session: sess}
	adapter := &recordingAdapter{}
	r := NewRouter(RouterConfig{AllowedUsers: []string{"u1"}, Logf: t.Logf}, host, store, adapter, nil)

	r.accept(textInbound("c1", "u1", "m1", "看看构建为什么失败"))
	waitFor(t, func() bool {
		for _, msg := range adapter.snapshot() {
			if msg.Final && strings.Contains(msg.Text, "配置错在第 3 行") {
				return true
			}
		}
		return false
	})

	if got := sess.prompts(); len(got) != 1 || got[0] != "看看构建为什么失败" {
		t.Fatalf("prompt not delivered: %v", got)
	}
	binding, ok := store.Get(ChatRef{ID: "c1", Kind: ChatP2P}.Key())
	if !ok || binding.SessionID != "s1" {
		t.Fatalf("binding not persisted: %+v ok=%v", binding, ok)
	}
}

// TestRouterSteersWhenHostIsBusy keeps the "two quick messages" behaviour on the
// new abstraction: a busy session turns the second message into a steer.
func TestRouterSteersWhenHostIsBusy(t *testing.T) {
	store, err := NewBindingStore("")
	if err != nil {
		t.Fatalf("NewBindingStore: %v", err)
	}
	sess := &fakeSession{id: "s1", reply: "ok", busy: true}
	r := NewRouter(RouterConfig{AllowedUsers: []string{"u1"}, Logf: t.Logf}, &fakeHost{session: sess}, store, &recordingAdapter{}, nil)

	r.accept(textInbound("c1", "u1", "m1", "补充一句"))
	waitFor(t, func() bool {
		for _, p := range sess.prompts() {
			if p == "steer:补充一句" {
				return true
			}
		}
		return false
	})
}

// TestRouterNewRebindsWithoutClosingSession pins the desktop-mode contract:
// /new must drop the binding, not the user's conversation.
func TestRouterNewRebindsWithoutClosingSession(t *testing.T) {
	store, err := NewBindingStore("")
	if err != nil {
		t.Fatalf("NewBindingStore: %v", err)
	}
	sess := &fakeSession{id: "s1", reply: "ok"}
	adapter := &recordingAdapter{}
	r := NewRouter(RouterConfig{AllowedUsers: []string{"u1"}, Logf: t.Logf}, &fakeHost{session: sess}, store, adapter, nil)

	r.accept(textInbound("c1", "u1", "m1", "hi"))
	waitFor(t, func() bool { _, ok := store.Get(ChatRef{ID: "c1", Kind: ChatP2P}.Key()); return ok })

	r.accept(textInbound("c1", "u1", "m2", "/new"))
	waitFor(t, func() bool { _, ok := store.Get(ChatRef{ID: "c1", Kind: ChatP2P}.Key()); return !ok })
	time.Sleep(20 * time.Millisecond)
}
