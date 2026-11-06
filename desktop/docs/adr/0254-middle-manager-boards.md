# 0254. Middle-manager boards: the rows employees file, and the one door that turns a proposal into work

* Status: Accepted for implementation

## Context

The staff employees became real before the boards did. The scout sweeps a
signed-in Zhihu, the analyst reads the replies, both write what they learned
into shared memory -- and what came out the other end was prose. Measured on
2026-09-20, the owner's console held eight active shared signals, every one of
them a paragraph of several hundred characters with no site, no URL, no author
and no state. There was nothing to sort, nothing to compare and nothing to
decide, because a paragraph has no columns.

Three things were missing, and they are the same thing seen from three sides:

* **There was no row for a decision to attach to.** A signal is a statement
  about the world; an opportunity is a statement about a person, and only the
  second one can be moved to a column called "replied".
* **There was no way to measure anything.** Nothing in the schema carried a
  view count or a reply count, so "which topic is working" had no answer, and
  the next day's writing had no input beyond the model's own recollection.
* **There was no place for a proposal to live.** `humanGate` exists as a node
  kind, but the driver treats it as a hard stop: `runFlight` ends the run with
  `HUMAN_GATE` rather than parking it. So the only way to keep a run
  "unattended" was to never ask the owner anything -- which is the opposite of
  what an unattended employee needs.

Meanwhile the console was the only surface. Everything an employee produced
could be read there, and nothing could be *done* there: the tabs answer "is my
team healthy", which is a different question from "what do I decide today".

## Decision

### 1. Four boards, one per decision, each a page rather than a process

`My apps` gains four entries beside the console: opportunities, content studio,
daily report and arsenal. They are React pages over the same `staff.*` RPC
surface, not separate applications, not separate databases, and not separate
windows. A board is a view; the console stays the only place an employee is
configured.

The split is by decision, not by employee. `笔杆子` files drafts that appear in
the content studio and `地推员` reads them from the same table; neither has a
board of its own, and no board belongs to one employee.

### 2. Five tables, and no second state store

`staff_leads`, `staff_contents`, `staff_metrics`, `staff_suggestions` and
`staff_directives` join the staff schema (v19 -> v20). They follow the rules the
rest of the module already keeps: a row that a run produced carries the
`employee_id` and `run_id` it came from, so a card can always be traced back to
the sentence that produced it, and nothing about a person is inferred -- the
quote is stored verbatim or the card is not filed.

`staff_metrics` is a daily snapshot keyed by `(day, dimension, label)`, local
day. A run reads the count a site prints and writes it; a second reading
overwrites rather than adds, because adding two readings of the same counter
would double it.

### 3. Two doors, and the owner holds the second one

Filing and deciding are separated deliberately:

* A run may **file**: `staff.lead.save` with a `runId` keeps whatever stage it
  finds, and `staff.content.save` with a `runId` can never produce a status
  above `draft`/`review`. An employee that could approve its own wording would
  make "approved" mean "written".
* Only the owner may **decide**: `staff.lead.move` sets a column and stamps
  `contacted_at` / `replied_at` / `won_at`, and `staff.content.status` is the
  only path to `approved`. No run reaches either.

Moving the stage stamps the milestone timestamps, and the daily board counts
those stamps rather than a separate total. That is why the report cannot
contradict the board it summarizes: every number on it is a count of rows that
already exist.

### 4. Proposals are derived from rows, not narrated by a model

`staff.advice.derive` reads the boards and raises what they are waiting on:
cards nobody has touched in twelve hours, people who replied and were not
answered, drafts awaiting approval, the topic outrunning the others, replies
that never became cards, stale shared wording. Each proposal carries citations
(`employeeId`, `runId`, `label`, `quote`) and a cost line, and each is
idempotent by `(board, title)` while it is still open.

Deriving rather than generating is the point. A model asked "what should I do
today" will always answer, and the answer is worth exactly the evidence behind
it -- so the analysis layer is deterministic over rows, and a run that wants to
propose something writes a suggestion through `staff.advice.create` with its
own citations.

### 5. Accepting a proposal is what creates work

`staff.advice.decide` is the only transition from proposal to action. Rejecting
keeps the citations and the note; accepting writes a `staff_directives` row
naming the employee, the verb and the payload. A directive reports back through
`staff.directive.update`, which closes the proposal with the same sentence --
so "we decided this and here is what happened" is a row, not a memory.

### 6. Employees file cards by ending their reply with data

A `produce` step whose config names `artifact: leads | content | outreach |
advice` gets a contract appended to its prompt: after the prose and before the
`STEP RESULT:` line, one fenced JSON block of a stated shape. The driver parses
the last fenced block, writes the rows with the run's identity attached, and
logs a parse miss instead of approximating one. A half-parsed card would be a
claim about a real person that nobody made.

## Consequences

* The owner's decisions become rows. "Why is this card here" and "who told that
  employee to do this" both have answers.
* A run can no longer be the last word on its own output: the prose is in the
  run log, the rows are on the board, and only the owner moves them.
* `humanGate` remains a hard stop. Parking a run and resuming it is a scheduler
  change, and this ADR does not make it -- the boards give the owner a place to
  decide *after* a run, which is what the pipeline actually needed first.

## Alternatives considered

* **A separate board database.** Rejected: two stores for one pipeline means a
  card and the run that filed it can disagree, which is the failure this whole
  module exists to prevent.
* **Storing signals as leads.** Rejected: the existing shared signals are
  paragraphs about the market, not cards about a person. Reusing the kind would
  have put unfalsifiable rows in a column called "contacted".
* **Letting a run set a stage.** Rejected: it would make "the owner decided
  this" and "a model decided this" the same row.
