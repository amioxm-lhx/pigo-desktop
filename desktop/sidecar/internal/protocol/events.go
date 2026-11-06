package protocol

import "encoding/json"

// EventEnvelope is the payload of every agent.event notification.
type EventEnvelope struct {
	SessionID string `json:"sessionId"`
	TurnID    string `json:"turnId,omitempty"`
	TS        int64  `json:"ts"`
	Event     any    `json:"event"`
	// ParentToolCallID and AgentName are set on events emitted inside a
	// subagent (D201, ADR 0062): the `Task` call that spawned it and the
	// definition name. The shell uses them to attribute transcript rows and to
	// keep subagent-internal events out of the parent's turn lifecycle.
	ParentToolCallID string `json:"parentToolCallId,omitempty"`
	AgentName        string `json:"agentName,omitempty"`
}

// Event payload shapes. Each concrete event carries its own "type"
// discriminator, so the envelope marshals to the shape the renderer already
// consumes.

const (
	TypeAgentStart     = "agent_start"
	TypeAgentEnd       = "agent_end"
	TypeTurnStart      = "turn_start"
	TypeTurnEnd        = "turn_end"
	TypeMessageStart   = "message_start"
	TypeMessageUpdate  = "message_update"
	TypeMessageEnd     = "message_end"
	TypeToolStart      = "tool_start"
	TypeToolUpdate     = "tool_update"
	TypeToolEnd        = "tool_end"
	TypeError          = "error"
	TypeAskToolRequest = "asktool_request"
	TypePlanningState  = "planning_state"
	TypeStatus         = "status"

	TypeCompactionStart = "compaction_start"
	TypeCompactionEnd   = "compaction_end"
)

// AgentStartEvent opens one agent run.
type AgentStartEvent struct {
	Type string `json:"type"`
}

// AgentEndEvent closes one agent run and lists the assistant rows it produced.
type AgentEndEvent struct {
	Type       string   `json:"type"`
	MessageIDs []string `json:"messageIds"`
}

// TurnStartEvent opens one model turn.
type TurnStartEvent struct {
	Type string `json:"type"`
}

// TurnEndEvent closes one model turn.
type TurnEndEvent struct {
	Type          string        `json:"type"`
	SubagentUsage *MessageUsage `json:"subagentUsage,omitempty"`
}

// MessageStartEvent starts one streamed message.
type MessageStartEvent struct {
	Type    string    `json:"type"`
	Message UiMessage `json:"message"`
}

// MessageUpdateEvent carries either an append-only delta frame or a snapshot
// replacement.
type MessageUpdateEvent struct {
	Type          string    `json:"type"`
	Message       UiMessage `json:"message"`
	DeltaText     *string   `json:"deltaText,omitempty"`
	DeltaThinking *string   `json:"deltaThinking,omitempty"`
	Stream        string    `json:"stream,omitempty"`
	ResetText     *bool     `json:"resetText,omitempty"`
	ResetThinking *bool     `json:"resetThinking,omitempty"`
}

// MessageEndEvent closes one message.
type MessageEndEvent struct {
	Type               string     `json:"type"`
	Message            UiMessage  `json:"message"`
	PrecedingAssistant *UiMessage `json:"precedingAssistant,omitempty"`
}

// ToolStartEvent announces one tool call the model requested.
type ToolStartEvent struct {
	Type       string          `json:"type"`
	ToolCallID string          `json:"toolCallId"`
	ToolName   string          `json:"toolName"`
	Args       json.RawMessage `json:"args"`
}

// ToolUpdateEvent streams a partial tool result.
type ToolUpdateEvent struct {
	Type          string          `json:"type"`
	ToolCallID    string          `json:"toolCallId"`
	PartialResult json.RawMessage `json:"partialResult,omitempty"`
}

// ToolEndEvent closes one tool call with its result.
type ToolEndEvent struct {
	Type       string          `json:"type"`
	ToolCallID string          `json:"toolCallId"`
	Result     json.RawMessage `json:"result"`
	IsError    bool            `json:"isError,omitempty"`
}

// ErrorEvent reports a failure the renderer shows on the session.
type ErrorEvent struct {
	Type  string   `json:"type"`
	Error AppError `json:"error"`
}

// StatusEvent publishes the live session status, including activity.
type StatusEvent struct {
	Type   string      `json:"type"`
	Status AgentStatus `json:"status"`
}

// AskToolRequestEvent asks the renderer to show one inline question card.
type AskToolRequestEvent struct {
	Type    string         `json:"type"`
	Request AskToolRequest `json:"request"`
}

// PlanningStateEvent reports the session's plan/goal contract state. Absent
// fields are omitted; the renderer merges them into the live checkpoint.
type PlanningStateEvent struct {
	Type                 string         `json:"type"`
	State                string         `json:"state"`
	Kind                 string         `json:"kind,omitempty"`
	ProposalID           string         `json:"proposalId,omitempty"`
	Title                string         `json:"title,omitempty"`
	Markdown             string         `json:"markdown,omitempty"`
	Question             string         `json:"question,omitempty"`
	Artifact             *PlanArtifact  `json:"artifact,omitempty"`
	Version              int64          `json:"version,omitempty"`
	Plan                 string         `json:"plan,omitempty"`
	Action               string         `json:"action,omitempty"`
	TargetPermissionMode string         `json:"targetPermissionMode,omitempty"`
	ExecutionID          string         `json:"executionId,omitempty"`
	ExecutionState       string         `json:"executionState,omitempty"`
	Proposal             *PlanProposal  `json:"proposal,omitempty"`
	Execution            *PlanExecution `json:"execution,omitempty"`
}

// CompactionStartEvent opens one context compaction.
type CompactionStartEvent struct {
	Type   string `json:"type"`
	Reason string `json:"reason"`
}

// CompactionEndEvent closes one context compaction. WillRetry is required by
// the renderer, so it is always emitted. A checkpoint that reached host-core
// carries its mark; a failure carries the named error instead.
type CompactionEndEvent struct {
	Type               string                 `json:"type"`
	Reason             string                 `json:"reason"`
	OK                 bool                   `json:"ok"`
	TokensBefore       *int                   `json:"tokensBefore,omitempty"`
	TokensAfter        *int                   `json:"tokensAfter,omitempty"`
	FirstKeptMessageID string                 `json:"firstKeptMessageId,omitempty"`
	WillRetry          bool                   `json:"willRetry"`
	Fallback           string                 `json:"fallback,omitempty"`
	Mark               *ContextCompactionMark `json:"mark,omitempty"`
	Error              *CompactionError       `json:"error,omitempty"`
	ErrorMessage       string                 `json:"errorMessage,omitempty"`
}

// ContextCompactionMark is the bounded identity the renderer draws a transcript
// row from (packages/shared/src/types/sessions.ts ContextCompactionMark).
type ContextCompactionMark struct {
	ID               string `json:"id"`
	ThroughMessageID string `json:"throughMessageId"`
	Summarized       bool   `json:"summarized"`
	Generation       int    `json:"generation"`
	SummaryTokens    int    `json:"summaryTokens"`
}

// CompactionError names why a compaction did not install a checkpoint.
type CompactionError struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}
