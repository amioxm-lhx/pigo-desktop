package runtime

import (
	"errors"
	"strings"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
)

// buildProvider turns one desktop provider row into a pigo provider and model.
//
// Desktop rows point at arbitrary endpoints, so the generic OpenAI / Anthropic
// / Responses drivers are used instead of pigo's named-provider registry.
func buildProvider(cfg protocol.ProviderConfig) (provider.Provider, provider.Model, error) {
	modelID := strings.TrimSpace(cfg.ModelID)
	if modelID == "" {
		return nil, provider.Model{}, errors.New("model/provider not configured")
	}

	var meta *protocol.ModelConfig
	if cfg.ModelConfig != nil {
		meta = cfg.ModelConfig
	}
	baseURL := providerBaseURL(cfg)

	style := resolveAPIStyle(cfg.APIStyle, meta)
	model := provider.Model{
		Provider:         cfg.ID,
		ID:               modelID,
		DisplayName:      firstNonEmpty(nameOf(meta), cfg.Name, modelID),
		SupportsThinking: cfg.SupportsReasoning,
		SupportsTools:    true,
	}
	if meta != nil {
		model.ContextWindow = meta.ContextWindow
		model.MaxOutputTokens = meta.MaxTokens
		model.SupportsThinking = model.SupportsThinking || meta.Reasoning
		if meta.ToolCall != nil {
			model.SupportsTools = *meta.ToolCall
		}
		for _, input := range meta.Input {
			if strings.EqualFold(input, "image") {
				model.SupportsImages = true
			}
		}
	}

	switch style {
	case "anthropic":
		if cfg.AuthKind == "oauth" {
			// Vendor access tokens carry Bearer auth; an API key uses x-api-key.
			return provider.NewAnthropicProtocolProvider(cfg.ID, baseURL, provider.AuthBearer, []provider.Model{model}), model, nil
		}
		return provider.NewAnthropicProvider(baseURL, []provider.Model{model}), model, nil
	case "responses":
		if baseURL == "" {
			return nil, provider.Model{}, errors.New("openai responses provider requires a base URL")
		}
		return provider.NewOpenAIResponsesProvider(cfg.ID, baseURL, []provider.Model{model}), model, nil
	default:
		if baseURL == "" {
			return nil, provider.Model{}, errors.New("openai-compatible provider requires a base URL")
		}
		return provider.NewOpenAICompatibleProvider(baseURL, []provider.Model{model}), model, nil
	}
}

func resolveAPIStyle(style string, meta *protocol.ModelConfig) string {
	values := []string{style}
	if meta != nil {
		values = append(values, meta.API)
	}
	for _, value := range values {
		normalized := strings.ToLower(strings.TrimSpace(value))
		switch {
		case normalized == "", normalized == "auto":
			continue
		case strings.Contains(normalized, "anthropic"):
			return "anthropic"
		case strings.Contains(normalized, "response"), strings.Contains(normalized, "resp_api"):
			return "responses"
		default:
			return "openai"
		}
	}
	return "openai"
}

func nameOf(meta *protocol.ModelConfig) string {
	if meta == nil {
		return ""
	}
	return meta.Name
}

func providerBaseURL(cfg protocol.ProviderConfig) string {
	baseURL := strings.TrimSpace(cfg.BaseURL)
	if cfg.ModelConfig != nil && strings.TrimSpace(cfg.ModelConfig.BaseURL) != "" {
		baseURL = strings.TrimSpace(cfg.ModelConfig.BaseURL)
	}
	return baseURL
}

func firstNonEmpty(values ...string) string {
	for _, value := range values {
		if strings.TrimSpace(value) != "" {
			return value
		}
	}
	return ""
}

func normalizeThinkingLevel(value string) agentcore.ThinkingLevel {
	switch agentcore.ThinkingLevel(strings.ToLower(strings.TrimSpace(value))) {
	case agentcore.ThinkingOff:
		return agentcore.ThinkingOff
	case agentcore.ThinkingMinimal:
		return agentcore.ThinkingMinimal
	case agentcore.ThinkingLow:
		return agentcore.ThinkingLow
	case agentcore.ThinkingMedium:
		return agentcore.ThinkingMedium
	case agentcore.ThinkingHigh:
		return agentcore.ThinkingHigh
	case agentcore.ThinkingXHigh:
		return agentcore.ThinkingXHigh
	case agentcore.ThinkingMax:
		return agentcore.ThinkingMax
	default:
		return agentcore.ThinkingMedium
	}
}
