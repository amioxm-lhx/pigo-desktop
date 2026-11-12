package im

import (
	"context"
	"errors"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

type resolvedCall struct {
	requestID string
	decision  PermissionDecision
}

// fakeGate stands in for the desktop control endpoint's permission queue. It
// keeps the queue after a Resolve by default, which is stricter than the real
// host: a poller that relies on the host clearing the list would double-ask.
type fakeGate struct {
	mu           sync.Mutex
	pending      []PendingPermission
	calls        []resolvedCall
	gone         bool
	unsupported  bool
	pendingCalls atomic.Int32
}

func (g *fakeGate) Pending(_ context.Context, sessionID string) ([]PendingPermission, error) {
	g.pendingCalls.Add(1)
	g.mu.Lock()
	defer g.mu.Unlock()
	if g.unsupported {
		return nil, ErrPermissionUnsupported
	}
	out := make([]PendingPermission, 0, len(g.pending))
	for _, p := range g.pending {
		if p.SessionID == sessionID {
			out = append(out, p)
		}
	}
	return out, nil
}

func (g *fakeGate) Resolve(_ context.Context, requestID string, decision PermissionDecision) error {
	g.mu.Lock()
	defer g.mu.Unlock()
	g.calls = append(g.calls, resolvedCall{requestID, decision})
	if g.gone {
		return ErrPermissionGone
	}
	return nil
}

func (g *fakeGate) set(pending ...PendingPermission) {
	g.mu.Lock()
	defer g.mu.Unlock()
	g.pending = pending
}

func (g *fakeGate) snapshot() []resolvedCall {
	g.mu.Lock()
	defer g.mu.Unlock()
	return append([]resolvedCall(nil), g.calls...)
}

type askFunc func(context.Context, ApprovalReq) (Decision, error)

type funcApprover struct {
	fn    askFunc
	calls atomic.Int32
}

func (a *funcApprover) Ask(ctx context.Context, req ApprovalReq) (Decision, error) {
	a.calls.Add(1)
	return a.fn(ctx, req)
}

func openRequest(id string) PendingPermission {
	return PendingPermission{
		RequestID: id,
		SessionID: "s1",
		ToolName:  "Edit",
		Reason:    "High-risk tool requires approval",
		Summary:   `{"file_path":"internal/im/router.go"}`,
		Remaining: 90 * time.Second,
	}
}

func startPoller(gate *fakeGate, approver Approver, policy ToolPolicy) context.CancelFunc {
	p := NewPermissionPoller(PermissionPollerConfig{
		Gate:      gate,
		Approver:  approver,
		Chat:      ChatRef{ID: "oc_chat", Kind: ChatP2P},
		SessionID: "s1",
		Policy:    policy,
		Timeout:   2 * time.Minute,
		Interval:  5 * time.Millisecond,
	})
	ctx, cancel := context.WithCancel(context.Background())
	go p.Run(ctx)
	return cancel
}

func waitForPermission(t *testing.T, cond func() bool) {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if cond() {
			return
		}
		time.Sleep(2 * time.Millisecond)
	}
	t.Fatal("condition not met within 2s")
}

func TestPermissionPollerAsksOnceAndResolves(t *testing.T) {
	gate := &fakeGate{}
	gate.set(openRequest("r1"))
	asked := make(chan ApprovalReq, 4)
	approver := &funcApprover{fn: func(_ context.Context, req ApprovalReq) (Decision, error) {
		asked <- req
		return Decision{Approve: true}, nil
	}}
	cancel := startPoller(gate, approver, ToolPolicyConfirm)
	defer cancel()

	var first ApprovalReq
	select {
	case first = <-asked:
	case <-time.After(2 * time.Second):
		t.Fatal("no approval card was sent")
	}
	if first.Chat.ID != "oc_chat" || first.SessionID != "s1" || first.Tool != "Edit" {
		t.Fatalf("bad approval request: %+v", first)
	}
	if first.Timeout > 90*time.Second {
		t.Fatalf("card timeout %s exceeds host remaining lifetime", first.Timeout)
	}
	if first.Summary == "" || first.Token == "" {
		t.Fatalf("card lacks a preview or correlation token: %+v", first)
	}

	waitForPermission(t, func() bool {
		for _, c := range gate.snapshot() {
			if c.requestID == "r1" && c.decision == PermissionAllowOnce {
				return true
			}
		}
		return false
	})
	// The fake host keeps the request queued on purpose. A second card here
	// would mean the poller forgot it had already answered.
	time.Sleep(40 * time.Millisecond)
	if got := approver.calls.Load(); got != 1 {
		t.Fatalf("asked %d times for one request", got)
	}
}

func TestPermissionPollerDecisionMapping(t *testing.T) {
	cases := []struct {
		name string
		dec  Decision
		err  error
		want PermissionDecision
	}{
		{"allow once", Decision{Approve: true}, nil, PermissionAllowOnce},
		{"allow session", Decision{Approve: true, Always: true}, nil, PermissionAllowSession},
		{"denied by human", Decision{}, nil, PermissionDeny},
		{"card failed", Decision{}, errors.New("send failed"), PermissionDeny},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			gate := &fakeGate{}
			gate.set(openRequest("r1"))
			approver := &funcApprover{fn: func(context.Context, ApprovalReq) (Decision, error) {
				return tc.dec, tc.err
			}}
			cancel := startPoller(gate, approver, ToolPolicyConfirm)
			defer cancel()
			waitForPermission(t, func() bool {
				for _, c := range gate.snapshot() {
					if c.requestID == "r1" {
						if c.decision != tc.want {
							t.Fatalf("decision = %s, want %s", c.decision, tc.want)
						}
						return true
					}
				}
				return false
			})
		})
	}
}

func TestPermissionPollerPoliciesSkipTheCard(t *testing.T) {
	for _, tc := range []struct {
		policy ToolPolicy
		want   PermissionDecision
	}{
		{ToolPolicyAllow, PermissionAllowOnce},
		{ToolPolicyDeny, PermissionDeny},
	} {
		t.Run(string(tc.policy), func(t *testing.T) {
			gate := &fakeGate{}
			gate.set(openRequest("r1"))
			approver := &funcApprover{fn: func(context.Context, ApprovalReq) (Decision, error) {
				t.Error("policy must not ask through the channel")
				return Decision{Approve: true}, nil
			}}
			cancel := startPoller(gate, approver, tc.policy)
			defer cancel()
			waitForPermission(t, func() bool {
				for _, c := range gate.snapshot() {
					if c.requestID == "r1" {
						if c.decision != tc.want {
							t.Fatalf("decision = %s, want %s", c.decision, tc.want)
						}
						return true
					}
				}
				return false
			})
			if got := approver.calls.Load(); got != 0 {
				t.Fatalf("asked %d times under %s", got, tc.policy)
			}
		})
	}
}

func TestPermissionPollerWithdrawsCardSettledElsewhere(t *testing.T) {
	gate := &fakeGate{gone: true}
	gate.set(openRequest("r1"))
	asking := make(chan struct{}, 1)
	withdrawn := make(chan struct{}, 1)
	approver := &funcApprover{fn: func(ctx context.Context, _ ApprovalReq) (Decision, error) {
		asking <- struct{}{}
		<-ctx.Done()
		withdrawn <- struct{}{}
		return Decision{}, ctx.Err()
	}}
	cancel := startPoller(gate, approver, ToolPolicyConfirm)
	defer cancel()

	select {
	case <-asking:
	case <-time.After(2 * time.Second):
		t.Fatal("no card was sent")
	}
	// The window answered first: the host drops the request from its queue.
	gate.set()
	select {
	case <-withdrawn:
	case <-time.After(2 * time.Second):
		t.Fatal("card was not withdrawn after the request left the host queue")
	}
}

func TestPermissionPollerStopsOnUnsupportedHost(t *testing.T) {
	// An old desktop app answers permissions/pending with NOT_ALLOWED. Retrying
	// it every 700ms would fill the channel log without ever sending a card.
	gate := &fakeGate{unsupported: true}
	approver := &funcApprover{fn: func(context.Context, ApprovalReq) (Decision, error) {
		t.Error("no card can be sent without a readable queue")
		return Decision{Approve: true}, nil
	}}
	cancel := startPoller(gate, approver, ToolPolicyConfirm)
	defer cancel()
	time.Sleep(80 * time.Millisecond)
	if got := gate.pendingCalls.Load(); got != 1 {
		t.Fatalf("polled %d times against an unsupported host", got)
	}
}

func TestPermissionPollerSkipsExpiringRequest(t *testing.T) {
	gate := &fakeGate{}
	expiring := openRequest("r1")
	expiring.Remaining = 3 * time.Second
	gate.set(expiring)
	approver := &funcApprover{fn: func(context.Context, ApprovalReq) (Decision, error) {
		t.Error("a request about to expire must not produce a card")
		return Decision{Approve: true}, nil
	}}
	cancel := startPoller(gate, approver, ToolPolicyConfirm)
	defer cancel()
	time.Sleep(60 * time.Millisecond)
	if got := approver.calls.Load(); got != 0 {
		t.Fatalf("asked %d times for an expiring request", got)
	}
	if got := gate.snapshot(); len(got) != 0 {
		t.Fatalf("expiring request was resolved: %+v", got)
	}
}

// ---- Router wiring ----

type gateHost struct {
	*fakeGate
}

func (h gateHost) CreateSession(context.Context, SessionSpec) (AgentSession, error) {
	return nil, errors.New("not used")
}
func (h gateHost) OpenSession(context.Context, string) (AgentSession, error) {
	return nil, errors.New("not used")
}
func (h gateHost) ListSessions(context.Context) ([]SessionSummary, error) { return nil, nil }

type approvingAdapter struct {
	*funcApprover
}

func (a approvingAdapter) ID() string                                { return "test" }
func (a approvingAdapter) Run(context.Context, chan<- Inbound) error { return nil }
func (a approvingAdapter) Send(context.Context, ChatRef, Outbound) (string, error) {
	return "", nil
}
func (a approvingAdapter) Update(context.Context, ChatRef, string, Outbound) error { return nil }
func (a approvingAdapter) Close() error                                            { return nil }

func TestRouterStartsPollerForHostsWithAPermissionGate(t *testing.T) {
	gate := &fakeGate{}
	gate.set(openRequest("r1"))
	approver := &funcApprover{fn: func(context.Context, ApprovalReq) (Decision, error) {
		return Decision{Approve: true}, nil
	}}
	adapter := approvingAdapter{approver}
	r := NewRouter(RouterConfig{}, gateHost{gate}, nil, adapter,
		NewApprovalRouter(ToolPolicyConfirm, time.Minute, nil))
	r.approve.Register("s1", ChatRef{ID: "oc_chat"}, approver)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	r.startPermissionPoller(ctx, "s1")

	waitForPermission(t, func() bool {
		for _, c := range gate.snapshot() {
			if c.requestID == "r1" && c.decision == PermissionAllowOnce {
				return true
			}
		}
		return false
	})
}

type plainHost struct{}

func (plainHost) CreateSession(context.Context, SessionSpec) (AgentSession, error) {
	return nil, errors.New("not used")
}
func (plainHost) OpenSession(context.Context, string) (AgentSession, error) {
	return nil, errors.New("not used")
}
func (plainHost) ListSessions(context.Context) ([]SessionSummary, error) { return nil, nil }

func TestRouterLeavesLocalHostsAlone(t *testing.T) {
	approver := &funcApprover{fn: func(context.Context, ApprovalReq) (Decision, error) {
		t.Error("a CLI host already gates tools in-process")
		return Decision{Approve: true}, nil
	}}
	adapter := approvingAdapter{approver}
	r := NewRouter(RouterConfig{}, plainHost{}, nil, adapter,
		NewApprovalRouter(ToolPolicyConfirm, time.Minute, nil))
	r.approve.Register("s1", ChatRef{ID: "oc_chat"}, approver)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	r.startPermissionPoller(ctx, "s1")
	time.Sleep(50 * time.Millisecond)
	if got := approver.calls.Load(); got != 0 {
		t.Fatalf("asked %d times on a host with no permission gate", got)
	}
}
