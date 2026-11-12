package desktop

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/im"
)

// fakeApp is a stand-in for the control endpoint. Each test installs the
// handler it needs; the auth gate and envelope shape are shared because they are
// what the real endpoint guarantees.
func fakeApp(t *testing.T, token string, handle func(op string, args map[string]any) (any, *Error)) *Client {
	t.Helper()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/invoke" {
			http.NotFound(w, r)
			return
		}
		if got := r.Header.Get("Authorization"); got != "Bearer "+token {
			w.WriteHeader(http.StatusUnauthorized)
			_, _ = io.WriteString(w, `{"ok":false,"error":{"code":"UNAUTHORIZED","message":"no"}}`)
			return
		}
		body, _ := io.ReadAll(r.Body)
		var req struct {
			Op   string         `json:"op"`
			Args map[string]any `json:"args"`
		}
		if err := json.Unmarshal(body, &req); err != nil {
			t.Errorf("client sent invalid JSON: %v", err)
			return
		}
		data, failure := handle(req.Op, req.Args)
		if failure != nil {
			_ = json.NewEncoder(w).Encode(map[string]any{"ok": false, "error": failure})
			return
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"ok": true, "data": data})
	}))
	t.Cleanup(srv.Close)
	return NewClient(srv.URL, token)
}

func TestInvokeRejectsWrongToken(t *testing.T) {
	c := fakeApp(t, "right", func(string, map[string]any) (any, *Error) { return map[string]any{}, nil })
	// Same endpoint, wrong secret: the app must refuse, not answer.
	c.Token = "wrong"
	_, err := c.Invoke(context.Background(), "session/list", nil)
	var typed *Error
	if !errors.As(err, &typed) || typed.Code != "UNAUTHORIZED" {
		t.Fatalf("want UNAUTHORIZED, got %v", err)
	}
}

func TestCreateSessionUsesProjectAndReturnsID(t *testing.T) {
	var sawArgs map[string]any
	c := fakeApp(t, "t", func(op string, args map[string]any) (any, *Error) {
		if op != "session/create" {
			t.Fatalf("unexpected op %q", op)
		}
		sawArgs = args
		return map[string]any{"session": map[string]any{"id": "s-42"}}, nil
	})
	sess, err := c.CreateSession(context.Background(), im.SessionSpec{Cwd: "/repo"})
	if err != nil {
		t.Fatalf("CreateSession: %v", err)
	}
	if sess.ID() != "s-42" {
		t.Fatalf("id = %q, want s-42", sess.ID())
	}
	if sawArgs["projectPath"] != "/repo" {
		t.Fatalf("projectPath not forwarded: %v", sawArgs)
	}
}

func TestOpenSessionReportsDeletedSession(t *testing.T) {
	c := fakeApp(t, "t", func(op string, _ map[string]any) (any, *Error) {
		return map[string]any{"session": nil}, nil
	})
	if _, err := c.OpenSession(context.Background(), "gone"); err == nil {
		t.Fatal("a deleted session must not open silently")
	}
}

func TestPromptMapsConflictToErrRunActive(t *testing.T) {
	c := fakeApp(t, "t", func(op string, _ map[string]any) (any, *Error) {
		return nil, &Error{Code: "CONFLICT", Message: "session is running"}
	})
	sess := &session{c: c, id: "s1"}
	err := sess.Prompt(context.Background(), "hi")
	if !errors.Is(err, im.ErrRunActive) {
		t.Fatalf("want ErrRunActive, got %v", err)
	}
}

func TestListSessionsReadsCamelCaseWire(t *testing.T) {
	c := fakeApp(t, "t", func(string, map[string]any) (any, *Error) {
		return map[string]any{"sessions": []map[string]any{{
			"id":          "s1",
			"title":       "重构 router",
			"projectPath": "/repo",
			"updatedAt":   "2026-11-12T02:20:00Z",
		}}}, nil
	})
	rows, err := c.ListSessions(context.Background())
	if err != nil {
		t.Fatalf("ListSessions: %v", err)
	}
	if len(rows) != 1 || rows[0].ID != "s1" || rows[0].Title != "重构 router" || rows[0].Cwd != "/repo" {
		t.Fatalf("bad row: %+v", rows)
	}
	if rows[0].UpdatedAt.IsZero() {
		t.Fatal("updatedAt was not parsed")
	}
}

// TestStreamSynthesizesProgressAndFinalAnswer covers the contract Router relies
// on: narration arrives as MessageEnd, the run closes with AgentEnd, and the
// text is the persisted assistant message rather than anything stitched from
// deltas.
func TestStreamSynthesizesProgressAndFinalAnswer(t *testing.T) {
	var polls atomic.Int32
	c := fakeApp(t, "t", func(op string, _ map[string]any) (any, *Error) {
		switch op {
		case "agent/status":
			n := polls.Add(1)
			if n <= 2 {
				return map[string]any{"status": map[string]any{
					"isRunning": true,
					"activity":  map[string]any{"phase": "waiting-model"},
				}}, nil
			}
			return map[string]any{"status": map[string]any{"isRunning": false}}, nil
		case "session/get":
			return map[string]any{"session": map[string]any{"messages": []map[string]any{
				{"id": "m1", "role": "user", "content": "问题"},
				{"id": "m2", "role": "assistant", "content": "最终答案"},
			}}}, nil
		}
		return nil, &Error{Code: "UNEXPECTED", Message: op}
	})

	sess := &session{c: c, id: "s1"}
	st := &stream{
		sess:     sess,
		interval: 5 * time.Millisecond,
		ch:       make(chan agentcore.AgentEvent, 16),
		done:     make(chan struct{}),
	}
	go st.loop()

	deadline := time.After(3 * time.Second)
	var sawNarration, sawEnd bool
	var finalText string
	for !sawEnd {
		select {
		case ev, ok := <-st.Events():
			if !ok {
				t.Fatal("stream closed before AgentEnd")
			}
			switch e := ev.(type) {
			case agentcore.MessageEndEvent:
				if am, isAssistant := e.Message.(agentcore.AssistantMessage); isAssistant {
					if txt := agentcore.ContentToText(am.Content); strings.Contains(txt, "最终答案") {
						sawNarration = true
					}
				}
			case agentcore.AgentEndEvent:
				sawEnd = true
				for _, m := range e.Messages {
					if am, isAssistant := m.(agentcore.AssistantMessage); isAssistant {
						finalText += agentcore.ContentToText(am.Content)
					}
				}
			}
		case <-deadline:
			t.Fatalf("stream did not finish (narration=%v)", sawNarration)
		}
	}
	if !sawNarration {
		t.Fatal("assistant text never reached the card")
	}
	if !strings.Contains(finalText, "最终答案") {
		t.Fatalf("AgentEnd carried %q, want the final answer", finalText)
	}
}

func TestStreamSurvivesTransientStatusFailure(t *testing.T) {
	var calls atomic.Int32
	c := fakeApp(t, "t", func(op string, _ map[string]any) (any, *Error) {
		if op == "agent/status" {
			if calls.Add(1) == 1 {
				return nil, &Error{Code: "INTERNAL", Message: "boom"}
			}
			return map[string]any{"status": map[string]any{"isRunning": true}}, nil
		}
		return map[string]any{"session": map[string]any{"messages": []map[string]any{}}}, nil
	})
	sess := &session{c: c, id: "s1"}
	st := &stream{sess: sess, interval: 5 * time.Millisecond, ch: make(chan agentcore.AgentEvent, 8), done: make(chan struct{})}
	go st.loop()
	// Drain whatever the healthy polls emit; the assertion is that the run is
	// still open after the failed poll, not that nothing was said.
	deadline := time.After(400 * time.Millisecond)
	for done := false; !done; {
		select {
		case ev := <-st.Events():
			if _, ended := ev.(agentcore.AgentEndEvent); ended {
				t.Fatal("a transient failure must not end the card")
			}
		case <-deadline:
			done = true
		}
	}
	if calls.Load() < 3 {
		t.Fatalf("poller stopped after a failure (calls=%d)", calls.Load())
	}
	st.Close()
}

func TestDiscoverReportsMissingApp(t *testing.T) {
	t.Setenv("PIGO_DESKTOP_CONTROL", "/nonexistent/control.json")
	t.Setenv("PI_DESKTOP_DATA_DIR", "")
	if _, err := Discover(); err == nil {
		t.Fatal("want an error when the app is not running")
	}
}
