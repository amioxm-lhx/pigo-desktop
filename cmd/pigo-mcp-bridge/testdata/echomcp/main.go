// Command echomcp is a minimal MCP stdio server used to smoke-test
// pigo-mcp-bridge end to end (docs/mcp-local-integration.md §2). It implements
// exactly the protocol surface the bridge exercises:
//
//	initialize                 → protocol 2024-11-05 reply
//	notifications/initialized  → ignored (notification)
//	tools/list                 → one tool: echo
//	tools/call {text}          → text content "echo: <text>"
//
// It is a test fixture, not a product; keep it minimal.
package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
	"strings"
)

type request struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params"`
}

func main() {
	sc := bufio.NewScanner(os.Stdin)
	sc.Buffer(make([]byte, 0, 64*1024), 16*1024*1024)
	out := bufio.NewWriter(os.Stdout)
	defer out.Flush()

	for sc.Scan() {
		line := strings.TrimSpace(sc.Text())
		if line == "" {
			continue
		}
		var r request
		if json.Unmarshal([]byte(line), &r) != nil {
			continue
		}
		if len(r.ID) == 0 {
			continue // notification; nothing to answer
		}
		switch r.Method {
		case "initialize":
			reply(out, r.ID, map[string]any{
				"protocolVersion": "2024-11-05",
				"capabilities":    map[string]any{"tools": map[string]any{}},
				"serverInfo":      map[string]any{"name": "echomcp", "version": "0.0.1"},
			})
		case "tools/list":
			reply(out, r.ID, map[string]any{
				"tools": []map[string]any{{
					"name":        "echo",
					"description": "Echo the given text back.",
					"inputSchema": map[string]any{
						"type":       "object",
						"properties": map[string]any{"text": map[string]any{"type": "string"}},
						"required":   []string{"text"},
					},
				}},
			})
		case "tools/call":
			var p struct {
				Name      string          `json:"name"`
				Arguments json.RawMessage `json:"arguments"`
			}
			_ = json.Unmarshal(r.Params, &p)
			var args struct {
				Text string `json:"text"`
			}
			_ = json.Unmarshal(p.Arguments, &args)
			reply(out, r.ID, map[string]any{
				"content": []map[string]any{{"type": "text", "text": "echo: " + args.Text}},
			})
		default:
			_ = json.NewEncoder(out).Encode(map[string]any{
				"jsonrpc": "2.0",
				"id":      json.RawMessage(r.ID),
				"error":   map[string]any{"code": -32601, "message": "method not supported"},
			})
		}
	}
}

func reply(out *bufio.Writer, id json.RawMessage, result any) {
	data, err := json.Marshal(map[string]any{
		"jsonrpc": "2.0",
		"id":      json.RawMessage(id),
		"result":  result,
	})
	if err != nil {
		fmt.Fprintln(os.Stderr, "echomcp: marshal reply:", err)
		return
	}
	out.Write(data)
	out.WriteByte('\n')
	out.Flush()
}
