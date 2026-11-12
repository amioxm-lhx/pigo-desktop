package desktop

import (
	"context"
	"encoding/json"
	"fmt"
	"time"

	"github.com/smallnest/pigo/internal/im"
)

// sessionSummary mirrors host-core's SessionSummary (camelCase on the wire).
type sessionSummary struct {
	ID          string `json:"id"`
	Title       string `json:"title"`
	ProjectPath string `json:"projectPath"`
	Mode        string `json:"mode"`
	ModelID     string `json:"modelId"`
	UpdatedAt   string `json:"updatedAt"`
}

type sessionPayload struct {
	Session *sessionSummary `json:"session"`
}

type listPayload struct {
	Sessions []sessionSummary `json:"sessions"`
}

// CreateSession starts a durable session in the app. spec.Cwd becomes the
// session's project so the conversation lands in the right project group
// instead of an unattributed session list.
func (c *Client) CreateSession(ctx context.Context, spec im.SessionSpec) (im.AgentSession, error) {
	args := map[string]any{}
	if spec.Cwd != "" {
		args["projectPath"] = spec.Cwd
	}
	if spec.Model != "" {
		args["modelId"] = spec.Model
	}
	raw, err := c.Invoke(ctx, "session/create", args)
	if err != nil {
		return nil, err
	}
	var payload sessionPayload
	if err := json.Unmarshal(raw, &payload); err != nil {
		return nil, fmt.Errorf("desktop session/create: %w", err)
	}
	if payload.Session == nil || payload.Session.ID == "" {
		return nil, fmt.Errorf("desktop session/create returned no id")
	}
	return &session{c: c, id: payload.Session.ID}, nil
}

// OpenSession attaches to an existing session. The existence check is a real
// read rather than a local assumption: the user may have deleted the session in
// the app while the chat kept pointing at it.
func (c *Client) OpenSession(ctx context.Context, id string) (im.AgentSession, error) {
	raw, err := c.Invoke(ctx, "session/get", map[string]any{"id": id, "messageLimit": 1})
	if err != nil {
		return nil, err
	}
	var payload sessionPayload
	if err := json.Unmarshal(raw, &payload); err != nil {
		return nil, fmt.Errorf("desktop session/get: %w", err)
	}
	if payload.Session == nil {
		return nil, fmt.Errorf("会话 %s 在桌面端已不存在", id)
	}
	return &session{c: c, id: id}, nil
}

// ListSessions implements /sessions against the app's durable list.
func (c *Client) ListSessions(ctx context.Context) ([]im.SessionSummary, error) {
	raw, err := c.Invoke(ctx, "session/list", nil)
	if err != nil {
		return nil, err
	}
	var payload listPayload
	if err := json.Unmarshal(raw, &payload); err != nil {
		return nil, fmt.Errorf("desktop session/list: %w", err)
	}
	out := make([]im.SessionSummary, 0, len(payload.Sessions))
	for _, s := range payload.Sessions {
		updated, _ := time.Parse(time.RFC3339, s.UpdatedAt)
		out = append(out, im.SessionSummary{
			ID:        s.ID,
			Title:     s.Title,
			Cwd:       s.ProjectPath,
			UpdatedAt: updated,
		})
	}
	return out, nil
}
