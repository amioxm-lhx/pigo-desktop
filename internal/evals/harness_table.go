// Package evals provides pigo's evaluation harness (ported from pi-go's
// evals, itself the Go port of the pi eval helpers): deterministic harness
// tables, canonical input hashing, run-record persistence, comparison
// summarization, and the pigo agent harness that drives a full agent loop
// against a task set in an isolated workspace.
//
// The package is the regression gate for structural changes (the serve layer,
// session leases, the SQLite store): a baseline recorded with RecordBaseline
// before a change is compared with CompareToBaseline after it, so a refactor
// that silently degrades agent behavior fails loudly instead of passing.
package evals

import (
	"fmt"
	"strings"
)

// Harness names one runnable eval implementation. Callers supply their own
// run functions (RunnerHarness) and use the table expansion for deterministic
// row ordering and metadata.
type Harness interface {
	Name() string
}

// EvalHarnessTableRow is one generated run in an eval harness table.
type EvalHarnessTableRow struct {
	Harness    Harness
	Name       string
	Repetition int
	Iteration  EvalHarnessIterationArtifact
}

// EvalHarnessTable expands baseline and candidate harnesses into repeated
// rows. Rows are ordered by repetition, then baseline followed by candidates.
// A zero repetitions is treated as one. It errors on an empty eval set name,
// an empty candidate list, duplicate harness names, or a negative repetition
// count, so a misconfigured comparison fails before any work is run.
func EvalHarnessTable(evalSet string, baseline Harness, candidates []Harness, repetitions int) ([]EvalHarnessTableRow, error) {
	if repetitions == 0 {
		repetitions = 1
	}
	if err := validateEvalHarnessOptions(evalSet, baseline, candidates, repetitions); err != nil {
		return nil, err
	}

	harnesses := append([]Harness{baseline}, candidates...)
	rows := make([]EvalHarnessTableRow, 0, len(harnesses)*repetitions)
	candidateNames := make([]string, len(candidates))
	for index, candidate := range candidates {
		candidateNames[index] = candidate.Name()
	}
	for repetition := 1; repetition <= repetitions; repetition++ {
		for _, harness := range harnesses {
			rows = append(rows, EvalHarnessTableRow{
				Harness:    harness,
				Name:       harness.Name(),
				Repetition: repetition,
				Iteration: EvalHarnessIterationArtifact{
					SchemaVersion: 1,
					EvalSet:       evalSet,
					GroupKey:      "",
					Harness:       harness.Name(),
					Baseline:      baseline.Name(),
					Candidates:    append([]string(nil), candidateNames...),
					Repetition:    repetition,
				},
			})
		}
	}
	return rows, nil
}

func validateEvalHarnessOptions(evalSet string, baseline Harness, candidates []Harness, repetitions int) error {
	if strings.TrimSpace(evalSet) == "" {
		return fmt.Errorf("evalSet must not be empty")
	}
	if len(candidates) == 0 {
		return fmt.Errorf("at least one candidate harness is required")
	}
	seen := make(map[string]struct{}, len(candidates)+1)
	for _, harness := range append([]Harness{baseline}, candidates...) {
		name := harness.Name()
		if _, exists := seen[name]; exists {
			return fmt.Errorf("harness names must be unique within an eval set")
		}
		seen[name] = struct{}{}
	}
	if repetitions < 1 {
		return fmt.Errorf("repetitions must be a positive integer")
	}
	return nil
}
