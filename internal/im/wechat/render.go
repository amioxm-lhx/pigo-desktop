package wechat

import (
	"fmt"
	"regexp"
	"strings"

	"github.com/smallnest/pigo/internal/im"
)

// WeChat renders a plain text bubble. There is no markdown, no card, and no
// button: everything the Feishu channel expresses as a card has to become text
// that reads well in a chat app. That is what this file is for.

var (
	// **bold**, __bold__
	reBold = regexp.MustCompile(`\*\*([^*]+)\*\*|__([^_]+)__`)
	// `inline code`
	reCodeSpan = regexp.MustCompile("`([^`]+)`")
	// leading ATX heading markers
	reHeading = regexp.MustCompile(`(?m)^#{1,6}\s*`)
	// [label](url) — WeChat does not linkify, so the label has to carry the url
	reLink = regexp.MustCompile(`\[([^\]]+)\]\((https?://[^)\s]+)\)`)
)

// renderMessage turns one Outbound into the text of a chat bubble.
func renderMessage(msg im.Outbound) string {
	var b strings.Builder

	if title := strings.TrimSpace(msg.Title); title != "" {
		// The title already reads like "pigo · 完成 · 12s"; bracket it so the
		// body is visually separate without spending a blank line on it.
		b.WriteString("【")
		b.WriteString(title)
		b.WriteString("】")
	}

	body := strings.TrimSpace(plainText(msg.Text))
	if body != "" {
		if b.Len() > 0 {
			b.WriteString("\n")
		}
		b.WriteString(body)
	}

	// Buttons become a numbered list. A channel that cannot press them should
	// at least not hide that a choice existed.
	if len(msg.Buttons) > 0 {
		if b.Len() > 0 {
			b.WriteString("\n")
		}
		b.WriteString("\n可选操作：\n")
		for i, btn := range msg.Buttons {
			fmt.Fprintf(&b, "%d. %s\n", i+1, strings.TrimSpace(btn.Text))
		}
	}

	if footer := strings.TrimSpace(msg.Footer); footer != "" {
		b.WriteString("\n\n")
		b.WriteString(footer)
	}
	return strings.TrimSpace(b.String())
}

// plainText strips the markdown that survived from the shared message builders
// but reads as noise in a plain-text bubble.
func plainText(s string) string {
	if s == "" {
		return ""
	}
	s = reLink.ReplaceAllString(s, "$1 ($2)")
	s = reBold.ReplaceAllStringFunc(s, func(m string) string {
		inner := strings.TrimSuffix(strings.TrimPrefix(m, "**"), "**")
		inner = strings.TrimSuffix(strings.TrimPrefix(inner, "__"), "__")
		return inner
	})
	s = reCodeSpan.ReplaceAllString(s, "$1")
	s = reHeading.ReplaceAllString(s, "")
	return s
}

// renderApproval is the confirmation question. It states the tool, what it will
// do, and exactly what to type back — the last part matters because the channel
// has no buttons, so an unclear instruction reads as a permission the user
// cannot grant.
func renderApproval(req im.ApprovalReq) string {
	var b strings.Builder
	b.WriteString("【pigo · 需要确认】\n")
	b.WriteString("这个操作会改动文件或执行命令，需要你同意：\n\n")
	fmt.Fprintf(&b, "工具：%s\n", req.Tool)
	if s := strings.TrimSpace(req.Summary); s != "" {
		fmt.Fprintf(&b, "内容：%s\n", plainText(s))
	}
	b.WriteString("\n回复「同意」执行本次操作；回复「本会话允许」则本会话内同类操作不再询问；回复「拒绝」放弃。")
	return b.String()
}

// decisionWords are the exact replies that resolve an approval. Matching is
// exact (after normalization) on purpose: a broad "contains 同意" would swallow
// an ordinary sentence like "我不同意这个方案" as an approval.
var (
	approveWords = map[string]struct{}{
		"同意": {}, "允许": {}, "可以": {}, "好": {}, "是": {}, "确认": {},
		"approve": {}, "ok": {}, "yes": {}, "y": {}, "1": {},
	}
	approveAlwaysWords = map[string]struct{}{
		"本会话允许": {}, "全部允许": {}, "总是允许": {}, "不再询问": {},
		"always": {}, "approve always": {},
	}
	denyWords = map[string]struct{}{
		"拒绝": {}, "不同意": {}, "不允许": {}, "否": {}, "不": {}, "取消": {}, "放弃": {},
		"deny": {}, "no": {}, "n": {}, "0": {}, "cancel": {},
	}
)

// parseDecision maps a chat reply to an approval outcome.
func parseDecision(text string) (im.Decision, bool) {
	key := normalizeReply(text)
	if key == "" {
		return im.Decision{}, false
	}
	if _, ok := approveAlwaysWords[key]; ok {
		return im.Decision{Approve: true, Always: true}, true
	}
	if _, ok := approveWords[key]; ok {
		return im.Decision{Approve: true}, true
	}
	if _, ok := denyWords[key]; ok {
		return im.Decision{}, true
	}
	return im.Decision{}, false
}

// normalizeReply lowercases and strips the punctuation and spaces a phone
// keyboard adds around a one-word answer.
func normalizeReply(text string) string {
	s := strings.ToLower(strings.TrimSpace(text))
	s = strings.Trim(s, "。．.!！?？,，~～、 \t\r\n")
	s = strings.Join(strings.Fields(s), " ")
	return s
}

// mediaLabel names an unsupported payload, so the reply can say what arrived
// rather than staying silent.
func mediaLabel(itemType MessageItemType) string {
	switch itemType {
	case ItemImage:
		return "图片"
	case ItemVoice:
		return "语音"
	case ItemFile:
		return "文件"
	case ItemVideo:
		return "视频"
	default:
		return ""
	}
}
