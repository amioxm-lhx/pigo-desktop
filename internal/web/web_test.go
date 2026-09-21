// Tests for the web package (plan phase 4): the embeddable HTTP frontend is
// driven entirely through its public surface — httptest over Handler()
// against a real serve.Service with scripted streams — which is exactly the
// embedding story a host gets: mount the handler, talk REST, follow the SSE
// stream. The shared-boundary test proves the phase-4 payoff: the same
// Service feeds the web frontend and any other consumer at once.
package web_test

import (
	"bufio"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
	"github.com/smallnest/pigo/internal/runtime"
	"github.com/smallnest/pigo/internal/serve"
	"github.com/smallnest/pigo/internal/session"
	"github.com/smallnest/pigo/internal/web"
)

// textTurn returns a StreamFn answering every call with one plain text turn.
func textTurn(text string) provider.StreamFn {
	return func(ctx context.Context, model string, llm provider.LlmContext, cfg provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		final := agentcore.AssistantMessage{
			RoleField:  agentcore.RoleAssistant,
			Content:    agentcore.ContentList{agentcore.NewTextContent(text)},
			StopReason: agentcore.StopReasonEndTurn,
		}
		s := provider.NewAssistantMessageEventStream(0)
		go func() {
			_ = s.Emit(ctx, provider.StreamDoneEvent{Message: final})
			s.Close()
		}()
		return s, nil
	}
}

// blockingStream never completes a turn until its context is cancelled, for
// the conflict/abort test.
func blockingStream() provider.StreamFn {
	return func(ctx context.Context, model string, llm provider.LlmContext, cfg provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		s := provider.NewAssistantMessageEventStream(0)
		go func() {
			<-ctx.Done()
			s.Close()
		}()
		return s, nil
	}
}

// newWebTestServer mounts a web.Server over a real serve.Service backed by a
// temp JSONL store. The factory records the headers and thinking levels it
// saw so tests can assert that HTTP-side reconfiguration reached the run.
func newWebTestServer(t *testing.T, stream provider.StreamFn) (*httptest.Server, serve.Service, *[]session.SessionHeader, *[]agentcore.ThinkingLevel) {
	t.Helper()
	store, err := session.NewStore(t.TempDir())
	if err != nil {
		t.Fatalf("NewStore: %v", err)
	}
	var headers []session.SessionHeader
	var thinkings []agentcore.ThinkingLevel
	svc, err := serve.NewService(store, func(h session.SessionHeader, th agentcore.ThinkingLevel) runtime.RunConfig {
		headers = append(headers, h)
		thinkings = append(thinkings, th)
		return runtime.RunConfig{
			LoopConfig: runtime.LoopConfig{Model: h.Model, Provider: h.Provider, Stream: stream},
		}
	})
	if err != nil {
		t.Fatalf("NewService: %v", err)
	}
	w := web.New(svc)
	ts := httptest.NewServer(w.Handler())
	t.Cleanup(func() {
		ts.Close()
		_ = w.Close()
	})
	return ts, svc, &headers, &thinkings
}

// httpDo is a small JSON-in/JSON-out request helper.
func httpDo(t *testing.T, method, url, body string) (*http.Response, []byte) {
	t.Helper()
	var rd io.Reader
	if body != "" {
		rd = strings.NewReader(body)
	}
	req, err := http.NewRequest(method, url, rd)
	if err != nil {
		t.Fatalf("%s %s: %v", method, url, err)
	}
	if body != "" {
		req.Header.Set("Content-Type", "application/json")
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatalf("%s %s: %v", method, url, err)
	}
	data, err := io.ReadAll(resp.Body)
	resp.Body.Close()
	if err != nil {
		t.Fatalf("read %s %s: %v", method, url, err)
	}
	return resp, data
}

// createSession creates a session over HTTP and returns its id.
func createSession(t *testing.T, ts *httptest.Server) string {
	t.Helper()
	resp, data := httpDo(t, "POST", ts.URL+"/api/sessions", `{"model":"fake","provider":"p1"}`)
	if resp.StatusCode != http.StatusCreated {
		t.Fatalf("create session = %d (%s), want 201", resp.StatusCode, data)
	}
	var created struct {
		ID string `json:"id"`
	}
	if err := json.Unmarshal(data, &created); err != nil || created.ID == "" {
		t.Fatalf("create session body = %s (%v), want an id", data, err)
	}
	return created.ID
}

// sessionStateDTO mirrors the GET /api/sessions/{id} payload. Entry fields
// have no JSON tags (session.Entry marshals under its Go field names), so the
// DTO matches them by name — the same wire shape remotecontrol's snapshot
// frames use.
type sessionStateDTO struct {
	Phase    string `json:"phase"`
	Snapshot struct {
		Header struct {
			ID    string `json:"id"`
			Model string `json:"model"`
		} `json:"header"`
		Entries []struct {
			ID       string
			ParentID string
		}
		LeafID string `json:"leafId"`
	} `json:"snapshot"`
}

// waitEntries polls GET /api/sessions/{id} until the persisted tree reaches
// want entries (the prompt run completes asynchronously) and returns the
// final state.
func waitEntries(t *testing.T, url string, want int) sessionStateDTO {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		resp, data := httpDo(t, "GET", url, "")
		if resp.StatusCode == http.StatusOK {
			var st sessionStateDTO
			if err := json.Unmarshal(data, &st); err == nil && len(st.Snapshot.Entries) >= want {
				return st
			}
		}
		time.Sleep(5 * time.Millisecond)
	}
	t.Fatalf("session at %s never reached %d entries", url, want)
	return sessionStateDTO{}
}

func TestModelsCreateAndInputValidation(t *testing.T) {
	ts, _, _, _ := newWebTestServer(t, textTurn("x"))

	// The model catalog rides over HTTP unchanged.
	resp, data := httpDo(t, "GET", ts.URL+"/api/models", "")
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("GET models = %d, want 200", resp.StatusCode)
	}
	var models struct {
		Models []struct {
			Provider string `json:"provider"`
			ID       string `json:"id"`
		} `json:"models"`
	}
	if err := json.Unmarshal(data, &models); err != nil || len(models.Models) == 0 {
		t.Fatalf("models body = %s (%v), want non-empty", data, err)
	}
	if models.Models[0].Provider != "openrouter" || models.Models[0].ID == "" {
		t.Fatalf("first model = %+v, want the catalog head", models.Models[0])
	}

	id := createSession(t, ts)

	// Unknown session → 404 (the store's os.ErrNotExist surfaces as such).
	resp, _ = httpDo(t, "GET", ts.URL+"/api/sessions/does-not-exist", "")
	if resp.StatusCode != http.StatusNotFound {
		t.Fatalf("GET unknown session = %d, want 404", resp.StatusCode)
	}

	// Blank prompt text → 400.
	resp, _ = httpDo(t, "POST", ts.URL+"/api/sessions/"+id+"/prompt", `{"text":"   "}`)
	if resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("blank prompt = %d, want 400", resp.StatusCode)
	}

	// Malformed JSON → 400.
	resp, _ = httpDo(t, "POST", ts.URL+"/api/sessions/"+id+"/prompt", `{not-json`)
	if resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("malformed prompt body = %d, want 400", resp.StatusCode)
	}

	// Wrong method → 405 (the mux patterns enforce it).
	resp, _ = httpDo(t, "POST", ts.URL+"/api/models", "")
	if resp.StatusCode != http.StatusMethodNotAllowed {
		t.Fatalf("POST models = %d, want 405", resp.StatusCode)
	}

	// Steer while idle → 409 (serve.ErrNoActiveRun).
	resp, _ = httpDo(t, "POST", ts.URL+"/api/sessions/"+id+"/steer", `{"text":"x"}`)
	if resp.StatusCode != http.StatusConflict {
		t.Fatalf("steer while idle = %d, want 409", resp.StatusCode)
	}
}

func TestPromptPersistAndContinuation(t *testing.T) {
	ts, _, _, _ := newWebTestServer(t, textTurn("web reply"))
	id := createSession(t, ts)
	base := ts.URL + "/api/sessions/" + id

	// The first prompt is accepted immediately; the run finishes async.
	resp, _ := httpDo(t, "POST", base+"/prompt", `{"text":"hello"}`)
	if resp.StatusCode != http.StatusAccepted {
		t.Fatalf("prompt = %d, want 202", resp.StatusCode)
	}
	st := waitEntries(t, base, 2)
	if st.Phase != "idle" || st.Snapshot.LeafID == "" {
		t.Fatalf("after first run: phase %q leaf %q, want idle/non-empty", st.Phase, st.Snapshot.LeafID)
	}
	if st.Snapshot.Header.ID != id {
		t.Fatalf("snapshot header id = %q, want %q", st.Snapshot.Header.ID, id)
	}
	if st.Snapshot.Entries[0].ParentID != "" || st.Snapshot.Entries[1].ParentID != st.Snapshot.Entries[0].ID {
		t.Fatalf("entry chain broken: %+v", st.Snapshot.Entries)
	}

	// The second prompt lands on the same cached runtime: the conversation
	// continues (4 entries), not a fresh replay of the stored tree.
	resp, _ = httpDo(t, "POST", base+"/prompt", `{"text":"again"}`)
	if resp.StatusCode != http.StatusAccepted {
		t.Fatalf("prompt 2 = %d, want 202", resp.StatusCode)
	}
	st = waitEntries(t, base, 4)

	// The session is listed, most recent first.
	resp, data := httpDo(t, "GET", ts.URL+"/api/sessions", "")
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("GET sessions = %d, want 200", resp.StatusCode)
	}
	var list struct {
		Sessions []struct {
			ID string `json:"id"`
		} `json:"sessions"`
	}
	if err := json.Unmarshal(data, &list); err != nil || len(list.Sessions) != 1 || list.Sessions[0].ID != id {
		t.Fatalf("sessions body = %s (%v), want exactly the created session", data, err)
	}
}

func TestPromptWhileRunningConflictAndAbort(t *testing.T) {
	ts, _, _, _ := newWebTestServer(t, blockingStream())
	id := createSession(t, ts)
	base := ts.URL + "/api/sessions/" + id

	resp, _ := httpDo(t, "POST", base+"/prompt", `{"text":"go"}`)
	if resp.StatusCode != http.StatusAccepted {
		t.Fatalf("prompt = %d, want 202", resp.StatusCode)
	}

	// A second prompt while the run is in flight hits the same cached runtime
	// and is refused (serve.ErrRunActive → 409).
	resp, _ = httpDo(t, "POST", base+"/prompt", `{"text":"again"}`)
	if resp.StatusCode != http.StatusConflict {
		t.Fatalf("prompt while running = %d, want 409", resp.StatusCode)
	}

	// Steering mid-run is accepted.
	resp, _ = httpDo(t, "POST", base+"/steer", `{"text":"mid"}`)
	if resp.StatusCode != http.StatusAccepted {
		t.Fatalf("steer mid-run = %d, want 202", resp.StatusCode)
	}

	// Abort settles the run; the already-appended prompt is still persisted.
	resp, _ = httpDo(t, "POST", base+"/abort", "")
	if resp.StatusCode != http.StatusAccepted {
		t.Fatalf("abort = %d, want 202", resp.StatusCode)
	}
	st := waitEntries(t, base, 1)
	if st.Phase != "idle" {
		t.Fatalf("phase after abort = %q, want idle", st.Phase)
	}
}

func TestSSESnapshotThenEvents(t *testing.T) {
	ts, _, _, _ := newWebTestServer(t, textTurn("sse reply"))
	id := createSession(t, ts)
	base := ts.URL + "/api/sessions/" + id

	frames := make(chan sseFrame, 64)
	sseCtx, cancel := context.WithCancel(context.Background())
	go readSSE(sseCtx, base+"/events", frames)

	// The stream opens with the snapshot frame: an empty session.
	f := nextFrame(t, frames)
	if f.event != "snapshot" {
		t.Fatalf("first frame = %q, want snapshot", f.event)
	}
	var snap struct {
		Entries []json.RawMessage `json:"entries"`
		LeafID  string            `json:"leafId"`
	}
	if err := json.Unmarshal([]byte(f.data), &snap); err != nil || len(snap.Entries) != 0 || snap.LeafID != "" {
		t.Fatalf("snapshot frame data = %s (%v), want an empty session", f.data, err)
	}

	// Prompt: the run's events flow through the stream in wire order.
	resp, _ := httpDo(t, "POST", base+"/prompt", `{"text":"hi"}`)
	if resp.StatusCode != http.StatusAccepted {
		t.Fatalf("prompt = %d, want 202", resp.StatusCode)
	}

	var kinds []string
	for {
		f := nextFrame(t, frames)
		if f.event != "agent_event" {
			t.Fatalf("frame after snapshot = %q, want agent_event", f.event)
		}
		var env struct {
			Type string `json:"type"`
		}
		if err := json.Unmarshal([]byte(f.data), &env); err != nil || env.Type == "" {
			t.Fatalf("event frame data = %s (%v), want an envelope with a type", f.data, err)
		}
		kinds = append(kinds, env.Type)
		if env.Type == "agent_end" {
			break
		}
	}
	cancel()

	// The full run rode the stream: same sequence the serve layer emits.
	want := []string{"agent_start", "turn_start", "message_end", "turn_end", "telemetry", "agent_end"}
	if !equalStrings(kinds, want) {
		t.Fatalf("event kinds = %v, want %v", kinds, want)
	}
}

func TestSetModelAndThinkingReachTheFactory(t *testing.T) {
	ts, _, headers, thinkings := newWebTestServer(t, textTurn("ok"))
	id := createSession(t, ts)
	base := ts.URL + "/api/sessions/" + id

	resp, _ := httpDo(t, "POST", base+"/model", `{"model":"m2"}`)
	if resp.StatusCode != http.StatusNoContent {
		t.Fatalf("set model = %d, want 204", resp.StatusCode)
	}
	resp, _ = httpDo(t, "POST", base+"/thinking", `{"level":"high"}`)
	if resp.StatusCode != http.StatusNoContent {
		t.Fatalf("set thinking = %d, want 204", resp.StatusCode)
	}
	resp, _ = httpDo(t, "POST", base+"/model", `{"model":""}`)
	if resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("set empty model = %d, want 400", resp.StatusCode)
	}

	resp, _ = httpDo(t, "POST", base+"/prompt", `{"text":"go"}`)
	if resp.StatusCode != http.StatusAccepted {
		t.Fatalf("prompt = %d, want 202", resp.StatusCode)
	}
	st := waitEntries(t, base, 2)
	if len(*headers) != 1 || (*headers)[0].Model != "m2" {
		t.Fatalf("factory saw models %v, want m2", *headers)
	}
	if len(*thinkings) != 1 || (*thinkings)[0] != "high" {
		t.Fatalf("factory saw thinkings %v, want high", *thinkings)
	}
	if st.Snapshot.Header.Model != "m2" {
		t.Fatalf("persisted header model = %q, want m2", st.Snapshot.Header.Model)
	}
}

func TestDeleteDisposesHandleNotData(t *testing.T) {
	ts, _, _, _ := newWebTestServer(t, textTurn("persisted"))
	id := createSession(t, ts)
	base := ts.URL + "/api/sessions/" + id

	resp, _ := httpDo(t, "POST", base+"/prompt", `{"text":"hello"}`)
	if resp.StatusCode != http.StatusAccepted {
		t.Fatalf("prompt = %d, want 202", resp.StatusCode)
	}
	waitEntries(t, base, 2)

	resp, _ = httpDo(t, "DELETE", base, "")
	if resp.StatusCode != http.StatusNoContent {
		t.Fatalf("delete = %d, want 204", resp.StatusCode)
	}
	// Idempotent.
	resp, _ = httpDo(t, "DELETE", base, "")
	if resp.StatusCode != http.StatusNoContent {
		t.Fatalf("delete again = %d, want 204", resp.StatusCode)
	}

	// The data survived the dispose; a GET reopens the session from the store.
	st := waitEntries(t, base, 2)
	if st.Phase != "idle" {
		t.Fatalf("phase after reopen = %q, want idle", st.Phase)
	}
}

// TestSharedServiceBoundary is the phase-4 payoff: one serve.Service feeds the
// web frontend and any other consumer (remotecontrol, headless) at once. A
// prompt driven over HTTP lands where a second frontend reopens it, and a
// prompt from that second frontend is visible back through the web API.
func TestSharedServiceBoundary(t *testing.T) {
	ts, svc, _, _ := newWebTestServer(t, textTurn("shared"))
	id := createSession(t, ts)
	base := ts.URL + "/api/sessions/" + id

	resp, _ := httpDo(t, "POST", base+"/prompt", `{"text":"from the web"}`)
	if resp.StatusCode != http.StatusAccepted {
		t.Fatalf("web prompt = %d, want 202", resp.StatusCode)
	}
	waitEntries(t, base, 2)

	// A second frontend opens the same session through the same Service —
	// no web-specific path, the serve boundary is the whole contract.
	ctx := context.Background()
	rt, err := svc.OpenSession(ctx, id)
	if err != nil {
		t.Fatalf("second frontend OpenSession: %v", err)
	}
	snap, err := rt.Snapshot(ctx)
	if err != nil || len(snap.Entries) != 2 {
		t.Fatalf("second frontend snapshot = %d entries (%v), want 2", len(snap.Entries), err)
	}

	// It continues the conversation; its run lands on disk through the same
	// store. The web cache holds its own handle, and a handle does not see
	// another frontend's writes (serve's model: Snapshot reads the handle's
	// cache, refreshed only by that handle's own runs) — so the web side sees
	// the full conversation after dropping the stale handle (DELETE) and
	// letting GET reopen the session.
	if err := rt.Prompt(ctx, "from the other frontend"); err != nil {
		t.Fatalf("second frontend Prompt: %v", err)
	}
	if err := rt.WaitIdle(ctx); err != nil {
		t.Fatalf("second frontend WaitIdle: %v", err)
	}

	resp, _ = httpDo(t, "DELETE", base, "")
	if resp.StatusCode != http.StatusNoContent {
		t.Fatalf("delete stale handle = %d, want 204", resp.StatusCode)
	}
	st := waitEntries(t, base, 4)
	if st.Phase != "idle" {
		t.Fatalf("phase after reopen = %q, want idle", st.Phase)
	}
}

// sseFrame is one parsed server-sent-events frame.
type sseFrame struct {
	event string
	data  string
}

// readSSE streams the SSE endpoint, parsing frames onto the channel until the
// context ends or the stream closes.
func readSSE(ctx context.Context, url string, frames chan<- sseFrame) {
	req, err := http.NewRequestWithContext(ctx, "GET", url, nil)
	if err != nil {
		return
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return
	}
	defer resp.Body.Close()
	scanner := bufio.NewScanner(resp.Body)
	scanner.Buffer(make([]byte, 0, 64*1024), 4*1024*1024)
	var event, data string
	for scanner.Scan() {
		line := scanner.Text()
		switch {
		case line == "":
			if event != "" || data != "" {
				frames <- sseFrame{event: event, data: data}
			}
			event, data = "", ""
		case strings.HasPrefix(line, ":"):
			// heartbeat comment line
		case strings.HasPrefix(line, "event: "):
			event = strings.TrimPrefix(line, "event: ")
		case strings.HasPrefix(line, "data: "):
			data = strings.TrimPrefix(line, "data: ")
		}
	}
}

func nextFrame(t *testing.T, frames <-chan sseFrame) sseFrame {
	t.Helper()
	select {
	case f := <-frames:
		return f
	case <-time.After(10 * time.Second):
		t.Fatal("timed out waiting for an SSE frame")
		return sseFrame{}
	}
}

func equalStrings(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
