package evalscmd

// Tests for the `pigo evals` subcommand: the flag surface (usage errors and
// the PIGO_MODEL/PIGO_PROVIDER fallback), and the record→compare round trip
// driven through a scripted stream so no network is touched.
import (
	"bytes"
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/evals"
	"github.com/smallnest/pigo/internal/provider"
)

// answerStream scripts a stream that answers every turn with the same text —
// enough to satisfy an answerContains expectation on every task.
func answerStream(t *testing.T, answer string) provider.StreamFn {
	t.Helper()
	return func(ctx context.Context, model string, llm provider.LlmContext, cfg provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		partial := agentcore.AssistantMessage{RoleField: agentcore.RoleAssistant}
		withText := partial
		withText.Content = agentcore.ContentList{agentcore.NewTextContent(answer)}
		final := withText
		final.StopReason = agentcore.StopReasonEndTurn
		stream := provider.NewAssistantMessageEventStream(0)
		go func() {
			for _, ev := range []provider.AssistantMessageEvent{
				provider.StreamStartEvent{Partial: partial},
				provider.StreamTextEvent{Partial: withText},
				provider.StreamDoneEvent{Message: final},
			} {
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

// cliTaskSet writes a two-task set to disk and returns its path.
func cliTaskSet(t *testing.T, dir string) string {
	t.Helper()
	set := map[string]any{
		"evalSet": "cli",
		"tasks": []any{
			map[string]any{"id": "t1", "prompt": "say ok", "expect": map[string]any{"answerContains": []string{"ok"}}},
			map[string]any{"id": "t2", "prompt": "say ok again", "expect": map[string]any{"answerContains": []string{"ok"}}},
		},
	}
	data, err := json.Marshal(set)
	if err != nil {
		t.Fatalf("marshal task set: %v", err)
	}
	path := filepath.Join(dir, "cli.tasks.json")
	if err := os.WriteFile(path, data, 0o644); err != nil {
		t.Fatalf("write task set: %v", err)
	}
	return path
}

func TestParseArgsValidation(t *testing.T) {
	cases := []struct {
		name  string
		args  []string
		valid bool
	}{
		{"no action", nil, false},
		{"both actions", []string{"--record", "--compare"}, false},
		{"unknown flag", []string{"--record", "--wat"}, false},
		{"missing value", []string{"--record", "--dir"}, false},
		{"bad repetitions", []string{"--record", "--repetitions", "0"}, false},
		{"non-numeric repetitions", []string{"--record", "--repetitions", "x"}, false},
		{"bad timeout", []string{"--record", "--timeout", "nope"}, false},
		{"record alone", []string{"--record"}, true},
		{"compare alone", []string{"--compare"}, true},
		{"full surface", []string{"--record", "--dir", "d", "--tasks", "f.json", "--repetitions", "3", "--timeout", "30s", "--keep-workspaces"}, true},
	}
	for _, tc := range cases {
		opts, err := parseArgs(tc.args)
		if tc.valid {
			if err != nil {
				t.Errorf("%s: unexpected error %v", tc.name, err)
			}
			continue
		}
		if err == nil {
			t.Errorf("%s: expected an error, got %+v", tc.name, opts)
		}
	}
}

func TestParseArgsModelFromEnvironment(t *testing.T) {
	t.Setenv("PIGO_MODEL", "env-model")
	t.Setenv("PIGO_PROVIDER", "env-provider")
	opts, err := parseArgs([]string{"--record"})
	if err != nil {
		t.Fatalf("parseArgs: %v", err)
	}
	if opts.model != "env-model" || opts.provider != "env-provider" {
		t.Fatalf("model/provider = %s/%s, want env-model/env-provider", opts.model, opts.provider)
	}
	// Flags win over the environment.
	opts, err = parseArgs([]string{"--record", "--model", "flag-model", "--provider", "flag-provider"})
	if err != nil {
		t.Fatalf("parseArgs: %v", err)
	}
	if opts.model != "flag-model" || opts.provider != "flag-provider" {
		t.Fatalf("model/provider = %s/%s, want flag-model/flag-provider", opts.model, opts.provider)
	}
}

func TestRunUsageErrors(t *testing.T) {
	cases := [][]string{nil, {"--record", "--compare"}, {"--bogus"}}
	for _, args := range cases {
		var out, errOut bytes.Buffer
		if code := Run(context.Background(), args, &out, &errOut); code != 2 {
			t.Errorf("Run(%v) = %d, want 2 (stderr: %s)", args, code, errOut.String())
		}
	}
}

func TestRunCommandRecordAndCompare(t *testing.T) {
	dir := t.TempDir()
	tasks := cliTaskSet(t, dir)
	base := evalsOptions{
		record:      true,
		dir:         dir,
		tasks:       tasks,
		repetitions: 1,
		timeout:     30 * time.Second,
		model:       "test-model",
	}
	good := evals.NewAgentHarness(evals.AgentHarnessConfig{
		Name:    "pigo",
		Model:   "test-model",
		Stream:  answerStream(t, "ok"),
		Timeout: 30 * time.Second,
	})

	var out, errOut bytes.Buffer
	if code := runCommand(context.Background(), base, good, &out, &errOut); code != 0 {
		t.Fatalf("record exit = %d, want 0 (stderr: %s)", code, errOut.String())
	}
	if _, err := os.Stat(evals.BaselinePath(dir, "cli")); err != nil {
		t.Fatalf("baseline not persisted: %v", err)
	}
	if !bytes.Contains(out.Bytes(), []byte("baseline recorded")) {
		t.Fatalf("record output missing summary: %s", out.String())
	}

	// Same behavior: compare must pass (exit 0).
	cmpOpts := base
	cmpOpts.record = false
	cmpOpts.compare = true
	out.Reset()
	if code := runCommand(context.Background(), cmpOpts, good, &out, &errOut); code != 0 {
		t.Fatalf("compare (identical) exit = %d, want 0 (stderr: %s)", code, errOut.String())
	}
	if !bytes.Contains(out.Bytes(), []byte("no regression")) {
		t.Fatalf("compare output missing verdict: %s", out.String())
	}

	// Degraded behavior: compare must fail closed (exit 1).
	bad := evals.NewAgentHarness(evals.AgentHarnessConfig{
		Name:    "pigo",
		Model:   "test-model",
		Stream:  answerStream(t, "nope"),
		Timeout: 30 * time.Second,
	})
	out.Reset()
	errOut.Reset()
	if code := runCommand(context.Background(), cmpOpts, bad, &out, &errOut); code != 1 {
		t.Fatalf("compare (degraded) exit = %d, want 1", code)
	}
	if !bytes.Contains(errOut.Bytes(), []byte("REGRESSED")) {
		t.Fatalf("stderr missing regression reason: %s", errOut.String())
	}
}

func TestRunCommandMissingTasksFile(t *testing.T) {
	dir := t.TempDir()
	opts := evalsOptions{record: true, dir: dir, tasks: filepath.Join(dir, "missing.json"), repetitions: 1, timeout: time.Second, model: "m"}
	h := evals.NewAgentHarness(evals.AgentHarnessConfig{Name: "pigo", Model: "m", Stream: answerStream(t, "ok"), Timeout: time.Second})
	var out, errOut bytes.Buffer
	if code := runCommand(context.Background(), opts, h, &out, &errOut); code != 1 {
		t.Fatalf("exit = %d, want 1 for a missing task set", code)
	}
}
