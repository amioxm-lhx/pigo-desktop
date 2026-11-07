# ADR 0256: Reopen the builtin command registry with guardrails

- Status: Accepted
- Date: 2026-11-07
- Deciders: PI-Desktop core
- Related: D421, ADR 0106, D250, ADR 0024, `04-ux/04-builtin-commands.md`

## Context

ADR 0106 froze the first-party registry at exactly five IDs after the
composer `/` menu and global search had grown noisy with dispatch-only
aliases and specification-only IDs. That cleanup was correct, and its
reasoning still holds: dead IDs must not masquerade as supported commands,
and actions that already own a dedicated UI surface should not compete with
task-oriented commands in search.

Since then the desktop rebuilt its slash-first workflows to match the pi CLI
in several passes. Each new builtin is backed by a real implementation — a
host/sidecar RPC or a read of renderer state — not a stale alias:

- `builtin.agent.btw`, `builtin.session.copy`, `builtin.session.info`,
  `builtin.app.status` (session and runtime reports)
- `builtin.session.export`, `builtin.agent.rebuild`, `builtin.agent.goal`
  (CLI parity for export, rebuild, and goal inspection)
- `builtin.session.tree`, `builtin.session.rewind` (conversation lineage and
  file-edit restore points)
- `builtin.memory.report`, `builtin.agent.dream` (persistent memory report
  and consolidation)

The registry now holds sixteen IDs while ADR 0106 still says "exactly five".
The drift happened silently, which violates the ADR contract itself.

## Decision

1. Replace ADR 0106's absolute freeze with three guardrails that every
   first-party command must satisfy:
   - It is registered in the single source of truth
     (`composer.rs` `BUILTIN_ALIASES`), which feeds the composer `/` menu
     and the renderer dispatch contract.
   - It has a real implementation behind it (a host or sidecar RPC, or a
     read of state the renderer already holds). A dispatch-only alias or a
     specification-only ID must not be registered.
   - It is documented in `04-ux/04-builtin-commands.md` together with its
     honest boundary (what the desktop cannot do relative to the CLI), and
     covered by the builtin-commands contract test.
2. Keep ADR 0106's removals in force: abort, session deletion, project,
   settings, plugin, log, session-rename, command-palette, reload-window,
   and DevTools stay out of the builtin registry, and removed commands do
   not return as compatibility aliases.
3. The sixteen IDs currently registered are recognized as the official
   first-party command surface.

## Consequences

- ADR 0106 is superseded in part: its freeze clause no longer applies, but
  it remains the record of why the registry was narrowed and why dead IDs
  are still banned.
- The command surface can keep evolving with the CLI without a new ADR per
  command, provided the three guardrails hold.
- Global search and the composer `/` menu stay bounded by the same rule that
  motivated 0106: only implemented, documented commands may register.

## Alternatives considered

### Edit 0106 in place to say sixteen

Rejected: ADRs are immutable records. Editing the freeze clause would erase
the reasoning that still applies and break the audit trail of how the
surface evolved.

### Move the new commands to plugin contributions

Rejected: these commands carry first-party semantics (session lifecycle,
context management, memory). Routing them through the plugin surface would
misrepresent their status and make them optional, which they are not.
