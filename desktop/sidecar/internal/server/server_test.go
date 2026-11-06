package server

import (
	"context"
	"encoding/json"
	"io"
	"log"
	"net"
	"testing"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/jsonrpc"
	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
)

func TestConfigureHealthAndStatusOverJSONRPC(t *testing.T) {
	clientToServer, serverSide := net.Pipe()
	serverToClient, clientSide := net.Pipe()

	serverConn := jsonrpc.NewConn(serverSide, serverToClient)
	clientConn := jsonrpc.NewConn(clientSide, clientToServer)

	srv := New(Options{Version: "test-version", Logger: log.New(io.Discard, "", 0)})
	srv.Register(serverConn)
	go func() { _ = serverConn.Serve() }()
	go func() { _ = clientConn.Serve() }()
	defer func() {
		clientConn.Close(nil)
		serverConn.Close(nil)
		_ = clientToServer.Close()
		_ = serverSide.Close()
		_ = serverToClient.Close()
		_ = clientSide.Close()
	}()

	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()

	health, err := call[protocol.HealthResult](ctx, clientConn, protocol.MethodSidecarHealth, map[string]any{})
	if err != nil {
		t.Fatalf("health: %v", err)
	}
	if !health.OK || health.Version != "test-version" || health.Runtimes != 0 {
		t.Fatalf("health = %+v", health)
	}

	configured, err := call[protocol.ConfigureResult](ctx, clientConn, protocol.MethodSidecarConfigure, protocol.ConfigureParams{
		HostBinary: "host-core",
		DataDir:    "/tmp",
	})
	if err != nil {
		t.Fatalf("configure: %v", err)
	}
	if !configured.OK || configured.Mode != "host-proxy" {
		t.Fatalf("configure = %+v", configured)
	}

	status, err := call[protocol.AgentStatusResult](ctx, clientConn, protocol.MethodAgentGetStatus, protocol.SessionParams{SessionID: "session-1"})
	if err != nil {
		t.Fatalf("getStatus: %v", err)
	}
	if status.Status.SessionID != "session-1" || status.Status.IsRunning {
		t.Fatalf("status = %+v", status.Status)
	}

	if _, err := call[protocol.OKResult](ctx, clientConn, protocol.MethodAgentDisposeSession, protocol.SessionParams{SessionID: "session-1"}); err != nil {
		t.Fatalf("disposeSession: %v", err)
	}

	stop, err := call[protocol.StopResult](ctx, clientConn, protocol.MethodAgentStop, protocol.SessionParams{SessionID: "session-1"})
	if err != nil {
		t.Fatalf("stop: %v", err)
	}
	if stop.Requested {
		t.Fatalf("stop without a runtime = %+v, want requested=false", stop)
	}

	if _, err := call[map[string]any](ctx, clientConn, protocol.MethodAgentSteer, protocol.SteerParams{
		SessionID:      "session-1",
		ExpectedTurnID: "turn-1",
		Content:        "steer",
	}); err == nil {
		t.Fatal("steer without a runtime should fail")
	}

	if _, err := call[protocol.OKResult](ctx, clientConn, protocol.MethodAskToolResolve, protocol.AskToolParams{
		SessionID: "session-1",
		RequestID: "ask-1",
		Answers:   []json.RawMessage{json.RawMessage(`["A"]`)},
	}); err == nil {
		t.Fatal("asktool.resolve without a runtime should fail")
	}

	if _, err := call[map[string]any](ctx, clientConn, protocol.MethodAgentExecutePlan, protocol.ExecuteApprovedPlanParams{
		PromptParams: protocol.PromptParams{
			SessionID: "session-1",
			Provider:  protocol.ProviderConfig{ID: "p", ModelID: "m", AuthKind: "none"},
		},
		Execution: &protocol.PlanExecution{
			ID: "exec-1", SessionID: "session-1", Kind: "plan", Plan: "# Plan",
		},
	}); err == nil {
		t.Fatal("executeApprovedPlan without a runtime should fail")
	}
}

func call[T any](ctx context.Context, conn *jsonrpc.Conn, method string, params any) (T, error) {
	var out T
	raw, err := conn.Call(ctx, method, params)
	if err != nil {
		return out, err
	}
	if len(raw) == 0 {
		return out, nil
	}
	if err := json.Unmarshal(raw, &out); err != nil {
		return out, err
	}
	return out, nil
}
