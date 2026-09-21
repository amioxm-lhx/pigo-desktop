// Package serve is the frontend-facing service layer: a thin, stable façade
// over the session store, the provider registry, and the agent runtime that
// turns repl/tui/headless/remotecontrol (and later a web frontend) into
// interchangeable consumers of one service instead of four parallel stacks.
//
// Dependency direction is strictly inward — serve knows runtime, session,
// provider and agentcore; none of them know serve — so the loop never
// depends on how it is being driven. The layer owns no policy of its own:
// provider resolution, tool sets and system prompts are supplied by the host
// through RunConfigFunc (typically cli/run's SetupEnv + NewConfig), which
// keeps serve embeddable from tests and future binaries alike.
//
// The types are pigo's own (session.SessionHeader, session.Entry,
// agentcore.*): no flattened protocol structs, no string-discriminated
// envelopes — the tree-shaped session model stays intact across the boundary.
package serve

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
	"github.com/smallnest/pigo/internal/runtime"
	"github.com/smallnest/pigo/internal/session"
)

// Service is the session-level entry point: enumerate what exists, start a
// new conversation, or reopen a stored one. Every method is safe for
// concurrent use; OpenSession/CreateSession return a live SessionRuntime,
// not a data bag — driving the conversation and reading its state both go
// through that handle.
type Service interface {
	// ListSessions returns the headers of every stored session, most
	// recently updated first (delegates to session.Store.List).
	ListSessions(ctx context.Context) ([]session.SessionHeader, error)
	// ListModels returns the selectable model catalog (the provider preset
	// catalog mapped onto provider.Model). It is the menu a frontend offers
	// before the first prompt; it never contacts a network.
	ListModels(ctx context.Context) ([]provider.Model, error)
	// CreateSession starts a fresh session (not yet persisted — the first
	// prompt run persists it) and returns its runtime.
	CreateSession(ctx context.Context, cfg SessionConfig) (SessionRuntime, error)
	// OpenSession reopens a stored session by id and returns its runtime
	// positioned at the most recently appended leaf.
	OpenSession(ctx context.Context, id string) (SessionRuntime, error)
}

// SessionConfig is the initial state of a new session.
type SessionConfig struct {
	// Model is the model id for the first run (e.g. a preset id).
	Model string
	// Provider names the serving provider; empty lets the host's
	// RunConfigFunc resolve it (model-name heuristics).
	Provider string
	// SystemPrompt is the session's standing system prompt.
	SystemPrompt string
	// Cwd is the working directory the session was started from.
	Cwd string
}

// RunConfigFunc builds the RunConfig for one prompt run on a session. It
// receives the session's current header (model/provider/system prompt —
// SetModel updates flow through here) and thinking level, and returns a
// ready-to-run config; serve fills in SessionID and composes steering on
// top of whatever the factory produced. Hosts that assemble configs via
// cli/run.NewConfig can close over their environment and ignore the header
// except for Model/Provider.
type RunConfigFunc func(header session.SessionHeader, thinking agentcore.ThinkingLevel) runtime.RunConfig

// NewService builds a Service over store. newRunConfig is mandatory: it is
// how the host injects providers, tools and prompts without serve depending
// on any of them.
func NewService(store session.Store, newRunConfig RunConfigFunc) (Service, error) {
	if store == nil {
		return nil, errors.New("serve: store must not be nil")
	}
	if newRunConfig == nil {
		return nil, errors.New("serve: newRunConfig must not be nil")
	}
	return &service{store: store, newRunConfig: newRunConfig, now: time.Now}, nil
}

// service is the concrete Service. It holds no per-session state; every
// SessionRuntime is independent.
type service struct {
	store        session.Store
	newRunConfig RunConfigFunc
	now          func() time.Time
}

// ListSessions delegates to the store (most-recent-first).
func (s *service) ListSessions(ctx context.Context) ([]session.SessionHeader, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	return s.store.List()
}

// ListProjects the preset catalog as the model menu. Models are grouped in
// catalog order (openrouter first, mirroring the CLI picker).
func (s *service) ListModels(ctx context.Context) ([]provider.Model, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	out := make([]provider.Model, 0, len(provider.PresetCatalog))
	for _, p := range provider.PresetCatalog {
		out = append(out, provider.Model{
			Provider:    p.Provider,
			ID:          p.ID,
			DisplayName: p.DisplayName,
		})
	}
	return out, nil
}

// CreateSession mints a fresh header and returns the runtime for it. The
// session file appears on disk only after the first prompt run completes —
// an abandoned empty session leaves no residue.
func (s *service) CreateSession(ctx context.Context, cfg SessionConfig) (SessionRuntime, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	now := s.now()
	header := session.SessionHeader{
		Version:      session.SchemaVersion,
		ID:           session.NewID(now),
		CreatedAt:    now,
		UpdatedAt:    now,
		Model:        cfg.Model,
		Provider:     cfg.Provider,
		SystemPrompt: cfg.SystemPrompt,
		Cwd:          cfg.Cwd,
	}
	return newSessionRuntime(s.store, s.newRunConfig, header), nil
}

// OpenSession loads the stored session and positions the runtime at the
// most recently appended leaf: the linear conversation from the root down
// that leaf becomes the continuation context, and the next prompt grows the
// same branch (no silent sibling fork).
func (s *service) OpenSession(ctx context.Context, id string) (SessionRuntime, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	header, entries, err := s.store.LoadEntries(id)
	if err != nil {
		return nil, fmt.Errorf("serve: open session %s: %w", id, err)
	}
	leaf := ""
	if len(entries) > 0 {
		leaf = entries[len(entries)-1].ID
	}
	rt := newSessionRuntime(s.store, s.newRunConfig, header)
	rt.attach(entries, leaf)
	return rt, nil
}
