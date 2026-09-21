// Package evalscmd implements the `pigo evals` subcommand: the CLI front door
// of the evaluation regression gate. It records a baseline of the smoke task
// set (`pigo evals --record`) and re-runs the set against that baseline after
// a structural change (`pigo evals --compare`), exiting non-zero when the
// comparison regresses — the gate every later phase of the migration plan
// closes with.
//
//	pigo evals --record                       # run the smoke set, persist the baseline
//	pigo evals --compare                      # re-run, diff against the baseline
//	pigo evals --record --tasks my.tasks.json # a custom task set
//
// The model is resolved like the main CLI (--model/--provider flags over the
// PIGO_MODEL/PIGO_PROVIDER environment variables, mirroring pi-go's
// PI_PROVIDER/PI_MODEL convention) and the run itself is driven by the same
// provider resolution chain (provider.ResolveProvider), so an eval run talks
// to the exact backend a normal run would.
//
// Exit codes: 0 success (recorded, or compared with no regression), 1 runtime
// failure or regression, 2 usage error.
package evalscmd

import (
	"context"
	"fmt"
	"io"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/smallnest/pigo/internal/evals"
	"github.com/smallnest/pigo/internal/provider"
)

// DefaultEvalsDir is where baselines live when --dir is not given: the
// project-local .pigo/evals, next to the other per-project pigo state, so a
// baseline travels with the checkout it gates.
const DefaultEvalsDir = ".pigo/evals"

// evalsOptions is the parsed `pigo evals` command line.
type evalsOptions struct {
	record        bool
	compare       bool
	dir           string
	tasks         string
	repetitions   int
	timeout       time.Duration
	keepWorkspace bool

	model    string
	provider string
	baseURL  string
	protocol string
	apiKey   string
}

// Run parses args, assembles the provider-backed harness, and executes the
// requested eval action. It returns a process exit code.
func Run(ctx context.Context, args []string, out, errOut io.Writer) int {
	opts, err := parseArgs(args)
	if err != nil {
		fmt.Fprintf(errOut, "pigo evals: %v\n", err)
		printUsage(errOut)
		return 2
	}

	prov, providerName, err := provider.ResolveProvider(opts.model, opts.baseURL, opts.protocol, opts.provider, os.Getenv)
	if err != nil {
		fmt.Fprintf(errOut, "pigo evals: %v\n", err)
		return 1
	}
	creds := provider.NewCredentialStore(nil)
	creds.SetOverride(providerName, opts.apiKey)
	harness := evals.NewAgentHarness(evals.AgentHarnessConfig{
		Name:           "pigo",
		Model:          opts.model,
		ProviderName:   providerName,
		Stream:         provider.StreamFnFromProvider(prov),
		GetAPIKey:      creds.GetAPIKey,
		Timeout:        opts.timeout,
		KeepWorkspaces: opts.keepWorkspace,
	})
	return runCommand(ctx, opts, harness, out, errOut)
}

// runCommand executes the record/compare action against an assembled harness.
// It is the testable core: Run only adds flag parsing and provider assembly.
func runCommand(ctx context.Context, opts evalsOptions, harness *evals.AgentHarness, out, errOut io.Writer) int {
	set, err := loadTaskSet(opts.tasks)
	if err != nil {
		fmt.Fprintf(errOut, "pigo evals: %v\n", err)
		return 1
	}

	switch {
	case opts.record:
		return runRecord(ctx, opts, harness, set, out, errOut)
	default:
		return runCompare(ctx, opts, harness, set, out, errOut)
	}
}

// loadTaskSet resolves the task set: an explicit --tasks file, else the
// built-in smoke set.
func loadTaskSet(tasksPath string) (evals.TaskSet, error) {
	if strings.TrimSpace(tasksPath) == "" {
		return evals.LoadSmokeTaskSet()
	}
	return evals.LoadTaskSetFile(tasksPath)
}

// runRecord implements `pigo evals --record`: run the set, persist the
// baseline, and print a summary line.
func runRecord(ctx context.Context, opts evalsOptions, harness *evals.AgentHarness, set evals.TaskSet, out, errOut io.Writer) int {
	announce(out, opts, set)
	records, err := evals.RecordBaseline(ctx, harness, set, opts.dir, opts.repetitions)
	if err != nil {
		fmt.Fprintf(errOut, "pigo evals: %v\n", err)
		return 1
	}
	passed, errored := 0, 0
	for _, record := range records {
		if record.Outcome == evals.OutcomeErrored {
			errored++
			continue
		}
		if record.Score >= 1 {
			passed++
		}
	}
	fmt.Fprintf(out, "baseline recorded: %d/%d rows passed (%d errored) → %s\n",
		passed, len(records), errored, evals.BaselinePath(opts.dir, set.EvalSet))
	return 0
}

// runCompare implements `pigo evals --compare`: re-run the set, pair the fresh
// rows with the stored baseline, print the comparison report, and map the
// regression gate onto the exit code.
func runCompare(ctx context.Context, opts evalsOptions, harness *evals.AgentHarness, set evals.TaskSet, out, errOut io.Writer) int {
	announce(out, opts, set)
	gate, err := evals.CompareToBaseline(ctx, harness, set, opts.dir, opts.repetitions)
	if err != nil {
		fmt.Fprintf(errOut, "pigo evals: %v\n", err)
		return 1
	}
	if report := evals.FormatHarnessComparisonReport(gate.Report); report != "" {
		fmt.Fprintln(out, report)
	}
	if gate.Regressed {
		fmt.Fprintf(errOut, "pigo evals: REGRESSED: %s\n", gate.Reason)
		return 1
	}
	fmt.Fprintln(out, "comparison passed: no regression against the baseline")
	return 0
}

// announce prints the run header: what is about to run, how many times, and
// where the baseline lives. The runs themselves stay silent inside the evals
// package, so this is the user's only progress signal up front.
func announce(out io.Writer, opts evalsOptions, set evals.TaskSet) {
	action := "record"
	if opts.compare {
		action = "compare"
	}
	taskSource := "built-in smoke set"
	if opts.tasks != "" {
		taskSource = opts.tasks
	}
	fmt.Fprintf(out, "pigo evals --%s: %d tasks × %d repetition(s) from %s (model %s)\n",
		action, len(set.Tasks), opts.repetitions, taskSource, opts.model)
}

// parseArgs parses the `pigo evals` flag surface by hand (early-dispatch
// subcommands do not engage the global pflag set, matching sessioncmd).
// Model selection prefers the flags and falls back to PIGO_MODEL/
// PIGO_PROVIDER, mirroring pi-go's PI_PROVIDER/PI_MODEL convention.
func parseArgs(args []string) (evalsOptions, error) {
	opts := evalsOptions{
		dir:         DefaultEvalsDir,
		repetitions: 1,
		timeout:     evals.DefaultTaskTimeout,
		model:       strings.TrimSpace(os.Getenv("PIGO_MODEL")),
		provider:    strings.TrimSpace(os.Getenv("PIGO_PROVIDER")),
	}
	if opts.model == "" {
		opts.model = "openrouter/free"
	}

	for i := 0; i < len(args); i++ {
		arg := args[i]
		next := func(what string) (string, error) {
			if i+1 >= len(args) {
				return "", fmt.Errorf("%s needs a value", what)
			}
			i++
			return args[i], nil
		}
		var value string
		var err error
		switch arg {
		case "--record":
			opts.record = true
			continue
		case "--compare":
			opts.compare = true
			continue
		case "--keep-workspaces":
			opts.keepWorkspace = true
			continue
		case "--dir", "--tasks", "--model", "--provider", "--base-url", "--protocol", "--api-key", "--timeout", "--repetitions":
			value, err = next(arg)
			if err != nil {
				return opts, err
			}
		default:
			return opts, fmt.Errorf("unknown flag %q", arg)
		}
		switch arg {
		case "--dir":
			opts.dir = value
		case "--tasks":
			opts.tasks = value
		case "--model":
			opts.model = value
		case "--provider":
			opts.provider = value
		case "--base-url":
			opts.baseURL = value
		case "--protocol":
			opts.protocol = value
		case "--api-key":
			opts.apiKey = value
		case "--timeout":
			d, err := time.ParseDuration(value)
			if err != nil || d <= 0 {
				return opts, fmt.Errorf("--timeout: invalid duration %q", value)
			}
			opts.timeout = d
		case "--repetitions":
			n, err := strconv.Atoi(value)
			if err != nil || n < 1 {
				return opts, fmt.Errorf("--repetitions: invalid count %q", value)
			}
			opts.repetitions = n
		}
	}

	if opts.record == opts.compare {
		if opts.record {
			return opts, fmt.Errorf("--record and --compare are mutually exclusive")
		}
		return opts, fmt.Errorf("nothing to do: pass --record or --compare")
	}
	return opts, nil
}

func printUsage(errOut io.Writer) {
	fmt.Fprintln(errOut, "usage: pigo evals (--record | --compare) [--dir DIR] [--tasks FILE] [--repetitions N]")
	fmt.Fprintln(errOut, "                 [--timeout DURATION] [--keep-workspaces]")
	fmt.Fprintln(errOut, "                 [--model ID] [--provider NAME] [--base-url URL] [--protocol P] [--api-key KEY]")
}
