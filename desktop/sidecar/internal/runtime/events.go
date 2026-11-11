package runtime

import (
	"encoding/json"
	"strings"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
)

// eventMapper turns pigo loop events into the desktop's normalized agent
// events. One mapper serves one turn.
type eventMapper struct {
	session *Session
	turnID  string

	messageID        string
	messageCreatedAt string
	text             string
	thinking         string
	messageOpen      bool
	messageCompleted bool
}

func (m *eventMapper) handle(event agentcore.AgentEvent) {
	switch ev := event.(type) {
	case agentcore.AgentStartEvent:
		m.emit(protocol.AgentStartEvent{Type: protocol.TypeAgentStart})
	case agentcore.TurnStartEvent:
		m.session.setActivity("waiting-model", "")
		m.emit(protocol.TurnStartEvent{Type: protocol.TypeTurnStart})
	case agentcore.MessageStartEvent:
		m.onMessageStart(ev)
	case agentcore.MessageUpdateEvent:
		m.onMessageUpdate(ev)
	case agentcore.MessageEndEvent:
		m.onMessageEnd(ev)
	case agentcore.ToolExecutionStartEvent:
		m.onToolStart(ev)
	case agentcore.ToolExecutionUpdateEvent:
		m.onToolUpdate(ev)
	case agentcore.ToolExecutionEndEvent:
		m.onToolEnd(ev)
	case agentcore.TurnEndEvent:
		m.onTurnEnd(ev)
		m.emit(protocol.TurnEndEvent{
			Type:          protocol.TypeTurnEnd,
			SubagentUsage: m.session.takeSubagentUsage(),
		})
	case agentcore.AgentEndEvent:
		// A canceled run can still deliver its terminal events because the
		// stream's send and the context cancellation are both ready; close the
		// open streaming row before closing the run so the renderer never sees
		// agent_end ahead of the row it terminates.
		if m.messageOpen {
			m.finishAborted()
		}
		m.session.clearActivitySilently()
		m.emitAgentEnd()
	case agentcore.CompactionStartEvent:
		m.session.setActivity("compacting", ev.Reason)
		m.emit(protocol.CompactionStartEvent{Type: protocol.TypeCompactionStart, Reason: ev.Reason})
	case agentcore.CompactionEvent:
		m.onCompaction(ev)
	case agentcore.SubAgentProgressEvent, agentcore.TelemetryEvent:
		// The minimal sidecar does not surface delegation progress or telemetry
		// yet; they are additive events the renderer can ignore.
	default:
		logf(m.session.logger, "unmapped pigo agent event: %T", event)
	}
}

func (m *eventMapper) onMessageStart(event agentcore.MessageStartEvent) {
	assistant, ok := event.Message.(agentcore.AssistantMessage)
	if !ok {
		return
	}
	m.session.clearActivity()
	m.beginMessage()
	m.text = assistantText(assistant)
	m.thinking = assistantThinking(assistant)

	message := m.uiMessage(assistant, protocol.StatusStreaming)
	m.emit(protocol.MessageStartEvent{Type: protocol.TypeMessageStart, Message: message})
}

// beginMessage opens a fresh assistant row with a fresh id.
//
// OpenAI-compatible gateways never emit a start event: their stream begins with
// the first delta, so this mapper synthesizes the row. That synthesized row must
// not inherit the id of the fragment that closed a moment ago (a response before
// each tool call): the host appends by id, so a reused id makes every later
// snapshot look like a replay of the first one and it is dropped, while the
// renderer overwrites the older row in place instead of adding a fragment. The
// user then sees the answer stream in and lose it on the next reload.
func (m *eventMapper) beginMessage() {
	m.messageID = newID()
	m.messageCreatedAt = time.Now().UTC().Format(time.RFC3339Nano)
	m.messageOpen = true
	m.messageCompleted = false
	m.text = ""
	m.thinking = ""
}

// closeMessage seals the current row so the next fragment opens its own.
func (m *eventMapper) closeMessage() {
	m.messageID = ""
	m.messageCreatedAt = ""
	m.messageOpen = false
	m.messageCompleted = true
	m.text = ""
	m.thinking = ""
}

func (m *eventMapper) onMessageUpdate(event agentcore.MessageUpdateEvent) {
	assistant, ok := event.Message.(agentcore.AssistantMessage)
	if !ok {
		return
	}
	if !m.messageOpen {
		// A provider that streams without a start event still needs a row for
		// the renderer to attach deltas to — and it has to be a new row, not
		// the one that already closed (see beginMessage).
		m.beginMessage()
		m.emit(protocol.MessageStartEvent{
			Type:    protocol.TypeMessageStart,
			Message: m.uiMessage(assistant, protocol.StatusStreaming),
		})
	}
	text, thinking := assistantText(assistant), assistantThinking(assistant)
	deltaText, resetText := delta(m.text, text)
	deltaThinking, resetThinking := delta(m.thinking, thinking)
	m.text, m.thinking = text, thinking

	if deltaText == "" && deltaThinking == "" && !resetText && !resetThinking {
		return
	}
	update := protocol.MessageUpdateEvent{
		Type:    protocol.TypeMessageUpdate,
		Message: streamingIdentity(m.uiMessage(assistant, protocol.StatusStreaming)),
		Stream:  "delta",
	}
	if deltaText != "" || resetText {
		update.DeltaText = stringPtr(deltaText)
	}
	if deltaThinking != "" || resetThinking {
		update.DeltaThinking = stringPtr(deltaThinking)
	}
	if resetText {
		update.ResetText = boolPtr(true)
	}
	if resetThinking {
		update.ResetThinking = boolPtr(true)
	}
	m.emit(update)
}

func (m *eventMapper) onMessageEnd(event agentcore.MessageEndEvent) {
	assistant, ok := event.Message.(agentcore.AssistantMessage)
	if !ok {
		return
	}
	wasOpen := m.messageOpen
	m.ensureMessage()
	m.text = assistantText(assistant)
	m.thinking = assistantThinking(assistant)

	status := protocol.StatusComplete
	switch assistant.StopReason {
	case agentcore.StopReasonError:
		status = protocol.StatusError
	case agentcore.StopReasonAborted:
		status = protocol.StatusAborted
	}
	message := m.uiMessage(assistant, status)
	if assistant.Usage != nil {
		message.Usage = &protocol.MessageUsage{
			InputTokens:  assistant.Usage.InputTokens,
			OutputTokens: assistant.Usage.OutputTokens,
			TotalTokens:  assistant.Usage.InputTokens + assistant.Usage.OutputTokens,
		}
	}
	var appErr *protocol.AppError
	if status == protocol.StatusError {
		failure := classifyAssistantFailure(assistant).AppError()
		appErr = &failure
		message.Error = appErr
	}
	if !wasOpen {
		// A provider failure can reach the loop before any start event; the
		// renderer still needs a row to close.
		m.emit(protocol.MessageStartEvent{
			Type:    protocol.TypeMessageStart,
			Message: m.uiMessage(assistant, protocol.StatusStreaming),
		})
	}
	m.emit(protocol.MessageEndEvent{Type: protocol.TypeMessageEnd, Message: message})
	m.session.recordAssistantID(m.messageID)
	m.closeMessage()

	if appErr != nil {
		m.emitError(*appErr)
	}
}

// onTurnEnd closes a terminal error row that never reached MessageStart: pigo
// reports a setup failure as a terminal assistant message on TurnEnd only, so
// the renderer would otherwise see a turn end with no error.
func (m *eventMapper) onTurnEnd(event agentcore.TurnEndEvent) {
	if m.messageOpen || m.messageCompleted {
		return
	}
	assistant := event.Message
	if assistant.StopReason != agentcore.StopReasonError {
		return
	}
	m.ensureMessage()
	m.text = assistantText(assistant)
	m.thinking = assistantThinking(assistant)
	failure := classifyAssistantFailure(assistant).AppError()
	message := m.uiMessage(assistant, protocol.StatusError)
	message.Error = &failure
	m.emit(protocol.MessageStartEvent{
		Type:    protocol.TypeMessageStart,
		Message: m.uiMessage(assistant, protocol.StatusStreaming),
	})
	m.emit(protocol.MessageEndEvent{Type: protocol.TypeMessageEnd, Message: message})
	m.session.recordAssistantID(m.messageID)
	m.closeMessage()
	m.emitError(failure)
}

// finishAborted closes the streaming row an aborted turn left open.
func (m *eventMapper) finishAborted() {
	if !m.messageOpen {
		return
	}
	message := protocol.UiMessage{
		ID:         m.messageID,
		Role:       protocol.RoleAssistant,
		Content:    m.text,
		Thinking:   m.thinking,
		CreatedAt:  ensureCreatedAt(m.messageCreatedAt),
		Status:     protocol.StatusAborted,
		ModelID:    m.session.model.ID,
		ProviderID: m.session.model.Provider,
	}
	m.emit(protocol.MessageEndEvent{Type: protocol.TypeMessageEnd, Message: message})
	m.session.recordAssistantID(m.messageID)
	m.closeMessage()
}

func (m *eventMapper) ensureMessage() {
	if m.messageID == "" {
		m.messageID = newID()
	}
	if m.messageCreatedAt == "" {
		m.messageCreatedAt = time.Now().UTC().Format(time.RFC3339Nano)
	}
}

func (m *eventMapper) onToolStart(event agentcore.ToolExecutionStartEvent) {
	m.session.clearActivity()
	m.session.addPendingTool()
	m.emit(protocol.ToolStartEvent{
		Type:       protocol.TypeToolStart,
		ToolCallID: event.ToolCallID,
		ToolName:   event.ToolName,
		Args:       rawOrNull(marshalRaw(event.Args)),
	})
}

func (m *eventMapper) onToolUpdate(event agentcore.ToolExecutionUpdateEvent) {
	m.emit(protocol.ToolUpdateEvent{
		Type:          protocol.TypeToolUpdate,
		ToolCallID:    event.ToolCallID,
		PartialResult: rawOrNull(marshalRaw(event.PartialResult)),
	})
}

func (m *eventMapper) onToolEnd(event agentcore.ToolExecutionEndEvent) {
	m.session.removePendingTool()
	m.emit(protocol.ToolEndEvent{
		Type:       protocol.TypeToolEnd,
		ToolCallID: event.ToolCallID,
		Result:     rawOrNull(marshalRaw(event.Result)),
		IsError:    event.IsError,
	})
	if event.ToolName == TaskToolName {
		// A delegate that settled before this event is published now, so the
		// terminal card cannot be overwritten by the starting snapshot.
		m.session.markTaskToolEnded(event.ToolCallID)
	}
	m.session.setActivity("waiting-model", "")
}

func (m *eventMapper) onCompaction(event agentcore.CompactionEvent) {
	before, after := event.TokensBefore, event.TokensAfter
	ok := event.ErrorMessage == ""
	var mark *protocol.ContextCompactionMark
	var firstKept string
	var failure *protocol.CompactionError
	if ok {
		// OnCompaction persisted the checkpoint on the loop goroutine before this
		// event reached the stream; the outcome carries the mark the renderer
		// draws. A persistence failure downgrades the event because the context
		// compaction is not durable across a runtime rebuild.
		outcome := m.session.takeCompactionOutcome()
		switch {
		case outcome.Err != nil:
			ok = false
			failure = &protocol.CompactionError{
				Code:    "CONTEXT_COMPACTION_FAILED",
				Message: outcome.Err.Error(),
			}
		default:
			mark = outcome.Mark
			firstKept = outcome.FirstKeptMessageID
		}
	} else {
		failure = &protocol.CompactionError{
			Code:    "CONTEXT_COMPACTION_FAILED",
			Message: event.ErrorMessage,
		}
	}
	m.emit(protocol.CompactionEndEvent{
		Type:               protocol.TypeCompactionEnd,
		Reason:             event.Reason,
		OK:                 ok,
		TokensBefore:       &before,
		TokensAfter:        &after,
		FirstKeptMessageID: firstKept,
		WillRetry:          false,
		Mark:               mark,
		Error:              failure,
		ErrorMessage:       event.ErrorMessage,
	})
	m.session.setActivity("waiting-model", "")
}

func (m *eventMapper) emitAgentEnd() {
	ids := m.session.assistantMessageIDs()
	if ids == nil {
		ids = []string{}
	}
	m.emit(protocol.AgentEndEvent{Type: protocol.TypeAgentEnd, MessageIDs: ids})
	m.session.markAgentEnded()
}

func (m *eventMapper) emitError(appErr protocol.AppError) {
	m.emit(protocol.ErrorEvent{
		Type:  protocol.TypeError,
		Error: appErr,
	})
}

func (m *eventMapper) emit(event any) {
	if m.session.emitter == nil {
		return
	}
	envelope := protocol.EventEnvelope{
		SessionID: m.session.sessionID,
		TurnID:    m.turnID,
		TS:        time.Now().UnixMilli(),
		Event:     event,
	}
	if err := m.session.emitter.Emit(envelope); err != nil {
		logf(m.session.logger, "emit agent event: %v", err)
	}
}

func (m *eventMapper) uiMessage(assistant agentcore.AssistantMessage, status string) protocol.UiMessage {
	return protocol.UiMessage{
		ID:         m.messageID,
		Role:       protocol.RoleAssistant,
		Content:    m.text,
		Thinking:   m.thinking,
		CreatedAt:  ensureCreatedAt(m.messageCreatedAt),
		Status:     status,
		ModelID:    firstNonEmptyString(assistant.Model, m.session.model.ID),
		ProviderID: firstNonEmptyString(assistant.Provider, m.session.model.Provider),
	}
}

// streamingIdentity strips the growing content/thinking from a delta frame.
// The renderer applies deltaText/deltaThinking onto the live row, so shipping
// the full snapshot on every frame only wastes bandwidth.
func streamingIdentity(message protocol.UiMessage) protocol.UiMessage {
	return protocol.UiMessage{
		ID:               message.ID,
		Role:             message.Role,
		CreatedAt:        message.CreatedAt,
		Status:           message.Status,
		ModelID:          message.ModelID,
		ProviderID:       message.ProviderID,
		ParentToolCallID: message.ParentToolCallID,
		AgentName:        message.AgentName,
	}
}

func assistantText(message agentcore.AssistantMessage) string {
	return agentcore.ContentToText(message.Content)
}

func assistantThinking(message agentcore.AssistantMessage) string {
	var b strings.Builder
	for _, block := range message.Content {
		if thinking, ok := block.(agentcore.ThinkingContent); ok {
			b.WriteString(thinking.Thinking)
		}
	}
	return b.String()
}

func delta(previous, current string) (string, bool) {
	if previous != "" && strings.HasPrefix(current, previous) {
		return current[len(previous):], false
	}
	if previous == "" {
		return current, false
	}
	return current, true
}

func stringPtr(value string) *string { return &value }

func boolPtr(value bool) *bool { return &value }

func ensureCreatedAt(value string) string {
	if value == "" {
		return time.Now().UTC().Format(time.RFC3339Nano)
	}
	return value
}

func rawOrNull(raw json.RawMessage) json.RawMessage {
	if len(raw) == 0 {
		return json.RawMessage("null")
	}
	return raw
}

func firstNonEmptyString(values ...string) string {
	for _, value := range values {
		if strings.TrimSpace(value) != "" {
			return value
		}
	}
	return ""
}
