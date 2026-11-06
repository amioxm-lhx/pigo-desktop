package runtime

import (
	"context"
	"encoding/json"
	"strings"

	"github.com/smallnest/pigo/desktop/sidecar/internal/jsonrpc"
	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
)

const (
	EnterPlanModeTool = "EnterPlanMode"
	EnterGoalModeTool = "EnterGoalMode"
	SubmitPlanTool    = "SubmitPlan"
	SubmitGoalTool    = "SubmitGoal"

	KindPlan = "plan"
	KindGoal = "goal"

	planningStateInactive         = "inactive"
	planningStatePlanning         = "planning"
	planningStateAwaitingApproval = "awaiting_approval"

	submitInvalidCode = "PLAN_INVALID_ARGUMENT"
	submitFailedCode  = "PLAN_SUBMIT_FAILED"
)

var enterToolNames = map[string]string{KindPlan: EnterPlanModeTool, KindGoal: EnterGoalModeTool}
var submitToolNames = map[string]string{KindPlan: SubmitPlanTool, KindGoal: SubmitGoalTool}

func proposalKindForMode(mode string) string {
	switch normalizeMode(mode) {
	case KindPlan:
		return KindPlan
	case KindGoal:
		return KindGoal
	default:
		return ""
	}
}

func planningStateForMode(mode string) string {
	if proposalKindForMode(mode) != "" {
		return planningStatePlanning
	}
	return planningStateInactive
}

func modeLabel(kind string) string {
	if kind == KindGoal {
		return "Goal"
	}
	return "Plan"
}

func enterToolKind(name string) string {
	switch name {
	case EnterPlanModeTool:
		return KindPlan
	case EnterGoalModeTool:
		return KindGoal
	default:
		return ""
	}
}

func submitToolKind(name string) string {
	switch name {
	case SubmitPlanTool:
		return KindPlan
	case SubmitGoalTool:
		return KindGoal
	default:
		return ""
	}
}

// modeTool implements the four plan/goal transition tools. Enter tools carry no
// arguments; submit tools carry one complete snapshot.
type modeTool struct {
	session     *Session
	name        string
	kind        string
	submit      bool
	description string
	schema      json.RawMessage
}

func newEnterModeTool(session *Session, kind string) agentcore.AgentTool {
	description := "Switch this same agent into Plan mode after the host confirms the durable session transition. Use when the user wants to agree on implementation steps before any change is made."
	if kind == KindGoal {
		description = "Switch this same agent into Goal mode after the host confirms the durable session transition. Use when the user states an outcome and wants to agree on the goal and acceptance criteria before autonomous execution."
	}
	return &modeTool{
		session:     session,
		name:        enterToolNames[kind],
		kind:        kind,
		description: description,
		schema:      json.RawMessage(`{"type":"object","properties":{}}`),
	}
}

func newSubmitContractTool(session *Session, kind string) agentcore.AgentTool {
	description := "Submit one complete Markdown implementation plan for user approval. After a rejected or interrupted approval, revise the plan and submit a new full snapshot. Do not use this until the plan is concrete."
	if kind == KindGoal {
		description = "Submit one complete Markdown goal contract for approval: the outcome, checkable acceptance criteria, and boundaries. After a rejected or interrupted approval, revise the contract and submit a new full snapshot. Do not use this until the goal is unambiguous."
	}
	name := submitToolNames[kind]
	return &modeTool{
		session:     session,
		name:        name,
		kind:        kind,
		submit:      true,
		description: description,
		schema: json.RawMessage(`{
			"type":"object",
			"properties":{
				"title":{"type":"string","description":"A concise title."},
				"markdown":{"type":"string","description":"The exact Markdown contract snapshot."},
				"question":{"type":"string","description":"The question the user answers when approving."}
			},
			"required":["title","markdown","question"]
		}`),
	}
}

func (t *modeTool) Name() string        { return t.name }
func (t *modeTool) Description() string { return t.description }
func (t *modeTool) Schema() json.RawMessage {
	return append(json.RawMessage(nil), t.schema...)
}
func (t *modeTool) ExecutionMode() agentcore.ToolExecutionMode {
	return agentcore.ToolExecutionSequential
}

func (t *modeTool) Execute(ctx context.Context, id string, args json.RawMessage, _ agentcore.ToolUpdateFunc) (agentcore.AgentToolResult, error) {
	if t.submit {
		return t.session.submitContract(ctx, id, t.kind, args)
	}
	return t.session.enterMode(ctx, id, t.kind)
}

// beforeToolCall enforces the mode boundary even though all four transition
// tools stay registered for the lifetime of the runtime. Enter tools belong to
// Agent mode; Submit tools belong to their own contract mode; Write/Edit are
// blocked once a session enters a contract mode.
func (s *Session) beforeToolCall(_ context.Context, call agentcore.AgentToolCall) *agentcore.BeforeToolCallDecision {
	name := call.Name
	mode := s.Mode()

	if kind := enterToolKind(name); kind != "" && mode != "agent" {
		return blockedTool(name + " is available only in Agent mode.")
	}
	if kind := submitToolKind(name); kind != "" && mode != kind {
		return blockedTool(name + " is available only in " + modeLabel(kind) + " mode.")
	}
	if mode != "agent" && (name == "Write" || name == "Edit") {
		return blockedTool(name + " is not available in " + modeLabel(mode) + " mode.")
	}
	return nil
}

func blockedTool(reason string) *agentcore.BeforeToolCallDecision {
	content := agentcore.ContentList{agentcore.NewTextContent(reason)}
	return &agentcore.BeforeToolCallDecision{Block: true, Content: &content}
}

// enterMode asks the host to commit the durable mode change, then adopts it.
func (s *Session) enterMode(ctx context.Context, toolCallID, kind string) (agentcore.AgentToolResult, error) {
	if s.host == nil {
		return agentcore.AgentToolResult{}, jsonrpc.AppError("HOST_UNAVAILABLE", "host channel is unavailable")
	}
	turnID := s.CurrentTurnID()
	if turnID == "" {
		return agentcore.AgentToolResult{}, jsonrpc.AppError("TURN_NOT_FOUND", "mode transition requires an active turn")
	}
	if _, err := s.host.Call(ctx, "plans.enter", map[string]any{
		"sessionId":  s.sessionID,
		"turnId":     turnID,
		"toolCallId": toolCallID,
		"kind":       kind,
	}); err != nil {
		return agentcore.AgentToolResult{}, err
	}

	s.setMode(kind)
	s.setPlanningState(planningStatePlanning, kind, nil)

	text := "Plan mode is active. Inspect the workspace, formulate the plan, then call SubmitPlan for approval."
	if kind == KindGoal {
		text = "Goal mode is active. Clarify the outcome and how it will be verified, then call SubmitGoal for approval."
	}
	return agentcore.AgentToolResult{
		Content: agentcore.ContentList{agentcore.NewTextContent(text)},
		Details: map[string]any{
			"mode":          kind,
			"kind":          kind,
			"planningState": planningStatePlanning,
		},
	}, nil
}

// submitContract submits one snapshot and opens the host approval gate.
func (s *Session) submitContract(ctx context.Context, toolCallID, kind string, args json.RawMessage) (agentcore.AgentToolResult, error) {
	title, markdown, question := submitArguments(args)
	if title == "" || strings.TrimSpace(markdown) == "" || question == "" {
		s.markToolError(toolCallID)
		return failureToolResult(modeLabel(kind)+" requires non-empty title, markdown, and question.", submitInvalidCode, false), nil
	}

	turnID := s.CurrentTurnID()
	if turnID == "" {
		s.markToolError(toolCallID)
		return failureToolResult(modeLabel(kind)+" submission requires an active turn.", submitFailedCode, true), nil
	}
	if s.host == nil {
		s.markToolError(toolCallID)
		return failureToolResult(modeLabel(kind)+" submission failed: host unavailable.", submitFailedCode, true), nil
	}

	var reply struct {
		Status   string                 `json:"status"`
		Proposal *protocol.PlanProposal `json:"proposal"`
	}
	request := map[string]any{
		"sessionId":  s.sessionID,
		"turnId":     turnID,
		"toolCallId": toolCallID,
		"kind":       kind,
		"title":      title,
		"markdown":   markdown,
		"question":   question,
	}
	if err := s.host.CallInto(ctx, "plans.submit", request, &reply); err != nil {
		s.markToolError(toolCallID)
		return failureToolResult(modeLabel(kind)+" submission failed: "+err.Error(), submitFailedCode, true), nil
	}

	proposal := reply.Proposal
	status := reply.Status
	if status == "" && proposal != nil {
		status = proposal.Status
	}
	if status != "pending" || proposal == nil ||
		strings.TrimSpace(proposal.ID) == "" ||
		proposal.Artifact == nil ||
		strings.TrimSpace(proposal.Artifact.RelativePath) == "" ||
		strings.TrimSpace(proposal.Artifact.SHA256) == "" ||
		proposal.Artifact.SizeBytes <= 0 {
		s.markToolError(toolCallID)
		return failureToolResult(modeLabel(kind)+" submission returned an invalid proposal.", submitFailedCode, true), nil
	}

	s.setPlanningState(planningStateAwaitingApproval, kind, proposal)

	text := "Plan submitted for approval. Execution begins only after approval."
	if kind == KindGoal {
		text = "Goal contract submitted for approval. Autonomous execution begins only after approval."
	}
	terminate := true
	return agentcore.AgentToolResult{
		Content:   agentcore.ContentList{agentcore.NewTextContent(text)},
		Details:   map[string]any{"proposal": proposal},
		Terminate: &terminate,
	}, nil
}

func submitArguments(args json.RawMessage) (string, string, string) {
	var params struct {
		Title    string `json:"title"`
		Markdown string `json:"markdown"`
		Question string `json:"question"`
	}
	if err := json.Unmarshal(args, &params); err != nil {
		return "", "", ""
	}
	return strings.TrimSpace(params.Title), params.Markdown, strings.TrimSpace(params.Question)
}

func failureToolResult(text, code string, terminate bool) agentcore.AgentToolResult {
	terminatePtr := terminate
	return agentcore.AgentToolResult{
		Content:   agentcore.ContentList{agentcore.NewTextContent(text)},
		Details:   map[string]any{"errorCode": code},
		Terminate: &terminatePtr,
	}
}

// setMode adopts a host-committed mode and rebuilds the system prompt. All
// transition tools remain registered; beforeToolCall enforces availability.
func (s *Session) setMode(mode string) {
	normalized := normalizeMode(mode)
	s.mu.Lock()
	s.mode = normalized
	s.params.Mode = normalized
	s.agentCtx.SystemPrompt = BuildSystemPrompt(s.params)
	s.planningState = planningStateForMode(normalized)
	s.pendingPlanID = ""
	s.mu.Unlock()
}

// setPlanningState records the contract state and publishes planning_state.
func (s *Session) setPlanningState(state, kind string, proposal *protocol.PlanProposal) {
	s.mu.Lock()
	s.planningState = state
	if proposal != nil {
		s.pendingPlanID = proposal.ID
	} else {
		s.pendingPlanID = ""
	}
	s.mu.Unlock()

	event := protocol.PlanningStateEvent{
		Type:  protocol.TypePlanningState,
		State: state,
		Kind:  kind,
	}
	if proposal != nil {
		event.ProposalID = proposal.ID
		event.Title = proposal.Title
		event.Markdown = proposal.Markdown
		event.Question = proposal.Question
		event.Artifact = proposal.Artifact
		event.Version = proposal.Version
		event.Plan = proposal.Plan
		event.Proposal = proposal
	}
	s.emitEvent(s.CurrentTurnID(), event)
}
