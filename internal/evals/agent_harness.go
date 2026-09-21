package evals

import (
	"context"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/agenttool"
	"github.com/smallnest/pigo/internal/provider"
	"github.com/smallnest/pigo/internal/runtime"
)

// DefaultTaskTimeout bounds one task run. A task that exceeds it counts as
// errored (not scored zero), so latency blowups are visible as harness errors
// rather than silent failures.
const DefaultTaskTimeout = 4 * time.Minute

// AgentHarnessConfig configures an AgentHarness.
type AgentHarnessConfig struct {
	// Name is the harness name carried onto observations. Two harnesses in
	// one comparison must differ in name.
	Name string
	// Model is the model id passed to the loop's LoopConfig.
	Model string
	// ProviderName is the provider label passed to LoopConfig (informational
	// for the run; the stream itself is what talks to the backend).
	ProviderName string
	// Stream drives the loop. In production it comes from
	// provider.StreamFnFromProvider(prov); tests inject a scripted stream.
	Stream provider.StreamFn
	// GetAPIKey optionally resolves a fresh key per request. Nil uses no key.
	GetAPIKey func(ctx context.Context, provider string) string
	// Timeout bounds one task run. Zero uses DefaultTaskTimeout.
	Timeout time.Duration
	// KeepWorkspaces, when true, leaves the per-task temp workspaces in place
	// for debugging instead of removing them at run end.
	KeepWorkspaces bool
}

// AgentHarness is the pigo agent harness: it drives a full agent loop (tools
// included) against one task in an isolated temporary workspace and scores
// the outcome against the task's expectations. It implements
// RunnerHarness[Task, TaskOutcome].
type AgentHarness struct {
	cfg AgentHarnessConfig
}

// NewAgentHarness builds an agent harness from cfg.
func NewAgentHarness(cfg AgentHarnessConfig) *AgentHarness {
	if cfg.Timeout <= 0 {
		cfg.Timeout = DefaultTaskTimeout
	}
	if strings.TrimSpace(cfg.Name) == "" {
		cfg.Name = "pigo"
	}
	return &AgentHarness{cfg: cfg}
}

// Name implements Harness.
func (h *AgentHarness) Name() string { return h.cfg.Name }

// Run implements RunnerHarness: it seeds the task workspace, assembles a
// minimal tool set and system prompt rooted there, runs the loop to
// completion, and scores the result. The harness deliberately assembles its
// own tool set (read/write/edit/grep/find/bash/todo) rather than reusing the
// run package's: evals must stay deterministic, so the non-deterministic
// builtins (web fetch/search, plugins, memory, skills, sub-agents) are out.
func (h *AgentHarness) Run(ctx context.Context, task Task) (HarnessRun[TaskOutcome], error) {
	if strings.TrimSpace(task.ID) == "" {
		return HarnessRun[TaskOutcome]{}, fmt.Errorf("eval task has an empty id")
	}

	root, err := os.MkdirTemp("", "pigo-evals-"+task.ID+"-")
	if err != nil {
		return HarnessRun[TaskOutcome]{}, fmt.Errorf("workspace: %w", err)
	}
	if !h.cfg.KeepWorkspaces {
		defer os.RemoveAll(root)
	}
	if err := SeedWorkspace(root, task.Files); err != nil {
		return HarnessRun[TaskOutcome]{}, err
	}

	agentCtx, cfg := h.assemble(root, task)

	runCtx, cancel := context.WithTimeout(ctx, h.cfg.Timeout)
	defer cancel()

	started := time.Now()
	run := h.drive(runCtx, agentCtx, cfg, root, task)
	elapsed := time.Since(started).Seconds() * 1000
	run.Timings = &HarnessTimings{TotalMs: &elapsed}
	return run, nil
}

// assemble builds the AgentContext and RunConfig for one task workspace. The
// system prompt is built per task (BuildSystemPrompt embeds the working
// directory) so each isolated workspace gets a correct environment block.
func (h *AgentHarness) assemble(root string, task Task) (*agentcore.AgentContext, runtime.RunConfig) {
	// One snapshot recorder backs write/edit; one job store backs the bash
	// family; one todo store — mirroring the production wiring so tool
	// behavior under eval matches a real run.
	snap := agenttool.NewFileSnapshotRecorder()
	jobs := agenttool.NewBashJobStore()
	tools := []agentcore.AgentTool{
		&agenttool.ReadTool{Root: root},
		&agenttool.WriteTool{Root: root, Snap: snap},
		&agenttool.EditTool{Root: root, Snap: snap},
		&agenttool.GrepTool{Root: root},
		&agenttool.FindTool{Root: root},
		&agenttool.BashTool{Dir: root, Jobs: jobs},
		&agenttool.TodoTool{Store: agenttool.NewTodoStore()},
	}
	registry := agenttool.NewToolRegistry()
	for _, t := range tools {
		_ = registry.Register(t)
	}

	sysPrompt, err := runtime.BuildSystemPrompt(runtime.PromptConfig{
		WorkingDir:        root,
		Root:              root,
		ReadToolAvailable: true,
	})
	if err != nil {
		// BuildSystemPrompt only fails on environment-block assembly; treat a
		// failure as an empty prompt rather than aborting the whole eval run.
		sysPrompt = ""
	}

	agentCtx := &agentcore.AgentContext{
		SystemPrompt: sysPrompt,
		Messages: []agentcore.AgentMessage{agentcore.UserMessage{
			RoleField: agentcore.RoleUser,
			Content:   agentcore.ContentList{agentcore.NewTextContent(task.Prompt)},
		}},
		Tools: tools,
	}

	cfg := runtime.RunConfig{
		LoopConfig: runtime.LoopConfig{
			Model:     h.cfg.Model,
			Provider:  h.cfg.ProviderName,
			Stream:    h.cfg.Stream,
			GetAPIKey: h.cfg.GetAPIKey,
		},
		Batch: agenttool.BatchConfig{ToolExecutorConfig: agenttool.ToolExecutorConfig{Registry: registry}},
	}
	return agentCtx, cfg
}

// drive runs the loop to completion, collects usage and the final answer from
// the event stream, and scores the workspace. Loop errors are recorded on the
// run (Errors) rather than returned, so a failed run still produces a scored
// observation — the comparison must see the failure, not lose the row.
func (h *AgentHarness) drive(ctx context.Context, agentCtx *agentcore.AgentContext, cfg runtime.RunConfig, root string, task Task) HarnessRun[TaskOutcome] {
	var run HarnessRun[TaskOutcome]

	var answer string
	totalTokens := 0.0
	hasTokens := false
	h2 := runtime.StreamHandler{
		OnEvent: func(ev agentcore.AgentEvent) {
			switch typed := ev.(type) {
			case agentcore.TurnEndEvent:
				answer = agentcore.ContentToText(typed.Message.Content)
				if typed.Message.Usage != nil {
					totalTokens += float64(typed.Message.Usage.InputTokens + typed.Message.Usage.OutputTokens)
					hasTokens = true
				}
			case agentcore.TelemetryEvent:
				// The run summary; nothing to accumulate here — usage rides
				// the per-turn messages above.
			}
		},
	}

	stream := runtime.StartRun(ctx, agentCtx, cfg)
	lastAssistant, err := runtime.DrainStream(ctx, stream, h2)
	if err != nil {
		run.Errors = append(run.Errors, err)
	}
	if lastAssistant != nil && lastAssistant.StopReason == agentcore.StopReasonEndTurn {
		answer = agentcore.ContentToText(lastAssistant.Content)
	}

	if hasTokens {
		run.Usage = &HarnessUsage{TotalTokens: &totalTokens}
	}
	run.Output = ScoreTask(task, root, answer)
	return run
}

// BaselineHarnessName derives the harness name under which a recorded
// baseline participates in a comparison: the current run keeps its own name
// while the stored rows are relabeled with the @baseline suffix, so a
// baseline and its candidate never collide as one harness.
func BaselineHarnessName(harnessName string) string {
	return harnessName + "@baseline"
}

// ObservationsFromRuns maps one pass's task outcomes onto comparison
// observations. It exists so both the recorder and the comparer produce
// identically-shaped rows; harnessName labels the rows, and baselineName
// names the baseline side the summarizer will pair against. The group key
// comes from the task id and the repetition index, keeping rows from separate
// passes paired per (task, repetition).
func ObservationsFromRuns(evalSet, file string, tasks []Task, outcomes map[string]HarnessRun[TaskOutcome], harnessName, baselineName string, repetition int) []HarnessObservation {
	observations := make([]HarnessObservation, 0, len(tasks))
	for _, task := range tasks {
		groupKey, _ := DeriveEvalGroupKey(map[string]any{"id": task.ID}, repetition)
		observation := HarnessObservation{
			EvalSet:    evalSet,
			GroupKey:   groupKey,
			TestName:   task.ID,
			File:       file,
			Harness:    harnessName,
			Baseline:   baselineName,
			Candidates: []string{harnessName},
			Repetition: repetition,
			Outcome:    OutcomeScored,
		}
		run, ok := outcomes[task.ID]
		if !ok {
			observation.Outcome = OutcomeErrored
			observations = append(observations, observation)
			continue
		}
		if len(run.Errors) > 0 {
			observation.Outcome = OutcomeErrored
		}
		score := run.Output.Score
		observation.Score = &score
		if run.Usage != nil && run.Usage.TotalTokens != nil {
			observation.TotalTokens = run.Usage.TotalTokens
		}
		if run.Timings != nil && run.Timings.TotalMs != nil {
			observation.TotalMs = run.Timings.TotalMs
		}
		observations = append(observations, observation)
	}
	return observations
}

// RunTasks runs every task in the set once, mapping task id to its run. A
// hard failure to even start one task (workspace creation, seeding) is
// returned as an error; per-task loop failures stay on the run's Errors.
func RunTasks(ctx context.Context, h *AgentHarness, set TaskSet) (map[string]HarnessRun[TaskOutcome], error) {
	outcomes := make(map[string]HarnessRun[TaskOutcome], len(set.Tasks))
	for _, task := range set.Tasks {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		run, err := h.Run(ctx, task)
		if err != nil {
			return nil, fmt.Errorf("task %s: %w", task.ID, err)
		}
		outcomes[task.ID] = run
	}
	return outcomes, nil
}

// FormatFailedChecks renders a task outcome's failed checks as one
// semicolon-separated line, for CLI reporting. It returns the empty string
// when every check passed.
func FormatFailedChecks(outcome TaskOutcome) string {
	if len(outcome.FailedCheck) == 0 {
		return ""
	}
	return strings.Join(outcome.FailedCheck, "; ")
}
