// Package config implements pigo's optional user config file at
// ~/.config/pigo/config.toml (honoring $XDG_CONFIG_HOME when set) plus the
// provider-agnostic base-url env-var name derivation. Values in the file
// replace pigo's built-in defaults, but an explicit command-line flag always
// wins over the file:
//
//	command-line flag > config.toml > built-in default
//
// A missing file is not an error (defaults apply); a malformed file is surfaced
// to the caller so it can warn rather than silently ignore user intent.
//
// The package is intentionally free of any cliOptions/run-assembly concern: it
// only loads and decodes the file and derives env-var names. Overlaying a
// FileConfig onto the parsed CLI options lives in cmd/pigo, alongside the
// options struct it mutates.
package config

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/BurntSushi/toml"

	"github.com/smallnest/pigo/internal/mcp"
)

// FileConfig is the on-disk shape of config.toml. Every field is optional; an
// absent (zero-value) field leaves the corresponding default/flag untouched.
// Keys are snake_case to read naturally in TOML.
type FileConfig struct {
	Model   string `toml:"model"`
	BaseURL string `toml:"base_url"`
	APIKey  string `toml:"api_key"`
	// Credential is a named credential REFERENCE (issue #568): the literal key
	// lives in $PIGO_HOME/.credentials.yaml (mode 0600) under this name, and
	// config.toml carries only the name. It resolves to the API-key tier, below
	// an explicit --api-key flag. Empty means no reference.
	Credential    string `toml:"credential"`
	Protocol      string `toml:"protocol"`
	Provider      string `toml:"provider"`
	ThinkingLevel string `toml:"thinking_level"`
	OutputFormat  string `toml:"output_format"`
	NoTools       bool   `toml:"no_tools"`
	NoSkills      bool   `toml:"no_skills"`
	Approve       bool   `toml:"approve"`
	SystemPrompt  string `toml:"system_prompt"`
	// AllowedTools and DisallowedTools are the tool-level admission boundary:
	// the config-file tier of --allowed-tools / --disallowed-tools. Names match
	// case-insensitively and DisallowedTools wins when a name appears in both.
	// A CLI flag replaces the file value wholesale rather than merging with it,
	// so passing --allowed-tools can widen a boundary the file narrowed.
	AllowedTools    []string `toml:"allowed_tools"`
	DisallowedTools []string `toml:"disallowed_tools"`
	// Prompts is the config.toml `prompts` array: paths (files or dirs) to load
	// prompt templates from at the settings tier (mirrors pi's settings prompts).
	Prompts []string `toml:"prompts"`
	// Memory, Checkpoint, and Compaction are nested TOML tables for the
	// persistent-memory / infinite-context feature. They are pure config
	// plumbing here; defaults/parsing live in memory.go (Resolve* helpers) and
	// the overlay into runtime options lives in cmd/pigo. See
	// tasks/spec-persistent-memory-infinite-context.md §3/§4/§5.2.
	Memory     MemoryConfig     `toml:"memory"`
	Checkpoint CheckpointConfig `toml:"checkpoint"`
	Compaction CompactionConfig `toml:"compaction"`
	// Dream is the [dream] TOML table for the /dream memory-consolidation
	// feature. Pure config plumbing here; defaults/normalization live in
	// internal/dream (Config). See tasks/spec-dream-memory-consolidation.md
	// §3.3.
	Dream DreamConfig `toml:"dream"`
	// MCP is the [mcp] TOML table for the native MCP client (US-014/US-015):
	// an enabled master switch plus the servers tier ([[mcp.servers]]). The
	// entry shape is the frozen mcp.toml shape (internal/mcp.Server), shared
	// with the standalone mcp.toml file the bridge and the default resolution
	// read. Pure config plumbing here: overlay into run.MCPSettings lives in
	// cmd/pigo, per this package's decode-only contract.
	MCP MCPConfig `toml:"mcp"`
	// IM is the [im] TOML table for the remote-control channels (Feishu today).
	// Pure config plumbing here: defaults, duration parsing and credential
	// resolution live in internal/cli/imcmd, per this package's decode-only
	// contract. See tasks/prd-im-remote-channels.md.
	IM IMConfig `toml:"im"`
}

// IMConfig is the [im] TOML table.
//
// Secrets are referenced, never embedded: app_secret is normally a
// `credential = "name"` pointer into $PIGO_HOME/.credentials.yaml (mode 0600).
// The literal app_secret field exists for throwaway testing only.
type IMConfig struct {
	// Enabled is the master switch. A disabled channel does not start even when
	// `pigo im serve` is invoked explicitly.
	Enabled bool `toml:"enabled"`
	// ToolPolicy is "confirm" (default), "allow" or "deny" — see
	// im.ToolPolicy. "allow" means a phone message can run bash unasked.
	ToolPolicy string `toml:"tool_policy"`
	// ApprovalTimeout is a duration string ("10m"). Empty keeps the default.
	ApprovalTimeout string `toml:"approval_timeout"`
	// SessionCwd is the working directory new IM sessions are bound to. Empty
	// means the process working directory at startup.
	SessionCwd string `toml:"session_cwd"`
	// SessionHost decides where the sessions this channel drives actually live:
	// "cli" (default) runs them inside the im process; "desktop" attaches to a
	// running pigo desktop app over its loopback control endpoint, so a chat
	// from Feishu continues the same conversation the app shows.
	SessionHost string `toml:"session_host"`
	// EgressWarning, default true, prints a one-time notice that conversation
	// content leaves the machine through the platform.
	EgressWarning *bool `toml:"egress_warning"`
	// Feishu is the [im.feishu] table.
	Feishu FeishuConfig `toml:"feishu"`
	// Wechat is the [im.wechat] table.
	Wechat WechatConfig `toml:"wechat"`
}

// WechatConfig is the [im.wechat] TOML table.
//
// This channel has no app to register and no secret to copy: it is bound by
// scanning a QR code once (`pigo im login --channel wechat`), and the resulting
// bot token is written to $PIGO_HOME/im/wechat.json in the same directory as the
// chat bindings. That token file is why there is no `credential` reference here
// — putting a rotating token in a hand-edited config would only create a second
// place to update it.
type WechatConfig struct {
	Enabled bool `toml:"enabled"`
	// BotToken overrides the token from the state file. Normally empty; it
	// exists for a deployment that manages the token as an environment-injected
	// secret rather than through an interactive login on the box.
	BotToken string `toml:"bot_token"`
	// BaseURL overrides the API root (default https://ilinkai.weixin.qq.com).
	BaseURL string `toml:"base_url"`
	// AllowedUsers lists the ilink user ids allowed to control the agent. Empty
	// means the channel refuses to start (fail-closed), exactly like Feishu.
	AllowedUsers []string `toml:"allowed_users"`
	// QueueDepth is the per-chat backlog bound (default 3).
	QueueDepth int `toml:"queue_depth"`
	// ProgressThrottle is how often the typing indicator may be refreshed.
	ProgressThrottle string `toml:"progress_throttle"`
	// RejectCooldown throttles the "queue is full" reply.
	RejectCooldown string `toml:"reject_cooldown"`
	// Model and Cwd override the top-level session settings for this channel.
	Model string `toml:"model"`
	Cwd   string `toml:"cwd"`
}

// FeishuConfig is the [im.feishu] TOML table.
type FeishuConfig struct {
	Enabled bool `toml:"enabled"`
	// AppID is the app's public identifier (not a secret).
	AppID string `toml:"app_id"`
	// AppSecret is the literal secret. Prefer Credential.
	AppSecret string `toml:"app_secret"`
	// Credential names an entry in $PIGO_HOME/.credentials.yaml holding the
	// app secret. It wins over AppSecret when both are set.
	Credential string `toml:"credential"`
	// AllowedUsers lists open_id or email of the humans allowed to control the
	// agent. Empty means the channel refuses to start (fail-closed).
	AllowedUsers []string `toml:"allowed_users"`
	// AllowGroups permits group chats; they additionally require an @-mention.
	AllowGroups bool `toml:"allow_groups"`
	// RequireMention overrides the group mention requirement (default true).
	RequireMention *bool `toml:"require_mention"`
	// QueueDepth is the per-chat backlog bound (default 3).
	QueueDepth int `toml:"queue_depth"`
	// ProgressThrottle is the minimum interval between progress-card updates.
	ProgressThrottle string `toml:"progress_throttle"`
	// RejectCooldown throttles the "queue is full" reply.
	RejectCooldown string `toml:"reject_cooldown"`
	// Model and Cwd override the top-level session settings for this channel.
	Model string `toml:"model"`
	Cwd   string `toml:"cwd"`
}

// MCPConfig is the [mcp] TOML table. Enabled is a pointer so an absent key
// (nil) is distinguishable from an explicit false: nil is treated as on, only
// enabled = false (or --no-mcp) turns discovery off.
type MCPConfig struct {
	Enabled *bool        `toml:"enabled"`
	Servers []mcp.Server `toml:"servers"`
}

// DreamConfig is the [dream] TOML table for /dream memory consolidation.
// Enabled is a pointer so an absent key (nil) is distinguishable from an
// explicit false: nil is treated as true, only enabled = false disables
// auto-trigger. IntervalDays and RecentSessions use zero as "apply default"
// (7 and 20 respectively); normalization lives in dream.Config.
type DreamConfig struct {
	Enabled        *bool `toml:"enabled"`
	IntervalDays   int   `toml:"interval_days"`
	RecentSessions int   `toml:"recent_sessions"`
}

// FileConfigPath returns the path to the user config file:
// $XDG_CONFIG_HOME/pigo/config.toml, or ~/.config/pigo/config.toml by default.
// It returns "" when neither can be resolved, so the caller treats the file as
// absent.
func FileConfigPath() string {
	if dir := os.Getenv("XDG_CONFIG_HOME"); dir != "" {
		return filepath.Join(dir, "pigo", "config.toml")
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return ""
	}
	return filepath.Join(home, ".config", "pigo", "config.toml")
}

// LoadFileConfig reads and decodes config.toml. A missing file (or an empty
// path) returns a zero config with no error; a malformed file is an error.
func LoadFileConfig(path string) (FileConfig, error) {
	if path == "" {
		return FileConfig{}, nil
	}
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return FileConfig{}, nil
		}
		return FileConfig{}, fmt.Errorf("read config %s: %w", path, err)
	}
	var cfg FileConfig
	if err := toml.Unmarshal(data, &cfg); err != nil {
		return FileConfig{}, fmt.Errorf("parse config %s: %w", path, err)
	}
	return cfg, nil
}

// GenericBaseURLEnvVar derives the generic base-url override env var name for a
// provider: the provider name uppercased with hyphens rewritten to underscores,
// suffixed with _BASE_URL. For example "zai-coding-cn" → "ZAI_CODING_CN_BASE_URL"
// and "deepseek" → "DEEPSEEK_BASE_URL". An empty provider name yields "".
//
// It lives here (not with ResolveBaseURL) because it is a pure name derivation
// with no dependency on the provider registry — the provider-agnostic part of
// base-url resolution. ResolveBaseURL itself lives in internal/provider.
func GenericBaseURLEnvVar(providerName string) string {
	n := strings.TrimSpace(providerName)
	if n == "" {
		return ""
	}
	n = strings.ReplaceAll(n, "-", "_")
	return strings.ToUpper(n) + "_BASE_URL"
}
