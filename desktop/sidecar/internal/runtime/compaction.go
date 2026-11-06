package runtime

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/smallnest/pigo/desktop/sidecar/internal/jsonrpc"
	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/compaction"
	"github.com/smallnest/pigo/internal/provider"
)

// Auto-compaction keeps a long session inside the model window and installs a
// durable checkpoint host-core can replay after a runtime rebuild. pigo's loop
// owns the trigger and the summarization call; the sidecar supplies the
// thresholds through RunConfig and persists the result from OnCompaction.

const (
	// compactionReserveTokens is held back from the window for the summary
	// request and the model's own output, matching pigo's default.
	compactionReserveTokens = 16384
	// compactionKeepRecentTokens is the approximate recent context retained
	// verbatim, matching pigo's default.
	compactionKeepRecentTokens = 20000
	// compactionPersistTimeout bounds the two host calls one checkpoint needs so
	// a stalled host cannot hold the agent loop indefinitely.
	compactionPersistTimeout = 15 * time.Second

	compactionRetentionCompletedTurn = "completed_turn"
)

// compactionSettingsForWindow derives thresholds that stay meaningful for both
// tiny and large windows. The defaults assume a real model window; a small one
// must not leave a negative usable budget or a retained tail larger than the
// whole window.
func compactionSettingsForWindow(window int) compaction.CompactionSettings {
	if window <= 0 {
		return compaction.CompactionSettings{}
	}
	reserve := window / 2
	if reserve > compactionReserveTokens {
		reserve = compactionReserveTokens
	}
	if reserve < 1 {
		reserve = 1
	}
	usable := window - reserve
	if usable < 1 {
		usable = 1
	}
	keep := usable / 3
	if keep > compactionKeepRecentTokens {
		keep = compactionKeepRecentTokens
	}
	if keep < 1 {
		keep = 1
	}
	return compaction.CompactionSettings{
		Enabled:          true,
		ReserveTokens:    reserve,
		KeepRecentTokens: keep,
	}
}

// compactionRecord mirrors CompactionRecord in
// crates/host-core/src/transcripts.rs, the payload session.appendCompaction
// takes. RetainedTail is always emitted because host-core reads it as an
// optional array.
type compactionRecord struct {
	ID                 string            `json:"id"`
	Summary            string            `json:"summary"`
	FirstKeptMessageID string            `json:"firstKeptMessageId,omitempty"`
	ThroughMessageID   string            `json:"throughMessageId"`
	TokensBefore       int               `json:"tokensBefore"`
	RetainedTail       []json.RawMessage `json:"retainedTail"`
	Details            map[string]any    `json:"details,omitempty"`
	ProviderID         string            `json:"providerId,omitempty"`
	ModelID            string            `json:"modelId,omitempty"`
	CreatedAt          string            `json:"createdAt"`
}

// compactionOutcome is one OnCompaction result waiting for its CompactionEvent.
type compactionOutcome struct {
	Mark               *protocol.ContextCompactionMark
	FirstKeptMessageID string
	Err                error
}

// onCompaction persists the checkpoint pigo just built. It runs on the loop
// goroutine before the event is emitted, so the event can carry the installed
// checkpoint's identity.
func (s *Session) onCompaction(ctx context.Context, event agentcore.CompactionEvent, before agentcore.MessageList) {
	if event.ErrorMessage != "" || strings.TrimSpace(event.Summary) == "" {
		return
	}
	persistCtx, cancel := context.WithTimeout(ctx, compactionPersistTimeout)
	defer cancel()
	outcome := compactionOutcome{}
	outcome.Mark, outcome.FirstKeptMessageID, outcome.Err = s.persistCompaction(persistCtx, before, event)
	if outcome.Err != nil {
		logf(s.logger, "session %s: compaction checkpoint not persisted: %v", s.sessionID, outcome.Err)
	}
	s.mu.Lock()
	s.compactionOutcomes = append(s.compactionOutcomes, outcome)
	s.mu.Unlock()
}

// takeCompactionOutcome pops the outcome for the CompactionEvent being mapped.
// Events and outcomes are produced in the same order, so a FIFO keeps them
// aligned even when the loop runs ahead of event delivery.
func (s *Session) takeCompactionOutcome() compactionOutcome {
	s.mu.Lock()
	defer s.mu.Unlock()
	if len(s.compactionOutcomes) == 0 {
		return compactionOutcome{}
	}
	outcome := s.compactionOutcomes[0]
	s.compactionOutcomes = s.compactionOutcomes[1:]
	return outcome
}

func (s *Session) nextCompactionGeneration() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.compactionGeneration + 1
}

// persistCompaction anchors pigo's cutoff on a durable transcript row and writes
// one checkpoint through host-core. It returns the mark the renderer draws and
// the first retained row id.
func (s *Session) persistCompaction(ctx context.Context, before agentcore.MessageList, event agentcore.CompactionEvent) (*protocol.ContextCompactionMark, string, error) {
	if s.host == nil {
		return nil, "", errors.New("host proxy unavailable")
	}
	detail, err := fetchSessionDetail(ctx, s.host, s.sessionID)
	if err != nil {
		return nil, "", fmt.Errorf("read session for compaction: %w", err)
	}
	hostMessages, hostIDs := compactionContextWithIDs(detail, s.model)
	through, firstKept, ok := compactionBoundary(hostMessages, hostIDs, before, event.SummarizedCount)
	if !ok {
		return nil, "", errors.New("compaction boundary does not map to the host transcript")
	}
	generation := s.nextCompactionGeneration()
	record := compactionRecord{
		ID:                 newID(),
		Summary:            event.Summary,
		FirstKeptMessageID: firstKept,
		ThroughMessageID:   through,
		TokensBefore:       event.TokensBefore,
		RetainedTail:       []json.RawMessage{},
		Details: map[string]any{
			"strategy":         "summary",
			"retainedTailMode": compactionRetentionCompletedTurn,
			"generation":       generation,
		},
		ProviderID: s.model.Provider,
		ModelID:    s.model.ID,
		CreatedAt:  time.Now().UTC().Format(time.RFC3339Nano),
	}
	if err := s.host.CallInto(ctx, "session.appendCompaction", map[string]any{
		"sessionId":  s.sessionID,
		"compaction": record,
	}, nil); err != nil {
		return nil, "", fmt.Errorf("append compaction checkpoint: %w", err)
	}
	s.mu.Lock()
	s.compactionGeneration = generation
	s.mu.Unlock()
	return &protocol.ContextCompactionMark{
		ID:               record.ID,
		ThroughMessageID: record.ThroughMessageID,
		Summarized:       true,
		Generation:       generation,
		SummaryTokens:    estimateSummaryTokens(record.Summary),
	}, firstKept, nil
}

// compactionContextWithIDs rebuilds the model context host-core would hand a
// fresh runtime, with the transcript row id behind each message. A message
// without a durable row carries an empty id.
func compactionContextWithIDs(detail *sessionDetail, model provider.Model) (agentcore.MessageList, []string) {
	if detail == nil || detail.Session == nil {
		return nil, nil
	}
	rows := detail.Session.Messages
	checkpoint := detail.Session.Compaction
	if checkpoint == nil {
		return historyToMessagesWithIDs(rows, model)
	}
	boundary := indexOfRow(rows, checkpoint.ThroughMessageID)
	if boundary < 0 {
		return historyToMessagesWithIDs(rows, model)
	}

	messages := agentcore.MessageList{agentcore.CompactionMessage{
		RoleField:    agentcore.RoleCompaction,
		Summary:      checkpoint.Summary,
		TokensBefore: checkpoint.TokensBefore,
		Timestamp:    parseHistoryTimestamp(checkpoint.CreatedAt),
	}}
	// The checkpoint itself is not a transcript row; its boundary message is the
	// durable anchor a later checkpoint can point at.
	ids := []string{checkpoint.ThroughMessageID}
	tail := retainedTailMessages(checkpoint)
	messages = append(messages, tail...)
	ids = append(ids, make([]string, len(tail))...)
	suffix, suffixIDs := historyToMessagesWithIDs(rows[boundary+1:], model)
	messages = append(messages, suffix...)
	ids = append(ids, suffixIDs...)
	return messages, ids
}

// compactionBoundary maps the summarized prefix onto host rows. It refuses the
// mapping when the two contexts diverge (a rewrite, a lagging host that skipped
// a message, or a transient internal prompt), because a checkpoint anchored on
// the wrong row would replay a context the user never saw.
func compactionBoundary(hostMessages agentcore.MessageList, hostIDs []string, live agentcore.MessageList, summarized int) (string, string, bool) {
	if summarized <= 0 || summarized > len(hostMessages) || summarized > len(live) {
		return "", "", false
	}
	for index := 0; index < summarized; index++ {
		if messageSignature(hostMessages[index]) != messageSignature(live[index]) {
			return "", "", false
		}
	}
	if summarized > len(hostIDs) {
		return "", "", false
	}
	through := strings.TrimSpace(hostIDs[summarized-1])
	if through == "" {
		return "", "", false
	}
	firstKept := ""
	if summarized < len(hostIDs) {
		firstKept = hostIDs[summarized]
	}
	return through, firstKept, true
}

// compactOverflow recovers a turn whose provider rejected the context as too
// large. It installs a checkpoint, rewrites the live context and the LLM view
// the caller is about to retry, and reports overflow compaction events. The
// caller retries the request once when it returns true.
func (s *Session) compactOverflow(ctx context.Context, llm *provider.LlmContext) bool {
	s.setActivityDetails(&protocol.AgentActivity{
		Phase:  "compacting",
		Since:  time.Now().UnixMilli(),
		Reason: "overflow",
	})
	s.emitEvent(s.CurrentTurnID(), protocol.CompactionStartEvent{
		Type:   protocol.TypeCompactionStart,
		Reason: "overflow",
	})

	before := append(agentcore.MessageList(nil), s.agentCtx.Messages...)
	tokensBefore := compaction.EstimateContextTokens(before).Tokens
	s.mu.Lock()
	model := s.model
	apiKey := s.runCfg.APIKey
	thinking := s.thinking
	s.mu.Unlock()
	settings := compactionSettingsForWindow(model.ContextWindow)
	res, err := compaction.Compact(
		ctx,
		s.baseStreamFn,
		model,
		before,
		settings,
		-1,
		nil,
		"",
		provider.StreamConfig{APIKey: apiKey, ThinkingLevel: thinking},
	)
	if err != nil || res == nil {
		message := "no context is available to compact"
		if err != nil {
			message = err.Error()
		}
		s.emitEvent(s.CurrentTurnID(), protocol.CompactionEndEvent{
			Type:         protocol.TypeCompactionEnd,
			Reason:       "overflow",
			OK:           false,
			TokensBefore: &tokensBefore,
			WillRetry:    false,
			Error:        &protocol.CompactionError{Code: "CONTEXT_COMPACTION_FAILED", Message: message},
		})
		s.setActivity("waiting-model", "")
		return false
	}

	mark, firstKept, persistErr := s.persistCompaction(ctx, before, agentcore.CompactionEvent{
		Reason:          "overflow",
		TokensBefore:    res.TokensBefore,
		SummarizedCount: res.FirstKeptIndex,
		Summary:         res.Summary,
	})
	rebuilt := res.RebuildContext(before, time.Now().UnixMilli())
	s.agentCtx.Messages = rebuilt
	llm.Messages = rebuilt
	after := compaction.EstimateContextTokens(rebuilt).Tokens

	event := protocol.CompactionEndEvent{
		Type:               protocol.TypeCompactionEnd,
		Reason:             "overflow",
		OK:                 persistErr == nil,
		TokensBefore:       &tokensBefore,
		TokensAfter:        &after,
		FirstKeptMessageID: firstKept,
		WillRetry:          true,
		Mark:               mark,
	}
	if persistErr != nil {
		event.Error = &protocol.CompactionError{
			Code:    "CONTEXT_COMPACTION_FAILED",
			Message: persistErr.Error(),
		}
	}
	s.emitEvent(s.CurrentTurnID(), event)
	s.setActivity("waiting-model", "")
	return true
}

// Compact installs one manual checkpoint behind agent.compact. The summary
// covers the whole live context and the transcript rows follow the checkpoint's
// boundary, matching the desktop's completed-turn checkpoint.
func (s *Session) Compact(ctx context.Context) error {
	s.mu.Lock()
	if s.disposed {
		s.mu.Unlock()
		return jsonrpc.AppError("RUNTIME_NOT_FOUND", "runtime not found for session")
	}
	if s.running || s.compacting {
		s.mu.Unlock()
		return jsonrpc.AppError("AGENT_BUSY", "session already has an active turn")
	}
	s.compacting = true
	s.mu.Unlock()
	defer func() {
		s.mu.Lock()
		s.compacting = false
		s.mu.Unlock()
	}()

	s.setActivityDetails(&protocol.AgentActivity{
		Phase:  "compacting",
		Since:  time.Now().UnixMilli(),
		Reason: "manual",
	})
	s.emitEvent(s.CurrentTurnID(), protocol.CompactionStartEvent{
		Type:   protocol.TypeCompactionStart,
		Reason: "manual",
	})

	before := append(agentcore.MessageList(nil), s.agentCtx.Messages...)
	tokensBefore := compaction.EstimateContextTokens(before).Tokens
	if len(before) == 0 {
		s.emitCompactionFailure("manual", tokensBefore, "no context is available to compact")
		s.clearActivity()
		return jsonrpc.AppError("CONTEXT_COMPACTION_FAILED", "no context is available to compact")
	}
	s.mu.Lock()
	model := s.model
	apiKey := s.runCfg.APIKey
	thinking := s.thinking
	s.mu.Unlock()
	settings := compactionSettingsForWindow(model.ContextWindow)
	reserve := settings.ReserveTokens
	if reserve <= 0 {
		reserve = compactionReserveTokens
	}
	summary, err := compaction.GenerateSummary(
		ctx,
		s.baseStreamFn,
		model,
		before,
		reserve,
		"",
		provider.StreamConfig{APIKey: apiKey, ThinkingLevel: thinking},
	)
	if err != nil || strings.TrimSpace(summary) == "" {
		message := "context compaction produced no summary"
		if err != nil {
			message = err.Error()
		}
		s.emitCompactionFailure("manual", tokensBefore, message)
		s.clearActivity()
		return jsonrpc.AppError("CONTEXT_COMPACTION_FAILED", "%s", message)
	}

	mark, firstKept, persistErr := s.persistCompaction(ctx, before, agentcore.CompactionEvent{
		Reason:          "manual",
		TokensBefore:    tokensBefore,
		SummarizedCount: len(before),
		Summary:         summary,
	})
	if persistErr != nil {
		s.emitCompactionFailure("manual", tokensBefore, persistErr.Error())
		s.clearActivity()
		return jsonrpc.AppError("CONTEXT_COMPACTION_FAILED", "%s", persistErr.Error())
	}

	now := time.Now().UnixMilli()
	s.agentCtx.Messages = agentcore.MessageList{agentcore.CompactionMessage{
		RoleField:    agentcore.RoleCompaction,
		Summary:      summary,
		TokensBefore: tokensBefore,
		Timestamp:    now,
	}}
	after := compaction.EstimateContextTokens(s.agentCtx.Messages).Tokens
	s.emitEvent(s.CurrentTurnID(), protocol.CompactionEndEvent{
		Type:               protocol.TypeCompactionEnd,
		Reason:             "manual",
		OK:                 true,
		TokensBefore:       &tokensBefore,
		TokensAfter:        &after,
		FirstKeptMessageID: firstKept,
		WillRetry:          false,
		Mark:               mark,
	})
	s.clearActivity()
	return nil
}

func (s *Session) emitCompactionFailure(reason string, tokensBefore int, message string) {
	s.emitEvent(s.CurrentTurnID(), protocol.CompactionEndEvent{
		Type:         protocol.TypeCompactionEnd,
		Reason:       reason,
		OK:           false,
		TokensBefore: &tokensBefore,
		WillRetry:    false,
		Error:        &protocol.CompactionError{Code: "CONTEXT_COMPACTION_FAILED", Message: message},
	})
}

// estimateSummaryTokens is the four-characters-per-token heuristic the shared
// context inspector uses.
func estimateSummaryTokens(summary string) int {
	return (utf8.RuneCountInString(summary) + 3) / 4
}
