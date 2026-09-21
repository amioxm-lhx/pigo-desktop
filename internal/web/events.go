// The SSE stream (GET /api/sessions/{id}/events): a freshly connecting
// client first receives the full session snapshot as one frame, then the
// incremental AgentEvent feed of subsequent runs. The ordering is
// subscribe → snapshot → snapshot frame → event loop, the same discipline
// remotecontrol's structured feed uses (phase 2c) for the same reason:
// subscribing before snapshotting means events cover the snapshot gap too —
// the two may overlap for a run that finishes in between, and a client
// resolves overlap by preferring events, never by losing the window. Writing
// the snapshot frame before entering the event loop guarantees wire order:
// snapshot first, events strictly after.
//
// Event payloads are runtime.EventEnvelope — the shared wire encoding of
// headless stream-json output and remotecontrol's FrameEvent frames — so a
// web client understands exactly what those frontends understand.
package web

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"

	"github.com/smallnest/pigo/internal/runtime"
)

// SSE frame kinds. All agent events ride one kind; the envelope's "type"
// field discriminates them (same single-discriminant shape as the other
// frontends' wire encodings).
const (
	sseSnapshotEvent = "snapshot"
	sseAgentEvent    = "agent_event"

	// sseHeartbeat is the comment-line interval that keeps intermediaries
	// from idling out a quiet stream.
	sseHeartbeat = 15 * time.Second
)

func (s *Server) handleEvents(w http.ResponseWriter, r *http.Request) {
	rt, err := s.runtime(r.Context(), r.PathValue("id"))
	if err != nil {
		writeError(w, statusForError(err), err.Error())
		return
	}
	flusher, ok := w.(http.Flusher)
	if !ok {
		writeError(w, http.StatusInternalServerError, "streaming unsupported")
		return
	}

	// Subscribe before snapshotting; see the file comment.
	sub, err := rt.Subscribe()
	if err != nil {
		writeError(w, statusForError(err), err.Error())
		return
	}
	defer sub.Close()
	snap, err := rt.Snapshot(r.Context())
	if err != nil {
		writeError(w, statusForError(err), err.Error())
		return
	}

	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.WriteHeader(http.StatusOK)

	if writeSSE(w, flusher, sseSnapshotEvent, snap) != nil {
		return
	}

	heartbeat := time.NewTicker(sseHeartbeat)
	defer heartbeat.Stop()
	for {
		select {
		case <-r.Context().Done():
			return
		case <-heartbeat.C:
			// A comment line keeps the connection alive during quiet periods.
			if _, err := io.WriteString(w, ": ping\n\n"); err != nil {
				return
			}
			flusher.Flush()
		case ev, ok := <-sub.Events():
			if !ok {
				// The runtime was disposed; the stream simply ends.
				return
			}
			if writeSSE(w, flusher, sseAgentEvent, runtime.EventEnvelope(ev)) != nil {
				return
			}
		}
	}
}

// writeSSE emits one SSE frame and flushes it. A write error (client gone)
// tells the caller to end the stream.
func writeSSE(w io.Writer, flusher http.Flusher, event string, payload any) error {
	data, err := json.Marshal(payload)
	if err != nil {
		return err
	}
	if _, err := fmt.Fprintf(w, "event: %s\ndata: %s\n\n", event, data); err != nil {
		return err
	}
	flusher.Flush()
	return nil
}
