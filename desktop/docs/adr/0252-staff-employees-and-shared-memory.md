# 0252. Staff employees: referenced credentials and promotion-gated shared memory

* Status: Accepted for implementation

## Context

PI-Desktop runs one agent per conversation. The product direction is a
one-person company built on top of it: a set of long-lived employees that run
unattended on a schedule, promote a product on external platforms, and hand work
to each other.

Two requirements follow that the current model cannot express:

1. Each employee must be able to run on its own API key and its own model.
2. Employees must be able to share what they learn while working.

The existing pieces are close but not sufficient:

- `providers` / `secrets_meta` / `SecretStore` already hold provider
  credentials, and `secrets_meta` already carries `owner_kind` / `owner_id` /
  `kind`, so a non-provider secret owner was anticipated.
- `scheduled_tasks` / `task_runs` already model a recurring prompt, but a task
  has no identity, no credentials, no model binding, and no memory.
- `sessions` carry `provider_id` / `model_id`, so a run can already select a
  model per session.

Sharing memory naively -- letting employees read each other's transcripts -- is
the obvious approach and the wrong one: one employee's speculation becomes
another employee's fact, with no way to trace or revoke it. Unattended runs make
that failure silent, because nobody is watching the moment it happens.

## Decision

### 1. An employee is a durable entity, not a session

An `employees` row owns a role, a workflow binding, a schedule, a quota, a
credential reference, and a model binding. Runs create sessions and link back
through `employee_runs`.

### 2. Credentials are referenced through a profile, never stored inline

```text
employee -> credential_profile -> (provider_id, secret_ref)
```

- `secret_ref` is `secret:profile:<profileId>:api_key`, held only in
  `SecretStore`; `secrets_meta` records `owner_kind = 'credential_profile'`.
- No employee row, run row, RPC response, or log line carries key material.
- Several employees may reference one profile (a shared company key) or each
  hold their own (a personal or cheaper key). Rotation happens at the profile.
- `employee_runs.credential_profile_id` records which key paid for a run, so
  cost is attributable per employee instead of only per provider.

A per-employee key does not mutate the referenced provider row: a provider
denotes how to talk to a vendor, a profile denotes which account pays.

### 3. Shared memory is typed, scoped, provenance-carrying, and promotion-gated

- Scope: `employee` (private) or `shared` (readable by every employee).
- Kind: `fact` | `preference` | `playbook` | `constraint` | `signal`.
- Every item carries `source_kind` + `source_ref` and a confidence.
- `constraint` items are user-authored only. The host rejects an
  employee-authored constraint, so an employee can never widen its own limits.
- A run may write its own scope directly. Publishing to `shared` requires
  passing the workflow's memory gate, which is deterministic code, not a model
  judgement.
- Items are append-only. An update writes a new row and sets `supersedes_id` on
  the old one; retrieval returns only `active`, unexpired items. Two live items
  that share a `key` but disagree are both surfaced and marked contested rather
  than silently resolved.
- Employees never read another employee's transcript. Sharing happens through
  memory items only.

### 4. host-core batches; the Electron main process runs the loop

host-core owns the durable side of a schedule: who is due, which workflow
version a run is pinned to, how a run ended, and how far to back off next time.
It stays headless, and `staff.scheduler.claim` is a single call that both advances
`next_run_at` and opens the session for each due employee, so two pollers cannot
start the same slot.

The loop that turns a claim into agent turns lives in the Electron main process,
because a session is launched there. It therefore needs the run's credential, so
`staff.run.binding` resolves profile, provider, model, thinking level, and base
URL in one audited reply. That reply is the only one that carries key material,
and it is deliberately missing from the renderer IPC allow-list, which the
bridge enumerates by hand: the omission is the enforcement, not a comment.

### 5. The operator console is a sidebar app, not a command

The console was asked to be reachable from the left navigation as
**My apps → AI employees**, not from the command palette. A palette entry would
have cost less code, but residency is the thing an unattended system has to make
legible, and a palette hides state behind the act of searching for it. The
sidebar row therefore carries the live signal itself: the in-flight run count, or
a tone-coded dot for healthy, errored, and not-resident.

The console is an operator surface, not a second control plane. Every action it
offers calls the same `staff.*` RPCs the resident driver calls, so there is one
place where a rule can be enforced and one place where it can be bypassed.

## Consequences

- Storage schema v17 adds `credential_profiles`, `employees`, `employee_runs`,
  `memory_items`.
- `SecretStore` is used unchanged with a new ref namespace.
- The renderer employee surface shows credential labels, never values.
- Deleting a credential profile is refused while an employee references it
  (`ON DELETE RESTRICT`); deleting a provider is refused while a profile
  references it.
- Memory retrieval must be bounded and cited. A workflow node declares what it
  reads, so the injected block stays inside the context budget.
- The resident driver is Electron main, not host-core: the host has no session
  launch path, and giving it one would duplicate the sidecar's lifecycle.
- The console is a lazily loaded renderer page, and the sidebar entry reads the
  same scheduler status the page header does, so the two cannot disagree about
  whether the driver is alive.

## Alternatives

- **A per-employee API key column on `employees`.** Rejected: key sprawl, no
  rotation path, and the employee list becomes a secret-bearing export.
- **A separate `providers` row per employee key.** Rejected: it turns the
  provider list into the account list, and two rows for one vendor cannot share
  a model catalog or a vendor account.
- **A shared transcript or one vector store as memory.** Rejected: no
  provenance, no revocation, and unverifiable claims propagate to every
  employee.
- **Model-judged memory promotion.** Rejected for the same reason as the
  content gate: an unattended promotion decision must be deterministic.
- **Running the whole loop inside host-core.** Rejected: the host would need to
  own agent-session lifecycle, which is the sidecar's job, and a headless host
  cannot be restarted around a live window the way the desktop driver can.
