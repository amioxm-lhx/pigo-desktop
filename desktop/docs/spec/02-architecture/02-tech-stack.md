# 02. Tech Stack

## 1. Stack table

| Layer | Tech | Baseline | Notes |
|---|---|---|---|
| Desktop shell | Electron | latest stable (pin at impl) | app shell |
| UI | React 19 + TypeScript | modern stable | English-first UI |
| Bundler | electron-vite / Vite | stable | multi-entry build |
| Styling | Tailwind CSS 4 | stable | utility-first |
| State | Zustand | stable | UI state |
| Host backend | **Rust** | stable Rust toolchain | tools/plugins/permissions/persistence adapters |
| Rust async | tokio | stable | host services |
| Host RPC | stdio JSON-RPC (NDJSON) | frozen (D001) | Electron main ↔ Rust host |
| Agent engine | pi-go `agent` + `agent/harness` (Go) | module pin in `pi-go-desktop/sidecar/go.mod` | agent loop; the only agent implementation ([ADR 0251](../../adr/0251-pi-go-agent-sidecar.md)) |
| Model API | pi-go `ai` (Go) | module pin in `pi-go-desktop/sidecar/go.mod` | provider adapters, OAuth, and stream handling |
| Agent engine (upstream) | `@earendil-works/pi-agent-core` | 0.85.1 | prompt-template loading only; no agent loop runs from it |
| Model API (upstream) | `@earendil-works/pi-ai` | 0.85.1 | helper modules Electron main bundles (one-shot complete, provider retry and headers, OAuth) |
| Model catalog | `https://models.dev/api.json` | bundled release snapshot + process-local refresh | sole provider/model metadata source |

> pi-ai is not consulted for model names, capabilities, limits, modalities,
> thinking levels, or prices. It remains the request transport dependency.
> ChatGPT Plus/Pro and GitHub Copilot OAuth availability still comes from the
> pinned pi-ai catalog; 0.85.1 is the first pin that lists `gpt-6-astra`.

| Node runtime | Node.js | `>= 22.19` | pi requirement |
| DB | SQLite | Rust host-core via `rusqlite` | sessions/settings |
| Packaging | electron-builder | stable | macOS arm64 and Intel x64, Windows x64, and Linux x64 release lanes |
| Package manager | pnpm | 11.18.x | JS monorepo |
| Lint/test | style-token checker (`scripts/check-style-tokens.mjs`) + vitest + cargo test; general JS linter still open (biome vs oxlint) | stable | dual stack quality |
| Schema (TS) | typebox | frozen (D011) | shared contracts |
| i18n | i18next + react-i18next | frozen (D012) | English source locale |

## 2. Language policy in engineering

- Product strings: English source
- Specs/ADRs: English primary
- Code identifiers: English
- Commits/issues/PRs: English preferred

## 3. Why Rust host core

- stronger sandboxing foundation
- better process/fs control
- long-term native performance and safety
- cleaner privilege separation from UI and model runtime

## 4. Why keep pi in Node/TS

This is the upstream rationale. The ported tree runs the sidecar in Go
(ADR 0251) and ships no Node agent process, so the points below describe the
pi software the port still depends on rather than a runtime that is present.

- mature multi-provider support
- existing agent event model
- skills/extensions ecosystem leverage
- avoid rewriting agent framework

### Why the port moves the sidecar to Go

- the agent engine is reused from pi-go instead of reimplemented
- one language for the agent loop, provider adapters, and harness
- the sidecar protocol is narrow enough to keep the swap isolated
- the removed Node sidecar stays in upstream history for comparison

The tradeoff is no JS/TS trusted extension support on the Go path
(jiti-loaded extensions were upstream-only), and the port still carries the
Node toolchain for Electron and its bundled helper packages.

## 5. Dependency boundaries

### Allowed
- official pi packages
- mainstream Electron/React ecosystem
- Rust crates for fs/process/sqlite/rpc/serde

### Careful
- heavy native node addons
- multiple competing RPC frameworks
- large editor stacks too early (Monaco)

### Not in MVP
- remote gateway frameworks
- marketplace backend
- custom LLM provider SDK replacing pi-ai

### Production packaging boundary

- Renderer-only libraries are development/build dependencies because Vite
  bundles their runtime code and lazy assets into `out/renderer`.
- Electron Main bundles pure-JS workspace packages. Packages that require
  runtime module resolution or a native ABI remain production dependencies;
  the current external set includes only `electron-updater`.
- No Node agent runtime ships. The ported build has no
  `Resources/agent-runtime/sidecar.js` and no `dist-bundle` extra resource; the
  helper modules Electron main imports are bundled into the main chunk by Vite.
- The Go sidecar ships as one target-native binary under `Resources/bin`. It
  links pi-go statically and must not require a Go toolchain on the user's
  machine. Its module cache and build cache are build inputs, not release
  assets.
- The desktop package has no interactive PTY dependency. Agent Bash remains a
  non-interactive runtime capability owned by the agent sidecar.
- Dependency source maps, tests, examples, and declarations are build inputs,
  not release assets. License and notice files remain distributable.
- Lazy renderer capabilities such as Mermaid, KaTeX, and Shiki remain local
  assets; package-size optimization must not introduce runtime CDN fetches.

## 6. Build matrix (MVP)

- JS workspace build (`pnpm`)
- Rust host build (`cargo`)
- integration smoke (`pnpm dev` boots all layers)
