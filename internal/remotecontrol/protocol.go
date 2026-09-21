package remotecontrol

import "github.com/smallnest/pigo/internal/serve"

// FrameType enumerates the WebSocket message kinds exchanged between the server
// and the paired browser. It is the wire contract shared with the embedded SPA.
type FrameType string

const (
	// FrameOutput streams session text from server to client.
	FrameOutput FrameType = "output"
	// FrameInput carries a prompt line submitted by the client to the server.
	FrameInput FrameType = "input"
	// FrameConfirm asks the client to approve/reject a risky tool call.
	FrameConfirm FrameType = "confirm"
	// FrameDecide carries the client's approval decision back to the server.
	FrameDecide FrameType = "decide"
	// FrameStatus reports session lifecycle changes to the client.
	FrameStatus FrameType = "status"
	// FrameSnapshot delivers the full structured session state (header, entry
	// tree, active leaf) to a freshly connected client — the baseline that
	// subsequent FrameEvent frames incrementally update. Sent at most once per
	// connection, before any event frames.
	FrameSnapshot FrameType = "snapshot"
	// FrameEvent streams one AgentEvent in the runtime's wire encoding (see
	// runtime.EventEnvelope: a "type"-discriminated JSON object) as an
	// incremental update following the snapshot.
	FrameEvent FrameType = "event"
)

// Status values carried in Frame.State for FrameStatus frames.
const (
	StatusConnected    = "connected"
	StatusEnded        = "ended"
	StatusDisconnected = "disconnected"
)

// Frame is a single WebSocket message. Fields are populated according to Type;
// unused fields are omitted from the JSON encoding.
type Frame struct {
	Type FrameType `json:"type"`

	// Output
	Text string `json:"text,omitempty"`

	// Confirm
	ConfirmID string `json:"confirmId,omitempty"`
	Tool      string `json:"tool,omitempty"`
	Summary   string `json:"summary,omitempty"`

	// Decide
	Approve bool `json:"approve,omitempty"`
	Always  bool `json:"always,omitempty"`

	// Status
	State  string `json:"state,omitempty"`  // connected | ended | disconnected
	Reason string `json:"reason,omitempty"` // human-readable detail for State

	// Snapshot (FrameSnapshot): the complete session state at connect time.
	Snapshot *serve.SessionSnapshot `json:"snapshot,omitempty"`

	// Event (FrameEvent): one AgentEvent in the runtime's wire encoding.
	Event map[string]any `json:"event,omitempty"`
}
