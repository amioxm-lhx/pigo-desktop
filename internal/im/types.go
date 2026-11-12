// Package im turns an instant-messaging channel into a remote control surface
// for a pigo session.
//
// The package is a consumer of internal/serve, not a second agent stack: an
// Adapter receives platform messages, a Router binds each chat to one pigo
// session, and everything the agent does flows back through serve's
// SessionRuntime (Snapshot / Subscribe / Prompt / Steer / Abort). No loop logic
// lives here, which is what lets the CLI and, later, the desktop app share one
// implementation.
//
// Layering:
//
//	Adapter (per platform)  ──in──▶  Queue (per-chat, bounded depth)
//	        ▲                              │
//	        └──────out──────  Router ──────┘
//	                             │
//	                        serve.SessionRuntime
//
// Two invariants drive the design:
//
//   - One chat maps to one session, and a session runs one prompt at a time
//     (serve.ErrRunActive). The Queue enforces that by serializing per chat
//     instead of surfacing a concurrency error the user cannot act on.
//   - A platform event handler must return fast (Feishu closes the door at 3s).
//     Adapters therefore only parse and hand off; the agent never runs on the
//     read goroutine.
package im

import "time"

// Kind classifies what an inbound message carries.
type Kind int

const (
	// KindText is a plain message: a prompt for the agent.
	KindText Kind = iota
	// KindCommand is a channel command (/new, /stop …). It is handled by the
	// Router itself and never reaches the model.
	KindCommand
	// KindAction is a card button press carrying Action.
	KindAction
)

// ChatKind distinguishes a one-to-one chat from a group chat. Group chats are
// gated separately because membership is not under our control.
type ChatKind string

const (
	// ChatP2P is a direct (one-to-one) conversation with the bot.
	ChatP2P ChatKind = "p2p"
	// ChatGroup is a group conversation. Group gating is stricter by default.
	ChatGroup ChatKind = "group"
)

// ChatRef identifies a conversation on the platform. ID is the platform's own
// identifier and is the key for both session binding and queue lanes.
type ChatRef struct {
	ID    string
	Kind  ChatKind
	Title string
}

// Key is the stable identity used for binding lookups and queue lanes. An
// unset Kind normalizes to ChatP2P so an adapter that only fills ID still gets
// a deterministic key.
func (c ChatRef) Key() string {
	if c.ID == "" {
		return ""
	}
	kind := c.Kind
	if kind == "" {
		kind = ChatP2P
	}
	return string(kind) + ":" + c.ID
}

// UserRef identifies the human who sent a message, for allow-list decisions.
type UserRef struct {
	// ID is the platform's stable user identifier: Feishu open_id.
	ID string
	// Name is a display name, used only in logs and prompts.
	Name string
	// Email is optional; when the allow list is expressed in emails, the
	// adapter resolves it and fills this in.
	Email string
}

// Display returns the best available human label.
func (u UserRef) Display() string {
	if u.Name != "" {
		return u.Name
	}
	if u.ID != "" {
		return u.ID
	}
	return "unknown"
}

// Inbound is one message handed from an Adapter to the Router.
type Inbound struct {
	// Adapter is the adapter id that produced this message.
	Adapter string
	// Chat is the conversation it arrived in.
	Chat ChatRef
	// Sender is who sent it.
	Sender UserRef
	// MsgID is the platform message id. It is the idempotency key: platforms
	// re-deliver on their own retry paths (Feishu event retries, WeChat weak
	// network), and a replayed MsgID must not start a second run.
	MsgID string
	// Text is the message body. For KindAction it may be empty.
	Text string
	// Kind classifies the message.
	Kind Kind
	// Action carries the button payload for KindAction.
	Action ActionRef
	// Mentioned reports whether the bot was @-mentioned. Group chats require
	// it (see RouterConfig.RequireMention).
	Mentioned bool
	// At is the platform-reported send time; zero means "use receipt time".
	At time.Time
	// Received is when the adapter observed the message.
	Received time.Time
}

// ActionRef is the payload of a card button press.
type ActionRef struct {
	// Name is the button's action: "approve", "approve_always", "deny".
	Name string
	// Token correlates the press with a pending approval request.
	Token string
	// MsgID is the card message that was pressed, so the card can be updated
	// in place to show the decision.
	MsgID string
}

// Button is one interactive control on an Outbound message.
type Button struct {
	// Text is the button label.
	Text string
	// Action is the action name reported back in ActionRef.Name.
	Action string
	// Token is the opaque correlation value the platform echoes back.
	Token string
	// Style is "primary", "danger" or "default".
	Style string
}

// Outbound is one message (or card) the channel sends.
type Outbound struct {
	// Title is the card header. When empty the platform renders text only.
	Title string
	// Text is the body (markdown where the platform supports it).
	Text string
	// Buttons, when non-empty, makes this an interactive card.
	Buttons []Button
	// Final marks the terminal message of a run. Adapters use it to decide
	// whether in-place update is still expected.
	Final bool
	// Footer is a small footnote line (duration, model, session id).
	Footer string
	// Tone tints the message: "" (info), "warn", "error", "success". It is
	// presentation only, but keeping it out of Title parsing means the Router
	// can phrase its headers in Chinese without a channel needing to
	// understand Chinese.
	Tone string
}

// Tone values accepted by Outbound.Tone.
const (
	ToneInfo    = ""
	ToneWarn    = "warn"
	ToneError   = "error"
	ToneSuccess = "success"
)

// Decision is the outcome of an approval request.
type Decision struct {
	// Approve runs the tool call.
	Approve bool
	// Always grants trust for the rest of the session.
	Always bool
}

// ApprovalReq is a request for the human to allow or deny one tool call.
type ApprovalReq struct {
	// Token is the correlation id: the router mints it, the adapter embeds it
	// in the button payload and echoes it back in ActionRef.Token.
	Token string
	// Chat is where to ask.
	Chat ChatRef
	// SessionID is the session the call belongs to.
	SessionID string
	// Tool is the tool name ("bash", "write" …).
	Tool string
	// Summary is the one-line preview of what the tool will do.
	Summary string
	// Timeout is how long to wait. On expiry the request resolves to deny.
	Timeout time.Duration
}
