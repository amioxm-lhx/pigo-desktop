package feishu

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/smallnest/pigo/internal/im"
)

// decodeCard unmarshals a rendered card back into a generic map so the test
// asserts on the wire shape rather than on our own struct field names.
func decodeCard(t *testing.T, raw string) map[string]any {
	t.Helper()
	var m map[string]any
	if err := json.Unmarshal([]byte(raw), &m); err != nil {
		t.Fatalf("card is not valid JSON: %v\n%s", err, raw)
	}
	return m
}

func headerTemplate(t *testing.T, m map[string]any) string {
	t.Helper()
	h, ok := m["header"].(map[string]any)
	if !ok {
		t.Fatalf("card has no header: %v", m)
	}
	return h["template"].(string)
}

// TestBuildCardIsValidJSONAndSetsUpdateMulti covers the two properties PATCH
// depends on: valid JSON, and update_multi so in-place updates reach every
// client showing the card.
func TestBuildCardIsValidJSONAndSetsUpdateMulti(t *testing.T) {
	raw, err := buildCard(im.Outbound{Title: "pigo · 处理中", Text: "hi", Tone: im.ToneWarn})
	if err != nil {
		t.Fatalf("buildCard: %v", err)
	}
	m := decodeCard(t, raw)
	cfg := m["config"].(map[string]any)
	if cfg["update_multi"] != true {
		t.Fatalf("update_multi = %v, want true", cfg["update_multi"])
	}
	if got := headerTemplate(t, m); got != "orange" {
		t.Fatalf("template = %q, want orange for warn tone", got)
	}
}

func TestToneMapsToTemplate(t *testing.T) {
	cases := map[string]string{
		im.ToneInfo:    "blue",
		im.ToneWarn:    "orange",
		im.ToneError:   "red",
		im.ToneSuccess: "green",
	}
	for tone, want := range cases {
		raw, err := buildCard(im.Outbound{Title: "t", Text: "b", Tone: tone})
		if err != nil {
			t.Fatalf("buildCard(%q): %v", tone, err)
		}
		if got := headerTemplate(t, decodeCard(t, raw)); got != want {
			t.Fatalf("tone %q -> %q, want %q", tone, got, want)
		}
	}
}

// TestBuildCardButtonsCarryToken is what makes an approval resolvable: the token
// rides in the button value and comes back in the card callback.
func TestBuildCardButtonsCarryToken(t *testing.T) {
	raw, err := buildCard(im.Outbound{
		Title:   "need confirm",
		Text:    "run bash?",
		Buttons: approvalButtons("tok-123"),
	})
	if err != nil {
		t.Fatalf("buildCard: %v", err)
	}
	m := decodeCard(t, raw)
	elements := m["elements"].([]any)
	last := elements[len(elements)-1].(map[string]any)
	if last["tag"] != "action" {
		t.Fatalf("last element tag = %v, want action", last["tag"])
	}
	actions := last["actions"].([]any)
	if len(actions) != 3 {
		t.Fatalf("button count = %d, want 3", len(actions))
	}
	var names []string
	for _, a := range actions {
		btn := a.(map[string]any)
		value := btn["value"].(map[string]any)
		if value["token"] != "tok-123" {
			t.Fatalf("button token = %v, want tok-123", value["token"])
		}
		names = append(names, value["action"].(string))
	}
	want := []string{"approve", "approve_always", "deny"}
	for i := range want {
		if names[i] != want[i] {
			t.Fatalf("actions = %v, want %v", names, want)
		}
	}
}

// TestBuildCardTruncatesLongBody guards the 30 KB card limit: a long answer must
// degrade to a marked truncation, not to a rejected send.
func TestBuildCardTruncatesLongBody(t *testing.T) {
	long := strings.Repeat("长", cardTextMax+500)
	raw, err := buildCard(im.Outbound{Title: "big", Text: long})
	if err != nil {
		t.Fatalf("buildCard: %v", err)
	}
	m := decodeCard(t, raw)
	body := m["elements"].([]any)[0].(map[string]any)["text"].(map[string]any)["content"].(string)
	if !strings.Contains(body, "已截断") {
		t.Fatal("long body was not marked as truncated")
	}
	if len([]rune(body)) > cardTextMax+20 {
		t.Fatalf("body still %d runes after truncation", len([]rune(body)))
	}
}

func TestBuildCardEmptyTextDoesNotEmitBlankCard(t *testing.T) {
	raw, err := buildCard(im.Outbound{Title: "t"})
	if err != nil {
		t.Fatalf("buildCard: %v", err)
	}
	body := decodeCard(t, raw)["elements"].([]any)[0].(map[string]any)["text"].(map[string]any)["content"].(string)
	if strings.TrimSpace(body) == "" {
		t.Fatal("empty text produced an empty card body")
	}
}

func TestBuildCardFooterAddsSeparator(t *testing.T) {
	raw, err := buildCard(im.Outbound{Title: "t", Text: "b", Footer: "2 步"})
	if err != nil {
		t.Fatalf("buildCard: %v", err)
	}
	elements := decodeCard(t, raw)["elements"].([]any)
	if len(elements) != 3 {
		t.Fatalf("element count = %d, want 3 (body, hr, footer)", len(elements))
	}
	if elements[1].(map[string]any)["tag"] != "hr" {
		t.Fatalf("second element = %v, want hr", elements[1])
	}
}
