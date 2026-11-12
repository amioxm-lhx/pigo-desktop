package im

import (
	"fmt"
	"sort"
	"strings"
)

// Command is a parsed channel control command: a slash line the channel handles
// itself rather than forwarding to the model.
type Command struct {
	// Name is the lower-cased command name without the slash.
	Name string
	// Args are the whitespace-split arguments.
	Args []string
}

// CommandNames lists the commands the Router understands, for /help.
var CommandNames = []string{
	"new", "resume", "sessions", "stop", "status", "model", "help",
}

// ParseCommand reports whether text is a channel command and, if so, returns it.
//
// Mentions are stripped first because group chats deliver "@_user_1 /status" —
// the mention placeholder is part of the text body. A message that consists only
// of a mention is not a command.
func ParseCommand(text string) (Command, bool) {
	trimmed := stripLeadingMentions(strings.TrimSpace(text))
	if !strings.HasPrefix(trimmed, "/") {
		return Command{}, false
	}
	fields := strings.Fields(strings.TrimPrefix(trimmed, "/"))
	if len(fields) == 0 {
		return Command{}, false
	}
	name := strings.ToLower(fields[0])
	// "/remote-control" style names are never ours; reject anything that is not
	// a bare word so a stray path is not mistaken for a command.
	for _, r := range name {
		if r < 'a' || r > 'z' {
			if r < '0' || r > '9' {
				return Command{}, false
			}
		}
	}
	return Command{Name: name, Args: fields[1:]}, true
}

// stripLeadingMentions removes leading "@_user_1"-style placeholders. Feishu
// renders a mention as such a token in the message body, so "/cmd" arrives as
// "@_user_1 /cmd" in a group.
func stripLeadingMentions(s string) string {
	for {
		s = strings.TrimSpace(s)
		if !strings.HasPrefix(s, "@") {
			return s
		}
		i := strings.IndexAny(s, " \t\r\n")
		if i < 0 {
			// The message is nothing but a mention.
			return ""
		}
		s = s[i+1:]
	}
}

// ParseButtonAction decodes the payload of a card button press into a Command.
// Feishu delivers button payloads as an arbitrary JSON object; the adapter
// flattens the interesting fields into ActionRef, and this maps the action name
// onto the command vocabulary so approvals share one code path with text input.
func ParseButtonAction(a ActionRef) (Command, bool) {
	switch a.Name {
	case "approve":
		return Command{Name: "approve", Args: []string{a.Token}}, true
	case "approve_always":
		return Command{Name: "approve_always", Args: []string{a.Token}}, true
	case "deny":
		return Command{Name: "deny", Args: []string{a.Token}}, true
	}
	return Command{}, false
}

// HelpText renders the command list for /help.
//
// It is deliberately short: this is read on a phone, and the point is
// discoverability, not documentation. Anything longer belongs in the README.
func HelpText() string {
	lines := []string{
		"**pigo 命令**",
		"",
		"直接发文字就是提问，以下是控制命令：",
		"",
		"`/new` 开一个新会话（旧的仍可 /resume）",
		"`/resume <id>` 接回历史会话",
		"`/sessions` 列出最近会话",
		"`/stop` 中止正在跑的这轮",
		"`/status` 当前会话状态",
		"`/model <id>` 切换模型",
		"`/help` 显示这条",
		"",
		"_同一个聊天最多排队 3 条消息，超出会提示稍后再试。_",
	}
	return strings.Join(lines, "\n")
}

// renderSessions formats the session list for /sessions.
func renderSessions(rows []sessionRow, limit int) string {
	if len(rows) == 0 {
		return "还没有历史会话，直接发一条消息就会开始。"
	}
	if limit <= 0 || limit > len(rows) {
		limit = len(rows)
	}
	var b strings.Builder
	b.WriteString("**最近会话**\n\n")
	for _, r := range rows[:limit] {
		marker := ""
		if r.Current {
			marker = " ← 当前"
		}
		title := r.Title
		if title == "" {
			title = "(未命名)"
		}
		fmt.Fprintf(&b, "- `%s` %s · %s%s\n", r.ID, title, r.When, marker)
	}
	b.WriteString("\n用 `/resume <id>` 接回其中一个。")
	return b.String()
}

// sessionRow is one line of the /sessions output.
type sessionRow struct {
	ID      string
	Title   string
	When    string
	Current bool
}

// sortSessionRows orders rows newest first.
func sortSessionRows(rows []sessionRow) {
	sort.SliceStable(rows, func(i, j int) bool { return rows[i].When > rows[j].When })
}
