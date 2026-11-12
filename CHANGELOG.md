# Changelog

All notable changes to **pigo** are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

pigo is a Go re-implementation of the [pi](https://pi.dev) AI coding agent — a
command-line coding assistant with both a headless script mode and an
interactive REPL/TUI.

## [Unreleased]

### Added
- **Tool confirmations in the Feishu channel when driving desktop sessions**:
  a high-risk tool used to show its approval dialog only in the desktop window,
  leaving a remote chat with no way to answer. The loopback control endpoint now
  exposes the host's open permission queue (`permissions/pending` / `resolve`),
  and `internal/im` mirrors it into the chat with a new `PermissionPoller`:
  one card per request, `allow-once` / `allow-session` / `deny` mapped to the
  host's decisions, fail-closed on any error or timeout, and cards withdrawn
  when the window answers first (the window's own card is cleared in return).
  `tool_policy = allow | deny` now applies to desktop sessions too.
- **Feishu remote-control channel**: `pigo im serve` drives pigo sessions from
  a Feishu chat over the SDK's WebSocket long connection, so the channel needs
  no public IP, domain or reverse proxy. A chat is bound to one session
  (`/new`, `/resume`, `/sessions`, `/stop`, `/status`, `/model`); one progress
  card is patched in place into the final answer; side-effect tool calls ask for
  confirmation on an interactive card and fail closed when nobody answers. New
  `internal/im` package (Adapter / Router / BindingStore / ApprovalRouter /
  per-chat Queue) sits on `internal/serve`, so no agent loop logic is
  duplicated. Per-chat backlogs are bounded (default 3) and the overflow reply
  is throttled, so a burst of messages yields one "排队太多，稍后再试。" rather
  than a wall of them.
- **Desktop sessions as an IM carrier**: `pigo im serve --session-host desktop`
  attaches to a running pigo desktop app instead of running sessions itself, so
  a request sent from Feishu continues the same conversation the app shows. The
  shell publishes a loopback control endpoint (`control_server.rs`, random port
  plus a per-launch 250-bit token in `.pi-agent-go-desktop/control.json`) that
  exposes only session create/list/get/configure and the turn lifecycle —
  settings, plugins, providers and deletes stay off it. `internal/im` now
  depends on its own `SessionHost`/`AgentSession` interfaces rather than
  `internal/serve` concrete types, with `ServeHost` as the local adapter and
  `internal/im/desktop` as the remote one; progress there is polled from
  `agent/status` and the transcript because the app's event stream is not
  subscribeable from outside.
- **Remote-channel settings page**: Settings → 远程控制 configures the Feishu
  channel without hand-editing TOML. It stores App ID/secret/allow-list in the
  app's data dir (`im-channel.json`, secret never returned to the renderer),
  renders the `config.toml` handed to `pigo im serve`, and supervises that child
  (`remote_channel.rs`): start/stop, resolved binary (explicit path → beside the
  app → PATH), control-endpoint liveness, exit code, and the tail of its
  stdout+stderr — which is where every real connection failure shows up. The
  child is killed on app exit, since it needs the app for sessions.
- **WeChat (ClawBot) remote-control channel**: `pigo im serve --channel wechat`
  drives the same sessions from a personal WeChat chat over Tencent's iLink bot
  protocol — plain HTTPS + JSON with long polling, so no SDK, no public IP and
  none of Feishu's 3-second callback deadline. New `internal/im/wechat` adapter
  reuses the existing `Adapter` / `Approver` interfaces, so `router.go`,
  `queue.go` and the command set are untouched. Two platform limits are absorbed
  there rather than leaking into the UI: **messages cannot be edited**, so
  `Update` returns `im.ErrUpdateUnsupported` and the router sends exactly once at
  `Final`; **there are no interactive buttons**, so a tool approval becomes a
  numbered text prompt answered with 同意 / 本会话允许 / 拒绝 (matched exactly,
  fail-closed on anything else). Per-peer `context_token` is echoed back to keep
  replies threaded, the bot's own messages are filtered out of the update stream,
  and errcode `-14` — a newer login replaced this token — pauses the poller for
  an hour instead of retrying into the same wall. 64-bit message ids are decoded
  as strings, because `any` would round-trip them through `float64` and lose the
  low digits.
- **WeChat pairing**: the bot token only exists as the result of a scan, so
  `pigo im login --channel wechat` runs the QR state machine itself
  (`wait` / `scaned` / `need_verifycode` / `scaned_but_redirect` / `expired` /
  `confirmed`), follows a redirect host mid-flow, renders the QR as a PNG for
  callers that cannot draw block art (`--qr-out`), and writes the credential to
  `$PIGO_HOME/im/wechat.json` (0600, atomic temp+rename).
- **Two channels in Settings → 远程控制**: the page now switches between 飞书 and
  微信 instead of assuming Feishu. The WeChat tab offers pairing (start, show the
  QR, submit the digits when the phone asks, cancel — refused while the channel
  runs, since a new login invalidates the token it is using), the allowed-user
  list, and an API base URL override. Only the selected channel's table is
  written; the token never becomes a settings field, and "configured" means the
  state file says so.

- **Tauri + Rust desktop application**: `desktop/` replaces the Electron shell
  with a Tauri 2 app (`apps/desktop-tauri`), a Rust `host-core` with SQLite
  persistence, a Go sidecar, and shared / i18n / plugin-sdk packages.
- **CLI-parity builtin commands and cross-session lineage**: sixteen
  implemented first-party commands (`/btw`, `/dream`, `/memory`, `/tree`,
  `/rewind`, `/export`, `/rebuild`, `/goal`, `/session`, `/status`, ...),
  session info / tree / side-thread panels, pigo-compatible JSONL export, a DB
  migration, eight-language i18n, and contract tests. ADR 0256 replaces the
  five-command freeze with three guardrails.
- **Pluggable session backends**: a `session.Store` contract with the original
  JSONL store as the default and an authoritative normalized SQLite backend
  (`internal/session/sqlite`), plus conformance suites and cross-process file
  leases.
- **Session service and embeddable web frontend**: `internal/serve` plus
  `internal/web`, an `http.Handler` exposing REST session/runtime controls and
  an SSE run-event stream without owning TLS, auth, or server lifecycle.
- **GitLab merge-request review webhook**: the read-only review-session
  contract from GitHub extended to GitLab (`X-Gitlab-Token` constant-time auth,
  event-UUID replay dedup, draft → ready filtering, optional project filter).
- **Evaluation harness**: `internal/evals` (agent harness, task sets,
  baselines, canonical scoring, summaries, artifacts) and the `pigo evals` CLI.
- **Pluggable telemetry sinks**: the `internal/telemetry` Sink contract with
  in-memory and no-op implementations and conformance tests, consuming the same
  `agentcore.TelemetryEvent` the runtime emits.
- **Path guard for file tools**: `internal/agenttool/pathguard.go` enforcing
  path boundaries on read / write / edit / search.
- Runtime: forward request headers through the provider layer and settle
  no-op compactions.
- Documentation: project analysis and architecture notes, book chapter updates,
  `docs/running-guide.md`, and ADR 0256.

### Changed
- Dropped test files orphaned by the Electron → Tauri migration.

### Fixed
- Desktop: model switching after adding a model in settings.
- Desktop: an answer streamed after a tool call disappeared on reload and the
  turn was reported as unfinished. OpenAI-compatible gateways begin a response
  with its first delta and never emit a start frame, so the sidecar's mapper
  reused the id of the assistant fragment that had just closed; the renderer
  overwrote the older row in place instead of adding one, and the host dropped
  the later snapshot as a replayed id. The mapper now opens every fragment with
  a fresh id, and `OpenAIDecoder` emits `StreamStartEvent` like the other
  decoders.
- Desktop: the MCP and skills pages offered no create button under an empty
  project-level group, and the global group's empty-state button reused the
  filter-driven target, so with the filter on "project" it created into the
  project level. Each group now pins its own level.
- CLI: an MCP server id present in both `mcp.toml` layers was connected twice,
  producing two tools under one `mcp_<server>_<tool>` name. The project layer
  now overrides the global entry in place (matching the desktop client), and its
  file-level `enabled = false` switch is honoured like the global layer's.
- Desktop on Windows: no file in the workspace panel could be opened, while the
  same tree worked on macOS. The allowed roots were kept lexically normalized
  (`D:\...`) but the clicked path was canonicalized (`\\?\D:\...`, and with the
  8.3 short name when it came from `%TEMP%`), so `starts_with` failed and every
  click was answered with "path outside allowed roots". Directory listing was
  unaffected because it never compares against the roots. Both sides now resolve
  through the same canonicalization.
- Desktop: the work panel's maximize toggle did nothing outside the chat page.
  Preview mode hides the center pane, and the guard that restores it before a
  destination is presented tested `page !== "chat"` instead of a page change, so
  entering preview anywhere else was reverted on the next frame — the dock
  flashed open and snapped back. Preview now ends only when the page actually
  changes.

## [0.5.6] - 2026-09-12

### Added
- **Session export and sharing**: `pigo session export` (#570).
- **Online model discovery**: `/models fetch` (#566).
- **Session-local reminder scheduler**: `schedule_create` / `schedule_list` /
  `schedule_delete` (#565).
- **GitHub PR review webhook**: `ready_for_review` events create read-only
  review sessions (HMAC-SHA256 verification, delivery-id replay dedup, read /
  grep / find only) (#567).
- **Credential reference mode**: `credential = "<name>"` resolves against
  `~/.pigo/.credentials.yaml`; `*_API_KEY` / `*_TOKEN` / `*_SECRET` / `PIGO_*`
  are scrubbed from child-process environments (#568).
- **Versioned telemetry export schema** with golden conformance tests (#569).
- Documentation: sandboxing and containerization modes (#571) and a refreshed
  harness capability matrix (#572).

### Fixed
- Restore the three-argument `RegisterLiveCommands` call in `think_test`.

## [0.5.5] - 2026-09-12

### Added
- **Tool-level admission control**: `--allowed-tools` / `--disallowed-tools`
  (repeatable, comma-separated, case-insensitive) narrow the tool set handed to
  the model, filling the gap between the full set and `--no-tools`. Deny wins
  over allow on conflict; task sub-agents inherit the boundary; an unknown tool
  name aborts with exit code 2 instead of being silently ignored. Because
  filtering happens at the tool-registration layer — before the side-effect
  confirmation gate — `--approve` waives confirmation prompts but cannot widen
  the boundary. Also configurable as `allowed_tools` / `disallowed_tools` in
  `config.toml`. (#555)
- **Public Go SDK**: the importable `agent` package with runnable examples
  (#554), plus structured events and custom tools (#563).
- **Provider presets**: Novita AI as a built-in provider (#556); Fable 5 /
  GLM-5.3 / DeepSeek Flash, OpenAI GPT-6/5.6 with MiMo/Grok updates, and a
  Google Gemini preset section.
- Colored diff rendering for `edit` tool results (#560).

### Changed
- Book chapters synced with the refactored `internal/cli/*` layout; follow-up
  review fixes for the SDK (#563).
- Tool-policy docs: clarify deny-wins across tiers and warn when `--no-tools`
  is combined with a policy.

### Fixed
- Bare provider names now resolve to that provider's default model.

## [0.5.4] - 2026-08-02

### Added
- **Background bash execution**: `bash` gains `run_in_background`, plus the
  `bash_output` and `kill_bash` tools to read incremental output and terminate
  background jobs.
- **`/rewind` edit restore points**: restore files changed by `write`/`edit`
  back to the state before a selected turn and fork the conversation there
  (files changed by `bash` are not covered).
- **`websearch` tool**: online search with an automatically selected backend
  (`TAVILY_API_KEY` → Tavily, `BRAVE_API_KEY` → Brave, otherwise keyless
  DuckDuckGo) and `allowed_domains` / `blocked_domains` filters.
- **`max` reasoning level**: a step above `xhigh` for `--thinking-level` and
  `/think`.

## [0.5.3] - 2026-08-02

### Added
- **OpenAI Responses API driver**: non-streaming (#539) and streaming text
  (#540), tool/function calls (#541), and image input + reasoning (#542).
  `openai/resp_api` is routed to the driver (#543), extended OpenAI protocol
  variants are parsed (#538), and DeepSeek now defaults to the Responses
  protocol.
- TUI: the startup banner shows the concrete OpenAI wire variant (#544), plus
  `/status` and `/session` slash commands and surfaced per-turn errors.
- Documentation for the three OpenAI wire variants (#545).

### Fixed
- Normalize `--protocol` before the named-provider conflict check.
- Surface provider errors and empty responses in the interactive REPL loop.

## [0.5.2] - 2026-08-01

### Added
- **`/dream` memory consolidation**: `[dream]` config table + state + due-check
  (#519), single-instance `dream.lock` with stale takeover (#520), deterministic
  consolidation plan and report (#521), a `--dream` subprocess runner with
  dry-run (#522), LLM-driven Consolidator (merge/prune) with writeback and
  Reconcile (#523), distilling new memories from recent session JSONL (#524),
  the `/dream` REPL command with report rendering (#525), and a startup
  background auto-consolidation trigger (#536).
- Bilingual `/dream` documentation and a reconcile validation test (#527).

### Fixed
- Platform-aware shell resolution for the `bash` tool on Windows (#518).

## [0.5.1] - 2026-08-01

### Added
- **Documentation website rebuild**: a static bilingual (zh/en) site skeleton
  with shared CSS + i18n (#497), a marketing homepage with hero/nav/footer
  (#498) and feature sections (#499), plus quickstart (#500), installation
  (#501), features (#502), configuration (#503), slash commands (#504) and
  architecture (#505) pages, navigation verification (#506), and a
  MiMo-inspired visual refresh (#517).
- Custom-domain `CNAME` and PRD/SPEC planning notes for the website and
  persistent memory.

## [0.5.0] - 2026-08-01

### Added
- **Persistent memory and "infinite context"**: SQLite FTS5 storage layer with
  `Store.Open` (#473), nested `[memory]` / `[checkpoint]` config tables and
  `compaction.max_context` (#479), path resolution with scope/type and traversal
  guard (#474), lazy reconcile with size-mtime fingerprints (#475), BM25 search
  with `buildFtsQuery` and a relative score floor (#476), the `memory_search`
  tool (#477), a memory-injection reminder provider with a budget (#478),
  checkpoint persistence + watermark (#480), checkpoint-based context rebuild
  with `/rebuild` and fallback (#482), session context inheritance via
  `contextFrom` / `contextWatermark` (#483), loop wiring with checkpoint recall
  (#481), and the `/memory` command with a memstatus report (#484).
- `/effect` as an alias of `/think`; a TUI compacting indicator and `/exit` /
  `/quit` commands.

### Fixed
- Wire `/remote-control` into the TUI and make LAN IP selection robust.

## [0.4.4] - 2026-08-01

### Added
- **Self-update**: `pigo update` with no package name (or flags-only calls such
  as `pigo update --check`) now upgrades the pigo binary itself to the latest
  GitHub Release, replacing the executable in place (with a `sudo` hint when the
  target path needs elevated permissions). (#465, #466, #468)
- **Startup upgrade hint**: the TUI banner shows the version row and, when a
  newer release is available, a `Run pigo update to upgrade` hint backed by a
  24h cached background release check. (#467)
- Initial CHANGELOG covering v0.1.0 through Unreleased.

### Changed
- **`pigo update` semantics**: a no-argument `pigo update` no longer updates
  every installed package; it now self-updates the binary. Update packages
  individually with `pigo update <name>`. (#468)
- Internationalized the codebase: all user-facing strings and internal comments
  were translated from Chinese to English.
- Documented self-update and the revised `update` semantics in the README and
  the docs site.

## [0.4.3] - 2026-07-31

### Added
- Generic **task tool** for sub-agent fan-out, with a concurrency semaphore and
  a nesting guard; advertised in the system prompt. (#454, #458)
- **Sub-agent progress reporting**: `SubAgentProgressEvent` and a context-carried
  progress emitter (#453), surfaced through a loop-injected emitter (#455),
  printed to stderr in headless mode (#457), and shown in a multi-line sub-agent
  status panel in the TUI. (#456)
- TUI prompt-history navigation, plus sub-agent panel and credential fixes.
- Subagent-orchestration PRD and SPEC.

## [0.4.2] - 2026-07-31

### Added
- **Remote control**: pair a phone/browser with a running REPL over LAN. Includes
  a pairing token and session credential store (#438), LAN address / free-port
  resolver (#439), terminal Unicode QR renderer (#440), HTTP + WebSocket server
  (#441), a REPL bridge seam (#442), a responsive web SPA (#444), and REPL
  wiring with server hardening. (#443, #445)
- `--cwd`/`-C` flag to run pigo as if launched in a given working directory.
- `/think` slash command to switch reasoning effort at runtime.

### Fixed
- Avoid a slice panic when persisting a session after compaction.
- Auto-scroll the TUI to the newest output on submit.

## [0.4.1] - 2026-07-30

### Added
- **User-extensible hooks**: core `internal/hooks` infrastructure (#417), layered
  `ConfigLayer.Hooks` with append-merge (#418), an `InstallHooks` assembly helper
  (#419), and wiring for PreToolUse/PostToolUse (#420), UserPromptSubmit with
  block + one-shot injection (#421), Stop/SubagentStop (#422), SessionStart (#423),
  and a `HookNotifier` for observer events (#424), converged across all six
  drivers (#425).
- Runnable hook examples, README Hooks section, and security notes (#426).
- `config.toml.example` template and a TUI screenshot for the README.

### Fixed
- Bound the hook runner timeout with `WaitDelay` so orphaned grandchildren can't
  block `Run`. (#437)
- Show tool arguments in the TUI tool-card header.

## [0.4.0] - 2026-07-29

### Added
- **Full-screen TUI** (Bubble Tea v2): skeleton with default entry gating (#384),
  theme layer and width-aware rendering (#385), a persistent bottom status bar
  (#386), an AgentEvent→`tea.Msg` bridge (#387), streaming transcript with
  viewport scrolling (#388), rich tool-call cards (#389), multi-line input with
  CJK editing and two-stage interrupt (#390), slash commands with an autocomplete
  popup (#391), and real run-seam binding with session resume/persistence. (#392)
- Startup logo + config splash, image paste (Ctrl+V/Cmd+V), multi-line paste
  collapsed into a placeholder, and mouse-selection copy.
- lipgloss v2 upgrade; slash-command registry sunk into `internal/cli/prompts`. (#393)
- TUI agent PRD and SPEC.

### Fixed
- Numerous TUI rendering fixes: scrollbar visibility and alignment across
  reflows, Shift+Enter newline reliability (with Ctrl+J/Alt+Enter fallbacks),
  CJK/IME input, terminal query-reply leakage into the input box, and blank-line
  turn separation. (#403–#416)

## [0.3.7] - 2026-07-28

### Added
- **Prompt templates**: shell-style arg tokenizer (#344) and expansion engine
  (#332) with tiered slash-command priority (#335), argument-hint frontmatter
  (#334), `--prompt-template`/`--no-prompt-templates` flags (#339), settings-tier
  `prompts` from `config.toml` (#338), project-level `.pigo/prompts` when trusted
  (#337), `~/.pigo/prompts` alongside legacy `~/.pigo/commands` (#336), and
  autocomplete/`/help` listings with argument-hint and source tier. (#340, #341)
- Install prompt packages to `~/.pigo/prompts`. (#342)
- Honor `~/.config/pigo/config.toml` over built-in defaults.

### Changed
- Large CLI refactor: `cmd/pigo/main.go` converged to a thin entry, with the REPL,
  `/status`, `/btw`, `/goal`, headless/subagent paths, pkgcmd, run-assembly,
  provider resolution, config loading, trust glue, and UI helpers each migrated
  into dedicated `internal/cli/*` packages. (#357–#369)

### Fixed
- Include the model in the Anthropic Messages request body.
- Gitignore pigo config files to avoid committing API keys.

## [0.3.6] - 2026-07-27

### Fixed
- Handle Ctrl+D/Ctrl+C as CSI-u reports in the REPL. (#329)
- Allow file tools to reach the advertised skills dir. (#327)
- Align the cursor with wide CJK runes in the line editor.

### Added
- Documented the interactive REPL and slash commands in `index.html`.

## [0.3.5] - 2026-07-27

### Added
- **Multi-line REPL input**: multi-line buffer with a cursor (#313), Shift+Enter
  newline / plain-Enter submit (#314), cross-line cursor movement and
  Home/End/Ctrl+A/E (#315), mid-line editing and line-merging backspace (#316),
  trailing-backslash line continuation (#317), full-line rendering with cursor
  repositioning (#318), and history record/restore. (#319)
- **Model-invoked skills**: name/description validation and
  disable-model-invocation (#301), a `FormatSkillsForPrompt` renderer (#302),
  `<available_skills>` injection into the system prompt (#303), and run-assembly
  wiring. (#304)
- `/status` slash command with env/credentials/telemetry sections and runtime
  config + context rendering, retaining telemetry across runs. (#291–#297)

## [0.3.4] - 2026-07-24

### Added
- **`/btw` side-thread**: command skeleton (#279), multi-turn follow-up and exit
  interaction (#280), bare `/btw` to reopen the most recent side thread (#281),
  and per-thread model/thinking overrides. (#282)
- **`/goal` autonomous mode** (mirrors pi-goal / Claude Code goal). (#278)
- Bundle built-in skills and install them on first run. (#277)
- Browse prior REPL inputs with up/down arrows on a blank line.

## [0.3.3] - 2026-07-24

### Added
- Render assistant replies as Markdown in the REPL. (#276)
- Cycle REPL input suggestions with the up/down arrows. (#275)

## [0.3.2] - 2026-07-24

### Added
- Recent-input suggestions in the REPL. (#274)

## [0.3.1] - 2026-07-24

### Added
- **pi-extension hosting**: an embedded Node host (`pihost.mjs`) to run pi
  extensions (#263), routing extension launchers through it (#264), plus
  `Plugin.CallCommand` / `Manager.Commands` command aggregation (#262) and
  `commands/call` wire types (#268), registering plugin commands as slash
  commands with prompt injection. (#265)

## [0.3.0] - 2026-07-23

### Added
- **Harness engineering**: a generic system-reminder dynamic-context injection
  mechanism (#248), a unified tool-result trimming budget in the tool executor
  (#250), structured harness telemetry collection (#251), and classification
  with bounded retries for tool-execution failures. (#252)
- `--thinking-level` flag with layered config resolution. (#246)
- **Model-name → provider inference**: an inference table (#234) wired into
  `resolveProvider` (#235) and documented in the README/`--help`. (#236)
- Output byte cap and head/tail preview truncation for the bash tool. (#249)
- Harness capability matrix and baseline docs; `internal` package diagrams.

### Fixed
- Model-quality degradation and strict-gateway compatibility issues. (#240–#244)

## [0.2.1] - 2026-07-22

### Added
- Chinese-market providers: Qianfan, Volcengine Ark, DashScope (Bailian), and
  Hunyuan. (#232)
- README architecture overview with the two-layer agent-loop flowchart and the
  runtime layering diagram.

## [0.2.0] - 2026-07-22

### Added
- **Provider registry**: a central registry as the single source of truth (#182),
  provider-derived API-key resolution (#183), a `--provider` flag to select a
  built-in provider (#184), base-URL override precedence (#185), and wiring for
  all OpenAI-protocol (#186) and Anthropic-Messages (#187) providers, with
  special-auth parameter validation. (#188)
- Curated model catalog expanded with 11 providers and curated models. (#189)
- **The companion book** *Writing the pi Agent in Go*: `book/` scaffolding with a
  pandoc + xelatex + ElegantBook build pipeline (#200), a full 10-chapter outline
  (#201), the introduction (#202), all chapters 1–10 and the afterword
  (#204, #212–#221), sample figures, and a PDF pipeline with SVG embedding. (#205)

### Changed
- Renamed `manual.html` to `index.html` and made the docs site bilingual (zh/en).

## [0.1.2] - 2026-07-20

### Added
- One-click install script (`install.sh`).
- REPL tool-status line coloring: green on success, red on failure.

## [0.1.1] - 2026-07-20

### Added
- CI GitHub Action and goreleaser support.

## [0.1.0] - 2026-07-20

Initial public release. pigo lands as a working Go re-implementation of the pi
coding agent.

### Added
- **Agent core**: an `EventStream` mechanism (#18), a tool registry with JSON
  Schema validation (#19), streaming backfill (#20), three-phase tool execution
  (#21), parallel & sequential batch tool execution (#22), and the two-layer
  agent loop `runLoop`. (#23)
- **Provider layer**: a unified `Provider` interface with a dual failure model
  (#25), a shared transport driver (SSE + retry + dual watchdogs) (#26),
  three-stage thinking normalization (#27), decoders for Anthropic Messages
  (#63), OpenAI-compatible (#29), and Gemini (#30), and the first concrete
  providers — Bedrock/OpenRouter/Ollama. (#33)
- **Built-in tools**: `read` (#34), `write` (#35), `edit` with diff (#36),
  `bash` with streaming/timeout/cancellation (#37), `grep`/`find`/`ls` honoring
  `.gitignore` (#38), `todo` task tracking (#127), and `webfetch`. (#128)
- **Model & auth**: a model registry and provider directory (#31), plus OAuth
  and API-key resolution. (#32)
- **Security**: an in-process sandbox gate and secret redaction. (#44)
- **Run modes**: a headless/stdio mode and the `pigo` CLI entry (#39),
  headless/stream-json runs carrying a session id with resume support (#176),
  and `--system-prompt` / `--append-system-prompt`. (#180)
- **System prompt & config**: system-prompt assembly with `AGENTS.md` injection
  (#40) and a layered configuration system. (#42)
- **Sessions**: local JSONL persistence and resume (#43); a session tree with
  id/parentId and v3 migration (#121); `/fork`, `/clone` (#122), `/tree`
  navigation (#123), `/export`/`/import` (#124), and `/copy` + `/session`. (#125)
- **Context compaction**: token accounting and trigger detection (#117),
  `FindCutPoint` (#118), summary generation with on-disk `CompactionEntry` (#119),
  and the `/compact` command with automatic loop compaction. (#120)
- **Extensibility**: sub-agent orchestration, skills, and slash-commands (#45);
  a plugin system with a subprocess JSON-RPC 2.0 protocol (#132) and lifecycle
  event subscription (#133); project trust (#134); and process-isolated
  sub-agent mode. (#135)
- **Package manager**: a lockfile and install-dir conventions (#154), `npm:<name>`
  reference parsing (#155), npm detection and content fetch (#156), pi package
  classification (#157), distribution of extensions (#158), skills (#159),
  prompts/commands (#160), and themes (#161), plus `pigo install` (#162),
  `list` + `uninstall` (#163), and `update`. (#164)
- **Interactive TUI** (bubbletea v2) with a lipgloss theme layer, width-aware
  rendering, Markdown output, streaming spinners, tool-call cards, viewport
  scrolling, role-styled transcripts, action slash-commands, and a model picker.
  (#41, #89–#95, #103)
- Image input as content blocks with two-provider encoding. (#126)
- Switched to `spf13/pflag` (#87) and split `internal/agent` into `agentcore`,
  `provider`, `agenttool`, and `runtime` leaf packages with a verified layering
  DAG. (#74–#79)

[Unreleased]: https://github.com/smallnest/pigo/compare/v0.5.6...HEAD
[0.5.6]: https://github.com/smallnest/pigo/compare/v0.5.5...v0.5.6
[0.5.5]: https://github.com/smallnest/pigo/compare/v0.5.4...v0.5.5
[0.5.4]: https://github.com/smallnest/pigo/compare/v0.5.3...v0.5.4
[0.5.3]: https://github.com/smallnest/pigo/compare/v0.5.2...v0.5.3
[0.5.2]: https://github.com/smallnest/pigo/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/smallnest/pigo/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/smallnest/pigo/compare/v0.4.4...v0.5.0
[0.4.4]: https://github.com/smallnest/pigo/compare/v0.4.3...v0.4.4
[0.4.3]: https://github.com/smallnest/pigo/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/smallnest/pigo/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/smallnest/pigo/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/smallnest/pigo/compare/v0.3.7...v0.4.0
[0.3.7]: https://github.com/smallnest/pigo/compare/v0.3.6...v0.3.7
[0.3.6]: https://github.com/smallnest/pigo/compare/v0.3.5...v0.3.6
[0.3.5]: https://github.com/smallnest/pigo/compare/v0.3.4...v0.3.5
[0.3.4]: https://github.com/smallnest/pigo/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/smallnest/pigo/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/smallnest/pigo/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/smallnest/pigo/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/smallnest/pigo/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/smallnest/pigo/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/smallnest/pigo/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/smallnest/pigo/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/smallnest/pigo/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/smallnest/pigo/releases/tag/v0.1.0

