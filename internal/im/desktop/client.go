// Package desktop drives a running pigo desktop app through its loopback
// control endpoint.
//
// The desktop app owns its conversations in host-core (SQLite metadata plus
// JSONL transcripts) and resolves provider, plugin and project context inside
// its Rust shell. A separate `pigo im serve` process therefore cannot open
// those sessions the way it opens local ones — but it does not need to: the app
// publishes a small control catalog (session/create, agent/prompt,
// agent/status …) that only needs a session id and a message body. This package
// is the client for that catalog, and it implements im.SessionHost so the
// channel layer drives desktop sessions exactly like local ones. The payoff is
// that a conversation started from Feishu is the same conversation the user
// sees in the app's sidebar.
package desktop

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// descriptorName is the file the app writes on startup, next to its data.
const descriptorName = "control.json"

// Descriptor is that file's contents.
type Descriptor struct {
	Port    int    `json:"port"`
	Token   string `json:"token"`
	PID     int    `json:"pid"`
	Version string `json:"version"`
}

// Client talks to one running desktop app.
type Client struct {
	// Endpoint is the base URL, e.g. http://127.0.0.1:51234.
	Endpoint string
	// Token is the per-launch bearer token from the descriptor.
	Token string
	// HTTP is the transport. Never nil after NewClient.
	HTTP *http.Client
}

// Error is a typed failure from the app. Code mirrors the app's own envelope
// codes, so callers can branch without matching on strings baked into messages.
type Error struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

func (e *Error) Error() string {
	if e.Code == "" {
		return e.Message
	}
	return e.Code + ": " + e.Message
}

// Discover locates a running app through its descriptor file.
//
// PIGO_DESKTOP_CONTROL wins when set (an explicit path, useful for tests and for
// a non-default install). Otherwise the app's data directory is probed, which is
// `~/.pi-agent-go-desktop` unless PI_DESKTOP_DATA_DIR moved it — the same rule
// the app itself uses (boot::resolve_data_dir).
func Discover() (*Client, error) {
	if path := strings.TrimSpace(os.Getenv("PIGO_DESKTOP_CONTROL")); path != "" {
		return NewFromFile(path)
	}
	var candidates []string
	if dir := strings.TrimSpace(os.Getenv("PI_DESKTOP_DATA_DIR")); dir != "" {
		candidates = append(candidates, filepath.Join(dir, descriptorName))
	}
	if home, err := os.UserHomeDir(); err == nil && home != "" {
		candidates = append(candidates, filepath.Join(home, ".pi-agent-go-desktop", descriptorName))
	}
	var lastErr error
	for _, path := range candidates {
		c, err := NewFromFile(path)
		if err == nil {
			return c, nil
		}
		lastErr = err
	}
	if lastErr == nil {
		lastErr = fmt.Errorf("no home directory")
	}
	return nil, fmt.Errorf("desktop control endpoint not found (is the app running?): %w", lastErr)
}

// NewFromFile builds a client from an explicit descriptor path.
func NewFromFile(path string) (*Client, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var desc Descriptor
	if err := json.Unmarshal(data, &desc); err != nil {
		return nil, fmt.Errorf("parse %s: %w", path, err)
	}
	if desc.Port <= 0 || desc.Token == "" {
		return nil, fmt.Errorf("%s has no port/token (app still starting?)", path)
	}
	return NewClient(fmt.Sprintf("http://127.0.0.1:%d", desc.Port), desc.Token), nil
}

// NewClient builds a client against an explicit endpoint.
func NewClient(endpoint, token string) *Client {
	return &Client{
		Endpoint: strings.TrimRight(endpoint, "/"),
		Token:    token,
		HTTP:     &http.Client{Timeout: 30 * time.Second},
	}
}

// envelope is the app's reply shape.
type envelope struct {
	OK    bool            `json:"ok"`
	Data  json.RawMessage `json:"data"`
	Error *Error          `json:"error"`
}

// Invoke calls one catalog operation and returns its data payload.
func (c *Client) Invoke(ctx context.Context, op string, args map[string]any) (json.RawMessage, error) {
	if args == nil {
		args = map[string]any{}
	}
	body, err := json.Marshal(map[string]any{"op": op, "args": args})
	if err != nil {
		return nil, err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.Endpoint+"/invoke", bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+c.Token)

	resp, err := c.HTTP.Do(req)
	if err != nil {
		return nil, fmt.Errorf("desktop control %s: %w", op, err)
	}
	defer resp.Body.Close()
	payload, err := io.ReadAll(io.LimitReader(resp.Body, 8<<20))
	if err != nil {
		return nil, fmt.Errorf("desktop control %s: read reply: %w", op, err)
	}
	if resp.StatusCode == http.StatusUnauthorized || resp.StatusCode == http.StatusForbidden {
		return nil, &Error{Code: "UNAUTHORIZED", Message: "control token rejected (restart the app to refresh it)"}
	}
	var env envelope
	if err := json.Unmarshal(payload, &env); err != nil {
		return nil, fmt.Errorf("desktop control %s: bad reply (%d): %s", op, resp.StatusCode, truncate(string(payload), 200))
	}
	if !env.OK {
		if env.Error != nil {
			return nil, env.Error
		}
		return nil, &Error{Code: "UNKNOWN", Message: "control call failed without a reason"}
	}
	return env.Data, nil
}

func truncate(s string, n int) string {
	if len(s) <= n {
		return s
	}
	return s[:n] + "…"
}
