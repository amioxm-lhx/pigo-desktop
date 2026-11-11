// Command pigo-mcp-bridge is a pigo plugin that bridges local stdio MCP
// servers into pigo as plugin tools (docs/mcp-local-integration.md §2, 方案 A).
//
// pigo launches this executable from its plugin directory and speaks the
// plugin JSON-RPC protocol over stdio (internal/plugin). The bridge in turn
// acts as an MCP client to each server configured in ~/.pigo/mcp.toml (or
// $PIGO_MCP_CONFIG), launching it as a subprocess and translating between the
// two protocols:
//
//	pigo ──plugin JSON-RPC──▶ pigo-mcp-bridge ──MCP JSON-RPC──▶ local MCP server
//
// Its exposed tools are named mcp_<server>_<tool> (mirroring the desktop
// client's namespace rule) so they can never collide with built-in tools.
//
// Fault tolerance contract: a configured server that fails to connect only
// logs to stderr and contributes no tools — the bridge still serves the rest.
// A tool-call failure degrades to an error tool result, never a panic, because
// a dead bridge means every MCP tool disappears from the agent at once.
//
// Security contract: MCP server subprocesses receive an explicit environment
// allowlist (PATH plus process basics, mirroring the desktop client and ADR
// 0038 §6) plus the env entries declared in the config. A nil child
// environment would inherit the bridge's full parent environment — including
// provider API keys — so the allowlist is mandatory (see config.go).
package main

import (
	"fmt"
	"os"
)

// bridgeVersion is reported in the plugin manifest and the MCP clientInfo.
const bridgeVersion = "0.1.0"

func main() {
	logf := func(format string, args ...any) {
		fmt.Fprintf(os.Stderr, "[pigo-mcp-bridge] "+format+"\n", args...)
	}

	path := ConfigPath()
	cfg, err := loadConfig(path)
	if err != nil {
		// A malformed config is a hard error: exit non-zero so pigo's fault-
		// tolerant plugin discovery logs the skip instead of serving a
		// silently wrong toolset.
		logf("config %s: %v", path, err)
		os.Exit(1)
	}

	b := newBridge(cfg, logf)
	defer b.closeAll()
	b.connectAll()
	b.buildManifest()

	// serve returns after a shutdown notification or stdin EOF; closeAll (also
	// deferred) tears down every MCP server subprocess.
	if err := b.serve(os.Stdin, os.Stdout); err != nil {
		logf("%v", err)
		os.Exit(1)
	}
}
