package runtime

import (
	"context"
	"strings"
	"testing"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
)

func oneShotTestParams() protocol.PromptParams {
	return protocol.PromptParams{
		SessionID: "session-oneshot",
		Provider: protocol.ProviderConfig{
			ID:       "desktop",
			ModelID:  "test-model",
			APIStyle: "openai",
			BaseURL:  "http://example.test",
			AuthKind: "none",
		},
	}
}

func textOnceStream(text string) provider.StreamFn {
	return func(ctx context.Context, _ string, _ provider.LlmContext, _ provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		stream := provider.NewAssistantMessageEventStream(4)
		go func() {
			defer stream.Close()
			now := time.Now().UnixMilli()
			final := agentcore.AssistantMessage{
				RoleField:  agentcore.RoleAssistant,
				StopReason: agentcore.StopReasonEndTurn,
				Content:    agentcore.ContentList{agentcore.NewTextContent(text)},
				Timestamp:  now,
			}
			_ = stream.Emit(ctx, provider.StreamStartEvent{Partial: agentcore.AssistantMessage{RoleField: agentcore.RoleAssistant, Timestamp: now}})
			_ = stream.Emit(ctx, provider.StreamTextEvent{Partial: final})
			_ = stream.Emit(ctx, provider.StreamDoneEvent{Message: final})
		}()
		return stream, nil
	}
}

func withOneShotStream(t *testing.T, stream provider.StreamFn) {
	t.Helper()
	old := oneShotStream
	oneShotStream = func(provider.Provider) provider.StreamFn { return stream }
	t.Cleanup(func() { oneShotStream = old })
}

func TestEnhancePromptDraftUsesOneShot(t *testing.T) {
	withOneShotStream(t, textOnceStream("Clearer draft"))
	params := protocol.EnhancePromptParams{PromptParams: oneShotTestParams(), Draft: "make this clear"}
	enhanced, err := EnhancePromptDraft(context.Background(), params)
	if err != nil {
		t.Fatalf("EnhancePromptDraft: %v", err)
	}
	if strings.TrimSpace(enhanced) != "Clearer draft" {
		t.Fatalf("enhanced = %q", enhanced)
	}
}

func TestEnhancePromptDraftRejectsEmptyDraft(t *testing.T) {
	params := protocol.EnhancePromptParams{PromptParams: oneShotTestParams(), Draft: "  "}
	if _, err := EnhancePromptDraft(context.Background(), params); err == nil {
		t.Fatal("an empty draft must fail")
	}
}

func TestSummarizeSessionTitleCleansModelOutput(t *testing.T) {
	withOneShotStream(t, textOnceStream("Title: 重构认证模块。"))
	params := protocol.SummarizeTitleParams{
		PromptParams:   oneShotTestParams(),
		UserPrompt:     "帮我重构用户认证模块",
		AssistantReply: "好的，先梳理现有登录流程",
	}
	title, err := SummarizeSessionTitle(context.Background(), params)
	if err != nil {
		t.Fatalf("SummarizeSessionTitle: %v", err)
	}
	if title != "重构认证模块" {
		t.Fatalf("title = %q", title)
	}
}

func TestCompletePluginBuildsUserAndAssistantMessages(t *testing.T) {
	var seen provider.LlmContext
	base := textOnceStream("plugin result")
	withOneShotStream(t, func(ctx context.Context, model string, llm provider.LlmContext, cfg provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		seen = llm
		return base(ctx, model, llm, cfg)
	})
	params := protocol.PluginCompleteParams{
		PromptParams: oneShotTestParams(),
		System:       "system",
		Messages: []protocol.PluginCompleteMessage{
			{Role: "user", Content: "hello"},
			{Role: "assistant", Content: "hi"},
		},
	}
	text, err := CompletePlugin(context.Background(), params)
	if err != nil {
		t.Fatalf("CompletePlugin: %v", err)
	}
	if text != "plugin result" {
		t.Fatalf("text = %q", text)
	}
	if seen.SystemPrompt != "system" || len(seen.Messages) != 2 {
		t.Fatalf("llm context = %#v", seen)
	}
	if seen.Messages[0].Role() != agentcore.RoleUser || seen.Messages[1].Role() != agentcore.RoleAssistant {
		t.Fatalf("roles = %q, %q", seen.Messages[0].Role(), seen.Messages[1].Role())
	}
}

func TestCompletePluginRejectsEmptyRequest(t *testing.T) {
	params := protocol.PluginCompleteParams{PromptParams: oneShotTestParams()}
	if _, err := CompletePlugin(context.Background(), params); err == nil {
		t.Fatal("an empty system and message list must fail")
	}
}

func TestCleanSummarizedTitle(t *testing.T) {
	cases := map[string]string{
		`"Debug WebSocket reconnect"`: "Debug WebSocket reconnect",
		"Title: 修复登录":                 "修复登录",
		"会话标题：修复登录":                   "修复登录",
		"  Refactor auth.  ":          "Refactor auth",
	}
	for input, want := range cases {
		if got := cleanSummarizedTitle(input); got != want {
			t.Errorf("cleanSummarizedTitle(%q) = %q, want %q", input, got, want)
		}
	}
}
