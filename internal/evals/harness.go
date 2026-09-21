package evals

import (
	"context"
	"fmt"
)

// HarnessRun is one completed eval harness execution: the produced output,
// the usage and timing recorded by the reporter, and any non-fatal errors.
type HarnessRun[TOutput any] struct {
	Output    TOutput
	Usage     *HarnessUsage
	Timings   *HarnessTimings
	Errors    []error
	Artifacts map[string]any
}

// HarnessUsage is the subset of usage recorded by the reporter. Every field is
// optional (nil = not measured) so a harness that cannot see token counts
// still participates in correctness comparisons.
type HarnessUsage struct {
	TotalTokens      *float64
	TotalMs          *float64
	EstimatedCostUsd *float64
	Metadata         map[string]any
}

// HarnessTimings records one optional timing field.
type HarnessTimings struct {
	TotalMs *float64
}

// RunnerHarness is a runnable eval harness: Harness (Name only) extended with
// a generic run function. Run must honor ctx cancellation.
type RunnerHarness[TInput any, TOutput any] interface {
	Harness
	Run(ctx context.Context, input TInput) (HarnessRun[TOutput], error)
}

// RunEvalHarnessTable executes every row of an expanded harness table and
// returns observations suitable for SummarizeHarnessComparisons. testName
// identifies the test (the input being run) and is carried onto every
// observation — the harness label rides the Harness field — so baseline and
// candidate rows of the same input land in the same group and pair up.
// scoreOf extracts the optional correctness score from a run's output (nil =
// unscored); the repetition is carried onto the observation's group key so
// paired comparison groups repeated runs correctly. It stops early
// (returning the error) when ctx is cancelled.
func RunEvalHarnessTable[TInput any, TOutput any](ctx context.Context, evalSet, testName string, baseline RunnerHarness[TInput, TOutput], candidates []RunnerHarness[TInput, TOutput], repetitions int, input TInput, scoreOf func(TOutput) *float64) ([]HarnessObservation, error) {
	tableCandidates := make([]Harness, 0, len(candidates))
	runners := make([]RunnerHarness[TInput, TOutput], 0, len(candidates)+1)
	runners = append(runners, baseline)
	for _, candidate := range candidates {
		tableCandidates = append(tableCandidates, candidate)
		runners = append(runners, candidate)
	}
	rows, err := EvalHarnessTable(evalSet, baseline, tableCandidates, repetitions)
	if err != nil {
		return nil, err
	}

	observations := make([]HarnessObservation, 0, len(rows))
	for _, row := range rows {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		var runner RunnerHarness[TInput, TOutput]
		for _, candidateRunner := range runners {
			if candidateRunner.Name() == row.Name {
				runner = candidateRunner
				break
			}
		}
		if runner == nil {
			return nil, fmt.Errorf("unknown harness %s", row.Name)
		}
		run, runErr := runner.Run(ctx, input)
		outcome := OutcomeScored
		if runErr != nil || len(run.Errors) > 0 {
			outcome = OutcomeErrored
		}
		observation := HarnessObservation{
			EvalSet:    evalSet,
			GroupKey:   fmt.Sprintf("[\"input\",%d]", row.Repetition),
			TestName:   testName,
			File:       evalSet + ".eval.go",
			Harness:    row.Name,
			Baseline:   baseline.Name(),
			Candidates: tableCandidateNames(tableCandidates),
			Repetition: row.Repetition,
			Outcome:    outcome,
		}
		if run.Usage != nil {
			observation.TotalTokens = run.Usage.TotalTokens
			observation.TotalMs = run.Usage.TotalMs
			observation.EstimatedCostUsd = run.Usage.EstimatedCostUsd
		}
		if run.Timings != nil {
			observation.TotalMs = run.Timings.TotalMs
		}
		if scoreOf != nil {
			observation.Score = scoreOf(run.Output)
		}
		observations = append(observations, observation)
	}
	return observations, nil
}

func tableCandidateNames(candidates []Harness) []string {
	out := make([]string, len(candidates))
	for index, candidate := range candidates {
		out[index] = candidate.Name()
	}
	return out
}
