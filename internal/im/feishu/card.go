package feishu

import (
	"encoding/json"
	"fmt"
	"strings"

	"github.com/smallnest/pigo/internal/im"
)

// cardTextMax bounds the markdown body we put in one card. Feishu rejects cards
// over 30 KB, and the limit counts the serialized JSON including styling, so
// this leaves headroom rather than sitting on the boundary.
const cardTextMax = 9000

// The types below mirror the subset of Feishu's card JSON (v1 message card
// schema) this channel emits. They are hand-written rather than imported from
// the SDK's card builder because the builder's fluent API costs far more code
// than the five fields actually used, and a hand-written struct makes the wire
// shape reviewable in one screen — which matters when a field rename silently
// downgrades a card to plain text.

type cardConfig struct {
	// WideScreenMode lets the card use the full chat width.
	WideScreenMode bool `json:"wide_screen_mode"`
	// UpdateMulti tells Feishu to propagate in-place updates to every client
	// that has the card on screen. Without it, PATCH updates only reach the
	// client that fetched the card last, so a phone could keep showing a stale
	// "处理中…" card forever.
	UpdateMulti bool `json:"update_multi"`
}

type cardText struct {
	Tag     string `json:"tag"`
	Content string `json:"content"`
}

type cardHeader struct {
	Template string    `json:"template,omitempty"`
	Title    *cardText `json:"title,omitempty"`
}

type cardAction struct {
	Tag   string            `json:"tag"`
	Text  *cardText         `json:"text"`
	Type  string            `json:"type,omitempty"`
	Value map[string]string `json:"value,omitempty"`
}

type cardElement struct {
	Tag     string       `json:"tag"`
	Text    *cardText    `json:"text,omitempty"`
	Actions []cardAction `json:"actions,omitempty"`
}

type card struct {
	Config   cardConfig    `json:"config"`
	Header   *cardHeader   `json:"header,omitempty"`
	Elements []cardElement `json:"elements"`
}

// toneTemplate maps a message tone onto a card header colour.
func toneTemplate(tone string) string {
	switch tone {
	case im.ToneWarn:
		return "orange"
	case im.ToneError:
		return "red"
	case im.ToneSuccess:
		return "green"
	default:
		return "blue"
	}
}

// buttonType maps a button style onto Feishu's button types.
func buttonType(style string) string {
	switch style {
	case "primary", "danger", "default":
		return style
	case "warn":
		return "primary"
	default:
		return "default"
	}
}

// buildCard renders an outbound message as a card JSON string.
//
// Every message goes out as a card, including plain replies. That is a
// deliberate uniformity: Feishu's PATCH endpoint only updates interactive
// messages, so a channel that sometimes sent text could not reliably turn its
// progress message into the final answer. One message type, one update path.
func buildCard(out im.Outbound) (string, error) {
	body := strings.TrimSpace(out.Text)
	if body == "" {
		body = "（空）"
	}
	if len([]rune(body)) > cardTextMax {
		body = string([]rune(body)[:cardTextMax]) + "\n\n…（已截断）"
	}

	c := card{
		Config: cardConfig{WideScreenMode: true, UpdateMulti: true},
		Elements: []cardElement{{
			Tag:  "div",
			Text: &cardText{Tag: "lark_md", Content: body},
		}},
	}
	if out.Title != "" {
		c.Header = &cardHeader{
			Template: toneTemplate(out.Tone),
			Title:    &cardText{Tag: "plain_text", Content: out.Title},
		}
	}
	if out.Footer != "" {
		c.Elements = append(c.Elements,
			cardElement{Tag: "hr"},
			cardElement{Tag: "div", Text: &cardText{Tag: "lark_md", Content: "_" + out.Footer + "_"}},
		)
	}
	if len(out.Buttons) > 0 {
		actions := make([]cardAction, 0, len(out.Buttons))
		for _, b := range out.Buttons {
			actions = append(actions, cardAction{
				Tag:  "button",
				Text: &cardText{Tag: "plain_text", Content: b.Text},
				Type: buttonType(b.Style),
				// The value object is echoed back verbatim in the card action
				// callback, and is the only channel carrying the correlation
				// token, so it must stay small and self-describing.
				Value: map[string]string{"action": b.Action, "token": b.Token},
			})
		}
		c.Elements = append(c.Elements, cardElement{Tag: "action", Actions: actions})
	}

	raw, err := json.Marshal(c)
	if err != nil {
		return "", fmt.Errorf("feishu: encode card: %w", err)
	}
	return string(raw), nil
}

// buildDecisionCard renders the post-decision state of an approval card, so the
// buttons disappear once pressed instead of inviting a second click.
func buildDecisionCard(title, body, tone string) (string, error) {
	return buildCard(im.Outbound{Title: title, Text: body, Tone: tone})
}

// approvalButtons builds the three-button approval row.
func approvalButtons(token string) []im.Button {
	return []im.Button{
		{Text: "允许", Action: "approve", Token: token, Style: "primary"},
		{Text: "本会话允许", Action: "approve_always", Token: token, Style: "default"},
		{Text: "拒绝", Action: "deny", Token: token, Style: "danger"},
	}
}
