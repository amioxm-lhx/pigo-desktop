// Package runtime owns one pigo agent session per desktop session. Everything
// durable (transcripts, tool execution, permissions) stays with host-core; this
// package only runs the loop and reports events.
package runtime

import (
	"context"
	"encoding/json"
	"log"
	"strings"
	"sync"

	"github.com/smallnest/pigo/desktop/sidecar/internal/jsonrpc"
	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/provider"
)

// Host is the reverse-RPC channel back to Electron main.
type Host interface {
	Call(ctx context.Context, method string, params any) (json.RawMessage, error)
	CallInto(ctx context.Context, method string, params any, out any) error
}

// Emitter publishes one agent event to Electron main.
type Emitter interface {
	Emit(envelope protocol.EventEnvelope) error
}

// Options configures a Manager.
type Options struct {
	Host    Host
	Emitter Emitter
	Logger  *log.Logger
	// StreamFn overrides the model stream. Production leaves it nil and the
	// runtime streams through the provider built from Provider.
	StreamFn provider.StreamFn
	// Model overrides the model metadata used when StreamFn is set (tests).
	Model provider.Model
}

// Manager owns the live session runtimes.
type Manager struct {
	host     Host
	emitter  Emitter
	logger   *log.Logger
	streamFn provider.StreamFn
	model    provider.Model

	mu       sync.Mutex
	sessions map[string]*Session
}

// NewManager creates a runtime manager.
func NewManager(options Options) *Manager {
	return &Manager{
		host:     options.Host,
		emitter:  options.Emitter,
		logger:   options.Logger,
		streamFn: options.StreamFn,
		model:    options.Model,
		sessions: map[string]*Session{},
	}
}

// Get returns the live runtime for a session, or nil.
func (m *Manager) Get(sessionID string) *Session {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.sessions[sessionID]
}

// SetEmitter attaches the event channel once the connection exists.
func (m *Manager) SetEmitter(emitter Emitter) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.emitter = emitter
}

// Count reports how many runtimes are live.
func (m *Manager) Count() int {
	m.mu.Lock()
	defer m.mu.Unlock()
	return len(m.sessions)
}

// Dispose drops one session's runtime.
func (m *Manager) Dispose(sessionID string) {
	m.mu.Lock()
	session := m.sessions[sessionID]
	delete(m.sessions, sessionID)
	m.mu.Unlock()
	if session != nil {
		session.Dispose()
	}
}

// DisposeAll releases every runtime.
func (m *Manager) DisposeAll() {
	m.mu.Lock()
	sessions := m.sessions
	m.sessions = map[string]*Session{}
	m.mu.Unlock()
	for _, session := range sessions {
		session.Dispose()
	}
}

// Rebuild drops one session's live runtime and immediately rebuilds it from
// the host transcript, without opening a turn. This is the desktop form of the
// CLI's /rebuild: host-core owns the durable transcript, so "rebuild" means
// throwing away the in-memory AgentContext and running NewSession again (which
// restores compaction checkpoints, the tool catalog and the provider binding).
// A session with no live runtime needs no work: its next prompt builds one.
func (m *Manager) Rebuild(ctx context.Context, sessionID string) (bool, error) {
	sessionID = strings.TrimSpace(sessionID)
	if sessionID == "" {
		return false, jsonrpc.InvalidParams("agent.rebuild requires sessionId")
	}

	m.mu.Lock()
	existing := m.sessions[sessionID]
	var params protocol.PromptParams
	if existing != nil {
		params = existing.params
	}
	m.mu.Unlock()

	if existing == nil {
		return false, nil
	}
	if existing.Running() {
		return false, jsonrpc.AppError("AGENT_BUSY", "session already has an active turn")
	}

	// A rebuild is not a prompt. Clear the previous turn's identity so
	// snapshotFromDetail does not mistake the last transcript row for the
	// current (already persisted) user message and drop it.
	params.TurnID = ""
	params.Content = ""
	params.UserMessageID = ""
	params.Attachments = nil
	params.TruncateFromMessageID = ""
	params.TruncateBefore = nil
	params.RefreshHistory = true

	m.Dispose(sessionID)
	if _, err := m.RuntimeFor(ctx, params); err != nil {
		return false, err
	}
	return true, nil
}

// RuntimeFor returns the runtime that should serve a prompt, building one when
// the session has none or when its binding changed.
func (m *Manager) RuntimeFor(ctx context.Context, params protocol.PromptParams) (*Session, error) {
	sessionID := strings.TrimSpace(params.SessionID)
	if sessionID == "" {
		return nil, jsonrpc.InvalidParams("agent.prompt requires sessionId")
	}
	if strings.TrimSpace(params.Provider.ModelID) == "" {
		return nil, jsonrpc.InvalidParams("agent.prompt requires provider.modelId")
	}
	params.Mode = normalizeMode(params.Mode)

	m.mu.Lock()
	existing := m.sessions[sessionID]
	if existing != nil && existing.Running() {
		m.mu.Unlock()
		return nil, jsonrpc.AppError("AGENT_BUSY", "session already has an active turn")
	}
	host, emitter, logger := m.host, m.emitter, m.logger
	streamFn, modelOverride := m.streamFn, m.model
	m.mu.Unlock()

	refresh := params.RefreshHistory ||
		strings.TrimSpace(params.TruncateFromMessageID) != "" ||
		params.TruncateBefore != nil
	if existing != nil && refresh {
		m.Dispose(sessionID)
		existing = nil
	}
	if existing != nil && !existing.Matches(params) {
		m.Dispose(sessionID)
		existing = nil
	}
	if existing != nil {
		if err := existing.RefreshIfChanged(ctx, params); err != nil {
			logf(logger, "session %s: history refresh skipped: %v", sessionID, err)
		}
		existing.UpdateParams(params)
		return existing, nil
	}

	session, err := NewSession(ctx, host, emitter, logger, params, streamFn, modelOverride)
	if err != nil {
		return nil, err
	}

	m.mu.Lock()
	// A concurrent prompt for the same session may have won the race while the
	// host catalog was fetched; the loser is disposed rather than leaked.
	if current := m.sessions[sessionID]; current != nil {
		m.mu.Unlock()
		session.Dispose()
		return current, nil
	}
	m.sessions[sessionID] = session
	m.mu.Unlock()
	return session, nil
}

func normalizeMode(mode string) string {
	switch strings.ToLower(strings.TrimSpace(mode)) {
	case "plan", "goal":
		return strings.ToLower(strings.TrimSpace(mode))
	default:
		return "agent"
	}
}
