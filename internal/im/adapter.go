package im

import (
	"context"
	"errors"
)

// ErrUpdateUnsupported is returned by Adapter.Update on platforms that cannot
// edit a message that was already delivered. The Router falls back to sending
// a fresh final message, which is why the interface does not require in-place
// update to be implemented usefully.
var ErrUpdateUnsupported = errors.New("im: adapter cannot update a sent message")

// Adapter is one platform's transport. Implementations own their connection
// lifecycle and the platform-specific rendering; they own no routing policy.
//
// Contract for Run: it blocks until ctx is cancelled or the connection fails,
// pushing inbound messages to in. It must not block on in — a full channel means
// the Router is behind, and dropping the event is better than stalling the
// platform's read loop (which would then look like a hang to the user). It must
// also return from the platform's own event callback within the platform's
// deadline; that is why parsing and hand-off are the only work allowed there.
type Adapter interface {
	// ID is the stable channel id ("feishu"). It prefixes logs and is stored
	// with bindings so a chat id from one platform can never collide with
	// another's.
	ID() string
	// Run establishes the channel and streams inbound messages.
	Run(ctx context.Context, in chan<- Inbound) error
	// Send delivers msg to chat and returns the platform message id, which is
	// the handle for a later Update.
	Send(ctx context.Context, chat ChatRef, msg Outbound) (string, error)
	// Update replaces an already-sent message in place. Platforms that cannot
	// do this return ErrUpdateUnsupported.
	Update(ctx context.Context, chat ChatRef, msgID string, msg Outbound) error
	// Close releases resources. It is safe to call after Run has returned.
	Close() error
}

// Approver is implemented by an Adapter that can ask the human a yes/no
// question through the channel (Feishu: interactive card buttons).
//
// A channel without an Approver never gets to run a side-effect tool: the
// Router fails closed rather than silently allowing it, because "nobody could be
// asked" must not degrade into "everybody is allowed".
type Approver interface {
	// Ask delivers an approval request and blocks until a decision arrives,
	// the request times out, or ctx is cancelled. A cancelled or expired
	// request resolves to deny.
	Ask(ctx context.Context, req ApprovalReq) (Decision, error)
}
