// This file implements the Streamable HTTP transport for MCP servers
// (docs/mcp-local-integration.md §3.1 transport_http.go), the second transport
// beside stdio. The protocol shape (2024-11-05): every JSON-RPC message is a
// POST with Accept: application/json, text/event-stream; the server replies
// either with one application/json body, or with an SSE stream whose data
// events carry the reply, or 202 for notifications. The initialize reply may
// carry an Mcp-Session-Id header that all subsequent requests must echo.
//
// The security gate (§3.7.4) lives at dial time: https:// by default; plain
// http:// only when the host is loopback (127.x/::1/localhost) or the entry
// opts in with allow_insecure = true. The gate runs BEFORE any network I/O so
// a misconfigured remote http endpoint fails fast, not after connecting.
package mcp

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"strings"
	"sync"
	"time"
)

// sseLineLimit bounds one SSE line (and one JSON body) at the same 16MB cap
// the stdio transport uses, so a hostile server cannot balloon memory.
const sseLineLimit = 16 * 1024 * 1024

// httpClientTimeout is the dial/TLS handshake window; per-call deadlines ride
// the context instead, because SSE reply streams have no fixed duration.
const httpDialTimeout = 10 * time.Second

// httpRPC is an rpcClient over Streamable HTTP. It is safe for sequential use
// from the Client (which never issues concurrent calls); the mutex guards the
// request-id counter and the session id learned at initialize.
type httpRPC struct {
	url     string
	client  *http.Client
	mu      sync.Mutex
	nextID  int64
	session string // Mcp-Session-Id from the initialize reply, if any
}

// dialHTTP validates the endpoint against the §3.7.4 gate and returns the
// transport. No network I/O happens here beyond the gate's URL parse.
func dialHTTP(cfg Server) (*httpRPC, error) {
	if err := checkEndpointSecurity(cfg.URL, cfg.AllowInsecure()); err != nil {
		return nil, err
	}
	u, err := url.Parse(cfg.URL)
	if err != nil {
		return nil, fmt.Errorf("mcp %q: bad url %q: %w", cfg.ID, cfg.URL, err)
	}
	return &httpRPC{
		url: u.String(),
		client: &http.Client{
			Timeout: httpDialTimeout, // dial/TLS only guard; calls use ctx deadlines
		},
	}, nil
}

// AllowInsecure reports the per-server opt-out from the https-only gate
// (allow_insecure in mcp.toml; nil = false).
func (s Server) AllowInsecure() bool { return s.AllowInsecureFlag != nil && *s.AllowInsecureFlag }

// checkEndpointSecurity is the gate: https always allowed; http only for
// loopback hosts or an explicit allow_insecure opt-in; anything else refused.
func checkEndpointSecurity(rawURL string, allowInsecure bool) error {
	u, err := url.Parse(strings.TrimSpace(rawURL))
	if err != nil {
		return fmt.Errorf("mcp: bad url %q: %w", rawURL, err)
	}
	switch u.Scheme {
	case "https":
		return nil
	case "http":
		host := u.Hostname()
		if isLoopbackHost(host) || allowInsecure {
			return nil
		}
		return fmt.Errorf("mcp: url %q is plain http to a non-loopback host; use https:// or set allow_insecure = true on the server entry", rawURL)
	default:
		return fmt.Errorf("mcp: url %q has unsupported scheme %q (use https:// or a loopback http://)", rawURL, u.Scheme)
	}
}

// isLoopbackHost reports whether a URL hostname is loopback: an IP in the
// loopback range (127.0.0.0/8, ::1) or the literal name "localhost".
func isLoopbackHost(host string) bool {
	if strings.EqualFold(host, "localhost") {
		return true
	}
	ip := net.ParseIP(host)
	return ip != nil && ip.IsLoopback()
}

// rpcRequest is the JSON-RPC 2.0 envelope for requests.
type rpcRequest struct {
	JSONRPC string `json:"jsonrpc"`
	ID      int64  `json:"id"`
	Method  string `json:"method"`
	Params  any    `json:"params,omitempty"`
}

// rpcResponse is the reply envelope; exactly one of Result/Error is set.
type rpcResponse struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Result  json.RawMessage `json:"result"`
	Error   *rpcError       `json:"error"`
}

// rpcError is the JSON-RPC error object.
type rpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

func (e *rpcError) Error() string {
	return fmt.Sprintf("rpc error %d: %s", e.Code, e.Message)
}

// Call posts one JSON-RPC request and extracts the reply, whether the server
// answered with plain JSON or an SSE stream.
func (h *httpRPC) Call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	h.mu.Lock()
	h.nextID++
	id := h.nextID
	session := h.session
	h.mu.Unlock()

	body, err := json.Marshal(rpcRequest{JSONRPC: "2.0", ID: id, Method: method, Params: params})
	if err != nil {
		return nil, fmt.Errorf("mcp http: encode request: %w", err)
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, h.url, bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json, text/event-stream")
	req.Header.Set("MCP-Protocol-Version", ProtocolVersion)
	if session != "" {
		req.Header.Set("Mcp-Session-Id", session)
	}
	resp, err := h.client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("mcp http: %s: %w", method, err)
	}
	defer resp.Body.Close()
	// The initialize reply may assign a session; remember it for every later
	// request (and tolerate servers that do not use sessions).
	if sid := resp.Header.Get("Mcp-Session-Id"); sid != "" {
		h.mu.Lock()
		h.session = sid
		h.mu.Unlock()
	}
	switch {
	case resp.StatusCode >= 200 && resp.StatusCode < 300:
		// fall through to body handling
	case resp.StatusCode == http.StatusAccepted:
		return nil, fmt.Errorf("mcp http: unexpected 202 for request %s", method)
	default:
		excerpt, _ := io.ReadAll(io.LimitReader(resp.Body, 4096))
		return nil, fmt.Errorf("mcp http: %s: status %d: %s", method, resp.StatusCode, strings.TrimSpace(string(excerpt)))
	}

	ct := strings.ToLower(strings.TrimSpace(strings.SplitN(resp.Header.Get("Content-Type"), ";", 2)[0]))
	switch ct {
	case "application/json":
		return decodeRPCResponse(resp.Body)
	case "text/event-stream":
		return decodeSSE(ctx, resp.Body, id)
	default:
		// Some servers answer 200 with an empty body for notifications only; a
		// request without a body has no reply to decode.
		return nil, fmt.Errorf("mcp http: %s: unexpected content-type %q", method, ct)
	}
}

// decodeRPCResponse parses one application/json JSON-RPC envelope.
func decodeRPCResponse(r io.Reader) (json.RawMessage, error) {
	data, err := io.ReadAll(io.LimitReader(r, sseLineLimit))
	if err != nil {
		return nil, fmt.Errorf("mcp http: read body: %w", err)
	}
	var env rpcResponse
	if err := json.Unmarshal(data, &env); err != nil {
		return nil, fmt.Errorf("mcp http: decode reply: %w", err)
	}
	return extractResult(&env)
}

// extractResult turns a decoded envelope into the result payload, mapping a
// JSON-RPC error object onto a Go error.
func extractResult(env *rpcResponse) (json.RawMessage, error) {
	if env.Error != nil {
		return nil, env.Error
	}
	return env.Result, nil
}

// decodeSSE reads an SSE reply stream and returns the data payload whose
// JSON-RPC id matches the request. The stream ends when the server closes it
// (the single-response case) or the context is cancelled; a server that keeps
// the stream open without our reply blocks until the caller's deadline, which
// is the init/call timeout.
func decodeSSE(ctx context.Context, r io.Reader, id int64) (json.RawMessage, error) {
	type resultOrErr struct {
		raw json.RawMessage
		err error
	}
	done := make(chan resultOrErr, 1)
	go func() {
		sc := bufio.NewScanner(r)
		sc.Buffer(make([]byte, 0, 64*1024), sseLineLimit)
		for sc.Scan() {
			line := strings.TrimSpace(sc.Text())
			if line == "" || strings.HasPrefix(line, ":") || strings.HasPrefix(line, "event:") {
				continue
			}
			payload, ok := strings.CutPrefix(line, "data:")
			if !ok {
				continue
			}
			payload = strings.TrimSpace(payload)
			if payload == "" || payload == "[DONE]" {
				continue
			}
			var env rpcResponse
			if err := json.Unmarshal([]byte(payload), &env); err != nil {
				// Not a JSON-RPC envelope (progress notifications ride the same
				// stream); skip and keep scanning.
				continue
			}
			if env.ID == nil {
				continue // server-initiated notification; not our reply
			}
			var gotID int64
			if err := json.Unmarshal(env.ID, &gotID); err != nil || gotID != id {
				continue // a different request's reply; skip
			}
			raw, err := extractResult(&env)
			done <- resultOrErr{raw, err}
			return
		}
		if err := sc.Err(); err != nil {
			done <- resultOrErr{nil, fmt.Errorf("mcp http: sse read: %w", err)}
			return
		}
		done <- resultOrErr{nil, fmt.Errorf("mcp http: sse stream ended without a reply")}
	}()
	select {
	case res := <-done:
		return res.raw, res.err
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}

// Notify posts a notification: no reply is expected, any 2xx counts.
func (h *httpRPC) Notify(method string, params any) error {
	body, err := json.Marshal(map[string]any{
		"jsonrpc": "2.0",
		"method":  method,
		"params":  params,
	})
	if err != nil {
		return err
	}
	ctx, cancel := context.WithTimeout(context.Background(), httpDialTimeout)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, h.url, bytes.NewReader(body))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json, text/event-stream")
	req.Header.Set("MCP-Protocol-Version", ProtocolVersion)
	h.mu.Lock()
	session := h.session
	h.mu.Unlock()
	if session != "" {
		req.Header.Set("Mcp-Session-Id", session)
	}
	resp, err := h.client.Do(req)
	if err != nil {
		return fmt.Errorf("mcp http: notify %s: %w", method, err)
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		excerpt, _ := io.ReadAll(io.LimitReader(resp.Body, 4096))
		return fmt.Errorf("mcp http: notify %s: status %d: %s", method, resp.StatusCode, strings.TrimSpace(string(excerpt)))
	}
	return nil
}

// Close terminates the HTTP session (DELETE per the Streamable HTTP
// spec). Best effort: an error is reported but the transport is done either
// way.
func (h *httpRPC) Close() error {
	h.mu.Lock()
	session := h.session
	h.mu.Unlock()
	if session == "" {
		return nil
	}
	ctx, cancel := context.WithTimeout(context.Background(), httpDialTimeout)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodDelete, h.url, nil)
	if err != nil {
		return err
	}
	req.Header.Set("Mcp-Session-Id", session)
	resp, err := h.client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	return nil
}
