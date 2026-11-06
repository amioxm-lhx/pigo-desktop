package jsonrpc

import (
	"context"
	"encoding/json"
	"net"
	"testing"
	"time"
)

func TestCallAndNotification(t *testing.T) {
	clientToServer, serverSide := net.Pipe()
	serverToClient, clientSide := net.Pipe()

	serverConn := NewConn(serverSide, serverToClient)
	clientConn := NewConn(clientSide, clientToServer)

	notified := make(chan string, 1)
	serverConn.Handle("echo", func(_ context.Context, params json.RawMessage) (any, error) {
		var request struct {
			Value string `json:"value"`
		}
		if err := json.Unmarshal(params, &request); err != nil {
			return nil, err
		}
		return map[string]string{"echo": request.Value}, nil
	})
	serverConn.HandleNotification("note", func(params json.RawMessage) {
		var request struct {
			Value string `json:"value"`
		}
		_ = json.Unmarshal(params, &request)
		notified <- request.Value
	})

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

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	raw, err := clientConn.Call(ctx, "echo", map[string]string{"value": "hello"})
	if err != nil {
		t.Fatalf("Call: %v", err)
	}
	var reply map[string]string
	if err := json.Unmarshal(raw, &reply); err != nil {
		t.Fatalf("decode reply: %v", err)
	}
	if reply["echo"] != "hello" {
		t.Fatalf("echo = %q, want hello", reply["echo"])
	}

	if err := clientConn.Notify("note", map[string]string{"value": "ping"}); err != nil {
		t.Fatalf("Notify: %v", err)
	}
	select {
	case value := <-notified:
		if value != "ping" {
			t.Fatalf("notification = %q, want ping", value)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("notification was not delivered")
	}
}

func TestUnknownMethodReturnsMethodNotFound(t *testing.T) {
	clientToServer, serverSide := net.Pipe()
	serverToClient, clientSide := net.Pipe()
	serverConn := NewConn(serverSide, serverToClient)
	clientConn := NewConn(clientSide, clientToServer)
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

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	_, err := clientConn.Call(ctx, "missing", map[string]any{})
	if err == nil {
		t.Fatal("want an error for an unknown method")
	}
	rpcErr, ok := err.(*Error)
	if !ok || rpcErr.Code != CodeMethodNotFound {
		t.Fatalf("error = %#v, want method-not-found", err)
	}
}
