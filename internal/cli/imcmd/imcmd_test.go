package imcmd

import (
	"strings"
	"testing"

	"github.com/smallnest/pigo/internal/cli/config"
	"github.com/smallnest/pigo/internal/im"
)

// TestResolveSettingsSessionHost covers the flag > config > default chain for
// the carrier choice, including the typo that would otherwise silently fall back
// to running sessions in the wrong place.
func TestResolveSettingsSessionHost(t *testing.T) {
	base := config.IMConfig{
		Enabled: true,
		Feishu: config.FeishuConfig{
			AppID:        "cli_test",
			AppSecret:    "secret",
			AllowedUsers: []string{"ou_1"},
		},
	}
	cases := []struct {
		name    string
		cfg     string
		flag    string
		want    string
		wantErr bool
	}{
		{name: "default is cli", want: im.SessionHostCLI},
		{name: "from config", cfg: "desktop", want: im.SessionHostDesktop},
		{name: "flag wins", cfg: "cli", flag: "desktop", want: im.SessionHostDesktop},
		{name: "flag also sets cli", flag: "cli", want: im.SessionHostCLI},
		{name: "rejects typos", cfg: "desktp", wantErr: true},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			cfg := base
			cfg.SessionHost = tc.cfg
			set, err := resolveSettings(channelFeishu, cfg, "", 0, "", "", nil, false, tc.flag, "")
			if tc.wantErr {
				if err == nil {
					t.Fatalf("want an error for %q", tc.cfg)
				}
				return
			}
			if err != nil {
				t.Fatalf("resolveSettings: %v", err)
			}
			if set.sessionHost != tc.want {
				t.Fatalf("sessionHost = %q, want %q", set.sessionHost, tc.want)
			}
		})
	}
}

// TestResolveSettingsWechat covers the channel switch: WeChat has no app id, no
// secret and no group surface, and its one hard requirement is a token that only
// a QR login can produce.
func TestResolveSettingsWechat(t *testing.T) {
	t.Setenv("PIGO_HOME", t.TempDir())
	base := config.IMConfig{
		Enabled: true,
		Wechat: config.WechatConfig{
			BotToken:     "tok_1",
			AllowedUsers: []string{"wxid_owner"},
			QueueDepth:   5,
		},
	}

	set, err := resolveSettings(channelWechat, base, "", 0, "", "", nil, false, "", "")
	if err != nil {
		t.Fatalf("resolveSettings: %v", err)
	}
	if set.channel != channelWechat {
		t.Fatalf("channel = %q", set.channel)
	}
	if set.appID != "" {
		t.Fatalf("wechat must not read the feishu app id, got %q", set.appID)
	}
	if set.queueDepth != 5 {
		t.Fatalf("queueDepth = %d, want 5", set.queueDepth)
	}
	if set.allowGroups {
		t.Fatal("wechat has no group surface; allowGroups must be false")
	}

	// A group flag on a channel with no groups is ignored rather than rejected:
	// the operator asked for direct chats either way.
	if _, err := resolveSettings(channelWechat, base, "", 0, "", "", nil, true, "", ""); err != nil {
		t.Fatalf("groups flag on wechat: %v", err)
	}

	// Without a token and without a state file the run must fail with the fix,
	// not start and then refuse every request.
	noToken := base
	noToken.Wechat.BotToken = ""
	_, err = resolveSettings(channelWechat, noToken, "", 0, "", "", nil, false, "", "")
	if err == nil {
		t.Fatal("want an error when wechat has no token")
	}
	if !strings.Contains(err.Error(), "pigo im login") {
		t.Fatalf("error should name the login command, got: %v", err)
	}

	// Unknown channels are rejected by name; they used to be rejected as
	// "not feishu", which read like a typo in the flag rather than a missing
	// adapter.
	if _, err := resolveSettings("telegram", base, "", 0, "", "", nil, false, "", ""); err == nil {
		t.Fatal("want an error for an unknown channel")
	}
}
