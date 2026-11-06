-- Staff employees, credential profiles, runs, workflows, and memory
-- (ADR 0252, spec 03-runtime/20). Idempotent on purpose: a fresh database
-- executes this during creation and a v16 database executes it from the
-- v16 -> v17 migration, so both paths cannot drift.
--
-- No credential material is stored here. A profile holds only the secret_ref
-- that SecretStore resolves, and secrets_meta records the profile as the owner.

CREATE TABLE IF NOT EXISTS credential_profiles (
  id          TEXT PRIMARY KEY,
  label       TEXT NOT NULL,
  provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE RESTRICT,
  secret_ref  TEXT NOT NULL,
  base_url    TEXT,
  config_json TEXT NOT NULL DEFAULT '{}',
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_credential_profiles_provider
  ON credential_profiles(provider_id);

CREATE TABLE IF NOT EXISTS employees (
  id                    TEXT PRIMARY KEY,
  name                  TEXT NOT NULL,
  role                  TEXT NOT NULL DEFAULT '',
  credential_profile_id TEXT REFERENCES credential_profiles(id) ON DELETE RESTRICT,
  provider_id           TEXT,
  model_id              TEXT,
  thinking_level        TEXT,
  workflow_id           TEXT,
  workflow_version      INTEGER,
  schedule_json         TEXT NOT NULL DEFAULT '{}',
  quota_json            TEXT NOT NULL DEFAULT '{}',
  -- A worker does the errand; a manager reads the boards, decides, and hands
  -- the errands out. The column exists so "who may dispatch" is a fact about
  -- the employee rather than a name the code recognises (ADR 0254).
  layer                 TEXT NOT NULL DEFAULT 'worker',
  -- The board a manager owns. Empty for a worker.
  board                 TEXT NOT NULL DEFAULT '',
  status                TEXT NOT NULL DEFAULT 'paused',
  next_run_at           INTEGER,
  last_run_at           INTEGER,
  created_at            INTEGER NOT NULL,
  updated_at            INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_employees_status ON employees(status);

-- An employee that is due carries next_run_at; the partial index keeps the
-- scheduler's due scan proportional to what is actually pending.
CREATE INDEX IF NOT EXISTS idx_employees_due
  ON employees(next_run_at) WHERE next_run_at IS NOT NULL;

-- A workflow is an identity; its graph lives in immutable versions. A run pins
-- a version, so an unattended run can always say which revision it followed.
CREATE TABLE IF NOT EXISTS workflows (
  id             TEXT PRIMARY KEY,
  name           TEXT NOT NULL,
  description    TEXT NOT NULL DEFAULT '',
  status         TEXT NOT NULL DEFAULT 'draft',
  latest_version INTEGER NOT NULL DEFAULT 0,
  created_at     INTEGER NOT NULL,
  updated_at     INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS workflow_versions (
  id          TEXT PRIMARY KEY,
  workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
  version     INTEGER NOT NULL,
  graph_json  TEXT NOT NULL,
  node_count  INTEGER NOT NULL DEFAULT 0,
  created_by  TEXT NOT NULL DEFAULT 'user',
  created_at  INTEGER NOT NULL,
  UNIQUE(workflow_id, version)
);

CREATE INDEX IF NOT EXISTS idx_workflow_versions
  ON workflow_versions(workflow_id, version DESC);

CREATE TABLE IF NOT EXISTS employee_runs (
  id                    TEXT PRIMARY KEY,
  employee_id           TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
  session_id            TEXT REFERENCES sessions(id) ON DELETE SET NULL,
  -- The directive a run is carrying out, when a manager dispatched it. It is
  -- how "what did that order actually produce" is answered months later.
  directive_id          TEXT,
  credential_profile_id TEXT,
  provider_id           TEXT,
  model_id              TEXT,
  trigger               TEXT NOT NULL DEFAULT 'schedule',
  status                TEXT NOT NULL DEFAULT 'running',
  workflow_id           TEXT,
  workflow_version      INTEGER,
  steps_json            TEXT NOT NULL DEFAULT '[]',
  current_step          INTEGER NOT NULL DEFAULT 0,
  results_json          TEXT NOT NULL DEFAULT '{}',
  reports_json          TEXT NOT NULL DEFAULT '{}',
  error_code            TEXT,
  started_at            INTEGER NOT NULL,
  ended_at              INTEGER
);

CREATE INDEX IF NOT EXISTS idx_employee_runs
  ON employee_runs(employee_id, started_at DESC);

CREATE TABLE IF NOT EXISTS memory_items (
  id            TEXT PRIMARY KEY,
  scope         TEXT NOT NULL,
  owner_id      TEXT,
  kind          TEXT NOT NULL,
  memory_key    TEXT NOT NULL DEFAULT '',
  content       TEXT NOT NULL,
  source_kind   TEXT NOT NULL,
  source_ref    TEXT NOT NULL DEFAULT '',
  confidence    REAL NOT NULL DEFAULT 0.5,
  status        TEXT NOT NULL DEFAULT 'active',
  supersedes_id TEXT,
  employee_id   TEXT,
  run_id        TEXT,
  created_at    INTEGER NOT NULL,
  expires_at    INTEGER
);

CREATE INDEX IF NOT EXISTS idx_memory_lookup
  ON memory_items(scope, kind, status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_memory_owner
  ON memory_items(scope, owner_id, status);

CREATE INDEX IF NOT EXISTS idx_memory_key
  ON memory_items(memory_key) WHERE memory_key <> '';

CREATE INDEX IF NOT EXISTS idx_memory_supersedes
  ON memory_items(supersedes_id) WHERE supersedes_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- The boards the middle managers read (ADR 0252, spec 03-runtime/20).
--
-- The employees produce evidence; these five tables hold what that evidence is
-- turned into. They are ordinary rows, not a second copy of the run log: every
-- board row that a run produced carries the run id and the employee id it came
-- from, so a card on a board can always be traced back to the sentence that
-- produced it. A suggestion is never applied by the board that wrote it -- it
-- is a proposal with evidence, and only an accepted one becomes a directive
-- with a named target and an owner.
-- ---------------------------------------------------------------------------

-- One opportunity on the business board: a person who said something we can
-- answer. `stage` is the column the card sits in; `score` is how strongly the
-- quoted words support a real need, and both are the owner's to change.
CREATE TABLE IF NOT EXISTS staff_leads (
  id           TEXT PRIMARY KEY,
  title        TEXT NOT NULL,
  site         TEXT NOT NULL DEFAULT '',
  url          TEXT NOT NULL DEFAULT '',
  author       TEXT NOT NULL DEFAULT '',
  quote        TEXT NOT NULL DEFAULT '',
  topic        TEXT NOT NULL DEFAULT '',
  angle        TEXT NOT NULL DEFAULT '',
  stage        TEXT NOT NULL DEFAULT 'new',
  score        INTEGER NOT NULL DEFAULT 0,
  tags_json    TEXT NOT NULL DEFAULT '[]',
  note         TEXT NOT NULL DEFAULT '',
  employee_id  TEXT,
  run_id       TEXT,
  owner_id     TEXT,
  contacted_at INTEGER,
  replied_at   INTEGER,
  won_at       INTEGER,
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_staff_leads_stage
  ON staff_leads(stage, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_staff_leads_run
  ON staff_leads(run_id) WHERE run_id IS NOT NULL;

-- One thing we intend to say somewhere. A content row outlives the run that
-- drafted it: the draft is edited, approved, published, and then measured, and
-- only the last of those is a fact about the world rather than an intention.
CREATE TABLE IF NOT EXISTS staff_contents (
  id            TEXT PRIMARY KEY,
  title         TEXT NOT NULL,
  channel       TEXT NOT NULL DEFAULT '',
  topic         TEXT NOT NULL DEFAULT '',
  body          TEXT NOT NULL DEFAULT '',
  variant       TEXT NOT NULL DEFAULT '',
  status        TEXT NOT NULL DEFAULT 'draft',
  lead_id       TEXT,
  employee_id   TEXT,
  run_id        TEXT,
  approved_at   INTEGER,
  published_at  INTEGER,
  published_url TEXT NOT NULL DEFAULT '',
  views         INTEGER NOT NULL DEFAULT 0,
  replies       INTEGER NOT NULL DEFAULT 0,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_staff_contents_status
  ON staff_contents(status, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_staff_contents_run
  ON staff_contents(run_id) WHERE run_id IS NOT NULL;

-- One day's numbers for one label. `dimension` says what the label counts --
-- a topic, a channel, or a single published piece -- so the daily board can
-- rank topics and attribute replies in the same pass.
CREATE TABLE IF NOT EXISTS staff_metrics (
  id         TEXT PRIMARY KEY,
  day        TEXT NOT NULL,
  dimension  TEXT NOT NULL DEFAULT 'topic',
  label      TEXT NOT NULL,
  views      INTEGER NOT NULL DEFAULT 0,
  replies    INTEGER NOT NULL DEFAULT 0,
  leads      INTEGER NOT NULL DEFAULT 0,
  inquiries  INTEGER NOT NULL DEFAULT 0,
  deals      INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(day, dimension, label)
);

CREATE INDEX IF NOT EXISTS idx_staff_metrics_day
  ON staff_metrics(day DESC, dimension, views DESC);

-- A middle manager's proposal. `evidence_json` holds the citations the claim
-- rests on, and `status` is the owner's answer -- an agent may write a
-- suggestion, but only the owner accepting one turns it into work.
CREATE TABLE IF NOT EXISTS staff_suggestions (
  id             TEXT PRIMARY KEY,
  board          TEXT NOT NULL DEFAULT 'leads',
  title          TEXT NOT NULL,
  body           TEXT NOT NULL DEFAULT '',
  action         TEXT NOT NULL DEFAULT '',
  impact         TEXT NOT NULL DEFAULT '',
  cost_note      TEXT NOT NULL DEFAULT '',
  evidence_json  TEXT NOT NULL DEFAULT '[]',
  status         TEXT NOT NULL DEFAULT 'open',
  directive_json TEXT NOT NULL DEFAULT '{}',
  result_note    TEXT NOT NULL DEFAULT '',
  decided_at     INTEGER,
  created_at     INTEGER NOT NULL,
  updated_at     INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_staff_suggestions_board
  ON staff_suggestions(board, status, created_at DESC);

-- What the owner approved, and what came of it. A directive names the employee
-- it was handed to, so "who was told to do this" is a column and not a story.
CREATE TABLE IF NOT EXISTS staff_directives (
  id            TEXT PRIMARY KEY,
  suggestion_id TEXT,
  employee_id   TEXT,
  employee_name TEXT NOT NULL DEFAULT '',
  -- The middle manager that handed this out, and the decision it made in the
  -- same breath. Both are null for a directive the owner approved, which is
  -- the older path and still the one that carries a proposal.
  issuer_employee_id TEXT,
  reason_run_id      TEXT,
  -- After this moment the errand is stale and the worker is left alone: an
  -- unattended order that missed its window must not surface a day later.
  deadline_at   INTEGER,
  priority      INTEGER NOT NULL DEFAULT 0,
  -- The run that picked it up, so a directive can name its own execution.
  claimed_run_id TEXT,
  board         TEXT NOT NULL DEFAULT '',
  verb          TEXT NOT NULL DEFAULT '',
  payload_json  TEXT NOT NULL DEFAULT '{}',
  status        TEXT NOT NULL DEFAULT 'queued',
  result_note   TEXT NOT NULL DEFAULT '',
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL,
  ended_at      INTEGER
);

CREATE INDEX IF NOT EXISTS idx_staff_directives_status
  ON staff_directives(status, updated_at DESC);

-- A queued directive is the scheduler's second due source, so the scan for it
-- is kept proportional to what is actually pending.
CREATE INDEX IF NOT EXISTS idx_staff_directives_queued
  ON staff_directives(employee_id, created_at) WHERE status = 'queued';

-- ---------------------------------------------------------------------------
-- What a middle manager decided (ADR 0254, spec 03-runtime/20).
--
-- A decision is what a manager produces before any work exists: the conclusion
-- it reached, the rows it read to reach it, and what it handed to whom. It is
-- stored rather than narrated because the console shows the manager's own
-- reasoning as the first thing on its board -- a manager whose work the owner
-- can only see as "the cards it eventually caused" is a black box with a
-- headcount.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS staff_decisions (
  id              TEXT PRIMARY KEY,
  employee_id     TEXT NOT NULL,
  employee_name   TEXT NOT NULL DEFAULT '',
  run_id          TEXT,
  board           TEXT NOT NULL DEFAULT '',
  -- The one-line conclusion, which is also the card the owner reads first.
  title           TEXT NOT NULL,
  verdict         TEXT NOT NULL DEFAULT '',
  -- The board rows the judgement was made from, and the quotes it cites.
  inputs_json     TEXT NOT NULL DEFAULT '[]',
  evidence_json   TEXT NOT NULL DEFAULT '[]',
  -- What this judgement handed to whom, when it handed anything out.
  dispatched_json TEXT NOT NULL DEFAULT '[]',
  suggestion_id   TEXT,
  note            TEXT NOT NULL DEFAULT '',
  created_at      INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_staff_decisions_board
  ON staff_decisions(board, created_at DESC);

