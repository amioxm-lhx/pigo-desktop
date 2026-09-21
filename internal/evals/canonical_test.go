package evals

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestCanonicalizeJSONSortsKeys(t *testing.T) {
	// Two maps built in different insertion orders must encode identically.
	first := map[string]any{"z": 1, "a": map[string]any{"y": true, "b": "x"}}
	second := map[string]any{"a": map[string]any{"b": "x", "y": true}, "z": 1}
	left, err := CanonicalizeJSON(first)
	if err != nil {
		t.Fatalf("CanonicalizeJSON: %v", err)
	}
	right, err := CanonicalizeJSON(second)
	if err != nil {
		t.Fatalf("CanonicalizeJSON: %v", err)
	}
	if string(left) != string(right) {
		t.Fatalf("encodings differ: %s vs %s", left, right)
	}
	if !strings.Contains(string(left), `"a":{"b":"x","y":true}`) {
		t.Fatalf("keys not sorted: %s", left)
	}
}

func TestCanonicalizeJSONRejects(t *testing.T) {
	type plain struct{ X int } // structs are not plain JSON objects
	if _, err := CanonicalizeJSON(plain{X: 1}); err == nil {
		t.Error("struct should be rejected")
	}
	circular := map[string]any{}
	inner := []any{circular}
	circular["self"] = inner
	if _, err := CanonicalizeJSON(circular); err == nil {
		t.Error("circular reference should be rejected")
	}
}

func TestHashInputIDFastPath(t *testing.T) {
	key, err := HashInput(map[string]any{"id": "write-hello", "prompt": "x"})
	if err != nil {
		t.Fatalf("HashInput: %v", err)
	}
	if key != "write-hello" {
		t.Fatalf("key = %q, want the id fast path", key)
	}

	key, err = HashInput(map[string]any{"prompt": "x"})
	if err != nil {
		t.Fatalf("HashInput: %v", err)
	}
	if len(key) != 64 { // sha256 hex
		t.Fatalf("hashed key length = %d, want 64", len(key))
	}
}

func TestDeriveEvalGroupKey(t *testing.T) {
	key, err := DeriveEvalGroupKey(map[string]any{"id": "t1"}, 3)
	if err != nil {
		t.Fatalf("DeriveEvalGroupKey: %v", err)
	}
	var decoded []any
	if err := json.Unmarshal([]byte(key), &decoded); err != nil {
		t.Fatalf("group key is not a JSON array: %v", err)
	}
	if len(decoded) != 2 || decoded[0] != "t1" {
		t.Fatalf("group key = %s", key)
	}
	if f, ok := decoded[1].(float64); !ok || int(f) != 3 {
		t.Fatalf("group key = %s", key)
	}
}

func TestParseEvalHarnessIterationArtifact(t *testing.T) {
	valid := map[string]any{
		"schemaVersion": 1,
		"evalSet":       "s",
		"groupKey":      "[\"t\",1]",
		"harness":       "pigo",
		"baseline":      "pigo@baseline",
		"candidates":    []any{"pigo"},
		"repetition":    1,
	}
	artifact, ok := ParseEvalHarnessIterationArtifact(valid)
	if !ok || artifact.EvalSet != "s" || artifact.Repetition != 1 {
		t.Fatalf("artifact = %+v ok=%v", artifact, ok)
	}
	if _, ok := ParseEvalHarnessIterationArtifact(map[string]any{"schemaVersion": 2}); ok {
		t.Fatal("schema version 2 should not parse")
	}
	if _, ok := ParseEvalHarnessIterationArtifact("not a map"); ok {
		t.Fatal("a non-map should not parse")
	}
}

func TestBaselineObservationsRelabeling(t *testing.T) {
	records := []BaselineRecord{{
		SchemaVersion: BaselineSchemaVersion,
		EvalSet:       "s",
		TaskID:        "t1",
		Harness:       "pigo",
		Repetition:    1,
		Outcome:       OutcomeScored,
		Score:         1,
	}}
	observations := BaselineObservations(records, "s.tasks.json")
	if len(observations) != 1 {
		t.Fatalf("observations = %d", len(observations))
	}
	observation := observations[0]
	if observation.Harness != "pigo@baseline" || observation.Baseline != "pigo@baseline" {
		t.Fatalf("harness/baseline = %s/%s, want pigo@baseline", observation.Harness, observation.Baseline)
	}
	if len(observation.Candidates) != 1 || observation.Candidates[0] != "pigo" {
		t.Fatalf("candidates = %v", observation.Candidates)
	}
	if observation.Score == nil || *observation.Score != 1 {
		t.Fatalf("score = %v", observation.Score)
	}
}

func TestSummarizeHarnessComparisonsDiagnostics(t *testing.T) {
	// A baseline row with no candidate row yields a missing-observation
	// diagnostic rather than a silent drop.
	observations := BaselineObservations([]BaselineRecord{{
		SchemaVersion: BaselineSchemaVersion,
		EvalSet:       "s",
		TaskID:        "t1",
		Harness:       "pigo",
		Repetition:    1,
		Outcome:       OutcomeScored,
		Score:         1,
	}}, "s.tasks.json")
	report := SummarizeHarnessComparisons(observations)
	if len(report.Diagnostics) == 0 {
		t.Fatal("expected a missing-observation diagnostic for the candidate side")
	}
	if report.Diagnostics[0].Reason != "missing-observation" {
		t.Fatalf("reason = %s", report.Diagnostics[0].Reason)
	}
}
