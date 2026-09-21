// This file implements the GitLab merge-request review webhook, mirroring
// githubreview.go (issue #567)'s isolation contract for a local / self-hosted
// GitLab: an opt-in, isolated HTTP endpoint that turns MR events into
// read-only review sessions.
//
// Design, the same contract adapted to GitLab's wire protocol:
//
//   - The endpoint mounts exactly one route (POST /gitlab); every other path
//     and method is a bare 404, so the listener exposes nothing else even
//     when a TLS reverse proxy forwards a public URL to it.
//   - Authenticity: GitLab authenticates webhooks with a shared token in
//     X-Gitlab-Token (not an HMAC), so the server constant-time compares the
//     header against the configured token. The token is supplied indirectly
//     (an env-var name resolved by the CLI), never inline in config.
//   - Replay protection: deliveries are deduplicated by the X-Gitlab-Event-UUID
//     id (a bounded, TTL-pruned cache); duplicates answer 202 and re-run
//     nothing.
//   - Event filter: only merge_request events act. The review-ready moments
//     are a non-draft MR being opened, and a draft MR being marked ready
//     (action "update" with changes.draft flipping true→false) — the GitLab
//     counterpart of GitHub's ready_for_review. Everything else answers 202
//     and is ignored. An optional project filter ("group/name") restricts
//     which repository triggers review.
//   - Delivery: on a match the server creates a persisted session carrying the
//     review prompt, then hands the request to the injected Runner in its own
//     goroutine, answering 200 immediately (GitLab expects a fast response).
//
// The review run is read-only by construction: its tool set is the read/grep/
// find subset shared with the GitHub endpoint, so the model can inspect the
// diff but cannot modify the workspace or execute commands.
package webhook

import (
	"context"
	"crypto/subtle"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"sync"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/agenttool"
	"github.com/smallnest/pigo/internal/cli/run"
	"github.com/smallnest/pigo/internal/provider"
	"github.com/smallnest/pigo/internal/runtime"
	"github.com/smallnest/pigo/internal/session"
)

// MRReviewRequest is one accepted merge-request review event, handed to
// MRRunner after the review session has been created and persisted.
type MRReviewRequest struct {
	Project      string // "group/name"
	MRNumber     int    // the MR's project-scoped iid (!N in GitLab notation)
	MRTitle      string
	MRURL        string
	SourceBranch string
	TargetBranch string
	SessionID    string // the persisted review session the prompt was written to
	Prompt       string // the read-only review prompt (already the session's first turn)
}

// MRRunner executes one MR review request. It runs on its own goroutine, off
// the HTTP request path. The default runner (GitLabServer.DefaultRunner)
// drives a real read-only agent run; tests inject fakes.
type MRRunner func(ctx context.Context, req MRReviewRequest) error

// GitLabServer is the isolated GitLab MR review webhook endpoint.
type GitLabServer struct {
	// Token is the shared GitLab webhook token (X-Gitlab-Token); required.
	Token string
	// Project, when non-empty, restricts review to "group/name"; empty
	// accepts every project's events.
	Project string
	// Store persists review sessions.
	Store session.Store
	// Workspace is the directory the review runs in (and sessions attribute).
	Workspace string
	// Model / ProviderName / SysPrompt stamp the created session's header and
	// feed the default runner.
	Model        string
	ProviderName string
	SysPrompt    string
	// Provider is the resolved provider the default runner streams from.
	Provider provider.Provider
	// Runner is called (in its own goroutine) for each accepted event. Required.
	Runner MRRunner
	// Now is the clock for delivery-cache pruning and session stamps; tests pin it.
	Now func() time.Time

	seenMu sync.Mutex
	seen   map[string]time.Time
}

func (s *GitLabServer) clock() time.Time {
	if s.Now != nil {
		return s.Now()
	}
	return time.Now()
}

// MRReviewPrompt builds the read-only review prompt for one MR event.
func MRReviewPrompt(req MRReviewRequest) string {
	return fmt.Sprintf(`Review merge request !%d in %s: "%s" (%s).

You are in READ-ONLY review mode: the read/grep/find tools are available, but do
not modify files, run commands, or attempt to switch tools. Inspect the working
tree (which holds the MR's source branch checked out) and produce:
1. A one-paragraph summary of what the MR changes.
2. Findings: correctness risks, edge cases, and security concerns, each with the
   file and line you base it on.
3. Suggested follow-ups, ordered by importance.`, req.MRNumber, req.Project, req.MRTitle, req.MRURL)
}

// gitlabMergeRequestEvent is the minimal merge_request webhook payload shape.
// Draft appears twice on purpose: newer GitLab sends "draft", older sends the
// synonymous "work_in_progress" — either being set marks the MR a draft.
type gitlabMergeRequestEvent struct {
	ObjectKind string `json:"object_kind"`
	Project    struct {
		PathWithNamespace string `json:"path_with_namespace"`
	} `json:"project"`
	ObjectAttributes struct {
		IID            int    `json:"iid"`
		Title          string `json:"title"`
		Action         string `json:"action"`
		Draft          bool   `json:"draft"`
		WorkInProgress bool   `json:"work_in_progress"`
		SourceBranch   string `json:"source_branch"`
		TargetBranch   string `json:"target_branch"`
		URL            string `json:"url"`
	} `json:"object_attributes"`
	Changes struct {
		Draft struct {
			Previous bool `json:"previous"`
			Current  bool `json:"current"`
		} `json:"draft"`
	} `json:"changes"`
}

// reviewReady reports whether the event marks the MR ready for review: either
// a non-draft MR being opened, or a draft being marked ready via an update
// (changes.draft true→false) — the counterpart of GitHub's ready_for_review.
func (ev *gitlabMergeRequestEvent) reviewReady() bool {
	draft := ev.ObjectAttributes.Draft || ev.ObjectAttributes.WorkInProgress
	switch ev.ObjectAttributes.Action {
	case "open":
		return !draft
	case "update":
		return ev.Changes.Draft.Previous && !ev.Changes.Draft.Current
	default:
		return false
	}
}

// Handler returns the isolated HTTP handler: exactly POST /gitlab; every other
// method/path is a bare 404 so the listener exposes nothing else.
func (s *GitLabServer) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("/gitlab", s.handleGitLab)
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/gitlab" && r.Method == http.MethodPost {
			mux.ServeHTTP(w, r)
			return
		}
		http.NotFound(w, r)
	})
}

// handleGitLab verifies, filters, and dispatches one webhook delivery.
func (s *GitLabServer) handleGitLab(w http.ResponseWriter, r *http.Request) {
	if s.Token == "" {
		http.Error(w, "webhook token not configured", http.StatusServiceUnavailable)
		return
	}
	body, err := io.ReadAll(io.LimitReader(r.Body, maxWebhookBody))
	if err != nil {
		http.Error(w, "unreadable body", http.StatusBadRequest)
		return
	}
	if !verifyToken(r.Header.Get("X-Gitlab-Token"), s.Token) {
		http.Error(w, "invalid token", http.StatusUnauthorized)
		return
	}
	delivery := r.Header.Get("X-Gitlab-Event-UUID")
	if delivery == "" || !s.rememberDelivery(delivery) {
		// A missing delivery id or a replay: acknowledge without acting.
		w.WriteHeader(http.StatusAccepted)
		fmt.Fprintln(w, "duplicate or unidentifiable delivery; ignored")
		return
	}
	if event := r.Header.Get("X-Gitlab-Event"); event != "Merge Request Hook" {
		w.WriteHeader(http.StatusAccepted)
		fmt.Fprintf(w, "event %q ignored; only merge request hooks are handled\n", event)
		return
	}

	var ev gitlabMergeRequestEvent
	if err := json.Unmarshal(body, &ev); err != nil {
		http.Error(w, "malformed payload", http.StatusBadRequest)
		return
	}
	if ev.ObjectKind != "merge_request" || !ev.reviewReady() ||
		(s.Project != "" && ev.Project.PathWithNamespace != s.Project) {
		w.WriteHeader(http.StatusAccepted)
		fmt.Fprintf(w, "merge_request action %q for %q ignored\n", ev.ObjectAttributes.Action, ev.Project.PathWithNamespace)
		return
	}

	req := MRReviewRequest{
		Project:      ev.Project.PathWithNamespace,
		MRNumber:     ev.ObjectAttributes.IID,
		MRTitle:      ev.ObjectAttributes.Title,
		MRURL:        ev.ObjectAttributes.URL,
		SourceBranch: ev.ObjectAttributes.SourceBranch,
		TargetBranch: ev.ObjectAttributes.TargetBranch,
		Prompt: MRReviewPrompt(MRReviewRequest{
			Project:  ev.Project.PathWithNamespace,
			MRNumber: ev.ObjectAttributes.IID,
			MRTitle:  ev.ObjectAttributes.Title,
			MRURL:    ev.ObjectAttributes.URL,
		}),
	}
	if err := s.createReviewSession(&req); err != nil {
		http.Error(w, "session creation failed: "+err.Error(), http.StatusInternalServerError)
		return
	}
	// The run outlives the HTTP exchange: r.Context() is canceled the moment
	// this handler returns, so the runner gets a detached context with a
	// generous ceiling instead.
	runCtx, cancel := context.WithTimeout(context.Background(), reviewRunTimeout)
	go func() {
		defer cancel()
		_ = s.Runner(runCtx, req)
	}()
	w.WriteHeader(http.StatusOK)
	fmt.Fprintf(w, "review session %s created for %s!%d\n", req.SessionID, req.Project, req.MRNumber)
}

// createReviewSession persists the review session with the review prompt as its
// first (user) turn, so /resume can continue it and the run's input is durable
// before the runner starts.
func (s *GitLabServer) createReviewSession(req *MRReviewRequest) error {
	now := s.clock()
	header := session.SessionHeader{
		ID:           session.NewID(now),
		CreatedAt:    now,
		UpdatedAt:    now,
		Model:        s.Model,
		Provider:     s.ProviderName,
		SystemPrompt: s.SysPrompt,
		Cwd:          s.Workspace,
	}
	msgs := agentcore.MessageList{agentcore.UserMessage{
		RoleField: agentcore.RoleUser,
		Content:   agentcore.ContentList{agentcore.NewTextContent(req.Prompt)},
	}}
	if err := s.Store.Save(header, msgs); err != nil {
		return err
	}
	req.SessionID = header.ID
	return nil
}

// rememberDelivery records a delivery id, pruning expired entries first, and
// reports whether it had not been seen before.
func (s *GitLabServer) rememberDelivery(id string) bool {
	s.seenMu.Lock()
	defer s.seenMu.Unlock()
	if s.seen == nil {
		s.seen = make(map[string]time.Time)
	}
	now := s.clock()
	for k, at := range s.seen {
		if now.Sub(at) > deliveryTTL {
			delete(s.seen, k)
		}
	}
	if _, dup := s.seen[id]; dup {
		return false
	}
	s.seen[id] = now
	return true
}

// verifyToken constant-time compares the X-Gitlab-Token header against the
// configured token. GitLab authenticates webhooks with a shared token rather
// than an HMAC, so the compare itself is the whole authenticity check.
func verifyToken(header, token string) bool {
	return subtle.ConstantTimeCompare([]byte(header), []byte(token)) == 1
}

// DefaultRunner drives a real read-only agent run for one MR review request:
// the session (already persisted with the prompt) is resumed into context, the
// run streams against the server's provider with only the read-only tools, and
// the produced messages are appended back to the same session.
func (s *GitLabServer) DefaultRunner() MRRunner {
	return func(ctx context.Context, req MRReviewRequest) error {
		_, entries, err := s.Store.LoadEntries(req.SessionID)
		if err != nil {
			return err
		}
		msgs := make(agentcore.MessageList, len(entries))
		for i, e := range entries {
			msgs[i] = e.Message
		}
		agentCtx := &agentcore.AgentContext{Messages: msgs, Tools: ReadOnlyTools(s.Workspace)}
		creds := provider.NewCredentialStore(nil)
		reg := agenttool.NewToolRegistry()
		for _, t := range agentCtx.Tools {
			_ = reg.Register(t)
		}
		cfg := run.NewConfig(s.Model, s.ProviderName, "", s.Provider, creds, reg, nil, nil)
		stream := runtime.StartRun(ctx, agentCtx, cfg)
		if _, err := runtime.DrainStream(ctx, stream, runtime.StreamHandler{}); err != nil {
			return err
		}
		if len(agentCtx.Messages) > len(msgs) {
			return s.Store.Append(req.SessionID, s.clock().UTC(), agentCtx.Messages[len(msgs):])
		}
		return nil
	}
}
