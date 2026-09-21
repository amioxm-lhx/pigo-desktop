// Package web is the embeddable HTTP frontend over serve.Service (plan phase
// 4): it exposes the session service as a plain http.Handler — REST endpoints
// for the session catalog and the runtime controls, plus an SSE stream for
// run events — so the host decides how (and whether) to serve it: a pigo
// binary, an httptest-driven test, or any embedding application mounting it
// behind its own middleware. The package owns no server lifecycle, no TLS,
// and no authentication; those are host concerns, which is exactly what lets
// remotecontrol (with its LAN pairing) and a web UI share one service
// boundary without either forcing its policy on the other.
//
// Dependency direction stays inward: web → serve → runtime/session/provider,
// and nothing inward knows web. Payloads are pigo's own types —
// serve.SessionSnapshot crosses the wire with its entry tree intact, and run
// events use runtime.EventEnvelope, the same wire encoding headless output and
// remotecontrol's FrameEvent frames use — so every frontend agrees on one
// shape.
package web

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"os"
	"strings"
	"sync"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/serve"
)

// maxBodyBytes bounds request bodies: prompts and settings are small, and a
// bound keeps a stray upload from being buffered whole.
const maxBodyBytes = 1 << 20

// Server adapts a serve.Service onto HTTP. Requests for the same session id
// share one cached serve.SessionRuntime, so a prompt and its event stream
// drive the same conversation. All exported methods are safe for concurrent
// use.
type Server struct {
	svc serve.Service

	mu       sync.Mutex
	runtimes map[string]serve.SessionRuntime // id → live handle shared across requests
}

// New builds a Server over svc. The Service is the only boundary this package
// knows; everything below it (store backend, providers, tools) is the host's
// choice.
func New(svc serve.Service) *Server {
	return &Server{svc: svc, runtimes: make(map[string]serve.SessionRuntime)}
}

// Handler returns the routed HTTP handler (REST + SSE). Serve it directly or
// mount it behind the host's own middleware; the returned handler serves
// concurrent requests.
func (s *Server) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /api/models", s.handleModels)
	mux.HandleFunc("GET /api/sessions", s.handleListSessions)
	mux.HandleFunc("POST /api/sessions", s.handleCreateSession)
	mux.HandleFunc("GET /api/sessions/{id}", s.handleGetSession)
	mux.HandleFunc("GET /api/sessions/{id}/events", s.handleEvents)
	mux.HandleFunc("POST /api/sessions/{id}/prompt", s.handlePrompt)
	mux.HandleFunc("POST /api/sessions/{id}/steer", s.handleSteer)
	mux.HandleFunc("POST /api/sessions/{id}/abort", s.handleAbort)
	mux.HandleFunc("POST /api/sessions/{id}/model", s.handleSetModel)
	mux.HandleFunc("POST /api/sessions/{id}/thinking", s.handleSetThinking)
	mux.HandleFunc("DELETE /api/sessions/{id}", s.handleDeleteSession)
	return mux
}

// Close disposes every cached runtime. Sessions stay persisted on disk; only
// the in-memory handles go away. An embedding host calls it on shutdown.
func (s *Server) Close() error {
	s.mu.Lock()
	rts := make([]serve.SessionRuntime, 0, len(s.runtimes))
	for _, rt := range s.runtimes {
		rts = append(rts, rt)
	}
	s.runtimes = make(map[string]serve.SessionRuntime)
	s.mu.Unlock()
	var firstErr error
	for _, rt := range rts {
		if err := rt.Dispose(); err != nil && firstErr == nil {
			firstErr = err
		}
	}
	return firstErr
}

// runtime returns the live handle for a session id, opening (and caching) it
// on first use. The whole open runs under the lock so two concurrent first
// requests cannot end up with two runtimes for one session — a split event
// stream. A cached handle that was disposed is replaced by a fresh open.
func (s *Server) runtime(ctx context.Context, id string) (serve.SessionRuntime, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if rt, ok := s.runtimes[id]; ok && rt.GetPhase() != serve.PhaseDisposed {
		return rt, nil
	}
	rt, err := s.svc.OpenSession(ctx, id)
	if err != nil {
		return nil, err
	}
	s.runtimes[id] = rt
	return rt, nil
}

// statusForError maps serve-layer errors onto HTTP statuses.
func statusForError(err error) int {
	switch {
	case errors.Is(err, os.ErrNotExist):
		return http.StatusNotFound
	case errors.Is(err, serve.ErrDisposed):
		return http.StatusGone
	case errors.Is(err, serve.ErrRunActive), errors.Is(err, serve.ErrNoActiveRun):
		return http.StatusConflict
	default:
		return http.StatusInternalServerError
	}
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func writeError(w http.ResponseWriter, status int, msg string) {
	writeJSON(w, status, map[string]string{"error": msg})
}

// decodeJSON reads one JSON value into v, bounding the body size. It has
// already written the error response when it returns false.
func decodeJSON(w http.ResponseWriter, r *http.Request, v any) bool {
	r.Body = http.MaxBytesReader(w, r.Body, maxBodyBytes)
	if err := json.NewDecoder(r.Body).Decode(v); err != nil {
		writeError(w, http.StatusBadRequest, "invalid JSON body: "+err.Error())
		return false
	}
	return true
}

// phaseName renders a serve.Phase for the wire.
func phaseName(p serve.Phase) string {
	switch p {
	case serve.PhaseIdle:
		return "idle"
	case serve.PhaseRunning:
		return "running"
	case serve.PhaseDisposed:
		return "disposed"
	default:
		return "unknown"
	}
}

func (s *Server) handleModels(w http.ResponseWriter, r *http.Request) {
	models, err := s.svc.ListModels(r.Context())
	if err != nil {
		writeError(w, statusForError(err), err.Error())
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"models": models})
}

func (s *Server) handleListSessions(w http.ResponseWriter, r *http.Request) {
	headers, err := s.svc.ListSessions(r.Context())
	if err != nil {
		writeError(w, statusForError(err), err.Error())
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"sessions": headers})
}

type createSessionRequest struct {
	Model        string `json:"model"`
	Provider     string `json:"provider"`
	SystemPrompt string `json:"systemPrompt,omitempty"`
	Cwd          string `json:"cwd,omitempty"`
}

func (s *Server) handleCreateSession(w http.ResponseWriter, r *http.Request) {
	var req createSessionRequest
	if !decodeJSON(w, r, &req) {
		return
	}
	rt, err := s.svc.CreateSession(r.Context(), serve.SessionConfig{
		Model:        req.Model,
		Provider:     req.Provider,
		SystemPrompt: req.SystemPrompt,
		Cwd:          req.Cwd,
	})
	if err != nil {
		writeError(w, statusForError(err), err.Error())
		return
	}
	s.mu.Lock()
	s.runtimes[rt.ID()] = rt
	s.mu.Unlock()
	writeJSON(w, http.StatusCreated, map[string]string{"id": rt.ID()})
}

// sessionState is the GET /api/sessions/{id} payload: the live phase plus the
// persisted snapshot, exactly as serve produces it.
type sessionState struct {
	Phase    string                `json:"phase"`
	Snapshot serve.SessionSnapshot `json:"snapshot"`
}

func (s *Server) handleGetSession(w http.ResponseWriter, r *http.Request) {
	rt, err := s.runtime(r.Context(), r.PathValue("id"))
	if err != nil {
		writeError(w, statusForError(err), err.Error())
		return
	}
	snap, err := rt.Snapshot(r.Context())
	if err != nil {
		writeError(w, statusForError(err), err.Error())
		return
	}
	writeJSON(w, http.StatusOK, sessionState{Phase: phaseName(rt.GetPhase()), Snapshot: snap})
}

type promptRequest struct {
	Text string `json:"text"`
}

func (s *Server) handlePrompt(w http.ResponseWriter, r *http.Request) {
	var req promptRequest
	if !decodeJSON(w, r, &req) {
		return
	}
	if strings.TrimSpace(req.Text) == "" {
		writeError(w, http.StatusBadRequest, "text must not be empty")
		return
	}
	rt, err := s.runtime(r.Context(), r.PathValue("id"))
	if err != nil {
		writeError(w, statusForError(err), err.Error())
		return
	}
	// Background context on purpose: the run must outlive this request.
	// r.Context() is cancelled the moment the 202 response is written, which
	// would abort the run mid-flight.
	if err := rt.Prompt(context.Background(), req.Text); err != nil {
		writeError(w, statusForError(err), err.Error())
		return
	}
	w.WriteHeader(http.StatusAccepted)
}

func (s *Server) handleSteer(w http.ResponseWriter, r *http.Request) {
	var req promptRequest
	if !decodeJSON(w, r, &req) {
		return
	}
	if strings.TrimSpace(req.Text) == "" {
		writeError(w, http.StatusBadRequest, "text must not be empty")
		return
	}
	rt, err := s.runtime(r.Context(), r.PathValue("id"))
	if err != nil {
		writeError(w, statusForError(err), err.Error())
		return
	}
	if err := rt.Steer(req.Text); err != nil {
		writeError(w, statusForError(err), err.Error())
		return
	}
	w.WriteHeader(http.StatusAccepted)
}

func (s *Server) handleAbort(w http.ResponseWriter, r *http.Request) {
	rt, err := s.runtime(r.Context(), r.PathValue("id"))
	if err != nil {
		writeError(w, statusForError(err), err.Error())
		return
	}
	rt.Abort() // no-op when idle
	w.WriteHeader(http.StatusAccepted)
}

type setModelRequest struct {
	Model string `json:"model"`
}

func (s *Server) handleSetModel(w http.ResponseWriter, r *http.Request) {
	var req setModelRequest
	if !decodeJSON(w, r, &req) {
		return
	}
	if req.Model == "" {
		writeError(w, http.StatusBadRequest, "model must not be empty")
		return
	}
	rt, err := s.runtime(r.Context(), r.PathValue("id"))
	if err != nil {
		writeError(w, statusForError(err), err.Error())
		return
	}
	if err := rt.SetModel(req.Model); err != nil {
		writeError(w, statusForError(err), err.Error())
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

type setThinkingRequest struct {
	Level agentcore.ThinkingLevel `json:"level"`
}

func (s *Server) handleSetThinking(w http.ResponseWriter, r *http.Request) {
	var req setThinkingRequest
	if !decodeJSON(w, r, &req) {
		return
	}
	if req.Level == "" {
		writeError(w, http.StatusBadRequest, "level must not be empty")
		return
	}
	rt, err := s.runtime(r.Context(), r.PathValue("id"))
	if err != nil {
		writeError(w, statusForError(err), err.Error())
		return
	}
	if err := rt.SetThinking(req.Level); err != nil {
		writeError(w, statusForError(err), err.Error())
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// handleDeleteSession disposes the cached runtime and forgets it. The session
// stays on disk — a later GET reopens it. Idempotent.
func (s *Server) handleDeleteSession(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	s.mu.Lock()
	rt, ok := s.runtimes[id]
	delete(s.runtimes, id)
	s.mu.Unlock()
	if ok {
		if err := rt.Dispose(); err != nil {
			writeError(w, statusForError(err), err.Error())
			return
		}
	}
	w.WriteHeader(http.StatusNoContent)
}
