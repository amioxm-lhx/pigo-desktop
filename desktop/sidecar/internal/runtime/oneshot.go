package runtime

import (
	"context"
	"errors"
	"regexp"
	"strings"

	"github.com/smallnest/pigo/desktop/sidecar/internal/jsonrpc"
	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
)

// One-shot provider calls used by the composer (prompt enhancement) and by the
// auto session title. They deliberately carry no history and no tools.

// oneShotStream is the provider stream factory; tests replace it.
var oneShotStream = provider.StreamFnFromProvider

const promptEnhancementSystemPrompt = `You are a writing assistant that improves message drafts. Rewrite the user's
draft to be clearer, more concise, and better organized while preserving the
original meaning, intent, tone, and language. Do not add facts or requests
that the draft does not imply. Keep the same language as the draft. If the
draft is already good, return it with at most minor polish. Output only the
rewritten draft — no explanations, no preamble, no code fences, no quotation
marks.`

const sessionTitleSystemPrompt = `You generate a short, concise, descriptive session title summarizing the conversation based on the user's initial prompt and context.
Rules:
1. Output ONLY the title text. Do NOT wrap in quotes, brackets, or backticks.
2. Do not include markdown formatting, trailing punctuation, or emojis.
3. Keep it under 25 characters (or 4-7 words).
4. Use the primary language of the user's prompt (e.g. Chinese for Chinese requests, English for English requests).
5. Focus on the key topic or action (e.g. "Debug WebSocket reconnect", "重构用户认证模块").`

var (
	titleSurroundingPattern = regexp.MustCompile(`^[` + "`" + `"'\x{201c}\x{201d}\x{300c}\x{300d}]+|[` + "`" + `"'\x{201c}\x{201d}\x{300c}\x{300d}]+$`)
	titlePrefixPattern      = regexp.MustCompile(`^(Title|Session Title|会话标题|标题)\s*[:：]\s*`)
	titleWhitespacePattern  = regexp.MustCompile(`\s+`)
	titleTrailingPattern    = regexp.MustCompile(`[.。!！?？]+$`)
)

// EnhancePromptDraft rewrites one composer draft.
func EnhancePromptDraft(ctx context.Context, params protocol.EnhancePromptParams) (string, error) {
	draft := strings.TrimSpace(params.Draft)
	if draft == "" {
		return "", jsonrpc.AppError("PROMPT_ENHANCEMENT_EMPTY", "The draft is empty.")
	}
	text, err := completeOneShot(ctx, params.PromptParams, promptEnhancementSystemPrompt, "Draft:\n"+draft)
	if err != nil {
		return "", err
	}
	if strings.TrimSpace(text) == "" {
		return "", jsonrpc.AppError("PROMPT_ENHANCEMENT_EMPTY", "The model returned an empty enhanced draft.")
	}
	return text, nil
}

// SummarizeSessionTitle produces the short title shown in the session list.
func SummarizeSessionTitle(ctx context.Context, params protocol.SummarizeTitleParams) (string, error) {
	prompt := strings.TrimSpace(params.UserPrompt)
	if prompt == "" {
		return "", jsonrpc.AppError("TITLE_SUMMARIZATION_EMPTY", "The user prompt is empty.")
	}
	if len([]rune(prompt)) > 1000 {
		prompt = string([]rune(prompt)[:1000])
	}
	reply := strings.TrimSpace(params.AssistantReply)
	if len([]rune(reply)) > 500 {
		reply = string([]rune(reply)[:500])
	}
	content := "User Prompt:\n" + prompt
	if reply != "" {
		content += "\n\nAssistant Response Summary:\n" + reply
	}
	text, err := completeOneShot(ctx, params.PromptParams, sessionTitleSystemPrompt, content)
	if err != nil {
		return "", err
	}
	title := cleanSummarizedTitle(text)
	if title == "" {
		return "", jsonrpc.AppError("TITLE_SUMMARIZATION_EMPTY", "The model returned an empty session title.")
	}
	return title, nil
}

// CompletePlugin runs one plugin agent.complete request. The shell resolved
// the model binding; this sidecar only builds the message list and streams it.
func CompletePlugin(ctx context.Context, params protocol.PluginCompleteParams) (string, error) {
	messages := make(agentcore.MessageList, 0, len(params.Messages))
	for _, message := range params.Messages {
		content := agentcore.ContentList{agentcore.NewTextContent(message.Content)}
		if message.Role == "assistant" {
			messages = append(messages, agentcore.AssistantMessage{
				RoleField: agentcore.RoleAssistant,
				Content:   content,
			})
		} else {
			messages = append(messages, agentcore.UserMessage{
				RoleField: agentcore.RoleUser,
				Content:   content,
			})
		}
	}
	if len(messages) == 0 && strings.TrimSpace(params.System) == "" {
		return "", jsonrpc.AppError("INVALID_ARGUMENT", "messages or system is required")
	}
	text, err := completeOneShotMessages(ctx, params.PromptParams, params.System, messages)
	if err != nil {
		return "", err
	}
	if strings.TrimSpace(text) == "" {
		return "", jsonrpc.AppError("INVALID_ARGUMENT", "the model returned an empty response")
	}
	return text, nil
}

// completeOneShot runs one provider request with no tools or history.
func completeOneShot(ctx context.Context, params protocol.PromptParams, systemPrompt, userText string) (string, error) {
	return completeOneShotMessages(
		ctx,
		params,
		systemPrompt,
		agentcore.MessageList{agentcore.UserMessage{
			RoleField: agentcore.RoleUser,
			Content:   agentcore.ContentList{agentcore.NewTextContent(userText)},
		}},
	)
}

func completeOneShotMessages(
	ctx context.Context,
	params protocol.PromptParams,
	systemPrompt string,
	messages agentcore.MessageList,
) (string, error) {
	prov, model, err := buildProvider(params.Provider)
	if err != nil {
		return "", jsonrpc.AppError("MODEL_NOT_CONFIGURED", "model/provider not configured")
	}
	key := strings.TrimSpace(params.Provider.APIKey)
	switch {
	case key == "" && params.Provider.AuthKind == "none":
		key = "pi-desktop-no-auth"
	case key == "":
		// Launch resolves a vendor token before a one-shot call; an empty key
		// means the account is signed out.
		return "", jsonrpc.AppError("MODEL_NOT_CONFIGURED", "model/provider not configured")
	}

	llm := provider.LlmContext{
		SystemPrompt: systemPrompt,
		Messages:     messages,
	}
	streamFn := oneShotStream(prov)
	stream, err := streamFn(ctx, model.ID, llm, provider.StreamConfig{
		APIKey:        key,
		ThinkingLevel: normalizeThinkingLevel(params.ThinkingLevel),
		Headers:       params.Provider.Headers,
	})
	if err != nil {
		classified := classifyAgentError(err)
		return "", jsonrpc.AppError(classified.Code, "%s", classified.Message)
	}

	var final agentcore.AssistantMessage
	for event := range stream.Events() {
		switch typed := event.(type) {
		case provider.StreamDoneEvent:
			final = typed.Message
		case provider.StreamErrorEvent:
			classified := classifyAssistantFailure(typed.Message)
			return "", jsonrpc.AppError(classified.Code, "%s", classified.Message)
		}
	}
	if len(final.Content) == 0 {
		result, resultErr := stream.Result(ctx)
		if resultErr != nil {
			classified := classifyAgentError(resultErr)
			return "", jsonrpc.AppError(classified.Code, "%s", classified.Message)
		}
		final = result
	}
	text := strings.TrimSpace(agentcore.ContentToText(final.Content))
	if text == "" {
		return "", errors.New("the model returned an empty response")
	}
	return text, nil
}

// cleanSummarizedTitle strips the decoration the model sometimes adds.
func cleanSummarizedTitle(raw string) string {
	text := strings.TrimSpace(raw)
	text = titleSurroundingPattern.ReplaceAllString(text, "")
	text = titlePrefixPattern.ReplaceAllString(text, "")
	text = titleWhitespacePattern.ReplaceAllString(text, " ")
	text = titleTrailingPattern.ReplaceAllString(text, "")
	text = strings.TrimSpace(text)
	runes := []rune(text)
	if len(runes) > 80 {
		text = string(runes[:80])
	}
	return text
}
