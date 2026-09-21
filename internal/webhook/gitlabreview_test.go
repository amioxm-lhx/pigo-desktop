// Tests for the GitLab merge-request review webhook: token verification,
// replay protection, event filtering (including the draft semantics),
// the isolated-route contract, the end-to-end path from a valid delivery to
// a persisted read-only review session, and the default runner.
package webhook

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/session"
)

const testMRPayload = `{"object_kind":"merge_request","event_type":"merge_request","project":{"path_with_namespace":"smallnest/pigo","web_url":"http://gitlab.local/smallnest/pigo"},"object_attributes":{"iid":5,"title":"Add scheduler","state":"opened","action":"open","draft":false,"source_branch":"feat/scheduler","target_branch":"main","url":"http://gitlab.local/smallnest/pigo/-/merge_requests/5"}}`

// testDraftReadyPayload is the other review-ready moment: a draft MR being
// marked ready (action "update", changes.draft true→false).
const testDraftReadyPayload = `{"object_kind":"merge_request","event_type":"merge_request","project":{"path_with_namespace":"smallnest/pigo"},"object_attributes":{"iid":7,"title":"Finish scheduler","state":"opened","action":"update","draft":false,"source_branch":"feat/draft","target_branch":"main","url":"http://gitlab.local/smallnest/pigo/-/merge_requests/7"},"changes":{"draft":{"previous":true,"current":false}}}`

// tokenedRequest builds a POST /gitlab request carrying the token header.
func tokenedRequest(t *testing.T, token, body, delivery, event string) *http.Request {
	t.Helper()
	req, err := http.NewRequest(http.MethodPost, "/gitlab", strings.NewReader(body))
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set("X-Gitlab-Token", token)
	req.Header.Set("X-Gitlab-Event", event)
	req.Header.Set("X-Gitlab-Event-UUID", delivery)
	return req
}

type recordingMRRunner struct {
	mu    sync.Mutex
	calls []MRReviewRequest
}

func (r *recordingMRRunner) run(_ context.Context, req MRReviewRequest) error {
	r.mu.Lock()
	r.calls = append(r.calls, req)
	r.mu.Unlock()
	return nil
}

func (r *recordingMRRunner) waitCalls(t *testing.T, n int) []MRReviewRequest {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		r.mu.Lock()
		got := len(r.calls)
		r.mu.Unlock()
		if got >= n {
			r.mu.Lock()
			defer r.mu.Unlock()
			return r.calls
		}
		time.Sleep(5 * time.Millisecond)
	}
	t.Fatalf("runner received %d calls, want %d", len(r.calls), n)
	return nil
}

func newGitLabTestServer(t *testing.T) (*GitLabServer, *recordingMRRunner) {
	t.Helper()
	store, err := session.NewStore(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	runner := &recordingMRRunner{}
	srv := &GitLabServer{
		Token:  "test-token",
		Store:  store,
		Runner: runner.run,
	}
	return srv, runner
}

// TestGitLabWebhookRejectsBadToken verifies missing, and wrong tokens are
// rejected with 401 and never reach the runner.
func TestGitLabWebhookRejectsBadToken(t *testing.T) {
	srv, runner := newGitLabTestServer(t)
	h := srv.Handler()

	// No token header.
	req := tokenedRequest(t, srv.Token, testMRPayload, "d-1", "Merge Request Hook")
	req.Header.Del("X-Gitlab-Token")
	resp := httptest.NewRecorder()
	h.ServeHTTP(resp, req)
	if resp.Code != http.StatusUnauthorized {
		t.Fatalf("missing token = %d, want 401", resp.Code)
	}

	// Wrong token.
	resp = httptest.NewRecorder()
	h.ServeHTTP(resp, tokenedRequest(t, "other-token", testMRPayload, "d-2", "Merge Request Hook"))
	if resp.Code != http.StatusUnauthorized {
		t.Fatalf("wrong token = %d, want 401", resp.Code)
	}

	// Empty token while one is configured.
	resp = httptest.NewRecorder()
	h.ServeHTTP(resp, tokenedRequest(t, "", testMRPayload, "d-3", "Merge Request Hook"))
	if resp.Code != http.StatusUnauthorized {
		t.Fatalf("empty token = %d, want 401", resp.Code)
	}

	if calls := runner.waitCalls(t, 0); len(calls) != 0 {
		t.Fatalf("runner called for %d rejected requests", len(calls))
	}
}

// TestGitLabWebhookReplayProtection verifies the same event UUID acts once
// and a missing UUID never acts.
func TestGitLabWebhookReplayProtection(t *testing.T) {
	srv, runner := newGitLabTestServer(t)
	h := srv.Handler()

	resp := httptest.NewRecorder()
	h.ServeHTTP(resp, tokenedRequest(t, srv.Token, testMRPayload, "delivery-1", "Merge Request Hook"))
	if resp.Code != http.StatusOK {
		t.Fatalf("first delivery = %d, want 200", resp.Code)
	}

	// Replay: same UUID, acknowledged but not acted on.
	resp = httptest.NewRecorder()
	h.ServeHTTP(resp, tokenedRequest(t, srv.Token, testMRPayload, "delivery-1", "Merge Request Hook"))
	if resp.Code != http.StatusAccepted {
		t.Fatalf("replay = %d, want 202", resp.Code)
	}

	// Missing UUID: acknowledged but not acted on.
	resp = httptest.NewRecorder()
	h.ServeHTTP(resp, tokenedRequest(t, srv.Token, testMRPayload, "", "Merge Request Hook"))
	if resp.Code != http.StatusAccepted {
		t.Fatalf("missing delivery id = %d, want 202", resp.Code)
	}

	if calls := runner.waitCalls(t, 1); len(calls) != 1 {
		t.Fatalf("runner called %d times, want exactly 1", len(calls))
	}
}

// TestGitLabWebhookEventFiltering verifies only review-ready merge_request
// events for the configured project act; everything else is 202-ignored.
func TestGitLabWebhookEventFiltering(t *testing.T) {
	srv, runner := newGitLabTestServer(t)
	srv.Project = "smallnest/pigo"
	h := srv.Handler()

	cases := []struct {
		name    string
		event   string
		payload string
	}{
		{"non merge request event", "Push Hook", testMRPayload},
		{"non ready action", "Merge Request Hook", strings.Replace(testMRPayload, `"action":"open"`, `"action":"close"`, 1)},
		{"draft MR opened", "Merge Request Hook", strings.Replace(testMRPayload, `"draft":false`, `"draft":true`, 1)},
		{"project filter mismatch", "Merge Request Hook", strings.Replace(testMRPayload, `"smallnest/pigo"`, `"other/group"`, 1)},
	}
	for _, tc := range cases {
		resp := httptest.NewRecorder()
		h.ServeHTTP(resp, tokenedRequest(t, srv.Token, tc.payload, "d-"+tc.name, tc.event))
		if resp.Code != http.StatusAccepted {
			t.Errorf("%s = %d, want 202", tc.name, resp.Code)
		}
	}

	// The accepted event still passes the project filter.
	resp := httptest.NewRecorder()
	h.ServeHTTP(resp, tokenedRequest(t, srv.Token, testMRPayload, "d-accept", "Merge Request Hook"))
	if resp.Code != http.StatusOK {
		t.Fatalf("accepted event = %d, want 200", resp.Code)
	}
	if calls := runner.waitCalls(t, 1); len(calls) != 1 || calls[0].Project != "smallnest/pigo" {
		t.Fatalf("runner calls = %+v, want one smallnest/pigo review", calls)
	}
}

// TestGitLabDraftToReadyTriggersReview verifies the second review-ready
// moment: a draft being marked ready (action update, changes.draft true→false)
// creates a review session too.
func TestGitLabDraftToReadyTriggersReview(t *testing.T) {
	srv, runner := newGitLabTestServer(t)
	h := srv.Handler()

	resp := httptest.NewRecorder()
	h.ServeHTTP(resp, tokenedRequest(t, srv.Token, testDraftReadyPayload, "d-draft-ready", "Merge Request Hook"))
	if resp.Code != http.StatusOK {
		t.Fatalf("draft→ready delivery = %d, want 200", resp.Code)
	}
	calls := runner.waitCalls(t, 1)
	if calls[0].MRNumber != 7 || calls[0].MRTitle != "Finish scheduler" || calls[0].SourceBranch != "feat/draft" {
		t.Fatalf("runner request = %+v, want the parsed draft→ready fields", calls[0])
	}
}

// TestGitLabWebhookRouteIsolation verifies the isolated-endpoint contract:
// only POST /gitlab exists; every other path or method is a bare 404.
func TestGitLabWebhookRouteIsolation(t *testing.T) {
	srv, _ := newGitLabTestServer(t)
	h := srv.Handler()

	for _, tc := range []struct{ method, path string }{
		{http.MethodGet, "/gitlab"},
		{http.MethodGet, "/"},
		{http.MethodPost, "/"},
		{http.MethodPost, "/gitlab/extra"},
		{http.MethodPost, "/api"},
		{http.MethodPost, "/github"}, // the sibling endpoint is a foreign path here
	} {
		req, err := http.NewRequest(tc.method, tc.path, nil)
		if err != nil {
			t.Fatal(err)
		}
		resp := httptest.NewRecorder()
		h.ServeHTTP(resp, req)
		if resp.Code != http.StatusNotFound {
			t.Errorf("%s %s = %d, want 404", tc.method, tc.path, resp.Code)
		}
	}
}

// TestGitLabWebhookCreatesReadOnlyReviewSession is the end-to-end proof: one
// valid delivery creates a persisted session whose first turn is the read-only
// MR review prompt, and the runner receives the request with the session id.
func TestGitLabWebhookCreatesReadOnlyReviewSession(t *testing.T) {
	srv, runner := newGitLabTestServer(t)
	h := srv.Handler()

	resp := httptest.NewRecorder()
	h.ServeHTTP(resp, tokenedRequest(t, srv.Token, testMRPayload, "delivery-e2e", "Merge Request Hook"))
	if resp.Code != http.StatusOK {
		t.Fatalf("delivery = %d, want 200", resp.Code)
	}

	calls := runner.waitCalls(t, 1)
	req := calls[0]
	if req.MRNumber != 5 || req.MRTitle != "Add scheduler" ||
		req.SourceBranch != "feat/scheduler" || req.TargetBranch != "main" {
		t.Fatalf("runner request = %+v, want the parsed MR fields", req)
	}
	if req.SessionID == "" {
		t.Fatal("runner request carries no session id")
	}

	// The session is durable before the runner starts, with the review prompt
	// as its first user turn.
	header, entries, err := srv.Store.LoadEntries(req.SessionID)
	if err != nil {
		t.Fatalf("LoadEntries: %v", err)
	}
	if len(entries) != 1 {
		t.Fatalf("session entries = %d, want 1", len(entries))
	}
	u, ok := entries[0].Message.(agentcore.UserMessage)
	if !ok {
		t.Fatalf("first entry = %T, want user message", entries[0].Message)
	}
	text := agentcore.ContentToText(u.Content)
	for _, want := range []string{"READ-ONLY", "!5", "Add scheduler"} {
		if !strings.Contains(text, want) {
			t.Errorf("prompt missing %q:\n%s", want, text)
		}
	}
	if header.ID != req.SessionID {
		t.Errorf("header.ID = %q, want %q", header.ID, req.SessionID)
	}

	// The tool set is read-only: exactly read/grep/find (shared with the
	// GitHub endpoint).
	names := map[string]bool{}
	for _, t2 := range ReadOnlyTools("/") {
		names[t2.Name()] = true
	}
	if len(names) != 3 || !names["read"] || !names["grep"] || !names["find"] {
		t.Fatalf("ReadOnlyTools = %v, want exactly read/grep/find", names)
	}
}

// TestDefaultMRRunnerPersistsReply verifies the default runner resumes the
// MR review session, runs the loop read-only, and appends the reply to the
// same session file.
func TestDefaultMRRunnerPersistsReply(t *testing.T) {
	srv, _ := newGitLabTestServer(t)
	prov := &fakeStreamProvider{}
	srv.Provider = prov
	srv.Model = "fake"
	srv.ProviderName = "fake"
	runner := srv.DefaultRunner()

	req := MRReviewRequest{Project: "smallnest/pigo", MRNumber: 5, MRTitle: "Add scheduler",
		Prompt: MRReviewPrompt(MRReviewRequest{Project: "smallnest/pigo", MRNumber: 5, MRTitle: "Add scheduler"})}
	if err := srv.createReviewSession(&req); err != nil {
		t.Fatalf("createReviewSession: %v", err)
	}
	if err := runner(context.Background(), req); err != nil {
		t.Fatalf("runner: %v", err)
	}

	_, entries, err := srv.Store.LoadEntries(req.SessionID)
	if err != nil {
		t.Fatalf("LoadEntries: %v", err)
	}
	if len(entries) != 2 {
		t.Fatalf("session entries = %d, want 2 (prompt + reply)", len(entries))
	}
	a, ok := entries[1].Message.(agentcore.AssistantMessage)
	if !ok || agentcore.ContentToText(a.Content) != "review complete" {
		t.Fatalf("second entry = %T %+v, want the assistant reply", entries[1].Message, entries[1].Message)
	}
	if prov.calls != 1 {
		t.Errorf("provider called %d times, want 1", prov.calls)
	}
}
