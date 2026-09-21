// SessionSnapshot is the complete, self-contained state of one session at a
// point in time: the header, the full persisted entry tree (every branch), and
// the id of the active leaf. It is what a freshly-connecting consumer needs
// before it can follow incremental events (the remotecontrol / web frontends,
// phase 2c/4): replaying the leaf path of Entries against LeafID reproduces
// exactly the conversation the session would continue from.
//
// The snapshot deliberately uses pigo's own types (session.SessionHeader,
// session.Entry) rather than a flattened protocol shape: the tree structure
// (id/parentId per entry) is what makes mid-session forking and branch
// switching work later, and flattening it here would lose that permanently.
package serve

import (
	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/session"
)

// SessionSnapshot carries the header, the entry tree, and the active leaf id.
// Entries is the full persisted tree in file order; the linear conversation
// feeding the active branch is session.PathToLeaf(Entries, LeafID).
type SessionSnapshot struct {
	// Header is the session's current header (model/provider/system prompt
	// as last set).
	Header session.SessionHeader `json:"header"`
	// Entries is the persisted entry tree (all branches) in file order.
	Entries []session.Entry `json:"entries"`
	// LeafID is the active leaf entry; empty for a session with no messages.
	LeafID string `json:"leafId"`
}

// Messages returns the linear message list feeding the active branch — the
// conversation a continuation run starts from. It is PathToLeaf projected
// onto the wrapped messages.
func (s SessionSnapshot) Messages() []agentcore.Message {
	path := session.PathToLeaf(s.Entries, s.LeafID)
	out := make([]agentcore.Message, 0, len(path))
	for _, e := range path {
		out = append(out, e.Message)
	}
	return out
}
