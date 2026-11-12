package im

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/runtime"
	"github.com/smallnest/pigo/internal/serve"
	"github.com/smallnest/pigo/internal/session"
	"github.com/smallnest/pigo/internal/trust"
)

// ErrNoApprover reports that the channel cannot ask the human anything, so a
// confirmation-required tool cannot run.
var ErrNoApprover = errors.New("im: channel cannot ask for approval")

// ToolPolicy decides what happens when the agent wants a side-effect tool.
type ToolPolicy string

const (
	// ToolPolicyConfirm asks the human through the channel. This is the
	// default: an agent reachable from a phone must not be able to run bash
	// without a human saying yes.
	ToolPolicyConfirm ToolPolicy = "confirm"
	// ToolPolicyAllow runs side-effect tools without asking. Only sensible for
	// a single-owner deployment where the chat is as trusted as the terminal.
	ToolPolicyAllow ToolPolicy = "allow"
	// ToolPolicyDeny blocks every side-effect tool, leaving read-only analysis.
	ToolPolicyDeny ToolPolicy = "deny"
)

// ApprovalRouter routes a side-effect tool confirmation to the chat that is
// driving the session.
//
// It exists because of an impedance mismatch: the tool hook only knows the
// session id (runtime.RunConfig is built once per run, before any message
// arrives), while the approval has to reach a specific chat. This type is the
// join table between the two, and WrapRunConfig is where it is spliced in.
type ApprovalRouter struct {
	policy  ToolPolicy
	timeout time.Duration

	mu        sync.Mutex
	bySession map[string]approvalTarget
	seq       uint64
	logf      func(string, ...any)
}

type approvalTarget struct {
	approver Approver
	chat     ChatRef
}

// NewApprovalRouter builds an approval router. policy "" defaults to confirm;
// timeout <= 0 defaults to 10 minutes.
func NewApprovalRouter(policy ToolPolicy, timeout time.Duration, logf func(string, ...any)) *ApprovalRouter {
	switch policy {
	case ToolPolicyAllow, ToolPolicyDeny, ToolPolicyConfirm:
	default:
		policy = ToolPolicyConfirm
	}
	if timeout <= 0 {
		timeout = 10 * time.Minute
	}
	if logf == nil {
		logf = func(string, ...any) {}
	}
	return &ApprovalRouter{
		policy:    policy,
		timeout:   timeout,
		bySession: make(map[string]approvalTarget),
		logf:      logf,
	}
}

// Register records which chat is driving a session. Called when a chat binds to
// a session and on every prompt (the chat is authoritative, not the session).
func (a *ApprovalRouter) Register(sessionID string, chat ChatRef, approver Approver) {
	if a == nil || sessionID == "" {
		return
	}
	a.mu.Lock()
	a.bySession[sessionID] = approvalTarget{approver: approver, chat: chat}
	a.mu.Unlock()
}

// Unregister forgets a session. Called when a chat rebinds elsewhere.
func (a *ApprovalRouter) Unregister(sessionID string) {
	if a == nil || sessionID == "" {
		return
	}
	a.mu.Lock()
	delete(a.bySession, sessionID)
	a.mu.Unlock()
}

func (a *ApprovalRouter) lookup(sessionID string) (approvalTarget, bool) {
	if a == nil {
		return approvalTarget{}, false
	}
	a.mu.Lock()
	defer a.mu.Unlock()
	t, ok := a.bySession[sessionID]
	return t, ok
}

// Policy reports the channel's tool policy. The desktop carrier needs it to
// decide whether a pending permission should become a card, an automatic
// allow-once, or an automatic deny.
func (a *ApprovalRouter) Policy() ToolPolicy {
	if a == nil {
		return ToolPolicyConfirm
	}
	return a.policy
}

// Timeout caps how long an approval card may wait for a human.
func (a *ApprovalRouter) Timeout() time.Duration {
	if a == nil {
		return 10 * time.Minute
	}
	return a.timeout
}

// Target returns the chat and question surface a session is bound to. Keeping
// this lookup here (instead of passing the chat around) means a permission
// request can never choose where its own card is sent.
func (a *ApprovalRouter) Target(sessionID string) (Approver, ChatRef, bool) {
	t, ok := a.lookup(sessionID)
	if !ok || t.approver == nil {
		return nil, ChatRef{}, false
	}
	return t.approver, t.chat, true
}

func (a *ApprovalRouter) nextToken() string {
	var buf [8]byte
	if _, err := rand.Read(buf[:]); err != nil {
		// A predictable token would let one card's button resolve another
		// request; fall back to a counter-based value rather than silently
		// reusing randomness that crypto/rand refused to give us.
		a.mu.Lock()
		a.seq++
		n := a.seq
		a.mu.Unlock()
		return fmt.Sprintf("t%d", n)
	}
	return hex.EncodeToString(buf[:])
}

// WrapRunConfig decorates the host's RunConfigFunc so runs driven from an IM
// chat get their side-effect confirmations routed to that chat.
//
// A session with no registered chat is left untouched, which is what makes this
// safe to install unconditionally: the CLI and desktop paths keep their own
// trust behaviour, and only IM sessions gain the extra gate.
func (a *ApprovalRouter) WrapRunConfig(inner serve.RunConfigFunc) serve.RunConfigFunc {
	if a == nil {
		return inner
	}
	return func(h session.SessionHeader, thinking agentcore.ThinkingLevel) runtime.RunConfig {
		cfg := inner(h, thinking)
		if _, ok := a.lookup(h.ID); !ok {
			return cfg
		}
		prev := cfg.Batch.BeforeToolCall
		cfg.Batch.BeforeToolCall = a.before(h.ID, prev)
		return cfg
	}
}

// before is the installed hook. The host hook runs first: if it already decided
// (allowed, blocked, or rewrote the input) that decision stands, because the
// host knows about trust directories and tool boundaries that IM does not.
func (a *ApprovalRouter) before(sessionID string, prev agentcore.BeforeToolCallFunc) agentcore.BeforeToolCallFunc {
	return func(ctx context.Context, call agentcore.AgentToolCall) *agentcore.BeforeToolCallDecision {
		if prev != nil {
			if d := prev(ctx, call); d != nil {
				return d
			}
		}
		if !trust.IsSideEffect(call.Tool, call.Name) {
			return nil
		}
		switch a.policy {
		case ToolPolicyAllow:
			return nil
		case ToolPolicyDeny:
			return blockDecision("IM 通道当前为只读模式（tool_policy = deny），已阻止该操作。")
		}

		target, ok := a.lookup(sessionID)
		if !ok || target.approver == nil {
			return blockDecision("当前通道无法向人确认，已阻止该操作（fail-closed）。")
		}
		dec, err := target.approver.Ask(ctx, ApprovalReq{
			Token:     a.nextToken(),
			Chat:      target.chat,
			SessionID: sessionID,
			Tool:      call.Name,
			Summary:   trust.ToolCallSummary(call),
			Timeout:   a.timeout,
		})
		if err != nil {
			a.logf("im: approval for %s failed: %v", call.Name, err)
			return blockDecision(fmt.Sprintf("审批请求失败（%v），已按拒绝处理。", err))
		}
		if !dec.Approve {
			return blockDecision("用户拒绝了该操作。请换一个不需要它的方案，或向用户说明需要什么权限。")
		}
		return nil
	}
}

func blockDecision(msg string) *agentcore.BeforeToolCallDecision {
	return &agentcore.BeforeToolCallDecision{
		Block:   true,
		Content: &agentcore.ContentList{agentcore.NewTextContent(msg)},
	}
}

// ApprovalSummaryLines renders the body of an approval card. Kept here (not in
// the adapter) so every channel phrases the question identically and the
// filename/command preview is redacted the same way.
func ApprovalSummaryLines(req ApprovalReq) string {
	var b strings.Builder
	b.WriteString("即将执行需要确认的操作：\n\n")
	fmt.Fprintf(&b, "**工具** `%s`\n", req.Tool)
	if s := strings.TrimSpace(req.Summary); s != "" {
		fmt.Fprintf(&b, "**内容** %s\n", s)
	}
	b.WriteString("\n同意后本次执行；选择「本会话允许」将不再询问同类操作。")
	return b.String()
}
