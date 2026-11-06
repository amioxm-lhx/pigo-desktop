package runtime

import (
	"context"
	"encoding/json"
	"errors"
	"regexp"
	"strconv"
	"strings"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
)

// Provider error classification for the agent path, mirroring the desktop's
// agent-errors table. The renderer keys its retry affordance off the code, so
// the table is part of the frozen wire contract.

// maxErrorMessageChars bounds what an envelope or transcript row carries;
// provider bodies can be huge.
const maxErrorMessageChars = 600

// ClassifiedError is one renderer-facing provider failure.
type ClassifiedError struct {
	Code      string
	Message   string
	Retriable bool
	Details   map[string]any
}

// AppError renders the classification in the wire shape the renderer shows and
// the failed transcript row carries.
func (c ClassifiedError) AppError() protocol.AppError {
	retriable := c.Retriable
	appErr := protocol.AppError{Code: c.Code, Message: c.Message, Retriable: &retriable}
	if len(c.Details) > 0 {
		if encoded, err := json.Marshal(c.Details); err == nil {
			appErr.Details = encoded
		}
	}
	return appErr
}

var (
	networkPattern = regexp.MustCompile(
		`(?i)ECONNREFUSED|ECONNRESET|ENOTFOUND|EAI_AGAIN|ETIMEDOUT|EPIPE|ENETUNREACH|EHOSTUNREACH|fetch failed|socket hang up|network error|connection refused|no such host`)

	contextPattern = regexp.MustCompile(
		`(?i)context[ _-]?length|maximum context|context window|too many tokens|prompt is too long|input token count|exceeds the (?:maximum|model)|token limit`)

	streamTerminationPattern = regexp.MustCompile(
		`(?i)\bterminated\b|stream ended without finish_reason|premature(?:ly)?\s+(?:closed|ended)|(?:stream|response).*(?:closed|interrupted)`)

	abortPattern         = regexp.MustCompile(`(?i)\babort`)
	unauthorizedPattern  = regexp.MustCompile(`(?i)invalid[ _]api[ _]key|api key not valid|unauthorized|authentication|permission denied`)
	rateLimitPattern     = regexp.MustCompile(`(?i)rate.?limit|too many requests|quota|overloaded`)
	modelNotFoundPattern = regexp.MustCompile(
		`(?i)model.{0,20}(not found|does not exist|unknown)|unknown model`)
	timeoutPattern = regexp.MustCompile(`(?i)timeout|timed out`)
	streamPattern  = regexp.MustCompile(`(?i)stream`)

	controlCharPattern            = regexp.MustCompile("[\x00-\x08\x0B\x0C\x0E-\x1F\x7F]")
	authorizationRedactionPattern = regexp.MustCompile(
		`(?i)(["']?authorization["']?\s*[:=]\s*["']?\s*bearer\s+)[^\s,"'}]+`)
	secretRedactionPattern = regexp.MustCompile(
		`(?i)(["']?(?:api[_-]?key|access[_-]?token|password)["']?\s*[:=]\s*["']?)[^"',}\s]+`)

	// statusMarkerPatterns probe a status the error text carries: pigo's
	// transport formats "transport: upstream 429: ..." and the provider drivers
	// add an HTTP marker on some paths.
	statusMarkerPatterns = []*regexp.Regexp{
		regexp.MustCompile(`(?i)upstream (\d{3})\b`),
		regexp.MustCompile(`^\s*(\d{3})\s*:`),
		regexp.MustCompile(`^\s*(\d{3})\b`),
		regexp.MustCompile(`\((\d{3})\)`),
		regexp.MustCompile(`(?i)status(?: code)?[ :]+(\d{3})\b`),
		regexp.MustCompile(`(?i)returned HTTP (\d{3})\b`),
		regexp.MustCompile(`(?i)\(HTTP (\d{3})\)`),
	}
)

// classifyError classifies one failure. aborted marks an abort the caller
// already knows about (a row that stopped with StopReasonAborted).
func classifyError(message string, cause error, aborted bool) ClassifiedError {
	safeMessage := trimErrorMessage(redactSensitiveErrorText(message))

	status, hasStatus := extractStatus(cause, message)
	details := map[string]any{}
	if hasStatus {
		details["providerStatus"] = status
	}

	result := func(code string, retriable bool) ClassifiedError {
		classified := ClassifiedError{Code: code, Message: safeMessage, Retriable: retriable}
		if len(details) > 0 {
			classified.Details = details
		}
		return classified
	}

	if aborted || errors.Is(cause, context.Canceled) || abortPattern.MatchString(message) {
		return result("TURN_ABORTED", false)
	}
	if hasNetworkCause(cause, message) {
		return result("NETWORK_ERROR", true)
	}

	if hasStatus {
		switch {
		case status == 401 || status == 403:
			return result("PROVIDER_UNAUTHORIZED", false)
		case status == 408:
			return result("TIMEOUT", true)
		case status == 413:
			return result("CONTEXT_TOO_LARGE", false)
		case status == 429:
			return result("PROVIDER_RATE_LIMITED", true)
		case status == 404:
			return result("MODEL_NOT_CONFIGURED", false)
		case status >= 500:
			return result("PROVIDER_ERROR", true)
		case status == 400 || status == 422:
			if contextPattern.MatchString(message) {
				return result("CONTEXT_TOO_LARGE", false)
			}
			return result("PROVIDER_ERROR", false)
		default:
			return result("PROVIDER_ERROR", true)
		}
	}

	switch {
	case unauthorizedPattern.MatchString(message):
		return result("PROVIDER_UNAUTHORIZED", false)
	case rateLimitPattern.MatchString(message):
		return result("PROVIDER_RATE_LIMITED", true)
	case contextPattern.MatchString(message):
		return result("CONTEXT_TOO_LARGE", false)
	case modelNotFoundPattern.MatchString(message):
		return result("MODEL_NOT_CONFIGURED", false)
	case timeoutPattern.MatchString(message):
		return result("TIMEOUT", true)
	case streamTerminationPattern.MatchString(message) || streamPattern.MatchString(message):
		return result("STREAM_FAILED", true)
	}
	return result("PROVIDER_ERROR", true)
}

func classifyAgentError(err error) ClassifiedError {
	if err == nil {
		return classifyError("", nil, false)
	}
	return classifyError(err.Error(), err, false)
}

// classifyAssistantFailure classifies the terminal error row a failed provider
// request leaves in the transcript.
func classifyAssistantFailure(assistant agentcore.AssistantMessage) ClassifiedError {
	message := strings.TrimSpace(assistant.ErrorMessage)
	if message == "" {
		message = "provider stream failed"
	}
	return classifyError(message, nil, assistant.StopReason == agentcore.StopReasonAborted)
}

func redactSensitiveErrorText(message string) string {
	message = authorizationRedactionPattern.ReplaceAllString(message, "${1}[REDACTED]")
	message = secretRedactionPattern.ReplaceAllString(message, "${1}[REDACTED]")
	return controlCharPattern.ReplaceAllString(message, "")
}

func trimErrorMessage(message string) string {
	runes := []rune(message)
	if len(runes) <= maxErrorMessageChars {
		return message
	}
	return string(runes[:maxErrorMessageChars]) + "…"
}

// extractStatus probes the HTTP status in the message (pigo's transport keeps
// the status in the error text rather than a typed error).
func extractStatus(cause error, message string) (int, bool) {
	if cause != nil {
		message = cause.Error() + " " + message
	}
	for _, pattern := range statusMarkerPatterns {
		match := pattern.FindStringSubmatch(message)
		if len(match) < 2 {
			continue
		}
		status, err := strconv.Atoi(match[1])
		if err != nil || status < 400 || status >= 600 {
			continue
		}
		return status, true
	}
	return 0, false
}

func hasNetworkCause(cause error, message string) bool {
	if networkPattern.MatchString(message) {
		return true
	}
	for depth, current := 0, cause; depth < 4 && current != nil; depth++ {
		if networkPattern.MatchString(current.Error()) {
			return true
		}
		current = errors.Unwrap(current)
	}
	return false
}

// providerStatusOf returns the HTTP status a classification carries, if any.
func providerStatusOf(classified ClassifiedError) int {
	switch value := classified.Details["providerStatus"].(type) {
	case int:
		return value
	case float64:
		return int(value)
	}
	return 0
}
