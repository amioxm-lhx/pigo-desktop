package evals

import (
	_ "embed"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// SmokeTaskSet is the built-in smoke eval set: 18 deterministic tasks over
// read/write/edit/search/bash and multi-step tool chains, each verifiable
// without any external dependency. It is the default task set for
// `pigo --evals` and the baseline the regression gate compares against.
//
//go:embed testdata/smoke.tasks.json
var SmokeTaskSet []byte

// Task is one eval task: a prompt, the files seeded into an isolated
// workspace before the run, and the expectations the completed run is scored
// against.
type Task struct {
	// ID is the stable task identifier. It is the canonical input key for
	// grouping (HashInput's "id" fast path), so two records with the same ID
	// and repetition pair up in comparisons.
	ID string `json:"id"`
	// Prompt is the user message handed to the agent.
	Prompt string `json:"prompt"`
	// Files are seeded into the workspace (parents created) before the run.
	Files map[string]string `json:"files,omitempty"`
	// Expect defines how the completed run is scored.
	Expect Expectation `json:"expect"`
}

// Expectation is the scored outcome of a task: filesystem state and the
// final answer text. Every listed condition must hold for a score of 1.
type Expectation struct {
	// Exists lists relative paths that must exist (file or directory).
	Exists []string `json:"exists,omitempty"`
	// Absent lists relative paths that must NOT exist.
	Absent []string `json:"absent,omitempty"`
	// Contains maps a relative path to substrings its content must include.
	Contains map[string][]string `json:"contains,omitempty"`
	// NotContains maps a relative path to substrings its content must not
	// include.
	NotContains map[string][]string `json:"notContains,omitempty"`
	// AnswerContains lists substrings the final assistant text must include
	// (compared case-insensitively: a model that answers "PI-7731" or
	// "pi-7731" both pass, keeping the gate robust to phrasing).
	AnswerContains []string `json:"answerContains,omitempty"`
}

// TaskSet is a named collection of tasks loaded from a JSON document.
type TaskSet struct {
	EvalSet string `json:"evalSet"`
	Tasks   []Task `json:"tasks"`
}

// LoadTaskSet decodes a task-set JSON document. It validates that the set is
// named, has at least one task, and that every task carries a non-empty ID
// and prompt — a malformed set is a usage error, not something to run
// partially.
func LoadTaskSet(data []byte) (TaskSet, error) {
	var set TaskSet
	if err := json.Unmarshal(data, &set); err != nil {
		return TaskSet{}, fmt.Errorf("parse task set: %w", err)
	}
	set.EvalSet = strings.TrimSpace(set.EvalSet)
	if set.EvalSet == "" {
		return TaskSet{}, fmt.Errorf("task set: evalSet must not be empty")
	}
	if len(set.Tasks) == 0 {
		return TaskSet{}, fmt.Errorf("task set %q: no tasks", set.EvalSet)
	}
	seen := make(map[string]struct{}, len(set.Tasks))
	for index, task := range set.Tasks {
		task.ID = strings.TrimSpace(task.ID)
		if task.ID == "" {
			return TaskSet{}, fmt.Errorf("task set %q: task %d has an empty id", set.EvalSet, index)
		}
		if task.Prompt == "" {
			return TaskSet{}, fmt.Errorf("task set %q: task %s has an empty prompt", set.EvalSet, task.ID)
		}
		if _, exists := seen[task.ID]; exists {
			return TaskSet{}, fmt.Errorf("task set %q: duplicate task id %s", set.EvalSet, task.ID)
		}
		seen[task.ID] = struct{}{}
		set.Tasks[index] = task
	}
	return set, nil
}

// LoadTaskSetFile reads and parses a task-set JSON file from disk.
func LoadTaskSetFile(path string) (TaskSet, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return TaskSet{}, err
	}
	return LoadTaskSet(data)
}

// LoadSmokeTaskSet returns the embedded smoke task set.
func LoadSmokeTaskSet() (TaskSet, error) {
	return LoadTaskSet(SmokeTaskSet)
}

// TaskOutcome is the scored result of one task run: the score (1 = every
// expectation met, 0 = at least one missed), the final answer text, and the
// individual failed checks so a report can say what broke rather than just
// that something did.
type TaskOutcome struct {
	TaskID      string   `json:"taskId"`
	Score       float64  `json:"score"`
	Answer      string   `json:"answer,omitempty"`
	FailedCheck []string `json:"failedCheck,omitempty"`
}

// ScoreTask evaluates a completed run's workspace (root) and final answer
// against the task's expectations. A path in Exists/Contains that fails to
// stat or read is a failed check, not an error: scoring must never turn a
// missing file into a harness error, or an errored run would be
// indistinguishable from a scored zero.
func ScoreTask(task Task, root, answer string) TaskOutcome {
	outcome := TaskOutcome{TaskID: task.ID, Score: 1, Answer: answer}
	fail := func(format string, args ...any) {
		outcome.Score = 0
		outcome.FailedCheck = append(outcome.FailedCheck, fmt.Sprintf(format, args...))
	}

	for _, rel := range task.Expect.Exists {
		if _, err := os.Stat(filepath.Join(root, rel)); err != nil {
			fail("exists: %s", rel)
		}
	}
	for _, rel := range task.Expect.Absent {
		if _, err := os.Stat(filepath.Join(root, rel)); err == nil {
			fail("absent: %s", rel)
		}
	}
	for rel, needles := range task.Expect.Contains {
		content, err := os.ReadFile(filepath.Join(root, rel))
		if err != nil {
			fail("contains: %s unreadable: %v", rel, err)
			continue
		}
		text := string(content)
		for _, needle := range needles {
			if !strings.Contains(text, needle) {
				fail("contains: %s missing %q", rel, needle)
			}
		}
	}
	for rel, needles := range task.Expect.NotContains {
		content, err := os.ReadFile(filepath.Join(root, rel))
		if err != nil {
			// A file that does not exist trivially satisfies notContains.
			continue
		}
		text := string(content)
		for _, needle := range needles {
			if strings.Contains(text, needle) {
				fail("notContains: %s still has %q", rel, needle)
			}
		}
	}
	answerFolded := strings.ToLower(answer)
	for _, needle := range task.Expect.AnswerContains {
		if !strings.Contains(answerFolded, strings.ToLower(needle)) {
			fail("answerContains: missing %q", needle)
		}
	}
	return outcome
}

// SeedWorkspace writes a task's files into root, creating parent directories
// as needed. An absolute or traversal path in Files is rejected: the eval
// workspace must stay self-contained.
func SeedWorkspace(root string, files map[string]string) error {
	for rel, content := range files {
		if filepath.IsAbs(rel) {
			return fmt.Errorf("seed %q: absolute paths are not allowed", rel)
		}
		target := filepath.Join(root, filepath.FromSlash(rel))
		relToRoot, err := filepath.Rel(root, target)
		if err != nil || relToRoot == ".." || strings.HasPrefix(relToRoot, ".."+string(filepath.Separator)) {
			return fmt.Errorf("seed %q: escapes the workspace", rel)
		}
		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			return fmt.Errorf("seed %q: %w", rel, err)
		}
		if err := os.WriteFile(target, []byte(content), 0o644); err != nil {
			return fmt.Errorf("seed %q: %w", rel, err)
		}
	}
	return nil
}
