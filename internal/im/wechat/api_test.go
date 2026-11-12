package wechat

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"
)

// fakeServer stands in for ilinkai.weixin.qq.com. Every handler records the
// request it saw so the tests can assert on the wire shape rather than on the
// adapter's own idea of it.
type fakeServer struct {
	*httptest.Server
	mu       sync.Mutex
	requests []recorded
}

type recorded struct {
	Path   string
	Method string
	Header http.Header
	Body   map[string]any
}

// newFakeServer installs handlers; a nil handler answers 200 with "{}".
func newFakeServer(t *testing.T, routes map[string]http.HandlerFunc) *fakeServer {
	t.Helper()
	fs := &fakeServer{}
	fs.Server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		var body map[string]any
		_ = json.Unmarshal(raw, &body)
		fs.mu.Lock()
		fs.requests = append(fs.requests, recorded{
			Path:   r.URL.Path,
			Method: r.Method,
			Header: r.Header.Clone(),
			Body:   body,
		})
		fs.mu.Unlock()

		if h, ok := routes[r.URL.Path]; ok {
			h(w, r)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ret":0}`))
	}))
	t.Cleanup(fs.Close)
	return fs
}

func (f *fakeServer) last(t *testing.T, path string) recorded {
	t.Helper()
	f.mu.Lock()
	defer f.mu.Unlock()
	for i := len(f.requests) - 1; i >= 0; i-- {
		if f.requests[i].Path == path {
			return f.requests[i]
		}
	}
	t.Fatalf("no request to %s; saw %v", path, f.paths())
	return recorded{}
}

func (f *fakeServer) paths() []string {
	f.mu.Lock()
	defer f.mu.Unlock()
	out := make([]string, 0, len(f.requests))
	for _, r := range f.requests {
		out = append(out, r.Path)
	}
	return out
}

// TestAPIRequestShape pins the headers and body every authenticated call needs.
//
// These are not cosmetic: without iLink-App-Id/ClientVersion the request is
// rejected outright, and without the bearer token the server answers as an
// anonymous client.
func TestAPIRequestShape(t *testing.T) {
	fs := newFakeServer(t, map[string]http.HandlerFunc{
		"/ilink/bot/sendmessage": func(w http.ResponseWriter, _ *http.Request) {
			_, _ = w.Write([]byte(`{"ret":0,"message_id":1234567890123456789}`))
		},
	})
	api := NewAPI(fs.URL, "tok_abc", nil)

	id, err := api.SendMessage(context.Background(), "user_1", "ctx_1", "hello")
	if err != nil {
		t.Fatalf("SendMessage: %v", err)
	}
	// A 19-digit id must survive decoding; a float64 would round it.
	if id != "1234567890123456789" {
		t.Fatalf("id = %q, want the exact 19-digit id", id)
	}

	req := fs.last(t, "/ilink/bot/sendmessage")
	if got := req.Header.Get("iLink-App-Id"); got != ILinkAppID {
		t.Fatalf("iLink-App-Id = %q", got)
	}
	if got := req.Header.Get("iLink-App-ClientVersion"); got != "132106" {
		t.Fatalf("iLink-App-ClientVersion = %q, want 132106 (2.4.10)", got)
	}
	if got := req.Header.Get("Authorization"); got != "Bearer tok_abc" {
		t.Fatalf("Authorization = %q", got)
	}
	if got := req.Header.Get("AuthorizationType"); got != "ilink_bot_token" {
		t.Fatalf("AuthorizationType = %q", got)
	}
	if got := req.Header.Get("X-WECHAT-UIN"); got == "" {
		t.Fatal("X-WECHAT-UIN is required on every request")
	}

	msg, _ := req.Body["msg"].(map[string]any)
	if msg == nil {
		t.Fatalf("body has no msg: %v", req.Body)
	}
	if msg["to_user_id"] != "user_1" || msg["context_token"] != "ctx_1" {
		t.Fatalf("addressing is wrong: %v", msg)
	}
	if msg["message_type"] != float64(MessageTypeBot) || msg["message_state"] != float64(StateFinish) {
		t.Fatalf("message enums are wrong: %v", msg)
	}
	items, _ := msg["item_list"].([]any)
	if len(items) != 1 {
		t.Fatalf("want exactly one item, got %v", msg["item_list"])
	}
	item, _ := items[0].(map[string]any)
	text, _ := item["text_item"].(map[string]any)
	if text["text"] != "hello" {
		t.Fatalf("text item = %v", item)
	}

	info, _ := req.Body["base_info"].(map[string]any)
	if info["channel_version"] != channelVersion || info["bot_agent"] != botAgent {
		t.Fatalf("base_info = %v", req.Body["base_info"])
	}
}

// TestUpdatesLongPollTimeout covers the contract the polling loop depends on: a
// client-side timeout is an empty batch, not a failure, and the cursor comes back
// unchanged so the next poll resumes where this one stopped.
func TestUpdatesLongPollTimeout(t *testing.T) {
	fs := newFakeServer(t, map[string]http.HandlerFunc{
		"/ilink/bot/getupdates": func(w http.ResponseWriter, r *http.Request) {
			select {
			case <-r.Context().Done():
			case <-time.After(2 * time.Second):
			}
		},
	})
	api := NewAPI(fs.URL, "tok", nil)

	start := time.Now()
	resp, err := api.Updates(context.Background(), "cursor-1", 80*time.Millisecond)
	if err != nil {
		t.Fatalf("a long-poll timeout must not be an error: %v", err)
	}
	if elapsed := time.Since(start); elapsed > time.Second {
		t.Fatalf("timeout not honoured, took %s", elapsed)
	}
	if len(resp.Msgs) != 0 {
		t.Fatalf("want no messages, got %d", len(resp.Msgs))
	}
	if resp.GetUpdatesBuf != "cursor-1" {
		t.Fatalf("cursor = %q, want it preserved", resp.GetUpdatesBuf)
	}
}

// TestUpdatesServerErrorIsFatal keeps a real failure distinguishable from a
// timeout; otherwise a broken token would look like a quiet channel.
func TestUpdatesServerErrorIsFatal(t *testing.T) {
	fs := newFakeServer(t, map[string]http.HandlerFunc{
		"/ilink/bot/getupdates": func(w http.ResponseWriter, _ *http.Request) {
			w.WriteHeader(http.StatusForbidden)
			_, _ = w.Write([]byte(`nope`))
		},
	})
	api := NewAPI(fs.URL, "tok", nil)
	if _, err := api.Updates(context.Background(), "", time.Second); err == nil {
		t.Fatal("want an error on http 403")
	}
}

// TestUpdatesDecodesMessages covers the inbound wire shape: text, voice
// transcript, media, and a uint64 id sent as a bare number.
func TestUpdatesDecodesMessages(t *testing.T) {
	payload := `{
	  "ret": 0,
	  "get_updates_buf": "cursor-2",
	  "longpolling_timeout_ms": 41000,
	  "msgs": [
	    {"message_id": 7300000000000000001, "from_user_id": "u1", "context_token": "ct1",
	     "message_type": 1, "create_time_ms": 1700000000000,
	     "item_list": [{"type": 1, "text_item": {"text": "hi"}}]},
	    {"message_id": "abc", "from_user_id": "u1", "message_type": 1,
	     "item_list": [{"type": 3, "voice_item": {"text": "语音转写"}}]},
	    {"message_id": "def", "from_user_id": "u1", "message_type": 1,
	     "item_list": [{"type": 2, "image_item": {}}]}
	  ]
	}`
	fs := newFakeServer(t, map[string]http.HandlerFunc{
		"/ilink/bot/getupdates": func(w http.ResponseWriter, _ *http.Request) {
			_, _ = w.Write([]byte(payload))
		},
	})
	api := NewAPI(fs.URL, "tok", nil)
	resp, err := api.Updates(context.Background(), "", time.Second)
	if err != nil {
		t.Fatalf("Updates: %v", err)
	}
	if len(resp.Msgs) != 3 {
		t.Fatalf("want 3 messages, got %d", len(resp.Msgs))
	}
	if got := idString(resp.Msgs[0].MessageID); got != "7300000000000000001" {
		t.Fatalf("numeric id decoded as %q", got)
	}
	if text, _ := messageText(resp.Msgs[0]); text != "hi" {
		t.Fatalf("text = %q", text)
	}
	if text, _ := messageText(resp.Msgs[1]); text != "语音转写" {
		t.Fatalf("voice transcript = %q", text)
	}
	if text, unsupported := messageText(resp.Msgs[2]); text != "" || unsupported != "图片" {
		t.Fatalf("image = (%q, %q)", text, unsupported)
	}
	if resp.LongPollingTimeoutMS != 41000 {
		t.Fatalf("longpolling_timeout_ms = %d", resp.LongPollingTimeoutMS)
	}
}

// TestTypingSkipsWithoutTicket documents the degradation: no ticket means no
// indicator, and crucially no request that could fail the run.
func TestTypingSkipsWithoutTicket(t *testing.T) {
	fs := newFakeServer(t, nil)
	api := NewAPI(fs.URL, "tok", nil)
	if err := api.SendTyping(context.Background(), "u1", "", TypingStart); err != nil {
		t.Fatalf("SendTyping without a ticket must be a no-op: %v", err)
	}
	if paths := fs.paths(); len(paths) != 0 {
		t.Fatalf("no request should have been made, saw %v", paths)
	}
}

// TestNormalizeBaseURL covers the three shapes the API root arrives in: default,
// a bare host after an idc redirect, and a trailing slash.
func TestNormalizeBaseURL(t *testing.T) {
	cases := map[string]string{
		"":                               DefaultBaseURL,
		"ilinkai.weixin.qq.com":          "https://ilinkai.weixin.qq.com",
		"https://ilinkai.weixin.qq.com/": "https://ilinkai.weixin.qq.com",
		"http://127.0.0.1:8080":          "http://127.0.0.1:8080",
	}
	for in, want := range cases {
		if got := normalizeBaseURL(in); got != want {
			t.Fatalf("normalizeBaseURL(%q) = %q, want %q", in, got, want)
		}
	}
	if !strings.HasPrefix(newClientID(), "pigo-wechat-") {
		t.Fatal("client ids must be recognisable in server logs")
	}
}
