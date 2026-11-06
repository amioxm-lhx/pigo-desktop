# ADR 0251 — pi-go Go agent sidecar

- **Status**: Accepted for implementation
- **Date**: 2026-09-15
- **Amends in part**: [ADR 0002](0002-use-pi-agent-harness.md) · [ADR 0003](0003-agent-in-main-process.md)
- **Related**: `PORTING-PI-GO.md` (repository root) · [Agent Runtime](../spec/03-runtime/02-agent-runtime.md) · [Process Model](../spec/03-runtime/07-process-model.md)

## Context

This repository is a port of PI-Desktop onto pi-agent-go. The desktop is a
three-layer host: a React renderer, a thin Electron main process, and the Rust
`host-core` that owns persistence, tool execution, permissions, plans, and the
plugin marketplace. The bottom layer — the agent sidecar — was a Node
implementation of the pi Agent Harness (`packages/agent-runtime`) running on
`@earendil-works/pi-agent-core` and `@earendil-works/pi-ai`, as frozen by
ADR 0002 and ADR 0003.

The goal of the port is to run the same desktop on pi-go instead. Two routes
were considered. Route B would rebuild the desktop around pi-go's own host
APIs, discarding `host-core` and the Electron shell. Route A keeps every layer
that already works and replaces only the agent sidecar. Route A was chosen:
`host-core` already owns the data model, tool execution, and permission
gateway, and its contract with the sidecar is a narrow, documented RPC.

## Decision

1. The agent sidecar is a Go program built on pi-go, launched as a child
   process of Electron main. It is the only layer of the desktop that changes.
   The renderer, preload bridge, Electron main process, Rust `host-core`, plugin
   runtime, MCP integration, and updater keep their upstream implementations.
2. The sidecar protocol is frozen verbatim. It remains NDJSON JSON-RPC 2.0 over
   stdio, using the existing `agent.*` request methods, `agent.event`
   notifications, and `host.proxy` callbacks. Electron main and `host-core`
   detect no difference between implementations, so the two can be swapped for
   comparison or rollback without touching either.
3. Session and transcript persistence remain owned exclusively by `host-core`
   SQLite. The Go sidecar does not enable pi-go's `session-backends/sqlite`;
   the desktop transcript is the single source of truth and a second writer
   would double-write and diverge.
4. Tool execution and permission decisions remain owned by `host-core`. The Go
   sidecar advertises the host tool catalog to the model and routes every tool
   call back through the `host.proxy` `tools.execute` request instead of
   running pi-go's built-in tools directly. pi-go tool implementations are used
   only where no host equivalent exists and the host remains authoritative.
5. Electron main resolves the sidecar in this order: an explicit
   `PI_DESKTOP_AGENT_BIN`, then the packaged
   `<resources>/bin/pi-go-desktop-agent`, then the development build under
   `sidecar/build/`. As of M4 there is no second implementation: a build
   without the binary fails at startup instead of falling back, and the
   `PI_DESKTOP_AGENT_IMPL` switch is gone.
6. The sidecar is a nested Go module (`sidecar/go.mod`) that depends on pi-go
   through a `replace` directive. The pi-go module itself stays free of desktop
   concerns, and `go build ./...` at the repository root is unaffected.
7. Trusted JS/TS extensions loaded through `jiti` are not supported by the Go
   sidecar for now; the renderer-side plugin system is unaffected. This is a
   known gap recorded in the porting notes.
8. M4 removed the Node agent process from the tree and from the build.
   `sidecar.ts`, `runtime.ts`, `subagent.ts`, `host-client.ts`, `node-proxy.ts`,
   `parent-host-proxy.ts`, and the extension loader and runner are deleted.
   `packages/agent-runtime` keeps only the pure helper modules Electron main
   imports, and packaging no longer produces or ships a `dist-bundle`. The
   helper package still depends on `@earendil-works/pi-ai` (provider adapters,
   retry classification, OAuth flows) and on `@earendil-works/pi-agent-core`'s
   prompt-template loader, so dropping `@earendil-works/*` entirely remains
   open. The Windows installer is built as `pi-agent-go-desktop-setup.exe`.

## Consequences

- The agent loop, provider streaming, context assembly, compaction, and
  tool-call orchestration move to pi-go. Behavioural parity of the sidecar
  contract, not of internal implementation, becomes the compatibility target.
- Renderer, Electron main, and `host-core` require no behavioural change. The
  desktop's user-visible features, persisted data semantics, and host RPC
  surface stay as upstream.
- The Go sidecar is now the only agent runtime. Regressions are attributed
  against upstream history and the pre-M4 commits rather than by switching
  implementations on one data directory.
- Packaging ships the Go binary and no Node agent runtime. The desktop still
  carries the Node toolchain for Electron and for the helper modules Vite
  bundles into the main chunk.
- pi-go's `ExtensionHost` is a Go rewrite and does not execute JS/TS extension
  code, so upstream trusted extensions are unavailable on the Go path until an
  equivalent host is designed.
- ADR 0002 and ADR 0003 describe the upstream Node sidecar, which no longer
  exists in this tree; `packages/agent-runtime` retains their helper modules
  only.

## Alternatives

- **Route B — reimplement the desktop around pi-go host APIs.** Rejected
  because it discards a working persistence model, permission gateway, plugin
  runtime, and Electron shell, and turns a contained port into a rewrite with
  no incremental validation point.
- **Keep the Node sidecar and call Go for individual capabilities.** Rejected
  because it keeps both agent loops alive and moves the boundary rather than
  removing it.
- **Replace `host-core` with pi-go instead of the sidecar.** Rejected because
  SQLite ownership, tool execution, and the permission gateway are already
  implemented and tested in Rust; replacing them would change the data
  ownership model that ADR 0011/ADR 0014 freeze.
- **Run the Go sidecar as a library inside Electron main.** Rejected because
  agent execution is expected to survive main-process restarts and to be
  supervised as a separate process, as ADR 0003 established.
