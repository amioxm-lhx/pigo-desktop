/** Shared wire types for the staff (virtual employee) domain. */

/**
 * A named account an employee runs on.
 *
 * The key itself never crosses this boundary: the row carries a secretRef that
 * only host-core can resolve, plus a hasSecret flag the UI shows instead.
 */
export type StaffCredentialProfile = {
  id: string;
  label: string;
  providerId: string;
  secretRef: string;
  baseUrl?: string;
  hasSecret: boolean;
  createdAt: string;
  updatedAt: string;
};

/** How often an employee runs. An empty object means manual only. */
export type StaffSchedule = {
  everyMinutes?: number;
  jitterMinutes?: number;
  /** Minutes from local midnight, ascending, e.g. [540, 1200] for 09:00/20:00. */
  timesOfDay?: number[];
  utcOffsetMinutes?: number;
};

/** Safety rails that keep an unattended employee from running away. */
export type StaffQuota = {
  maxRunsPerDay?: number;
  /**
   * How many errands a manager may hand to workers in one local day. The owner
   * does not approve each errand, so this ceiling is what bounds a manager.
   */
  maxDirectivesPerDay?: number;
};

/**
 * What an employee is for. A worker does the errand it was given; a manager
 * reads a board, decides, and hands errands out.
 */
export const STAFF_EMPLOYEE_LAYERS = ["worker", "manager"] as const;
export type StaffEmployeeLayer = (typeof STAFF_EMPLOYEE_LAYERS)[number];

export type StaffEmployeeStatus = "paused" | "running" | "stopped";

export type StaffEmployee = {
  id: string;
  name: string;
  role: string;
  /** `worker` does the errand; `manager` hands errands out. */
  layer: StaffEmployeeLayer;
  /** The board a manager owns; empty for a worker. */
  board: string;
  credentialProfileId?: string;
  providerId?: string;
  modelId?: string;
  thinkingLevel?: string;
  workflowId?: string;
  workflowVersion?: number;
  status: StaffEmployeeStatus;
  schedule: StaffSchedule;
  quota: StaffQuota;
  nextRunAt?: string;
  lastRunAt?: string;
  createdAt: string;
  updatedAt: string;
};

/** `directive` is a run carrying out a manager's order. */
export type StaffRunTrigger = "schedule" | "manual" | "event" | "directive";
export type StaffRunStatus = "running" | "succeeded" | "failed" | "aborted";

export type StaffRun = {
  id: string;
  employeeId: string;
  /** The manager's order this run is carrying out, when it is carrying one. */
  directiveId?: string;
  sessionId?: string;
  credentialProfileId?: string;
  providerId?: string;
  modelId?: string;
  trigger: StaffRunTrigger;
  status: StaffRunStatus;
  workflowId?: string;
  workflowVersion?: number;
  stepCount: number;
  currentStep: number;
  /** The pinned steps' titles in execution order. */
  stepTitles: string[];
  /**
   * What each finished step reported, keyed by its one-based step number. A
   * run that died mid-way has no entry for the step it died in.
   */
  results?: Record<string, string>;
  /**
   * Each finished step's full reply, keyed like `results`: the collected
   * material the one-line result summarizes.
   */
  reports?: Record<string, string>;
  errorCode?: string;
  startedAt: string;
  endedAt?: string;
};

/** Node kinds a workflow graph may contain. A closed set, enforced by host-core. */
export const STAFF_NODE_KINDS = [
  "trigger",
  "probe",
  "analyze",
  "produce",
  "agent",
  "memory",
  "condition",
  "handoff",
  "humanGate",
  "notify",
  /** Reads and dispatches. Only a manager's workflow may contain one. */
  "board",
] as const;

/**
 * What a `board` node can be told to do.
 *
 * `read` and `report` are answered deterministically or filed as a proposal;
 * `judge` writes a decision card and queues the orders it decided on.
 */
export const STAFF_BOARD_ACTIONS = ["read", "judge", "dispatch", "report"] as const;
export type StaffBoardAction = (typeof STAFF_BOARD_ACTIONS)[number];
export type StaffNodeKind = (typeof STAFF_NODE_KINDS)[number];

/** Layout is view state, but it lives in the graph so a version keeps its shape. */
export type StaffNodePosition = { x: number; y: number };

export type StaffGraphNode = {
  id: string;
  kind: StaffNodeKind;
  title?: string;
  instruction?: string;
  config?: Record<string, unknown>;
  position?: StaffNodePosition;
};

export type StaffGraphEdge = { from: string; to: string };

export type StaffGraph = {
  nodes: StaffGraphNode[];
  edges: StaffGraphEdge[];
};

export type StaffWorkflowStatus = "draft" | "active" | "archived";

export type StaffWorkflow = {
  id: string;
  name: string;
  description: string;
  status: StaffWorkflowStatus;
  latestVersion: number;
  nodeCount?: number;
  createdAt: string;
  updatedAt: string;
};

export type StaffWorkflowVersion = {
  id: string;
  workflowId: string;
  version: number;
  nodeCount: number;
  createdBy: string;
  createdAt: string;
};

/** One compiled node of a run's pinned graph, in execution order. */
export type StaffWorkflowStep = {
  nodeId: string;
  kind: StaffNodeKind;
  title: string;
  instruction: string;
  /** The node's config as written, so the driver can render it as parameters. */
  config: Record<string, unknown>;
  gate?: string;
  handoffEmployeeId?: string;
  memory?: {
    operation: "read" | "write" | "publish" | "retire";
    scope?: "employee" | "shared";
    kind?: StaffMemoryKind;
    limit?: number;
  };
};

/** The editor's answer to "would this graph be accepted?", asked before saving. */
export type StaffGraphValidation = {
  valid: boolean;
  error?: string;
  nodeCount?: number;
  steps?: number;
};

export type StaffMemoryScope = "employee" | "shared";
export type StaffMemoryKind = "fact" | "preference" | "playbook" | "constraint" | "signal";
export type StaffMemorySourceKind = "user" | "interaction" | "run" | "url";
export type StaffMemoryStatus = "active" | "superseded" | "retired";

export type StaffMemoryItem = {
  id: string;
  scope: StaffMemoryScope;
  ownerId?: string;
  kind: StaffMemoryKind;
  key: string;
  text: string;
  sourceKind: StaffMemorySourceKind;
  sourceRef: string;
  confidence: number;
  status: StaffMemoryStatus;
  supersedesId?: string;
  employeeId?: string;
  runId?: string;
  createdAt: string;
  expiresAt?: string;
  /** Two live items share a key with different text; the pair is shown, not merged. */
  contested?: boolean;
};

/**
 * Channels a probe step may collect from.
 *
 * The set is closed for the same reason node kinds are: an unattended run
 * cannot be allowed to invent a source nobody configured a collector for. The
 * two WeChat channels are first-class here because they are where the
 * Chinese-language conversations actually happen, even though they need a
 * logged-in session rather than a public feed.
 */
export const STAFF_SOURCE_CHANNELS = [
  "juejin",
  "zhihu",
  "bilibili",
  "wechatMp",
  "wechatChannels",
] as const;

export type StaffSourceChannel = (typeof STAFF_SOURCE_CHANNELS)[number];

/** i18n key for a channel's display name, e.g. staff.config.sourceWechatMp. */
export function staffSourceChannelKey(channel: string): string {
  return "staff.config.source" + channel.charAt(0).toUpperCase() + channel.slice(1);
}

/**
 * Whether a channel's output lands nowhere the owner can point at.
 *
 * A private message is sent, not posted: there is no page to read it back
 * from, so asking for its address would chase a link that cannot exist and
 * counting it as work would leave a number on the rail that never clears.
 * Every other channel lands somewhere a reply watch can revisit later.
 *
 * The whole point of putting this here rather than in the page that first
 * needed it: the desk, the rail's badge and the pipeline all ask the same
 * question, and one of them answering it differently is a page that disagrees
 * with the page next to it.
 */
export function addresslessChannel(channel: string | undefined): boolean {
  return (channel ?? "").trim().toLowerCase() === "direct";
}

/**
 * A published piece whose return leg has no address yet: the owner's task.
 *
 * Being published is not the same as being readable. The watch reads replies
 * back from the address the piece went out under, so a piece that went out
 * with no address -- and could have had one -- is work waiting on the owner,
 * which is what the badge on the rail counts and what the desk asks for.
 */
export function needsAddress(content: Pick<StaffContent, "status" | "publishedUrl" | "channel">): boolean {
  return (
    content.status === "published" &&
    !content.publishedUrl &&
    !addresslessChannel(content.channel)
  );
}

/** A built-in employee blueprint the console can install in one action. */
export type StaffPreset = {
  id: string;
  /** Display name for the seeded employee, e.g. 侦察员. */
  name: string;
  /** `worker` or `manager`. */
  layer: StaffEmployeeLayer;
  /** The board a manager blueprint owns; empty for a worker. */
  board: string;
  role: string;
  workflowName: string;
  workflowDescription: string;
  schedule: StaffSchedule;
  quota: StaffQuota;
  /** Channels this employee's probe steps collect from. */
  channels: StaffSourceChannel[];
  /** True once an employee of this name already exists. */
  installed: boolean;
  /**
   * True when the hired employee runs a different graph than this blueprint.
   *
   * The console offers the blueprint back rather than applying it: a blueprint
   * is data, not a migration, so moving an employee onto a newer copy is the
   * owner's decision and is reported here so they can make it.
   */
  blueprintChanged?: boolean;
};

export type StaffPresetInstallResult = {
  installed: string[];
  /** Employees moved onto this build's copy of their blueprint. */
  updated?: string[];
  skipped: string[];
  providerId?: string;
  credentialProfileId?: string;
};

/** What the resident driver is doing right now, for the console header. */
/**
 * What the return leg last did, and when it runs next.
 *
 * The console shows this rather than a per-card form, because the numbers it
 * carries are read off the page by the watch: the owner's job is to publish
 * and to answer, not to type in what the site already knows.
 */
export type StaffWatchStatus = {
  resident: boolean;
  intervalMs: number;
  running: boolean;
  nextAt: string;
  last?: StaffWatchOutcome;
  lastError?: string;
};

/** One pass of the watch, as the console reports it. */
export type StaffWatchOutcome = {
  at: string;
  trigger: "timer" | "manual";
  targets: number;
  found: number;
  newReplies: number;
  leads: number;
  lines: string[];
};
export type StaffSchedulerStatus = {
  resident: boolean;
  pollMs: number;
  lastTickAt?: string;
  lastError?: string;
  claimedTotal: number;
  inFlight: Array<{
    runId: string;
    employeeId: string;
    employeeName: string;
    sessionId: string;
    step: number;
    stepCount: number;
  }>;
};

/**
 * A target site the employees collect from, and whether this machine holds a
 * signed-in session for it.
 *
 * The session itself is not described here: it lives in the site's own
 * Chromium partition, and the only fact the console needs is whether the
 * cookie that site issues after a real login is present.
 */
export type StaffSite = {
  channel: StaffSourceChannel;
  /** False when no collector exists for this channel yet. */
  supported: boolean;
  signedIn: boolean;
  signedInAt?: string;
  /**
   * One page pinned by the owner, or empty.
   *
   * A non-empty value wins outright and the watchlist is not read, which is
   * how a single page is debugged. Left empty, a sweep reads the search page
   * of each keyword below instead.
   */
  seedUrl: string;
  maxUrls: number;
  /** The questions this site is watched for; one search page each per sweep. */
  keywords: string[];
  /** What the site gates on, so the owner knows what the login unlocks. */
  gate: "login" | "none";
};

export type StaffSiteSignInResult = {
  signedIn: boolean;
  /** True when the owner closed the login window without finishing. */
  cancelled?: boolean;
};

/**
 * One page a collector actually read, as the run log shows it.
 *
 * A sweep reads two levels: the list page a site hands a signed-in visitor,
 * and the pages that list links to, where the content and the comments live.
 * `kind` is what tells the two apart in a run log, and `summary` is the
 * detail page's own preamble -- a question's body, or how many answers it has.
 */
export type StaffCollectedPage = {
  channel: StaffSourceChannel;
  url: string;
  title: string;
  itemCount: number;
  items: Array<{
    title: string;
    url: string;
    excerpt?: string;
    /** What the page printed next to the row: a vote count, a date. */
    meta?: string;
    /** Which level of the page the row came from. `item` is a list row. */
    kind?: "item" | "answer" | "comment";
  }>;
  /** The page's own preamble, when it has one. */
  summary?: string;
  /** Set instead of items when the page could not be read. */
  error?: string;
};

/** The columns an opportunity card may sit in, in board order. */
export const STAFF_LEAD_STAGES = [
  "new",
  "qualified",
  "contacted",
  "replied",
  "talking",
  "won",
  "lost",
  "dropped",
] as const;
export type StaffLeadStage = (typeof STAFF_LEAD_STAGES)[number];

/**
 * One opportunity: somebody said something we can answer.
 *
 * `quote` and `url` are the point of the row and everything else is filing.
 * A card that cannot be clicked back to the sentence it came from is an
 * opinion, so the board shows the sentence first and the score second.
 */
export type StaffLead = {
  id: string;
  title: string;
  site: string;
  url: string;
  author: string;
  quote: string;
  topic: string;
  angle: string;
  stage: StaffLeadStage;
  score: number;
  tags: string[];
  note: string;
  employeeId?: string;
  /** The run that filed this card, so the board can cite its evidence. */
  runId?: string;
  ownerId?: string;
  contactedAt?: string;
  repliedAt?: string;
  wonAt?: string;
  createdAt: string;
  updatedAt: string;
};

export const STAFF_CONTENT_STATUSES = [
  "draft",
  "review",
  "approved",
  // Decided, and handed to the poster: not out in the world yet, and no
  // longer waiting on the owner.
  "queued",
  "published",
  "retired",
] as const;
export type StaffContentStatus = (typeof STAFF_CONTENT_STATUSES)[number];

/** One thing we intend to say somewhere, and what came of saying it. */
export type StaffContent = {
  id: string;
  title: string;
  channel: string;
  topic: string;
  body: string;
  /** The slot this draft fills in an A/B pair, e.g. "A" or "B". */
  variant: string;
  status: StaffContentStatus;
  leadId?: string;
  employeeId?: string;
  runId?: string;
  approvedAt?: string;
  publishedAt?: string;
  publishedUrl: string;
  views: number;
  replies: number;
  createdAt: string;
  updatedAt: string;
};

export const STAFF_METRIC_DIMENSIONS = ["topic", "channel", "content"] as const;
export type StaffMetricDimension = (typeof STAFF_METRIC_DIMENSIONS)[number];

/** One local day's numbers for one label. */
export type StaffMetric = {
  id: string;
  day: string;
  dimension: StaffMetricDimension;
  label: string;
  views: number;
  replies: number;
  leads: number;
  inquiries: number;
  deals: number;
  createdAt: string;
  updatedAt: string;
};

/** One day's funnel, counted off the cards that already exist. */
export type StaffDayFunnel = {
  day: string;
  leads: number;
  contacted: number;
  replied: number;
  won: number;
  approved: number;
  published: number;
  views: number;
  replies: number;
};

/** A day's worth of ranking plus the two weeks behind it. */
export type StaffReportSummary = {
  day: string;
  today: StaffDayFunnel;
  yesterday: StaffDayFunnel;
  topics: Array<{
    label: string;
    views: number;
    replies: number;
    leads: number;
    inquiries: number;
    deals: number;
  }>;
  trend: Array<{
    day: string;
    views: number;
    replies: number;
    leads: number;
    inquiries: number;
    deals: number;
  }>;
  pipeline: Partial<Record<StaffLeadStage, number>>;
  days: number;
};

/**
 * One stage of the data ledger: what a station holds, and what a sweep takes.
 *
 * `cleanable` is measured by the same predicate the delete uses, so the number
 * the owner reads before confirming is the number of rows that will actually go.
 */
export type StaffLedgerStage = {
  /** One of scan / leads / contents / publish / metrics / dispatch / runs / memory. */
  id: string;
  rows: number;
  bytes: number;
  /**
   * Rows this stage wrote, still keeps on disk, and nothing uses any more.
   *
   * A retired draft is a row on disk like any other. Counting only the live
   * ones would print "0" over a table full of text, so the archived half is
   * reported next to the live half rather than folded into it.
   */
  parkedRows: number;
  parkedBytes: number;
  /** Rows this stage's own rule would take today. Never more than parkedRows. */
  cleanable: number;
  cleanableBytes: number;
};

/**
 * The ledger behind the data page.
 *
 * `database` is the whole SQLite file rather than the sum of the stages: the
 * page's point is that this team's rows are a small part of it, and the rest is
 * the owner's own conversation history.
 */
export type StaffDataStats = {
  stages: StaffLedgerStage[];
  database: { bytes: number };
  retention: {
    leads: number;
    contents: number;
    dispatch: number;
    metrics: number;
    runsKept: number;
    runs: number;
  };
};

/** What a cleanup took, or would take when the call was a dry run. */
/** What happened when the owner exported the ledger. */
export type StaffDataExportResult = {
  /** False when the owner closed the save dialog, which is not a failure. */
  saved: boolean;
  path?: string;
  bytes?: number;
};

export type StaffPruneResult = {
  dryRun: boolean;
  taken: Array<{ rule: string; rows: number; bytes: number }>;
};

export const STAFF_SUGGESTION_BOARDS = ["leads", "contents", "report", "arsenal"] as const;
export type StaffSuggestionBoard = (typeof STAFF_SUGGESTION_BOARDS)[number];

export type StaffSuggestionStatus = "open" | "accepted" | "rejected" | "done";
export type StaffDirectiveStatus = "queued" | "running" | "done" | "failed";

/**
 * One citation a proposal rests on.
 *
 * This is what makes a claim checkable: an employee, the run, and the step
 * inside it, so the owner can open the very reply the sentence came from
 * instead of taking the summary's word for it.
 */
export type StaffEvidence = {
  employeeId?: string;
  employeeName?: string;
  runId?: string;
  step?: number;
  label: string;
  quote?: string;
};

/** What the owner approved, and what came of it. */
export type StaffDirective = {
  id: string;
  suggestionId?: string;
  employeeId?: string;
  employeeName: string;
  /** The middle manager that handed this out, when one did. */
  issuerEmployeeId?: string;
  /** The judgement this order came out of. */
  reasonRunId?: string;
  /** After this moment the order is stale and stays undone. */
  deadlineAt?: string;
  priority: number;
  /** The run that picked it up. */
  claimedRunId?: string;
  board: string;
  verb: string;
  payload: Record<string, unknown>;
  status: StaffDirectiveStatus;
  resultNote: string;
  createdAt: string;
  updatedAt: string;
  endedAt?: string;
};

/** A middle manager's proposal, with the citations it rests on. */
export type StaffSuggestion = {
  id: string;
  board: StaffSuggestionBoard;
  title: string;
  body: string;
  action: string;
  impact: string;
  costNote: string;
  evidence: StaffEvidence[];
  status: StaffSuggestionStatus;
  directive?: StaffDirective;
  resultNote: string;
  decidedAt?: string;
  createdAt: string;
  updatedAt: string;
};

/**
 * What a board node read, as the manager's prompt receives it.
 *
 * Deterministic: the counts and the quotes come from the same tables the owner
 * is looking at, so "why did it say that" is answered by the board itself.
 */
export type StaffBoardBrief = {
  board: string;
  /** One line per thing the manager saw, headed [看板] or [我派的活]. */
  notes: string[];
  summary: Array<{ label: string; count: number }>;
};

/**
 * One thing a middle manager concluded.
 *
 * The console draws these above the cards the manager's orders eventually
 * produced: the card is the reasoning, and the board below it is the result.
 * `note` holds what could not be handed out and why -- a refusal is a result
 * the owner has to be able to read, not a silence.
 */
export type StaffDecision = {
  id: string;
  employeeId: string;
  employeeName: string;
  runId?: string;
  board: StaffSuggestionBoard;
  title: string;
  verdict: string;
  inputs: string[];
  evidence: StaffEvidence[];
  /** The orders this judgement handed out. */
  dispatched: StaffDirective[];
  suggestionId?: string;
  note: string;
  createdAt: string;
};

/** What one manager may hand out, and what it has spent of today's ceiling. */
export type StaffDispatchScope = {
  employeeId: string;
  board: string;
  targets: string[];
  verbs: string[];
  spentToday: number;
  maxPerDay: number;
};
