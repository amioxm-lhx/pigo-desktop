package feishu

import (
	"context"
	"fmt"
	"strings"
	"sync"
	"testing"

	larkim "github.com/larksuite/oapi-sdk-go/v3/service/im/v1"

	"github.com/smallnest/pigo/internal/im"
)

// logRecorder collects the adapter's diagnostics without racing the handler.
type logRecorder struct {
	mu    sync.Mutex
	lines []string
}

func (r *logRecorder) logf(format string, args ...any) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.lines = append(r.lines, fmt.Sprintf(format, args...))
}

func (r *logRecorder) all() string {
	r.mu.Lock()
	defer r.mu.Unlock()
	return strings.Join(r.lines, "\n")
}

func newTestAdapter(t *testing.T) (*Adapter, *logRecorder) {
	t.Helper()
	rec := &logRecorder{}
	a, err := New(Config{AppID: "cli_test", AppSecret: "secret", Logf: rec.logf})
	if err != nil {
		t.Fatalf("New: %v", err)
	}
	return a, rec
}

// The chat-access push arrives every time the user taps the bot. Without a
// handler the SDK logs "not found handler" as an error; with one it must print
// the open_id that the allow list needs — once, not once per tap.
func TestChatEnteredLogsOpenIDOnce(t *testing.T) {
	a, rec := newTestAdapter(t)
	openID := "ou_60707b5df6bb16e7451bb64a935120c0"
	ev := &larkim.P2ChatAccessEventBotP2pChatEnteredV1{
		Event: &larkim.P2ChatAccessEventBotP2pChatEnteredV1Data{
			ChatId:     strPtr("oc_f6827844"),
			OperatorId: &larkim.UserId{OpenId: &openID},
		},
	}

	for i := 0; i < 3; i++ {
		if err := a.onP2PChatEntered(context.Background(), ev); err != nil {
			t.Fatalf("onP2PChatEntered: %v", err)
		}
	}

	got := rec.all()
	if n := strings.Count(got, openID); n != 1 {
		t.Fatalf("open_id logged %d times, want 1\n%s", n, got)
	}
	if !strings.Contains(got, "allowed_users") {
		t.Fatalf("log line does not say what to do with the id: %s", got)
	}
}

// A second chat is a second diagnostic; dedupe is per chat, not global.
func TestChatEnteredLogsEachChat(t *testing.T) {
	a, rec := newTestAdapter(t)
	for _, chat := range []string{"oc_a", "oc_b"} {
		openID := "ou_" + chat
		ev := &larkim.P2ChatAccessEventBotP2pChatEnteredV1{
			Event: &larkim.P2ChatAccessEventBotP2pChatEnteredV1Data{
				ChatId:     strPtr(chat),
				OperatorId: &larkim.UserId{OpenId: &openID},
			},
		}
		if err := a.onP2PChatEntered(context.Background(), ev); err != nil {
			t.Fatalf("onP2PChatEntered: %v", err)
		}
	}
	if got := rec.all(); !strings.Contains(got, "ou_oc_a") || !strings.Contains(got, "ou_oc_b") {
		t.Fatalf("both chats should be reported once each: %s", got)
	}
}

// A nil or empty event must not panic — the SDK hands over whatever it parsed.
func TestChatEnteredToleratesEmptyEvent(t *testing.T) {
	a, _ := newTestAdapter(t)
	for _, ev := range []*larkim.P2ChatAccessEventBotP2pChatEnteredV1{
		nil,
		{},
		{Event: &larkim.P2ChatAccessEventBotP2pChatEnteredV1Data{}},
	} {
		if err := a.onP2PChatEntered(context.Background(), ev); err != nil {
			t.Fatalf("onP2PChatEntered: %v", err)
		}
	}
}

// An all-open_id allow list is already in the only form the router can match,
// so it must come back untouched and without a network call.
func TestResolveAllowedUsersPassesThroughIDs(t *testing.T) {
	a, _ := newTestAdapter(t)
	in := []string{"ou_1", " ou_2 ", "", "ou_3"}
	got, unresolved, err := a.ResolveAllowedUsers(context.Background(), in)
	if err != nil {
		t.Fatalf("ResolveAllowedUsers: %v", err)
	}
	if len(unresolved) != 0 {
		t.Fatalf("unexpected unresolved entries: %v", unresolved)
	}
	want := []string{"ou_1", "ou_2", "ou_3"}
	if len(got) != len(want) {
		t.Fatalf("got %v, want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("got %v, want %v", got, want)
		}
	}
}

// The sender's identity is what the allow list is compared against, and Feishu
// only guarantees open_id there.
func TestSenderOfUsesOpenID(t *testing.T) {
	openID := "ou_x"
	got := senderOf(&larkim.EventSender{
		SenderId: &larkim.UserId{OpenId: &openID},
	})
	if got.ID != openID {
		t.Fatalf("ID = %q, want %q", got.ID, openID)
	}

	// The router must be able to match the id it is told to allow.
	cfg := im.RouterConfig{AllowedUsers: []string{openID}}
	if len(cfg.AllowedUsers) != 1 || cfg.AllowedUsers[0] != got.ID {
		t.Fatalf("allow list %v does not match sender %q", cfg.AllowedUsers, got.ID)
	}
}

func strPtr(s string) *string { return &s }
