// Subagent live rows: a delegate's own messages and tool calls, projected onto
// the desktop protocol and attributed to the `Task` call that spawned it
// (D201, ADR 0062).
//
// The mapper is deliberately independent of eventMapper: a delegate's rows are
// transcript content nested under its Task card, not parent turn state, so they
// must not move the session's activity, pending-tool counter or assistant-id
// list. Turn and agent lifecycle events stay with the parent; only messages and
// tool calls cross the boundary.
package runtime

import (
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
)

type subagentEventMapper struct {
	session          *Session
	turnID           string
	parentToolCallID string
	agentName        string

	messageID        string
	messageCreatedAt string
	text             string
	thinking         string
	messageOpen      bool
	messageCompleted bool
}

func newSubagentEventMapper(session *Session, turnID, parentToolCallID, agentName string) *subagentEventMapper {
	return &subagentEventMapper{
		session:          session,
		turnID:           turnID,
		parentToolCallID: parentToolCallID,
		agentName:        agentName,
	}
}

func (m *subagentEventMapper) handle(event agentcore.AgentEvent) {
	switch typed := event.(type) {
	case agentcore.MessageStartEvent:
		m.onMessageStart(typed)
	case agentcore.MessageUpdateEvent:
		m.onMessageUpdate(typed)
	case agentcore.MessageEndEvent:
		m.onMessageEnd(typed)
	case agentcore.TurnEndEvent:
		m.onTurnEnd(typed)
	case agentcore.AgentEndEvent:
		m.finishAborted()
	case agentcore.ToolExecutionStartEvent:
		m.emit(protocol.ToolStartEvent{
			Type:       protocol.TypeToolStart,
			ToolCallID: typed.ToolCallID,
			ToolName:   typed.ToolName,
			Args:       rawOrNull(marshalRaw(typed.Args)),
		})
	case agentcore.ToolExecutionUpdateEvent:
		m.emit(protocol.ToolUpdateEvent{
			Type:          protocol.TypeToolUpdate,
			ToolCallID:    typed.ToolCallID,
			PartialResult: rawOrNull(marshalRaw(typed.PartialResult)),
		})
	case agentcore.ToolExecutionEndEvent:
		m.emit(protocol.ToolEndEvent{
			Type:       protocol.TypeToolEnd,
			ToolCallID: typed.ToolCallID,
			Result:     rawOrNull(marshalRaw(typed.Result)),
			IsError:    typed.IsError,
		})
	}
}

func (m *subagentEventMapper) onMessageStart(event agentcore.MessageStartEvent) {
	assistant, ok := event.Message.(agentcore.AssistantMessage)
	if !ok {
		return
	}
	m.messageID = newID()
	m.messageCreatedAt = time.Now().UTC().Format(time.RFC3339Nano)
	m.messageOpen = true
	m.messageCompleted = false
	m.text = assistantText(assistant)
	m.thinking = assistantThinking(assistant)
	m.emit(protocol.MessageStartEvent{
		Type:    protocol.TypeMessageStart,
		Message: m.uiMessage(assistant, protocol.StatusStreaming),
	})
}

func (m *subagentEventMapper) onMessageUpdate(event agentcore.MessageUpdateEvent) {
	assistant, ok := event.Message.(agentcore.AssistantMessage)
	if !ok {
		return
	}
	if !m.messageOpen {
		m.messageOpen = true
		m.ensureMessage()
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

func (m *subagentEventMapper) onMessageEnd(event agentcore.MessageEndEvent) {
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
	if status == protocol.StatusError {
		failure := classifyAssistantFailure(assistant).AppError()
		message.Error = &failure
	}
	if !wasOpen {
		m.emit(protocol.MessageStartEvent{
			Type:    protocol.TypeMessageStart,
			Message: m.uiMessage(assistant, protocol.StatusStreaming),
		})
	}
	m.emit(protocol.MessageEndEvent{Type: protocol.TypeMessageEnd, Message: message})
	m.messageOpen = false
	m.messageCompleted = true
}

// onTurnEnd closes a terminal error row that never reached MessageStart: pigo
// reports a setup failure as a terminal assistant message on TurnEnd only.
func (m *subagentEventMapper) onTurnEnd(event agentcore.TurnEndEvent) {
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
	m.messageCompleted = true
}

func (m *subagentEventMapper) finishAborted() {
	if !m.messageOpen {
		return
	}
	message := protocol.UiMessage{
		ID:               m.messageID,
		Role:             protocol.RoleAssistant,
		Content:          m.text,
		Thinking:         m.thinking,
		CreatedAt:        ensureCreatedAt(m.messageCreatedAt),
		Status:           protocol.StatusAborted,
		ModelID:          m.session.model.ID,
		ProviderID:       m.session.model.Provider,
		ParentToolCallID: m.parentToolCallID,
		AgentName:        m.agentName,
	}
	m.emit(protocol.MessageEndEvent{Type: protocol.TypeMessageEnd, Message: message})
	m.messageOpen = false
	m.messageCompleted = true
}

func (m *subagentEventMapper) ensureMessage() {
	if m.messageID == "" {
		m.messageID = newID()
	}
	if m.messageCreatedAt == "" {
		m.messageCreatedAt = time.Now().UTC().Format(time.RFC3339Nano)
	}
}

func (m *subagentEventMapper) uiMessage(assistant agentcore.AssistantMessage, status string) protocol.UiMessage {
	return protocol.UiMessage{
		ID:               m.messageID,
		Role:             protocol.RoleAssistant,
		Content:          m.text,
		Thinking:         m.thinking,
		CreatedAt:        ensureCreatedAt(m.messageCreatedAt),
		Status:           status,
		ModelID:          firstNonEmptyString(assistant.Model, m.session.model.ID),
		ProviderID:       firstNonEmptyString(assistant.Provider, m.session.model.Provider),
		ParentToolCallID: m.parentToolCallID,
		AgentName:        m.agentName,
	}
}

func (m *subagentEventMapper) emit(event any) {
	m.session.emitAttributedEvent(m.turnID, m.parentToolCallID, m.agentName, event)
}
