package wechat

import (
	"strings"
	"testing"

	"github.com/smallnest/pigo/internal/im"
)

// TestPlainText covers the markdown that leaks into a plain-text bubble from the
// shared message builders. Left alone it renders as literal asterisks and
// backticks, which reads as broken output rather than as emphasis.
func TestPlainText(t *testing.T) {
	cases := map[string]string{
		"**会话** `abc`\n":              "会话 abc\n",
		"### 标题\n正文":                  "标题\n正文",
		"__粗体__ 和 **另一个**":            "粗体 和 另一个",
		"[文档](https://example.com/x)": "文档 (https://example.com/x)",
		"- 一\n- 二":                    "- 一\n- 二",
		"":                            "",
		"没有标记":                        "没有标记",
	}
	for in, want := range cases {
		if got := plainText(in); got != want {
			t.Fatalf("plainText(%q) = %q, want %q", in, got, want)
		}
	}
}

// TestRenderMessage pins the bubble layout: bracketed title, stripped body, and a
// footer separated by a blank line. The title is bracketed because WeChat has no
// card header — a bare "pigo · 完成 · 3s" would read as part of the answer.
func TestRenderMessage(t *testing.T) {
	got := renderMessage(im.Outbound{
		Title:  "pigo · 完成 · 3s",
		Text:   "**结果**在这里",
		Footer: "chat · 2 步",
	})
	want := "【pigo · 完成 · 3s】\n结果在这里\n\nchat · 2 步"
	if got != want {
		t.Fatalf("renderMessage:\n got %q\nwant %q", got, want)
	}

	// Buttons cannot be pressed, but hiding that a choice existed is worse than
	// rendering them as a list.
	if got := renderMessage(im.Outbound{Text: "选一个", Buttons: []im.Button{{Text: "同意"}, {Text: "拒绝"}}}); got !=
		"选一个\n\n可选操作：\n1. 同意\n2. 拒绝" {
		t.Fatalf("buttons rendered as %q", got)
	}

	if got := renderMessage(im.Outbound{}); got != "" {
		t.Fatalf("an empty outbound must produce no message, got %q", got)
	}
}

// TestParseDecision is the guard against the two failure modes of a text
// approval: accepting a sentence as consent, and refusing an obvious "同意"
// because of trailing punctuation a phone keyboard added.
func TestParseDecision(t *testing.T) {
	cases := []struct {
		in     string
		ok     bool
		approv bool
		always bool
	}{
		{"同意", true, true, false},
		{"同意。", true, true, false},
		{"  同意\n", true, true, false},
		{"OK", true, true, false},
		{"本会话允许", true, true, true},
		{"拒绝", true, false, false},
		{"不同意", true, false, false},
		{"no", true, false, false},
		// A sentence that merely contains the word is not consent.
		{"我不同意这个方案，先看看别的", false, false, false},
		{"帮我看看这个目录", false, false, false},
		{"", false, false, false},
	}
	for _, tc := range cases {
		d, ok := parseDecision(tc.in)
		if ok != tc.ok {
			t.Fatalf("parseDecision(%q) ok = %v, want %v", tc.in, ok, tc.ok)
		}
		if !ok {
			continue
		}
		if d.Approve != tc.approv || d.Always != tc.always {
			t.Fatalf("parseDecision(%q) = %+v, want approve=%v always=%v", tc.in, d, tc.approv, tc.always)
		}
	}
}

// TestRenderApprovalStatesWhatToType: with no buttons, the instruction is the
// only way the user knows how to answer.
func TestRenderApprovalStatesWhatToType(t *testing.T) {
	got := renderApproval(im.ApprovalReq{Tool: "bash", Summary: "rm -rf **tmp**"})
	for _, want := range []string{"需要确认", "bash", "rm -rf tmp", "回复「同意」", "回复「拒绝」"} {
		if !strings.Contains(got, want) {
			t.Fatalf("approval text is missing %q:\n%s", want, got)
		}
	}
}
