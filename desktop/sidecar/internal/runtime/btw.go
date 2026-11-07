package runtime

import (
	"context"
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

// EventScopeSide marks agent.event envelopes that belong to an out-of-band
// side thread rather than the durable session transcript. The Tauri shell
// forwards them to the renderer but must not persist them or let them drive
// the session's turn lifecycle.
const EventScopeSide = "side"

// SideRunner answers /btw questions against a read-only snapshot of the main
// transcript. Deliberately separate from Manager/Session:
//
//   - no Session is created, so AGENT_BUSY, session.appendMessage and the
//     host's one-running-session index are never touched;
//   - the main AgentContext is never mutated. History is re-read from the host
//     (session.get) and copied into a private context, so a side question can
//     run while the main turn is streaming;
//   - no tools are exposed, so there is no unconfirmed shell behind a GUI that
//     has no permission card for it.
//
// Side state lives only in this process; closing the panel or restarting the
// sidecar discards it.
type SideRunner struct {
	host   Host
	logger *log.Logger

	mu      sync.Mutex
	emitter Emitter
	runs    map[string]context.CancelFunc
}

// NewSideRunner creates the side-thread runner. SetEmitter attaches the event
// channel once the JSON-RPC connection exists.
func NewSideRunner(host Host, logger *log.Logger) *SideRunner {
	return &SideRunner{
		host:   host,
		logger: logger,
		runs:   map[string]context.CancelFunc{},
	}
}

// SetEmitter attaches the outbound event channel.
func (r *SideRunner) SetEmitter(emitter Emitter) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.emitter = emitter
}

// Ask admits one side question and streams its answer as scoped agent.event
// notifications. It returns as soon as the run is started.
func (r *SideRunner) Ask(ctx context.Context, params protocol.BtwParams) (string, error) {
	sessionID := strings.TrimSpace(params.SessionID)
	question := strings.TrimSpace(params.Question)
	if sessionID == "" {
		return "", jsonrpc.InvalidParams("agent.btw requires sessionId")
	}
	if question == "" {
		return "", jsonrpc.InvalidParams("agent.btw requires question")
	}
	binding := params.Binding
	if strings.TrimSpace(binding.SessionID) == "" {
		binding.SessionID = sessionID
	}
	if strings.TrimSpace(binding.Provider.ModelID) == "" {
		return "", jsonrpc.AppError("MODEL_NOT_CONFIGURED", "model/provider not configured")
	}

	prov, model, err := buildProvider(binding.Provider)
	if err != nil {
		return "", jsonrpc.AppError("MODEL_NOT_CONFIGURED", "model/provider not configured")
	}
	apiKey := strings.TrimSpace(binding.Provider.APIKey)
	switch {
	case apiKey == "" && binding.Provider.AuthKind == "none":
		apiKey = "pi-desktop-no-auth"
	case apiKey == "" && binding.Provider.AuthKind == "oauth":
		if r.host == nil {
			return "", jsonrpc.AppError("PROVIDER_NOT_BOUND", "vendor account auth unavailable: no host channel")
		}
		apiKey, err = resolveOAuthKey(ctx, r.host, sessionID, binding.Provider.ID)
		if err != nil {
			return "", err
		}
	case apiKey == "":
		return "", jsonrpc.AppError("MODEL_NOT_CONFIGURED", "model/provider not configured")
	}

	// The host is authoritative for the transcript. A failed read degrades to
	// an empty context instead of failing the side question, matching
	// NewSession's history restore policy.
	history, histErr := loadHistory(ctx, r.host, historyRequest{
		SessionID: sessionID,
		Model:     model,
	})
	if histErr != nil {
		logf(r.logger, "btw session %s: history restore skipped: %v", sessionID, histErr)
	}
	messages := make(agentcore.MessageList, 0, len(history.Messages)+1)
	messages = append(messages, history.Messages...)
	messages = append(messages, agentcore.UserMessage{
		RoleField: agentcore.RoleUser,
		Content:   agentcore.ContentList{agentcore.NewTextContent(question)},
		Timestamp: time.Now().UnixMilli(),
	})

	agentCtx := &agentcore.AgentContext{
		SystemPrompt: btwSystemPrompt(binding),
		Messages:     messages,
	}

	// An empty registry is a guard, not a tool surface: a model that still
	// tries to call a tool gets an "unknown tool" result instead of a nil-map
	// panic, and ShouldStopAfterTurn bounds the run to a single answer.
	cfg := pigoruntime.RunConfig{
		LoopConfig: pigoruntime.LoopConfig{
			Model:         model.ID,
			Provider:      binding.Provider.ID,
			APIKey:        apiKey,
			ThinkingLevel: normalizeThinkingLevel(binding.ThinkingLevel),
			Stream:        provider.StreamFnFromProvider(prov),
			Headers:       binding.Provider.Headers,
			// ContextWindow/Compaction stay zero: auto-compaction is off for a
			// throwaway context that must never write a checkpoint.
		},
		Batch: agenttool.BatchConfig{
			ToolExecutorConfig: agenttool.ToolExecutorConfig{
				Registry: agenttool.NewToolRegistry(),
			},
		},
		ShouldStopAfterTurn: func(context.Context, *agentcore.AgentContext) bool {
			return true
		},
		EventBuffer: 16,
		SessionID:   sessionID,
	}

	sideID := strings.TrimSpace(params.SideID)
	if sideID == "" {
		sideID = newID()
	}
	r.mu.Lock()
	if _, exists := r.runs[sideID]; exists {
		r.mu.Unlock()
		return "", jsonrpc.AppError("BTW_BUSY", "side thread is already answering")
	}
	runCtx, cancel := context.WithCancel(context.Background())
	r.runs[sideID] = cancel
	r.mu.Unlock()

	go r.run(sideID, sessionID, question, model.ID, runCtx, cancel, pigoruntime.StartRun(runCtx, agentCtx, cfg))
	return sideID, nil
}

// Cancel stops one in-flight side answer.
func (r *SideRunner) Cancel(sideID string) bool {
	r.mu.Lock()
	cancel := r.runs[strings.TrimSpace(sideID)]
	r.mu.Unlock()
	if cancel == nil {
		return false
	}
	cancel()
	return true
}

// Count reports how many side answers are in flight.
func (r *SideRunner) Count() int {
	r.mu.Lock()
	defer r.mu.Unlock()
	return len(r.runs)
}

func (r *SideRunner) run(
	sideID, sessionID, question, modelID string,
	ctx context.Context,
	cancel context.CancelFunc,
	stream *pigoruntime.LoopEventStream,
) {
	defer func() {
		cancel()
		r.mu.Lock()
		delete(r.runs, sideID)
		r.mu.Unlock()
	}()

	mapper := &btwMapper{runner: r, sessionID: sessionID, sideID: sideID}
	mapper.emit(protocol.BtwStartEvent{
		Type:     protocol.TypeBtwStart,
		SideID:   sideID,
		Question: question,
		ModelID:  modelID,
	})

	for event := range stream.Events() {
		switch ev := event.(type) {
		case agentcore.MessageStartEvent:
			mapper.onStart(ev)
		case agentcore.MessageUpdateEvent:
			mapper.onUpdate(ev)
		case agentcore.MessageEndEvent:
			mapper.onEnd(ev)
		case agentcore.TurnEndEvent:
			mapper.onTurnEnd(ev)
		default:
			// agent_start / turn_start / agent_end / tool events: a side answer
			// has no durable rows and no tool surface, so they are ignored.
		}
	}
	if _, err := stream.Result(context.Background()); err != nil &&
		!errors.Is(err, context.Canceled) && !errors.Is(err, agentcore.ErrStreamIncomplete) &&
		!mapper.terminal {
		mapper.emitError(classifyAgentError(err).AppError())
	}
	if !mapper.terminal {
		mapper.emitEnd(ctx.Err() != nil)
	}
}

// btwSystemPrompt is intentionally lighter than BuildSystemPrompt: no native
// tool-call instructions, shell, mode or skill catalog, because the side
// thread exposes no tools and must not promise actions it cannot take.
func btwSystemPrompt(binding protocol.PromptParams) string {
	parts := []string{
		"You are answering a brief side question (\"/btw\") while the user continues a main conversation.",
		"The main transcript below is read-only context. Answer concisely in the user's language, using that context and your own knowledge.",
		"No tools are available in this side thread. Do not claim to have read, edited, or executed anything in this answer.",
		"Your answer is shown in a temporary panel and is never added to the main conversation history.",
	}
	if project := strings.TrimSpace(binding.ProjectPath); project != "" {
		parts = append(parts, "The active workspace root is "+project+".")
	}
	if instructions := renderInstructions(binding.ProjectInstructions); instructions != "" {
		parts = append(parts, instructions)
	}
	if memory := strings.TrimSpace(binding.ProjectMemory); memory != "" {
		parts = append(parts, "# Project memory\n\n"+memory)
	}
	return strings.Join(parts, "\n\n")
}

// btwMapper turns one loop's message events into the renderer-only side events.
type btwMapper struct {
	runner    *SideRunner
	sessionID string
	sideID    string

	text     string
	terminal bool
}

func (m *btwMapper) onStart(event agentcore.MessageStartEvent) {
	assistant, ok := event.Message.(agentcore.AssistantMessage)
	if !ok {
		return
	}
	m.text = assistantText(assistant)
}

func (m *btwMapper) onUpdate(event agentcore.MessageUpdateEvent) {
	assistant, ok := event.Message.(agentcore.AssistantMessage)
	if !ok {
		return
	}
	current := assistantText(assistant)
	chunk, reset := delta(m.text, current)
	m.text = current
	if chunk == "" && !reset {
		return
	}
	m.emit(protocol.BtwDeltaEvent{
		Type:   protocol.TypeBtwDelta,
		SideID: m.sideID,
		Text:   chunk,
		Reset:  reset,
	})
}

func (m *btwMapper) onEnd(event agentcore.MessageEndEvent) {
	assistant, ok := event.Message.(agentcore.AssistantMessage)
	if !ok {
		return
	}
	m.text = assistantText(assistant)
	if assistant.StopReason == agentcore.StopReasonError {
		m.emitError(classifyAssistantFailure(assistant).AppError())
		return
	}
	m.emitEnd(assistant.StopReason == agentcore.StopReasonAborted)
}

// onTurnEnd closes a setup failure that never reached message_start: pigo
// reports a failed stream as a terminal assistant message on TurnEnd only.
func (m *btwMapper) onTurnEnd(event agentcore.TurnEndEvent) {
	if m.terminal || event.Message.StopReason != agentcore.StopReasonError {
		return
	}
	m.text = assistantText(event.Message)
	m.emitError(classifyAssistantFailure(event.Message).AppError())
}

func (m *btwMapper) emitEnd(aborted bool) {
	if m.terminal {
		return
	}
	m.terminal = true
	m.emit(protocol.BtwEndEvent{
		Type:    protocol.TypeBtwEnd,
		SideID:  m.sideID,
		Text:    m.text,
		Aborted: aborted,
	})
}

func (m *btwMapper) emitError(appErr protocol.AppError) {
	if m.terminal {
		return
	}
	m.terminal = true
	m.emit(protocol.BtwErrorEvent{
		Type:   protocol.TypeBtwError,
		SideID: m.sideID,
		Error:  appErr,
	})
}

func (m *btwMapper) emit(event any) {
	m.runner.mu.Lock()
	emitter := m.runner.emitter
	logger := m.runner.logger
	m.runner.mu.Unlock()
	if emitter == nil {
		return
	}
	envelope := protocol.EventEnvelope{
		SessionID: m.sessionID,
		TurnID:    m.sideID,
		TS:        time.Now().UnixMilli(),
		Event:     event,
		Scope:     EventScopeSide,
		SideID:    m.sideID,
	}
	if err := emitter.Emit(envelope); err != nil {
		logf(logger, "emit btw event: %v", err)
	}
}
