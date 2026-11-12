package desktop

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/smallnest/pigo/internal/im"
)

// permissionWire mirrors host-core's PendingPermission. The request fields are
// flattened into the same object, so a late-attaching client sees exactly what
// the original `permissions.request` notification carried.
type permissionWire struct {
	RequestID   string          `json:"requestId"`
	SessionID   string          `json:"sessionId"`
	ToolCallID  string          `json:"toolCallId"`
	ToolName    string          `json:"toolName"`
	Risk        string          `json:"risk"`
	ArgsPreview json.RawMessage `json:"argsPreview"`
	Reason      string          `json:"reason"`
	TimeoutMS   uint64          `json:"timeoutMs"`
	RemainingMS uint64          `json:"remainingMs"`
	CreatedAt   string          `json:"createdAt"`
	ExpiresAt   string          `json:"expiresAt"`
}

// Pending implements im.PermissionGate over the control endpoint. Scoping by
// session is not cosmetic: without it a chat could be shown — and could answer
// — a confirmation belonging to a completely different conversation.
func (c *Client) Pending(ctx context.Context, sessionID string) ([]im.PendingPermission, error) {
	raw, err := c.Invoke(ctx, "permissions/pending", map[string]any{"sessionId": sessionID})
	if err != nil {
		if hasCode(err, "NOT_ALLOWED") {
			return nil, im.ErrPermissionUnsupported
		}
		return nil, err
	}
	var payload struct {
		Requests []permissionWire `json:"requests"`
	}
	if err := json.Unmarshal(raw, &payload); err != nil {
		return nil, fmt.Errorf("desktop permissions/pending: %w", err)
	}
	out := make([]im.PendingPermission, 0, len(payload.Requests))
	for _, w := range payload.Requests {
		if w.RequestID == "" {
			continue
		}
		out = append(out, im.PendingPermission{
			RequestID: w.RequestID,
			SessionID: w.SessionID,
			ToolName:  w.ToolName,
			Reason:    w.Reason,
			Summary:   previewText(w.ArgsPreview),
			Remaining: time.Duration(w.RemainingMS) * time.Millisecond,
		})
	}
	return out, nil
}

// Resolve implements im.PermissionGate. A host that has already dropped the
// request (the window answered it, or its 120s timeout fired) returns
// NOT_FOUND; the caller treats that as settled-outside-the-channel rather than
// as a failure to report.
func (c *Client) Resolve(ctx context.Context, requestID string, decision im.PermissionDecision) error {
	_, err := c.Invoke(ctx, "permissions/resolve", map[string]any{
		"requestId": requestID,
		"decision":  string(decision),
	})
	if err != nil && hasCode(err, "NOT_FOUND") {
		return im.ErrPermissionGone
	}
	return err
}

// hasCode reads the control endpoint's typed envelope error without string
// matching on a human-readable message.
func hasCode(err error, code string) bool {
	var typed *Error
	return errors.As(err, &typed) && strings.EqualFold(typed.Code, code)
}

// previewText renders the host's bounded args preview for a chat card. Objects
// stay compact JSON (the renderer wraps it in a code block); a plain string is
// already the most readable form.
func previewText(raw json.RawMessage) string {
	trimmed := strings.TrimSpace(string(raw))
	if trimmed == "" || trimmed == "null" {
		return ""
	}
	var text string
	if err := json.Unmarshal(raw, &text); err == nil {
		return text
	}
	return trimmed
}
