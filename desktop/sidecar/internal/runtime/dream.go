package runtime

import (
	"context"
	"errors"
	"log"
	"strings"
	"sync"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/jsonrpc"
	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/dream"
	"github.com/smallnest/pigo/internal/session"
)

// DreamRunner executes the /dream memory-consolidation pass for the desktop.
//
// The CLI runs dream as a separate process (`pigo --dream`) so a long
// consolidation pass cannot disturb the interactive session. The desktop has no
// such binary to spawn — the sidecar is its own entry point — so the pass runs
// in-process instead, guarded by a single-flight latch. What made the CLI's
// subprocess isolation valuable is still true here: dream builds its own model
// client, never touches a live Session's AgentContext, and serializes against
// itself through dream's on-disk lock. The one thing the desktop must add is the
// latch, because two concurrent GUI clicks would otherwise both reach for that
// lock and make the second one silently "skip".
type DreamRunner struct {
	host   Host
	logger *log.Logger

	// MemoryRoot overrides the resolved root (tests only).
	MemoryRoot string

	mu      sync.Mutex
	running bool
}

// NewDreamRunner creates the runner. host supplies the transcript source so the
// distillation pass sees the desktop's own sessions, which live in host-core's
// SQLite rather than in pigo's file session store.
func NewDreamRunner(host Host, logger *log.Logger) *DreamRunner {
	return &DreamRunner{host: host, logger: logger}
}

// Run executes one consolidation pass and returns the change report.
//
// The provider is the session's own launch binding, so dream consolidates with
// the model the user is actually talking to rather than a second configuration.
func (r *DreamRunner) Run(ctx context.Context, params protocol.DreamParams) (dream.Report, error) {
	binding := params.Provider
	if strings.TrimSpace(binding.ModelID) == "" {
		return dream.Report{}, jsonrpc.AppError("MODEL_NOT_CONFIGURED", "model/provider not configured")
	}
	if strings.TrimSpace(binding.APIKey) == "" && binding.AuthKind != "none" {
		return dream.Report{}, jsonrpc.AppError("AUTH_REQUIRED", "no credential available for this provider")
	}
	if !r.begin() {
		return dream.Report{}, jsonrpc.AppError("DREAM_BUSY", "a dream pass is already running")
	}
	defer r.end()

	consolidator, err := dream.NewLLMConsolidator(
		binding.ModelID,
		providerBaseURL(binding),
		dreamProtocol(binding),
		// A provider name is deliberately not forwarded: the desktop's provider
		// rows are user-defined ids that pigo's registry does not know, and
		// ResolveProvider would reject an unknown name. Protocol + base URL are
		// enough to build the same wire client the session itself uses.
		"",
		binding.APIKey,
		normalizeThinkingLevel(params.ThinkingLevel),
	)
	if err != nil {
		return dream.Report{}, jsonrpc.AppError("DREAM_PROVIDER", "%s", err)
	}

	runner := &dream.Runner{
		Consolidator: consolidator,
		MemoryRoot:   r.MemoryRoot,
		Sessions:     hostSessionSource{ctx: ctx, host: r.host},
	}
	report, err := runner.Run(ctx, dream.RunOptions{
		DryRun:     params.DryRun,
		ProjectDir: strings.TrimSpace(params.ProjectPath),
	})
	if err != nil {
		logf(r.logger, "agent.dream failed: %v", err)
		return dream.Report{}, jsonrpc.AppError("DREAM_FAILED", "%s", err)
	}
	logf(r.logger, "agent.dream dryRun=%v merged=%d pruned=%d distilled=%d", report.DryRun, report.Merged, report.Pruned, report.Distilled)
	return report, nil
}

func (r *DreamRunner) begin() bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.running {
		return false
	}
	r.running = true
	return true
}

func (r *DreamRunner) end() {
	r.mu.Lock()
	r.running = false
	r.mu.Unlock()
}

// dreamProtocol maps the desktop's api style onto pigo's wire selector. The
// sidecar's own resolveAPIStyle collapses "openai/resp_api" to "responses",
// which ResolveProvider does not recognize, so the value is expanded back here.
func dreamProtocol(cfg protocol.ProviderConfig) string {
	switch resolveAPIStyle(cfg.APIStyle, cfg.ModelConfig) {
	case "anthropic":
		return "anthropic"
	case "responses":
		return "openai/resp_api"
	default:
		return "openai"
	}
}

// hostSessionSource adapts host-core's session RPCs to dream.SessionSource so
// the distillation pass reads the desktop's transcripts. Without it dream would
// only ever see pigo's file store (~/.pigo/sessions), which the desktop never
// writes — the pass would then report distilled=0 forever.
type hostSessionSource struct {
	ctx  context.Context
	host Host
}

type hostSessionSummary struct {
	ID          string `json:"id"`
	ProjectPath string `json:"projectPath"`
	UpdatedAt   string `json:"updatedAt"`
}

type hostSessionMessage struct {
	Role             string `json:"role"`
	Content          string `json:"content"`
	ParentToolCallID string `json:"parentToolCallId"`
	Status           string `json:"status"`
}

func (s hostSessionSource) List() ([]session.SessionHeader, error) {
	if s.host == nil {
		return nil, errors.New("dream: no host connection")
	}
	var out struct {
		Sessions []hostSessionSummary `json:"sessions"`
	}
	if err := s.host.CallInto(s.ctx, "session.list", map[string]any{}, &out); err != nil {
		return nil, err
	}
	headers := make([]session.SessionHeader, 0, len(out.Sessions))
	for _, item := range out.Sessions {
		if strings.TrimSpace(item.ID) == "" {
			continue
		}
		headers = append(headers, session.SessionHeader{
			ID:        item.ID,
			UpdatedAt: parseHostTime(item.UpdatedAt),
			// Cwd is what dream matches a project against; the desktop records
			// the same value as the session's project path.
			Cwd: item.ProjectPath,
		})
	}
	return headers, nil
}

func (s hostSessionSource) Load(id string) (session.SessionHeader, agentcore.MessageList, error) {
	if s.host == nil {
		return session.SessionHeader{}, nil, errors.New("dream: no host connection")
	}
	var out struct {
		Session struct {
			ID              string               `json:"id"`
			ProjectPath     string               `json:"projectPath"`
			UpdatedAt       string               `json:"updatedAt"`
			Messages        []hostSessionMessage `json:"messages"`
			ParentSessionID string               `json:"parentSessionId"`
		} `json:"session"`
	}
	if err := s.host.CallInto(s.ctx, "session.get", map[string]any{"id": id}, &out); err != nil {
		return session.SessionHeader{}, nil, err
	}
	header := session.SessionHeader{
		ID:        out.Session.ID,
		UpdatedAt: parseHostTime(out.Session.UpdatedAt),
		Cwd:       out.Session.ProjectPath,
	}
	var messages agentcore.MessageList
	for _, item := range out.Session.Messages {
		content := strings.TrimSpace(item.Content)
		if content == "" {
			continue
		}
		// Subagent rows belong to a delegate's context, not the parent
		// conversation dream distills.
		if strings.TrimSpace(item.ParentToolCallID) != "" {
			continue
		}
		switch item.Role {
		case protocol.RoleUser:
			messages = append(messages, agentcore.UserMessage{
				RoleField: agentcore.RoleUser,
				Content:   agentcore.ContentList{agentcore.NewTextContent(content)},
			})
		case protocol.RoleAssistant:
			messages = append(messages, agentcore.AssistantMessage{
				RoleField: agentcore.RoleAssistant,
				Content:   agentcore.ContentList{agentcore.NewTextContent(content)},
			})
		}
	}
	return header, messages, nil
}

// parseHostTime accepts the ISO-8601 timestamps host-core stores and degrades to
// the zero time (which dream treats as "no update information") rather than
// failing a whole listing over one malformed row.
func parseHostTime(value string) time.Time {
	value = strings.TrimSpace(value)
	if value == "" {
		return time.Time{}
	}
	if parsed, err := time.Parse(time.RFC3339, value); err == nil {
		return parsed
	}
	return time.Time{}
}
