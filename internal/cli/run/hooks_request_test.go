// Tests for the request-scoped hook bridge (plan phase 1b): the TransformContext
// injection (ephemeral, reminder-shaped, inner transform preserved) and the
// BeforeRequest block path, driven through a real Dispatcher with sh hooks —
// plus the InstallSeams wiring so every driver gets both seams.
package run

import (
	"context"
	"strings"
	"testing"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/hooks"
	"github.com/smallnest/pigo/internal/provider"
	"github.com/smallnest/pigo/internal/runtime"
)

// requestHookSet builds a HookSet whose TransformContext entry echoes
// transformJSON and whose BeforeRequest entry echoes beforeJSON (or exits 2
// to block, with stderr as the reason).
func requestHookSet(transformJSON, beforeJSON string, beforeBlocks bool) hooks.HookSet {
	set := hooks.HookSet{}
	if transformJSON != "" {
		set[hooks.EventTransformContext] = []hooks.HookMatcherConfig{
			{Matcher: "*", Hooks: []hooks.HookConfig{{Command: "echo '" + transformJSON + "'"}}},
		}
	}
	if beforeJSON != "" || beforeBlocks {
		var parts []string
		if beforeJSON != "" {
			parts = append(parts, "echo '"+beforeJSON+"'")
		}
		if beforeBlocks {
			parts = append(parts, `echo "policy denied" >&2; exit 2`)
		}
		set[hooks.EventBeforeRequest] = []hooks.HookMatcherConfig{
			{Matcher: "*", Hooks: []hooks.HookConfig{{Command: strings.Join(parts, "; ")}}},
		}
	}
	return set
}

func TestTransformContextHookInjectsEphemeralContext(t *testing.T) {
	set := requestHookSet(`{"additionalContext":"use style guide X"}`, "", false)
	d := hooks.NewDispatcher(set, t.TempDir(), nil)
	if d == nil {
		t.Fatal("dispatcher should be built")
	}

	inner := func(ctx context.Context, msgs agentcore.MessageList) agentcore.MessageList {
		// The caller's existing transform must run first and its output is what
		// the hook injection appends to.
		return append(msgs, agentcore.UserMessage{RoleField: agentcore.RoleUser})
	}
	transform := transformContextHook(d, HookDeps{SessionID: "s1", ProjectDir: "/w"}, "m1", "p1", inner)

	msgs := agentcore.MessageList{agentcore.UserMessage{RoleField: agentcore.RoleUser, Content: agentcore.ContentList{agentcore.NewTextContent("hi")}}}
	out := transform(context.Background(), msgs)
	if len(out) != 3 { // original + inner's append + hook injection
		t.Fatalf("shaped messages = %d, want 3: %+v", len(out), out)
	}
	last, ok := out[len(out)-1].(agentcore.UserMessage)
	if !ok {
		t.Fatalf("injected message should be a user message, got %+v", out[len(out)-1])
	}
	text := agentcore.ContentToText(last.Content)
	if !strings.Contains(text, "use style guide X") {
		t.Fatalf("injected text = %q", text)
	}
	if !strings.Contains(text, "<system-reminder>") {
		t.Fatalf("injection should be system-reminder wrapped (ephemeral shape): %q", text)
	}
}

func TestTransformContextHookNoOpWithoutContext(t *testing.T) {
	// A hook that returns no additionalContext leaves the list untouched.
	set := requestHookSet(`{}`, "", false)
	d := hooks.NewDispatcher(set, t.TempDir(), nil)
	transform := transformContextHook(d, HookDeps{}, "m", "p", nil)
	msgs := agentcore.MessageList{agentcore.UserMessage{RoleField: agentcore.RoleUser}}
	if out := transform(context.Background(), msgs); len(out) != 1 {
		t.Fatalf("messages = %d, want 1 (untouched)", len(out))
	}
}

func TestBeforeRequestHookBlocks(t *testing.T) {
	set := requestHookSet("", "", true) // exit 2 = block, stderr is the reason
	d := hooks.NewDispatcher(set, t.TempDir(), nil)
	before := beforeRequestHook(d, HookDeps{}, "m1", "p1")

	err := before(context.Background(), provider.LlmContext{Messages: agentcore.MessageList{agentcore.UserMessage{RoleField: agentcore.RoleUser}}})
	if err == nil {
		t.Fatal("expected a block error")
	}
	if !strings.Contains(err.Error(), "BeforeRequest hook") || !strings.Contains(err.Error(), "policy denied") {
		t.Fatalf("block error should carry the hook reason: %v", err)
	}
}

func TestBeforeRequestHookAllows(t *testing.T) {
	set := requestHookSet("", `{"decision":"approve"}`, false)
	d := hooks.NewDispatcher(set, t.TempDir(), nil)
	before := beforeRequestHook(d, HookDeps{}, "m", "p")
	if err := before(context.Background(), provider.LlmContext{}); err != nil {
		t.Fatalf("approve decision should allow, got %v", err)
	}
}

func TestInstallSeamsWiresRequestHooks(t *testing.T) {
	set := requestHookSet(`{"additionalContext":"ctx"}`, "", false)
	d := hooks.NewDispatcher(set, t.TempDir(), nil)

	cfg := runtime.RunConfig{LoopConfig: runtime.LoopConfig{Model: "m1", Provider: "p1"}}
	InstallSeams(&cfg, d, HookDeps{SessionID: "s", ProjectDir: "/w"})
	if cfg.LoopConfig.TransformContext == nil {
		t.Fatal("InstallSeams should wire TransformContext")
	}
	if cfg.LoopConfig.BeforeRequest == nil {
		t.Fatal("InstallSeams should wire BeforeRequest")
	}

	// The wired TransformContext must still dispatch (the bridge is live).
	msgs := agentcore.MessageList{agentcore.UserMessage{RoleField: agentcore.RoleUser}}
	out := cfg.LoopConfig.TransformContext(context.Background(), msgs)
	if len(out) != 2 {
		t.Fatalf("wired transform should inject one message, got %d", len(out))
	}
	// The wired BeforeRequest must allow (the configured hook approves).
	if err := cfg.LoopConfig.BeforeRequest(context.Background(), provider.LlmContext{Messages: msgs}); err != nil {
		t.Fatalf("wired BeforeRequest should allow, got %v", err)
	}
}

func TestInstallSeamsNoHooksLeavesSeamsUnset(t *testing.T) {
	cfg := runtime.RunConfig{}
	InstallSeams(&cfg, nil, HookDeps{})
	if cfg.LoopConfig.TransformContext != nil || cfg.LoopConfig.BeforeRequest != nil {
		t.Fatal("nil dispatcher must leave the request seams unset (zero-cost no-hooks path)")
	}
}

func TestBeforeRequestBridgeEndToEnd(t *testing.T) {
	// The full chain: a BeforeRequest-blocking hook set, wired via InstallSeams,
	// aborts a real loop turn without contacting the provider.
	set := requestHookSet("", "", true)
	d := hooks.NewDispatcher(set, t.TempDir(), nil)

	streamCalls := 0
	cfg := runtime.RunConfig{
		LoopConfig: runtime.LoopConfig{
			Model: "fake",
			Stream: func(ctx context.Context, model string, llm provider.LlmContext, cfg provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
				streamCalls++
				s := provider.NewAssistantMessageEventStream(0)
				go s.Close()
				return s, nil
			},
		},
	}
	InstallSeams(&cfg, d, HookDeps{SessionID: "s", ProjectDir: t.TempDir()})

	agentCtx := &agentcore.AgentContext{Messages: agentcore.MessageList{agentcore.UserMessage{RoleField: agentcore.RoleUser, Content: agentcore.ContentList{agentcore.NewTextContent("hi")}}}}
	stream := runtime.StartRun(context.Background(), agentCtx, cfg)
	msg, err := runtime.DrainStream(context.Background(), stream, runtime.StreamHandler{})
	if err != nil {
		t.Fatalf("DrainStream: %v", err)
	}
	if msg == nil || msg.StopReason != agentcore.StopReasonError {
		t.Fatalf("blocked run should end with an erroring message, got %+v", msg)
	}
	if !strings.Contains(msg.ErrorMessage, "policy denied") {
		t.Fatalf("error message should carry the hook reason: %q", msg.ErrorMessage)
	}
	if streamCalls != 0 {
		t.Fatalf("provider contacted %d time(s) despite a blocked request", streamCalls)
	}
}
