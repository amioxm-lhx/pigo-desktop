package im

import (
	"context"
	"errors"
	"time"
)

// PermissionDecision is the host-core vocabulary for answering an open tool
// permission request. Keeping the strings here (rather than in the desktop
// client) means every carrier agrees on what "allow once" means, and a channel
// cannot invent a decision the host would silently turn into a denial.
type PermissionDecision string

const (
	// PermissionAllowOnce runs just this tool call.
	PermissionAllowOnce PermissionDecision = "allow-once"
	// PermissionAllowSession also records a session grant, so the host stops
	// asking about the same tool for the rest of the conversation.
	PermissionAllowSession PermissionDecision = "allow-session"
	// PermissionDeny refuses the call. It is the fail-closed outcome of every
	// error, timeout and cancellation along the approval path.
	PermissionDeny PermissionDecision = "deny"
)

// ErrPermissionGone says the host no longer has this request queued: the
// window answered it, it expired, or the turn was cancelled. A remote channel
// treats it as success — the question is settled either way — rather than
// reporting a failure to the user.
var ErrPermissionGone = errors.New("im: permission request is no longer pending")

// ErrPermissionUnsupported says the running desktop app predates the
// permission-queue ops. A channel must stop polling rather than log the same
// NOT_ALLOWED every tick, and must say what to rebuild: replacing pigo.exe
// alone cannot add an op to the app's control endpoint.
var ErrPermissionUnsupported = errors.New("im: host does not expose the permission queue")

// PendingPermission is one open tool confirmation a session host is waiting on.
// It is deliberately a projection, not the host's own record: the channel only
// needs enough to phrase a card and to know when asking is already pointless.
type PendingPermission struct {
	// RequestID is the host-issued id Resolve answers by.
	RequestID string
	// SessionID is the session that opened the request.
	SessionID string
	// ToolName is what is about to run ("Edit", "Bash", …).
	ToolName string
	// Reason is the host's one-line explanation of why approval is needed.
	Reason string
	// Summary is a bounded preview of the tool arguments.
	Summary string
	// Remaining is how long the host will still wait. Zero means unknown.
	Remaining time.Duration
}

// PermissionGate is the slice of a session host that owns tool confirmations.
// A host implementing it asks its questions somewhere the channel cannot see
// (the desktop app's window), so the channel polls Pending and writes decisions
// back with Resolve. Hosts that run the agent in-process do not implement this
// — their confirmations go through ApprovalRouter.WrapRunConfig instead.
type PermissionGate interface {
	Pending(ctx context.Context, sessionID string) ([]PendingPermission, error)
	Resolve(ctx context.Context, requestID string, decision PermissionDecision) error
}
