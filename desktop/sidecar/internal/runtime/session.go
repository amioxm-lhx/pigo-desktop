package runtime

import (
	"context"
	"encoding/json"
	"errors"
	"log"
	"strings"
	"sync"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/jsonrpc"
	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/agenttool"
	"github.com/smallnest/pigo/internal/provider"
	pigoruntime "github.com/smallnest/pigo/internal/runtime"
)

// Session runs one conversation's agent loop on pigo.
type Session struct {
	host    Host
	emitter Emitter
	logger  *log.Logger

	sessionID string
	mode      string
	shell     protocol.CommandShell
	thinking  agentcore.ThinkingLevel
	params    protocol.PromptParams

	model        provider.Model
	streamFn     provider.StreamFn
	baseStreamFn provider.StreamFn
	baseURL      string
	apiStyle     string

	agentCtx *agentcore.AgentContext
	runCfg   pigoruntime.RunConfig
	tools    []agentcore.AgentTool

	// Host catalog fetched at construction (also the child delegate's tool
	// source) and the parent's live tool map for `tools: inherit`.
	hostCatalog []protocol.ToolDefinition
	liveTools   map[string]agentcore.AgentTool

	subagents         []protocol.SubagentDefinition
	subagentProviders map[string]protocol.ProviderConfig
	subagentSem       chan struct{}

	// Background delegation registry (ADR 0089). Records outlive turns until
	// pruned; the wait targets are the delegates one TaskWait/auto-resume is
	// currently converging on.
	delegationMu          sync.Mutex
	delegations           map[string]*delegationRecord
	delegationWaitTargets []*delegationRecord
	turnEpoch             int
	turnSubagentUsage     *protocol.MessageUsage

	mu                sync.Mutex
	refreshMu         sync.Mutex
	running           bool
	currentTurn       string
	cancel            context.CancelFunc
	stopFlag          bool
	disposed          bool
	agentEnded        bool
	assistantIDs      []string
	toolErrors        map[string]bool
	pendingTools      int
	steering          []agentcore.AgentMessage
	pendingAskTools   map[string]*pendingAsk
	historyRevision   string
	planningState     string
	pendingPlanID     string
	activity          *protocol.AgentActivity
	compacting        bool
	overflowCompacted bool

	compactionGeneration int
	compactionOutcomes   []compactionOutcome
}

// NewSession builds one runtime from a prompt binding.
func NewSession(
	ctx context.Context,
	host Host,
	emitter Emitter,
	logger *log.Logger,
	params protocol.PromptParams,
	streamOverride provider.StreamFn,
	modelOverride provider.Model,
) (*Session, error) {
	session := &Session{
		host:              host,
		emitter:           emitter,
		logger:            logger,
		sessionID:         strings.TrimSpace(params.SessionID),
		mode:              normalizeMode(params.Mode),
		shell:             params.CommandShell,
		thinking:          normalizeThinkingLevel(params.ThinkingLevel),
		params:            params,
		baseURL:           providerBaseURL(params.Provider),
		apiStyle:          resolveAPIStyle(params.Provider.APIStyle, params.Provider.ModelConfig),
		toolErrors:        map[string]bool{},
		pendingAskTools:   map[string]*pendingAsk{},
		planningState:     planningStateForMode(params.Mode),
		subagents:         params.Subagents,
		subagentProviders: params.SubagentProviders,
		subagentSem:       make(chan struct{}, MaxSubagentConcurrency),
		delegations:       map[string]*delegationRecord{},
		liveTools:         map[string]agentcore.AgentTool{},
	}

	model := modelOverride
	if streamOverride != nil {
		if strings.TrimSpace(model.ID) == "" {
			model = provider.Model{Provider: params.Provider.ID, ID: params.Provider.ModelID}
		}
		session.model = model
		session.streamFn = streamOverride
	} else {
		prov, builtModel, err := buildProvider(params.Provider)
		if err != nil {
			return nil, err
		}
		session.model = builtModel
		session.streamFn = provider.StreamFnFromProvider(prov)
	}
	// The primary request path owns the retry budget and the overflow recovery.
	// Compaction summaries use the unwrapped stream so a summary that overflows
	// cannot recursively compact the session it is summarizing.
	session.baseStreamFn = session.streamFn
	session.streamFn = session.withProviderRetry(session.baseStreamFn)

	apiKey := strings.TrimSpace(params.Provider.APIKey)
	if apiKey == "" && params.Provider.AuthKind == "none" {
		apiKey = "pi-desktop-no-auth"
	}
	if apiKey == "" && params.Provider.AuthKind == "oauth" {
		// The host resolved a fresh access token for launch; when it is absent
		// ask for one directly so a late-mounted session still authenticates.
		resolved, err := resolveOAuthKey(ctx, host, session.sessionID, params.Provider.ID)
		if err != nil {
			return nil, err
		}
		apiKey = resolved
	}

	tools, hostCatalog, err := buildHostTools(ctx, session)
	if err != nil {
		return nil, err
	}
	session.hostCatalog = hostCatalog
	tools = append(tools, &browserPreviewTool{session: session})
	tools = append(tools, newAskTool(session), newSkillTool(session),
		newEnterModeTool(session, KindPlan),
		newEnterModeTool(session, KindGoal),
		newSubmitContractTool(session, KindPlan),
		newSubmitContractTool(session, KindGoal),
	)
	session.liveTools = make(map[string]agentcore.AgentTool, len(tools))
	for _, tool := range tools {
		session.liveTools[tool.Name()] = tool
	}
	if len(session.subagents) > 0 {
		tools = append(tools,
			newTaskTool(session),
			newTaskWaitTool(session),
			newTaskListTool(session),
			newTaskStopTool(session),
		)
	}
	session.tools = tools
	session.agentCtx = &agentcore.AgentContext{
		SystemPrompt: BuildSystemPrompt(params),
		Tools:        tools,
	}
	if history, err := loadHistory(ctx, host, historyRequest{
		SessionID:            session.sessionID,
		CurrentUserMessageID: params.UserMessageID,
		CurrentContent:       params.Content,
		TruncateFromID:       params.TruncateFromMessageID,
		TruncateBefore:       params.TruncateBefore,
		Model:                session.model,
	}); err != nil {
		logf(logger, "session %s: history restore skipped: %v", session.sessionID, err)
	} else {
		session.agentCtx.Messages = history.Messages
		session.historyRevision = history.Revision
		session.compactionGeneration = history.Generation
	}

	registry := agenttool.NewToolRegistry()
	for _, tool := range tools {
		if err := registry.Register(tool); err != nil {
			logf(logger, "tool %q has an unusable schema, dropping it: %v", tool.Name(), err)
		}
	}

	oauthProviderID := ""
	if params.Provider.AuthKind == "oauth" {
		oauthProviderID = params.Provider.ID
	}
	session.runCfg = pigoruntime.RunConfig{
		LoopConfig: pigoruntime.LoopConfig{
			Model:         session.model.ID,
			Provider:      params.Provider.ID,
			ThinkingLevel: session.thinking,
			Stream:        session.streamFn,
			SummaryStream: session.baseStreamFn,
			APIKey:        apiKey,
			ContextWindow: session.model.ContextWindow,
			Headers:       params.Provider.Headers,
			// Vendor tokens expire in about an hour. Resolve one per request
			// so a long session never sends a stale token, and never carries a
			// refresh token in this process.
			GetAPIKey: func(callCtx context.Context, _ string) string {
				if oauthProviderID == "" {
					return ""
				}
				token, resolveErr := resolveOAuthKey(callCtx, host, session.sessionID, oauthProviderID)
				if resolveErr != nil {
					logf(logger, "session %s: provider auth refresh failed: %v", session.sessionID, resolveErr)
					return ""
				}
				return token
			},
		},
		Batch: agenttool.BatchConfig{
			ToolExecutorConfig: agenttool.ToolExecutorConfig{
				Registry:       registry,
				BeforeToolCall: session.beforeToolCall,
				AfterToolCall:  session.afterToolCall,
			},
		},
		EventBuffer:  64,
		SessionID:    session.sessionID,
		OnCompaction: session.onCompaction,
		GetFollowUpMessages: func(ctx context.Context, _ *agentcore.AgentContext) []agentcore.AgentMessage {
			return session.followUpMessages(ctx)
		},
		GetSteeringMessages: func(context.Context) []agentcore.AgentMessage {
			return session.takeSteeringMessages()
		},
		ShouldStopAfterTurn: func(context.Context, *agentcore.AgentContext) bool {
			return session.StopRequested()
		},
	}
	// Auto-compaction is inert without a known window; pigo's loop treats that
	// the same way, so a provider row without a context size keeps its old
	// behavior instead of compacting on every turn.
	session.runCfg.Compaction = compactionSettingsForWindow(session.model.ContextWindow)
	return session, nil
}

// resolveOAuthKey asks the host for a short-lived vendor access token. The
// request is bound to (sessionId, providerId); main validates the pair and
// never returns the refresh token.
func resolveOAuthKey(ctx context.Context, host Host, sessionID, providerID string) (string, error) {
	var out struct {
		APIKey string `json:"apiKey"`
	}
	if err := host.CallInto(ctx, "provider.resolveAuth", map[string]string{
		"sessionId":  sessionID,
		"providerId": providerID,
	}, &out); err != nil {
		return "", jsonrpc.AppError("PROVIDER_NOT_BOUND", "vendor account auth unavailable: %v", err)
	}
	if strings.TrimSpace(out.APIKey) == "" {
		return "", jsonrpc.AppError("PROVIDER_NOT_BOUND", "vendor account auth returned an empty token")
	}
	return out.APIKey, nil
}

// Prompt starts one turn. It returns as soon as the turn is admitted; the turn
// runs on its own goroutine and reports through the emitter.
func (s *Session) Prompt(turnID, userMessageID, content string) error {
	s.mu.Lock()
	if s.disposed {
		s.mu.Unlock()
		return jsonrpc.AppError("RUNTIME_NOT_FOUND", "runtime not found for session")
	}
	if s.running || s.compacting {
		s.mu.Unlock()
		return jsonrpc.AppError("AGENT_BUSY", "session already has an active turn")
	}
	if strings.TrimSpace(turnID) == "" {
		turnID = newID()
	}
	s.overflowCompacted = false
	// The prompt context is rebuilt for every turn, so a newly resolved
	// instruction file or memory block reaches the model without rebuilding the
	// runtime.
	s.agentCtx.SystemPrompt = BuildSystemPrompt(s.params)
	s.shell = s.params.CommandShell
	attachments := append([]protocol.PromptAttachment(nil), s.params.Attachments...)
	s.turnEpoch++
	s.turnSubagentUsage = nil
	s.running = true
	s.currentTurn = turnID
	s.stopFlag = false
	s.agentEnded = false
	s.assistantIDs = nil
	s.pendingTools = 0
	s.toolErrors = map[string]bool{}
	s.steering = nil
	turnCtx, cancel := context.WithCancel(context.Background())
	s.cancel = cancel
	planningState := s.planningState
	mode := s.mode
	s.mu.Unlock()

	s.setActivity("starting", "")
	if kind := proposalKindForMode(mode); kind != "" && planningState != planningStateAwaitingApproval {
		s.emitEvent(turnID, protocol.PlanningStateEvent{
			Type:  protocol.TypePlanningState,
			State: planningStatePlanning,
			Kind:  kind,
		})
	}

	s.agentCtx.Messages = append(s.agentCtx.Messages, agentcore.UserMessage{
		RoleField: agentcore.RoleUser,
		Content:   userContent(content, attachments),
		Timestamp: time.Now().UnixMilli(),
	})

	stream := pigoruntime.StartRun(turnCtx, s.agentCtx, s.runCfg)
	go s.consume(turnID, turnCtx, stream)
	return nil
}

func (s *Session) consume(turnID string, ctx context.Context, stream *pigoruntime.LoopEventStream) {
	mapper := &eventMapper{session: s, turnID: turnID}
	for event := range stream.Events() {
		mapper.handle(event)
	}
	if ctx.Err() != nil {
		mapper.finishAborted()
	}

	final, err := stream.Result(context.Background())
	_ = final
	if err != nil && !errors.Is(err, context.Canceled) && !errors.Is(err, agentcore.ErrStreamIncomplete) {
		mapper.emitError(classifyAgentError(err).AppError())
	}
	// A turn cannot end with live delegates: the resume path waits for them, so
	// anything still running belongs to a failed or aborted turn.
	s.abortRunningDelegations()

	s.mu.Lock()
	s.pruneTerminalFailuresLocked()
	s.activity = nil
	// A compaction outcome is consumed by the event that immediately follows it;
	// anything left over belongs to an event that never reached the stream.
	s.compactionOutcomes = nil
	s.overflowCompacted = false
	s.running = false
	s.currentTurn = ""
	if s.cancel != nil {
		s.cancel()
		s.cancel = nil
	}
	s.stopFlag = false
	s.pendingTools = 0
	s.mu.Unlock()

	// agent_end is the last event a consumer sees, so the session must already
	// read idle when it arrives.
	if !s.ended() {
		mapper.emitAgentEnd()
	}
}

// UpdateParams refreshes the prompt inputs for a reused runtime. The caller
// guarantees the session is idle.
func (s *Session) UpdateParams(params protocol.PromptParams) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.params = params
	s.shell = params.CommandShell
	s.subagents = params.Subagents
	s.subagentProviders = params.SubagentProviders
	s.thinking = normalizeThinkingLevel(params.ThinkingLevel)
	s.runCfg.LoopConfig.ThinkingLevel = s.thinking
	if key := strings.TrimSpace(params.Provider.APIKey); key != "" {
		s.runCfg.LoopConfig.APIKey = key
	}
	s.runCfg.LoopConfig.Headers = params.Provider.Headers
	window := s.model.ContextWindow
	if config := params.Provider.ModelConfig; config != nil && config.ContextWindow > 0 {
		window = config.ContextWindow
	}
	s.runCfg.LoopConfig.ContextWindow = window
	s.runCfg.Compaction = compactionSettingsForWindow(window)
}

// Steer queues user input into the running turn. The loop pulls it after the
// current turn's tool execution, matching the desktop's per-turn semantics.
func (s *Session) Steer(params protocol.SteerParams) (map[string]any, error) {
	s.mu.Lock()
	if !s.running {
		s.mu.Unlock()
		return nil, jsonrpc.AppError("TURN_NOT_FOUND", "No active turn to steer")
	}
	if params.ExpectedTurnID != "" && params.ExpectedTurnID != s.currentTurn {
		s.mu.Unlock()
		return nil, jsonrpc.AppError("TURN_NOT_FOUND", "No active turn to steer")
	}
	turnID := s.currentTurn
	messageID := strings.TrimSpace(params.Message.ID)
	if messageID == "" {
		messageID = newID()
	}
	createdAt := strings.TrimSpace(params.Message.CreatedAt)
	if createdAt == "" {
		createdAt = time.Now().UTC().Format(time.RFC3339Nano)
	}
	s.steering = append(s.steering, agentcore.UserMessage{
		RoleField: agentcore.RoleUser,
		Content:   userContent(params.Content, params.Attachments),
		Timestamp: time.Now().UnixMilli(),
	})
	s.mu.Unlock()

	if s.emitter != nil {
		steering := true
		row := protocol.UiMessage{
			ID:        messageID,
			Role:      protocol.RoleUser,
			Content:   params.Content,
			CreatedAt: createdAt,
			Status:    protocol.StatusComplete,
			Steering:  &steering,
		}
		s.emitEvent(turnID, protocol.MessageEndEvent{
			Type:    protocol.TypeMessageEnd,
			Message: row,
		})
	}
	return map[string]any{"accepted": true, "turnId": turnID, "messageId": messageID}, nil
}

func (s *Session) takeSteeringMessages() []agentcore.AgentMessage {
	s.mu.Lock()
	defer s.mu.Unlock()
	messages := s.steering
	s.steering = nil
	return messages
}

// hasSteeringMessages reports whether input is queued without consuming it, so
// a delegation wait can hand the turn back to the user.
func (s *Session) hasSteeringMessages() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return len(s.steering) > 0
}

// RequestGracefulStop asks the loop to end after the current turn completes.
func (s *Session) RequestGracefulStop() protocol.StopResult {
	s.mu.Lock()
	defer s.mu.Unlock()
	if !s.running || s.stopFlag {
		return protocol.StopResult{Requested: false}
	}
	s.stopFlag = true
	return protocol.StopResult{Requested: true}
}

// Abort cancels the running turn.
func (s *Session) Abort() {
	s.resolvePendingAskTools()
	s.abortRunningDelegations()
	s.mu.Lock()
	cancel := s.cancel
	s.mu.Unlock()
	if cancel != nil {
		cancel()
	}
}

// Stop requests a graceful stop after the current turn. Delegates are aborted
// with it: a stopped parent cannot use their reports.
func (s *Session) Stop() {
	s.abortRunningDelegations()
	s.mu.Lock()
	s.stopFlag = true
	s.mu.Unlock()
}

// StopRequested reports whether a graceful stop was requested.
func (s *Session) StopRequested() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.stopFlag
}

// Dispose tears the session down.
func (s *Session) Dispose() {
	s.resolvePendingAskTools()
	s.abortRunningDelegations()
	s.mu.Lock()
	s.disposed = true
	cancel := s.cancel
	s.mu.Unlock()
	if cancel != nil {
		cancel()
	}
}

// Status returns the session's current status.
func (s *Session) Status() protocol.AgentStatus {
	s.mu.Lock()
	defer s.mu.Unlock()
	return protocol.AgentStatus{
		SessionID:                s.sessionID,
		IsRunning:                s.running || s.compacting,
		CurrentTurnID:            s.currentTurn,
		ModelID:                  s.model.ID,
		PendingToolConfirmations: s.pendingTools,
		PlanningState:            s.planningState,
		PendingPlanID:            s.pendingPlanID,
		Activity:                 s.activity,
	}
}

// ExecuteApprovedPlan admits an approved plan or goal as an internal turn.
// The instruction is part of the model context only; it is never emitted as a
// user row or persisted by the host.
func (s *Session) ExecuteApprovedPlan(execution protocol.PlanExecution, turnID string) error {
	s.mu.Lock()
	if s.disposed {
		s.mu.Unlock()
		return jsonrpc.AppError("RUNTIME_NOT_FOUND", "runtime not found for session")
	}
	if s.running || s.compacting {
		s.mu.Unlock()
		return jsonrpc.AppError("AGENT_BUSY", "session already has an active turn")
	}
	if strings.TrimSpace(execution.SessionID) != s.sessionID {
		s.mu.Unlock()
		return jsonrpc.AppError("PLAN_EXECUTION_NOT_FOUND", "approved plan belongs to another session")
	}
	if strings.TrimSpace(turnID) == "" {
		s.mu.Unlock()
		return jsonrpc.AppError("TURN_NOT_FOUND", "execution turn id required")
	}

	s.mode = "agent"
	s.params.Mode = "agent"
	s.agentCtx.SystemPrompt = BuildSystemPrompt(s.params)
	s.planningState = planningStateInactive
	s.pendingPlanID = ""
	s.turnEpoch++
	s.turnSubagentUsage = nil
	s.running = true
	s.currentTurn = turnID
	s.stopFlag = false
	s.agentEnded = false
	s.assistantIDs = nil
	s.pendingTools = 0
	s.toolErrors = map[string]bool{}
	s.steering = nil
	s.overflowCompacted = false
	turnCtx, cancel := context.WithCancel(context.Background())
	s.cancel = cancel
	s.mu.Unlock()

	s.setActivity("starting", "")
	instruction := planExecutionInstruction(execution)
	s.agentCtx.Messages = append(s.agentCtx.Messages, agentcore.UserMessage{
		RoleField: agentcore.RoleUser,
		Content:   agentcore.ContentList{agentcore.NewTextContent(instruction)},
		Timestamp: time.Now().UnixMilli(),
	})

	stream := pigoruntime.StartRun(turnCtx, s.agentCtx, s.runCfg)
	go s.consume(turnID, turnCtx, stream)
	return nil
}

// planExecutionInstruction builds the internal prompt that drives an approved
// plan/goal execution.
func planExecutionInstruction(execution protocol.PlanExecution) string {
	kind := "plan"
	if execution.Kind == KindGoal {
		kind = "goal"
	}
	label := kind
	lines := []string{
		"Execute the approved " + label + " now.",
		"Approved " + label + " title: " + execution.Title,
		"Approval question: " + execution.Question,
	}
	if execution.Artifact != nil && strings.TrimSpace(execution.Artifact.RelativePath) != "" {
		lines = append(lines, "Use the host-created "+kind+" artifact at the workspace-relative path: "+execution.Artifact.RelativePath)
	}
	if kind == KindGoal {
		lines = append(lines,
			"Treat the following Markdown as the exact approved goal contract. Do not renegotiate it or ask for approval again.",
			"<approved-goal-markdown>",
			execution.Plan,
			"</approved-goal-markdown>",
			"Reach the goal autonomously, verify every acceptance criterion yourself, and finish with a criterion-by-criterion report of met/unmet and the evidence.",
		)
	} else {
		lines = append(lines,
			"Treat the following Markdown as the exact approved plan. Do not replace it or ask for approval again.",
			"<approved-plan-markdown>",
			execution.Plan,
			"</approved-plan-markdown>",
			"Implement the approved plan with the normal Agent tools, then report the result.",
		)
	}
	return strings.Join(lines, "\n")
}

// setActivity publishes the phase that explains a quiet interval. Unchanged
// activity does not emit another status event.
func (s *Session) setActivity(phase, reason string) {
	if phase == "" {
		s.clearActivity()
		return
	}
	s.mu.Lock()
	if s.activity != nil && s.activity.Phase == phase && s.activity.Reason == reason {
		s.mu.Unlock()
		return
	}
	s.mu.Unlock()
	s.setActivityDetails(&protocol.AgentActivity{
		Phase:  phase,
		Since:  time.Now().UnixMilli(),
		Reason: reason,
	})
}

// setActivityDetails publishes a full activity record (attempt, delay, error).
// Retry transitions change the attempt every time, so this path always emits.
func (s *Session) setActivityDetails(activity *protocol.AgentActivity) {
	s.mu.Lock()
	s.activity = activity
	s.mu.Unlock()
	s.emitEvent(s.CurrentTurnID(), protocol.StatusEvent{
		Type:   protocol.TypeStatus,
		Status: s.Status(),
	})
}

func (s *Session) clearActivity() {
	s.mu.Lock()
	if s.activity == nil {
		s.mu.Unlock()
		return
	}
	s.activity = nil
	s.mu.Unlock()
	s.emitEvent(s.CurrentTurnID(), protocol.StatusEvent{
		Type:   protocol.TypeStatus,
		Status: s.Status(),
	})
}

func (s *Session) clearActivitySilently() {
	s.mu.Lock()
	s.activity = nil
	s.mu.Unlock()
}

// Running reports whether a turn is in flight.
func (s *Session) Running() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.running
}

// CurrentTurnID returns the turn in flight (empty when idle).
func (s *Session) CurrentTurnID() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.currentTurn
}

// SessionID returns the desktop session id.
func (s *Session) SessionID() string { return s.sessionID }

// Mode returns the active mode.
func (s *Session) Mode() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.mode
}

// Matches reports whether an existing runtime still serves the binding.
func (s *Session) Matches(params protocol.PromptParams) bool {
	if s.sessionID != strings.TrimSpace(params.SessionID) {
		return false
	}
	if s.mode != normalizeMode(params.Mode) {
		return false
	}
	return s.matchProvider(params.Provider)
}

func (s *Session) matchProvider(cfg protocol.ProviderConfig) bool {
	style := resolveAPIStyle(cfg.APIStyle, cfg.ModelConfig)
	baseURL := providerBaseURL(cfg)
	return s.model.ID == strings.TrimSpace(cfg.ModelID) &&
		s.model.Provider == cfg.ID &&
		s.apiStyle == style &&
		s.baseURL == baseURL
}

func (s *Session) markToolError(id string) {
	s.mu.Lock()
	s.toolErrors[id] = true
	s.mu.Unlock()
}

func (s *Session) addPendingTool() {
	s.mu.Lock()
	s.pendingTools++
	s.mu.Unlock()
}

func (s *Session) removePendingTool() {
	s.mu.Lock()
	if s.pendingTools > 0 {
		s.pendingTools--
	}
	s.mu.Unlock()
}

func (s *Session) markAgentEnded() {
	s.mu.Lock()
	s.agentEnded = true
	s.mu.Unlock()
}

// emitEvent publishes one event on the session's turn. It is the non-mapper
// path used by sidecar-local interactions (steering rows and asktool cards).
func (s *Session) emitEvent(turnID string, event any) {
	s.emitAttributedEvent(turnID, "", "", event)
}

// emitAttributedEvent publishes one event with the subagent provenance a
// delegate's rows carry (D201, ADR 0062). Empty attribution means the event is
// the parent session's own.
func (s *Session) emitAttributedEvent(turnID, parentToolCallID, agentName string, event any) {
	if s.emitter == nil {
		return
	}
	envelope := protocol.EventEnvelope{
		SessionID:        s.sessionID,
		TurnID:           turnID,
		TS:               time.Now().UnixMilli(),
		Event:            event,
		ParentToolCallID: parentToolCallID,
		AgentName:        agentName,
	}
	if err := s.emitter.Emit(envelope); err != nil {
		logf(s.logger, "emit agent event: %v", err)
	}
}

// pruneTerminalFailuresLocked drops assistant rows the desktop transcript never
// sends back to the model: a provider-error row is a UI failure, and an empty
// aborted row contributes nothing. This keeps the live context aligned with a
// host transcript rebuild.
func (s *Session) pruneTerminalFailuresLocked() {
	filtered := make(agentcore.MessageList, 0, len(s.agentCtx.Messages))
	for _, message := range s.agentCtx.Messages {
		assistant, ok := message.(agentcore.AssistantMessage)
		if ok && (assistant.StopReason == agentcore.StopReasonError ||
			(assistant.StopReason == agentcore.StopReasonAborted && len(assistant.Content) == 0)) {
			continue
		}
		filtered = append(filtered, message)
	}
	s.agentCtx.Messages = filtered
}

func (s *Session) consumeToolError(id string) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	failed := s.toolErrors[id]
	delete(s.toolErrors, id)
	return failed
}

func (s *Session) afterToolCall(_ context.Context, call agentcore.AgentToolCall, _ agentcore.AgentToolResult, _ bool) *agentcore.AfterToolCallResult {
	if !s.consumeToolError(call.ID) {
		return nil
	}
	failed := true
	return &agentcore.AfterToolCallResult{IsError: &failed}
}

func (s *Session) ended() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.agentEnded
}

func (s *Session) recordAssistantID(id string) {
	s.mu.Lock()
	s.assistantIDs = append(s.assistantIDs, id)
	s.mu.Unlock()
}

func (s *Session) assistantMessageIDs() []string {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := make([]string, len(s.assistantIDs))
	copy(out, s.assistantIDs)
	return out
}

// userContent builds the content list a user turn carries. Only inlined images
// participate; file attachments stay metadata in the host transcript, matching
// the reference desktop runtime.
func userContent(text string, attachments []protocol.PromptAttachment) agentcore.ContentList {
	blocks := make(agentcore.ContentList, 0, len(attachments)+1)
	if strings.TrimSpace(text) != "" {
		blocks = append(blocks, agentcore.NewTextContent(text))
	}
	for _, attachment := range attachments {
		if attachment.Kind != "image" || strings.TrimSpace(attachment.Data) == "" {
			continue
		}
		mimeType := strings.TrimSpace(attachment.MimeType)
		if mimeType == "" {
			mimeType = "image/png"
		}
		blocks = append(blocks, agentcore.NewImageContent(attachment.Data, mimeType))
	}
	return blocks
}

func logf(logger *log.Logger, format string, args ...any) {
	if logger != nil {
		logger.Printf(format, args...)
	}
}

// JSON helper used by the event mapper.
func marshalRaw(value any) json.RawMessage {
	if value == nil {
		return nil
	}
	raw, err := json.Marshal(value)
	if err != nil {
		return nil
	}
	return raw
}
