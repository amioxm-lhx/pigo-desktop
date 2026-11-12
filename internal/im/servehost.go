package im

import (
	"context"
	"errors"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/serve"
)

// ServeHost adapts *serve.Service to SessionHost. It is the carrier the CLI
// uses: sessions live in the local pigo store and run in this process.
type ServeHost struct {
	svc serve.Service
}

// NewServeHost wraps a serve.Service as a SessionHost.
func NewServeHost(svc serve.Service) *ServeHost { return &ServeHost{svc: svc} }

// CreateSession starts a fresh session.
func (h *ServeHost) CreateSession(ctx context.Context, spec SessionSpec) (AgentSession, error) {
	rt, err := h.svc.CreateSession(ctx, serve.SessionConfig{Model: spec.Model, Cwd: spec.Cwd})
	if err != nil {
		return nil, err
	}
	return &serveSession{rt: rt}, nil
}

// OpenSession reopens a stored session by id.
func (h *ServeHost) OpenSession(ctx context.Context, id string) (AgentSession, error) {
	rt, err := h.svc.OpenSession(ctx, id)
	if err != nil {
		return nil, err
	}
	return &serveSession{rt: rt}, nil
}

// ListSessions returns headers for /sessions.
func (h *ServeHost) ListSessions(ctx context.Context) ([]SessionSummary, error) {
	headers, err := h.svc.ListSessions(ctx)
	if err != nil {
		return nil, err
	}
	out := make([]SessionSummary, 0, len(headers))
	for _, h := range headers {
		// CLI sessions have no stored title; /sessions derives the label from
		// the working directory (see Router.sessionsText).
		out = append(out, SessionSummary{
			ID:        h.ID,
			Cwd:       h.Cwd,
			UpdatedAt: h.UpdatedAt,
		})
	}
	return out, nil
}

// serveSession adapts serve.SessionRuntime to AgentSession.
type serveSession struct {
	rt serve.SessionRuntime
}

func (s *serveSession) ID() string { return s.rt.ID() }

func (s *serveSession) Busy() bool { return s.rt.GetPhase() == serve.PhaseRunning }

func (s *serveSession) Prompt(ctx context.Context, text string) error {
	err := s.rt.Prompt(ctx, text)
	if errors.Is(err, serve.ErrRunActive) {
		return ErrRunActive
	}
	return err
}

func (s *serveSession) Steer(text string) error { return s.rt.Steer(text) }

func (s *serveSession) Abort() { s.rt.Abort() }

func (s *serveSession) SetModel(model string) error { return s.rt.SetModel(model) }

func (s *serveSession) Subscribe() (EventStream, error) {
	sub, err := s.rt.Subscribe()
	if err != nil {
		return nil, err
	}
	return serveStream{sub: sub}, nil
}

func (s *serveSession) Close() error { return s.rt.Dispose() }

// serveStream adapts *serve.Subscription to EventStream. Kept as a value type
// so Close stays idempotent through the subscription's own sync.Once.
type serveStream struct {
	sub *serve.Subscription
}

func (s serveStream) Events() <-chan agentcore.AgentEvent { return s.sub.Events() }

func (s serveStream) Close() { s.sub.Close() }
