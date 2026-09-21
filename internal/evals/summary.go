package evals

import (
	"encoding/json"
	"fmt"
	"sort"
	"strconv"
	"strings"
)

// Observation outcomes mirror the pi eval union.
const (
	OutcomeScored   = "scored"
	OutcomeUnscored = "unscored"
	OutcomeSkipped  = "skipped"
	OutcomePending  = "pending"
	OutcomeErrored  = "errored"
)

// HarnessObservation is one recorded harness run with optional usage and
// correctness metrics.
type HarnessObservation struct {
	EvalSet          string   `json:"evalSet"`
	GroupKey         string   `json:"groupKey"`
	TestName         string   `json:"testName"`
	File             string   `json:"file"`
	Harness          string   `json:"harness"`
	Baseline         string   `json:"baseline"`
	Candidates       []string `json:"candidates"`
	Repetition       int      `json:"repetition"`
	TotalTokens      *float64 `json:"totalTokens,omitempty"`
	TotalMs          *float64 `json:"totalMs,omitempty"`
	EstimatedCostUsd *float64 `json:"estimatedCostUsd,omitempty"`
	Outcome          string   `json:"outcome"`
	Score            *float64 `json:"score,omitempty"`
}

// PairedMetricSummary summarizes a metric across paired baseline/candidate
// observations.
type PairedMetricSummary struct {
	TotalPairs    int      `json:"totalPairs"`
	EligiblePairs int      `json:"eligiblePairs"`
	BaselineMean  *float64 `json:"baselineMean,omitempty"`
	CandidateMean *float64 `json:"candidateMean,omitempty"`
	MeanDelta     *float64 `json:"meanDelta,omitempty"`
}

// CorrectnessLiftSummary summarizes pass-rate differences.
type CorrectnessLiftSummary struct {
	TotalPairs        int      `json:"totalPairs"`
	EligiblePairs     int      `json:"eligiblePairs"`
	BaselinePassRate  *float64 `json:"baselinePassRate,omitempty"`
	CandidatePassRate *float64 `json:"candidatePassRate,omitempty"`
	Lift              *float64 `json:"lift,omitempty"`
	BaselineWins      int      `json:"baselineWins"`
	CandidateWins     int      `json:"candidateWins"`
	Ties              int      `json:"ties"`
}

// HarnessPairComparison compares one candidate against the baseline.
type HarnessPairComparison struct {
	Baseline         string                 `json:"baseline"`
	Candidate        string                 `json:"candidate"`
	Correctness      CorrectnessLiftSummary `json:"correctness"`
	TotalTokens      PairedMetricSummary    `json:"totalTokens"`
	TotalMs          PairedMetricSummary    `json:"totalMs"`
	EstimatedCostUsd PairedMetricSummary    `json:"estimatedCostUsd"`
}

// HarnessComparisonDiagnostic describes an incomplete observation group — a
// missing, duplicated, errored, or unscorable observation that otherwise
// silently shrinks the comparison.
type HarnessComparisonDiagnostic struct {
	EvalSet    string `json:"evalSet"`
	GroupKey   string `json:"groupKey"`
	TestName   string `json:"testName"`
	File       string `json:"file"`
	Repetition int    `json:"repetition"`
	Harness    string `json:"harness"`
	Reason     string `json:"reason"`
}

// HarnessEvalSetReport contains comparisons for one eval set.
type HarnessEvalSetReport struct {
	EvalSet     string                  `json:"evalSet"`
	Comparisons []HarnessPairComparison `json:"comparisons"`
}

// HarnessComparisonReport is the top-level comparison result.
type HarnessComparisonReport struct {
	SchemaVersion int                           `json:"schemaVersion"`
	EvalSets      []HarnessEvalSetReport        `json:"evalSets"`
	Diagnostics   []HarnessComparisonDiagnostic `json:"diagnostics"`
}

type harnessDescriptor struct {
	name  string
	index int
}

type observationGroup struct {
	evalSet               string
	groupKey              string
	testName              string
	file                  string
	repetition            int
	observationsByHarness map[string][]HarnessObservation
}

type evalSetData struct {
	baseline         harnessDescriptor
	candidatesByName map[string]harnessDescriptor
	groupsByKey      map[string]*observationGroup
}

type observationPair struct {
	baseline  HarnessObservation
	candidate HarnessObservation
}

func getOrCreateGroup(mapping map[string]*observationGroup, key string, create func() *observationGroup) *observationGroup {
	if group := mapping[key]; group != nil {
		return group
	}
	group := create()
	mapping[key] = group
	return group
}

func mean(values []float64) *float64 {
	if len(values) == 0 {
		return nil
	}
	total := 0.0
	for _, value := range values {
		total += value
	}
	result := total / float64(len(values))
	return &result
}

// preciseDifference subtracts through a 15-digit round-trip so the reported
// delta does not carry float noise (e.g. 0.30000000000000004).
func preciseDifference(left, right float64) *float64 {
	formatted := strconv.FormatFloat(left-right, 'g', 15, 64)
	result, err := strconv.ParseFloat(formatted, 64)
	if err != nil {
		result = left - right
	}
	return &result
}

func groupObservations(observations []HarnessObservation) map[string]*evalSetData {
	evalSets := make(map[string]*evalSetData)
	for _, observation := range observations {
		data := evalSets[observation.EvalSet]
		if data == nil {
			data = &evalSetData{
				baseline:         harnessDescriptor{name: observation.Baseline, index: 0},
				candidatesByName: make(map[string]harnessDescriptor),
				groupsByKey:      make(map[string]*observationGroup),
			}
			evalSets[observation.EvalSet] = data
		}

		for index, name := range observation.Candidates {
			existing, exists := data.candidatesByName[name]
			if !exists || index < existing.index {
				data.candidatesByName[name] = harnessDescriptor{name: name, index: index}
			}
		}

		groupKey, _ := json.Marshal([]any{observation.File, observation.TestName, observation.GroupKey})
		group := getOrCreateGroup(data.groupsByKey, string(groupKey), func() *observationGroup {
			return &observationGroup{
				evalSet:               observation.EvalSet,
				groupKey:              observation.GroupKey,
				testName:              observation.TestName,
				file:                  observation.File,
				repetition:            observation.Repetition,
				observationsByHarness: make(map[string][]HarnessObservation),
			}
		})
		group.observationsByHarness[observation.Harness] = append(group.observationsByHarness[observation.Harness], observation)
	}
	return evalSets
}

func orderedHarnesses(data *evalSetData) []harnessDescriptor {
	candidates := orderedCandidates(data)
	result := make([]harnessDescriptor, 0, len(candidates)+1)
	result = append(result, data.baseline)
	result = append(result, candidates...)
	return result
}

func orderedCandidates(data *evalSetData) []harnessDescriptor {
	result := make([]harnessDescriptor, 0, len(data.candidatesByName))
	for _, descriptor := range data.candidatesByName {
		result = append(result, descriptor)
	}
	sort.Slice(result, func(i, j int) bool {
		if result[i].index != result[j].index {
			return result[i].index < result[j].index
		}
		return result[i].name < result[j].name
	})
	return result
}

func orderedGroups(data *evalSetData) []*observationGroup {
	result := make([]*observationGroup, 0, len(data.groupsByKey))
	for _, group := range data.groupsByKey {
		result = append(result, group)
	}
	sort.Slice(result, func(i, j int) bool {
		if result[i].groupKey != result[j].groupKey {
			return result[i].groupKey < result[j].groupKey
		}
		return result[i].repetition < result[j].repetition
	})
	return result
}

func collectDiagnostics(harnesses []harnessDescriptor, groups []*observationGroup) []HarnessComparisonDiagnostic {
	var diagnostics []HarnessComparisonDiagnostic
	for _, group := range groups {
		for _, descriptor := range harnesses {
			observations := group.observationsByHarness[descriptor.name]
			reason := ""
			switch {
			case len(observations) == 0:
				reason = "missing-observation"
			case len(observations) > 1:
				reason = "duplicate-observation"
			case observations[0].Outcome == OutcomeErrored:
				reason = "harness-error"
			case observations[0].Outcome == OutcomeUnscored:
				reason = "missing-score"
			case observations[0].Outcome != OutcomeScored:
				reason = "unscorable-outcome"
			}
			if reason == "" {
				continue
			}
			diagnostics = append(diagnostics, HarnessComparisonDiagnostic{
				EvalSet:    group.evalSet,
				GroupKey:   group.groupKey,
				TestName:   group.testName,
				File:       group.file,
				Repetition: group.repetition,
				Harness:    descriptor.name,
				Reason:     reason,
			})
		}
	}
	return diagnostics
}

func pairObservations(groups []*observationGroup, baselineHarness, candidateHarness string) []observationPair {
	var pairs []observationPair
	for _, group := range groups {
		baselines := group.observationsByHarness[baselineHarness]
		candidates := group.observationsByHarness[candidateHarness]
		if len(baselines) == 1 && len(candidates) == 1 {
			pairs = append(pairs, observationPair{baseline: baselines[0], candidate: candidates[0]})
		}
	}
	return pairs
}

func summarizeMetric(pairs []observationPair, selectValue func(HarnessObservation) *float64, totalPairs int) PairedMetricSummary {
	var baselineValues []float64
	var candidateValues []float64
	for _, pair := range pairs {
		if pair.baseline.Outcome != OutcomeScored || pair.candidate.Outcome != OutcomeScored {
			continue
		}
		baselineValue := selectValue(pair.baseline)
		candidateValue := selectValue(pair.candidate)
		if baselineValue == nil || candidateValue == nil {
			continue
		}
		baselineValues = append(baselineValues, *baselineValue)
		candidateValues = append(candidateValues, *candidateValue)
	}

	baselineMean := mean(baselineValues)
	candidateMean := mean(candidateValues)
	return PairedMetricSummary{
		TotalPairs:    totalPairs,
		EligiblePairs: len(baselineValues),
		BaselineMean:  baselineMean,
		CandidateMean: candidateMean,
		MeanDelta:     metricDelta(baselineMean, candidateMean),
	}
}

func metricDelta(baselineMean, candidateMean *float64) *float64 {
	if baselineMean == nil || candidateMean == nil {
		return nil
	}
	return preciseDifference(*candidateMean, *baselineMean)
}

func summarizeCorrectness(pairs []observationPair, totalPairs int) CorrectnessLiftSummary {
	eligiblePairs := 0
	baselinePasses := 0
	candidatePasses := 0
	baselineWins := 0
	candidateWins := 0
	ties := 0
	for _, pair := range pairs {
		if pair.baseline.Outcome != OutcomeScored || pair.candidate.Outcome != OutcomeScored {
			continue
		}
		if pair.baseline.Score == nil || pair.candidate.Score == nil {
			continue
		}
		eligiblePairs++
		baselinePassed := *pair.baseline.Score >= 1
		candidatePassed := *pair.candidate.Score >= 1
		if baselinePassed {
			baselinePasses++
		}
		if candidatePassed {
			candidatePasses++
		}
		switch {
		case baselinePassed == candidatePassed:
			ties++
		case baselinePassed:
			baselineWins++
		default:
			candidateWins++
		}
	}

	var baselinePassRate *float64
	var candidatePassRate *float64
	if eligiblePairs > 0 {
		baselineValue := float64(baselinePasses) / float64(eligiblePairs)
		candidateValue := float64(candidatePasses) / float64(eligiblePairs)
		baselinePassRate = &baselineValue
		candidatePassRate = &candidateValue
	}
	return CorrectnessLiftSummary{
		TotalPairs:        totalPairs,
		EligiblePairs:     eligiblePairs,
		BaselinePassRate:  baselinePassRate,
		CandidatePassRate: candidatePassRate,
		Lift:              metricDelta(baselinePassRate, candidatePassRate),
		BaselineWins:      baselineWins,
		CandidateWins:     candidateWins,
		Ties:              ties,
	}
}

func compareHarnesses(baseline, candidate harnessDescriptor, groups []*observationGroup) HarnessPairComparison {
	pairs := pairObservations(groups, baseline.name, candidate.name)
	return HarnessPairComparison{
		Baseline:         baseline.name,
		Candidate:        candidate.name,
		Correctness:      summarizeCorrectness(pairs, len(groups)),
		TotalTokens:      summarizeMetric(pairs, func(o HarnessObservation) *float64 { return o.TotalTokens }, len(groups)),
		TotalMs:          summarizeMetric(pairs, func(o HarnessObservation) *float64 { return o.TotalMs }, len(groups)),
		EstimatedCostUsd: summarizeMetric(pairs, func(o HarnessObservation) *float64 { return o.EstimatedCostUsd }, len(groups)),
	}
}

// SummarizeHarnessComparisons groups observations and produces deterministic
// per-eval-set comparisons plus diagnostics. Grouping is by (file, testName,
// groupKey) so repeated runs of the same task form one pair per repetition.
func SummarizeHarnessComparisons(observations []HarnessObservation) HarnessComparisonReport {
	var report HarnessComparisonReport
	report.SchemaVersion = 1
	dataByEvalSet := groupObservations(observations)
	evalSetNames := make([]string, 0, len(dataByEvalSet))
	for name := range dataByEvalSet {
		evalSetNames = append(evalSetNames, name)
	}
	sort.Strings(evalSetNames)

	for _, evalSetName := range evalSetNames {
		data := dataByEvalSet[evalSetName]
		harnesses := orderedHarnesses(data)
		candidates := orderedCandidates(data)
		groups := orderedGroups(data)
		evalSetReport := HarnessEvalSetReport{EvalSet: evalSetName}
		for _, candidate := range candidates {
			evalSetReport.Comparisons = append(evalSetReport.Comparisons, compareHarnesses(data.baseline, candidate, groups))
		}
		report.EvalSets = append(report.EvalSets, evalSetReport)
		report.Diagnostics = append(report.Diagnostics, collectDiagnostics(harnesses, groups)...)
	}

	sort.Slice(report.Diagnostics, func(i, j int) bool {
		left := report.Diagnostics[i]
		right := report.Diagnostics[j]
		if left.EvalSet != right.EvalSet {
			return left.EvalSet < right.EvalSet
		}
		if left.File != right.File {
			return left.File < right.File
		}
		if left.GroupKey != right.GroupKey {
			return left.GroupKey < right.GroupKey
		}
		if left.Repetition != right.Repetition {
			return left.Repetition < right.Repetition
		}
		return left.Harness < right.Harness
	})
	return report
}

func formatPercentage(value *float64) string {
	if value == nil {
		return "unavailable"
	}
	return fmt.Sprintf("%.1f%%", *value*100)
}

func formatSigned(value float64, fractionDigits int) string {
	format := fmt.Sprintf("%%.%df", fractionDigits)
	formatted := fmt.Sprintf(format, value)
	if value >= 0 {
		formatted = "+" + formatted
	}
	return formatted
}

func formatCoverage(eligiblePairs, totalPairs int) string {
	return fmt.Sprintf("(%d/%d pairs)", eligiblePairs, totalPairs)
}

func formatReportLine(label, value string) string {
	return fmt.Sprintf("    %9s  %s", label, value)
}

func formatMetric(label string, metric PairedMetricSummary, formatValue func(float64) string, formatDelta func(float64) string, comparisonPairs int) string {
	coverage := ""
	if metric.EligiblePairs != 0 && metric.EligiblePairs != comparisonPairs {
		coverage = " " + formatCoverage(metric.EligiblePairs, metric.TotalPairs)
	}
	if metric.BaselineMean == nil || metric.CandidateMean == nil || metric.MeanDelta == nil {
		return formatReportLine(label, "unavailable"+coverage)
	}
	delta := formatDelta(*metric.MeanDelta)
	values := fmt.Sprintf("(candidate %s, baseline %s)", formatValue(*metric.CandidateMean), formatValue(*metric.BaselineMean))
	return formatReportLine(label, delta+" "+values+coverage)
}

// FormatHarnessComparisonReport renders the comparison report as plain text.
// It returns an empty string when no eval set produced a comparison, so a
// caller can skip printing a vacuous report.
func FormatHarnessComparisonReport(report HarnessComparisonReport) string {
	hasComparisons := false
	for _, evalSet := range report.EvalSets {
		if len(evalSet.Comparisons) > 0 {
			hasComparisons = true
			break
		}
	}
	if !hasComparisons {
		return ""
	}

	lines := []string{"Eval Comparisons"}
	for _, evalSet := range report.EvalSets {
		lines = append(lines, "  "+evalSet.EvalSet)
		for index, comparison := range evalSet.Comparisons {
			if index > 0 {
				lines = append(lines, "")
			}
			lines = append(lines, formatReportLine("Baseline", comparison.Baseline))
			lines = append(lines, formatReportLine("Candidate", comparison.Candidate+" "+formatCoverage(comparison.Correctness.EligiblePairs, comparison.Correctness.TotalPairs)))
			if comparison.Correctness.Lift == nil {
				lines = append(lines, formatReportLine("Pass rate", "unavailable"))
			} else {
				lift := *comparison.Correctness.Lift * 100
				delta := formatSigned(lift, 1) + " pp"
				values := fmt.Sprintf("(candidate %s, baseline %s)", formatPercentage(comparison.Correctness.CandidatePassRate), formatPercentage(comparison.Correctness.BaselinePassRate))
				lines = append(lines, formatReportLine("Pass rate", delta+" "+values))
			}
			lines = append(lines, formatMetric("Tokens", comparison.TotalTokens, func(value float64) string { return fmt.Sprintf("%.1f", value) }, func(value float64) string { return formatSigned(value, 1) }, comparison.Correctness.EligiblePairs))
			lines = append(lines, formatMetric("Latency", comparison.TotalMs, func(value float64) string { return fmt.Sprintf("%.1fms", value) }, func(value float64) string { return formatSigned(value, 1) + "ms" }, comparison.Correctness.EligiblePairs))
			lines = append(lines, formatMetric("Est. cost", comparison.EstimatedCostUsd, func(value float64) string { return fmt.Sprintf("$%.4f", value) }, func(value float64) string {
				if value >= 0 {
					return fmt.Sprintf("+$%.4f", value)
				}
				return fmt.Sprintf("-$%.4f", -value)
			}, comparison.Correctness.EligiblePairs))
		}
	}
	if len(report.Diagnostics) > 0 {
		lines = append(lines, "  Incomplete observations")
		for _, diagnostic := range report.Diagnostics {
			lines = append(lines, fmt.Sprintf("    %s: %s/%s repetition %d, harness %s", diagnostic.Reason, diagnostic.File, diagnostic.TestName, diagnostic.Repetition, diagnostic.Harness))
		}
	}
	return strings.Join(lines, "\n")
}
