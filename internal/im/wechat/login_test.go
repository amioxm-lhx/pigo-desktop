package wechat

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"strings"
	"sync"
	"testing"
	"time"
)

// qrScript replays a fixed sequence of scan states and records the verify code
// each poll carried, which is what proves the code reached the wire.
type qrScript struct {
	mu       sync.Mutex
	statuses []string
	codes    []string
	qrIssued int
	final    map[string]any
}

func (s *qrScript) next(r *http.Request) string {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.codes = append(s.codes, r.URL.Query().Get("verify_code"))
	if len(s.statuses) == 0 {
		return qrWait
	}
	st := s.statuses[0]
	s.statuses = s.statuses[1:]
	return st
}

func (s *qrScript) routes() map[string]http.HandlerFunc {
	return map[string]http.HandlerFunc{
		"/ilink/bot/get_bot_qrcode": func(w http.ResponseWriter, _ *http.Request) {
			s.mu.Lock()
			s.qrIssued++
			n := s.qrIssued
			s.mu.Unlock()
			_, _ = w.Write([]byte(`{"ret":0,"qrcode":"Q` + itoa(n) + `","qrcode_img_content":"https://liteapp.example/q` + itoa(n) + `"}`))
		},
		"/ilink/bot/get_qrcode_status": func(w http.ResponseWriter, r *http.Request) {
			st := s.next(r)
			if st == qrConfirmed {
				body := map[string]any{
					"status":        qrConfirmed,
					"bot_token":     "bot-token-1",
					"ilink_bot_id":  "bot_abc",
					"ilink_user_id": "wxid_owner",
					"baseurl":       "https://ilinkai.weixin.qq.com",
				}
				writeJSON(w, body)
				return
			}
			writeJSON(w, map[string]any{"status": st})
		},
	}
}

func writeJSON(w http.ResponseWriter, body map[string]any) {
	w.Header().Set("Content-Type", "application/json")
	raw, err := json.Marshal(body)
	if err != nil {
		w.WriteHeader(http.StatusInternalServerError)
		return
	}
	_, _ = w.Write(raw)
}

// TestLoginConfirmed covers the happy path: one QR code, a scan, and credentials
// that carry the bot identity, the owner and the host to use afterwards.
func TestLoginConfirmed(t *testing.T) {
	script := &qrScript{statuses: []string{qrWait, qrConfirmed}}
	fs := newFakeServer(t, script.routes())

	var shown []string
	creds, err := Login(context.Background(), LoginOptions{
		BaseURL: fs.URL,
		Timeout: 10 * time.Second,
		ShowQR:  func(u string) { shown = append(shown, u) },
	})
	if err != nil {
		t.Fatalf("Login: %v", err)
	}
	if len(shown) != 1 || shown[0] != "https://liteapp.example/q1" {
		t.Fatalf("ShowQR got %v", shown)
	}
	if creds.Token != "bot-token-1" || creds.BotID != "bot_abc" || creds.UserID != "wxid_owner" {
		t.Fatalf("credentials = %+v", creds)
	}
	if creds.BaseURL != "https://ilinkai.weixin.qq.com" {
		t.Fatalf("base url = %q", creds.BaseURL)
	}

	// The login endpoints are anonymous: a token here would mean the caller
	// leaked a credential into a request that does not need one.
	qreq := fs.last(t, "/ilink/bot/get_bot_qrcode")
	if got := qreq.Header.Get("Authorization"); got != "" {
		t.Fatalf("get_bot_qrcode must not authenticate, sent %q", got)
	}
	if qreq.Header.Get("iLink-App-Id") == "" {
		t.Fatal("even anonymous requests carry iLink-App-Id")
	}
}

// TestLoginVerifyCode covers the pairing that demands the digits shown on the
// phone: the code must be echoed on the next poll, and an empty prompt must not
// loop forever.
func TestLoginVerifyCode(t *testing.T) {
	script := &qrScript{statuses: []string{qrNeedVerifyCode, qrConfirmed}}
	fs := newFakeServer(t, script.routes())

	var prompts int
	creds, err := Login(context.Background(), LoginOptions{
		BaseURL: fs.URL,
		Timeout: 10 * time.Second,
		ShowQR:  func(string) {},
		AskVerifyCode: func(string) (string, error) {
			prompts++
			return "4321", nil
		},
	})
	if err != nil {
		t.Fatalf("Login: %v", err)
	}
	if creds == nil || prompts != 1 {
		t.Fatalf("prompts = %d, creds = %+v", prompts, creds)
	}

	script.mu.Lock()
	codes := append([]string(nil), script.codes...)
	script.mu.Unlock()
	if len(codes) < 2 || codes[len(codes)-1] != "4321" {
		t.Fatalf("verify codes sent = %v, want the last one to be 4321", codes)
	}
}

// TestLoginRefreshesExpiredQR: an expired code is reissued and shown again,
// rather than ending a login the user is halfway through.
func TestLoginRefreshesExpiredQR(t *testing.T) {
	script := &qrScript{statuses: []string{qrExpired, qrConfirmed}}
	fs := newFakeServer(t, script.routes())

	var shown []string
	if _, err := Login(context.Background(), LoginOptions{
		BaseURL: fs.URL,
		Timeout: 10 * time.Second,
		ShowQR:  func(u string) { shown = append(shown, u) },
	}); err != nil {
		t.Fatalf("Login: %v", err)
	}
	if len(shown) != 2 || !strings.HasSuffix(shown[1], "/q2") {
		t.Fatalf("shown = %v, want a second, different code", shown)
	}
}

// TestLoginAlreadyBound surfaces the duplicate-login answer as its own error: the
// caller turns it into an explanation, and a generic failure would hide the fix.
func TestLoginAlreadyBound(t *testing.T) {
	script := &qrScript{statuses: []string{qrBindedRedirect}}
	fs := newFakeServer(t, script.routes())

	_, err := Login(context.Background(), LoginOptions{
		BaseURL: fs.URL,
		Timeout: 10 * time.Second,
		ShowQR:  func(string) {},
	})
	if !errors.Is(err, ErrLoginAlreadyBound) {
		t.Fatalf("err = %v, want ErrLoginAlreadyBound", err)
	}
}

// TestLoginWithoutVerifyPromptExplains: a pairing that needs digits and a caller
// that cannot ask for them must fail with the instruction, not with a hang.
func TestLoginWithoutVerifyPromptExplains(t *testing.T) {
	script := &qrScript{statuses: []string{qrNeedVerifyCode}}
	fs := newFakeServer(t, script.routes())

	_, err := Login(context.Background(), LoginOptions{
		BaseURL: fs.URL,
		Timeout: 10 * time.Second,
		ShowQR:  func(string) {},
	})
	if err == nil {
		t.Fatal("want an error when a verify code is required but unreadable")
	}
	if !strings.Contains(err.Error(), "pigo im login") {
		t.Fatalf("error should name the fix, got: %v", err)
	}
}
