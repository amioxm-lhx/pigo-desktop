package evals

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// BaselineSchemaVersion is the version of the baseline record format written
// by RecordBaseline and read by CompareToBaseline.
const BaselineSchemaVersion = 1

// BaselineRecord is one persisted baseline row: the scored outcome of one
// task under one repetition, as recorded at baseline time. Stored rows are
// relabeled onto the "@baseline" harness name at comparison time so they pair
// with the fresh candidate rows instead of colliding with them.
type BaselineRecord struct {
	SchemaVersion int     `json:"schemaVersion"`
	EvalSet       string  `json:"evalSet"`
	TaskID        string  `json:"taskId"`
	Harness       string  `json:"harness"`
	Repetition    int     `json:"repetition"`
	Outcome       string  `json:"outcome"`
	Score         float64 `json:"score"`
	TotalTokens   float64 `json:"totalTokens,omitempty"`
	TotalMs       float64 `json:"totalMs,omitempty"`
}

// BaselinePath returns the on-disk location of the baseline for evalSet
// beneath dir: <dir>/<evalSet>.baseline.jsonl. evalSet becomes a file name,
// so it must be a plain base name; validateEvalSetName enforces that before
// any write or read reaches disk.
func BaselinePath(dir, evalSet string) string {
	return filepath.Join(dir, evalSet+".baseline.jsonl")
}

// validateEvalSetName rejects an eval set name that cannot be used as one
// path segment (separators, ".", ".."), so a hostile task-set file cannot
// steer baseline writes outside the evals directory.
func validateEvalSetName(evalSet string) error {
	if strings.TrimSpace(evalSet) == "" {
		return fmt.Errorf("eval set name must not be empty")
	}
	if evalSet == "." || evalSet == ".." || strings.ContainsAny(evalSet, `/\\`) {
		return fmt.Errorf("invalid eval set name %q", evalSet)
	}
	return nil
}

// RecordBaseline runs every task in set through h once per repetition and
// writes the scored rows to the baseline file beneath dir, replacing any
// previous baseline (a baseline is a point-in-time snapshot, not a log). It
// also appends the full run records to runs.jsonl for audit.
func RecordBaseline(ctx context.Context, h *AgentHarness, set TaskSet, dir string, repetitions int) ([]BaselineRecord, error) {
	if repetitions < 1 {
		repetitions = 1
	}
	if err := validateEvalSetName(set.EvalSet); err != nil {
		return nil, err
	}
	records := make([]BaselineRecord, 0, len(set.Tasks)*repetitions)
	for repetition := 1; repetition <= repetitions; repetition++ {
		outcomes, err := RunTasks(ctx, h, set)
		if err != nil {
			return nil, err
		}
		for _, task := range set.Tasks {
			record := BaselineRecord{
				SchemaVersion: BaselineSchemaVersion,
				EvalSet:       set.EvalSet,
				TaskID:        task.ID,
				Harness:       h.Name(),
				Repetition:    repetition,
				Outcome:       OutcomeScored,
			}
			run, ok := outcomes[task.ID]
			if !ok || len(run.Errors) > 0 {
				record.Outcome = OutcomeErrored
			}
			record.Score = run.Output.Score
			if run.Usage != nil && run.Usage.TotalTokens != nil {
				record.TotalTokens = *run.Usage.TotalTokens
			}
			if run.Timings != nil && run.Timings.TotalMs != nil {
				record.TotalMs = *run.Timings.TotalMs
			}
			records = append(records, record)
		}
	}

	if err := os.MkdirAll(dir, 0o755); err != nil {
		return nil, err
	}
	encoded := make([]byte, 0, len(records)*128)
	for _, record := range records {
		line, err := json.Marshal(record)
		if err != nil {
			return nil, err
		}
		encoded = append(encoded, line...)
		encoded = append(encoded, '\n')
	}
	if err := os.WriteFile(BaselinePath(dir, set.EvalSet), encoded, 0o644); err != nil {
		return nil, err
	}

	// Audit trail: one runs.jsonl row per recorded task run.
	for _, record := range records {
		if err := AppendHarnessRunReport(dir, HarnessRunRecord{
			RunID:    "baseline-" + set.EvalSet,
			Test:     map[string]any{"taskId": record.TaskID, "repetition": record.Repetition},
			Harness:  record.Harness,
			Usage:    map[string]any{"totalTokens": record.TotalTokens},
			Timings:  map[string]any{"totalMs": record.TotalMs},
			Metadata: map[string]any{"evalSet": set.EvalSet, "outcome": record.Outcome, "score": record.Score},
		}); err != nil {
			return nil, err
		}
	}
	return records, nil
}

// LoadBaseline reads the baseline rows for evalSet beneath dir. An absent or
// unreadable baseline is an error: the caller must know the gate cannot run
// rather than silently comparing against nothing.
func LoadBaseline(dir, evalSet string) ([]BaselineRecord, error) {
	file, err := os.Open(BaselinePath(dir, evalSet))
	if err != nil {
		return nil, fmt.Errorf("load baseline for %q: %w", evalSet, err)
	}
	defer file.Close()

	var records []BaselineRecord
	scanner := bufio.NewScanner(file)
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	line := 0
	for scanner.Scan() {
		line++
		text := scanner.Text()
		if text == "" {
			continue
		}
		var record BaselineRecord
		if err := json.Unmarshal([]byte(text), &record); err != nil {
			return nil, fmt.Errorf("baseline %q line %d: %w", evalSet, line, err)
		}
		if record.SchemaVersion != BaselineSchemaVersion {
			return nil, fmt.Errorf("baseline %q line %d: unsupported schema version %d", evalSet, line, record.SchemaVersion)
		}
		records = append(records, record)
	}
	if err := scanner.Err(); err != nil {
		return nil, err
	}
	if len(records) == 0 {
		return nil, fmt.Errorf("baseline %q is empty", evalSet)
	}
	return records, nil
}

// BaselineObservations relabels stored baseline rows onto the baseline-side
// harness name so they pair with fresh candidate rows in
// SummarizeHarnessComparisons. The stored rows keep their task ids and
// repetitions, which is what pairs them with the candidate rows.
func BaselineObservations(records []BaselineRecord, file string) []HarnessObservation {
	observations := make([]HarnessObservation, 0, len(records))
	for _, record := range records {
		groupKey, _ := DeriveEvalGroupKey(map[string]any{"id": record.TaskID}, record.Repetition)
		harness := record.Harness
		candidate := harness
		baseline := BaselineHarnessName(harness)
		observation := HarnessObservation{
			EvalSet:    record.EvalSet,
			GroupKey:   groupKey,
			TestName:   record.TaskID,
			File:       file,
			Harness:    baseline,
			Baseline:   baseline,
			Candidates: []string{candidate},
			Repetition: record.Repetition,
			Outcome:    record.Outcome,
		}
		if record.Outcome == OutcomeScored {
			score := record.Score
			observation.Score = &score
			if record.TotalTokens > 0 {
				tokens := record.TotalTokens
				observation.TotalTokens = &tokens
			}
			if record.TotalMs > 0 {
				ms := record.TotalMs
				observation.TotalMs = &ms
			}
		}
		observations = append(observations, observation)
	}
	return observations
}

// GateResult is the regression gate's decision: the comparison report plus
// whether the candidate regressed.
type GateResult struct {
	Report    HarnessComparisonReport
	Regressed bool
	// Reason explains a regression or an unrunnable gate (no eligible pairs).
	Reason string
}

// EvaluateGate inspects a comparison report and decides whether the candidate
// regressed. The gate is conservative in both directions: a strictly lower
// candidate pass rate regresses, and so does a comparison with no eligible
// pairs at all (missing baseline rows, all-errored runs) — an unmeasurable
// comparison must fail closed, not pass silently.
func EvaluateGate(report HarnessComparisonReport) GateResult {
	result := GateResult{Report: report}
	eligibleTotal := 0
	for _, evalSet := range report.EvalSets {
		for _, comparison := range evalSet.Comparisons {
			eligibleTotal += comparison.Correctness.EligiblePairs
			if comparison.Correctness.EligiblePairs == 0 {
				continue
			}
			if comparison.Correctness.Lift == nil {
				continue
			}
			if *comparison.Correctness.Lift < 0 {
				result.Regressed = true
				result.Reason = fmt.Sprintf("eval set %q: pass rate dropped from %s to %s",
					evalSet.EvalSet,
					formatPercentage(comparison.Correctness.BaselinePassRate),
					formatPercentage(comparison.Correctness.CandidatePassRate))
				return result
			}
		}
	}
	if eligibleTotal == 0 {
		result.Regressed = true
		result.Reason = "no eligible pairs: baseline and candidate observations did not pair up (missing rows or all runs errored)"
	}
	return result
}

// CompareToBaseline runs set through h once per repetition, pairs the fresh
// rows with the stored baseline rows beneath dir, and evaluates the
// regression gate. It is the `--evals --evals-compare` implementation: the
// caller maps Regressed to a non-zero exit code.
func CompareToBaseline(ctx context.Context, h *AgentHarness, set TaskSet, dir string, repetitions int) (GateResult, error) {
	if repetitions < 1 {
		repetitions = 1
	}
	if err := validateEvalSetName(set.EvalSet); err != nil {
		return GateResult{}, err
	}
	baselineRecords, err := LoadBaseline(dir, set.EvalSet)
	if err != nil {
		return GateResult{}, err
	}

	file := set.EvalSet + ".tasks.json"
	stored := BaselineObservations(baselineRecords, file)
	fresh := make([]HarnessObservation, 0, len(set.Tasks)*repetitions)
	for repetition := 1; repetition <= repetitions; repetition++ {
		outcomes, err := RunTasks(ctx, h, set)
		if err != nil {
			return GateResult{}, err
		}
		fresh = append(fresh, ObservationsFromRuns(set.EvalSet, file, set.Tasks, outcomes, h.Name(), BaselineHarnessName(h.Name()), repetition)...)
	}

	observations := make([]HarnessObservation, 0, len(stored)+len(fresh))
	observations = append(observations, stored...)
	observations = append(observations, fresh...)
	report := SummarizeHarnessComparisons(observations)
	gate := EvaluateGate(report)
	return gate, nil
}
