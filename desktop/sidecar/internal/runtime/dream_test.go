package runtime

import (
	"context"
	"encoding/json"
	"errors"
	"testing"

	"github.com/smallnest/pigo/desktop/sidecar/internal/jsonrpc"
	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
)

// dreamHost serves the two session RPCs the dream transcript source needs and
// records what was asked, so the adapter's mapping can be asserted without a
// live runtime.
type dreamHost struct {
	sessions []hostSessionSummary
	detail   map[string]struct {
		path     string
		updated  string
		messages []hostSessionMessage
	}
	asked []string
}

func (h *dreamHost) Call(_ context.Context, method string, params any) (json.RawMessage, error) {
	h.asked = append(h.asked, method)
	switch method {
	case "session.list":
		return json.Marshal(map[string]any{"sessions": h.sessions})
	case "session.get":
		input, _ := params.(map[string]any)
		id, _ := input["id"].(string)
		row, ok := h.detail[id]
		if !ok {
			return nil, errors.New("session not found")
		}
		return json.Marshal(map[string]any{"session": map[string]any{
			"id":          id,
			"projectPath": row.path,
			"updatedAt":   row.updated,
			"messages":    row.messages,
		}})
	default:
		return nil, errors.New("unexpected host method " + method)
	}
}

func (h *dreamHost) CallInto(ctx context.Context, method string, params any, out any) error {
	raw, err := h.Call(ctx, method, params)
	if err != nil {
		return err
	}
	return json.Unmarshal(raw, out)
}

func TestHostSessionSourceListsProjectsAsCwd(t *testing.T) {
	host := &dreamHost{sessions: []hostSessionSummary{
		{ID: "s1", ProjectPath: "/repo/a", UpdatedAt: "2026-11-07T10:00:00Z"},
		{ID: "", ProjectPath: "/repo/b", UpdatedAt: "2026-11-07T11:00:00Z"},
		{ID: "s2", ProjectPath: "/repo/c"},
	}}
	source := hostSessionSource{ctx: context.Background(), host: host}
	headers, err := source.List()
	if err != nil {
		t.Fatalf("list: %v", err)
	}
	if len(headers) != 2 {
		t.Fatalf("expected the id-less row to be dropped, got %d headers", len(headers))
	}
	if headers[0].ID != "s1" || headers[0].Cwd != "/repo/a" {
		t.Fatalf("unexpected first header: %+v", headers[0])
	}
	if headers[0].UpdatedAt.IsZero() {
		t.Fatal("updatedAt should parse")
	}
	// A missing timestamp degrades to the zero time instead of failing the list.
	if !headers[1].UpdatedAt.IsZero() {
		t.Fatalf("expected zero time for a missing timestamp, got %v", headers[1].UpdatedAt)
	}
}

func TestHostSessionSourceLoadMapsOnlyConversationRows(t *testing.T) {
	host := &dreamHost{detail: map[string]struct {
		path     string
		updated  string
		messages []hostSessionMessage
	}{
		"s1": {
			path:    "/repo/a",
			updated: "2026-11-07T10:00:00Z",
			messages: []hostSessionMessage{
				{Role: "user", Content: "how do I deploy?"},
				{Role: "assistant", Content: "  "},
				{Role: "assistant", Content: "run make deploy", ParentToolCallID: "t1"},
				{Role: "assistant", Content: "use the release script"},
				{Role: "tool", Content: "irrelevant for distillation"},
			},
		},
	}}
	source := hostSessionSource{ctx: context.Background(), host: host}
	header, messages, err := source.Load("s1")
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	if header.Cwd != "/repo/a" {
		t.Fatalf("cwd = %q", header.Cwd)
	}
	if len(messages) != 2 {
		t.Fatalf("expected user+assistant only, got %d messages: %#v", len(messages), messages)
	}
	if _, ok := messages[0].(agentcore.UserMessage); !ok {
		t.Fatalf("first message should be a user message, got %T", messages[0])
	}
	assistant, ok := messages[1].(agentcore.AssistantMessage)
	if !ok {
		t.Fatalf("second message should be an assistant message, got %T", messages[1])
	}
	if got := agentcore.ContentToText(assistant.Content); got != "use the release script" {
		t.Fatalf("assistant text = %q (subagent rows must be excluded)", got)
	}
}

func TestDreamRejectsMissingProvider(t *testing.T) {
	runner := NewDreamRunner(&dreamHost{}, nil)
	_, err := runner.Run(context.Background(), protocol.DreamParams{})
	var rpcErr *jsonrpc.Error
	if !errors.As(err, &rpcErr) {
		t.Fatalf("expected a JSON-RPC error, got %v", err)
	}
	if data, ok := rpcErr.Data.(map[string]string); !ok || data["errorCode"] != "MODEL_NOT_CONFIGURED" {
		t.Fatalf("expected MODEL_NOT_CONFIGURED, got %v", err)
	}
}

func TestDreamRejectsMissingCredential(t *testing.T) {
	runner := NewDreamRunner(&dreamHost{}, nil)
	params := protocol.DreamParams{SessionID: "s1"}
	params.Provider = protocol.ProviderConfig{ModelID: "gpt-4o", BaseURL: "http://localhost:1234"}
	_, err := runner.Run(context.Background(), params)
	var rpcErr *jsonrpc.Error
	if !errors.As(err, &rpcErr) {
		t.Fatalf("expected a JSON-RPC error, got %v", err)
	}
	if data, ok := rpcErr.Data.(map[string]string); !ok || data["errorCode"] != "AUTH_REQUIRED" {
		t.Fatalf("expected AUTH_REQUIRED, got %v", err)
	}
}

// TestDreamIsSingleFlight proves two concurrent passes cannot both run: the
// on-disk lock would make the second a silent "skipped" run, which reads as a
// successful no-op in the UI.
func TestDreamIsSingleFlight(t *testing.T) {
	runner := NewDreamRunner(&dreamHost{}, nil)
	if !runner.begin() {
		t.Fatal("first pass should acquire the latch")
	}
	if runner.begin() {
		t.Fatal("a second pass must not acquire the latch")
	}
	params := protocol.DreamParams{SessionID: "s1"}
	params.Provider = protocol.ProviderConfig{
		ModelID: "gpt-4o",
		BaseURL: "http://127.0.0.1:1",
		APIKey:  "test-key",
	}
	_, err := runner.Run(context.Background(), params)
	var rpcErr *jsonrpc.Error
	if !errors.As(err, &rpcErr) {
		t.Fatalf("expected a JSON-RPC error, got %v", err)
	}
	if data, ok := rpcErr.Data.(map[string]string); !ok || data["errorCode"] != "DREAM_BUSY" {
		t.Fatalf("expected DREAM_BUSY, got %v", err)
	}
	runner.end()
	if !runner.begin() {
		t.Fatal("the latch must be released after a pass")
	}
	runner.end()
}

func TestDreamProtocolMapsAPIStyles(t *testing.T) {
	cases := []struct {
		name string
		cfg  protocol.ProviderConfig
		want string
	}{
		{name: "anthropic", cfg: protocol.ProviderConfig{APIStyle: "anthropic"}, want: "anthropic"},
		{name: "responses", cfg: protocol.ProviderConfig{APIStyle: "openai/resp_api"}, want: "openai/resp_api"},
		{name: "chat", cfg: protocol.ProviderConfig{APIStyle: "openai"}, want: "openai"},
		{name: "model metadata wins", cfg: protocol.ProviderConfig{ModelConfig: &protocol.ModelConfig{API: "anthropic"}}, want: "anthropic"},
		{name: "empty defaults to chat", cfg: protocol.ProviderConfig{}, want: "openai"},
	}
	for _, test := range cases {
		if got := dreamProtocol(test.cfg); got != test.want {
			t.Fatalf("%s: dreamProtocol = %q, want %q", test.name, got, test.want)
		}
	}
}
