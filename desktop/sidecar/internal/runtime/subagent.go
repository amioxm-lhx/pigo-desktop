// Subagent delegation: the `Task` family of tools (ADR 0062 / ADR 0089).
//
// A delegate is a full agent loop with an independent AgentContext, system
// prompt and tool set. `Task` starts one in the background and returns
// immediately with a delegation id; the parent keeps working and converges with
// `TaskWait`, inspects `TaskList`, or cancels with `TaskStop`. Reports the parent
// has not read are delivered by the resume-after-idle follow-up prompt, so a
// delegate's lifecycle never silently ends the parent's turn.
//
// A pinned provider/model is looked up by `<providerId>/<modelId>`; a missing
// entry is a named tool error rather than a silent fallback to the session
// model.
package runtime

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/agenttool"
	"github.com/smallnest/pigo/internal/provider"
	pigoruntime "github.com/smallnest/pigo/internal/runtime"
)

const (
	// TaskToolName starts a delegate; the lifecycle tools drive the record it
	// returned.
	TaskToolName     = "Task"
	TaskWaitToolName = "TaskWait"
	TaskListToolName = "TaskList"
	TaskStopToolName = "TaskStop"

	// MaxSubagentConcurrency bounds running delegates per session, across
	// batches (ADR 0089).
	MaxSubagentConcurrency = 10
	// maxRetainedDelegations caps the registry; finished records are dropped
	// oldest-first, running ones never are.
	maxRetainedDelegations = 100
	// maxSubagentReportChars keeps the only thing that enters the parent's
	// context from becoming the context problem delegation avoids.
	maxSubagentReportChars = 12_000
	// maxTaskWaitResultChars bounds one TaskWait result block.
	maxTaskWaitResultChars = 50_000
	// taskWaitDefaultTimeoutSeconds / taskWaitMaxTimeoutSeconds: a timeout is
	// not a failure, but one call must not park the turn.
	taskWaitDefaultTimeoutSeconds = 600
	taskWaitMaxTimeoutSeconds     = 900

	delegationResumePrompt = "The following subagents have finished. Integrate their reports and continue the user's original task. Call TaskStop only if you have decided a still-running delegate should not continue."

	delegationRunning = "running"
	delegationStopped = "stopped"
)

// Tools that are never inherited, even with `tools: inherit`. Nested fan-out
// and mode switches stay with the parent; asktool needs a user and
// new_context/ToolSearch mutate the parent runtime.
var subagentInheritDeny = map[string]bool{
	TaskToolName: true, TaskWaitToolName: true, TaskListToolName: true, TaskStopToolName: true,
	"EnterPlanMode": true, "EnterGoalMode": true, "asktool": true,
	"new_context": true, "ToolSearch": true,
}

// subagentToolErrors is a delegate's own failed-call set. A child loop shares
// the parent session for host calls but must not leak error flags into the
// parent's per-turn bookkeeping.
type subagentToolErrors struct {
	mu  sync.Mutex
	ids map[string]bool
}

func newSubagentToolErrors() *subagentToolErrors {
	return &subagentToolErrors{ids: map[string]bool{}}
}

func (e *subagentToolErrors) mark(id string) {
	e.mu.Lock()
	e.ids[id] = true
	e.mu.Unlock()
}

func (e *subagentToolErrors) consume(id string) bool {
	e.mu.Lock()
	defer e.mu.Unlock()
	failed := e.ids[id]
	delete(e.ids, id)
	return failed
}

// ---------------------------------------------------------------------------
// Delegation registry
// ---------------------------------------------------------------------------

type delegationRecord struct {
	mu sync.Mutex

	id            string
	agentName     string
	modelID       string
	thinkingLevel string
	status        string
	startedAt     int64
	completedAt   int64
	report        string
	errCode       string
	errMessage    string
	turns         int
	toolCalls     int
	lastToolName  string
	lastPhase     string

	// Task row identity, so settlement can refresh the card the parent saw.
	taskCallID string
	taskTurnID string
	taskArgs   json.RawMessage

	abort      context.CancelFunc
	completion chan struct{}
	finishOnce sync.Once

	// toolEnded is true once the executor emitted the Task tool_end; a fast
	// delegate may settle first, so the terminal snapshot waits for it.
	toolEnded  bool
	settlement *delegationSnapshot

	stopRequested   bool
	startedEpoch    int
	reportDelivered bool
}

type delegationSnapshot struct {
	id              string
	agentName       string
	modelID         string
	thinkingLevel   string
	status          string
	startedAt       int64
	completedAt     int64
	report          string
	errCode         string
	errMessage      string
	turns           int
	toolCalls       int
	lastToolName    string
	lastPhase       string
	reportDelivered bool
}

func (r *delegationRecord) finish() {
	r.finishOnce.Do(func() { close(r.completion) })
}

func (r *delegationRecord) snapshot() delegationSnapshot {
	r.mu.Lock()
	defer r.mu.Unlock()
	return delegationSnapshot{
		id:              r.id,
		agentName:       r.agentName,
		modelID:         r.modelID,
		thinkingLevel:   r.thinkingLevel,
		status:          r.status,
		startedAt:       r.startedAt,
		completedAt:     r.completedAt,
		report:          r.report,
		errCode:         r.errCode,
		errMessage:      r.errMessage,
		turns:           r.turns,
		toolCalls:       r.toolCalls,
		lastToolName:    r.lastToolName,
		lastPhase:       r.lastPhase,
		reportDelivered: r.reportDelivered,
	}
}

func (r *delegationRecord) isRunning() bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	return r.status == delegationRunning
}

func (r *delegationRecord) noteActivity(phase, toolName string) {
	r.mu.Lock()
	if toolName != "" {
		r.lastToolName = toolName
	}
	r.lastPhase = phase
	r.mu.Unlock()
}

func (s *Session) registerDelegation(record *delegationRecord) {
	s.delegationMu.Lock()
	defer s.delegationMu.Unlock()
	if s.delegations == nil {
		s.delegations = map[string]*delegationRecord{}
	}
	s.delegations[record.id] = record
}

func (s *Session) delegation(id string) *delegationRecord {
	s.delegationMu.Lock()
	defer s.delegationMu.Unlock()
	return s.delegations[id]
}

func (s *Session) delegationList() []*delegationRecord {
	s.delegationMu.Lock()
	records := make([]*delegationRecord, 0, len(s.delegations))
	for _, record := range s.delegations {
		records = append(records, record)
	}
	s.delegationMu.Unlock()
	sort.SliceStable(records, func(left, right int) bool {
		return records[left].startedAt < records[right].startedAt
	})
	return records
}

func (s *Session) runningDelegations() []*delegationRecord {
	records := s.delegationList()
	running := make([]*delegationRecord, 0, len(records))
	for _, record := range records {
		if record.isRunning() {
			running = append(running, record)
		}
	}
	return running
}

func (s *Session) pendingCurrentTurnDelegations(epoch int) []*delegationRecord {
	records := s.delegationList()
	pending := make([]*delegationRecord, 0, len(records))
	for _, record := range records {
		snapshot := record.snapshot()
		if snapshot.reportDelivered || record.startedEpoch != epoch {
			continue
		}
		if snapshot.status == delegationStopped || snapshot.status == "aborted" {
			continue
		}
		pending = append(pending, record)
	}
	return pending
}

func (s *Session) abortRunningDelegations() {
	for _, record := range s.runningDelegations() {
		if record.abort != nil {
			record.abort()
		}
	}
}

func (s *Session) pruneFinishedDelegations() {
	type finishedRecord struct {
		record   *delegationRecord
		finished int64
	}
	s.delegationMu.Lock()
	finished := make([]finishedRecord, 0, len(s.delegations))
	for _, record := range s.delegations {
		snapshot := record.snapshot()
		if snapshot.status != delegationRunning {
			finished = append(finished, finishedRecord{record: record, finished: snapshot.completedAt})
		}
	}
	if len(finished) <= maxRetainedDelegations {
		s.delegationMu.Unlock()
		return
	}
	sort.SliceStable(finished, func(left, right int) bool {
		return finished[left].finished < finished[right].finished
	})
	for _, entry := range finished[:len(finished)-maxRetainedDelegations] {
		delete(s.delegations, entry.record.id)
	}
	s.delegationMu.Unlock()
}

// currentTurnEpoch returns the turn generation a delegation belongs to.
func (s *Session) currentTurnEpoch() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.turnEpoch
}

// addSubagentUsage accumulates one delegate turn's token usage so turn_end can
// report what delegation cost.
func (s *Session) addSubagentUsage(usage *agentcore.Usage) {
	if usage == nil {
		return
	}
	s.mu.Lock()
	if s.turnSubagentUsage == nil {
		s.turnSubagentUsage = &protocol.MessageUsage{}
	}
	s.turnSubagentUsage.InputTokens += usage.InputTokens
	s.turnSubagentUsage.OutputTokens += usage.OutputTokens
	s.turnSubagentUsage.TotalTokens = s.turnSubagentUsage.InputTokens + s.turnSubagentUsage.OutputTokens
	s.mu.Unlock()
}

// takeSubagentUsage reads and clears the delegate usage accumulated since the
// previous parent turn_end.
func (s *Session) takeSubagentUsage() *protocol.MessageUsage {
	s.mu.Lock()
	defer s.mu.Unlock()
	usage := s.turnSubagentUsage
	s.turnSubagentUsage = nil
	return usage
}

func delegationSummary(snapshot delegationSnapshot) map[string]any {
	summary := map[string]any{
		"delegationId":  snapshot.id,
		"agent":         snapshot.agentName,
		"modelId":       snapshot.modelID,
		"thinkingLevel": snapshot.thinkingLevel,
		"status":        snapshot.status,
		"startedAt":     snapshot.startedAt,
		"turns":         snapshot.turns,
		"toolCalls":     snapshot.toolCalls,
	}
	if snapshot.lastToolName != "" {
		summary["lastToolName"] = snapshot.lastToolName
	}
	if snapshot.completedAt != 0 {
		summary["completedAt"] = snapshot.completedAt
	}
	if snapshot.errCode != "" || snapshot.errMessage != "" {
		summary["error"] = map[string]any{
			"code":    firstNonEmpty(snapshot.errCode, "SUBAGENT_FAILED"),
			"message": firstNonEmpty(snapshot.errMessage, snapshot.status),
		}
	}
	return summary
}

func elapsedSeconds(snapshot delegationSnapshot, now int64) int {
	end := snapshot.completedAt
	if end == 0 {
		end = now
	}
	seconds := int((end - snapshot.startedAt + 500) / 1000)
	if seconds < 1 {
		return 1
	}
	return seconds
}

func formatDelegationHeartbeat(snapshot delegationSnapshot, now int64) string {
	parts := []string{
		snapshot.agentName + " (" + snapshot.id + ")",
		snapshot.status,
		fmt.Sprintf("%ds", elapsedSeconds(snapshot, now)),
	}
	if snapshot.turns > 0 {
		parts = append(parts, fmt.Sprintf("%d turns", snapshot.turns))
	}
	if snapshot.toolCalls > 0 {
		parts = append(parts, fmt.Sprintf("%d tool calls", snapshot.toolCalls))
	}
	if snapshot.lastToolName != "" {
		parts = append(parts, "last tool "+snapshot.lastToolName)
	}
	return strings.Join(parts, ", ")
}

type delegationResult struct {
	id     string
	agent  string
	status string
	report string
}

// formatDelegationResults joins reports into one bounded block and reports which
// ids actually fit.
func formatDelegationResults(results []delegationResult, note string) (string, map[string]bool) {
	parts := make([]string, 0, len(results)+1)
	included := map[string]bool{}
	total := 0
	omitted := 0
	for _, result := range results {
		block := "## " + result.agent + " (" + result.id + ") — " + result.status + "\n" + result.report
		if total+len(block) > maxTaskWaitResultChars {
			omitted++
			continue
		}
		parts = append(parts, block)
		included[result.id] = true
		total += len(block) + 2
	}
	if omitted > 0 {
		suffix := "s"
		if omitted == 1 {
			suffix = ""
		}
		parts = append(parts, fmt.Sprintf("[%d more result%s omitted to protect this context; call TaskWait with their delegationIds to re-read one.]", omitted, suffix))
	}
	out := nonEmpty(append([]string{strings.TrimSpace(note)}, parts...))
	return strings.Join(out, "\n\n"), included
}

// settleDelegation records a settled run, refreshes the Task card, wakes every
// TaskWait and delivers the report through auto-resume when nobody is waiting.
func (s *Session) settleDelegation(record *delegationRecord, outcome subagentOutcome) {
	record.mu.Lock()
	if record.status != delegationRunning {
		record.mu.Unlock()
		return
	}
	status := outcome.status
	if record.stopRequested {
		// TaskStop wins over whatever the canceled run reported: the delegate
		// was stopped, not finished or failed.
		status = delegationStopped
	}
	record.status = status
	record.completedAt = time.Now().UnixMilli()
	record.report = truncateReport(outcome.report)
	record.errCode = outcome.errCode
	record.errMessage = outcome.errMessage
	if status == delegationStopped {
		record.errCode = ""
		record.errMessage = ""
	}
	record.turns = outcome.turns
	record.toolCalls = outcome.toolCalls
	record.lastToolName = outcome.lastToolName
	snapshot := delegationSnapshot{
		id: record.id, agentName: record.agentName, modelID: record.modelID,
		thinkingLevel: record.thinkingLevel, status: record.status,
		startedAt: record.startedAt, completedAt: record.completedAt,
		report: record.report, errCode: record.errCode, errMessage: record.errMessage,
		turns: record.turns, toolCalls: record.toolCalls,
		lastToolName: record.lastToolName, lastPhase: record.lastPhase,
		reportDelivered: record.reportDelivered,
	}
	record.settlement = &snapshot
	record.mu.Unlock()

	s.flushDelegationSettlement(record)
	record.finish()
	s.pruneFinishedDelegations()
	s.refreshDelegationWait()
}

// markTaskToolEnded lets a settled-fast delegate publish its terminal Task row
// once the executor has emitted tool_end for the starting call.
func (s *Session) markTaskToolEnded(toolCallID string) {
	for _, record := range s.delegationList() {
		record.mu.Lock()
		match := record.taskCallID == toolCallID
		if match {
			record.toolEnded = true
		}
		record.mu.Unlock()
		if match {
			s.flushDelegationSettlement(record)
			return
		}
	}
}

// flushDelegationSettlement publishes a terminal Task row when both the
// delegate settled and the Task tool_end has arrived.
func (s *Session) flushDelegationSettlement(record *delegationRecord) {
	record.mu.Lock()
	snapshot := record.settlement
	if snapshot == nil || !record.toolEnded {
		record.mu.Unlock()
		return
	}
	record.settlement = nil
	settled := *snapshot
	taskCallID := record.taskCallID
	taskTurnID := record.taskTurnID
	taskArgs := record.taskArgs
	record.mu.Unlock()
	s.publishDelegationSettlement(settled, taskCallID, taskTurnID, taskArgs)
}

func truncateReport(report string) string {
	report = strings.TrimSpace(report)
	if len(report) <= maxSubagentReportChars {
		return report
	}
	return report[:maxSubagentReportChars] + "\n\n[report truncated to protect this context]"
}

// publishDelegationSettlement refreshes the original Task row through the
// full-message event, so a card that still says `running` learns how its
// subagent ended. The row id matches the Task tool-call id, so the renderer
// replaces it in place.
func (s *Session) publishDelegationSettlement(snapshot delegationSnapshot, taskCallID, taskTurnID string, taskArgs json.RawMessage) {
	if taskCallID == "" || snapshot.status == delegationRunning {
		return
	}
	text := "Delegation " + snapshot.id + ": " + snapshot.status + "."
	envelope := map[string]any{
		"content": []map[string]any{{"type": "text", "text": text}},
		"details": delegationSummary(snapshot),
	}
	raw, err := json.Marshal(envelope)
	if err != nil {
		return
	}
	createdAt := time.UnixMilli(snapshot.startedAt).UTC().Format(time.RFC3339Nano)
	s.emitEvent(taskTurnID, protocol.MessageEndEvent{
		Type: protocol.TypeMessageEnd,
		Message: protocol.UiMessage{
			ID:         taskCallID,
			Role:       "tool",
			Content:    text,
			CreatedAt:  createdAt,
			Status:     protocol.StatusComplete,
			ToolName:   TaskToolName,
			ToolCallID: taskCallID,
			ToolStatus: protocol.ToolStatusSuccess,
			ToolArgs:   taskArgs,
			ToolResult: raw,
		},
	})
}

// waitingSubagentsActivity renders the live wait roster.
func waitingSubagentsActivity(targets []*delegationRecord, since int64) *protocol.AgentActivity {
	running := make([]delegationSnapshot, 0, len(targets))
	for _, record := range targets {
		snapshot := record.snapshot()
		if snapshot.status == delegationRunning {
			running = append(running, snapshot)
		}
	}
	activity := &protocol.AgentActivity{
		Phase:         "waiting-subagents",
		Since:         since,
		SubagentCount: len(running),
	}
	if len(running) > 0 {
		agents := make([]protocol.AgentActivityAgent, 0, len(running))
		for _, snapshot := range running {
			agent := protocol.AgentActivityAgent{Name: snapshot.agentName}
			if snapshot.lastPhase != "" {
				agent.LastPhase = snapshot.lastPhase
			}
			if snapshot.lastToolName != "" {
				agent.LastToolName = snapshot.lastToolName
			}
			agents = append(agents, agent)
		}
		activity.Agents = agents
	}
	return activity
}

func (s *Session) beginDelegationWait(targets []*delegationRecord) {
	running := make([]*delegationRecord, 0, len(targets))
	for _, record := range targets {
		if record.isRunning() {
			running = append(running, record)
		}
	}
	if len(running) == 0 {
		return
	}
	s.setDelegationWaitTargets(targets)
	since := time.Now().UnixMilli()
	s.mu.Lock()
	if s.activity != nil && s.activity.Phase == "waiting-subagents" && s.activity.Since != 0 {
		since = s.activity.Since
	}
	s.mu.Unlock()
	s.setActivityDetails(waitingSubagentsActivity(targets, since))
}

func (s *Session) refreshDelegationWait() {
	targets := s.delegationWaitTargetsSnapshot()
	if targets == nil {
		return
	}
	s.mu.Lock()
	current := s.activity
	s.mu.Unlock()
	if current == nil || current.Phase != "waiting-subagents" {
		return
	}
	s.setActivityDetails(waitingSubagentsActivity(targets, current.Since))
}

func (s *Session) endDelegationWait() {
	s.setDelegationWaitTargets(nil)
	s.mu.Lock()
	waiting := s.activity != nil && s.activity.Phase == "waiting-subagents"
	s.mu.Unlock()
	if waiting {
		s.clearActivity()
	}
}

func (s *Session) setDelegationWaitTargets(targets []*delegationRecord) {
	s.delegationMu.Lock()
	s.delegationWaitTargets = targets
	s.delegationMu.Unlock()
}

func (s *Session) delegationWaitTargetsSnapshot() []*delegationRecord {
	s.delegationMu.Lock()
	defer s.delegationMu.Unlock()
	return s.delegationWaitTargets
}

// waitForDelegations resolves once `required` targets settle, the deadline
// passes, the run aborts, or stopWhen asks to stop. It returns true when the
// wait was cut short rather than converged.
func (s *Session) waitForDelegations(ctx context.Context, targets []*delegationRecord, required int, deadline time.Time, stopWhen func() bool) bool {
	if required <= 0 {
		return false
	}
	settledCount := func() int {
		count := 0
		for _, record := range targets {
			if !record.isRunning() {
				count++
			}
		}
		return count
	}
	if settledCount() >= required {
		return false
	}
	ticker := time.NewTicker(100 * time.Millisecond)
	defer ticker.Stop()
	var deadlineAt <-chan time.Time
	if !deadline.IsZero() {
		delay := time.Until(deadline)
		if delay < 0 {
			delay = 0
		}
		timer := time.NewTimer(delay)
		defer timer.Stop()
		deadlineAt = timer.C
	}
	if stopWhen != nil && stopWhen() {
		return true
	}
	for {
		select {
		case <-ctx.Done():
			return true
		case <-deadlineAt:
			return true
		case <-ticker.C:
			if settledCount() >= required {
				return false
			}
			if stopWhen != nil && stopWhen() {
				return true
			}
		}
	}
}

// followUpMessages delivers reports the parent has not consumed. It runs when
// the parent's inner loop settles, keeping the turn open until every current
// turn delegate has either reported or been stopped (D328).
func (s *Session) followUpMessages(ctx context.Context) []agentcore.AgentMessage {
	if steering := s.takeSteeringMessages(); len(steering) > 0 {
		return steering
	}
	epoch := s.currentTurnEpoch()
	pending := s.pendingCurrentTurnDelegations(epoch)
	if len(pending) == 0 {
		return nil
	}
	s.beginDelegationWait(pending)
	interrupted := s.waitForDelegations(ctx, pending, len(pending), time.Time{}, func() bool {
		return s.StopRequested() || s.hasSteeringMessages()
	})
	s.endDelegationWait()
	if ctx.Err() != nil || s.StopRequested() {
		return nil
	}
	if interrupted {
		// Input admitted while the parent idled belongs to this turn; run it
		// before deciding whether a report still needs delivering.
		if steering := s.takeSteeringMessages(); len(steering) > 0 {
			return steering
		}
	}
	settled := make([]*delegationRecord, 0, len(pending))
	results := make([]delegationResult, 0, len(pending))
	for _, record := range pending {
		snapshot := record.snapshot()
		if snapshot.status == delegationRunning || snapshot.reportDelivered {
			continue
		}
		settled = append(settled, record)
		report := snapshot.report
		if report == "" {
			report = "(" + snapshot.status + " without a report)"
		}
		results = append(results, delegationResult{
			id: snapshot.id, agent: snapshot.agentName, status: snapshot.status, report: report,
		})
	}
	if len(settled) == 0 {
		return nil
	}
	formatted, _ := formatDelegationResults(results, "")
	for _, record := range settled {
		record.mu.Lock()
		record.reportDelivered = true
		record.mu.Unlock()
	}
	text := strings.Join(nonEmpty([]string{delegationResumePrompt, formatted}), "\n\n")
	return []agentcore.AgentMessage{
		agentcore.UserMessage{
			RoleField: agentcore.RoleUser,
			Content:   agentcore.ContentList{agentcore.NewTextContent(text)},
			Timestamp: time.Now().UnixMilli(),
		},
	}
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

type taskTool struct {
	session *Session
}

func newTaskTool(session *Session) *taskTool {
	return &taskTool{session: session}
}

func (t *taskTool) Name() string { return TaskToolName }

func (t *taskTool) Description() string {
	definitions := t.session.subagentDefinitionsSnapshot()
	if len(definitions) == 0 {
		return "No subagents are configured; do the work directly."
	}
	lines := []string{
		"Start one subagent in the background and return immediately; you keep working while it runs, then converge with TaskWait when you need its report.",
		"Use it when the work is separable: parallel exploration of independent directions (one Task per direction in the same assistant message), a multi-file implementation with a complete spec (fixer), an adversarial read-only review of a change you just made (code-reviewer), or a wide search / long log / multi-file survey whose intermediate output would otherwise fill this context (explorer, test-runner).",
		"Do not delegate what you can finish in a couple of tool calls, and do not delegate anything that needs the user — a subagent cannot ask a question or propose a plan on your behalf.",
		"The `task` brief is the delegate's only instruction. It cannot see this conversation, and you cannot correct it while it runs, so state the goal, the paths and facts it cannot infer, and exactly what to report back.",
		"To run delegates concurrently, emit several Task calls in one assistant message. The runtime delivers their reports when they finish; call TaskStop only to cancel.",
		"",
		"Available subagents:",
	}
	for _, definition := range definitions {
		entry := "- `" + definition.Name + "` — " + definition.Description
		if label := subagentToolsLabel(definition); label != "" {
			entry += " (tools: " + label + ")"
		}
		lines = append(lines, entry)
	}
	return strings.Join(lines, "\n")
}

func (t *taskTool) Schema() json.RawMessage {
	definitions := t.session.subagentDefinitionsSnapshot()
	names := make([]string, 0, len(definitions))
	for _, definition := range definitions {
		names = append(names, definition.Name)
	}
	parameters := map[string]any{
		"type": "object",
		"properties": map[string]any{
			"agent": map[string]any{
				"type":        "string",
				"enum":        names,
				"description": "Name of the subagent to run.",
			},
			"task": map[string]any{
				"type":        "string",
				"description": "The complete brief: goal, context the delegate cannot infer, and the exact report you want back.",
			},
			"description": map[string]any{
				"type":        "string",
				"description": "Short label for this delegation (3-6 words), shown to the user.",
			},
			"model": map[string]any{
				"type":        "string",
				"description": "Override the delegate's model for this run with a configured delegation model key. Omit to use the subagent's default.",
			},
		},
		"required":             []string{"agent", "task"},
		"additionalProperties": false,
	}
	encoded, err := json.Marshal(parameters)
	if err != nil {
		return json.RawMessage(`{"type":"object","properties":{}}`)
	}
	return encoded
}

func (t *taskTool) ExecutionMode() agentcore.ToolExecutionMode {
	return agentcore.ToolExecutionParallel
}

type taskArgs struct {
	Agent       string `json:"agent"`
	Task        string `json:"task"`
	Description string `json:"description,omitempty"`
	Model       string `json:"model,omitempty"`
}

func (t *taskTool) Execute(ctx context.Context, id string, args json.RawMessage, _ agentcore.ToolUpdateFunc) (agentcore.AgentToolResult, error) {
	var call taskArgs
	if len(args) > 0 {
		if err := json.Unmarshal(args, &call); err != nil {
			return agentcore.AgentToolResult{}, fmt.Errorf("Task: decode args: %w", err)
		}
	}
	call.Agent = strings.TrimSpace(call.Agent)
	call.Task = strings.TrimSpace(call.Task)
	call.Model = strings.TrimSpace(call.Model)
	if call.Task == "" {
		return t.session.subagentToolError(id, "Task needs a non-empty `task` brief."), nil
	}
	definition, ok := t.session.subagentDefinition(call.Agent)
	if !ok {
		return t.session.subagentToolError(id, "Unknown subagent \""+call.Agent+"\". Available: "+strings.Join(t.session.subagentNames(), ", ")+"."), nil
	}

	if running := len(t.session.runningDelegations()); running >= MaxSubagentConcurrency {
		return t.session.subagentToolError(id, fmt.Sprintf("%d subagents are already running for this session. Wait for some with TaskWait or stop them with TaskStop before delegating more.", MaxSubagentConcurrency)), nil
	}

	childErrors := newSubagentToolErrors()
	childTools := t.session.subagentTools(definition, childErrors)
	if len(childTools) == 0 {
		return t.session.subagentToolError(id, "The "+definition.Name+" subagent declares no tool available in this session."), nil
	}
	runCfg, modelID, thinking, err := t.session.subagentRunConfig(ctx, definition, call.Model, childTools, childErrors)
	if err != nil {
		return t.session.subagentToolError(id, "The "+definition.Name+" subagent cannot run: "+err.Error()), nil
	}
	if t.session.subagentSem != nil {
		select {
		case t.session.subagentSem <- struct{}{}:
		default:
			return t.session.subagentToolError(id, fmt.Sprintf("%d subagents are already running for this session. Wait for some with TaskWait or stop them with TaskStop before delegating more.", MaxSubagentConcurrency)), nil
		}
	}

	startedAt := time.Now().UnixMilli()
	turnID := t.session.CurrentTurnID()
	delegationID := newID()
	childCtx, cancel := context.WithCancel(context.Background())
	record := &delegationRecord{
		id:            delegationID,
		agentName:     definition.Name,
		modelID:       modelID,
		thinkingLevel: string(thinking),
		status:        delegationRunning,
		startedAt:     startedAt,
		lastPhase:     "thinking",
		startedEpoch:  t.session.currentTurnEpoch(),
		completion:    make(chan struct{}),
		abort:         cancel,
		taskCallID:    id,
		taskTurnID:    turnID,
		taskArgs:      args,
	}
	t.session.registerDelegation(record)

	go func() {
		defer func() {
			if recovered := recover(); recovered != nil {
				logf(t.session.logger, "subagent %s panicked: %v", definition.Name, recovered)
				t.session.settleDelegation(record, subagentOutcome{
					status:     "failed",
					errCode:    "UNEXPECTED_DELEGATION_PANIC",
					errMessage: fmt.Sprint(recovered),
				})
				cancel()
			}
			if t.session.subagentSem != nil {
				<-t.session.subagentSem
			}
		}()
		t.session.runDelegation(childCtx, record, definition, call.Task, runCfg)
		cancel()
	}()

	label := strings.TrimSpace(call.Description)
	text := "Delegation " + delegationID + " started: the " + definition.Name + " subagent is working in the background"
	if label != "" {
		text += " (" + label + ")"
	}
	text += ". Continue your own independent work, then call TaskWait with this delegationId to converge, or TaskStop to stop it."
	return agentcore.AgentToolResult{
		Content: agentcore.ContentList{agentcore.NewTextContent(text)},
		Details: map[string]any{
			"delegationId":  delegationID,
			"agent":         definition.Name,
			"status":        delegationRunning,
			"startedAt":     startedAt,
			"modelId":       modelID,
			"thinkingLevel": string(thinking),
		},
	}, nil
}

// subagentOutcome is one settled delegate run.
type subagentOutcome struct {
	status       string
	report       string
	errCode      string
	errMessage   string
	turns        int
	toolCalls    int
	lastToolName string
}

// runDelegation drives one child agent loop and settles its record.
func (s *Session) runDelegation(ctx context.Context, record *delegationRecord, definition protocol.SubagentDefinition, prompt string, runCfg pigoruntime.RunConfig) {
	childCtx := &agentcore.AgentContext{
		SystemPrompt: definition.Prompt,
		Messages: agentcore.MessageList{
			agentcore.UserMessage{
				RoleField: agentcore.RoleUser,
				Content:   agentcore.ContentList{agentcore.NewTextContent(prompt)},
				Timestamp: time.Now().UnixMilli(),
			},
		},
		Tools: runCfg.Batch.ToolExecutorConfig.Registry.List(),
	}
	turns := 0
	maxReached := false
	runCfg.ShouldStopAfterTurn = func(_ context.Context, agentCtx *agentcore.AgentContext) bool {
		turns++
		record.mu.Lock()
		record.turns = turns
		record.lastPhase = "thinking"
		record.lastToolName = ""
		record.mu.Unlock()
		s.refreshDelegationWait()
		maxTurns := 0
		if definition.MaxTurns != nil && *definition.MaxTurns > 0 {
			maxTurns = *definition.MaxTurns
		}
		if maxTurns <= 0 || turns < maxTurns {
			return false
		}
		last := agentcore.LastAssistantOf(agentCtx.Messages)
		if last != nil && (len(last.ToolCalls()) > 0 || last.StopReason == agentcore.StopReasonLength) {
			maxReached = true
			return true
		}
		return false
	}
	var lastTool string
	var toolCalls int
	// Delegate rows reach the renderer through the parent turn's event stream,
	// stamped with the Task call that spawned them, so the Task card can show
	// its subagent's own steps live and after a reload.
	childEvents := newSubagentEventMapper(
		s,
		firstNonEmpty(record.taskTurnID, s.CurrentTurnID()),
		record.taskCallID,
		definition.Name,
	)
	handler := pigoruntime.StreamHandler{
		OnEvent: func(event agentcore.AgentEvent) {
			childEvents.handle(event)
			switch typed := event.(type) {
			case agentcore.TurnStartEvent:
				record.noteActivity("thinking", "")
				s.refreshDelegationWait()
			case agentcore.TurnEndEvent:
				s.addSubagentUsage(typed.Message.Usage)
			case agentcore.ToolExecutionStartEvent:
				lastTool = typed.ToolName
				toolCalls++
				record.mu.Lock()
				record.toolCalls = toolCalls
				record.lastToolName = lastTool
				record.lastPhase = ""
				record.mu.Unlock()
				s.refreshDelegationWait()
			}
		},
	}
	stream := pigoruntime.StartRun(ctx, childCtx, runCfg)
	final, runErr := pigoruntime.DrainStream(ctx, stream, handler)

	outcome := subagentOutcome{status: "completed", turns: turns, toolCalls: toolCalls, lastToolName: lastTool}
	if final != nil {
		outcome.report = agentcore.ContentToText(final.Content)
		if outcome.report == "" {
			outcome.report = final.ErrorMessage
		}
		switch final.StopReason {
		case agentcore.StopReasonError:
			outcome.status = "failed"
			outcome.errCode = "SUBAGENT_FAILED"
			outcome.errMessage = firstNonEmpty(final.ErrorMessage, outcome.report, "subagent stopped on an error")
		case agentcore.StopReasonAborted:
			outcome.status = "aborted"
			outcome.errCode = "SUBAGENT_ABORTED"
			outcome.errMessage = firstNonEmpty(final.ErrorMessage, "subagent was aborted")
		}
	}
	if runErr != nil {
		if errors.Is(runErr, context.Canceled) {
			outcome.status = "aborted"
			outcome.errCode = "SUBAGENT_ABORTED"
			outcome.errMessage = "subagent was aborted"
		} else {
			outcome.status = "failed"
			outcome.errCode = "SUBAGENT_FAILED"
			outcome.errMessage = runErr.Error()
		}
		if outcome.report == "" {
			outcome.report = runErr.Error()
		}
	} else if ctx.Err() != nil && outcome.status == "completed" {
		// The loop can observe cancellation between a settled turn and the next
		// request; a canceled run is not a report worth delivering.
		outcome.status = "aborted"
		outcome.errCode = "SUBAGENT_ABORTED"
		outcome.errMessage = "subagent was aborted"
	}
	if maxReached && outcome.status == "completed" {
		outcome.status = "truncated"
		outcome.errCode = "SUBAGENT_TRUNCATED"
		maxTurns := 0
		if definition.MaxTurns != nil {
			maxTurns = *definition.MaxTurns
		}
		note := fmt.Sprintf("(subagent %q reached its maxTurns cap of %d)", definition.Name, maxTurns)
		if outcome.report == "" {
			outcome.report = note
		} else {
			outcome.report += "\n\n" + note
		}
	}
	if outcome.report == "" {
		outcome.report = fmt.Sprintf("(subagent %q produced no text output)", definition.Name)
	}
	s.settleDelegation(record, outcome)
}

// ---------------------------------------------------------------------------
// Lifecycle tools
// ---------------------------------------------------------------------------

type taskWaitTool struct{ session *Session }

func newTaskWaitTool(session *Session) *taskWaitTool { return &taskWaitTool{session: session} }
func (t *taskWaitTool) Name() string                 { return TaskWaitToolName }
func (t *taskWaitTool) Description() string {
	return "Wait for one or more subagents started by Task and return their reports. `delegationIds` defaults to every running subagent; use mode \"any\" with `minCompleted` to converge as soon as the first (or first N) finish. Settled delegations return immediately, so re-reading a report by id is cheap. A wait timeout is not a failure: unfinished delegates keep working and the runtime delivers their reports when they finish."
}
func (t *taskWaitTool) Schema() json.RawMessage {
	parameters := map[string]any{
		"type": "object",
		"properties": map[string]any{
			"delegationIds": map[string]any{
				"type":        "array",
				"items":       map[string]any{"type": "string"},
				"description": "Delegation ids returned by Task. Defaults to all running subagents.",
			},
			"mode": map[string]any{
				"type":        "string",
				"enum":        []any{"all", "any"},
				"description": "Wait for every target (all) or the first to finish (any).",
			},
			"minCompleted": map[string]any{
				"type":        "number",
				"minimum":     1,
				"description": "With mode \"any\": wait until at least this many finished.",
			},
			"timeoutSeconds": map[string]any{
				"type":        "number",
				"minimum":     1,
				"maximum":     taskWaitMaxTimeoutSeconds,
				"description": fmt.Sprintf("Max seconds to wait; defaults to %d.", taskWaitDefaultTimeoutSeconds),
			},
		},
		"additionalProperties": false,
	}
	encoded, err := json.Marshal(parameters)
	if err != nil {
		return json.RawMessage(`{"type":"object","properties":{}}`)
	}
	return encoded
}
func (t *taskWaitTool) ExecutionMode() agentcore.ToolExecutionMode {
	return agentcore.ToolExecutionSequential
}

type taskWaitArgs struct {
	DelegationIDs  []string `json:"delegationIds,omitempty"`
	Mode           string   `json:"mode,omitempty"`
	MinCompleted   int      `json:"minCompleted,omitempty"`
	TimeoutSeconds int      `json:"timeoutSeconds,omitempty"`
}

func (t *taskWaitTool) Execute(ctx context.Context, _ string, args json.RawMessage, _ agentcore.ToolUpdateFunc) (agentcore.AgentToolResult, error) {
	var params taskWaitArgs
	if len(args) > 0 {
		if err := json.Unmarshal(args, &params); err != nil {
			return agentcore.AgentToolResult{}, fmt.Errorf("TaskWait: decode args: %w", err)
		}
	}
	mode := "all"
	if strings.EqualFold(strings.TrimSpace(params.Mode), "any") {
		mode = "any"
	}
	minCompleted := params.MinCompleted
	if minCompleted < 1 {
		minCompleted = 1
	}
	timeout := params.TimeoutSeconds
	if timeout <= 0 {
		timeout = taskWaitDefaultTimeoutSeconds
	}
	if timeout > taskWaitMaxTimeoutSeconds {
		timeout = taskWaitMaxTimeoutSeconds
	}

	session := t.session
	var targets []*delegationRecord
	unknown := make([]string, 0)
	if len(params.DelegationIDs) > 0 {
		for _, id := range params.DelegationIDs {
			id = strings.TrimSpace(id)
			if id == "" {
				continue
			}
			if record := session.delegation(id); record != nil {
				targets = append(targets, record)
			} else {
				unknown = append(unknown, id)
			}
		}
	} else {
		targets = session.runningDelegations()
	}
	if len(targets) == 0 {
		text := "No subagents are currently running."
		if len(params.DelegationIDs) > 0 {
			text = "None of the requested delegation ids exist in this session. Call TaskList to see them."
		}
		return agentcore.AgentToolResult{
			Content: agentcore.ContentList{agentcore.NewTextContent(text)},
			Details: map[string]any{"delegations": []any{}},
		}, nil
	}

	required := len(targets)
	if mode == "any" {
		required = minCompleted
		if required > len(targets) {
			required = len(targets)
		}
	}
	deadline := time.Now().Add(time.Duration(timeout) * time.Second)
	session.beginDelegationWait(targets)
	interrupted := session.waitForDelegations(ctx, targets, required, deadline, nil)
	session.endDelegationWait()
	if ctx.Err() != nil {
		return agentcore.AgentToolResult{}, ctx.Err()
	}

	now := time.Now().UnixMilli()
	results := make([]delegationResult, 0, len(targets))
	entries := make([]map[string]any, 0, len(targets))
	finished := 0
	heartbeats := make([]string, 0, len(targets))
	for _, target := range targets {
		snapshot := target.snapshot()
		report := snapshot.report
		if snapshot.status == delegationRunning {
			report = formatDelegationHeartbeat(snapshot, now)
			heartbeats = append(heartbeats, report)
		} else {
			finished++
			if report == "" {
				report = "(" + snapshot.status + " without a report)"
			}
		}
		entry := delegationSummary(snapshot)
		entry["report"] = report
		entries = append(entries, entry)
		results = append(results, delegationResult{
			id: snapshot.id, agent: snapshot.agentName, status: snapshot.status, report: report,
		})
	}

	note := ""
	if interrupted {
		note = fmt.Sprintf("Still running after %ds: %d/%d finished. This is not a failure — unfinished delegates keep working and the runtime will deliver their reports when they finish. Call TaskStop only to cancel.", timeout, finished, len(targets))
		if len(heartbeats) > 0 {
			note += "\n" + strings.Join(heartbeats, "\n")
		}
	} else if mode == "any" {
		note = fmt.Sprintf("Converged after %d of %d finished.", finished, len(targets))
	}
	if len(unknown) > 0 {
		unknownNote := "Unknown delegation ids (not found in this session): " + strings.Join(unknown, ", ") + "."
		if note == "" {
			note = unknownNote
		} else {
			note += "\n" + unknownNote
		}
	}

	formatted, included := formatDelegationResults(results, note)
	for _, target := range targets {
		snapshot := target.snapshot()
		if snapshot.status == delegationRunning || !included[snapshot.id] {
			continue
		}
		target.mu.Lock()
		target.reportDelivered = true
		target.mu.Unlock()
	}

	details := map[string]any{"delegations": entries}
	if interrupted {
		details["status"] = "timeout"
	} else {
		details["status"] = "completed"
	}
	if len(unknown) > 0 {
		details["unknownIds"] = unknown
	}
	return agentcore.AgentToolResult{
		Content: agentcore.ContentList{agentcore.NewTextContent(formatted)},
		Details: details,
	}, nil
}

type taskListTool struct{ session *Session }

func newTaskListTool(session *Session) *taskListTool { return &taskListTool{session: session} }
func (t *taskListTool) Name() string                 { return TaskListToolName }
func (t *taskListTool) Description() string {
	return "List the subagents started by Task in this session with their status. Use it to check progress without waiting, or before TaskStop to choose what to stop."
}
func (t *taskListTool) Schema() json.RawMessage {
	return json.RawMessage(`{"type":"object","properties":{},"additionalProperties":false}`)
}
func (t *taskListTool) ExecutionMode() agentcore.ToolExecutionMode {
	return agentcore.ToolExecutionSequential
}
func (t *taskListTool) Execute(_ context.Context, _ string, _ json.RawMessage, _ agentcore.ToolUpdateFunc) (agentcore.AgentToolResult, error) {
	records := t.session.delegationList()
	now := time.Now().UnixMilli()
	text := "No subagents have been started in this session."
	summaries := make([]map[string]any, 0, len(records))
	if len(records) > 0 {
		lines := make([]string, 0, len(records))
		for _, record := range records {
			snapshot := record.snapshot()
			lines = append(lines, "- "+formatDelegationHeartbeat(snapshot, now))
			summaries = append(summaries, delegationSummary(snapshot))
		}
		text = strings.Join(lines, "\n")
	}
	return agentcore.AgentToolResult{
		Content: agentcore.ContentList{agentcore.NewTextContent(text)},
		Details: map[string]any{"delegations": summaries},
	}, nil
}

type taskStopArgs struct {
	DelegationIDs []string `json:"delegationIds,omitempty"`
}

type taskStopTool struct{ session *Session }

func newTaskStopTool(session *Session) *taskStopTool { return &taskStopTool{session: session} }
func (t *taskStopTool) Name() string                 { return TaskStopToolName }
func (t *taskStopTool) Description() string {
	return "Stop one or more running subagents. `delegationIds` defaults to every running subagent. Stopped subagents report as stopped; their partial work is lost."
}
func (t *taskStopTool) Schema() json.RawMessage {
	parameters := map[string]any{
		"type": "object",
		"properties": map[string]any{
			"delegationIds": map[string]any{
				"type":        "array",
				"items":       map[string]any{"type": "string"},
				"description": "Delegation ids returned by Task. Defaults to all running subagents.",
			},
		},
		"additionalProperties": false,
	}
	encoded, err := json.Marshal(parameters)
	if err != nil {
		return json.RawMessage(`{"type":"object","properties":{}}`)
	}
	return encoded
}
func (t *taskStopTool) ExecutionMode() agentcore.ToolExecutionMode {
	return agentcore.ToolExecutionSequential
}

func (t *taskStopTool) Execute(_ context.Context, _ string, args json.RawMessage, _ agentcore.ToolUpdateFunc) (agentcore.AgentToolResult, error) {
	var params taskStopArgs
	if len(args) > 0 {
		if err := json.Unmarshal(args, &params); err != nil {
			return agentcore.AgentToolResult{}, fmt.Errorf("TaskStop: decode args: %w", err)
		}
	}
	ids := make([]string, 0, len(params.DelegationIDs))
	for _, id := range params.DelegationIDs {
		if id = strings.TrimSpace(id); id != "" {
			ids = append(ids, id)
		}
	}
	var targets []*delegationRecord
	if len(ids) > 0 {
		for _, id := range ids {
			if record := t.session.delegation(id); record != nil {
				targets = append(targets, record)
			}
		}
	} else {
		targets = t.session.runningDelegations()
	}
	for _, target := range targets {
		target.mu.Lock()
		target.stopRequested = true
		target.mu.Unlock()
		if target.abort != nil {
			target.abort()
		}
	}
	for _, target := range targets {
		<-target.completion
	}

	text := "No matching running subagents to stop."
	if len(targets) > 0 {
		suffix := "s"
		if len(targets) == 1 {
			suffix = ""
		}
		text = fmt.Sprintf("Stopped %d subagent%s.", len(targets), suffix)
	}
	summaries := make([]map[string]any, 0, len(targets))
	for _, target := range targets {
		summaries = append(summaries, delegationSummary(target.snapshot()))
	}
	return agentcore.AgentToolResult{
		Content: agentcore.ContentList{agentcore.NewTextContent(text)},
		Details: map[string]any{"stopped": summaries},
	}, nil
}

// ---------------------------------------------------------------------------
// Definition / tool / provider resolution
// ---------------------------------------------------------------------------

func (s *Session) subagentDefinitionsSnapshot() []protocol.SubagentDefinition {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := make([]protocol.SubagentDefinition, len(s.subagents))
	copy(out, s.subagents)
	return out
}

func (s *Session) subagentNames() []string {
	definitions := s.subagentDefinitionsSnapshot()
	names := make([]string, 0, len(definitions))
	for _, definition := range definitions {
		names = append(names, definition.Name)
	}
	return names
}

func (s *Session) subagentDefinition(name string) (protocol.SubagentDefinition, bool) {
	for _, definition := range s.subagentDefinitionsSnapshot() {
		if definition.Name == name {
			return definition, true
		}
	}
	return protocol.SubagentDefinition{}, false
}

// subagentToolError marks one immediate Task-family failure and keeps the
// explanation readable in the result the model receives.
func (s *Session) subagentToolError(id, text string) agentcore.AgentToolResult {
	s.markToolError(id)
	return agentcore.AgentToolResult{
		Content: agentcore.ContentList{agentcore.NewTextContent(text)},
		Details: map[string]any{"error": text},
	}
}

// subagentTools resolves a definition's tool set and builds the child tools:
// host tools come from the catalog fetched at construction, BrowserPreview is a
// session-local surface with the delegate's error set, and the inherited Skill
// tool is reused from the parent's live set.
func (s *Session) subagentTools(definition protocol.SubagentDefinition, childErrors *subagentToolErrors) []agentcore.AgentTool {
	resolved := resolveSubagentToolNames(definition, s.liveToolNames())
	tools := make([]agentcore.AgentTool, 0, len(resolved))
	for _, name := range resolved {
		if host := hostToolByName(s, s.hostCatalog, name, definition.Permission); host != nil {
			host.errors = childErrors
			tools = append(tools, host)
			continue
		}
		switch name {
		case SkillToolName:
			if skill, ok := s.liveTools[name]; ok {
				tools = append(tools, skill)
			}
		case BrowserPreviewToolName:
			tools = append(tools, &browserPreviewTool{session: s, errors: childErrors})
		}
	}
	return tools
}

// liveToolNames returns the parent's registered tool names. The Task family is
// added after this map is built, so a delegate can never nest one.
func (s *Session) liveToolNames() []string {
	names := make([]string, 0, len(s.liveTools))
	for name := range s.liveTools {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}

// resolveSubagentToolNames mirrors the shared contract: declared tools only,
// unless `tools: inherit` unions in the parent's live catalog minus the deny
// list.
func resolveSubagentToolNames(definition protocol.SubagentDefinition, parentToolNames []string) []string {
	declared := make([]string, 0, len(definition.Tools))
	for _, name := range definition.Tools {
		if name != "inherit" {
			declared = append(declared, name)
		}
	}
	if !definition.InheritTools {
		return declared
	}
	resolved := make([]string, 0, len(parentToolNames)+len(declared))
	seen := map[string]bool{}
	for _, name := range append(append([]string{}, parentToolNames...), declared...) {
		if subagentInheritDeny[name] || seen[name] {
			continue
		}
		seen[name] = true
		resolved = append(resolved, name)
	}
	return resolved
}

func subagentToolsLabel(definition protocol.SubagentDefinition) string {
	if definition.InheritTools {
		if len(definition.Tools) > 0 {
			return "inherit + " + strings.Join(definition.Tools, ", ")
		}
		return "inherit"
	}
	if len(definition.Tools) == 0 {
		return "none"
	}
	return strings.Join(definition.Tools, ", ")
}

// subagentRunConfig builds the child loop configuration. A pinned provider is
// rebuilt from the shell-resolved config; a definition without a pin inherits
// the session provider, key resolution and headers. `override` may name another
// configured delegation model key.
func (s *Session) subagentRunConfig(
	ctx context.Context,
	definition protocol.SubagentDefinition,
	override string,
	childTools []agentcore.AgentTool,
	childErrors *subagentToolErrors,
) (pigoruntime.RunConfig, string, agentcore.ThinkingLevel, error) {
	s.mu.Lock()
	thinking := s.thinking
	parent := s.runCfg.LoopConfig
	providerSupport := s.params.Provider.SupportedThinkingLevels
	providers := s.subagentProviders
	mainKey := s.params.Provider.ID + "/" + s.params.Provider.ModelID
	s.mu.Unlock()

	var pinned *protocol.ProviderConfig
	modelID := parent.Model
	if definition.Model != nil || override != "" {
		key := override
		if key == "" {
			key = definition.Model.ProviderID + "/" + definition.Model.ModelID
		}
		config, ok := providers[key]
		switch {
		case ok:
			pinned = &config
		case key == mainKey:
			// The parent model is a valid default: inherit it.
		case definition.Model != nil && key == definition.Model.ProviderID+"/"+definition.Model.ModelID:
			return pigoruntime.RunConfig{}, "", "", fmt.Errorf(
				"model pin %q is not configured for this session; add the provider or drop `model` from the definition", key,
			)
		default:
			available := make([]string, 0, len(providers))
			for candidate := range providers {
				available = append(available, candidate)
			}
			sort.Strings(available)
			hint := " No delegation model overrides are configured."
			if len(available) > 0 {
				hint = " Available: " + strings.Join(available, ", ") + "."
			}
			return pigoruntime.RunConfig{}, "", "", fmt.Errorf("model %q is not available for delegation.%s", key, hint)
		}
	}

	loop := parent
	loop.Stream = s.baseStreamFn
	loop.SummaryStream = nil
	loop.Extra = copyStringMap(parent.Extra)
	loop.GetAPIKey = parent.GetAPIKey
	if definition.MaxTokens != nil && *definition.MaxTokens > 0 {
		loop.Extra["max_tokens"] = *definition.MaxTokens
	}

	level := thinking
	if pinned != nil {
		config := *pinned
		prov, model, err := buildProvider(config)
		if err != nil {
			return pigoruntime.RunConfig{}, "", "", fmt.Errorf("model pin %q: %w", config.ID+"/"+config.ModelID, err)
		}
		apiKey := strings.TrimSpace(config.APIKey)
		if apiKey == "" && config.AuthKind == "none" {
			apiKey = "pi-desktop-no-auth"
		}
		loop.Stream = provider.StreamFnFromProvider(prov)
		loop.Model = config.ModelID
		loop.Provider = config.ID
		loop.APIKey = apiKey
		loop.ContextWindow = model.ContextWindow
		loop.Compaction = compactionSettingsForWindow(model.ContextWindow)
		loop.Headers = config.Headers
		loop.GetAPIKey = nil
		if config.AuthKind == "oauth" {
			// Vendor tokens expire in about an hour; resolve one per request and
			// never let the refresh token reach this process.
			loop.GetAPIKey = func(callCtx context.Context, _ string) string {
				token, resolveErr := resolveOAuthKey(callCtx, s.host, s.sessionID, config.ID)
				if resolveErr != nil {
					logf(s.logger, "subagent %s: provider auth refresh failed: %v", definition.Name, resolveErr)
					return ""
				}
				return token
			}
			if apiKey == "" {
				token, resolveErr := resolveOAuthKey(ctx, s.host, s.sessionID, config.ID)
				if resolveErr != nil {
					return pigoruntime.RunConfig{}, "", "", fmt.Errorf("model pin %q: %w", config.ID+"/"+config.ModelID, resolveErr)
				}
				loop.APIKey = token
			}
		}
		modelID = config.ModelID
		level = clampSubagentThinking(definition.ThinkingLevel, config.SupportedThinkingLevels, thinking)
	} else {
		level = clampSubagentThinking(definition.ThinkingLevel, providerSupport, thinking)
	}
	loop.ThinkingLevel = level

	registry := agenttool.NewToolRegistry()
	for _, tool := range childTools {
		if host, ok := tool.(*hostTool); ok {
			host.errors = childErrors
		}
		if err := registry.Register(tool); err != nil {
			logf(s.logger, "subagent %s tool %q has an unusable schema, dropping it: %v", definition.Name, tool.Name(), err)
		}
	}
	cfg := pigoruntime.RunConfig{
		LoopConfig: loop,
		Batch: agenttool.BatchConfig{
			ToolExecutorConfig: agenttool.ToolExecutorConfig{
				Registry:       registry,
				BeforeToolCall: s.beforeToolCall,
				AfterToolCall: func(_ context.Context, call agentcore.AgentToolCall, _ agentcore.AgentToolResult, _ bool) *agentcore.AfterToolCallResult {
					if !childErrors.consume(call.ID) {
						return nil
					}
					failed := true
					return &agentcore.AfterToolCallResult{IsError: &failed}
				},
			},
		},
		EventBuffer: 64,
	}
	return cfg, modelID, level, nil
}

func clampSubagentThinking(declared string, supported []string, fallback agentcore.ThinkingLevel) agentcore.ThinkingLevel {
	candidate := strings.ToLower(strings.TrimSpace(declared))
	if candidate == "" || candidate == "omit" {
		return fallback
	}
	level := agentcore.ThinkingLevel(candidate)
	if len(supported) == 0 {
		return level
	}
	for _, item := range supported {
		if strings.EqualFold(item, candidate) {
			return level
		}
	}
	return fallback
}

func copyStringMap(source map[string]any) map[string]any {
	if len(source) == 0 {
		return map[string]any{}
	}
	out := make(map[string]any, len(source))
	for key, value := range source {
		out[key] = value
	}
	return out
}
