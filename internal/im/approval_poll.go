package im

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"strings"
	"time"
)

const (
	// defaultPermissionPollInterval is a little faster than the desktop carrier's
	// status poll: a confirmation is a blocking question, and a second of lag is
	// much more noticeable there than on a progress card.
	defaultPermissionPollInterval = 700 * time.Millisecond
	// permissionCallTimeout bounds one control-endpoint round trip. A wedged
	// call must not pin the poller forever; the next tick retries.
	permissionCallTimeout = 5 * time.Second
	// minAskWindow is the remaining lifetime below which we do not send a card.
	// By the time a human reads a message the host will have denied the request
	// on its own, so a card would only invite a click that cannot count.
	minAskWindow = 8 * time.Second
	// maxPermissionSummaryRunes bounds the argument preview in a card.
	maxPermissionSummaryRunes = 800
)

// PermissionPollerConfig wires one session's open permission queue to one chat.
type PermissionPollerConfig struct {
	// Gate reads and answers the host's queue.
	Gate PermissionGate
	// Approver is the channel's card-based question surface.
	Approver Approver
	// Chat is where the card goes. It comes from the binding, never from the
	// pending request, so a request cannot redirect its own approval.
	Chat ChatRef
	// SessionID scopes the poll: a chat never sees another session's questions.
	SessionID string
	// Policy is the channel's tool policy. allow/deny answer without asking;
	// confirm asks through Approver.
	Policy ToolPolicy
	// Timeout caps how long a card waits. The host's own remaining lifetime is
	// always the tighter bound.
	Timeout time.Duration
	// Interval overrides the poll cadence (tests use a short one).
	Interval time.Duration
	// Logf receives diagnostics, never message bodies.
	Logf func(format string, args ...any)
}

// PermissionPoller mirrors a host's pending permission requests into a chat
// card for the duration of one run.
//
// It exists because the desktop carrier cannot push its `permissions.request`
// events outside the app process, and the control endpoint is request/response
// only. Polling the host's open set is the same honest trade the carrier already
// makes for progress events, with one important difference: every path out of
// here must end in a decision. A dropped poll, a failed card or a closed app
// leaves the host's own timeout to deny the call, never to allow it.
type PermissionPoller struct {
	cfg PermissionPollerConfig
}

// NewPermissionPoller applies defaults and validates nothing else: a caller
// that forgot an Approver simply gets a poller that logs and denies, which is
// safer than a construction error a caller might ignore.
func NewPermissionPoller(cfg PermissionPollerConfig) *PermissionPoller {
	if cfg.Interval <= 0 {
		cfg.Interval = defaultPermissionPollInterval
	}
	if cfg.Timeout <= 0 {
		cfg.Timeout = 10 * time.Minute
	}
	if cfg.Logf == nil {
		cfg.Logf = func(string, ...any) {}
	}
	if cfg.Policy == "" {
		cfg.Policy = ToolPolicyConfirm
	}
	return &PermissionPoller{cfg: cfg}
}

type pendingAsk struct {
	cancel context.CancelFunc
	token  string
}

// Run polls until ctx is cancelled. It is safe to run in its own goroutine;
// inflight state is owned by this goroutine alone, and Ask goroutines report
// completion through a buffered channel.
func (p *PermissionPoller) Run(ctx context.Context) {
	if p.cfg.Gate == nil || p.cfg.Approver == nil || p.cfg.SessionID == "" {
		return
	}
	cfg := p.cfg
	inflight := make(map[string]*pendingAsk)
	// answered keeps a just-settled id out of the next poll even if the host's
	// queue has not caught up with our Resolve yet. Without it, the gap between
	// "card answered" and "queue no longer lists the request" sends a second
	// card for one question.
	answered := make(map[string]bool)
	resolved := make(chan string, 32)

	defer func() {
		for id, ask := range inflight {
			ask.cancel()
			delete(inflight, id)
		}
	}()

	drain := func() {
		for {
			select {
			case id := <-resolved:
				delete(inflight, id)
				answered[id] = true
			default:
				return
			}
		}
	}

	poll := func() bool {
		pollCtx, cancel := context.WithTimeout(ctx, permissionCallTimeout)
		pending, err := cfg.Gate.Pending(pollCtx, cfg.SessionID)
		cancel()
		if err != nil {
			if errors.Is(err, ErrPermissionUnsupported) {
				cfg.Logf("im: this desktop app has no permissions/pending op; rebuild the app to answer tool confirmations from chat")
				return false
			}
			if ctx.Err() == nil {
				cfg.Logf("im: permission poll session=%s: %v", cfg.SessionID, err)
			}
			return true
		}

		live := make(map[string]bool, len(pending))
		for _, req := range pending {
			if req.RequestID == "" {
				continue
			}
			live[req.RequestID] = true
			if _, asking := inflight[req.RequestID]; asking || answered[req.RequestID] {
				continue
			}

			switch cfg.Policy {
			case ToolPolicyAllow:
				p.resolve(req.RequestID, PermissionAllowOnce)
				continue
			case ToolPolicyDeny:
				p.resolve(req.RequestID, PermissionDeny)
				continue
			}

			// A card that cannot be answered in time is worse than no card:
			// the host has its own timeout and will deny the call anyway.
			if req.Remaining > 0 && req.Remaining < minAskWindow {
				cfg.Logf("im: permission %s expires in %s, leaving it to the host", req.RequestID, req.Remaining)
				continue
			}
			askCtx, askCancel := context.WithCancel(ctx)
			token := newPollToken()
			inflight[req.RequestID] = &pendingAsk{cancel: askCancel, token: token}
			go p.ask(askCtx, req, token, resolved)
		}

		// A request that left the host's queue without us answering it was
		// settled in the app (or expired). Withdraw the card so its buttons do
		// not promise a decision nobody can still make.
		for id, ask := range inflight {
			if !live[id] {
				cfg.Logf("im: permission %s settled outside the channel", id)
				ask.cancel()
				delete(inflight, id)
			}
		}
		for id := range answered {
			if !live[id] {
				delete(answered, id)
			}
		}
		return true
	}

	if !poll() {
		return
	}
	ticker := time.NewTicker(cfg.Interval)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case id := <-resolved:
			delete(inflight, id)
			answered[id] = true
		case <-ticker.C:
			drain()
			if !poll() {
				return
			}
		}
	}
}

// ask sends one card and writes the decision back. It never lets an error, a
// cancellation or a timeout leak into "allowed": the host sees an explicit deny
// in every one of those cases.
func (p *PermissionPoller) ask(ctx context.Context, req PendingPermission, token string, resolved chan<- string) {
	cfg := p.cfg
	defer func() {
		select {
		case resolved <- req.RequestID:
		case <-ctx.Done():
		}
	}()

	timeout := cfg.Timeout
	if req.Remaining > 0 && req.Remaining < timeout {
		timeout = req.Remaining
	}
	// The host started its clock before this poll noticed the request. Leave a
	// small margin so a button pressed in the last second can still count,
	// instead of showing "已允许" for a decision the host already timed out.
	if req.Remaining > 0 && timeout > 2*time.Second {
		timeout -= 2 * time.Second
	}
	dec, err := cfg.Approver.Ask(ctx, ApprovalReq{
		Token:     token,
		Chat:      cfg.Chat,
		SessionID: cfg.SessionID,
		Tool:      req.ToolName,
		Summary:   permissionSummary(req),
		Timeout:   timeout,
	})

	decision := PermissionDeny
	switch {
	case err != nil:
		cfg.Logf("im: approval for %s failed (%v), denying", req.ToolName, err)
	case !dec.Approve:
		cfg.Logf("im: approval for %s denied in chat", req.ToolName)
	case dec.Always:
		decision = PermissionAllowSession
	default:
		decision = PermissionAllowOnce
	}

	// The ask context may already be cancelled (the app answered first). Use a
	// fresh one so the host still receives our explicit decision where it can.
	resolveCtx, cancel := context.WithTimeout(context.Background(), permissionCallTimeout)
	defer cancel()
	if err := cfg.Gate.Resolve(resolveCtx, req.RequestID, decision); err != nil {
		if errors.Is(err, ErrPermissionGone) {
			return
		}
		cfg.Logf("im: resolve permission %s: %v", req.RequestID, err)
	}
}

func (p *PermissionPoller) resolve(requestID string, decision PermissionDecision) {
	ctx, cancel := context.WithTimeout(context.Background(), permissionCallTimeout)
	defer cancel()
	if err := p.cfg.Gate.Resolve(ctx, requestID, decision); err != nil {
		if errors.Is(err, ErrPermissionGone) {
			return
		}
		p.cfg.Logf("im: auto-resolve permission %s as %s: %v", requestID, decision, err)
	}
}

// permissionSummary keeps the card informative but bounded. The host already
// truncates long string leaves; this is the second limit, for the card itself.
func permissionSummary(req PendingPermission) string {
	var b strings.Builder
	if s := strings.TrimSpace(req.Reason); s != "" {
		b.WriteString(s)
	}
	if s := strings.TrimSpace(req.Summary); s != "" {
		runes := []rune(s)
		if len(runes) > maxPermissionSummaryRunes {
			s = string(runes[:maxPermissionSummaryRunes]) + "…"
		}
		if b.Len() > 0 {
			b.WriteString("\n")
		}
		fmt.Fprintf(&b, "```\n%s\n```", s)
	}
	return b.String()
}

// newPollToken mints the correlation value a card button echoes back. It is
// separate from ApprovalRouter's tokens: a stale card from one path must never
// be able to settle a question opened by the other.
func newPollToken() string {
	var buf [8]byte
	if _, err := rand.Read(buf[:]); err == nil {
		return "poll-" + hex.EncodeToString(buf[:])
	}
	return fmt.Sprintf("poll-%d", time.Now().UnixNano())
}
