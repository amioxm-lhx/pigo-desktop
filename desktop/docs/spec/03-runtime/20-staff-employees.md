# 20. Staff Employees

> Decision: [ADR 0252](../../adr/0252-staff-employees-and-shared-memory.md).
> Storage schema: v17. Owner: Rust host-core.

## 1. Goal

Let a set of long-lived employees run unattended on their own schedule, each
with its own credential profile and model binding, sharing what they learn
through a memory record that carries provenance instead of a shared transcript.

## 2. Ownership

| Concern | Owner |
|---|---|
| employee / profile / run / memory rows | Rust host-core (SQLite) |
| credential material | `SecretStore` (schema `14-secrets-storage.md`) |
| schedule execution | host-core batches the claims; the resident Electron main driver turns each one into a session (§12) |
| agent execution | agent sidecar, one session per run |
| employee list, workflow editor, memory review | renderer |

The renderer never sees a credential value and never writes memory directly.

## 3. Data model

```sql
CREATE TABLE credential_profiles (
  id            TEXT PRIMARY KEY,
  label         TEXT NOT NULL,
  provider_id   TEXT NOT NULL REFERENCES providers(id) ON DELETE RESTRICT,
  secret_ref    TEXT NOT NULL,
  base_url      TEXT,
  config_json   TEXT NOT NULL DEFAULT '{}',
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL
);

CREATE TABLE employees (
  id                    TEXT PRIMARY KEY,
  name                  TEXT NOT NULL,
  role                  TEXT NOT NULL DEFAULT '',
  credential_profile_id TEXT REFERENCES credential_profiles(id) ON DELETE RESTRICT,
  provider_id           TEXT,
  model_id              TEXT,
  thinking_level        TEXT,
  workflow_id           TEXT,
  workflow_version      TEXT,
  status                TEXT NOT NULL DEFAULT 'paused',   -- paused | running | stopped
  quota_json            TEXT NOT NULL DEFAULT '{}',
  created_at            INTEGER NOT NULL,
  updated_at            INTEGER NOT NULL
);

CREATE TABLE employee_runs (
  id                    TEXT PRIMARY KEY,
  employee_id           TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
  session_id            TEXT REFERENCES sessions(id) ON DELETE SET NULL,
  credential_profile_id TEXT,
  provider_id           TEXT,
  model_id              TEXT,
  trigger               TEXT NOT NULL DEFAULT 'schedule',  -- schedule | manual | event
  status                TEXT NOT NULL DEFAULT 'running',
  error_code            TEXT,
  started_at            INTEGER NOT NULL,
  ended_at              INTEGER
);

CREATE TABLE memory_items (
  id           TEXT PRIMARY KEY,
  scope        TEXT NOT NULL,                     -- employee | shared
  owner_id     TEXT,                              -- employee id when scope = employee
  kind         TEXT NOT NULL,                     -- fact | preference | playbook | constraint | signal
  key          TEXT NOT NULL DEFAULT '',          -- contention / dedupe key
  text         TEXT NOT NULL,
  source_kind  TEXT NOT NULL,                     -- user | interaction | run | url
  source_ref   TEXT NOT NULL DEFAULT '',
  confidence   REAL NOT NULL DEFAULT 0.5,
  status       TEXT NOT NULL DEFAULT 'active',    -- active | superseded | retired
  supersedes_id TEXT,
  employee_id  TEXT,
  run_id       TEXT,
  created_at   INTEGER NOT NULL,
  expires_at   INTEGER
);
```

Credentials are referenced, never inlined: `profile -> secret_ref ->
SecretStore`. `secrets_meta` records `owner_kind = 'credential_profile'` and
`owner_id = <profile id>`.

The physical columns for the last table are `memory_key` and `content`; `key`
and `text` are the wire names, so the SQL stays clear of names that read like
keywords while a response reads `key` / `text`.

## 4. Credential resolution

A run resolves credentials in this order, and fails closed:

1. `employee.credential_profile_id` -> profile row.
2. profile `secret_ref` -> `secrets.getForRuntime` (audited, ephemeral).
3. profile `base_url` overrides the provider base URL when set.
4. missing secret -> `PROVIDER_SECRET_MISSING`; the run does not start.

An employee with no profile falls back to the provider's own secret, so an
existing single-account setup keeps working unchanged.

## 5. Memory semantics

### 5.1 Kinds and write authority

| kind | meaning | who may author |
|---|---|---|
| `fact` | observable, source-backed | any run, but `source_ref` is required |
| `preference` | user preference | any run |
| `playbook` | a tactic that measurably worked | memory gate only |
| `signal` | one observed interest signal | any run |
| `constraint` | a limit or prohibition | the user only |

The host rejects an employee-authored `constraint`. An employee must not be able
to widen its own limits, and unattended runs make that a hard boundary rather
than a policy. The same boundary covers removal: retiring a `constraint` is
refused unless the call identifies the user, because deleting a limit loosens it.

A `playbook` is written by the gate alone, so a run cannot mint one into even
its own scope.

### 5.2 Scopes and promotion

- A run writes `scope = 'employee'` for its own employee id, directly, and may
  not write `scope = 'shared'` at all.
- Publishing to `scope = 'shared'` requires the workflow memory gate, which is
  deterministic code and decides three things, in order:
  1. a non-`user` author must supply `source_ref`;
  2. an item whose text is already live is a duplicate, compared after trimming,
     lowercasing, and collapsing whitespace, and is refused;
  3. a live `constraint` owns its non-empty `key`: no other kind may be
     published under it. Contradiction is otherwise a semantic question, so the
     rule is structural rather than interpretive.
- `source_kind = 'user'` bypasses the gate. That is why a run-scoped write may
  not claim it: `staff.*` refuses `sourceKind = "user"` when the request carries
  a `runId`.
- Retrieval returns `status = 'active'` and not past `expires_at`, ordered by
  confidence then recency, and bounded by a caller-supplied limit.
- Two active items with the same non-empty `key` and different `text` are both
  returned and both flagged `contested`. The gate above deliberately lets that
  pair in: it refuses the same claim twice, not two live disagreements.

### 5.3 Append-only

Updates never mutate a row. A replacement is inserted and the previous row moves
to `status = 'superseded'` with `supersedes_id` pointing at the new row, so any
answer can be traced back to the run that produced it.

## 6. Lifecycle

| status | meaning |
|---|---|
| `paused` | exists, not scheduled; the default for a new employee |
| `running` | eligible for scheduling |
| `stopped` | halted by a gate or by the user; requires an explicit restart |

`employee_runs` mirrors the `task_runs` conventions: a run is `running` until it
finishes, fails, or the host restarts. Startup marks orphaned `running` runs as
`aborted`, matching existing boot maintenance.

## 7. RPC surface

`staff.*` is dispatched by host-core `rpc/mod.rs` into `staff::handle`, which
owns the whole surface so a new call is one edit in one place.

| Method | Purpose |
|---|---|
| `staff.profiles.list` / `staff.profile.create` / `staff.profile.update` / `staff.profile.delete` | credential profiles |
| `staff.profile.setSecret` | write the profile's API key into `SecretStore`; the only call that accepts key material |
| `staff.employee.list` / `staff.employee.get` / `staff.employee.create` / `staff.employee.update` / `staff.employee.delete` | the employee roster |
| `staff.employee.runs` | recent runs for one employee or all |
| `staff.memory.write` / `staff.memory.publish` | private write, and gated promotion to shared |
| `staff.memory.query` / `staff.memory.supersede` / `staff.memory.retire` | read and append-only replacement; a call carrying `runId` acts as a run, and one without acts as the user |
| `staff.workflow.list` / `staff.workflow.get` / `staff.workflow.create` / `staff.workflow.update` / `staff.workflow.versions` / `staff.workflow.delete` | versioned graphs; `create`, and an `update` that carries a `graph`, write a new immutable version |
| `staff.workflow.validate` | parse and compile a candidate graph without saving it |
| `staff.employee.refreshSchedule` | recompute `next_run_at` for one employee without touching its other columns |
| `staff.run.start` / `staff.run.finish` / `staff.run.step` | open a run, close it with a status, advance its step counter |
| `staff.run.binding` | resolve the frozen credential, provider, model, thinking level, and endpoint for one run; the only reply that carries key material |
| `staff.scheduler.claim` | one call that advances due schedules and opens their sessions; returns `{ claims, skipped }` |
| `staff.presets.list` | the built-in blueprints a fresh console can hire, each with the workflow graph and schedule it would install |
| `staff.presets.install` | hire the named blueprints against one credential profile; idempotent by employee name |

The credential value is write-only: no reply carries it, and a profile exposes
`hasSecret` plus its `secretRef` instead. Resolving a key for a run happens
in-process through `resolve_run_binding`, whose result is neither `Serialize`
nor `Debug`. `staff.run.binding` is the one exception, and §12 explains why it
can exist without handing the renderer a key.

## 8. Failure modes

| case | behavior |
|---|---|
| missing secret | run refused with `PROVIDER_SECRET_MISSING` |
| profile deleted while referenced | refused by `ON DELETE RESTRICT` |
| provider deleted while a profile references it | refused |
| unknown kind or scope | write rejected, no row inserted |
| employee-authored constraint | write rejected |
| missing `source_ref` on a `fact` | write rejected unless promoted by the gate |

## 9. Acceptance criteria

All seven are covered by `crates/host-core/src/staff/tests.rs`.

- [x] two employees can run on different credential profiles and models in the
      same app, and each run records which profile paid
- [x] no RPC response, log line, or employee row contains key material
- [x] an employee cannot write a `constraint`
- [x] a shared item cannot be written without a source reference
- [x] superseding an item keeps the previous row and links both directions
- [x] retrieval never returns superseded, retired, or expired items
- [x] a database at v16 migrates to v17 without touching existing rows

## 10. Workflow graph contract

A workflow is a versioned graph. `staff.workflow.create` writes version 1;
`staff.workflow.update` writes the next version when it carries a `graph`, and
only edits name, description, and status when it does not. Versions are
immutable, and a run pins `(workflow_id, workflow_version)` when it is claimed,
so editing a graph never changes what an in-flight or historical run did.

The editor draws the same graph it saves. `position` is view state that travels
with the version, so reopening an old version restores the shape it had. The one
exception is a graph still on the single row the first blueprints shipped: that
row is a machine's work rather than the owner's, so it is laid out again on open
and lands on the bands the auto-layout button produces. The one
exception is a graph still on the single row the first blueprints shipped: that
row is a machine's work rather than the owner's, so it is laid out again on open
and lands on the bands the auto-layout button produces.

| Rule | Enforced by | Editor behaviour |
|---|---|---|
| exactly one `trigger` | host | adding a second trigger is refused before it is drawn |
| every node reachable from the trigger | host | reported when validation runs |
| no cycles | host | reported when validation runs |
| per-kind required config | host | the kind's fields are the ones rendered |
| node id is alphanumeric, `-`, or `_` | host | generated ids already satisfy it |
| at most 200 nodes and 600 edges | host | the palette stops offering nodes |

The closed kind set is:

```text
trigger | probe | analyze | produce | agent | memory | condition | handoff | humanGate | notify
```

`staff.workflow.validate` runs the same parser and compiler without saving, and
answers `{ valid, error?, nodeCount?, steps? }`, so the editor can explain a
rejection before the user commits to it.

A `probe` node walks a closed channel set: `juejin`, `zhihu`, `bilibili`,
`wechatMp`, and `wechatChannels`, carried as a `sources` array. A graph that names any other
channel is refused when it is parsed, and a legacy graph whose probe still carries one
`channel` string keeps compiling, because the parser reads it as a one-element list.

A `memory` node names one of four operations, and that one list is what the
validator accepts and the compiler emits:

```text
read | write | publish | retire
```

`read` is answered by the driver itself: it reads the store and hands what it
found to the next step, without a model turn. One run may hold more than one
read node, and what they find accumulates: a worker told to read both its
playbook and the positions worth posting at is handed both. Replacing instead of
appending would quietly halve the briefing -- the seeded poster read its playbook
and reported "0 placements, no candidate positions supplied" every round until
the second read stopped erasing the first. `write` and `publish` record that
step's own outcome, to the employee's scope and to shared scope respectively.
Parsing still accepts `query` as an older spelling of `read` and compiles it to
`read`: the driver once knew only that word while the console wrote `read`, so a
graph naming `read` -- the seeded analyst among them -- paid for a model turn and
then recorded that turn's prose as its signal (run 6e6e5672). `retire` has no
driver implementation yet, so a retire node runs a turn and records nothing.

## 11. Schedule semantics

| field | meaning |
|---|---|
| `everyMinutes` | fixed-interval grid, 5..1440; the anchor is the employee's `created_at`, so a restart never shifts the grid |
| `timesOfDay` | `HH:MM` anchors, at most 24, sorted and deduplicated; mutually exclusive with `everyMinutes` |
| `jitterMinutes` | deterministic offset derived from the employee id and the slot, 0..1440, so two employees sharing a cadence do not fire together |
| `utcOffsetMinutes` | the zone the anchors are read in |
| `maxRunsPerDay` | daily ceiling, default 96 |

```json
{ "everyMinutes": 30, "jitterMinutes": 5, "utcOffsetMinutes": 480, "quota": { "maxRunsPerDay": 48 } }
```

A failed run backs off exponentially from 5 minutes to 6 hours instead of
retrying on its own grid, and a scheduled employee must pin a workflow version:
an unattended run that silently followed the newest revision would run
something nobody approved.

## 12. Residency

host-core stays headless. It owns batching, state, and recovery, and never opens
a session. The resident driver lives in the desktop shell (`staff_driver.rs`
under the Tauri port), because that is where a session is launched, and it polls
on a timer that does not depend on a window being open. The shell resolves each
step's turn through `staff.run.binding`: the run's own key and endpoint override
the provider row, so a credential profile can differ from the row it names.

Probe collection still belongs to the site collector. Until that surface is
ported, a probe step is handed the spec's empty-input line
(`- (nothing: no memory item and no collected page reached this step)`) and the
run records that it collected nothing rather than fetching on its own.

`staff.scheduler.claim` is deliberately one call: it advances `next_run_at`
and opens the session for every due employee in the same transaction, so two
pollers cannot start the same slot. It returns `{ claims, skipped }`, where a
skipped employee carries the error that kept it out of this batch.

Credentials reach that driver through `staff.run.binding`, which resolves the
run's profile, provider, model, thinking level, base URL, and key. It exists as
an RPC because the driver is a separate process from host-core, and it is
deliberately absent from the renderer's IPC allow-list: the desktop bridge
enumerates `staff.*` channels by hand, so a missing entry is the enforcement.

A step's outcome is read from its reply rather than guessed at: the driver takes
the last line that *starts* with `STEP RESULT:`. Both halves of that rule carry
weight. A step may quote the marker while it explains the workflow, and the first
match anywhere in the reply once stored the tail of such a sentence and published
it to shared memory as the run's signal (run 6e6e5672).

Every step prompt also states what the step was handed, including when that is
nothing: an empty memory read and a probe that collected nothing are written out
as `- (nothing: no memory item and no collected page reached this step)`. The step
is told to work from that material and from nothing else on the machine, and that
saying what is missing and stopping is a complete answer. Without that boundary a
step with no input has only the run's own licence to continue: step 2 of run
ac6038f1 was handed no memory item and no collected page, and answered by reading
scratch files left behind by unrelated sessions -- nine and a half minutes and
148k output tokens to produce "evidence" the employee never collected.

A finished step's outcome line travels to the steps after it, under `What this run
has already done:`. Without it a graph is a list of strangers: the node whose job
is to record what the team learned has nothing to record but its own narration,
and the closing step cannot say what the run did. A `memory` node that only reads
contributes nothing to that list, because the driver answers it without a turn.

A node that records asks for a different closing line. Its reply's last line is the
item, read back verbatim by whoever comes next, so it is asked to make that line the
finding itself rather than a note about the step. Asking every node for "what actually
happened" filed exactly that: the seeded scout wrote "发布了共享信号一条 -- 本轮无可用商机..."
and the seeded analyst wrote a file name plus "未读机上任何文件", and both were handed
back the next round as what the team knows. The wording lives in the prompt builder
rather than in each graph, so a node the owner draws later is held to the same contract.


## 13. Operator surface

The renderer is an operator console, not a second control plane: every action it
offers goes through the same `staff.*` RPCs the driver uses, and it holds no state
the database does not already hold.

It is reached from the sidebar's **My apps → AI team** row, which sits in the
sidebar footer area rather than behind the command palette, so residency is
visible without searching for it. The row carries a live badge: the in-flight run
count, or a dot whose tone is green while the driver is healthy, amber after a
scheduler error, and red when the driver is not resident.

The default tab is built around the second question an unattended system has to
answer: what, if anything, is waiting on the owner? It reads the same run history the
run log does, and an employee that already recovered is deliberately absent, because a
console that keeps asking about solved problems is a console nobody reads.

The page polls `staff.scheduler.status` every five seconds, the sidebar badge polls
every twenty, and both also refresh on the `pi-desktop/staff/event/changed` event,
which is what makes an unattended run visible without reloading anything.

| tab | what it does |
|---|---|
| Overview | the default tab: today's runs and success rate, then everything that needs a human -- a scheduler fault, an employee that is not scheduled to run, and each employee whose newest run failed or stopped on a gate |
| Employees | roster, credential profile, provider, model, thinking level, workflow and pinned version, schedule, quota |
| Workflows | the node graph, per-kind parameters, YAML, validation, version history |
| Memory | filtered browse, writes, promotion to shared, retirement, contested marks |
| Keys & models | credential profiles and write-only secrets |
| Run log | runs per employee, outcome, error, the result each finished step reported, and a jump into the session |

One check keeps the boundary honest end to end and sits deliberately outside the
unit suites, because it needs a real host process:

```text
node apps/desktop/test/manual/staff-wire-smoke.mjs
```

It drives the real host over NDJSON against a scratch data directory, waits for one
schedule slot to come due, and fails if any reply other than
`staff.run.binding` carries key material.

## 14. Preset employees

An empty roster is not a starting point, so a fresh console offers the built-in
roster. The first two are the pair the funnel rests on; the other five carry a
topic the rest of the way to money:

| preset id | name | job |
|---|---|---|
| `scout` | 侦察员 | sweeps the target communities, keeps what matches the product's signals, and files the finds as opportunity cards |
| `interestAnalyst` | 选题员 | reads the shared findings, decides which threads show real interest, and writes the conclusion back to shared memory for whoever writes the copy |
| `writer` | 笔杆子 | turns a topic into copy -- forum replies, short posts, video scripts -- in two or three openings for the owner to choose between |
| `poster` | 地推员 | takes approved wording out to the places the question was asked, one variant per placement, and records where each one went |
| `receptionist` | 接线员 | answers whoever came in, asks what only they can answer, grades the conversation, and hands the warm ones on as a proposal |
| `closer` | 跟单员 | carries conversations that are already warm through trial, quote, contract and payment, recording the blocker at each step |
| `aftersales` | 售后员 | follows up after delivery, collects what customers actually said, and looks for repeat business and referrals |

The names are product-facing and stay in Chinese; the ids are the stable keys.
The chain hands off through the boards rather than through each other: the
writer does not message the poster, both read the same rows. That is what makes
an unattended pipeline debuggable -- when a post is wrong, the row that said
what to write and the row that says who will say it are both still there.

A run files and never decides. A `produce` step whose config names
`artifact: leads | content | outreach | advice` ends its prompt with a contract
for one fenced JSON block, and the driver files what it parses: leads at stage
`new`, drafts at `draft` or `review` regardless of the status asked for, and
proposals on the named board with the run's own citation attached. A reply that
does not parse files nothing and logs a parse miss, because a half-parsed card
would be a claim about a real person that nobody made.

`staff.presets.list` returns each blueprint's roster entry -- id, name, role, workflow
name and description, channels, schedule, quota -- and whether an employee of that
name is already hired. It carries no graph: the roster entry is what an install
writes around the blueprint's own graph. `staff.presets.install` takes the preset ids plus one credential profile and
hires them in one transaction:

- an employee whose name already exists is skipped, never duplicated, so the call is
  safe to repeat on every launch
- both employees resolve to the owner's credential profile; the profile holds the key
  and the employee row never does
- an install with no resolvable secret still writes the roster entry but leaves it
  paused, so an unconfigured app does not start runs it cannot pay for
- a blueprint whose graph changed is not re-applied to an employee that is already
  hired, because a pinned workflow version is what a run reproduces. `install` with
  `adoptBlueprint` is the one way to move an existing employee onto the copy this
  build ships, and it appends a version rather than editing one, so the graph that
  employee had been running is still readable afterwards. `staff.presets.list`
  reports `blueprintChanged` per blueprint, which is what lets the console offer
  that without the owner diffing two graphs by hand.

The two blueprints are joined through shared memory rather than through each
other: the scout's `file` node publishes the demand signals its triage step
judged, and the analyst's first step reads that same scope for that same kind of
item. Until the scout had that node nothing it found ever left its own run -- the
analyst's read was answered by a store no employee had ever written, and the one
item in it had come from the analyst's own publish. A node that files a judgement
also has to say what to compress: a `publish` node records its own step's outcome,
so without an instruction it answers with its own narration, which is exactly what
the first round published.

The scout sweeps twice a day and the analyst runs once a day after the scout's
second sweep. Both carry a `maxRunsPerDay` ceiling, so a stuck workflow cannot
burn the owner's quota.

The scout's sweep step names only the sites that have a collector -- Zhihu today,
with the other five channels waiting on their own adapters. The list is the site
registry's, and the step is written around it: a sweep hands the employee what the
driver read, so naming a site nobody can read would send the employee off to fetch
it by hand, and the site answers that with the very login wall that stopped the
first round. The remit is unchanged; the coverage grows site by site.

The roster is hired by name, so an owner who renamed or deleted an employee
keeps that decision: installing a blueprint whose name -- or whose earlier name
-- is already on the roster is skipped rather than duplicated.

The desktop seeds the profile from `PI_STAFF_BOOTSTRAP_API_KEY`,
`PI_STAFF_BOOTSTRAP_BASE_URL`, and `PI_STAFF_BOOTSTRAP_MODEL_ID` when they are set, and otherwise
from `staff-bootstrap.json` in the data directory. The file is read once per install, and
its contents reach neither the renderer nor the log.

## 15. Site sessions

A sweep reads sites that gate their content behind a login, so the console
keeps one signed-in session per site, and the employees read through it.

The gate was measured rather than assumed. On 2026-09-20 a fresh Chromium
profile was pointed at `https://www.zhihu.com/hot`:

- the request landed on `/signin?next=%2Fhot`, so the document was the login
  wall and not the list;
- the anonymous session was issued `_xsrf`, `__zse_ck`, `_zap`, `d_c0`, `SESSIONID`,
  `JOID` and the Baidu counters, none of which unlock content;
- `z_c0`, the cookie a real login sets, was absent;
- the JSON endpoints failed from the page's own origin:
  `/api/v3/feed/topstory/hot-lists/total` answered `401` and
  `/api/v4/questions/<id>/answers` answered `403`, because both expect the
  `x-zse-96` signature that only the site's own bundle computes.

The collector therefore does not forge that signature, solve a captcha, or
spoof a fingerprint. It loads the page inside the site's own partition and
reads what rendered, which fails loudly -- a login wall, an empty list --
instead of quietly returning nothing.

Rules:

- One Chromium partition per site, `persist:pi-staff-site-<channel>`. Cookies are the
  session; they stay in Chromium's store and never enter `SecretStore` or a log.
- Signing in opens the site's own page in its own window. The owner scans the
  site's QR code with the site's app, so no password is typed into this app.
  The window closes by itself once the session carries the site's login cookie.
- A **manual** run that reaches a supported site with no session opens that
  site's sign-in window and waits for it, because the owner is at the console
  and asking for a sweep -- the window is the only way this process can read
  the site, and the run would otherwise come back empty. A scheduled run never
  opens a window: at 09:30 there is nobody to scan a code, so the site is
  reported as skipped instead.
- The session is signed in only while that cookie is present and unexpired.
  Signing out clears the partition's cookies, local storage, IndexedDB,
  cache storage, and service workers.
- The site window's user agent drops Chromium's `Electron/<version>` token,
  because that token describes a client the site has no rules for. Nothing else
  about the request is changed.
- A sweep opens the site's own timeout-bounded page once per run by default:
  `maxUrls` is 1 unless the owner raises it, and the hard ceiling is 20.
- The page is read in a window the owner never sees, with Chromium's
  background throttling off for that window only: a throttled page stops
  running the site's own scripts, and a list that never rendered would be
  reported as an empty one. Nothing else about the request is changed.
- A list page is titles, and nothing else. What a business signal has to be
  judged on -- the question's own text, the answers with their vote counts and
  dates, and the comments under them -- is one level down, so a sweep also
  follows the links the list page itself carried: 3 by default, never more
  than 5.
- Only a same-site `https` link pointing at a page that channel's detail reader
  knows (for Zhihu, `/question/`) is ever followed, so a link that leaves the
  site cannot be opened inside the site's signed-in session. Targets keep the
  page's own order rather than a re-ranking -- deciding what is worth reading
  is the triage step's job -- and a question reached from two lists is one
  page.
- A detail page is read the way a reader reads it. The collector scrolls a
  bounded number of times, because the answers below the fold are rendered as
  the reader scrolls, and it presses the answer's own comment count -- that
  count is a button, and its comments are not in the document until it is
  pressed. One answer is opened, read and closed at a time, so the panel never
  outlives the answer it belongs to. No endpoint is called and no signature is
  forged.
- A detail page that will not load, or that meets a login wall, is left out of
  the sweep and written to the app log. The list page it came from is already
  in the step, and one unreachable question must not turn a good sweep into a
  failed one.
- What one prompt may carry is capped at 24,000 characters across every page a
  sweep read, spent in the order the pages were read, with each page itself
  capped at 6,000. Pages over the cap are counted in the step rather than
  dropped silently.
- A site that is configured but not signed in is reported in the step as
  skipped. An unattended run must never let "we could not look" read as
  "there was nothing there".
- A probe step is told to read what the driver collected and that it has no
  equivalent fetcher of its own: the site answers anything that is not its own
  front end with the same login wall, so a hand-rolled fetch returns less than
  the driver is already holding, and the attempt spends the turn.
- A site with no collector is a gap in the product rather than a finding about
  this sweep, so it stays out of the step: the console already lists it as not
  wired up, and one repeated line per unimplemented channel every run would
  only spend the model's context on the same sentence.
- Site settings live in `staff-sites.json` in the data directory: a seed URL
  (http(s) only), a watchlist of search keywords, and the per-run page cap.
  Login state is never stored there; the cookie is the only source of truth.

The Tauri shell implements this in `staff_sites.rs`. The sign-in window is a
`WebviewWindow` on the site's own page, and the collector is a hidden one that
hands its result back through a `pigo-collect://` navigation the shell
intercepts, so no remote page gains IPC access. On macOS the shell cannot
express the per-site partition: WKWebView gives every webview in the process
one cookie store. The session is still read from the cookie, but the store is
shared rather than per-site, and sign-out deletes the site's cookies instead of
wiping the store, because wiping it would also drop the renderer's own
preferences.

The console exposes this as the `Sites` tab: one row per channel with its
state, a scan-to-sign-in button, a seed URL, a page cap, and a collect-once
button that reads one page and the pages it links to, so the owner can see what
a logged-in session
actually returns before trusting a scheduled run to it.

`apps/desktop/test/manual/staff-site-zhihu-live.cjs` runs these same modules
inside Electron against the live site. Without a profile it proves the gate:
an unsigned session is refused by name, `/hot` lands on the sign-in page with
no `z_c0`, and the sign-in page really renders a code to scan. Pointed at the
console's own profile through `PI_STAFF_SITE_USER_DATA` -- with the app closed,
because Chromium holds a lock on that profile -- it asserts that a scanned
session returns items.

### The reply watch

The shell's `staff_watch.rs` is the return leg. Every fifteen minutes, and
whenever the owner presses "read now", it asks host-core which published pieces
carry an address (`staff.watch.list`), opens each of those pages in the same
hidden collector, finds our piece by the fingerprint of its body, then reads
the reply count and the comments under it. Each reading is written back through
`staff.watch.ingest`; host-core writes the piece's count, today's topic trend,
and one card per stranger who answered.

A page that does not carry the fingerprint, or that cannot be opened because no
session is held, is reported as unread -- never as zero, and never as "nobody
answered". A reply becomes one card however often the watch runs, because the
card is keyed by the site, the author and the words themselves.

## 16. Boards

The employees produce rows; the boards are where the owner reads them and
decides. Four boards ship beside the console under "My apps":

| board | reads | the decision it holds |
|---|---|---|
| opportunities | `staff_leads` | which cards are worth answering, and who owns each |
| content studio | `staff_contents` | which draft version is approved, and what has gone out |
| daily report | `staff_metrics` plus every board | which topic is working, counted off the rows themselves |
| arsenal | shared memory and credential profiles | what every employee reads before it starts |

A board is a page over the same `staff.*` RPC surface -- not a second database,
not a second window, and not owned by one employee. The split is by decision:
`笔杆子` files drafts that appear in the content studio and `地推员` reads them
from the same table.

### Two doors

Filing and deciding are separate calls, and the second one is the owner's:

- `staff.lead.save` with a `runId` keeps whatever stage the card already had,
  and a new card filed by a run lands at `new`. `staff.content.save` with a
  `runId` can never produce a status above `draft`/`review`. An employee that
  could approve its own wording would make "approved" mean "written".
- `staff.lead.move` is the only path between columns, and
  `staff.content.status` is the only path to `approved`. Neither is reachable
  from a run. Moving a card stamps `contacted_at`, `replied_at` or `won_at`,
  which is what the daily board counts.

### Proposals

Every board carries the same rail: a proposal, the citations it rests on, what
it costs, and two buttons. `staff.advice.derive` reads the rows and raises what
the boards are waiting on -- cards untouched for twelve hours, people who
replied and were not answered, drafts awaiting approval, the topic outrunning
the others, replies that never became cards, shared wording gone stale. Each is
idempotent by `(board, title)` while it is still open, so the console can ask on
every render.

Deriving rather than generating is deliberate: a model asked what to do today
will always answer, and the answer is worth exactly the evidence behind it. A
run that wants to propose something writes a suggestion through
`staff.advice.create` with its own citations.

A proposal is raised together with the work order it would become, and the
board writes that order because the board is what knows whose job the follow-up
is: a card nobody picked up goes to `接线员`, a topic that is winning goes to
`笔杆子`, and a draft waiting to be approved goes to nobody, because only the
owner may approve one. Accepting hands the order over; the owner may redirect
it, and whatever they do not restate is kept.

`staff.advice.decide` is the only transition from proposal to action. Rejecting
keeps the citations and the note -- "we decided not to" is the most useful thing
a board can tell the next analysis. Accepting writes a `staff_directives` row
naming the employee, the verb and the payload; the directive reports back
through `staff.directive.update`, which closes the proposal with the same
sentence.

### Metrics

`staff_metrics` is a daily snapshot keyed by `(day, dimension, label)` over a
local day, where the dimension is `topic`, `channel` or `content`. A second
reading of the same counter overwrites rather than adds, because adding two
readings of the same counter would double it. Nothing on the daily board is
stored as a total: every figure is a count of rows that already exist, so the
report cannot disagree with the boards it summarizes.

### Bounds

`humanGate` remains a hard stop: a run that reaches one ends with `HUMAN_GATE`
rather than parking. The boards give the owner a place to decide after a run,
which is what the pipeline needed first; parking and resuming a run is a
scheduler change and is not claimed here.
