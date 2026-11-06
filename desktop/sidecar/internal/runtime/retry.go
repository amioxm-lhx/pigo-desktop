package runtime

import (
	"context"
	"math"
	"math/rand/v2"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/provider"
)

// Provider retry policy and the stream wrapper that applies it. pigo's
// transport reports a setup failure as an error return (usually
// "transport: upstream <status>: <body>"), so the wrapper owns the schedule
// for request-setup failures. A stream that fails after it started streaming
// is terminal in this shell: replaying already-consumed deltas would break the
// renderer's append-only rows.
const (
	providerMaxRetries = 10

	rateLimitInitialDelayMS = 2_000
	rateLimitJitterFactor   = 0.25
	rateLimitMaxDelayMS     = 30_000

	setupRetryInitialDelayMS = 1_000
	setupRetryMaxDelayMS     = 8_000
)

// transientRetryCodes may claim the retry budget. CONTEXT_TOO_LARGE has its
// own recovery path (compaction), and auth/model failures are not repaired by
// re-sending the request.
var transientRetryCodes = map[string]bool{
	"NETWORK_ERROR":         true,
	"TIMEOUT":               true,
	"STREAM_FAILED":         true,
	"PROVIDER_ERROR":        true,
	"PROVIDER_RATE_LIMITED": true,
}

// retryDelayHook lets tests bypass the real backoff. Nil in production.
var retryDelayHook func(ClassifiedError, int) int

// retryWaitHook lets tests bypass the real sleep. Nil in production.
var retryWaitHook func(context.Context, time.Duration) bool

// withProviderRetry wraps the provider stream with the retry policy and the
// overflow-compaction recovery.
func (s *Session) withProviderRetry(inner provider.StreamFn) provider.StreamFn {
	return func(ctx context.Context, model string, llm provider.LlmContext, cfg provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		for attempt := 1; ; attempt++ {
			stream, err := inner(ctx, model, llm, cfg)
			if err == nil {
				return stream, nil
			}
			if ctx.Err() != nil {
				return nil, ctx.Err()
			}

			classified := classifyAgentError(err)
			if classified.Code == "CONTEXT_TOO_LARGE" {
				// One compaction attempt per turn. A successful compaction rewrites
				// the live context and the LLM view, then the request is rebuilt.
				if s.claimOverflowCompaction() && s.compactOverflow(ctx, &llm) {
					continue
				}
				return nil, err
			}
			if !classified.Retriable || !transientRetryCodes[classified.Code] || attempt > providerMaxRetries {
				return nil, err
			}

			delay := s.providerRetryDelay(classified, attempt)
			s.emitRetryActivity(classified, attempt, delay)
			if !waitForRetry(ctx, time.Duration(delay)*time.Millisecond) {
				return nil, ctx.Err()
			}
		}
	}
}

// providerRetryDelay picks the schedule for one claimed attempt. Rate limits
// back off more slowly; every other transient failure doubles from one second
// and caps at eight.
func (s *Session) providerRetryDelay(classified ClassifiedError, attempt int) int {
	if retryDelayHook != nil {
		return retryDelayHook(classified, attempt)
	}
	safeAttempt := math.Max(1, float64(attempt))
	if classified.Code == "PROVIDER_RATE_LIMITED" {
		base := float64(rateLimitInitialDelayMS) * math.Pow(2, safeAttempt-1)
		delay := base * (1 + rateLimitJitterFactor*rand.Float64())
		return int(math.Min(rateLimitMaxDelayMS, math.Ceil(delay)))
	}
	base := float64(setupRetryInitialDelayMS) * math.Pow(2, safeAttempt-1)
	return int(math.Min(setupRetryMaxDelayMS, base))
}

// emitRetryActivity publishes the retrying phase the renderer shows during a
// backoff. The next MessageStart clears it once the successful attempt starts
// streaming.
func (s *Session) emitRetryActivity(classified ClassifiedError, attempt, delayMS int) {
	activity := &protocol.AgentActivity{
		Phase:        "retrying",
		Since:        time.Now().UnixMilli(),
		Attempt:      attempt,
		RetryDelayMs: int64(delayMS),
		Error: &protocol.AgentActivityError{
			Code:           classified.Code,
			Message:        classified.Message,
			ProviderStatus: providerStatusOf(classified),
		},
	}
	s.setActivityDetails(activity)
}

// waitForRetry waits out the backoff, returning false when the turn was
// cancelled instead.
func waitForRetry(ctx context.Context, delay time.Duration) bool {
	if retryWaitHook != nil {
		return retryWaitHook(ctx, delay)
	}
	if delay <= 0 {
		return ctx.Err() == nil
	}
	timer := time.NewTimer(delay)
	defer timer.Stop()
	select {
	case <-timer.C:
		return true
	case <-ctx.Done():
		return false
	}
}

// claimOverflowCompaction admits one overflow recovery per turn.
func (s *Session) claimOverflowCompaction() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.overflowCompacted {
		return false
	}
	s.overflowCompacted = true
	return true
}

func (s *Session) resetOverflowCompaction() {
	s.mu.Lock()
	s.overflowCompacted = false
	s.mu.Unlock()
}
