package evals

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestLoadSmokeTaskSet(t *testing.T) {
	set, err := LoadSmokeTaskSet()
	if err != nil {
		t.Fatalf("LoadSmokeTaskSet: %v", err)
	}
	if set.EvalSet != "smoke" {
		t.Fatalf("evalSet = %q, want smoke", set.EvalSet)
	}
	if len(set.Tasks) != 18 {
		t.Fatalf("task count = %d, want 18", len(set.Tasks))
	}
	seen := make(map[string]bool)
	for _, task := range set.Tasks {
		if task.ID == "" || task.Prompt == "" {
			t.Fatalf("task %q has an empty id or prompt", task.ID)
		}
		if seen[task.ID] {
			t.Fatalf("duplicate task id %s", task.ID)
		}
		seen[task.ID] = true
	}
}

func TestLoadTaskSetValidation(t *testing.T) {
	cases := []struct {
		name string
		json string
	}{
		{"empty evalSet", `{"evalSet":"", "tasks":[{"id":"a","prompt":"p"}]}`},
		{"no tasks", `{"evalSet":"s", "tasks":[]}`},
		{"empty id", `{"evalSet":"s", "tasks":[{"id":"","prompt":"p"}]}`},
		{"empty prompt", `{"evalSet":"s", "tasks":[{"id":"a","prompt":""}]}`},
		{"duplicate id", `{"evalSet":"s", "tasks":[{"id":"a","prompt":"p"},{"id":"a","prompt":"q"}]}`},
		{"malformed", `{`},
	}
	for _, tc := range cases {
		if _, err := LoadTaskSet([]byte(tc.json)); err == nil {
			t.Errorf("%s: expected an error", tc.name)
		}
	}
}

func TestSeedWorkspaceRejectsEscapes(t *testing.T) {
	root := t.TempDir()
	bad := map[string]string{
		"../escape.txt": "x",
		"sub/../../x":   "x",
		"/abs/path.txt": "x",
	}
	for rel, content := range bad {
		if err := SeedWorkspace(root, map[string]string{rel: content}); err == nil {
			t.Errorf("SeedWorkspace(%q) should have failed", rel)
		}
	}
}

func TestSeedWorkspaceCreatesParents(t *testing.T) {
	root := t.TempDir()
	if err := SeedWorkspace(root, map[string]string{"src/alpha.go": "package main\n"}); err != nil {
		t.Fatalf("SeedWorkspace: %v", err)
	}
	data, err := os.ReadFile(filepath.Join(root, "src", "alpha.go"))
	if err != nil {
		t.Fatalf("read seeded file: %v", err)
	}
	if string(data) != "package main\n" {
		t.Fatalf("content = %q", string(data))
	}
}

func TestScoreTask(t *testing.T) {
	root := t.TempDir()
	if err := SeedWorkspace(root, map[string]string{"a.txt": "hello world\n"}); err != nil {
		t.Fatalf("SeedWorkspace: %v", err)
	}

	task := Task{
		ID:     "t",
		Prompt: "p",
		Expect: Expectation{
			Exists:         []string{"a.txt"},
			Absent:         []string{"b.txt"},
			Contains:       map[string][]string{"a.txt": {"hello"}},
			NotContains:    map[string][]string{"a.txt": {"beta"}},
			AnswerContains: []string{"PI-7731"},
		},
	}

	outcome := ScoreTask(task, root, "The code is pi-7731.")
	if outcome.Score != 1 {
		t.Fatalf("score = %v, want 1 (failed: %v)", outcome.Score, outcome.FailedCheck)
	}

	// Miss the answer check only.
	outcome = ScoreTask(task, root, "something else")
	if outcome.Score != 0 {
		t.Fatalf("score = %v, want 0", outcome.Score)
	}
	if len(outcome.FailedCheck) != 1 || !strings.Contains(outcome.FailedCheck[0], "answerContains") {
		t.Fatalf("failed checks = %v", outcome.FailedCheck)
	}

	// A missing file fails exists and contains but never errors.
	outcome = ScoreTask(Task{
		ID:     "t2",
		Prompt: "p",
		Expect: Expectation{
			Exists:   []string{"gone.txt"},
			Contains: map[string][]string{"gone.txt": {"x"}},
		},
	}, root, "")
	if outcome.Score != 0 {
		t.Fatalf("score = %v, want 0", outcome.Score)
	}

	// notContains on a missing file is trivially satisfied.
	outcome = ScoreTask(Task{
		ID:     "t3",
		Prompt: "p",
		Expect: Expectation{NotContains: map[string][]string{"gone.txt": {"x"}}},
	}, root, "")
	if outcome.Score != 1 {
		t.Fatalf("score = %v, want 1", outcome.Score)
	}
}

func TestBaselinePathRejectsTraversalNames(t *testing.T) {
	for _, name := range []string{"..", ".", "a/b", `a\b`, ""} {
		if err := validateEvalSetName(name); err == nil {
			t.Errorf("validateEvalSetName(%q) should have failed", name)
		}
	}
	if err := validateEvalSetName("smoke"); err != nil {
		t.Errorf("validateEvalSetName(smoke) = %v, want nil", err)
	}
}

func TestBaselineRecordRoundTrip(t *testing.T) {
	dir := t.TempDir()
	record := BaselineRecord{
		SchemaVersion: BaselineSchemaVersion,
		EvalSet:       "s",
		TaskID:        "t1",
		Harness:       "pigo",
		Repetition:    1,
		Outcome:       OutcomeScored,
		Score:         1,
		TotalTokens:   42,
		TotalMs:       1.5,
	}
	encoded, err := json.Marshal(record)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if err := os.WriteFile(BaselinePath(dir, "s"), append(encoded, '\n'), 0o644); err != nil {
		t.Fatalf("write baseline: %v", err)
	}

	loaded, err := LoadBaseline(dir, "s")
	if err != nil {
		t.Fatalf("LoadBaseline: %v", err)
	}
	if len(loaded) != 1 || loaded[0].TaskID != "t1" || loaded[0].Score != 1 {
		t.Fatalf("loaded = %+v", loaded)
	}

	if _, err := LoadBaseline(dir, "missing"); err == nil {
		t.Fatal("LoadBaseline on a missing set should error")
	}
}
