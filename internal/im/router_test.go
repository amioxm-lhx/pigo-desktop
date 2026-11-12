package im

import (
	"context"
	"errors"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
)

// recordingAdapter is an Adapter that records what the router sent.
type recordingAdapter struct {
	mu    sync.Mutex
	sends []Outbound
	chats []ChatRef
}

func (a *recordingAdapter) ID() string { return "test" }

func (a *recordingAdapter) Run(context.Context, chan<- Inbound) error { return nil }

func (a *recordingAdapter) Send(_ context.Context, chat ChatRef, msg Outbound) (string, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.chats = append(a.chats, chat)
	a.sends = append(a.sends, msg)
	return "m" + string(rune('0'+len(a.sends))), nil
}

func (a *recordingAdapter) Update(context.Context, ChatRef, string, Outbound) error {
	return ErrUpdateUnsupported
}

func (a *recordingAdapter) Close() error { return nil }

func (a *recordingAdapter) snapshot() []Outbound {
	a.mu.Lock()
	defer a.mu.Unlock()
	out := make([]Outbound, len(a.sends))
	copy(out, a.sends)
	return out
}

// stubHost satisfies SessionHost; the router tests exercise command handling
// and gating, never a real session.
type stubHost struct{}

func (stubHost) ListSessions(context.Context) ([]SessionSummary, error) { return nil, nil }

func (stubHost) CreateSession(context.Context, SessionSpec) (AgentSession, error) {
	return nil, errors.New("not used")
}

func (stubHost) OpenSession(context.Context, string) (AgentSession, error) {
	return nil, errors.New("not used")
}

func newTestRouter(t *testing.T, cfg RouterConfig) (*Router, *recordingAdapter) {
	t.Helper()
	store, err := NewBindingStore("")
	if err != nil {
		t.Fatalf("NewBindingStore: %v", err)
	}
	adapter := &recordingAdapter{}
	if cfg.AllowedUsers == nil {
		cfg.AllowedUsers = []string{"u1"}
	}
	if cfg.Logf == nil {
		cfg.Logf = t.Logf
	}
	return NewRouter(cfg, stubHost{}, store, adapter, nil), adapter
}

func textInbound(chat, sender, msgID, text string) Inbound {
	return Inbound{
		Adapter: "test",
		Chat:    ChatRef{ID: chat, Kind: ChatP2P},
		Sender:  UserRef{ID: sender},
		MsgID:   msgID,
		Text:    text,
		Kind:    KindText,
	}
}

// TestRouterDropsUnauthorizedSender is the security-critical default: a message
// from anyone not on the allow list produces no reply at all, so a stranger
// cannot use the bot as an oracle to confirm it exists.
func TestRouterDropsUnauthorizedSender(t *testing.T) {
	r, adapter := newTestRouter(t, RouterConfig{})
	r.accept(textInbound("c1", "intruder", "m1", "/help"))

	time.Sleep(50 * time.Millisecond)
	if got := adapter.snapshot(); len(got) != 0 {
		t.Fatalf("sent %d messages to an unauthorized sender, want 0", len(got))
	}
}

// TestRouterEmptyAllowListDeniesAll pins the fail-closed default: an empty allow
// list must not be read as "everyone".
func TestRouterEmptyAllowListDeniesAll(t *testing.T) {
	r, adapter := newTestRouter(t, RouterConfig{AllowedUsers: []string{}})
	// newTestRouter fills a default when nil; force the empty case explicitly.
	r.cfg.AllowedUsers = nil
	r.accept(textInbound("c1", "u1", "m1", "/help"))

	time.Sleep(50 * time.Millisecond)
	if got := adapter.snapshot(); len(got) != 0 {
		t.Fatalf("sent %d messages with an empty allow list, want 0", len(got))
	}
}

// TestRouterGroupNeedsMentionAndOptIn covers both group gates.
func TestRouterGroupNeedsMentionAndOptIn(t *testing.T) {
	cases := []struct {
		name      string
		allow     bool
		mentioned bool
		wantSend  int
	}{
		{"groups disabled", false, true, 0},
		{"group without mention", true, false, 0},
		{"group with mention", true, true, 1},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			r, adapter := newTestRouter(t, RouterConfig{AllowGroups: tc.allow, RequireMention: true})
			in := textInbound("g1", "u1", "m1", "/help")
			in.Chat.Kind = ChatGroup
			in.Mentioned = tc.mentioned
			r.accept(in)

			waitFor(t, func() bool { return len(adapter.snapshot()) >= tc.wantSend })
			time.Sleep(20 * time.Millisecond)
			if got := len(adapter.snapshot()); got != tc.wantSend {
				t.Fatalf("sent %d, want %d", got, tc.wantSend)
			}
		})
	}
}

// TestRouterDropsReplayedMessage verifies the idempotency key: platforms retry,
// and a retried message must not start a second run.
func TestRouterDropsReplayedMessage(t *testing.T) {
	r, adapter := newTestRouter(t, RouterConfig{})
	in := textInbound("c1", "u1", "same-id", "/help")
	r.accept(in)
	r.accept(in)

	waitFor(t, func() bool { return len(adapter.snapshot()) >= 1 })
	time.Sleep(50 * time.Millisecond)
	if got := len(adapter.snapshot()); got != 1 {
		t.Fatalf("replayed message produced %d replies, want 1", got)
	}
}

// TestRouterRejectReplyIsThrottled is the user-facing half of the queue policy:
// a burst of rejected messages yields exactly one "try later" reply, worded as
// configured.
func TestRouterRejectReplyIsThrottled(t *testing.T) {
	r, adapter := newTestRouter(t, RouterConfig{RejectCooldown: time.Minute})
	in := textInbound("c1", "u1", "m1", "hello")

	for i := 0; i < 5; i++ {
		r.handleRejected(in, DefaultQueueDepth)
	}
	waitFor(t, func() bool { return len(adapter.snapshot()) >= 1 })
	time.Sleep(50 * time.Millisecond)

	got := adapter.snapshot()
	if len(got) != 1 {
		t.Fatalf("rejection replies = %d, want 1 (throttled)", len(got))
	}
	if got[0].Text != DefaultRejectReply {
		t.Fatalf("rejection text = %q, want %q", got[0].Text, DefaultRejectReply)
	}
	if got[0].Tone != ToneWarn {
		t.Fatalf("rejection tone = %q, want %q", got[0].Tone, ToneWarn)
	}
}

// TestRouterRejectReplyHonoursCustomText checks the configurable wording.
func TestRouterRejectReplyHonoursCustomText(t *testing.T) {
	const custom = "队列已满，请稍后。"
	r, adapter := newTestRouter(t, RouterConfig{RejectReply: custom})
	r.handleRejected(textInbound("c1", "u1", "m1", "hi"), 3)

	waitFor(t, func() bool { return len(adapter.snapshot()) >= 1 })
	if got := adapter.snapshot()[0].Text; got != custom {
		t.Fatalf("text = %q, want %q", got, custom)
	}
}

// TestRouterRejectHonoursCooldownWindow checks that the throttle expires, so a
// chat is not permanently muted after one burst.
func TestRouterRejectHonoursCooldownWindow(t *testing.T) {
	now := time.Date(2026, 11, 12, 10, 0, 0, 0, time.UTC)
	r, adapter := newTestRouter(t, RouterConfig{
		RejectCooldown: 10 * time.Second,
		Now:            func() time.Time { return now },
	})
	in := textInbound("c1", "u1", "m1", "hi")

	r.handleRejected(in, 3)
	now = now.Add(11 * time.Second)
	r.handleRejected(in, 3)

	waitFor(t, func() bool { return len(adapter.snapshot()) >= 2 })
	if got := len(adapter.snapshot()); got != 2 {
		t.Fatalf("replies = %d, want 2 (one per window)", got)
	}
}

// TestRouterHelpCommand verifies the command path runs without touching the
// agent, which is what makes it usable while a run is in flight.
func TestRouterHelpCommand(t *testing.T) {
	r, adapter := newTestRouter(t, RouterConfig{})
	r.accept(textInbound("c1", "u1", "m1", "@_user_1 /help"))

	waitFor(t, func() bool { return len(adapter.snapshot()) >= 1 })
	got := adapter.snapshot()[0]
	if !strings.Contains(got.Text, "/new") {
		t.Fatalf("help text missing /new: %q", got.Text)
	}
}

// TestRouterStatusCommand checks that /status reports the per-chat quota, since
// that is the number the rejection message refers to.
func TestRouterStatusCommand(t *testing.T) {
	r, adapter := newTestRouter(t, RouterConfig{QueueDepth: 2})
	r.accept(textInbound("c1", "u1", "m1", "/status"))

	waitFor(t, func() bool { return len(adapter.snapshot()) >= 1 })
	if txt := adapter.snapshot()[0].Text; !strings.Contains(txt, "上限 2") {
		t.Fatalf("status text missing queue depth: %q", txt)
	}
}

// TestApprovalRouterFailsClosed proves that a session with no registered chat
// cannot run a side-effect tool: silence must not mean consent.
func TestApprovalRouterFailsClosed(t *testing.T) {
	ar := NewApprovalRouter(ToolPolicyConfirm, time.Minute, t.Logf)
	hook := ar.before("unregistered-session", nil)

	decision := hook(context.Background(), agentcore.AgentToolCall{Name: "bash", Arguments: []byte(`{"command":"rm -rf /"}`)})
	if decision == nil || !decision.Block {
		t.Fatalf("decision = %+v, want Block=true", decision)
	}
}

// TestApprovalRouterDenyPolicy covers tool_policy=deny.
func TestApprovalRouterDenyPolicy(t *testing.T) {
	ar := NewApprovalRouter(ToolPolicyDeny, time.Minute, t.Logf)
	ar.Register("s1", ChatRef{ID: "c1"}, &fakeApprover{})
	hook := ar.before("s1", nil)

	decision := hook(context.Background(), agentcore.AgentToolCall{Name: "bash"})
	if decision == nil || !decision.Block {
		t.Fatalf("decision = %+v, want Block=true", decision)
	}
}

// TestApprovalRouterAllowPolicy covers tool_policy=allow.
func TestApprovalRouterAllowPolicy(t *testing.T) {
	ar := NewApprovalRouter(ToolPolicyAllow, time.Minute, t.Logf)
	ar.Register("s1", ChatRef{ID: "c1"}, &fakeApprover{})
	hook := ar.before("s1", nil)

	if decision := hook(context.Background(), agentcore.AgentToolCall{Name: "bash"}); decision != nil {
		t.Fatalf("decision = %+v, want nil (allowed)", decision)
	}
}

// TestApprovalRouterAsksRegisteredChat checks the happy path.
func TestApprovalRouterAsksRegisteredChat(t *testing.T) {
	fa := &fakeApprover{decision: Decision{Approve: true}}
	ar := NewApprovalRouter(ToolPolicyConfirm, time.Minute, t.Logf)
	ar.Register("s1", ChatRef{ID: "c1"}, fa)
	hook := ar.before("s1", nil)

	if decision := hook(context.Background(), agentcore.AgentToolCall{Name: "bash"}); decision != nil {
		t.Fatalf("decision = %+v, want nil (approved)", decision)
	}
	if fa.asked != 1 {
		t.Fatalf("approver asked %d times, want 1", fa.asked)
	}
	if fa.last.Token == "" {
		t.Fatal("approval request carried no token")
	}
}

// TestApprovalRouterHonoursHostHook checks that the host's own gate wins: trust
// directories and tool boundaries are the host's knowledge, not IM's.
func TestApprovalRouterHonoursHostHook(t *testing.T) {
	fa := &fakeApprover{decision: Decision{Approve: true}}
	ar := NewApprovalRouter(ToolPolicyConfirm, time.Minute, t.Logf)
	ar.Register("s1", ChatRef{ID: "c1"}, fa)

	prev := func(context.Context, agentcore.AgentToolCall) *agentcore.BeforeToolCallDecision {
		return &agentcore.BeforeToolCallDecision{Block: true, Content: &agentcore.ContentList{agentcore.NewTextContent("host says no")}}
	}
	hook := ar.before("s1", prev)
	if decision := hook(context.Background(), agentcore.AgentToolCall{Name: "bash"}); decision == nil || !decision.Block {
		t.Fatalf("decision = %+v, want host's Block=true", decision)
	}
	if fa.asked != 0 {
		t.Fatalf("approver asked %d times, want 0 (host already decided)", fa.asked)
	}
}

type fakeApprover struct {
	decision Decision
	err      error
	asked    int
	last     ApprovalReq
}

func (f *fakeApprover) Ask(_ context.Context, req ApprovalReq) (Decision, error) {
	f.asked++
	f.last = req
	return f.decision, f.err
}
