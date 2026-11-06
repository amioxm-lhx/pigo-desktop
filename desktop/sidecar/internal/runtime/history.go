package runtime

import (
	"context"
	"encoding/json"
	"strconv"
	"strings"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
)

// historyRefreshTimeout bounds the pre-prompt transcript check so a slow host
// cannot stall prompt admission indefinitely.
const historyRefreshTimeout = 30 * time.Second

// sessionDetail is the part of the host's session.get reply the sidecar needs.
type sessionDetail struct {
	Session *sessionView `json:"session"`
}

type sessionView struct {
	Messages     []historyMessage   `json:"messages"`
	Compaction   *historyCompaction `json:"compaction,omitempty"`
	MessageCount int                `json:"messageCount"`
	UpdatedAt    string             `json:"updatedAt,omitempty"`
}

// historyCompaction is the active checkpoint host-core reports. It stands in
// for every transcript row through ThroughMessageID.
type historyCompaction struct {
	Summary            string            `json:"summary"`
	FirstKeptMessageID string            `json:"firstKeptMessageId,omitempty"`
	ThroughMessageID   string            `json:"throughMessageId"`
	TokensBefore       int               `json:"tokensBefore"`
	RetainedTail       []json.RawMessage `json:"retainedTail"`
	Details            map[string]any    `json:"details,omitempty"`
	CreatedAt          string            `json:"createdAt"`
}

// historyRequest captures the inputs one history restore needs.
type historyRequest struct {
	SessionID            string
	CurrentUserMessageID string
	CurrentContent       string
	TruncateFromID       string
	TruncateBefore       *int
	Model                provider.Model
}

// historyMessage is one stored transcript row. It is the read side of the
// UiMessage the sidecar streams.
type historyMessage struct {
	ID               string          `json:"id"`
	Role             string          `json:"role"`
	Content          string          `json:"content"`
	Thinking         string          `json:"thinking,omitempty"`
	CreatedAt        string          `json:"createdAt"`
	Status           string          `json:"status,omitempty"`
	ModelID          string          `json:"modelId,omitempty"`
	ProviderID       string          `json:"providerId,omitempty"`
	IsError          bool            `json:"isError,omitempty"`
	ParentToolCallID string          `json:"parentToolCallId,omitempty"`
	ToolName         string          `json:"toolName,omitempty"`
	ToolCallID       string          `json:"toolCallId,omitempty"`
	ToolArgs         json.RawMessage `json:"toolArgs,omitempty"`
	ToolStatus       string          `json:"toolStatus,omitempty"`
	Usage            *historyUsage   `json:"usage,omitempty"`
}

type historyUsage struct {
	InputTokens  int `json:"inputTokens"`
	OutputTokens int `json:"outputTokens"`
}

// historySnapshot is the model context rebuilt from host-core plus the
// host-authoritative revision it came from. Generation is the active
// checkpoint's generation (0 when the transcript has no checkpoint), so a
// compaction after a restart stamps the next one.
type historySnapshot struct {
	Messages   agentcore.MessageList
	Revision   string
	Generation int
}

// loadHistory rebuilds the model context from host-core's transcript. The load
// is best-effort: a host that cannot answer still lets the prompt run with an
// empty context, matching the desktop sidecar's behavior.
func loadHistory(ctx context.Context, host Host, request historyRequest) (historySnapshot, error) {
	if host == nil || strings.TrimSpace(request.SessionID) == "" {
		return historySnapshot{}, nil
	}
	detail, err := fetchSessionDetail(ctx, host, request.SessionID)
	if err != nil {
		return historySnapshot{}, err
	}
	return snapshotFromDetail(detail, request), nil
}

func fetchSessionDetail(ctx context.Context, host Host, sessionID string) (*sessionDetail, error) {
	var detail sessionDetail
	if err := host.CallInto(ctx, "session.get", map[string]any{"id": sessionID}, &detail); err != nil {
		return nil, err
	}
	return &detail, nil
}

func snapshotFromDetail(detail *sessionDetail, request historyRequest) historySnapshot {
	if detail == nil || detail.Session == nil {
		return historySnapshot{}
	}
	rows := append([]historyMessage(nil), detail.Session.Messages...)
	rows = truncateHistory(rows, request.TruncateFromID, request.TruncateBefore)
	// Electron main persists the current user row before it calls agent.prompt;
	// that row must not be sent to the model twice.
	if last := len(rows) - 1; last >= 0 && rows[last].Role == "user" {
		matches := request.CurrentUserMessageID != "" && rows[last].ID == request.CurrentUserMessageID
		if !matches && request.CurrentUserMessageID == "" && rows[last].Content == request.CurrentContent {
			matches = true
		}
		if matches {
			rows = rows[:last]
		}
	}

	revision := sessionRevision(detail.Session)
	if checkpoint := detail.Session.Compaction; checkpoint != nil {
		if boundary := indexOfRow(rows, checkpoint.ThroughMessageID); boundary >= 0 {
			messages := agentcore.MessageList{agentcore.CompactionMessage{
				RoleField:    agentcore.RoleCompaction,
				Summary:      checkpoint.Summary,
				TokensBefore: checkpoint.TokensBefore,
				Timestamp:    parseHistoryTimestamp(checkpoint.CreatedAt),
			}}
			messages = append(messages, retainedTailMessages(checkpoint)...)
			messages = append(messages, historyToMessages(rows[boundary+1:], request.Model)...)
			return historySnapshot{
				Messages:   messages,
				Revision:   revision,
				Generation: checkpointGeneration(checkpoint.Details),
			}
		}
	}
	return historySnapshot{Messages: historyToMessages(rows, request.Model), Revision: revision}
}

// checkpointGeneration reads the generation a checkpoint was stamped with. A
// record without the marker predates generation tracking and counts as the
// first checkpoint.
func checkpointGeneration(details map[string]any) int {
	switch value := details["generation"].(type) {
	case int:
		if value >= 1 {
			return value
		}
	case float64:
		if value >= 1 {
			return int(value)
		}
	case json.Number:
		if parsed, err := value.Int64(); err == nil && parsed >= 1 {
			return int(parsed)
		}
	}
	return 0
}

func sessionRevision(view *sessionView) string {
	if view == nil {
		return ""
	}
	if strings.TrimSpace(view.UpdatedAt) == "" && view.MessageCount == 0 {
		return ""
	}
	return strings.TrimSpace(view.UpdatedAt) + "|" + strconv.Itoa(view.MessageCount)
}

// RefreshIfChanged reloads a live runtime's model context when host-core's
// transcript no longer matches it. It is called before a prompt is admitted,
// so a regenerate, fork, or external transcript rewrite cannot keep feeding
// the model a stale history. A host that is merely behind the live context
// (the current turn has not been persisted yet) is treated as a lagging
// writer, not as a rewrite: the live context is kept and only the revision
// marker advances.
func (s *Session) RefreshIfChanged(ctx context.Context, params protocol.PromptParams) error {
	if s.host == nil {
		return nil
	}
	s.refreshMu.Lock()
	defer s.refreshMu.Unlock()

	s.mu.Lock()
	if s.disposed || s.running || s.compacting {
		s.mu.Unlock()
		return nil
	}
	previousRevision := s.historyRevision
	live := append(agentcore.MessageList(nil), s.agentCtx.Messages...)
	s.mu.Unlock()

	refreshCtx, cancel := context.WithTimeout(ctx, historyRefreshTimeout)
	defer cancel()
	detail, err := fetchSessionDetail(refreshCtx, s.host, s.sessionID)
	if err != nil {
		return err
	}
	if detail.Session == nil {
		return nil
	}
	revision := sessionRevision(detail.Session)
	if previousRevision != "" && revision != "" && previousRevision == revision {
		return nil
	}

	snapshot := snapshotFromDetail(detail, historyRequest{
		SessionID:            s.sessionID,
		CurrentUserMessageID: params.UserMessageID,
		CurrentContent:       params.Content,
		Model:                s.model,
	})

	s.mu.Lock()
	if s.running || s.disposed {
		s.mu.Unlock()
		return nil
	}
	if sameMessagesPrefix(snapshot.Messages, live) {
		// The host knows every message the live context knows (or fewer);
		// keep the live context so an unpersisted final turn is not lost.
		s.historyRevision = revision
		s.compactionGeneration = snapshot.Generation
		s.mu.Unlock()
		return nil
	}
	s.agentCtx.Messages = snapshot.Messages
	s.historyRevision = revision
	s.compactionGeneration = snapshot.Generation
	s.mu.Unlock()
	logf(s.logger, "session %s: history refreshed from host-core", s.sessionID)
	return nil
}

// sameMessagesPrefix reports whether every message host knows is also the
// corresponding live message. A host that is behind (a prefix) or equal is
// fine; a divergence means the transcript was edited or replaced.
func sameMessagesPrefix(hostMessages, liveMessages agentcore.MessageList) bool {
	if len(hostMessages) > len(liveMessages) {
		return false
	}
	for i := range hostMessages {
		if messageSignature(hostMessages[i]) != messageSignature(liveMessages[i]) {
			return false
		}
	}
	return true
}

func messageSignature(message agentcore.Message) string {
	switch value := message.(type) {
	case agentcore.UserMessage:
		// Host rows carry user text but never image bytes, so a live image
		// message must still compare equal to the row it was persisted from.
		// Without this, every refresh would replace the live context with the
		// text-only history and silently drop the image from later turns.
		return "user|" + userTextSignature(value.Content)
	case agentcore.AssistantMessage:
		return "assistant|" + contentSignature(value.Content)
	case agentcore.ToolResultMessage:
		return "tool|" + value.ToolCallID + "|" + value.ToolName + "|" + contentSignature(value.Content)
	case agentcore.CompactionMessage:
		return "compaction|" + value.Summary + "|" + strconv.Itoa(value.TokensBefore)
	default:
		return "unknown"
	}
}

func userTextSignature(content agentcore.ContentList) string {
	var b strings.Builder
	for _, block := range content {
		if text, ok := block.(agentcore.TextContent); ok {
			b.WriteString("t:")
			b.WriteString(text.Text)
			b.WriteString("\x1f")
		}
	}
	return b.String()
}

func contentSignature(content agentcore.ContentList) string {
	var b strings.Builder
	for _, block := range content {
		switch value := block.(type) {
		case agentcore.TextContent:
			b.WriteString("t:")
			b.WriteString(value.Text)
			b.WriteString("\x1f")
		case agentcore.ThinkingContent:
			b.WriteString("k:")
			b.WriteString(value.Thinking)
			b.WriteString("\x1f")
		case agentcore.ToolCallContent:
			b.WriteString("c:")
			b.WriteString(value.ID)
			b.WriteString("|")
			b.WriteString(value.Name)
			b.WriteString("|")
			b.WriteString(canonicalJSON(value.Arguments))
			b.WriteString("\x1f")
		case agentcore.ImageContent:
			b.WriteString("i:")
			b.WriteString(value.MimeType)
			b.WriteString("|")
			b.WriteString(strconv.Itoa(len(value.Data)))
			b.WriteString("\x1f")
		default:
			b.WriteString("?:")
			b.WriteString("\x1f")
		}
	}
	return b.String()
}

// canonicalJSON normalizes tool arguments so provider JSON and stored JSON
// compare equal when their key order or whitespace differs.
func canonicalJSON(raw json.RawMessage) string {
	if len(raw) == 0 {
		return ""
	}
	var value any
	if err := json.Unmarshal(raw, &value); err != nil {
		return string(raw)
	}
	encoded, err := json.Marshal(value)
	if err != nil {
		return string(raw)
	}
	return string(encoded)
}

// truncateHistory applies the regenerated-turn boundary. When Electron main
// already forwarded truncateFromMessageId to host-owned session.truncateFrom,
// the id is no longer present and the host transcript is already correct; the
// lookup then leaves the rows untouched rather than failing a valid prompt.
func truncateHistory(rows []historyMessage, fromID string, before *int) []historyMessage {
	if id := strings.TrimSpace(fromID); id != "" {
		if index := indexOfRow(rows, id); index >= 0 {
			return rows[:index]
		}
		return rows
	}
	if before != nil {
		limit := *before
		if limit < 0 {
			limit = 0
		}
		if limit < len(rows) {
			return rows[:limit]
		}
	}
	return rows
}

// retainedTailMessages projects a checkpoint's retained tail the way the
// desktop runtime does: only the latest retained user message can survive, and
// a completed-turn checkpoint retains none of them.
func retainedTailMessages(checkpoint *historyCompaction) agentcore.MessageList {
	var latest *agentcore.UserMessage
	for _, raw := range checkpoint.RetainedTail {
		message, ok := retainedUserMessage(raw)
		if !ok {
			continue
		}
		latest = &message
	}
	if latest == nil || compactionRetentionMode(checkpoint.Details) == "completed_turn" {
		return nil
	}
	return agentcore.MessageList{*latest}
}

func compactionRetentionMode(details map[string]any) string {
	if mode, ok := details["retainedTailMode"].(string); ok && mode == "completed_turn" {
		return "completed_turn"
	}
	return "active_turn"
}

func retainedUserMessage(raw json.RawMessage) (agentcore.UserMessage, bool) {
	var probe struct {
		Role      string          `json:"role"`
		Content   json.RawMessage `json:"content"`
		Timestamp int64           `json:"timestamp"`
	}
	if err := json.Unmarshal(raw, &probe); err != nil || probe.Role != "user" {
		return agentcore.UserMessage{}, false
	}

	content := retainedContent(probe.Content)
	if len(content) == 0 {
		return agentcore.UserMessage{}, false
	}
	return agentcore.UserMessage{
		RoleField: agentcore.RoleUser,
		Content:   content,
		Timestamp: probe.Timestamp,
	}, true
}

func retainedContent(raw json.RawMessage) agentcore.ContentList {
	trimmed := strings.TrimSpace(string(raw))
	if trimmed == "" || trimmed == "null" {
		return nil
	}
	if strings.HasPrefix(trimmed, "\"") {
		var text string
		if err := json.Unmarshal(raw, &text); err != nil {
			return nil
		}
		return agentcore.ContentList{agentcore.NewTextContent(text)}
	}
	if strings.HasPrefix(trimmed, "[") {
		var content agentcore.ContentList
		if err := json.Unmarshal(raw, &content); err != nil {
			return nil
		}
		return content
	}
	return nil
}

func indexOfRow(rows []historyMessage, id string) int {
	if strings.TrimSpace(id) == "" {
		return -1
	}
	for i, row := range rows {
		if row.ID == id {
			return i
		}
	}
	return -1
}

// historyToMessages converts transcript rows into pigo messages. It mirrors the
// desktop runtime: tool rows attach their calls to the assistant row above
// them, failed assistant rows stay out of the model context, and subagent rows
// never enter the parent's context.
func historyToMessages(rows []historyMessage, model provider.Model) agentcore.MessageList {
	messages, _ := historyToMessagesWithIDs(rows, model)
	return messages
}

// historyToMessagesWithIDs is historyToMessages plus the host row id each
// message came from, aligned index-for-index. A message without a durable row
// (a synthesized tool carrier) gets an empty id. Compaction uses this to anchor
// its checkpoint boundary on a real transcript row.
func historyToMessagesWithIDs(rows []historyMessage, model provider.Model) (agentcore.MessageList, []string) {
	messages := make(agentcore.MessageList, 0, len(rows))
	ids := make([]string, 0, len(rows))
	carrierIndex := -1

	for _, row := range rows {
		if row.ParentToolCallID != "" {
			continue
		}
		timestamp := parseHistoryTimestamp(row.CreatedAt)

		switch row.Role {
		case "user":
			carrierIndex = -1
			if strings.TrimSpace(row.Content) == "" {
				continue
			}
			messages = append(messages, agentcore.UserMessage{
				RoleField: agentcore.RoleUser,
				Content:   agentcore.ContentList{agentcore.NewTextContent(row.Content)},
				Timestamp: timestamp,
			})
			ids = append(ids, row.ID)
		case "assistant":
			carrierIndex = -1
			if row.Status == "error" || row.IsError {
				continue
			}
			content := make(agentcore.ContentList, 0, 2)
			if strings.TrimSpace(row.Thinking) != "" {
				content = append(content, agentcore.NewThinkingContent(row.Thinking))
			}
			if strings.TrimSpace(row.Content) != "" {
				content = append(content, agentcore.NewTextContent(row.Content))
			}
			assistant := agentcore.AssistantMessage{
				RoleField:  agentcore.RoleAssistant,
				Content:    content,
				Provider:   firstNonEmpty(row.ProviderID, model.Provider),
				Model:      firstNonEmpty(row.ModelID, model.ID),
				StopReason: agentcore.StopReasonEndTurn,
				Timestamp:  timestamp,
			}
			if row.Usage != nil {
				assistant.Usage = &agentcore.Usage{
					InputTokens:  row.Usage.InputTokens,
					OutputTokens: row.Usage.OutputTokens,
				}
			}
			messages = append(messages, assistant)
			ids = append(ids, row.ID)
			carrierIndex = len(messages) - 1
		case "tool":
			if row.ToolCallID == "" || row.ToolName == "" {
				continue
			}
			if carrierIndex < 0 {
				// A truncated branch can leave a tool row without its assistant
				// row. Synthesize the carrier so the call/result pair stays
				// well-formed for the provider.
				messages = append(messages, agentcore.AssistantMessage{
					RoleField:  agentcore.RoleAssistant,
					Provider:   model.Provider,
					Model:      model.ID,
					StopReason: agentcore.StopReasonToolUse,
					Timestamp:  timestamp,
				})
				ids = append(ids, "")
				carrierIndex = len(messages) - 1
			}
			carrier, ok := messages[carrierIndex].(agentcore.AssistantMessage)
			if !ok {
				continue
			}
			carrier.Content = append(carrier.Content, agentcore.NewToolCallContent(row.ToolCallID, row.ToolName, historyArguments(row.ToolArgs)))
			carrier.StopReason = agentcore.StopReasonToolUse
			messages[carrierIndex] = carrier
			messages = append(messages, agentcore.ToolResultMessage{
				RoleField:  agentcore.RoleToolResult,
				ToolCallID: row.ToolCallID,
				ToolName:   row.ToolName,
				Content:    agentcore.ContentList{agentcore.NewTextContent(row.Content)},
				Timestamp:  timestamp,
			})
			ids = append(ids, row.ID)
		}
	}

	filtered := make(agentcore.MessageList, 0, len(messages))
	filteredIDs := make([]string, 0, len(ids))
	for index, message := range messages {
		if assistant, ok := message.(agentcore.AssistantMessage); ok && len(assistant.Content) == 0 {
			continue
		}
		filtered = append(filtered, message)
		filteredIDs = append(filteredIDs, ids[index])
	}
	return filtered, filteredIDs
}

func historyArguments(raw json.RawMessage) json.RawMessage {
	if len(raw) == 0 {
		return json.RawMessage("{}")
	}
	var object map[string]any
	if err := json.Unmarshal(raw, &object); err != nil || object == nil {
		return json.RawMessage("{}")
	}
	return raw
}

func parseHistoryTimestamp(value string) int64 {
	if strings.TrimSpace(value) == "" {
		return time.Now().UnixMilli()
	}
	parsed, err := time.Parse(time.RFC3339Nano, value)
	if err != nil {
		return time.Now().UnixMilli()
	}
	return parsed.UnixMilli()
}
