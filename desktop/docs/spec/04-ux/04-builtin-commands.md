# 04. Builtin Commands

## 1. Goal

Define the first-party command palette entries available without plugins.
The same entries are available in the composer `/` menu.

Shortcut: **Cmd/Ctrl + Shift + P** (D014)

## 2. Command ID convention

```text
builtin.<domain>.<action>
```

## 3. Core builtin catalog

The Rust `BUILTIN_ALIASES` table is the single source of truth and contains
exactly these commands. Plugin commands extend the searchable command list
dynamically; app navigation, project management, settings, plugin management,
and diagnostics are not builtin command contracts.

| id | title | keywords | category | risk | behavior |
|---|---|---|---|---|---|
| `builtin.session.new` | New Task | new, chat, task | Session | low | reuse the latest empty session in the current group or create a durable empty session, then focus the composer |
| `builtin.agent.btw` | Ask Aside (by the way) | btw, aside, side question | Session | low | run one out-of-band question against a snapshot of the active session; the answer never enters the transcript |
| `builtin.agent.compact` | Compact Conversation Context | compact, context, tokens | Session | low | create a model-context checkpoint for the idle active session |
| `builtin.session.copy` | Copy Last Reply | copy, clipboard | Session | low | copy the newest assistant answer as plain text to the system clipboard |
| `builtin.session.info` | Session Info | session, info, tokens | Session | low | open a read-only session report in the composer dock |
| `builtin.app.status` | Runtime Status | status, runtime, environment | App | low | open a read-only runtime/environment report in the composer dock |
| `builtin.mode.agent` | Switch to Agent | mode, agent | Session | low | set the idle session mode to Agent |
| `builtin.mode.plan` | Switch to Plan | mode, plan, planning | Session | low | set the idle session mode to Plan |
| `builtin.mode.goal` | Switch to Goal | mode, goal, objective, autonomous | Session | low | set the idle session mode to Goal |
| `builtin.session.export` | Export Session (pigo JSONL) | export, jsonl, backup | Session | low | write the active session as pigo-compatible JSONL through a native save dialog |
| `builtin.agent.rebuild` | Rebuild Conversation Context | rebuild, reload, context | Session | low | discard the live runtime and rebuild the model context from the host transcript without opening a turn |
| `builtin.agent.goal` | Set or Inspect Goal | goal, objective, status | Session | low | start, inspect or clear a goal; token budgets and pause/resume are REPL-only |
| `builtin.session.tree` | Conversation Nodes (tree) | tree, nodes, branch, variants | Session | low | open the numbered node list; `/tree <n>` forks an independent session through node `n` |
| `builtin.session.rewind` | Restore Points (rewind) | rewind, restore, files, rollback | Session | medium | open the file-edit restore points; `/rewind <n>` rolls journaled files back and forks the conversation before that turn |
| `builtin.memory.report` | Persistent Memory Report | memory, remember, recall | Session | low | open a read-only report of the persistent-memory index and its per-scope entry counts |
| `builtin.agent.dream` | Consolidate Memory (dream) | dream, memory, consolidate | Session | medium | run one memory-consolidation pass with the session's own model; `/dream dry-run` analyzes without writing |

`/session` and `/status` read renderer state only: they issue no host RPC and
never write to the session. Figures are the values the store already holds, not
an estimate computed for the report. `/memory` is the exception — the memory
index belongs to the sidecar, so the card performs one read-only
`memory.report` call and renders its answer.

## 4. Visibility and execution rules

- The IDs above are the complete first-party registry. Removed IDs are not
  palette results and are not renderer dispatch cases; plugin commands remain
  independently discoverable.
- `New Task` uses the current project or temporary group. It selects the
  group's most recent empty session when present; otherwise it reveals the
  empty home on the first frame and creates the durable empty session. The
  action is idempotent within a group.
- `Compact Conversation Context` is available while the active session is idle;
  an active turn or checkpoint remains busy according to the existing compaction
  contract.
- Mode commands use the same active-session configuration path as the Composer
  Agent/Plan/Goal chip. They update an idle session immediately. With no active
  session, they update the persisted default for the next session; a running
  session or pending approval is not changed.
- `SubmitPlan` and `SubmitGoal` are model tools, not palette commands. There is
  no Chat mode or request-changes alias.
- The former app/project/settings/plugin/log commands remain available through
  their dedicated surfaces where applicable, but are not part of the command
  palette or the composer builtin namespace.

## 5. Execution results

Commands return:

```ts
type CommandExecutionResult =
  | { ok: true; navigation?: string; message?: string }
  | { ok: false; error: AppError }
```

## 6. Acceptance

1. The builtin registry contains exactly the unique, prefixed IDs listed above,
   with no duplicates among ids or slash aliases.
2. Palette search matches each title and keyword set.
3. Mode switch commands update the idle session mode immediately; Plan, Goal,
   and Agent refer to the same pi Agent.
4. Compact works while idle and returns `AGENT_BUSY` during an active
   turn/checkpoint.
5. Removed IDs and the legacy `newChat`, `openProject`, and `openSettings`
   dispatch aliases do not appear in the registry or renderer switch.

## 7. Composer slash aliases (D123, ADR 0024, ADR 0106)

Builtin commands surface in the composer `/` menu through short aliases defined
in the Rust `BUILTIN_ALIASES` table (`src-tauri/src/composer.rs`). Each row also
declares an `argMode` (`none`, `optional`, `required`) that is forwarded as
`argMode` on the `ComposerCommand`; the renderer's `dispatchComposerCommand`
uses that metadata instead of matching command ids. Adding a no-argument
builtin therefore requires one registry row plus one palette case, and no
change to the composer submit hook.

| alias | palette id | argMode |
|---|---|---|
| `/new` | `builtin.session.new` | none |
| `/btw` | `builtin.agent.btw` | required |
| `/compact` | `builtin.agent.compact` | none |
| `/copy` | `builtin.session.copy` | none |
| `/session` | `builtin.session.info` | none |
| `/status` | `builtin.app.status` | none |
| `/agent-mode` | `builtin.mode.agent` | optional |
| `/plan-mode` | `builtin.mode.plan` | optional |
| `/goal-mode` | `builtin.mode.goal` | optional |
| `/export` | `builtin.session.export` | none |
| `/rebuild` | `builtin.agent.rebuild` | none |
| `/goal` | `builtin.agent.goal` | optional |
| `/tree` | `builtin.session.tree` | optional |
| `/rewind` | `builtin.session.rewind` | optional |
| `/memory` | `builtin.memory.report` | none |
| `/dream` | `builtin.agent.dream` | optional |

### Session export / rebuild / goal

- `/export` asks host-core for the full session detail and writes pigo's
  `SessionHeader` + `entryWire` JSONL (schema v3, linear `parentId` chain,
  readable `user`/`assistant` rows only). The output is importable by the same
  desktop via `importer::parse_pi` and resumable by the CLI. A cancelled save
  dialog is reported as a non-error.
- `/rebuild` is a sidecar operation: the host transcript is authoritative, so
  the command disposes the live `AgentContext` and rebuilds it (compaction
  checkpoints, tool catalog, provider binding) via `Manager.Rebuild`. It refuses
  to run during an active turn with `AGENT_BUSY`; a session with no live runtime
  needs no work because its next prompt builds one.
- `/goal <objective>` switches to Goal mode and sends the objective; `/goal
  status` reports the current mode and planning phase; `/goal clear` returns to
  Agent mode. The desktop runtime executes **one approved goal turn**, not the
  REPL's autonomous loop, so `pause`, `resume` and `--tokens` are explicitly
  reported as unsupported rather than being faked.

### Persistent memory /memory and consolidation /dream

`/memory` opens a read-only card over the pigo memory store: whether it is
enabled, the resolved root (`$PIGO_HOME/memory`, else `~/.pigo/memory`), the
SQLite index path, entry counts per scope, and the reconcile delta applied
before counting. The sidecar reconciles first, exactly like the CLI, so the
counts describe the Markdown files on disk rather than a stale index. An
unopenable index is reported **inside** the card instead of failing the request,
because "the index cannot be opened" is the diagnostic itself.

Unlike the CLI's `/memory`, the card carries no checkpoint section: the desktop
has no CLI-style automatic checkpoint pass, so context figures come from the
renderer's own compaction rows and last reported usage.

`/dream` runs one memory-consolidation pass and reports the CLI's change
counters (merged / deduped / paths cleaned / pruned / distilled / bytes /
files). Three boundaries are deliberate:

- **In-process, not a subprocess.** The CLI spawns `pigo --dream`; the desktop
  has no such binary to spawn, so the sidecar runs `dream.Runner` itself behind
  a single-flight latch. A second request while one is running returns
  `DREAM_BUSY` instead of silently degrading into the on-disk lock's "skipped"
  no-op. Dream still builds its own model client and never touches a live
  session's `AgentContext`.
- **The session's own model.** The pass uses the session's launch binding
  (provider, model, credential) rather than a second configuration.
- **Desktop transcripts are the distillation source.** host-core's SQLite has
  the conversations, so the sidecar adapts `session.list` / `session.get` to the
  runner's `SessionSource`; without that adapter the pass would only see pigo's
  file session store, which the desktop never writes, and `distilled` would stay
  0 forever. Subagent rows and tool-result rows are excluded from the distilled
  transcript.

`/dream dry-run` (also `--dry-run`, `-n`) analyzes and reports without writing
files or dream state, matching the CLI flag. The command requires an active
session and a provider with a credential; both failures are reported as toasts
rather than empty reports.

### Conversation nodes /tree and restore points /rewind

`/tree` has two layers, because the desktop stores two different things:

**Fork lineage (session level, real tree).** A branch is a real session, so the
lineage is metadata rather than message structure. Schema v22 records
`parent_session_id` and `forked_from_message_id` on the forked session, and
`session.fork` writes both from the source session and cut point it already
knows. `/tree` renders the tree the open session belongs to — its ancestor
chain plus every branch hanging off it, indented with roots first, a `Current`
badge on the open session and a `Source missing` badge when a recorded parent is
gone — and clicking a row switches the renderer to that branch's session.
Unrelated sessions stay in the sidebar; this panel is a branch view, not a
second session list.
Sessions created before v22, and every root session, have no parent and render
as roots; the column is nullable, so no backfill is required.

**Transcript nodes (message level, linear).** The transcript is still a linear
host-owned JSONL file with no message parent pointer (ADR 0023), so the node
list underneath the lineage is the numbered sequence of the open session, not
another branch diagram:

- `/tree` reads the full `session.get` transcript and opens the numbered node
  list (role, one-line preview, tool name, subagent nesting). A user turn
  carrying a regenerate family (`revisionRootId` / `revisionCount` /
  `activeRevision`) exposes a variant selector backed by
  `session.activateRevision`.
- `/tree <n>` (or the `Branch here` button) calls `session.fork` with
  `throughMessageId` = node `n` and activates the copy. This is the desktop
  equivalent of switching the REPL's active leaf: the source session is never
  modified. Unlike REPL, the branch is a separate session, so the child records
  its origin instead of sharing one tree.
- `/rewind` lists restore points, oldest first. A point is one user turn that
  produced at least one message-owned review snapshot (ADR 0043), i.e. a
  `Write` or `Edit` tool result with `details.review`. These are the same file
  mutations the REPL journals in `FileSnapshotRecorder`.
- `/rewind <n>` rolls back the active changes of point `n` and every later
  point **newest first** (`review.rollback` refuses to overwrite a file whose
  post-tool hash changed), then forks the conversation to the node immediately
  before that turn. Conflicts, `unavailable` snapshots and already-rolled-back
  rows are counted as skipped, never as restored. For the first turn there is
  no preceding node, so files are restored but the conversation is left intact.
- **Scope boundary:** Bash/shell side effects, scratch-root writes and tools
  without a structured `path` are not journaled and are never restored. There
  is no session-wide file checkpoint or undo stack. This matches the REPL's
  rewind v1 scope; the panel states it explicitly.

A `required` alias sent without an argument is consumed locally with a hint
instead of falling through as prompt text. `/btw <question>` opens the side
thread; `/copy`, `/session`, `/status` and `/memory` take no argument.

Aliases share one namespace with template and plugin command names; builtin
aliases win collisions, then project templates, then user templates, then
plugin commands. Selecting an alias inserts `/alias `; sending `/new` or
`/compact` alone executes locally without creating an empty prompt. The
Agent/Plan/Goal aliases also support a prompt body:
`/agent-mode <prompt>`, `/plan-mode <prompt>`, or `/goal-mode <prompt>` switches
the idle session (or the next-session default) and sends `<prompt>` through the
normal prompt path. A mode alias sent with no body remains a local mode switch.
The prompt body remains the visible user turn; a failed dispatch does not clear
the composer draft. Former builtin aliases are no longer resolved and are
handled as ordinary unknown slash text unless supplied by another command
source.

## 8. Composer skill entries

Active built-in, plugin, and user-owned Skills also surface in the composer `/`
menu. They use the exact Skill id as the slash name, show the Skill's display
name and description, and form a separate **Skills** group after extension
commands. This group is always last; a Skill never shadows a command or
template with the same name.

Selecting a Skill inserts `/<skill-id> `. Sending `/<skill-id>` with optional
prompt text keeps that typed form as the visible transcript chip and asks the
model to call the existing `Skill` tool with the validated id before answering.
Only Skills active for the current project are listed or accepted, so project
scope and plugin activation remain enforced at send time. If the Skill is no
longer active, the text follows the normal unknown-slash prompt path.

## 9. Ideographic comma opens the slash menu (D405)

A Chinese IME produces `、` (U+3001) where the ASCII `/` is meant, so reaching
the menu otherwise means switching input methods mid-sentence. When the composer
is empty, a committed `、` as its first character is rewritten to `/` before
trigger detection runs, and the ordinary slash menu opens with the same
insertion, filtering, and send behavior described above.

Only the first character of an empty draft is rewritten. A `、` anywhere else in
the draft is ordinary punctuation and is never touched, and the alias has no
effect on the `@` file menu.
