package desktop

import (
	"context"
	"encoding/json"
	"errors"
	"sync"
	"testing"
	"time"

	"github.com/smallnest/pigo/internal/im"
)

// The control endpoint's permission pair is the only bridge between the app's
// window and a remote chat, so the wire shape is worth pinning down: a field
// rename here would otherwise show up as a silent "no confirmations" bug.
func TestPendingPermissionsReadsHostWire(t *testing.T) {
	var (
		mu   sync.Mutex
		seen map[string]any
	)
	c := fakeApp(t, "t", func(op string, args map[string]any) (any, *Error) {
		if op != "permissions/pending" {
			t.Errorf("op = %q, want permissions/pending", op)
		}
		mu.Lock()
		seen = args
		mu.Unlock()
		return map[string]any{"requests": []map[string]any{
			{
				"requestId":   "r1",
				"sessionId":   "s1",
				"toolCallId":  "tc1",
				"toolName":    "Edit",
				"risk":        "high",
				"argsPreview": map[string]any{"file_path": "internal/im/router.go"},
				"reason":      "High-risk tool requires approval",
				"timeoutMs":   120000,
				"createdAt":   "2026-11-12T09:00:00Z",
				"expiresAt":   "2026-11-12T09:02:00Z",
				"remainingMs": 98000,
			},
			{
				"requestId":   "r2",
				"sessionId":   "s1",
				"toolName":    "Bash",
				"argsPreview": "rm -rf ./dist",
				"remainingMs": 5000,
			},
			{"sessionId": "s1"}, // no request id: never answerable, skip it
		}}, nil
	})

	rows, err := c.Pending(context.Background(), "s1")
	if err != nil {
		t.Fatalf("Pending: %v", err)
	}
	if len(rows) != 2 {
		t.Fatalf("rows = %+v, want 2", rows)
	}
	mu.Lock()
	scoped := seen["sessionId"]
	mu.Unlock()
	if scoped != "s1" {
		t.Fatalf("sessionId not forwarded: %v", seen)
	}
	if rows[0].RequestID != "r1" || rows[0].SessionID != "s1" || rows[0].ToolName != "Edit" {
		t.Fatalf("bad first row: %+v", rows[0])
	}
	if rows[0].Remaining != 98*time.Second {
		t.Fatalf("remaining = %s, want 98s", rows[0].Remaining)
	}
	if rows[0].Reason == "" || rows[0].Summary == "" {
		t.Fatalf("card would have nothing to show: %+v", rows[0])
	}
	if rows[1].Summary != "rm -rf ./dist" {
		t.Fatalf("string preview = %q", rows[1].Summary)
	}
}

func TestResolvePermissionSendsDecision(t *testing.T) {
	var (
		mu    sync.Mutex
		calls []map[string]any
	)
	c := fakeApp(t, "t", func(op string, args map[string]any) (any, *Error) {
		mu.Lock()
		calls = append(calls, map[string]any{"op": op, "args": args})
		mu.Unlock()
		return map[string]any{"ok": true}, nil
	})
	if err := c.Resolve(context.Background(), "r1", im.PermissionAllowSession); err != nil {
		t.Fatalf("Resolve: %v", err)
	}
	mu.Lock()
	defer mu.Unlock()
	if len(calls) != 1 {
		t.Fatalf("calls = %+v", calls)
	}
	if calls[0]["op"] != "permissions/resolve" {
		t.Fatalf("op = %v", calls[0]["op"])
	}
	args := calls[0]["args"].(map[string]any)
	if args["requestId"] != "r1" || args["decision"] != "allow-session" {
		t.Fatalf("args = %+v", args)
	}
}

func TestPendingPermissionsReportsUnsupportedHost(t *testing.T) {
	c := fakeApp(t, "t", func(string, map[string]any) (any, *Error) {
		return nil, &Error{Code: "NOT_ALLOWED", Message: "operation is not exposed"}
	})
	_, err := c.Pending(context.Background(), "s1")
	if !errors.Is(err, im.ErrPermissionUnsupported) {
		t.Fatalf("err = %v, want ErrPermissionUnsupported", err)
	}
}

func TestResolvePermissionNotFoundIsGone(t *testing.T) {
	c := fakeApp(t, "t", func(string, map[string]any) (any, *Error) {
		return nil, &Error{Code: "NOT_FOUND", Message: "unknown request"}
	})
	err := c.Resolve(context.Background(), "r1", im.PermissionDeny)
	if !errors.Is(err, im.ErrPermissionGone) {
		t.Fatalf("err = %v, want ErrPermissionGone", err)
	}
}

func TestPreviewText(t *testing.T) {
	cases := []struct {
		raw  string
		want string
	}{
		{`""`, ""},
		{`null`, ""},
		{`"rm -rf ./dist"`, "rm -rf ./dist"},
		{`{"file_path":"a.go"}`, `{"file_path":"a.go"}`},
	}
	for _, tc := range cases {
		if got := previewText(json.RawMessage(tc.raw)); got != tc.want {
			t.Errorf("previewText(%s) = %q, want %q", tc.raw, got, tc.want)
		}
	}
}
