package evals

import (
	"context"
	"encoding/json"
	"path/filepath"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
)

// scriptedTurn is one scripted assistant turn: the stream events replayed
// for a single StreamFn call.
type scriptedTurn []provider.AssistantMessageEvent

// textTurn scripts a turn that streams text as start → text delta →
// done(end_turn), optionally carrying token usage for accounting tests.
func textTurn(text string, usage *agentcore.Usage) scriptedTurn {
	partial := agentcore.AssistantMessage{RoleField: agentcore.RoleAssistant}
	withText := partial
	withText.Content = agentcore.ContentList{agentcore.NewTextContent(text)}
	final := withText
	final.StopReason = agentcore.StopReasonEndTurn
	final.Usage = usage
	return scriptedTurn{
		provider.StreamStartEvent{Partial: partial},
		provider.StreamTextEvent{Partial: withText},
		provider.StreamDoneEvent{Message: final},
	}
}

// toolCallTurn scripts a turn that requests one tool call.
func toolCallTurn(id, name, args string) scriptedTurn {
	partial := agentcore.AssistantMessage{RoleField: agentcore.RoleAssistant}
	withCall := partial
	withCall.Content = agentcore.ContentList{agentcore.NewToolCallContent(id, name, json.RawMessage(args))}
	final := withCall
	final.StopReason = agentcore.StopReasonToolUse
	return scriptedTurn{
		provider.StreamStartEvent{Partial: partial},
		provider.StreamToolCallEvent{Partial: withCall},
		provider.StreamDoneEvent{Message: final},
	}
}

// scriptedStream replays turns in order, one per StreamFn call, recording the
// shaped context it was handed. Once exhausted it replays an empty end_turn,
// mirroring the faux provider's behavior so a runaway loop settles.
func scriptedStream(t *testing.T, turns ...scriptedTurn) provider.StreamFn {
	t.Helper()
	var calls atomic.Int32
	var mu sync.Mutex
	contexts := []provider.LlmContext{}
	return func(ctx context.Context, model string, llm provider.LlmContext, cfg provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		mu.Lock()
		contexts = append(contexts, llm)
		mu.Unlock()
		index := int(calls.Add(1)) - 1
		turn := textTurn("", nil)
		if index < len(turns) {
			turn = turns[index]
		}
		stream := provider.NewAssistantMessageEventStream(0)
		go func() {
			for _, ev := range turn {
				if err := stream.Emit(ctx, ev); err != nil {
					stream.SetError(err)
					stream.Close()
					return
				}
			}
			stream.Close()
		}()
		return stream, nil
	}
}

// constantStream replays the same turn on every StreamFn call, unlike
// scriptedStream which consumes its script once — use it when a harness runs
// the same input repeatedly (repetitions > 1) and every run must behave
// identically.
func constantStream(t *testing.T, turn scriptedTurn) provider.StreamFn {
	t.Helper()
	return func(ctx context.Context, model string, llm provider.LlmContext, cfg provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		stream := provider.NewAssistantMessageEventStream(0)
		go func() {
			for _, ev := range turn {
				if err := stream.Emit(ctx, ev); err != nil {
					stream.SetError(err)
					stream.Close()
					return
				}
			}
			stream.Close()
		}()
		return stream, nil
	}
}

// testSet builds a one-task task set.
func testSet(evalSet string, tasks ...Task) TaskSet {
	return TaskSet{EvalSet: evalSet, Tasks: tasks}
}

func TestAgentHarnessWriteTask(t *testing.T) {
	stream := scriptedStream(t,
		toolCallTurn("call-1", "write", `{"path":"hello.txt","content":"hello world\n"}`),
		textTurn("Created hello.txt.", &agentcore.Usage{InputTokens: 10, OutputTokens: 5}),
	)
	h := NewAgentHarness(AgentHarnessConfig{Name: "pigo", Model: "test-model", Stream: stream, Timeout: 30 * time.Second})
	set := testSet("s", Task{
		ID:     "write-hello",
		Prompt: "create hello.txt",
		Expect: Expectation{
			Exists:   []string{"hello.txt"},
			Contains: map[string][]string{"hello.txt": {"hello world"}},
		},
	})

	outcomes, err := RunTasks(context.Background(), h, set)
	if err != nil {
		t.Fatalf("RunTasks: %v", err)
	}
	run := outcomes["write-hello"]
	if len(run.Errors) > 0 {
		t.Fatalf("unexpected errors: %v", run.Errors)
	}
	if run.Output.Score != 1 {
		t.Fatalf("score = %v, failed: %v", run.Output.Score, run.Output.FailedCheck)
	}
	if run.Usage == nil || run.Usage.TotalTokens == nil || *run.Usage.TotalTokens != 15 {
		t.Fatalf("usage = %+v, want 15 tokens", run.Usage)
	}
	if run.Timings == nil || run.Timings.TotalMs == nil || *run.Timings.TotalMs < 0 {
		t.Fatalf("timings = %+v", run.Timings)
	}
}

func TestAgentHarnessReadThenAnswer(t *testing.T) {
	stream := scriptedStream(t,
		toolCallTurn("call-1", "read", `{"path":"notes.txt"}`),
		textTurn("PI-7731", nil),
	)
	h := NewAgentHarness(AgentHarnessConfig{Name: "pigo", Model: "test-model", Stream: stream, Timeout: 30 * time.Second})
	set := testSet("s", Task{
		ID:     "read-single",
		Prompt: "read notes.txt",
		Files:  map[string]string{"notes.txt": "The launch code is PI-7731.\n"},
		Expect: Expectation{AnswerContains: []string{"PI-7731"}},
	})

	outcomes, err := RunTasks(context.Background(), h, set)
	if err != nil {
		t.Fatalf("RunTasks: %v", err)
	}
	if run := outcomes["read-single"]; run.Output.Score != 1 {
		t.Fatalf("score = %v, failed: %v", run.Output.Score, run.Output.FailedCheck)
	}
}

func TestAgentHarnessBashTask(t *testing.T) {
	stream := scriptedStream(t,
		toolCallTurn("call-1", "bash", `{"command":"echo pigo-smoke-ok > output.txt"}`),
		textTurn("Done.", nil),
	)
	h := NewAgentHarness(AgentHarnessConfig{Name: "pigo", Model: "test-model", Stream: stream, Timeout: 30 * time.Second})
	set := testSet("s", Task{
		ID:     "bash-write-output",
		Prompt: "write output.txt via bash",
		Expect: Expectation{Contains: map[string][]string{"output.txt": {"pigo-smoke-ok"}}},
	})

	outcomes, err := RunTasks(context.Background(), h, set)
	if err != nil {
		t.Fatalf("RunTasks: %v", err)
	}
	if run := outcomes["bash-write-output"]; run.Output.Score != 1 {
		t.Fatalf("score = %v, failed: %v", run.Output.Score, run.Output.FailedCheck)
	}
}

func TestAgentHarnessFailingExpectation(t *testing.T) {
	// The script never creates the file, so the expectation must fail while
	// the run itself stays error-free (a scored zero, not a harness error).
	stream := scriptedStream(t, textTurn("I did nothing.", nil))
	h := NewAgentHarness(AgentHarnessConfig{Name: "pigo", Model: "test-model", Stream: stream, Timeout: 30 * time.Second})
	set := testSet("s", Task{
		ID:     "write-hello",
		Prompt: "create hello.txt",
		Expect: Expectation{Exists: []string{"hello.txt"}},
	})

	outcomes, err := RunTasks(context.Background(), h, set)
	if err != nil {
		t.Fatalf("RunTasks: %v", err)
	}
	run := outcomes["write-hello"]
	if len(run.Errors) != 0 {
		t.Fatalf("errors = %v, want none", run.Errors)
	}
	if run.Output.Score != 0 {
		t.Fatalf("score = %v, want 0", run.Output.Score)
	}
	if FormatFailedChecks(run.Output) == "" {
		t.Fatal("FormatFailedChecks should describe the failure")
	}
}

func TestRecordAndCompareBaseline(t *testing.T) {
	dir := t.TempDir()
	set := testSet("smoke",
		Task{
			ID:     "write-hello",
			Prompt: "create hello.txt",
			Expect: Expectation{Exists: []string{"hello.txt"}},
		},
		Task{
			ID:     "write-bye",
			Prompt: "create bye.txt",
			Expect: Expectation{Exists: []string{"bye.txt"}},
		},
	)

	// Baseline harness does the work correctly for both tasks.
	good := NewAgentHarness(AgentHarnessConfig{
		Name:    "pigo",
		Model:   "test-model",
		Stream:  taskAwareStream(t, map[string]string{"write-hello": "hello.txt", "write-bye": "bye.txt"}),
		Timeout: 30 * time.Second,
	})
	records, err := RecordBaseline(context.Background(), good, set, dir, 1)
	if err != nil {
		t.Fatalf("RecordBaseline: %v", err)
	}
	if len(records) != 2 {
		t.Fatalf("records = %d, want 2", len(records))
	}
	if _, err := LoadBaseline(dir, "smoke"); err != nil {
		t.Fatalf("baseline was not persisted: %v", err)
	}

	// Same-good harness: the gate must pass.
	sameGood := NewAgentHarness(AgentHarnessConfig{
		Name:    "pigo",
		Model:   "test-model",
		Stream:  taskAwareStream(t, map[string]string{"write-hello": "hello.txt", "write-bye": "bye.txt"}),
		Timeout: 30 * time.Second,
	})
	gate, err := CompareToBaseline(context.Background(), sameGood, set, dir, 1)
	if err != nil {
		t.Fatalf("CompareToBaseline: %v", err)
	}
	if gate.Regressed {
		t.Fatalf("gate regressed with identical behavior: %s", gate.Reason)
	}

	// Bad harness only completes one task: pass rate must drop and the gate
	// must fail closed.
	bad := NewAgentHarness(AgentHarnessConfig{
		Name:    "pigo",
		Model:   "test-model",
		Stream:  taskAwareStream(t, map[string]string{"write-hello": "hello.txt"}),
		Timeout: 30 * time.Second,
	})
	gate, err = CompareToBaseline(context.Background(), bad, set, dir, 1)
	if err != nil {
		t.Fatalf("CompareToBaseline: %v", err)
	}
	if !gate.Regressed {
		t.Fatal("gate should regress on a 1/2 vs 2/2 pass rate")
	}
	report := FormatHarnessComparisonReport(gate.Report)
	if report == "" {
		t.Fatal("comparison report should not be empty")
	}
}

// taskAwareStream scripts one write-tool call per task, creating exactly the
// files in wanted. A task whose file is absent gets an empty answer turn —
// the scripted equivalent of a model that fails to act. Once a tool result
// has come back (the last shaped message is a tool result) the stream settles
// with a final text turn, so the loop terminates instead of re-issuing calls.
func taskAwareStream(t *testing.T, wanted map[string]string) provider.StreamFn {
	t.Helper()
	return func(ctx context.Context, model string, llm provider.LlmContext, cfg provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		settled := false
		if len(llm.Messages) > 0 {
			if _, ok := llm.Messages[len(llm.Messages)-1].(agentcore.ToolResultMessage); ok {
				settled = true
			}
		}
		create := ""
		if !settled {
			for i := len(llm.Messages) - 1; i >= 0; i-- {
				if user, ok := llm.Messages[i].(agentcore.UserMessage); ok {
					prompt := agentcore.ContentToText(user.Content)
					for _, target := range wanted {
						if containsFold(prompt, filepath.Base(target)) {
							create = target
						}
					}
					break
				}
			}
		}
		stream := provider.NewAssistantMessageEventStream(0)
		go func() {
			var turn scriptedTurn
			if !settled && create != "" {
				args, _ := json.Marshal(map[string]string{"path": create, "content": "content\n"})
				turn = toolCallTurn("call-1", "write", string(args))
			} else {
				turn = textTurn("Done.", nil)
			}
			for _, ev := range turn {
				if err := stream.Emit(ctx, ev); err != nil {
					stream.SetError(err)
					stream.Close()
					return
				}
			}
			stream.Close()
		}()
		return stream, nil
	}
}

func containsFold(haystack, needle string) bool {
	return indexFold(haystack, needle) >= 0
}

func indexFold(haystack, needle string) int {
	for i := 0; i+len(needle) <= len(haystack); i++ {
		if equalFold(haystack[i:i+len(needle)], needle) {
			return i
		}
	}
	return -1
}

func equalFold(a, b string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := 0; i < len(a); i++ {
		if lowerByte(a[i]) != lowerByte(b[i]) {
			return false
		}
	}
	return true
}

func lowerByte(b byte) byte {
	if b >= 'A' && b <= 'Z' {
		return b + ('a' - 'A')
	}
	return b
}

func TestRunEvalHarnessTable(t *testing.T) {
	good := constantStream(t, textTurn("ok", nil))
	bad := constantStream(t, textTurn("", nil))
	baseline := NewAgentHarness(AgentHarnessConfig{Name: "baseline", Model: "m", Stream: good, Timeout: 10 * time.Second})
	candidate := NewAgentHarness(AgentHarnessConfig{Name: "candidate", Model: "m", Stream: bad, Timeout: 10 * time.Second})

	task := Task{ID: "t", Prompt: "p", Expect: Expectation{AnswerContains: []string{"ok"}}}
	taskScore := func(o TaskOutcome) *float64 { return &o.Score }
	observations, err := RunEvalHarnessTable[Task, TaskOutcome](context.Background(), "s", "t", baseline, []RunnerHarness[Task, TaskOutcome]{candidate}, 2, task, taskScore)
	if err != nil {
		t.Fatalf("RunEvalHarnessTable: %v", err)
	}
	if len(observations) != 4 { // 2 harnesses x 2 repetitions
		t.Fatalf("observations = %d, want 4", len(observations))
	}
	report := SummarizeHarnessComparisons(observations)
	if len(report.EvalSets) != 1 || len(report.EvalSets[0].Comparisons) != 1 {
		t.Fatalf("report = %+v", report.EvalSets)
	}
	comparison := report.EvalSets[0].Comparisons[0]
	if comparison.Baseline != "baseline" || comparison.Candidate != "candidate" {
		t.Fatalf("comparison roles = %s vs %s", comparison.Baseline, comparison.Candidate)
	}
	if comparison.Correctness.EligiblePairs != 2 {
		t.Fatalf("eligible pairs = %d, want 2", comparison.Correctness.EligiblePairs)
	}
	if comparison.Correctness.BaselinePassRate == nil || *comparison.Correctness.BaselinePassRate != 1 {
		t.Fatalf("baseline pass rate = %v", comparison.Correctness.BaselinePassRate)
	}
	if comparison.Correctness.CandidatePassRate == nil || *comparison.Correctness.CandidatePassRate != 0 {
		t.Fatalf("candidate pass rate = %v", comparison.Correctness.CandidatePassRate)
	}
	gate := EvaluateGate(report)
	if !gate.Regressed {
		t.Fatal("gate should regress: candidate pass rate 0 vs baseline 1")
	}
	if FormatHarnessComparisonReport(report) == "" {
		t.Fatal("formatted report should not be empty")
	}
}

func TestEvaluateGateFailsClosed(t *testing.T) {
	// No eligible pairs at all: the gate must fail closed rather than pass
	// silently on an unmeasurable comparison.
	report := SummarizeHarnessComparisons(nil)
	gate := EvaluateGate(report)
	if !gate.Regressed {
		t.Fatal("empty comparison should regress (fail closed)")
	}
	if gate.Reason == "" {
		t.Fatal("reason should explain the failure")
	}
}

func TestHarnessTableValidation(t *testing.T) {
	newHarness := func(name string) Harness { return namedHarness{name} }
	if _, err := EvalHarnessTable("", newHarness("a"), []Harness{newHarness("b")}, 1); err == nil {
		t.Error("empty evalSet should error")
	}
	if _, err := EvalHarnessTable("s", newHarness("a"), nil, 1); err == nil {
		t.Error("no candidates should error")
	}
	if _, err := EvalHarnessTable("s", newHarness("a"), []Harness{newHarness("a")}, 1); err == nil {
		t.Error("duplicate harness names should error")
	}
	rows, err := EvalHarnessTable("s", newHarness("a"), []Harness{newHarness("b")}, 0)
	if err != nil {
		t.Fatalf("zero repetitions should default to one: %v", err)
	}
	if len(rows) != 2 {
		t.Fatalf("rows = %d, want 2", len(rows))
	}
}

// namedHarness is the minimal Harness implementation for table tests.
type namedHarness struct{ name string }

func (h namedHarness) Name() string { return h.name }

func TestObservationsFromRuns(t *testing.T) {
	set := testSet("s", Task{ID: "t1", Prompt: "p"})
	outcomes := map[string]HarnessRun[TaskOutcome]{
		"t1": {Output: TaskOutcome{TaskID: "t1", Score: 1}, Timings: &HarnessTimings{TotalMs: floatPtr(12.5)}},
	}
	observations := ObservationsFromRuns("s", "s.tasks.json", set.Tasks, outcomes, "pigo", "pigo@baseline", 1)
	if len(observations) != 1 {
		t.Fatalf("observations = %d", len(observations))
	}
	observation := observations[0]
	if observation.Harness != "pigo" || observation.Baseline != "pigo@baseline" {
		t.Fatalf("roles = %s / %s", observation.Harness, observation.Baseline)
	}
	if observation.TotalMs == nil || *observation.TotalMs != 12.5 {
		t.Fatalf("totalMs = %v", observation.TotalMs)
	}

	// A missing outcome maps to an errored row.
	observations = ObservationsFromRuns("s", "s.tasks.json", set.Tasks, nil, "pigo", "pigo@baseline", 1)
	if observations[0].Outcome != OutcomeErrored {
		t.Fatalf("outcome = %s, want errored", observations[0].Outcome)
	}
}

func floatPtr(v float64) *float64 { return &v }
