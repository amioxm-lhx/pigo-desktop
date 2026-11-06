# PI-Go Desktop port

This directory is [PI-Desktop](https://github.com/vastsa/PI-Desktop) ported onto
pi-agent-go. The port keeps the desktop shell and replaces the agent kernel with
pi-go (route A).

## Provenance and license

| Item | Value |
|---|---|
| Upstream repository | `https://github.com/vastsa/PI-Desktop` |
| Upstream commit | `3a35078e` |
| Upstream version | 0.14.8 |
| License of this directory | **LGPL-3.0-or-later** (inherited, see `LICENSE`) |
| License of the rest of the repo | MIT (pi-go itself) |

This directory and the pi-go packages carry different licenses. Distributions
must treat `pi-go-desktop/` as LGPL-3.0-or-later and everything else as MIT.

## Strategy

The desktop is a three-layer host. Only the bottom layer changes:

```text
PI-Go Desktop
├── Renderer         React UI                                    TypeScript  kept
├── Electron Main    windows / IPC / plugin runtime / MCP / updater  TypeScript  kept
├── host-core        DB / tool execution / permissions / plans / plugins / MCP  Rust  kept
└── agent sidecar    agent loop / provider streaming / tool orchestration  Go  new, on pi-go
```

Replaced: the process role of `packages/agent-runtime` (about 24k lines of TS),
which ran on `@earendil-works/pi-agent-core` + `@earendil-works/pi-ai`. The new
implementation uses pi-go's `ai` + `agent` + `agent/harness`, and M4 deleted the
replaced files; what remains in that package is the pure helper modules Electron
main imports.

### Ownership boundaries

| Process | Owns |
|---|---|
| Electron Main | window lifecycle, IPC fan-in/out, child supervision, plugin runtime, MCP, updater |
| Renderer | UI only |
| host-core (Rust) | SQLite sessions and transcripts, tool execution, permission gateway, plans, plugin marketplace, secrets |
| agent sidecar (Go) | provider calls and streaming, agent loop, context and compaction, tool-call orchestration |

Three invariants hold across the port:

1. The NDJSON JSON-RPC contract between Electron main and the sidecar is kept
   verbatim (`agent.*` methods, `agent.event` notifications, `host.proxy`
   fallback), so neither main nor host-core knows which sidecar is running.
2. Session and transcript persistence stays with host-core's SQLite. The Go
   sidecar does not write through pi-go's `session-backends/sqlite`, which would
   double-write.
3. Tool execution stays with host-core. The Go sidecar calls back through
   `host.proxy` `tools.execute` instead of running pi-go's built-in tools as the
   execution path.

## Wire contract

Transport: NDJSON JSON-RPC 2.0 on stdio, one message per line.

### Inbound (Electron main to sidecar)

`sidecar.configure`, `sidecar.health`, `agent.prompt`, `agent.steer`,
`agent.steeringContext`, `agent.abort`, `agent.stop`, `agent.compact`,
`agent.getStatus`, `agent.disposeSession`, `agent.executeApprovedPlan`,
`agent.testRuntimeIdentity`, `asktool.resolve`

### Outbound (sidecar to Electron main)

| Kind | Method | Payload |
|---|---|---|
| notification | `agent.event` | `AgentEventEnvelope`: `agent_start`/`agent_end`, `turn_start`/`turn_end`, `message_start`/`message_update`/`message_end`, `tool_start`/`tool_update`/`tool_end`, `tool_permission_request`, `asktool_request`, `compaction_start`/`compaction_end`, `planning_state`, `status`, `error`, `custom` |
| request | `host.proxy` | forwards `tools.execute`/`tools.list`/`tools.abort`, `session.*`, `workspace.get`, `plans.*`, `provider.resolveAuth`, `provider.resolveSubagentModel`, `project.instructions.resolve`, `extensions.*` |

`agent.event` envelope fields: `sessionId`, `turnId`, `ts`, `event`,
`parentToolCallId`, `agentName`.

Errors keep the shape the Node sidecar sent: `code` -32000 for application
errors and -32601 for unknown methods, each with `data.errorCode`.

## Layout

```text
pi-go-desktop/
├── apps/desktop/            Electron app (renderer, main process, packaging config)
├── packages/
│   ├── agent-runtime/       pure helper modules Electron main imports (Node sidecar removed in M4)
│   ├── shared/ i18n/ plugin-sdk/ plugin-devkit/
├── crates/host-core/        Rust host-core
├── sidecar/                 Go agent sidecar (module pi-go-desktop-agent)
├── docs/                    upstream specs and ADRs, kept as the behaviour contract
├── scripts/                 build and E2E scripts
├── Cargo.toml / package.json / pnpm-workspace.yaml
└── PORTING-PI-GO.md         this file
```

`sidecar/` is a nested Go module. Go excludes it from the parent module's
`go build ./...`, so pi-go itself is unaffected. From M2 on it consumes pi-go
through `replace pi-go => ../..`.

## Milestones

| Milestone | Content | Status |
|---|---|---|
| M0 | Source import, license and docs, working baseline | done |
| M1 | Go sidecar skeleton: NDJSON transport, `sidecar.configure`/`health`, process switch | done |
| M2 | Agent loop: `agent.prompt` and friends, tools through `host.proxy`, event mapping | done |
| M3 | Desktop-specific behaviour: provider credentials, instruction chain, mode prompts, plans, asktool, subagents, compaction checkpoints, attachments | in progress (compaction, approved plans, the asktool and delegation landed) |
| M4 | Split `packages/agent-runtime`, package the Go binary, rename the installer | done (one library dependency on `@earendil-works/*` remains, recorded below) |

### M1 in detail

What landed:

- `sidecar/` implements the transport, the dispatcher, `sidecar.configure`,
  `sidecar.health`, `agent.getStatus`, and `agent.disposeSession`. Every other
  method of the frozen contract answered a named `NOT_IMPLEMENTED` at this
  milestone, so the renderer never hung on behaviour that had not landed; all of
  them have real handlers now.
- Electron main resolves the sidecar implementation once, in
  `apps/desktop/electron/main/agent-sidecar.ts`:
  1. `PI_DESKTOP_AGENT_BIN` when set and present
  2. `<resources>/bin/pi-go-desktop-agent[.exe]` (packaged)
  3. `sidecar/build/pi-go-desktop-agent[.exe]` (source checkout)
  4. otherwise the Node sidecar, so the desktop still runs before the Go binary
     is built
  `PI_DESKTOP_AGENT_IMPL=node` forces the Node sidecar; `=go` fails loudly
  instead of silently falling back.
- The desktop logs which implementation is running
  (`agent.sidecar.configured.go`).

Evidence recorded while landing M1:

| Check | Result |
|---|---|
| `cd sidecar && gofmt -l . && go vet ./... && go build ./... && go test ./... -count=1` | clean |
| `node sidecar/scripts/protocol-smoke.mjs` (spawns the real binary) | configure, health, getStatus, disposeSession answer; the methods that had not landed returned `NOT_IMPLEMENTED` (since replaced by real handlers); stdin EOF exits 0 |
| `pnpm typecheck` | all 9 workspace projects pass |
| `cargo build -p host-core` | ok |
| `node scripts/e2e-electron-boot.mjs` with `PI_DESKTOP_AGENT_IMPL=go` | PASS |
| Electron with `PI_DESKTOP_AGENT_IMPL=go`, logs in the data dir | `[pi-go-agent] ... ready version=0.1.0`, `sidecar.configure` handled, `agent sidecar configured (go)` |

Baseline of the imported tree, measured on Windows before any behaviour change:
`node --test test/*.test.mjs` in `apps/desktop` reports 1654 tests, 1631 pass,
21 fail, 2 skipped. The same failures occur in an untouched `pi-desktop`
checkout on the same machine (macOS release tooling and Windows temp-path
scoping), so they are not regressions from this port.

### M2 in detail

What landed (the runtimes):

- `internal/runtime` runs one pi-go agent per session. `Manager.RuntimeFor`
  reuses a live runtime while its binding is unchanged and rebuilds it when the
  provider, mode, or command shell changes; `Session.Prompt` admits one turn at
  a time and refuses a second one with `AGENT_BUSY`.
- The model binding comes from the provider row Electron main ships: `apiStyle`
  first, then the catalog's per-model `api`, defaulting to
  `openai-completions`. The base URL prefers the row and falls back to the
  catalog, both header maps merge with the row winning, and an Anthropic base
  URL is canonicalised so the adapter's own `/v1` does not double up. A
  vendor-account row resolves auth through `host.proxy` `provider.resolveAuth`
  on every request, so the refresh token never reaches the sidecar.
- The stored transcript is rebuilt into model context the way
  `DesktopAgentRuntime.historyToEntries` did it: tool rows attach their calls to
  the assistant message above them, a truncated branch gets a synthesised
  carrier, failed assistant rows and subagent rows stay out, and assistant rows
  with neither text nor calls are dropped.
- The host tool catalog from `tools.list` is wrapped as pi-go agent tools. Every
  call goes back through `host.proxy` `tools.execute` carrying the approved
  shell id and dialect plus the requested timeout, so execution and permission
  decisions stay in host-core.
- Event mapping covers the live-turn contract: `agent_start`, `turn_start`,
  `message_start`/`message_update`/`message_end`, `tool_start`/`tool_end`,
  `turn_end`, `agent_end`, and `error`. Three details are pinned to the Node
  sidecar's wire shape: a delta frame carries identity plus deltas with the
  growing text stripped (`streamingMessageIdentity` in
  `packages/shared/src/message-stream.ts`), the streaming row's `createdAt`
  stays fixed for the life of the row, and `tool_end.result` is the whole
  `AgentToolResult`. Electron main stores that result as `toolResult`
  (`apps/desktop/electron/main/runtime/event-persistence.ts`), and
  `toolResultRoot` in `apps/desktop/src/lib/work-panel-tabs.ts` reads
  `details.root` from it, so a bare content array would silently break the
  Write/Edit review panel.
- Provider failures are classified with the Node table
  (`packages/agent-runtime/src/agent-errors.ts` -> `internal/runtime/errors.go`):
  the code, retriable flag, and the bounded `providerStatus`/`providerCode`
  details reach the renderer, provider bodies are redacted and cut to 600
  characters, and a structured `*ai.ProviderHTTPError` or a pi-go summary
  ("provider returned HTTP 429: ...", "Bedrock returned HTTP 500: ...") is
  probed before the keyword fallbacks. Two adapter-level details are recorded
  here because they are not visible in the Node source:
  - pi-go names its transport in the error text instead of the `"<status>: "`
    shape pi-ai produces, so the status patterns gained two pi-go shapes.
  - A user Stop cancels the HTTP request, which pi-go's Chat Completions
    adapter used to report as a *completed* partial answer (the SSE scan error
    was discarded). That adapter now fails the request, and the runtime closes
    a canceled turn as an aborted bubble with no error event, matching the
    desktop's stop semantics.
- Pre-flight failures keep the Node sidecar's names: `MODEL_NOT_CONFIGURED` for
  a row with no model or no way to authenticate, `COMMAND_SHELL_INVALID` for a
  missing shell, `AGENT_BUSY` for a second prompt, `TURN_NOT_FOUND` for steering
  without an active turn, and `RUNTIME_NOT_FOUND` after disposal. A session
  with no host channel is refused before a runtime is built, instead of
  dereferencing a nil host client.

What has landed since (recovery, tool detail, attachments):

- Provider retry (`internal/runtime/retry.go`) uses the Node budgets and
  schedule: a rate limit draws from its own ten-attempt budget and every other
  transient failure from a shared ten-attempt budget, across the request and
  stream phases, so a fault that changes phase cannot multiply the budget. A
  server-stated `retry-after-ms` / `Retry-After` wins outright; otherwise 429s
  wait 2s, 4s, ... with positive jitter up to 30s and other transient failures
  wait 1s, 2s, 4s, then stay at 8s. The wrapper owns the schedule and disables
  pi-go's in-adapter retry (`MaxRetries = 0`), so one logical turn has exactly
  one budget. An opaque 400/422 whose body says nothing gets the single informed
  repair attempt: the next request drops the derived output limit and the repair
  never consumes the shared budget.
- A response that can still be recovered from holds its bubble open
  (`internal/runtime/recovery.go`): `message_end` arms the recovery instead of
  closing the row, and the next `message_start` re-emits the same row as a
  snapshot `message_update`, so a recovered turn leaves one assistant row and no
  duplicate error row behind. The agent's own `agent_start`/`turn_start` events
  are suppressed while a recovery drives the loop, because the turn already
  announced itself.
- A silent turn (no visible text, no tool call) is re-run once with the
  `SILENT_TURN_NUDGE` text on the system prompt, restored afterwards unless a
  path-scoped instruction reload rewrote it. A second silent turn stops guessing
  and closes as `EMPTY_MODEL_RESPONSE` with `retriable: true`, which keeps the
  renderer's continue affordance. A response that looks like the internal
  `multi_tool_use.parallel` wrapper is logged as a lost tool batch.
- Context overflow is detected with `ai.IsContextOverflow` against the model's
  window (128k when the row publishes none) and reported as an error. It is
  deliberately not repaired: only the manual compaction path is ported, so an
  overflow whose provider message never named the problem reports
  `CONTEXT_TOO_LARGE` directly rather than pretending a retry could shrink the
  context. The automatic `threshold`/`overflow` triggers are listed under M3
  below.
- Tool detail: `tool_execution_update` becomes a `tool_update` frame, `tool_end`
  carries the `toolUsage` estimate (the call plus its result, measured the way
  `estimateToolTokenUsage` measures them), and the last tool of a batch
  publishes the `preparing` activity. The tool-role `message_end` snapshot is
  not emitted for an ordinary call, because nothing reads it; a `Task` call is
  the exception, because the delegation's settlement republishes that row.
- Bash progress and cancellation (`internal/runtime/progress.go`): the runtime
  subscribes to host-core's `tools.output` notifications for the duration of a
  Bash call, keeps one partial result per 100ms with the 64KiB head/tail
  truncation marker, and calls `tools.abort` when the call's signal fires, so a
  stopped turn does not leave the command running behind it.
- Prompt attachments: an inlined image becomes a content block of the user
  message, so a vision model actually receives it. A file-only attachment stays
  plain text, because Electron main names the file in the prompt itself.
- The provider row's stored `apiStyle` is translated into pi-go's wire API the
  way `apiBindingForProviderModel` does it (`internal/runtime/api_style.go`): the
  catalog's per-model pin wins over the provider-wide style, both the snake_case
  and the kebab spelling resolve, and a style the table does not know stays on
  Chat Completions. Without this the sidecar compared the stored value
  (`chat_completions`) against pi-go's wire identifiers
  (`openai-completions`) and refused every real provider row with
  `unsupported api style`.
- A thinking-level change rebuilds the runtime, so moving the composer's level
  takes effect on the next turn instead of staying frozen at build time.
- `sidecar/scripts/live-turn.mjs` closes the gap the protocol smoke test cannot:
  it plays Electron main against the built binary and a real provider, answers
  `session.get`, `tools.list`, and `tools.execute` from memory, and asserts the
  turn shape end to end. It needs `DEEPSEEK_API_KEY` or `SILICONFLOW_API_KEY`
  in the environment (or `PI_DESKTOP_LIVE_PROVIDER` plus
  `PI_DESKTOP_LIVE_MODEL`) and exits non-zero on any failed check.

### M3 in progress (compaction, approved plans, asktool)

The three methods that used to answer `NOT_IMPLEMENTED` have real handlers now,
so every entry of the frozen contract is implemented.

- `agent.compact` (`internal/runtime/compaction.go`) ports the manual checkpoint
  path: the safe request budget (`contextBudget`,
  `retainedUserMessageBudget`), the Codex-shaped preparation (one contiguous
  range to summarize, no split turn, a retained tail only an active turn keeps),
  the summary request through pi-go's harness
  (`harness.PrepareCompactionFromMessages` + `CompactWithPreparation`), the
  lookup that drops a checkpoint whose boundary row is gone, and
  `persistCheckpoint`, which writes the durable record through
  `session.appendCompaction` *before* it installs the rebuilt context and only
  then reports the event the renderer draws the row from. A rebuilt runtime
  restores the stored checkpoint (`SessionDetail.Session.compaction`), so a
  restarted session keeps the window the previous one compacted to. The wire
  shapes are pinned: the record is camelCase with `retainedTail` always an
  array, `compaction_end` always carries `willRetry`, and the mark carries the
  bounded identity from the shared package (`contextCompactionMark`).
- Automatic compaction (`internal/runtime/auto_compaction.go`) restores the two
  triggers the Node runtime had. `threshold` is evaluated before a prompt is
  admitted and again between the turns of a running one, so a long tool loop
  never reaches the provider's hard limit; a checkpoint it installs is a normal
  session checkpoint, so the transcript is untouched and a restarted runtime
  restores it. `overflow` is the safety net for a rejection that arrived anyway:
  the turn spends one recovery, which compacts and re-issues the request. Both
  are evaluated against the same safe budget `agent.compact` uses, so manual and
  automatic checkpoints agree on when a context is too large, and the
  retained-tail retention mode follows the Node runtime (`active_turn` mid-turn,
  `completed_turn` between turns).
  A delegate compacts too, through the same helpers and its own provider
  catalog: it has no durable transcript to name a boundary with and no
  `agent.compact`, so without this a long exploration loop ran until the
  provider rejected it and reported a failed run. Its compacted window lives in
  the run's message list only — the summary and the retained tail — and its
  summary request is generated through the model its definition resolved to,
  pinned or authorized, not through whatever the session is bound to. Two
  supporting fixes came with it: the pre-flight summary guard now measures a
  tool result the way the summarization prompt folds it (a couple of thousand
  characters) rather than at its full stored size, which used to refuse
  compactions that fit comfortably, and a delegate builds its provider catalog
  for itself instead of borrowing the session's.
- `asktool.resolve` (`internal/runtime/asktool.go`) ports the interactive
  question card. `asktool` is sidecar-local (host-core has no execution path for
  it and never lists it in `tools.list`), runs `sequential`, pauses the turn on
  `asktool_request`, and is answered by `asktool.resolve` — or by nulls when the
  user skips the card, the turn is stopped, or the runtime is disposed. The
  model-facing text is the shared package's `formatAskToolOutput` byte for byte
  (answers joined with an ideographic comma, a full-width colon after the
  question), the validation matches `normalizeAskQuestions`, and a question set
  the card cannot render is a failed tool call that *keeps* its own text and
  details (`AfterToolCall` sets the flag, because pi-go expresses "failed" by
  replacing the result).
- `agent.executeApprovedPlan` (`internal/runtime/plan.go`) runs a host-approved
  plan or goal as a normal agent turn. The instruction is synthesized in this
  runtime only and appended to its in-memory context, so Electron main never
  receives a user event for it and cannot persist or render it as a transcript
  row. Execution forces agent mode, including the system prompt the Node runtime
  rebuilds per request; it refuses another session's plan, an empty turn id, and
  a turn already in flight; and a failure that arrives after admission reaches
  the renderer as an `error` event, which is why its reply is sent as soon as
  the plan is admitted.

Not ported, recorded here because each changes observable behaviour:

- The retained-tail recovery family: a manual compaction that cannot produce a
  summary fails as `CONTEXT_COMPACTION_FAILED` instead of installing a reduced
  checkpoint.
- The `new_context` tool, and the compaction settings a prompt row carries
  (`PromptParams.CompactionSettings`), which are only reported in the sidecar
  log.

What remains:

1. Prompt features the sidecar accepts but does not run: plugin tools, plugin
   skills, and trusted extensions. Each is reported once per session in the
   sidecar log instead of being dropped silently. (Delegation used to be on this
   list; it landed with the Task family, see the section above.)
2. Compaction beyond the automatic triggers: the retained-tail recovery family,
   the `new_context` tool, and a prompt row's compaction settings (see the
   automatic-compaction section below).
3. The mutation-recovery guards (mutation failure counts and
   `pendingMutationTermination`) are not ported. Approved plan execution now
   runs, but without those guards, so an autonomous turn has no
   mutation-failure circuit breaker.
4. `agent.testRuntimeIdentity` is implemented but probe-only: like the Node
   sidecar it answers method-not-found unless `PI_DESKTOP_PLAN_UI_PROBE=1`, and
   it has not yet been run against the Go sidecar from the probe.
5. Every wire API a desktop row can select now streams. Chat Completions,
   Responses (including the Codex subscription endpoint), Anthropic Messages,
   Google Generative AI, and pi-messages all push `text_start`/`text_delta`/
   `text_end` (plus the thinking and tool-call equivalents) as the provider
   sends them. The last two were pi-go-wide request/response adapters:
   `streamFinalMessage` in `ai/provider_http.go` parsed the whole body and
   pushed it as one delta, so an `anthropic_messages`, `google_generative_ai`,
   or `pi_messages` turn appeared in one piece. They now have SSE
   implementations of their own, and a stream that ends without the provider's
   terminal frame (Anthropic `message_stop`, Google `finishReason`, the gateway
   `done`/`error` frame) fails the turn instead of resolving the partial text as
   a finished answer. Adapters this milestone cannot reach stay on the old path:
   `google-vertex`, `bedrock`, and `azure` are not selectable api styles in the
   desktop, and the OpenAI adapter keeps its whole-response fallback for calls
   that inject a custom `fetch`. The discarded scan error this milestone fixed
   in the Chat Completions adapter (`ai/openai_completions.go`) was audited in
   the others: Responses and pi-messages both check it, and Mistral routes
   through Chat Completions.


### M3 mode transitions and the packaged deployment

The mode surface the Go sidecar was missing is implemented, and the deployment
state — which until this round could not run the Go sidecar at all — now runs
it from an installer.

Mode transitions (`internal/runtime/mode.go`). `EnterPlanMode`,
`EnterGoalMode` and `EnterAgentMode` call `plans.enter` and then switch the
runtime; `SubmitPlan` and `SubmitGoal` validate their own arguments first (an
empty title, blank markdown or missing question is `PLAN_INVALID_ARGUMENT` and
does not terminate the turn), call `plans.submit`, and only then publish
`planning_state: awaiting_approval` with the node-shaped details the renderer
draws the approval bar from. A host failure is `PLAN_SUBMIT_FAILED` and closes
the turn. `beforeToolCall` keeps the Node rules: a transition tool must be the
only call in its assistant message, entering a mode only happens in agent mode,
and a submission only happens in the mode that submits it.

Tool sets (`internal/runtime/tools.go`). Agent mode sees
Read/Write/Edit/Bash/Glob/Grep, plan and goal see Read/Glob/Grep/Bash, and the
mode tools are appended to whichever set is active, so the model cannot call a
tool its mode forbids. The argument aliases the Node runtime accepted are folded
in `PrepareArguments` (`file_path` -> `path` for Read/Write/Edit, `query` ->
`pattern` for Glob/Grep).

`BrowserPreview` (`internal/runtime/localtools.go`) is sidecar-local: Electron
main owns the tool because it drives the work panel's WebContentsView, so the
sidecar proxies `tools.execute` with the session's mode and folds `file_path`
into `path` the same way.

Failed calls. pi-go expresses "this call failed" by replacing the result, which
would lose the flag the renderer reads, so `Session.markToolFailed` records the
tool call and `afterToolCall` puts the failure back (the details carry
`toolCallFailed`/`terminatesTurn`). Host tools that answer `ok: false` are
marked the same way.

Packaging. `scripts/build-agent.mjs` builds
`sidecar/build/pi-go-desktop-agent[.exe]` and every packaging script (`pack`,
`dist`, `dist:mac`, `dist:win`, `dist:linux`) runs it before electron-builder,
which copies the binary to `resources/bin/` next to host-core on all three
platforms. Until that round a packaged app carried no Go sidecar at all and
silently ran the Node bundle. M4 then removed the Node sidecar outright: main
spawns only the Go binary, a build without it fails at startup, and the boot
probe still reports `agentRuntime` (`"go"`) and `packaged` so deployment checks
assert the runtime instead of inferring it.

Deployment acceptance. `scripts/e2e-packaged-boot.mjs` runs the artifact the
same way an installed app runs — its own executable, its own resources, no
`PI_DESKTOP_AGENT_BIN` — and asserts that both binaries exist and are non-empty,
that the probe reports `agentRuntime: "go"` and `packaged: true`, the menu and
maximize contract, the 800-session responsiveness contract, that every bundled
plugin restored, and that no fallback warning was logged.
`PI_DESKTOP_PACKAGED_APP` points the same harness at an installed copy.
`scripts/e2e-plan-ui.mjs` gained `PI_DESKTOP_E2E_PACKAGED=1`, which runs the
whole Plan UI acceptance suite, including its live model case, against the
packaged app.

Plugin module format (found by running the packaged build). The unpacked build
tree lives inside this checkout, so Node's format detection found
`apps/desktop/package.json` (`"type": "module"`) above
`resources/plugins/<id>/main.js`, ran the bundled CommonJS `pi.file-manager` as
ESM, and failed to restore it with `require is not defined in ES module scope` —
while the same plugin loaded from an installed copy, which has no such ancestor.
A plugin's module format is now decided by the plugin (`pluginDeclaresEsm` reads
only the plugin's own package.json) and by its source: the entry is compiled as
CommonJS and the ESM loader is used only when that fails with an ESM-only syntax
error. `apps/desktop/test/plugin-module-format.test.mjs` pins the CommonJS case
under an ancestor `"type": "module"`, both ESM cases, and a genuinely broken
plugin.

Live round, and one acceptance gap. On the packaged app the live Plan case runs
a real model round — prompt, `EnterPlanMode`, `SubmitPlan`, approval, approved
execution, the exact `PLAN_LIVE_EXECUTED` marker, and the terminal UI state —
and then fails its last assertion, `runtime identity after live approval
execution changed runtime identity`. The Node reference fails it identically
(the pre-M4 commit with `PI_DESKTOP_AGENT_IMPL=node`, whose `sidecar.ts` and
`runtime.ts` are byte-identical to upstream): both rebuild the runtime when the
mode changes from plan to agent, because `matches()`/`Matches()` compare the
mode, while the assertion expects the runtime to survive the approval. The
assertion is upstream's and was left alone; changing it (or changing runtime
identity semantics) is a separate decision.

Evidence recorded while landing the runtimes:

| Check | Result |
|---|---|
| `cd sidecar && gofmt -l . && go vet ./... && go build ./... && go test ./... -count=1` | clean; `hostproxy`, `jsonrpc`, `protocol`, `runtime`, `server` all pass |
| `internal/runtime` tests | history rebuild (tool carrier, failed/subagent/empty rows, orphan tool row), `argumentsOf`, usage round trip, `cumulativeDelta`, timestamp parsing, `BuildModel` api-style and base-URL fallbacks, header merge, Anthropic canonicalisation, OAuth/key auth resolution through the host |
| `TestPromptStreamsOneAssistantTurn` (faux provider, fake host) | a full turn emits `agent_start`/`turn_start`/`message_start`/deltas/`message_end`/`turn_end`/`agent_end`; deltas concatenate to the provider text; `createdAt` stays fixed; delta frames carry no growing text |
| `TestPromptProxiesToolCallsToTheHost` | `tools.execute` carries session, turn, tool call id, mode, shell id/dialect and `timeoutMs`; `tool_end.result.details` reaches the renderer |
| `TestPromptStreamsThroughTheRealOpenAIAdapter` (`httptest` SSE server) | pi-go's Chat Completions adapter streams two chunks into deltas and a final row with usage, with no test seam in the path |
| `TestPromptReportsAProviderFailureAsAFailedRowAndErrorEvent` | a failed provider response closes the bubble as a failed row (status `error`, partial text kept, `error` payload attached) *and* emits one `error` event with `PROVIDER_RATE_LIMITED`; the row stays out of the model context |
| `TestPromptClosesAnAbortedTurnWithoutAnErrorEvent` | an assistant row that stopped with `ai.StopReasonAborted` closes as `aborted` with no error event |
| `TestAbortClosesTheTurnAsAborted` (`httptest` SSE server that holds the stream open) | a Stop mid-stream closes the turn as aborted with the streamed prefix and no error event |
| `TestFailTurnClosesTheBubbleAndReportsTheError` | a prompt call that fails before the stream still closes a failed bubble before the error event, and a stopped turn with no bubble emits nothing |
| `TestClassifyErrorCoversTheNodeTable`, `TestClassifyErrorLetsAbortWinOverCompactionFailure`, `TestClassifyErrorKeepsDiagnosticsBounded`, `TestClassifyProviderErrorFoldsInA429`, `TestClassifyErrorRedactsAndTruncatesProviderBodies`, `TestClassifyAssistantErrorUsesTheStopReasonAndMessage`, `TestClassifyRunErrorKeepsAnApplicationCode`, `TestClassifiedErrorRendersTheWireShape` | the whole `agent-errors.test.ts` table plus the pi-go status shapes, the 429 fold, redaction, truncation, and the application-code passthrough |
| `go test ./ai/ -run TestChatCompletions` in pi-go | a dropped stream fails the request instead of resolving as a completed answer, and a clean end still completes |
| `TestPromptRetriesARateLimitUntilTheBudgetIsSpent` | a 429 spends its ten-attempt budget and only the failure that outlives it reaches the renderer: one `message_start`, one failed `message_end`, one `error` event with `retryAttempt: 10` |
| `TestPromptRecoversAStreamFailureIntoTheSameBubble` | a 502 that arrives mid-stream is re-sent, and the retry streams into the bubble the failed attempt left open: one bubble id, one completed row, no error event |
| `TestPromptRerunsASilentTurnOnceThenReportsIt` | a silent turn is re-run exactly once and a second silent turn closes as `EMPTY_MODEL_RESPONSE` (`retriable: true`) with an error event and no extra bubble |
| `TestPromptStreamsToolProgressAndUsage` | a `tools.output` notification for the running Bash call reaches the renderer as a `tool_update` partial result, a notification for another call does not, and `tool_end` carries a `toolUsage` estimate |
| `TestAppendToolProgressKeepsTheHeadAndTheTail` | the progress buffer keeps its head and tail with the truncation marker once it passes the 64KiB cap |
| `TestPromptSendsInlinedImagesAsContentBlocks`, `TestPromptKeepsFileOnlyAttachmentsAsText` | an inlined image becomes a `text`+`image` content pair on the user message, while a file-only attachment stays a plain string |
| `TestTestRuntimeIdentityIsProbeOnly`, `TestRuntimeIdentityIsStablePerRuntime` | the identity RPC is method-not-found without the probe environment variable and refuses an unknown session; a runtime id is stable per runtime and unique across runtimes |
| `TestPromptRefusesOverlappingAndDisposedTurns`, `TestSteeringRequiresTheExpectedTurn` | `AGENT_BUSY`, `RUNTIME_NOT_FOUND`, `TURN_NOT_FOUND`, steering context, and the idempotent graceful stop behave as the Node sidecar's |
| `TestBuildModelMapsEveryStoredAPIStyle` | all seven stored styles (`chat_completions`, `responses`, `anthropic_messages`, `google_generative_ai`, `openai_codex_responses`, `pi_messages`, `opencode_go`) reach the matching pi-go adapter with the Node binding's default endpoint; a catalog pin overrides the row style; an unknown style stays on Chat Completions |
| `TestRuntimeForReusesThenRebuildsTheRuntime` | an unchanged binding reuses the live runtime, and a model, mode, or thinking-level change rebuilds it without leaving the old runtime behind |
| `TestEventWireShape` (`asktool_request`, `compaction_start`, `compaction_end`) | the three new events marshal to the frozen shape, `compaction_end` always carries `willRetry` (a failure included), and the mark is the bounded identity the renderer stores |
| `TestCompactInstallsACheckpoint`, `TestCompactStampsTheNextGeneration` | `agent.compact` persists one `session.appendCompaction` record (boundary at the last row, `retainedTail` an empty array, generation 1, then 4 after a restored checkpoint, provider and model stamped), reports `compaction_start` then `compaction_end` as session-level events, and rebuilds the model context to the summary alone |
| `TestCheckpointRequiresItsBoundaryRow`, `TestNewSessionRestoresTheStoredCheckpoint` | a checkpoint whose boundary row is gone is dropped and the context falls back to the raw transcript, while a live boundary restores as the summary plus the rows after it |
| `TestCompactRefusesARunningTurn`, `TestCompactReportsAFailedSummary` | a compaction during a turn answers `AGENT_BUSY` without announcing itself, and a summary the provider cannot produce answers `CONTEXT_COMPACTION_FAILED`, reports `compaction_end` with the error and no mark, and persists nothing |
| `TestPromptCompactsAnOversizedSessionBeforeTheModelRequest`, `TestPromptLeavesASmallSessionUncompacted` | a session past its safe request budget compacts before the prompt is admitted — one `session.appendCompaction` checkpoint, one `compaction_start`/`compaction_end` pair with reason `threshold`, `willRetry: true`, completed-turn retention, the new prompt still the last message of the request the loop issues, and a rebuilt context under the budget — while a session below the budget installs nothing and announces nothing |
| `TestToolLoopCompactsBetweenRequestsAndKeepsRunning` | a tool loop that pushes its own context past the budget compacts mid-turn with active-turn retention, the turn still finishes on the compacted window with no error event, and the request the compaction produced is smaller than the one it replaced |
| `TestOverflowRecoveryCompactsAndRetriesOnce`, `TestOverflowRecoveryIsSpentOncePerTurn` | a provider rejection for an oversized context is recovered once: one `overflow` checkpoint with `willRetry: true`, the request re-issued, the answer reaching the transcript with no error event, and a second rejection in the same turn reported instead of retried |
| `TestDelegateCompactsItsOwnContext` | a delegate whose loop crosses its own limit asks for a summary through its own provider catalog, keeps running on the compacted window, settles `completed`, and reports through the parent — with no session checkpoint installed and no summary leaking into the parent's context |
| `TestFormatAskToolOutputMatchesTheSharedHelper`, `TestNormalizeAskQuestionsValidatesTheCard` | the model-facing asktool text is byte-identical to the shared helper (full-width colon, ideographic comma, empty answer for a skipped question), and the card validation matches `normalizeAskQuestions` including the twenty-question limit |
| `TestAskToolRejectsAnUnrenderableQuestionSet`, `TestAskToolRoundTripAnswersTheModel`, `TestResolveAskToolDeliversTheAnswers`, `TestResolveAskToolRejectsBadAnswers`, `TestAbortSkipsPendingAskTools` | a rejected question set is a failed tool call that keeps its text and details and opens no card; the round trip publishes `asktool_request`, answers it, and formats the result; every invalid payload answers `ASKTOOL_NOT_FOUND`/`ASKTOOL_INVALID_ARGUMENT` with the Node message and leaves the card pending; a stop answers outstanding cards as skipped |
| `TestExecuteApprovedPlanRunsTheApprovedContract`, `TestPlanExecutionInstructionShape` | the approved contract runs as one agent-mode turn whose instruction (the verbatim plan and goal text, with the artifact path and the approved markdown) reaches the model but never becomes a user row, and both wrappers differ exactly as in `runtime.ts` |
| `TestExecuteApprovedPlanRefusesBadRequests`, `TestReportTurnErrorPublishesTheFailure` | a foreign session answers `PLAN_EXECUTION_NOT_FOUND`, an empty turn id `TURN_NOT_FOUND`, a busy runtime `AGENT_BUSY`, and a disposed one `RUNTIME_NOT_FOUND`; a failure after admission reaches the renderer as an `error` event on the executing turn |
| `TestWireContractOverConn` (`agent.compact`, `agent.executeApprovedPlan`, `asktool.resolve`) | the three methods answer `MODEL_NOT_CONFIGURED`, `PLAN_EXECUTION_NOT_FOUND`, and `ASKTOOL_NOT_FOUND` over the real transport, each on the application error code |
| `node sidecar/scripts/protocol-smoke.mjs` (real binary) | configure, health, getStatus, disposeSession; an unconfigured prompt answers `MODEL_NOT_CONFIGURED` without touching the host, a prompt without a shell answers `COMMAND_SHELL_INVALID`; `agent.compact` answers `MODEL_NOT_CONFIGURED`, `agent.executeApprovedPlan` answers `PLAN_EXECUTION_NOT_FOUND`, and `asktool.resolve` answers `ASKTOOL_NOT_FOUND`, each on the application error code; stdin EOF exits 0 |
| `node sidecar/scripts/live-turn.mjs` (real binary, real provider, DeepSeek `deepseek-chat`) | two turns in about 1.5-2s each: `sidecar.configure` ok, both turns admitted, the model called `Bash`, `tools.execute` returned the marker, the answer quoted it, and both turns ran `agent_start` → deltas → `message_end` (`status: complete`) → `turn_end` → `agent_end` with no `error` event; host calls `session.get, tools.list, tools.execute, session.get` |
| `pnpm run build:agent` | builds `sidecar/build/pi-go-desktop-agent` |
| `node scripts/e2e-electron-boot.mjs` (Go binary on the path) | PASS (both probes) |
| `go test ./ai/ -run TestAnthropicStream` in pi-go | a Messages SSE stream maps thinking (with signature accumulation), text, and a partial-JSON tool call into their events, carries usage from `message_start`/`message_delta`, fails the turn when `message_stop` never arrives, and surfaces an `error` event mid-stream |
| `go test ./ai/ -run TestGoogleStream` in pi-go | a `streamGenerateContent?alt=sse` stream maps thought/text parts and a function call (finishing as `toolUse`), maps usage with cached tokens subtracted from `promptTokenCount`, fails without a `finishReason`, and reports `SAFETY` or a gateway `error` chunk as a provider error |
| `go test ./ai/ -run TestPiMessages` in pi-go | the gateway's own frames reach the caller as they arrive, the terminal `done` frame carries usage and the stop reason, an `error` frame keeps the streamed prefix, a stream without a terminal frame fails, and a non-2xx answer reports the status |
| `go test ./ai/ -run TestProviderRequestHeaders|TestStreamingRequests` in pi-go | a row's `Authorization` (any spelling) beats the adapter's bearer token and reaches the streaming request, and a nil override suppresses the header including the adapter default |
| `cd sidecar && gofmt -l . && go vet ./... && go build ./... && go test ./... -count=1` | clean; `internal/runtime/mode_test.go` covers the three per-mode tool sets, `EnterPlanMode`/`EnterGoalMode` (host call, prompt, tools, `planning_state`), `SubmitPlan` (success, host failure, incomplete request), the `beforeToolCall` transition rules, `BrowserPreview` through `tools.execute` (alias folding and mode), and host-tool failure marking with its one-shot flag |
| `node scripts/build-agent.mjs` | builds `sidecar/build/pi-go-desktop-agent.exe` (10.4 MB) |
| `pnpm --filter @pi-desktop/desktop run pack` | exits 0; `release/win-unpacked/resources/bin/` holds `pi-desktop-host-core.exe` and `pi-go-desktop-agent.exe` |
| `pnpm --filter @pi-desktop/desktop run dist:win` | built `pi-agent-go-desktop-setup.exe` (119.2 MB, NSIS x64) and `PI-Desktop-Portable-0.14.8.exe`; the installer is the M4 rename below |
| `node scripts/e2e-packaged-boot.mjs` | PASS: both binaries present and non-empty, `agentRuntime: "go"`, `packaged: true`, menu-free frameless chrome, 800 sessions listed in 67-124 ms per read with a 52 ms main-thread gap, `pi.browser` and `pi.file-manager` restored, no `AGENT_SIDECAR_FALLBACK` |
| the same harness with `PI_DESKTOP_PACKAGED_APP` pointed at a silent install (`Setup.exe /S /D=<temp>`) | PASS with the same results; the install was removed afterwards |
| `PI_DESKTOP_E2E_PACKAGED=1 node scripts/e2e-plan-ui.mjs` | 5 passed / 0 failed against the packaged app, and again against the installed copy |
| `pi-agent-go-desktop-setup.exe` silent install (`/S /D=<temp>`) then `PI_DESKTOP_PACKAGED_APP=<install>` on `e2e-packaged-boot.mjs` | PASS with the same results as the unpacked build; install exited 0 and was removed afterwards |
| `PI_DESKTOP_E2E_PACKAGED=1` plus `PI_DESKTOP_TEST_*` (DeepSeek `deepseek-chat`), unpacked and installed | the live case runs a real model round inside both artifacts (prompt -> `EnterPlanMode` -> `SubmitPlan` -> approval -> approved execution -> exact `PLAN_LIVE_EXECUTED` marker); 5 passed, and the sixth fails on the runtime-identity assertion only - see "The live case's runtime-identity assertion" below |
| `node --test test/plugin-module-format.test.mjs` | a CommonJS plugin loads with `require` intact under an ancestor `"type": "module"`, an ESM plugin loads with and without its own package.json, and a broken plugin still fails to load |
| an ad-hoc harness (Electron main's three host calls answered in memory) against `deepseek-chat`, prompt row carrying `subagents` | a real model round delegates: `Task` is advertised and accepted, the delegate runs `Read` twice from its own system prompt, the Task row settles to `completed`, the parent resumes with `alpha.txt` and `beta.txt`, and no `subagents is not implemented` line is logged |
| the same harness with a streaming fixture provider instead of a live model | deterministic replay of the same path: every delegate envelope and message row carries `agentName`/`parentToolCallId`, the delegate's own `Read` runs under the parent's host channel, the settlement names the delegate and its turn count, and the resume prompt carries the report |
| `pnpm --filter @pi-desktop/desktop run dist:win` (this round) | `pi-agent-go-desktop-setup.exe` 125,069,143 bytes, SHA-256 `4310D241B5084AEA3076C247926580A997BAB13484E56521310888E8E4649870`; portable `PI-Agent-Go-Desktop-Portable-0.14.8.exe` 124,788,909 bytes |
| `pi-agent-go-desktop-setup.exe` `/S /D=D:\dev\tmp-pi\agent-go-test-install` while an installed upstream PI-Desktop is running | the port extracts its payload next to upstream without touching it: `D:\software\PI-Desktop` (45 files, 393,102,129 bytes, `PI-Desktop.exe` SHA-256 `D0F57236...`) and `C:\Program Files\PI-Desktop` are byte-identical afterwards, all 9 upstream processes keep their PIDs and start times, and the port's uninstall GUID (`fd14a24f-8d9e-5e7c-b512-f0d6c2f74308`) stays distinct from upstream's `15e37c3a-49c6-50e1-85c8-6db5caabf630` |
| `node scripts/e2e-packaged-boot.mjs` with `PI_DESKTOP_PACKAGED_APP` pointed at that install | PASS: the installed copy boots, reports `agentRuntime: "go"`, and `resources/bin/pi-go-desktop-agent.exe` matches the built sidecar byte for byte (SHA-256 `5E633BEC22E88680C5035833950AE282CF3F1AAE4A2207773278927D4D05D64B`) |
| the same installer run elevated (what the interactive installer asks for), then uninstalled with `/S` | writes its own ARP entry and Start menu shortcut, and removal takes only those plus its own directory; upstream's key, files and processes are untouched |
| a `/S` install from a non-elevated shell | extracts the payload but cannot write the machine-wide ARP entry or the ProgramData shortcut, so an unattended deployment has to start the installer elevated (the interactive path requests that by itself) |
| `node --test test/*.test.mjs` (apps/desktop) | 1660 tests, 1653 passed / 7 failed; all 7 are pre-existing and local to this checkout (1 `plugin-fs-scope` needs symlink privileges, 4 `packaging-footprint`/`macos-release-verification` cases need a macOS host, 1 `logger-routing`, and 1 `agent-extensions` that spawns `node -e` through a shell which Node 24 no longer parses) |

### M3 delegation: Task, TaskWait, TaskList, TaskStop

Delegation was the last missing piece of the agent surface: the Go sidecar
accepted a prompt's `subagents` array and reported it as unimplemented. A
delegate is now a second pi-go agent inside the same sidecar process
(`internal/runtime/subagent.go`), and the four tools ride the same host channel
the parent uses, so a child's tool call takes the same permission and
containment path.

**The two boundaries.** The parent's model context only ever gains a delegate's
final report, and a delegate's lifecycle never ends the parent's turn. `Task`
starts a run and returns a delegation id; `TaskWait` converges on reports;
`TaskList` and `TaskStop` are the inspection pair. A report the parent has not
read yet is delivered by the resume-after-idle prompt (`resumeAfterDelegations`),
which keeps the turn open until it is delivered — ending a turn is not an abort.

**Definition handling.** Electron main parses the agent definitions and resolves
their model pins before the prompt is sent, so the sidecar receives
`subagents`, `subagentProviders` and `subagentModelKeys`. A definition that
declares `tools: inherit` gets the parent's live catalog minus
`SUBAGENT_INHERIT_DENY_TOOLS` (the Task family, the mode transitions, `asktool`,
`ToolSearch`, `new_context`), because nested fan-out and mode switches stay with
the parent and a delegate has no user to ask. `Task.model` resolves in the order
override > definition pin > session provider, and an override the host has not
authorized asks `provider.resolveSubagentModel` on demand, the way the Node
runtime resolved a model added after launch.

**Prompt.** The system prompt gained the `## Delegation` block (upstream
`runtime.ts`) whenever the session has definitions, including the delegation
model catalogue; a delegate's own prompt is the definition body plus the shell,
scratch-directory, search, editing and project-instruction guidance it can act
on, and deliberately not the parent's collaboration rules — a delegate has no
user to talk to. `Session.Matches` compares the definition and provider
catalogues, so editing an agent file rebuilds the runtime and the new catalogue
reaches the model.

**Turn caps and failures.** The definition's `maxTurns` is applied the way
`SubagentRun.afterToolCall` applies it: the delegate's own `AfterToolCall` runs
the parent's outcome rules first and then terminates the run, so the outcome
reads `truncated` with whatever the delegate managed to say. A provider failure
arrives as an `ai.StopReasonError` message; its `ErrorMessage` becomes the
outcome's diagnostic and the delegate's last text is still reported, matching
`result()` upstream. The user's Stop, dispose, or a fatal parent error aborts
every running delegate.

**Wire shape.** Child rows keep the delegate's provenance: `agentName` and
`parentToolCallId` on the envelope and on the message row, so Electron main
persists them as child rows and the renderer draws the delegation card,
`SubagentPanel` and the topology view from the same fields upstream used.
`TaskWait` and `TaskList` report `details.delegations[]` with
`delegationId`/`agent`/`status`/`modelId`/`thinkingLevel`/`startedAt`/
`completedAt`/`report`, which is what `subagent-topology.ts` reads.

**Tests.** `internal/runtime/subagent_test.go` drives real turns through a faux
provider that answers the parent's and the delegate's requests from one queue:
Task + TaskWait convergence and report, provenance on every child row, the
resume-after-idle delivery, an unknown agent name, TaskList, the concurrency
cap, a `maxTurns` truncation, and a delegate provider failure reaching the
parent. `scripts/e2e-subagent-models.mjs` (E2E-166) now asserts the ported
surface against the real sidecar over NDJSON and a local streaming fixture: the
four tools are advertised, the delegate runs with its own system prompt, its
rows carry `parentToolCallId`, the report reaches the parent's context, and
plugin tools/skills are still reported as unported. The suite no longer accepts
"no Task tool advertised".

Two limits worth naming: the delegation registry is per runtime, so a rebuilt
runtime (mode change, provider change, an edited definition) loses the live
handles to delegates started by the previous one — the same trade-off the Node
runtime made when it was rebuilt; and `agent.testRuntimeIdentity` remains
probe-only.

## M4 (done)

`packages/agent-runtime` played two roles and could not be deleted as a whole.
The split is now in place.

**Removed (the process role)** - 30 files, about 18.7k lines:

- the agent process: `sidecar.ts`, `runtime.ts`, `subagent.ts`, `host-client.ts`
- the host plumbing only the process used: `parent-host-proxy.ts`,
  `node-proxy.ts`, `sidecar-config.ts`, `session-context.ts`, `path-lock.ts`,
  `stream-coalescer.ts`, `delegation-message.ts`, `mode-prompts.ts`,
  `project-instructions-prompt.ts`
- the trusted-extension runtime: `extensions/loader.ts`, `extensions/runner.ts`
- their tests, plus the Node-only tests that exercised the deleted runtime

**Kept (the library role)** - 20 modules Electron main imports, re-exported from
`src/index.ts`: `agent-errors`, `agent-messages`, `extensions/discovery`,
`extensions/types`, `model-capabilities`, `one-shot-complete`,
`opencode-session-headers`, `plugin-session-context`, `plugin-skills`,
`plugin-skills-prompt`, `project-instructions`, `project-memory-prompt`,
`prompt-enhancement`, `prompt-templates`, `provider-binding`,
`provider-headers`, `provider-retry`, `session-title-summarize`,
`subagent-definitions`, `thinking-level`.

**Electron main.** `agent-sidecar.ts` no longer resolves a Node entry or spawns
`process.execPath`: it resolves the Go binary, spawns it, and fails at startup
when no binary is present. `PI_DESKTOP_AGENT_IMPL` is gone, so is the
`AGENT_SIDECAR_FALLBACK` warning. The boot probe still reports
`agentRuntime: "go"`.

**Packaging.** `apps/desktop/package.json` lost the `bundle:runtime` script and
the `../../packages/agent-runtime/dist-bundle` extra resource, and the five
packaging scripts no longer call the bundler. Nothing named `agent-runtime`
ships in `Resources`; the helper modules reach the app only through the Vite
main chunk.

**The installer is `pi-agent-go-desktop-setup.exe`.** `build.nsis.artifactName`
is now `pi-agent-go-desktop-setup.${ext}` - name only, no version - so the
release asset has one stable, space-free name. The portable target keeps its
upstream name (`PI-Desktop-Portable-<version>.exe`).

**Trusted extensions are now recorded as absent.** The four E2E-241 to E2E-244
status entries in `docs/spec/06-delivery/04-e2e-test-plan.md` (and their
`docs/zh-CN` twins) claimed `pnpm test:e2e:trusted-extensions` still covered
them. That harness drove the Node runner M4 deletes: Electron main still holds
the extension bridge, but the Go sidecar registers no `extensions.command.run`
handler, so the method answers method-not-found and no extension command, hook,
or tool can run. The entries now say the journey is unimplemented in this tree
and name what upstream's harness covered.

**What still depends on `@earendil-works/*`** - deliberately, and this is the
open part of the milestone:

| Import site | What it uses | Why it stays |
|---|---|---|
| `packages/agent-runtime/src/provider-binding.ts` | `createModels`, `createProvider`, the six `.lazy` API adapters, `GITHUB_COPILOT_MODELS` | Electron main builds the provider catalog from these tables |
| `packages/agent-runtime/src/provider-retry.ts`, `provider-headers.ts` | `FetchFunction`, `ProviderHeaders`, error classification types | retry and header policy Electron main applies to its own requests |
| `packages/agent-runtime/src/one-shot-complete.ts` | `createModels`, stream types | `plugin-agent-complete` still runs one-shot completions in main |
| `packages/agent-runtime/src/prompt-templates.ts` | `loadSourcedPromptTemplates`, `parseCommandArgs`, `substituteArgs`, `NodeExecutionEnv` | composer slash templates must behave exactly like pi CLI |
| `apps/desktop/electron/main/oauth.ts` | `InMemoryModelsStore`, builtin providers, `registerBunOAuthFlows` | vendor sign-in flows stay in main (permitted by the pre-split note) |

Dropping those imports means reimplementing prompt-template loading and the
OAuth flows in Go or in the desktop, which is a separate decision with its own
acceptance work. M4 landed the part that mattered for deployment: no Node agent
process is built, shipped, or reachable.

**The live case's runtime-identity assertion.** `e2e-plan-ui.mjs` compares the
runtime id at the pending approval with the runtime id after the approved
execution and expects them to be equal, and they are not:

```
FAIL E2E-106-live-agent - runtime identity after live approval execution
changed runtime identity: {"expectedRuntimeId":"271223fa-...",
"actual":"7a5ae63e-...","expectedMode":"plan","mode":"agent"}
```

A mode change rebuilds the runtime, by design and in both implementations:
`resolveAgentRuntimeLaunch(..., { mode: "agent" })` in
`apps/desktop/electron/main/runtime/plans.ts` sends `agent.executeApprovedPlan`
with `mode: "agent"` while the live runtime is still in `plan` mode, and the
runtime cache compares the mode. Upstream Node does the same thing in
`runtimeFor` (`packages/agent-runtime/src/sidecar.ts`, whose `matches()` in
`runtime.ts` ends `this.mode === config.mode`), so the assertion cannot hold for
the reference implementation either - it is an assertion/implementation
disagreement in the source tree this port came from, not a ported defect. The
behaviour is faithful here: `Session.Matches` in `sidecar/internal/runtime/session.go`
compares the mode, and `Manager.RuntimeFor` disposes and rebuilds when it
differs. The live case is opt-in (it needs `PI_DESKTOP_TEST_*`) and is the only
red line in the packaged suite.

**Evidence**

| Check | Result |
|---|---|
| `cd sidecar && gofmt -l . && go vet ./... && go build ./... && go test ./... -count=1` | clean; every package passes |
| `pnpm --filter @pi-desktop/desktop typecheck` | PASS (the deleted modules left no dangling import) |
| `pnpm --filter @pi-desktop/desktop run pack` | PASS; `release/win-unpacked/resources/` carries `bin/pi-go-desktop-agent.exe` and no `agent-runtime/` directory |
| `pnpm --filter @pi-desktop/desktop run dist:win` | builds `pi-agent-go-desktop-setup.exe` |
| `node scripts/e2e-packaged-boot.mjs` | PASS: `agentRuntime: "go"`, `packaged: true`, both bundled plugins restored, session list responsive |
| `PI_DESKTOP_E2E_PACKAGED=1 node scripts/e2e-plan-ui.mjs` | 5 passed / 0 failed |
| `node scripts/e2e-subagent-models.mjs` | PASS against the Go binary (E2E-166, port scope: one ordinary turn, ignored delegate fields logged, no Task tool advertised, no delegate model resolved) |
| `node scripts/e2e-plugin-import-deps.mjs` | PASS for the installer boundary; the trusted-extension runner half is gone with the Node sidecar |
| `node --test test/*.test.mjs` (apps/desktop) | same pre-existing Windows failures as before this round, no new ones |

## M5 (done) product identity split

The port shipped under the upstream identity, so its installer closed the
user's PI-Desktop and installed over it: `appId`, `productName`,
`executableName`, `shortcutName` and the data directory were all upstream's, and
NSIS reads that tuple as "the same product". Nothing in the UI named the running
engine either, so the swap could only be inferred from behaviour - "is this Go
or the old one?" had no answer inside the app.

**One product, one identity.** `APP_ID` is `com.pi-agent-go.desktop`, `APP_NAME`
and `productName` are `PI-Agent-Go-Desktop`, the Windows
`executableName`/`nsis.shortcutName` are `PI-Agent-Go-Desktop`, the Linux
executable and the deb/rpm package names are `pi-agent-go-desktop`,
`desktopName` is `pi-agent-go-desktop.desktop`, and the portable artifact is
`PI-Agent-Go-Desktop-Portable-<version>.exe`. The NSIS `artifactName` keeps the
M4 name, `pi-agent-go-desktop-setup.exe`. The macOS helpers moved with it:
`PI-Agent-Go-Desktop-macOS-open.command` carries
`EXPECTED_BUNDLE_ID = com.pi-agent-go.desktop`, and `verify-macos-release.sh`
looks for `PI-Agent-Go-Desktop.app`. The renderer document title, the tray,
splash and sidebar strings, the `updates.desc` caption and the built-in
plugin-development skill name were all still upstream's and now name the port.

**Data and lock isolation.** The default data directory is
`~/.pi-agent-go-desktop` (`PI_DESKTOP_DATA_DIR` still wins). Electron's
`userData`, and with it the single-instance lock, follows the app name, so the
two builds no longer contend for one `pi.sqlite` or one lock.

**Updates are off.** The port has no electron-updater feed, and the upstream one
carries the Node implementation this port replaces: the Windows NSIS and Linux
AppImage paths install whatever they download, so a packaged build would have
reverted itself on the next quit. `build.publish` is gone (no `app-update.yml`
ships in `Resources`), `resolveUpdateMode` answers `disabled` unless
`PI_DESKTOP_UPDATE_FEED=1`, and the releases link points at the port's own
repository.

**The engine is reported, not guessed.** `app.getVersion` gained an `agent`
field - `{ impl, engine, version, binary }` - answered by the sidecar itself:
`AGENT_ENGINE = "pi-go"`, the supervised `impl`, the version from its own
`sidecar.health` reply (bounded to 2s and cached on success, so a wedged or
restarting sidecar cannot hang the row), and the binary's file name. Settings ->
About renders it, and the boot probe reads the same object back through the
preload bridge, which is the renderer's own path:

```text
PI-Agent-Go-Desktop 0.14.8
protocol 11 · host 0.14.8
Agent engine: pi-go · go sidecar · 0.1.0 · pi-go-desktop-agent.exe
```

Protocol strings, file-format names (`.pi-desktopignore`), the
`pi-desktop/browser/event/preview` and `pi-desktop/agent/compact` topics, and
the MCP tool ids are unchanged: those are wire contracts, not branding.

**The agent's own name.** The identity split missed the Go sidecar's system
prompt, so the first reply of every session still opened with "I am PI-Desktop"
even though the window, the tray and About all said otherwise. The default
prompt and the Plan and Goal framings now say PI-Agent-Go-Desktop, as do the two
other strings the model reads verbatim - the BrowserPreview tool description and
the bundled plugin-development skill. `prompt_test.go` and a case in
`plugin-skills.test.mjs` pin all four, because this is exactly the kind of string
a rename walks past.

**Evidence.** The installer was run silently into
`<scratch>\agent-go-test-install` while the user's upstream `PI-Desktop 0.14.8`
(`D:\software\PI-Desktop`, the Node build) was running and using its own
`host-core`:

| Check | Result |
|---|---|
| `pnpm --filter @pi-desktop/desktop typecheck` | PASS |
| `node --test test/*.test.mjs` (apps/desktop) | 1658 assertions, 1650 passed, 6 failed - the same six Windows-only failures as an untouched `a1c7861` worktree (`plugin-fs-scope` needs symlink privilege, four `packaging-footprint`/`macos-release-verification` macOS cases, one `logger-routing` case) |
| `cd sidecar && gofmt -l . && go vet ./... && go test ./... -count=1` | clean; every package passes |
| `pnpm --filter @pi-desktop/desktop run dist:win` | `pi-agent-go-desktop-setup.exe`, 124,955,441 B, SHA-256 `44452F30982B4DC132B45CEA67E93BB2A638F706433D75576A6EC8C3CB2E9E71`; `release/win-unpacked/resources` carries `bin/pi-go-desktop-agent.exe` and no `app-update.yml` |
| install | `PI-Agent-Go-Desktop.exe` in the directory that was asked for, its own Start-menu shortcut, its own uninstall key `fd14a24f-...`; the upstream tree is byte-for-byte unchanged (45 files, no add, no remove, no size or mtime change) and the upstream key `15e37c3a-...` is untouched |
| upstream app while installing | eight `PI-Desktop` processes and its `pi-desktop-host-core` kept the same PIDs across the install and the uninstall |
| uninstall | its own directory, shortcut and uninstall key removed; the upstream install still unchanged |
| `node scripts/e2e-packaged-boot.mjs` (against the installed copy) | PASS x4: `appName` `PI-Agent-Go-Desktop`, `agentRuntime` `go`, `agentEngine.engine` `pi-go` with binary `pi-go-desktop-agent.exe`, and `aboutEngine` read back through the preload bridge |
| `PI_DESKTOP_E2E_PACKAGED=1 node scripts/e2e-plan-ui.mjs` with a real DeepSeek `deepseek-chat` round | 5 passed / 1 failed - the same live runtime-identity assertion M4 documents; every other case, including the live approval execution, passed |
| `pnpm --filter @pi-desktop/docs build` | PASS. A zh-CN spec page linked ADR 0251 with a relative path that has no translated twin (`docs/zh-CN/adr/` holds only `index.md`), which broke the site build at `a1c7861`; it now uses the `/adr/0251-pi-go-agent-sidecar` form the ADR index already used |
| `pnpm --filter @pi-desktop/docs check:locales` / `pnpm check:release-docs` | 77 English/Chinese pairs verified; release docs aligned with 0.14.8 |

## Differences from upstream

1. Upstream `.git` history was not copied.
2. `.github/workflows/` still guards on the upstream repository and its release
   secrets. It must be rewritten before it is used; this repository is hosted on
   Gitee, so GitHub Actions does not run. The macOS artifact-name verification
   inside it was updated with the M5 rename so it matches what `dist:mac` now
   writes.
3. `package.json` gains a `build:agent` script that builds the Go sidecar
   into `sidecar/build/`.
4. `.github/pr-assets/` holds upstream PR screenshots (about 4.7MB) that no
   build uses. It is pending removal.
5. Product identity is the port's own: `PI-Agent-Go-Desktop`,
   `com.pi-agent-go.desktop`, `~/.pi-agent-go-desktop`, `pi-agent-go-desktop`
   for the Linux and deb/rpm names. M0-M4 deliberately kept upstream's strings
   so the baseline stayed behaviourally identical, which is also why an install
   from that era replaced the other build instead of sitting beside it (M5).
   Wire contracts - protocol strings, `.pi-desktopignore`, the
   `pi-desktop/*` notification topics, the MCP tool ids - keep their names.
6. `packages/agent-runtime` kept its upstream implementation through M0-M3 so
   the Node sidecar stayed available for comparison. M4 deleted the process
   role; the file list and the remaining `@earendil-works/*` imports are in the
   M4 section above.
7. pi-go's root `.gitignore` used a bare `sessions/`, which also matched this
   tree's `apps/desktop/src/features/sessions/`. It is now anchored to `/sessions/`
   so the ported source is not silently dropped from commits.
8. pi-go gained `ai/api_adapters.go` (+ `ai/api_adapters_test.go`): the package
   now exports the API style constants, `ProviderStreamsForAPI`, and
   `SupportedAPIs`. A host that stores a desktop provider row's `apiStyle` has
   to turn that string into a stream adapter, and the adapters were previously
   unreachable from outside `ai`. The OpenAI Codex subscription API is exported
   with them even though pi-go's own factory registration does not list it,
   because a desktop row can select it.
9. The sidecar resolves a provider row's `apiStyle` through the desktop's alias
   table before mapping it, so both `chat_completions` and `openai-completions`
   reach Chat Completions. The Node runtime only normalized a catalog pin, which
   left a row written with the kebab spelling on Chat Completions even when it
   meant Anthropic or Google; the sidecar recognizes the style in either
   spelling. An unrecognized style keeps the Node fallback to Chat Completions.
10. The sidecar applies the session's thinking level when it builds a runtime and
    rebuilds on a change, where the Node runtime passed the level to the
    provider on every request. The transcript is re-read from host-core, so the
    visible effect is the same; the cost is one extra `session.get`/`tools.list`
    round trip per level change.

11. pi-go's `ai` package gained stream implementations for the wire APIs that
    were request/response only: `ai/anthropic_stream.go`, `ai/google_stream.go`,
    and the rewritten pi-messages path in `ai/pi_messages.go`. Each one follows
    the upstream adapter's event shape (block start/delta/end, usage on the
    terminal frame), so a delta frame stays what it was when the Node sidecar
    ran against a streaming provider. Anthropic request bodies also carry the
    model's thinking switch and budget now, mirroring `adjustMaxTokensForThinking`
    in the Node `simple-options` module, and a provider row's own headers win
    over the adapter's credential on every streaming request
    (`providerRequestHeaders` in `ai/provider_http.go`).

## Known limitations

- pi-go's `ExtensionHost` is a Go rewrite of the TypeScript ExtensionRunner and
  **does not execute JS extensions**. The upstream sidecar's trusted extensions
  (TS loaded through jiti) have no equivalent in the Go sidecar. Current
  decision: skip that feature in M2; the plugin system, which runs in Electron
  main, is unaffected.
- pi-go's `protocol` package speaks CBOR framing, not this sidecar's NDJSON
  JSON-RPC, so the transport is implemented in the sidecar rather than reused.