package wechat

import (
	"context"
	"errors"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"time"

	"github.com/smallnest/pigo/internal/im"
)

// newTestAdapter builds an adapter pointed at a fake server, with its state in a
// throwaway directory.
func newTestAdapter(t *testing.T, baseURL, token string) *Adapter {
	t.Helper()
	a, err := New(Config{
		Token:     token,
		BaseURL:   baseURL,
		StatePath: filepath.Join(t.TempDir(), "wechat.json"),
		LongPoll:  2 * time.Second,
	})
	if err != nil {
		t.Fatalf("New: %v", err)
	}
	t.Cleanup(func() { _ = a.Close() })
	return a
}

// TestRunDispatchesInbound drives the real polling loop against a fake server:
// one batch of messages, then a block. This is the only test that exercises the
// loop's own control flow rather than a helper.
func TestRunDispatchesInbound(t *testing.T) {
	var served bool
	fs := newFakeServer(t, map[string]http.HandlerFunc{
		"/ilink/bot/getupdates": func(w http.ResponseWriter, r *http.Request) {
			if !served {
				served = true
				_, _ = w.Write([]byte(`{"ret":0,"get_updates_buf":"c1","msgs":[
				  {"message_id":"m1","from_user_id":"u1","context_token":"ct1","message_type":1,
				   "item_list":[{"type":1,"text_item":{"text":"你好"}}]}]}`))
				return
			}
			<-r.Context().Done()
		},
	})
	a := newTestAdapter(t, fs.URL, "tok")

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	in := make(chan im.Inbound, 4)
	errc := make(chan error, 1)
	go func() { errc <- a.Run(ctx, in) }()

	select {
	case got := <-in:
		if got.Text != "你好" || got.Chat.ID != "u1" || got.MsgID != "m1" {
			t.Fatalf("inbound = %+v", got)
		}
		if got.Chat.Kind != im.ChatP2P || got.Sender.ID != "u1" {
			t.Fatalf("routing identity is wrong: %+v", got)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("no inbound message was dispatched")
	}

	cancel()
	select {
	case err := <-errc:
		if !errors.Is(err, context.Canceled) {
			t.Fatalf("Run returned %v, want context.Canceled", err)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("Run did not stop on cancel")
	}

	// The cursor and the context token have to survive: without the cursor a
	// restart replays the batch, without the token the reply is not threaded.
	st, err := LoadState(a.path)
	if err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	if st.SyncBuf != "c1" {
		t.Fatalf("sync buf = %q, want c1", st.SyncBuf)
	}
	if st.ContextTokens["u1"] != "ct1" {
		t.Fatalf("context token = %q", st.ContextTokens["u1"])
	}
}

// TestAskResolvesFromTextReply is the button replacement: a pending confirmation
// turns the next matching reply into a decision and keeps it out of the queue.
func TestAskResolvesFromTextReply(t *testing.T) {
	fs := newFakeServer(t, nil)
	a := newTestAdapter(t, fs.URL, "tok")
	in := make(chan im.Inbound, 4)

	type result struct {
		d   im.Decision
		err error
	}
	got := make(chan result, 1)
	go func() {
		d, err := a.Ask(context.Background(), im.ApprovalReq{
			Chat:    im.ChatRef{ID: "u1", Kind: im.ChatP2P},
			Tool:    "bash",
			Summary: "rm -rf build",
			Timeout: 5 * time.Second,
		})
		got <- result{d, err}
	}()

	waitPending(t, a, "u1")

	a.dispatch(context.Background(), in, Message{
		FromUserID:  "u1",
		MessageType: MessageTypeUser,
		ItemList:    []Item{{Type: ItemText, TextItem: &TextItem{Text: "同意"}}},
	})

	select {
	case r := <-got:
		if r.err != nil {
			t.Fatalf("Ask: %v", r.err)
		}
		if !r.d.Approve {
			t.Fatalf("decision = %+v, want approve", r.d)
		}
	case <-time.After(3 * time.Second):
		t.Fatal("Ask did not resolve")
	}

	// The vote must not also become a prompt.
	select {
	case prompt := <-in:
		t.Fatalf("the decision was also queued as a prompt: %+v", prompt)
	case <-time.After(100 * time.Millisecond):
	}
}

// TestPendingApprovalKeepsNonDecisionsFlowing: while a confirmation is open, a
// real message is still a message. Dropping it would lose the user's follow-up
// at the exact moment they are most likely to type one.
func TestPendingApprovalKeepsNonDecisionsFlowing(t *testing.T) {
	fs := newFakeServer(t, nil)
	a := newTestAdapter(t, fs.URL, "tok")
	in := make(chan im.Inbound, 4)

	go func() {
		_, _ = a.Ask(context.Background(), im.ApprovalReq{
			Chat:    im.ChatRef{ID: "u1"},
			Tool:    "bash",
			Timeout: 200 * time.Millisecond,
		})
	}()
	waitPending(t, a, "u1")

	a.dispatch(context.Background(), in, Message{
		FromUserID:  "u1",
		MessageType: MessageTypeUser,
		ItemList:    []Item{{Type: ItemText, TextItem: &TextItem{Text: "先别急，换个方案"}}},
	})
	select {
	case prompt := <-in:
		if prompt.Text != "先别急，换个方案" {
			t.Fatalf("prompt = %q", prompt.Text)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("a non-decision message was swallowed by the pending approval")
	}

	// The request times out rather than staying open forever; the caller sees a
	// distinct error so the transcript can say "nobody answered".
	time.Sleep(400 * time.Millisecond)
	if _, ok := peekPending(a, "u1"); ok {
		t.Fatal("a timed-out approval must be removed")
	}
}

// TestUpdateReportsUnsupported pins the platform limitation the router relies on:
// WeChat cannot edit a sent message, and saying so is what stops the progress card
// from being re-sent as six separate bubbles.
func TestUpdateReportsUnsupported(t *testing.T) {
	a := newTestAdapter(t, newFakeServer(t, nil).URL, "tok")
	err := a.Update(context.Background(), im.ChatRef{ID: "u1"}, "m1", im.Outbound{Text: "progress"})
	if !errors.Is(err, im.ErrUpdateUnsupported) {
		t.Fatalf("Update = %v, want ErrUpdateUnsupported", err)
	}
}

// TestSendRendersToText checks that what leaves the adapter is a single readable
// bubble, addressed with the peer's stored context token.
func TestSendRendersToText(t *testing.T) {
	fs := newFakeServer(t, nil)
	a := newTestAdapter(t, fs.URL, "tok")
	a.setContextToken("u1", "ct1")

	if _, err := a.Send(context.Background(), im.ChatRef{ID: "u1"}, im.Outbound{
		Title: "pigo · 完成",
		Text:  "**好了**",
	}); err != nil {
		t.Fatalf("Send: %v", err)
	}

	req := fs.last(t, "/ilink/bot/sendmessage")
	msg, _ := req.Body["msg"].(map[string]any)
	if msg["context_token"] != "ct1" {
		t.Fatalf("context_token = %v, want the stored one", msg["context_token"])
	}
	items, _ := msg["item_list"].([]any)
	if len(items) != 1 {
		t.Fatalf("items = %v", msg["item_list"])
	}
	item, _ := items[0].(map[string]any)
	text, _ := item["text_item"].(map[string]any)
	if text["text"] != "【pigo · 完成】\n好了" {
		t.Fatalf("rendered text = %q", text["text"])
	}
}

// TestCloseReleasesPendingApprovals: a shutdown must not leave the tool hook
// blocked until its timeout.
func TestCloseReleasesPendingApprovals(t *testing.T) {
	a := newTestAdapter(t, newFakeServer(t, nil).URL, "tok")
	errc := make(chan error, 1)
	go func() {
		_, err := a.Ask(context.Background(), im.ApprovalReq{
			Chat:    im.ChatRef{ID: "u1"},
			Tool:    "bash",
			Timeout: 10 * time.Second,
		})
		errc <- err
	}()
	waitPending(t, a, "u1")
	_ = a.Close()

	select {
	case err := <-errc:
		if err == nil {
			t.Fatal("Ask should fail once the channel is closed")
		}
	case <-time.After(3 * time.Second):
		t.Fatal("Close did not release the blocked approval")
	}
}

// TestStoreRoundTrip covers the persistence contract: missing file is empty,
// malformed file is an error, and a different bot account invalidates the cursor
// while a token refresh keeps it.
func TestStoreRoundTrip(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "im", "wechat.json")

	st, err := LoadState(path)
	if err != nil {
		t.Fatalf("missing file must not be an error: %v", err)
	}
	if st.Token != "" || len(st.ContextTokens) != 0 {
		t.Fatalf("empty state expected, got %+v", st)
	}

	st.Token = "tok"
	st.BotID = "bot1"
	st.SyncBuf = "cursor"
	st.ContextTokens["u1"] = "ct1"
	if err := SaveState(path, st); err != nil {
		t.Fatalf("SaveState: %v", err)
	}

	back, err := LoadState(path)
	if err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	if back.Token != "tok" || back.SyncBuf != "cursor" || back.ContextTokens["u1"] != "ct1" {
		t.Fatalf("round trip lost data: %+v", back)
	}

	// The file holds a credential; on a POSIX filesystem it must not be readable
	// by anyone else.
	if runtime.GOOS != "windows" {
		info, ierr := os.Stat(path)
		if ierr != nil {
			t.Fatalf("stat: %v", ierr)
		}
		if perm := info.Mode().Perm(); perm&0o077 != 0 {
			t.Fatalf("state file is %o, want 0600", perm)
		}
	}

	// A token refresh for the same bot keeps the cursor.
	kept := ApplyCredentials(back, &Credentials{Token: "tok2", BotID: "bot1", UserID: "me"})
	if kept.SyncBuf != "cursor" || kept.ContextTokens["u1"] != "ct1" {
		t.Fatalf("token refresh dropped the cursor: %+v", kept)
	}

	// A different bot is a different account: its cursor and peer tokens mean
	// nothing here and would silently skip that account's backlog.
	swapped := ApplyCredentials(back, &Credentials{Token: "tok3", BotID: "bot2", UserID: "other"})
	if swapped.SyncBuf != "" || len(swapped.ContextTokens) != 0 {
		t.Fatalf("account switch kept another account's state: %+v", swapped)
	}

	// Malformed content is an error, not a silent reset: starting over would lose
	// the cursor with no explanation.
	if err := os.WriteFile(path, []byte("{not json"), 0o600); err != nil {
		t.Fatalf("write: %v", err)
	}
	if _, err := LoadState(path); err == nil {
		t.Fatal("want an error for a malformed state file")
	}
}

// TestNewRefusesWithoutToken keeps "not paired" a startup error rather than a
// channel that starts and then refuses every request.
func TestNewRefusesWithoutToken(t *testing.T) {
	_, err := New(Config{StatePath: filepath.Join(t.TempDir(), "wechat.json")})
	if err == nil {
		t.Fatal("want an error without a token")
	}
}

// ---- helpers ----

func waitPending(t *testing.T, a *Adapter, chatID string) {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if _, ok := peekPending(a, chatID); ok {
			return
		}
		time.Sleep(5 * time.Millisecond)
	}
	t.Fatalf("approval for %s never became pending", chatID)
}

func peekPending(a *Adapter, chatID string) (chan im.Decision, bool) {
	a.mu.Lock()
	defer a.mu.Unlock()
	ch, ok := a.pending[chatID]
	return ch, ok
}
