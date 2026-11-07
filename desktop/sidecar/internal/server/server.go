// Package server implements the requests Electron main sends to the sidecar.
package server

import (
	"context"
	"encoding/json"
	"log"
	"strings"
	"sync"

	"github.com/smallnest/pigo/desktop/sidecar/internal/jsonrpc"
	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	sidecarruntime "github.com/smallnest/pigo/desktop/sidecar/internal/runtime"
)

// Options configures a Server.
type Options struct {
	Version string
	Logger  *log.Logger
	Host    sidecarruntime.Host
}

// Server holds the sidecar's cross-session state.
type Server struct {
	version string
	logger  *log.Logger
	host    sidecarruntime.Host
	manager *sidecarruntime.Manager
	btw     *sidecarruntime.SideRunner
	dream   *sidecarruntime.DreamRunner

	mu     sync.RWMutex
	config protocol.ConfigureParams
}

// New creates a Server.
func New(options Options) *Server {
	return &Server{
		version: options.Version,
		logger:  options.Logger,
		host:    options.Host,
		manager: sidecarruntime.NewManager(sidecarruntime.Options{
			Host:   options.Host,
			Logger: options.Logger,
		}),
		btw:   sidecarruntime.NewSideRunner(options.Host, options.Logger),
		dream: sidecarruntime.NewDreamRunner(options.Host, options.Logger),
	}
}

// Register wires the minimal method set onto conn.
func (s *Server) Register(conn *jsonrpc.Conn) {
	emitter := newEventEmitter(conn)
	s.manager.SetEmitter(emitter)
	s.btw.SetEmitter(emitter)

	conn.Handle(protocol.MethodSidecarConfigure, s.handleConfigure)
	conn.Handle(protocol.MethodSidecarHealth, s.handleHealth)
	conn.Handle(protocol.MethodAgentGetStatus, s.handleGetStatus)
	conn.Handle(protocol.MethodAgentDisposeSession, s.handleDisposeSession)
	conn.Handle(protocol.MethodAgentRebuild, s.handleRebuild)
	conn.Handle(protocol.MethodAgentPrompt, s.handlePrompt)
	conn.Handle(protocol.MethodAgentSteer, s.handleSteer)
	conn.Handle(protocol.MethodAgentAbort, s.handleAbort)
	conn.Handle(protocol.MethodAgentStop, s.handleStop)
	conn.Handle(protocol.MethodAskToolResolve, s.handleAskToolResolve)
	conn.Handle(protocol.MethodAgentCompact, s.handleCompact)
	conn.Handle(protocol.MethodAgentBtw, s.handleBtw)
	conn.Handle(protocol.MethodAgentBtwCancel, s.handleBtwCancel)
	conn.Handle(protocol.MethodAgentExecutePlan, s.handleExecuteApprovedPlan)
	conn.Handle(protocol.MethodPromptEnhance, s.handlePromptEnhance)
	conn.Handle(protocol.MethodSessionTitle, s.handleSummarizeTitle)
	conn.Handle(protocol.MethodPluginComplete, s.handlePluginComplete)
	conn.Handle(protocol.MethodMemoryReport, s.handleMemoryReport)
	conn.Handle(protocol.MethodAgentDream, s.handleDream)
}

func (s *Server) handleConfigure(_ context.Context, params json.RawMessage) (any, error) {
	config, err := jsonrpc.Decode[protocol.ConfigureParams](params)
	if err != nil {
		return nil, err
	}
	s.mu.Lock()
	s.config = config
	s.mu.Unlock()
	logf(s.logger, "sidecar.configure hostBinary=%q dataDir=%q", config.HostBinary, config.DataDir)
	return protocol.ConfigureResult{OK: true, Mode: "host-proxy"}, nil
}

func (s *Server) handleHealth(_ context.Context, _ json.RawMessage) (any, error) {
	return protocol.HealthResult{
		OK:       true,
		Version:  s.version,
		Runtimes: s.manager.Count(),
	}, nil
}

func (s *Server) handleGetStatus(_ context.Context, params json.RawMessage) (any, error) {
	parsed, err := jsonrpc.Decode[protocol.SessionParams](params)
	if err != nil {
		return nil, err
	}
	sessionID := strings.TrimSpace(parsed.SessionID)
	if sessionID == "" {
		return nil, jsonrpc.InvalidParams("agent.getStatus requires sessionId")
	}
	if session := s.manager.Get(sessionID); session != nil {
		return protocol.AgentStatusResult{Status: session.Status()}, nil
	}
	return protocol.AgentStatusResult{Status: protocol.AgentStatus{SessionID: sessionID}}, nil
}

func (s *Server) handleDisposeSession(_ context.Context, params json.RawMessage) (any, error) {
	parsed, err := jsonrpc.Decode[protocol.SessionParams](params)
	if err != nil {
		return nil, err
	}
	sessionID := strings.TrimSpace(parsed.SessionID)
	if sessionID != "" {
		s.manager.Dispose(sessionID)
	}
	return protocol.OKResult{OK: true}, nil
}

// handleRebuild serves /rebuild. Unlike agent.disposeSession it reloads the
// context immediately, so the next prompt sees the rebuilt transcript even
// when the user does not send anything first.
func (s *Server) handleRebuild(ctx context.Context, params json.RawMessage) (any, error) {
	parsed, err := jsonrpc.Decode[protocol.SessionParams](params)
	if err != nil {
		return nil, err
	}
	sessionID := strings.TrimSpace(parsed.SessionID)
	if sessionID == "" {
		return nil, jsonrpc.InvalidParams("agent.rebuild requires sessionId")
	}
	rebuilt, err := s.manager.Rebuild(ctx, sessionID)
	if err != nil {
		return nil, err
	}
	return protocol.RebuildResult{OK: true, Rebuilt: rebuilt}, nil
}

func (s *Server) handlePrompt(ctx context.Context, params json.RawMessage) (any, error) {
	parsed, err := jsonrpc.Decode[protocol.PromptParams](params)
	if err != nil {
		return nil, err
	}
	if strings.TrimSpace(parsed.SessionID) == "" {
		return nil, jsonrpc.InvalidParams("agent.prompt requires sessionId")
	}
	turnID := strings.TrimSpace(parsed.TurnID)
	if turnID == "" {
		turnID = sidecarruntime.NewTurnID()
	}

	session, err := s.manager.RuntimeFor(ctx, parsed)
	if err != nil {
		return nil, err
	}
	if err := session.Prompt(turnID, parsed.UserMessageID, parsed.Content); err != nil {
		return nil, err
	}
	return map[string]any{"accepted": true, "turnId": turnID}, nil
}

func (s *Server) handleSteer(_ context.Context, params json.RawMessage) (any, error) {
	parsed, err := jsonrpc.Decode[protocol.SteerParams](params)
	if err != nil {
		return nil, err
	}
	sessionID := strings.TrimSpace(parsed.SessionID)
	if sessionID == "" {
		return nil, jsonrpc.InvalidParams("agent.steer requires sessionId")
	}
	session := s.manager.Get(sessionID)
	if session == nil {
		return nil, jsonrpc.AppError("TURN_NOT_FOUND", "No active turn to steer")
	}
	return session.Steer(parsed)
}

func (s *Server) handleAbort(ctx context.Context, params json.RawMessage) (any, error) {
	parsed, err := jsonrpc.Decode[struct {
		SessionID string `json:"sessionId"`
		TurnID    string `json:"turnId"`
	}](params)
	if err != nil {
		return nil, err
	}
	session := s.manager.Get(strings.TrimSpace(parsed.SessionID))
	if parsed.TurnID != "" && session != nil && session.CurrentTurnID() != parsed.TurnID {
		return map[string]any{"ok": false, "aborted": false}, nil
	}
	if s.host != nil && session != nil {
		abortParams := map[string]any{"sessionId": parsed.SessionID}
		if parsed.TurnID != "" {
			abortParams["turnId"] = parsed.TurnID
		}
		if _, err := s.host.Call(ctx, "plans.abort", abortParams); err != nil {
			logf(s.logger, "plans.abort after agent.abort failed: %v", err)
		}
	}
	if session != nil {
		session.Abort()
	}
	return map[string]any{"ok": true}, nil
}

func (s *Server) handleStop(_ context.Context, params json.RawMessage) (any, error) {
	parsed, err := jsonrpc.Decode[protocol.SessionParams](params)
	if err != nil {
		return nil, err
	}
	session := s.manager.Get(strings.TrimSpace(parsed.SessionID))
	if session == nil {
		return protocol.StopResult{Requested: false}, nil
	}
	return session.RequestGracefulStop(), nil
}

func (s *Server) handleAskToolResolve(_ context.Context, params json.RawMessage) (any, error) {
	parsed, err := jsonrpc.Decode[protocol.AskToolParams](params)
	if err != nil {
		return nil, err
	}
	session := s.manager.Get(strings.TrimSpace(parsed.SessionID))
	if session == nil {
		return nil, jsonrpc.AppError("ASKTOOL_NOT_FOUND", "runtime not found for asktool request")
	}
	return session.ResolveAskTool(parsed)
}

func (s *Server) handlePromptEnhance(ctx context.Context, params json.RawMessage) (any, error) {
	parsed, err := jsonrpc.Decode[protocol.EnhancePromptParams](params)
	if err != nil {
		return nil, err
	}
	enhanced, err := sidecarruntime.EnhancePromptDraft(ctx, parsed)
	if err != nil {
		return nil, err
	}
	return map[string]any{"enhancedDraft": enhanced}, nil
}

func (s *Server) handleSummarizeTitle(ctx context.Context, params json.RawMessage) (any, error) {
	parsed, err := jsonrpc.Decode[protocol.SummarizeTitleParams](params)
	if err != nil {
		return nil, err
	}
	title, err := sidecarruntime.SummarizeSessionTitle(ctx, parsed)
	if err != nil {
		return nil, err
	}
	return map[string]any{"title": title}, nil
}

func (s *Server) handlePluginComplete(ctx context.Context, params json.RawMessage) (any, error) {
	parsed, err := jsonrpc.Decode[protocol.PluginCompleteParams](params)
	if err != nil {
		return nil, err
	}
	text, err := sidecarruntime.CompletePlugin(ctx, parsed)
	if err != nil {
		return nil, err
	}
	return map[string]any{"text": text}, nil
}

// handleMemoryReport serves /memory. It is read-only against the transcript: the
// memory index is reconciled and counted, never written.
func (s *Server) handleMemoryReport(_ context.Context, params json.RawMessage) (any, error) {
	parsed, err := jsonrpc.Decode[protocol.MemoryReportParams](params)
	if err != nil {
		return nil, err
	}
	return sidecarruntime.BuildMemoryReport(strings.TrimSpace(parsed.Root)), nil
}

// handleDream serves /dream. The pass can run for minutes (several model calls),
// so it answers once the consolidation finished and reports the change counts;
// the renderer shows a busy state for the duration.
func (s *Server) handleDream(ctx context.Context, params json.RawMessage) (any, error) {
	parsed, err := jsonrpc.Decode[protocol.DreamParams](params)
	if err != nil {
		return nil, err
	}
	report, err := s.dream.Run(ctx, parsed)
	if err != nil {
		return nil, err
	}
	raw, err := json.Marshal(report)
	if err != nil {
		return nil, jsonrpc.AppError("DREAM_ENCODE", "%s", err)
	}
	return protocol.DreamResult{OK: true, Report: raw}, nil
}

// handleBtw admits one /btw side question. Unlike agent.prompt it does not go
// through Manager.RuntimeFor: no Session, no host turn and no durable rows.
func (s *Server) handleBtw(ctx context.Context, params json.RawMessage) (any, error) {
	parsed, err := jsonrpc.Decode[protocol.BtwParams](params)
	if err != nil {
		return nil, err
	}
	sideID, err := s.btw.Ask(ctx, parsed)
	if err != nil {
		return nil, err
	}
	return map[string]any{"accepted": true, "sideId": sideID}, nil
}

func (s *Server) handleBtwCancel(_ context.Context, params json.RawMessage) (any, error) {
	parsed, err := jsonrpc.Decode[protocol.BtwCancelParams](params)
	if err != nil {
		return nil, err
	}
	return map[string]any{"cancelled": s.btw.Cancel(parsed.SideID)}, nil
}

// handleCompact runs one manual context compaction and answers once the
// checkpoint has been installed. The renderer follows compaction_start/end on
// agent.event and reads this reply only to know the request finished.
func (s *Server) handleCompact(ctx context.Context, params json.RawMessage) (any, error) {
	parsed, err := jsonrpc.Decode[protocol.PromptParams](params)
	if err != nil {
		return nil, err
	}
	if strings.TrimSpace(parsed.SessionID) == "" {
		return nil, jsonrpc.InvalidParams("agent.compact requires sessionId")
	}
	session, err := s.manager.RuntimeFor(ctx, parsed)
	if err != nil {
		return nil, err
	}
	if err := session.Compact(ctx); err != nil {
		return nil, err
	}
	return map[string]any{"accepted": true}, nil
}

func (s *Server) handleExecuteApprovedPlan(ctx context.Context, params json.RawMessage) (any, error) {
	parsed, err := jsonrpc.Decode[protocol.ExecuteApprovedPlanParams](params)
	if err != nil {
		return nil, err
	}
	if strings.TrimSpace(parsed.SessionID) == "" || parsed.Execution == nil {
		return nil, jsonrpc.InvalidParams("agent.executeApprovedPlan requires sessionId and execution")
	}
	// Approved execution always runs with the full Agent toolset.
	parsed.Mode = "agent"
	parsed.Content = ""
	parsed.UserMessageID = ""
	turnID := strings.TrimSpace(parsed.TurnID)
	if turnID == "" {
		turnID = sidecarruntime.NewTurnID()
	}
	session, err := s.manager.RuntimeFor(ctx, parsed.PromptParams)
	if err != nil {
		return nil, err
	}
	if err := session.ExecuteApprovedPlan(*parsed.Execution, turnID); err != nil {
		return nil, err
	}
	return map[string]any{"accepted": true, "turnId": turnID}, nil
}

func logf(logger *log.Logger, format string, args ...any) {
	if logger != nil {
		logger.Printf(format, args...)
	}
}
