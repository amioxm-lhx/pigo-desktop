// Request-scoped hooks (plan phase 1b): the bridge that wires the hooks
// Dispatcher into the loop's two request seams. The runtime stays hook-agnostic
// — it only exposes TransformContext and BeforeRequest — while this file knows
// about both sides, mirroring hooks_install.go's layering.
//
// TransformContext dispatches while the request context is being shaped, BEFORE
// the reminder wrapTransform runs (the loop composes reminders around whatever
// TransformContext is already installed), so a hook's injected context is
// shaped like any other context modification and reminders still land last —
// preserving their ephemerality contract. Injected context is appended as a
// system-reminder-wrapped user message on the shaped copy only: it never
// enters the persisted history.
//
// BeforeRequest dispatches after the LLM context is fully assembled and before
// the provider stream opens; a block aborts the turn with the hook's reason.
// Neither event is a tool event, so tool matchers are ignored (matcher.go's
// empty-toolName rule) — and with no hook configured under the event,
// MatchHooks returns nothing and no process is forked (the lazily-guarded
// hot path).
package run

import (
	"context"
	"fmt"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/hooks"
	"github.com/smallnest/pigo/internal/provider"
	"github.com/smallnest/pigo/internal/runtime"
)

// installRequestHooks wires the request-scoped hook points (TransformContext,
// BeforeRequest) onto cfg. It is the request-side half of InstallSeams: a nil
// dispatcher is a no-op, so the no-hooks run pays nothing.
//
// The wiring order matters for reminder ephemerality: this composes the hook
// AROUND the existing TransformContext (inner first, hook after), and the loop
// then composes reminders around the result — so the final order is
// inner transform → hook injection → reminders, with reminders last.
func installRequestHooks(cfg *runtime.RunConfig, d *hooks.Dispatcher, deps HookDeps) {
	if d == nil {
		return
	}
	cfg.LoopConfig.TransformContext = transformContextHook(d, deps, cfg.LoopConfig.Model, cfg.LoopConfig.Provider, cfg.LoopConfig.TransformContext)
	cfg.LoopConfig.BeforeRequest = beforeRequestHook(d, deps, cfg.LoopConfig.Model, cfg.LoopConfig.Provider)
}

// transformContextHook adapts the Dispatcher's TransformContext event to the
// TransformContext seam. It runs the caller's existing transform first, then
// dispatches; any additionalContext the hooks contribute is appended to the
// shaped copy as a system-reminder-wrapped user message (the same ephemeral
// injection shape the reminder registry uses), so it reaches the model for
// this request only. A block is advisory at this stage — shaping has no
// side effect to undo — so blocks are surfaced as context instead, and the
// authoritative interception point is BeforeRequest.
func transformContextHook(
	d *hooks.Dispatcher,
	deps HookDeps,
	model, providerName string,
	inner func(ctx context.Context, msgs agentcore.MessageList) agentcore.MessageList,
) func(ctx context.Context, msgs agentcore.MessageList) agentcore.MessageList {
	return func(ctx context.Context, msgs agentcore.MessageList) agentcore.MessageList {
		if inner != nil {
			msgs = inner(ctx, msgs)
		}
		dec := d.Dispatch(ctx, hooks.EventTransformContext, "", hooks.HookInput{
			EventType:   hooks.EventTransformContext,
			SessionID:   deps.SessionID,
			ProjectDir:  deps.ProjectDir,
			Provider:    providerName,
			Model:       model,
			MessageCount: len(msgs),
		})
		text := joinHookText(dec.AdditionalContext, dec.Reason)
		if text == "" {
			return msgs
		}
		injected := agentcore.UserMessage{
			RoleField: agentcore.RoleUser,
			Content:   agentcore.ContentList{agentcore.NewTextContent(runtime.WrapSystemReminder(text))},
		}
		out := make(agentcore.MessageList, 0, len(msgs)+1)
		out = append(out, msgs...)
		out = append(out, injected)
		return out
	}
}

// beforeRequestHook adapts the Dispatcher's BeforeRequest event to the
// BeforeRequest seam. A block aborts the turn: the returned error carries the
// hook's reason and the loop renders it as an erroring assistant message —
// the provider is never contacted for a blocked request.
func beforeRequestHook(d *hooks.Dispatcher, deps HookDeps, model, providerName string) func(ctx context.Context, llm provider.LlmContext) error {
	return func(ctx context.Context, llm provider.LlmContext) error {
		dec := d.Dispatch(ctx, hooks.EventBeforeRequest, "", hooks.HookInput{
			EventType:   hooks.EventBeforeRequest,
			SessionID:   deps.SessionID,
			ProjectDir:  deps.ProjectDir,
			Provider:    providerName,
			Model:       model,
			MessageCount: len(llm.Messages),
		})
		if dec.Block {
			if dec.Reason != "" {
				return fmt.Errorf("request blocked by BeforeRequest hook: %s", dec.Reason)
			}
			return fmt.Errorf("request blocked by BeforeRequest hook")
		}
		return nil
	}
}
