package im

import (
	"context"
	"errors"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
)

// ErrRunActive reports that another run already owns the session. The Router
// turns it into a steer: the text joins the current turn instead of being
// dropped, which is what a user sending two quick messages expects.
var ErrRunActive = errors.New("im: a run is already active on this session")

// Session host identifiers, as accepted by [im] session_host.
const (
	// SessionHostCLI runs sessions inside the im process (the pigo store).
	SessionHostCLI = "cli"
	// SessionHostDesktop drives a running pigo desktop app over its control
	// endpoint, so the conversation is the same one the app shows.
	SessionHostDesktop = "desktop"
)

// SessionSpec is what a Router asks a host to create for a new chat.
type SessionSpec struct {
	// Model pins the model for the new session. Empty lets the host decide.
	Model string
	// Cwd is the working directory the session is bound to.
	Cwd string
}

// SessionSummary is one row of /sessions.
type SessionSummary struct {
	ID        string
	Title     string
	Cwd       string
	UpdatedAt time.Time
}

// SessionHost creates and opens sessions. internal/serve implements it through
// ServeHost; the desktop control plane implements it through its own client.
// That interface is the whole reason one Router can drive either carrier
// without the channel layer knowing which one it is talking to.
type SessionHost interface {
	CreateSession(ctx context.Context, spec SessionSpec) (AgentSession, error)
	OpenSession(ctx context.Context, id string) (AgentSession, error)
	ListSessions(ctx context.Context) ([]SessionSummary, error)
}

// EventStream is the live feed of one run.
type EventStream interface {
	// Events is closed when the run ends or the stream is closed.
	Events() <-chan agentcore.AgentEvent
	// Close detaches the stream. Idempotent.
	Close()
}

// AgentSession is the narrow slice of a session the Router needs. It is
// deliberately smaller than serve.SessionRuntime: the desktop control plane can
// answer all of this over HTTP, while snapshot/thinking/dispose are pigo-side
// concerns the channel never touches.
type AgentSession interface {
	// ID is the stable session id the binding table stores.
	ID() string
	// Busy reports whether a run is in flight.
	Busy() bool
	// Prompt starts a run and returns immediately. When the session is already
	// running it returns ErrRunActive.
	Prompt(ctx context.Context, text string) error
	// Steer injects text into the active run.
	Steer(text string) error
	// Abort cancels the active run. No-op when idle.
	Abort()
	// SetModel switches the model for subsequent runs.
	SetModel(model string) error
	// Subscribe returns the live feed for the next run.
	Subscribe() (EventStream, error)
	// Close releases the session handle. It never deletes stored history.
	Close() error
}
