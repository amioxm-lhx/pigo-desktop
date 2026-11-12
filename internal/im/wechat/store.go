package wechat

import (
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"time"
)

// stateVersion is the on-disk schema version, so a future shape can migrate
// instead of guessing at an old file.
const stateVersion = 1

// State is everything that has to survive a restart.
//
// The bot token alone is not enough. The channel also needs the getupdates
// cursor, or a restart replays the last batch of messages and the agent answers
// things it already answered; and it needs each peer's context_token, because an
// outbound message without one is accepted but not threaded into the right
// conversation.
type State struct {
	Version       int               `json:"version"`
	Token         string            `json:"token,omitempty"`
	BotID         string            `json:"bot_id,omitempty"`
	UserID        string            `json:"user_id,omitempty"`
	BaseURL       string            `json:"base_url,omitempty"`
	SavedAt       time.Time         `json:"saved_at,omitempty"`
	SyncBuf       string            `json:"sync_buf,omitempty"`
	ContextTokens map[string]string `json:"context_tokens,omitempty"`
}

// DefaultStatePath returns <PIGO_HOME>/im/wechat.json, falling back to
// ~/.pigo/im/wechat.json — the same root the chat→session bindings use, so one
// directory holds the whole channel's durable state.
func DefaultStatePath() string {
	if dir := os.Getenv("PIGO_HOME"); dir != "" {
		return filepath.Join(dir, "im", "wechat.json")
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return ""
	}
	return filepath.Join(home, ".pigo", "im", "wechat.json")
}

// LoadState reads the state file. A missing file is an empty state, not an
// error: that is simply "not logged in yet". A malformed file IS an error —
// silently starting over would throw away the cursor and the context tokens
// with no explanation.
func LoadState(path string) (State, error) {
	st := State{Version: stateVersion, ContextTokens: map[string]string{}}
	if path == "" {
		return st, nil
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		if errors.Is(err, fs.ErrNotExist) {
			return st, nil
		}
		return st, fmt.Errorf("wechat: read %s: %w", path, err)
	}
	if len(raw) == 0 {
		return st, nil
	}
	if err := json.Unmarshal(raw, &st); err != nil {
		return st, fmt.Errorf("wechat: parse %s: %w", path, err)
	}
	if st.ContextTokens == nil {
		st.ContextTokens = map[string]string{}
	}
	return st, nil
}

// SaveState writes the state atomically: temp file in the same directory, then
// rename. A crash mid-write leaves the previous file intact instead of a
// truncated one that would read as "logged out".
//
// The file carries a bot token, so it is created 0600 and the directory 0700.
func SaveState(path string, st State) error {
	if path == "" {
		return errors.New("wechat: state path is empty")
	}
	if st.Version == 0 {
		st.Version = stateVersion
	}
	st.SavedAt = time.Now()
	if st.ContextTokens == nil {
		st.ContextTokens = map[string]string{}
	}
	raw, err := json.MarshalIndent(st, "", "  ")
	if err != nil {
		return fmt.Errorf("wechat: encode state: %w", err)
	}
	raw = append(raw, '\n')

	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return fmt.Errorf("wechat: create %s: %w", dir, err)
	}
	tmp, err := os.CreateTemp(dir, ".wechat-*.tmp")
	if err != nil {
		return fmt.Errorf("wechat: create temp state: %w", err)
	}
	tmpName := tmp.Name()
	defer os.Remove(tmpName)

	if err := tmp.Chmod(0o600); err != nil {
		tmp.Close()
		return fmt.Errorf("wechat: chmod temp state: %w", err)
	}
	if _, err := tmp.Write(raw); err != nil {
		tmp.Close()
		return fmt.Errorf("wechat: write state: %w", err)
	}
	if err := tmp.Sync(); err != nil {
		tmp.Close()
		return fmt.Errorf("wechat: sync state: %w", err)
	}
	if err := tmp.Close(); err != nil {
		return fmt.Errorf("wechat: close state: %w", err)
	}
	if err := os.Rename(tmpName, path); err != nil {
		return fmt.Errorf("wechat: replace %s: %w", path, err)
	}
	return nil
}

// CredentialsFromState projects the login fields out of a state file.
func CredentialsFromState(st State) *Credentials {
	if st.Token == "" {
		return nil
	}
	return &Credentials{
		Token:   st.Token,
		BotID:   st.BotID,
		UserID:  st.UserID,
		BaseURL: st.BaseURL,
	}
}

// ApplyCredentials folds a successful login into the state, preserving the
// cursor and context tokens of the previous binding.
//
// Those are dropped when the account changes, not when the token is refreshed:
// the cursor is scoped to the bot account, so reusing one across a different
// account would skip that account's backlog.
func ApplyCredentials(st State, creds *Credentials) State {
	if creds == nil {
		return st
	}
	if st.BotID != "" && st.BotID != creds.BotID {
		st.SyncBuf = ""
		st.ContextTokens = map[string]string{}
	}
	st.Token = creds.Token
	st.BotID = creds.BotID
	st.UserID = creds.UserID
	st.BaseURL = creds.BaseURL
	return st
}
