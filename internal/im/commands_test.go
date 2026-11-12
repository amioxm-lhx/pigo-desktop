package im

import (
	"strings"
	"testing"
)

func TestParseCommand(t *testing.T) {
	cases := []struct {
		name   string
		in     string
		wantOK bool
		want   string
		args   []string
	}{
		{"plain", "/new", true, "new", nil},
		{"with args", "/resume 20261112-0900", true, "resume", []string{"20261112-0900"}},
		{"upper case", "/STATUS", true, "status", nil},
		{"leading mention", "@_user_1 /status", true, "status", nil},
		{"two mentions", "@_all @_user_2 /help", true, "help", nil},
		{"surrounding space", "   /stop  ", true, "stop", nil},
		{"not a command", "hello", false, "", nil},
		{"bare slash", "/", false, "", nil},
		{"mention only", "@_user_1", false, "", nil},
		{"path-like", "/usr/bin/env", false, "", nil},
		{"empty", "", false, "", nil},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got, ok := ParseCommand(tc.in)
			if ok != tc.wantOK {
				t.Fatalf("ok = %v, want %v (in=%q)", ok, tc.wantOK, tc.in)
			}
			if !ok {
				return
			}
			if got.Name != tc.want {
				t.Fatalf("name = %q, want %q", got.Name, tc.want)
			}
			if len(got.Args) != len(tc.args) {
				t.Fatalf("args = %v, want %v", got.Args, tc.args)
			}
			for i := range tc.args {
				if got.Args[i] != tc.args[i] {
					t.Fatalf("args[%d] = %q, want %q", i, got.Args[i], tc.args[i])
				}
			}
		})
	}
}

func TestParseButtonAction(t *testing.T) {
	cases := []struct {
		action string
		want   string
		ok     bool
	}{
		{"approve", "approve", true},
		{"approve_always", "approve_always", true},
		{"deny", "deny", true},
		{"nonsense", "", false},
	}
	for _, tc := range cases {
		cmd, ok := ParseButtonAction(ActionRef{Name: tc.action, Token: "tok"})
		if ok != tc.ok {
			t.Fatalf("%q ok = %v, want %v", tc.action, ok, tc.ok)
		}
		if !ok {
			continue
		}
		if cmd.Name != tc.want {
			t.Fatalf("%q name = %q, want %q", tc.action, cmd.Name, tc.want)
		}
		if len(cmd.Args) != 1 || cmd.Args[0] != "tok" {
			t.Fatalf("%q args = %v, want [tok]", tc.action, cmd.Args)
		}
	}
}

// TestHelpTextStatesQueueLimit keeps the documented policy discoverable: the
// help text is where a user learns why their fourth message bounced.
func TestHelpTextStatesQueueLimit(t *testing.T) {
	txt := HelpText()
	for _, want := range []string{"/new", "/stop", "/status", "排队"} {
		if !strings.Contains(txt, want) {
			t.Fatalf("HelpText missing %q", want)
		}
	}
}

func TestChatRefKeyDefaultsToP2P(t *testing.T) {
	if got := (ChatRef{ID: "oc_1"}).Key(); got != "p2p:oc_1" {
		t.Fatalf("Key() = %q, want p2p:oc_1", got)
	}
	if got := (ChatRef{ID: "oc_1", Kind: ChatGroup}).Key(); got != "group:oc_1" {
		t.Fatalf("Key() = %q, want group:oc_1", got)
	}
	if got := (ChatRef{}).Key(); got != "" {
		t.Fatalf("empty ChatRef Key() = %q, want empty", got)
	}
}
