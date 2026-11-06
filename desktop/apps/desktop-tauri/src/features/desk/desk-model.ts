/**
 * The owner's desk: one round of the team's work, read off the rows that exist.
 *
 * Three pages read this file -- Today, Pipeline and Data -- and every number in
 * them is a count of something the host already stores. Nothing here estimates,
 * pads or invents. Where a fact genuinely is not kept -- the pages a sweep reads
 * are thrown away the moment its run ends -- the station says so instead of
 * showing a zero, because a zero reads like a failure and this is not one.
 *
 * The pipeline is the same tables the boards show, cut by stage instead of by
 * column, so that "where is the round" has exactly one answer. The order is the
 * order work actually travels:
 *
 *   巡站 -> 分级 -> 选题 -> 写稿 -> 定稿 -> 发布 -> 回音 -> 接洽 -> 成交 -> 交付
 *
 * Three of those ten stations are the owner's own door or the world's answer;
 * the middle two -- 定稿 and 发布 -- belong to nobody but the owner, which is
 * the one thing the state colour on every card is there to say.
 */
import type { TFunction } from "i18next";
import type {
  StaffContent,
  StaffDirective,
  StaffEmployee,
  StaffLead,
  StaffMemoryItem,
  StaffMetric,
  StaffReportSummary,
  StaffRun,
  StaffSchedulerStatus,
  StaffSite,
  StaffSuggestion,
  StaffWatchStatus,
} from "@pi-desktop/shared";
import {
  agoLabel,
  contentStatusKey,
  leadStageKey,
} from "../boards/board-format";
import { runStatusLabel } from "../staff/staff-format";
import { addresslessChannel, needsAddress, staffSourceChannelKey } from "@pi-desktop/shared";

/** Everything the desk reads, loaded in one pass so the three pages agree. */
export type DeskSnapshot = {
  employees: StaffEmployee[];
  runs: StaffRun[];
  contents: StaffContent[];
  leads: StaffLead[];
  metrics: StaffMetric[];
  advice: StaffSuggestion[];
  directives: StaffDirective[];
  memory: StaffMemoryItem[];
  sites: StaffSite[];
  watch: StaffWatchStatus | null;
  scheduler: StaffSchedulerStatus | null;
  summary: StaffReportSummary | null;
};

export const EMPTY_DESK: DeskSnapshot = {
  employees: [],
  runs: [],
  contents: [],
  leads: [],
  metrics: [],
  advice: [],
  directives: [],
  memory: [],
  sites: [],
  watch: null,
  scheduler: null,
  summary: null,
};

/* ------------------------------------------------------------------ time -- */

/** Midnight local, which is the day every "today" count on these pages is cut by. */
export function localDayStart(at: number | Date = Date.now()): number {
  const day = new Date(at instanceof Date ? at.getTime() : at);
  day.setHours(0, 0, 0, 0);
  return day.getTime();
}

/** True when a stored timestamp falls on or after an instant. */
export function happenedSince(value: string | undefined, since: number): boolean {
  if (!value) return false;
  const at = Date.parse(value);
  return Number.isFinite(at) && at >= since;
}

/** The newest of a set of timestamps, or nothing when none of them parse. */
export function newestAt(values: Array<string | undefined>): string | undefined {
  let newest: string | undefined;
  let newestMs = Number.NEGATIVE_INFINITY;
  for (const value of values) {
    if (!value) continue;
    const at = Date.parse(value);
    if (Number.isFinite(at) && at > newestMs) {
      newestMs = at;
      newest = value;
    }
  }
  return newest;
}

/**
 * The clock time of a moment, in the reader's own locale and zone.
 *
 * A roster is read at a glance, and "14:30" is the whole of what a next slot
 * needs to say. A moment that is not today gains its date, because "09:30"
 * about tomorrow reads as a slot that has already been missed.
 */
export function slotLabel(
  value: string | undefined,
  locale: string,
  now: number = Date.now(),
): string {
  if (!value) return "";
  const at = new Date(value);
  if (!Number.isFinite(at.getTime())) return "";
  const start = localDayStart(now);
  const sameDay = at.getTime() >= start && at.getTime() < start + 86_400_000;
  return new Intl.DateTimeFormat(locale, {
    ...(sameDay ? {} : { month: "2-digit", day: "2-digit" }),
    hour: "2-digit",
    minute: "2-digit",
  }).format(at);
}

/**
 * The rows that moved inside a window.
 *
 * A station counts what happened in the window it is being read at, so the
 * three views on the pipeline page are the same code over a different cut
 * rather than three numbers that have to agree by hand.
 */
export function within<T extends { updatedAt?: string; createdAt?: string }>(
  items: T[],
  since?: number,
): T[] {
  if (since === undefined) return items;
  return items.filter((item) =>
    happenedSince(item.updatedAt ?? item.createdAt, since),
  );
}

/**
 * The real UTF-8 size of one row.
 *
 * `String.length` would be wrong by a third on Chinese, which is most of what
 * these tables hold, so the byte count is measured rather than estimated.
 */
export function utf8Bytes(value: unknown): number {
  try {
    return new TextEncoder().encode(JSON.stringify(value) ?? "").length;
  } catch {
    return 0;
  }
}

/** A byte count the way a person reads one. */
export function sizeLabel(bytes: number, t: TFunction): string {
  if (bytes <= 0) return t("desk.size.none");
  if (bytes < 1024) return t("desk.size.bytes", { count: bytes });
  if (bytes < 1024 * 1024) return t("desk.size.kb", { count: Math.round(bytes / 102.4) / 10 });
  return t("desk.size.mb", { count: Math.round(bytes / 104857.6) / 10 });
}

/* --------------------------------------------------------------- stations -- */

export const DESK_STATIONS = [
  "probe",
  "sift",
  "topics",
  "draft",
  "approve",
  "publish",
  "watch",
  "talk",
  "close",
  "deliver",
] as const;
export type DeskStationId = (typeof DESK_STATIONS)[number];

/**
 * How a station is lit.
 *
 * - `owner`   the ball is in the owner's court: nothing moves until they act.
 * - `stuck`   something is broken -- a site is signed out, an employee is
 *             stopped while work is waiting behind it.
 * - `machine` an employee has this, and it is either working or waiting a turn.
 * - `idle`    nothing has reached this station yet.
 */
export type DeskStationState = "owner" | "stuck" | "machine" | "idle";

/** One row of a station's detail table, already worded for the page. */
export type DeskStationRow = {
  id: string;
  cells: string[];
  link?: string;
};

/** A reason a station is not moving, as a key plus the only value it needs. */
export type DeskStationNote = { key: string; value?: string };

export type DeskStation = {
  id: DeskStationId;
  state: DeskStationState;
  /** The number on the card; `STATION_TEXT[id].out` says what it counts. */
  count: number;
  /** The rows behind that number, newest first. */
  rows: DeskStationRow[];
  /** How many rows were left out, so a capped table never reads as a total. */
  hidden: number;
  /** When anything last moved here. */
  at?: string;
  /** The employee this station belongs to, when one is hired for it. */
  employee?: StaffEmployee;
  note?: DeskStationNote;
  /** A channel whose sign-in clears the note, when the fix is a sign-in. */
  channel?: string;
};

/**
 * The words for each station, whole and in one table.
 *
 * Written as complete keys for the same reason the boards write theirs that
 * way: a key assembled at the call site is a key nothing checks, and three
 * pages have to agree on what these ten stations are called.
 */
export const STATION_TEXT: Record<
  DeskStationId,
  { name: string; who: string; out: string; columns: [string, string, string] }
> = {
  probe: {
    name: "desk.station.probe",
    who: "desk.who.scout",
    out: "desk.station.probeOut",
    columns: ["desk.col.lead", "desk.col.site", "desk.col.at"],
  },
  sift: {
    name: "desk.station.sift",
    who: "desk.who.leadBoss",
    out: "desk.station.siftOut",
    columns: ["desk.col.lead", "desk.col.score", "desk.col.stage"],
  },
  topics: {
    name: "desk.station.topics",
    who: "desk.who.analyst",
    out: "desk.station.topicsOut",
    columns: ["desk.col.signal", "desk.col.source", "desk.col.at"],
  },
  draft: {
    name: "desk.station.draft",
    who: "desk.who.writer",
    out: "desk.station.draftOut",
    columns: ["desk.col.title", "desk.col.status", "desk.col.variant"],
  },
  approve: {
    name: "desk.station.approve",
    who: "desk.who.you",
    out: "desk.station.approveOut",
    columns: ["desk.col.title", "desk.col.forWhom", "desk.col.waited"],
  },
  publish: {
    name: "desk.station.publish",
    who: "desk.who.pusher",
    out: "desk.station.publishOut",
    columns: ["desk.col.title", "desk.col.variant", "desk.col.ready"],
  },
  watch: {
    name: "desk.station.watch",
    who: "desk.who.resident",
    out: "desk.station.watchOut",
    columns: ["desk.col.title", "desk.col.where", "desk.col.readBack"],
  },
  talk: {
    name: "desk.station.talk",
    who: "desk.who.reception",
    out: "desk.station.talkOut",
    columns: ["desk.col.lead", "desk.col.site", "desk.col.quote"],
  },
  close: {
    name: "desk.station.close",
    who: "desk.who.closer",
    out: "desk.station.closeOut",
    columns: ["desk.col.lead", "desk.col.score", "desk.col.moved"],
  },
  deliver: {
    name: "desk.station.deliver",
    who: "desk.who.aftercare",
    out: "desk.station.deliverOut",
    columns: ["desk.col.lead", "desk.col.wonAt", "desk.col.note"],
  },
};

/**
 * Which employee works each station.
 *
 * Matched by board where the seat is a manager's -- the board is a durable
 * foreign key -- and by the name the roster ships otherwise, which is the same
 * handle the preset installer itself uses. A renamed employee simply drops the
 * "who is on it" details; the station still counts its rows.
 */
const STATION_SEAT: Record<DeskStationId, { board?: string; names?: string[] }> = {
  probe: { names: ["侦察员", "侦察探针"] },
  sift: { board: "leads" },
  topics: { names: ["选题员", "兴趣分析"] },
  draft: { names: ["笔杆子"] },
  approve: {},
  publish: { names: ["地推员"] },
  watch: {},
  talk: { names: ["接线员"] },
  close: { names: ["跟单员"] },
  deliver: { names: ["售后员"] },
};

export function stationEmployee(
  employees: StaffEmployee[],
  id: DeskStationId,
): StaffEmployee | undefined {
  const seat = STATION_SEAT[id];
  if (seat.board) {
    return employees.find(
      (employee) => employee.layer === "manager" && employee.board === seat.board,
    );
  }
  if (seat.names) {
    return employees.find((employee) => seat.names!.includes(employee.name));
  }
  return undefined;
}

/** How many rows of a station's detail the page draws before it counts them. */
export const STATION_ROW_CAP = 20;

function rowsOf<T>(
  items: T[],
  toRow: (item: T) => DeskStationRow,
): { rows: DeskStationRow[]; hidden: number } {
  return {
    rows: items.slice(0, STATION_ROW_CAP).map(toRow),
    hidden: Math.max(0, items.length - STATION_ROW_CAP),
  };
}

/** Newest first, so the top of a station's table is what just happened. */
function byNewest<T extends { updatedAt?: string; createdAt?: string }>(items: T[]): T[] {
  return [...items].sort(
    (a, b) =>
      Date.parse(b.updatedAt ?? b.createdAt ?? "") -
      Date.parse(a.updatedAt ?? a.createdAt ?? ""),
  );
}

/**
 * The ten stations, with their counts and the rows behind them.
 *
 * The cut between stations is a stage of the funnel, not a table: 巡站 counts
 * every card the scout filed, 分级 counts the ones the lead boss has not thrown
 * away, and so on down to 交付. That is deliberate -- one table feeding two
 * stations would mean the same row counted twice with no way to tell.
 */
export function buildStations(
  snapshot: DeskSnapshot,
  t: TFunction,
  now: number = Date.now(),
  since?: number,
): DeskStation[] {
  const { employees, sites } = snapshot;

  const leads = within(snapshot.leads, since);
  const contents = within(snapshot.contents, since);
  const memory = within(snapshot.memory, since);

  const openLeads = leads.filter((lead) => lead.stage !== "dropped" && lead.stage !== "lost");
  /**
   * Versions written, cut by the day they were created rather than the day
   * something last touched them. The watch bumps a published piece's reply
   * count every fifteen minutes; cutting this by `updated_at` would file last
   * week's article under this morning's writing.
   */
  const written = snapshot.contents.filter(
    (content) =>
      content.status !== "retired" && (since === undefined || happenedSince(content.createdAt, since)),
  );
  /**
   * The owner's own queue.
   *
   * A run files a draft at `draft` or `review`, and neither of them goes
   * anywhere without a yes. Both are therefore the same answer to "who is
   * holding this up", and reading only `review` would leave every draft a run
   * actually filed sitting here with nothing asking about it.
   */
  const awaiting = contents.filter(
    (content) => content.status === "draft" || content.status === "review",
  );
  const approved = contents.filter((content) => content.status === "approved");
  const handover = contents.filter((content) => content.status === "queued");
  const published = contents.filter((content) => content.status === "published");
  const signals = memory.filter((item) => item.kind === "signal" && item.status === "active");
  const replied = leads.filter((lead) => lead.stage === "replied");
  const closing = leads.filter(
    (lead) =>
      lead.stage === "qualified" || lead.stage === "contacted" || lead.stage === "talking",
  );
  const won = leads.filter((lead) => lead.stage === "won");

  /** The site a card came from, or that we do not know which one it was. */
  const leadSite = (lead: StaffLead) => lead.site || t("desk.unknownSite");
  const leadTitle = (lead: StaffLead) => lead.title || lead.quote || t("desk.untitled");
  const employeeName = (id?: string) =>
    employees.find((employee) => employee.id === id)?.name ?? t("desk.unknownAuthor");
  // The whole book, not the window: a draft written today answers a card that
  // was filed last week, and saying "who is this for" has to name it.
  const leadOf = (id?: string) => snapshot.leads.find((lead) => lead.id === id);

  const signedOut = sites.filter((site) => site.supported && !site.signedIn);
  const siteNote = (channel: string): DeskStationNote => ({
    key: "desk.note.siteSignedOut",
    value: t(staffSourceChannelKey(channel)),
  });

  const live = (id: DeskStationId, count: number, at?: string): DeskStationState => {
    if (count > 0) return "machine";
    return at ? "machine" : "idle";
  };

  const stations: DeskStation[] = [];
  const push = (station: DeskStation) => stations.push(station);

  {
    const items = byNewest(leads);
    const { rows, hidden } = rowsOf(items, (lead) => ({
      id: lead.id,
      cells: [leadTitle(lead), leadSite(lead), agoLabel(lead.createdAt, t)],
      link: lead.url || undefined,
    }));
    const stuck = signedOut.length > 0;
    push({
      id: "probe",
      state: stuck ? "stuck" : live("probe", leads.length, newestAt(items.map((i) => i.createdAt))),
      count: leads.length,
      rows,
      hidden,
      at: newestAt(items.map((i) => i.createdAt)),
      note: stuck ? siteNote(signedOut[0].channel) : undefined,
      channel: stuck ? signedOut[0].channel : undefined,
    });
  }

  {
    const items = byNewest(openLeads);
    const { rows, hidden } = rowsOf(items, (lead) => ({
      id: lead.id,
      cells: [
        leadTitle(lead),
        t("boards.leads.score", { score: lead.score }),
        t(leadStageKey(lead.stage)),
      ],
      link: lead.url || undefined,
    }));
    push({
      id: "sift",
      state: live("sift", openLeads.length, newestAt(items.map((i) => i.updatedAt))),
      count: openLeads.length,
      rows,
      hidden,
      at: newestAt(items.map((i) => i.updatedAt)),
    });
  }

  {
    const items = byNewest(signals);
    const { rows, hidden } = rowsOf(items, (item) => ({
      id: item.id,
      cells: [item.text, item.sourceRef || item.sourceKind, agoLabel(item.createdAt, t)],
      link: item.sourceKind === "url" ? item.sourceRef : undefined,
    }));
    push({
      id: "topics",
      state: live("topics", signals.length, newestAt(items.map((i) => i.createdAt))),
      count: signals.length,
      rows,
      hidden,
      at: newestAt(items.map((i) => i.createdAt)),
    });
  }

  {
    const items = byNewest(written);
    const { rows, hidden } = rowsOf(items, (content) => ({
      id: content.id,
      cells: [
        content.title,
        t(contentStatusKey(content.status)),
        content.variant || t("desk.none"),
      ],
    }));
    push({
      id: "draft",
      state: live("draft", items.length, newestAt(items.map((i) => i.createdAt))),
      count: items.length,
      rows,
      hidden,
      at: newestAt(items.map((i) => i.createdAt)),
    });
  }

  {
    const items = byNewest(awaiting);
    const { rows, hidden } = rowsOf(items, (content) => ({
      id: content.id,
      cells: [
        content.title,
        leadOf(content.leadId)
          ? leadTitle(leadOf(content.leadId)!)
          : t("desk.todo.openQuestion"),
        agoLabel(content.updatedAt, t),
      ],
    }));
    push({
      id: "approve",
      state: items.length > 0 ? "owner" : "idle",
      count: items.length,
      rows,
      hidden,
      at: newestAt(items.map((i) => i.updatedAt)),
    });
  }

  {
    // One station, two halves, and only one of them is the owner's: a piece
    // they have not pressed publish on is waiting for them, and a piece the
    // poster is holding is waiting for the poster. They are the same step of
    // the loop -- what is not out yet -- so they are drawn together and the
    // last cell says which half a row is in.
    const items = byNewest([...approved, ...handover]);
    const { rows, hidden } = rowsOf(items, (content) => ({
      id: content.id,
      cells: [
        content.title,
        content.variant || t("desk.none"),
        content.status === "queued" ? t("desk.ready.handedOver") : t("desk.ready.yes"),
      ],
    }));
    push({
      id: "publish",
      state: approved.length > 0 ? "owner" : items.length > 0 ? "machine" : "idle",
      count: items.length,
      rows,
      hidden,
      at: newestAt(items.map((i) => i.updatedAt)),
      employee: stationEmployee(employees, "publish"),
    });
  }

  {
    const items = byNewest(published);
    const { rows, hidden } = rowsOf(items, (content) => ({
      id: content.id,
      cells: [
        content.title,
        // A piece with no public page is not an address somebody forgot to
        // paste, so it says what it is rather than asking for a link that
        // cannot exist.
        content.publishedUrl ||
          (addresslessChannel(content.channel) ? t("desk.todo.noAddress") : t("desk.todo.noLink")),
        t("desk.readBack", { views: content.views, replies: content.replies }),
      ],
      link: content.publishedUrl || undefined,
    }));
    push({
      id: "watch",
      state: items.length > 0 ? "machine" : "idle",
      count: items.length,
      rows,
      hidden,
      at: newestAt(items.map((i) => i.updatedAt)),
      note: snapshot.watch?.lastError ? { key: "desk.note.watchError" } : undefined,
    });
  }

  {
    const items = byNewest(replied);
    const { rows, hidden } = rowsOf(items, (lead) => ({
      id: lead.id,
      cells: [leadTitle(lead), leadSite(lead), lead.quote || t("desk.none")],
      link: lead.url || undefined,
    }));
    const seat = stationEmployee(employees, "talk");
    const stopped = seat && seat.status !== "running" && items.length > 0;
    push({
      id: "talk",
      state: stopped ? "stuck" : live("talk", items.length, newestAt(items.map((i) => i.updatedAt))),
      count: items.length,
      rows,
      hidden,
      at: newestAt(items.map((i) => i.updatedAt)),
      employee: seat,
      note: stopped ? { key: "desk.note.seatStopped", value: seat!.name } : undefined,
    });
  }

  {
    const items = byNewest(closing);
    const { rows, hidden } = rowsOf(items, (lead) => ({
      id: lead.id,
      cells: [leadTitle(lead), t("boards.leads.score", { score: lead.score }), agoLabel(lead.updatedAt, t)],
      link: lead.url || undefined,
    }));
    const seat = stationEmployee(employees, "close");
    const stopped = seat && seat.status !== "running" && items.length > 0;
    push({
      id: "close",
      state: stopped ? "stuck" : live("close", items.length, newestAt(items.map((i) => i.updatedAt))),
      count: items.length,
      rows,
      hidden,
      at: newestAt(items.map((i) => i.updatedAt)),
      employee: seat,
      note: stopped ? { key: "desk.note.seatStopped", value: seat!.name } : undefined,
    });
  }

  {
    // A win is dated by `wonAt`, which editing the card afterwards does not
    // touch, so a "today" cut reads that field rather than `updatedAt`.
    const items =
      since === undefined
        ? byNewest(won)
        : won.filter((lead) => happenedSince(lead.wonAt ?? lead.updatedAt, since));
    const { rows, hidden } = rowsOf(items, (lead) => ({
      id: lead.id,
      cells: [leadTitle(lead), agoLabel(lead.wonAt ?? lead.updatedAt, t), lead.note || t("desk.none")],
      link: lead.url || undefined,
    }));
    push({
      id: "deliver",
      state: live("deliver", items.length, newestAt(items.map((i) => i.wonAt))),
      count: items.length,
      rows,
      hidden,
      at: newestAt(items.map((i) => i.wonAt)),
      employee: stationEmployee(employees, "deliver"),
    });
  }

  return stations;
}

/** Where the round has got to, in one line the header can draw as a bar. */
export type DeskProgress = {
  filled: number;
  total: number;
  /** The first station lit for the owner, if any. */
  waiting: DeskStationId | null;
  /** The first station that is broken, if any. */
  stuck: DeskStationId | null;
  /** The last station anything has actually reached. */
  furthest: DeskStationId | null;
};

export function loopProgress(stations: DeskStation[]): DeskProgress {
  let waiting: DeskStationId | null = null;
  let stuck: DeskStationId | null = null;
  let furthest: DeskStationId | null = null;
  let filled = 0;
  for (const station of stations) {
    if (station.count > 0) {
      filled += 1;
      furthest = station.id;
    }
    if (station.state === "owner" && !waiting) waiting = station.id;
    if (station.state === "stuck" && !stuck) stuck = station.id;
  }
  return { filled, total: stations.length, waiting, stuck, furthest };
}

/* ------------------------------------------------------------------ runs -- */

export type DeskStep = { title: string; result?: string; report?: string };

/** One run, as the timeline draws it: the steps it took and what each one said. */
export type DeskRunLine = {
  runId: string;
  employeeId: string;
  employee: string;
  status: StaffRun["status"];
  trigger: string;
  startedAt: string;
  endedAt?: string;
  currentStep: number;
  stepCount: number;
  steps: DeskStep[];
  errorCode?: string;
};

export function runToLine(run: StaffRun, employees: StaffEmployee[]): DeskRunLine {
  const results = run.results ?? {};
  const reports = run.reports ?? {};
  return {
    runId: run.id,
    employeeId: run.employeeId,
    employee:
      employees.find((employee) => employee.id === run.employeeId)?.name ?? run.employeeId.slice(0, 8),
    status: run.status,
    trigger: run.trigger,
    startedAt: run.startedAt,
    endedAt: run.endedAt,
    currentStep: run.currentStep,
    stepCount: run.stepCount,
    steps: (run.stepTitles ?? []).map((title, index) => ({
      title,
      result: results[String(index + 1)],
      report: reports[String(index + 1)],
    })),
    errorCode: run.errorCode,
  };
}

/** The newest run in the snapshot, which is the round the header narrates. */
export function latestRun(snapshot: DeskSnapshot, now: number = Date.now()): DeskRunLine | null {
  const runs = snapshot.runs.filter((run) => happenedSince(run.startedAt, now - 7 * 86_400_000));
  if (runs.length === 0) return null;
  const newest = [...runs].sort(
    (a, b) => Date.parse(b.startedAt) - Date.parse(a.startedAt),
  )[0];
  return runToLine(newest, snapshot.employees);
}

/**
 * How many rounds have started since the reader's day began.
 *
 * The desk shows the newest round in full; this is the one number that says
 * whether the machine has been busy all day or just woke up, which is what the
 * folded "machine is running" line leads with.
 */
export function roundsToday(snapshot: DeskSnapshot, now: number = Date.now()): number {
  const since = localDayStart(now);
  return snapshot.runs.filter((run) => happenedSince(run.startedAt, since)).length;
}

/** A run that died, with the name of whoever was carrying it. */
export type DeskDeadRun = { run: DeskRunLine; at: string };

export function deadRuns(snapshot: DeskSnapshot, now: number = Date.now()): DeskDeadRun[] {
  const since = now - 86_400_000;
  return snapshot.runs
    .filter(
      (run) =>
        (run.status === "failed" || run.status === "aborted") && happenedSince(run.startedAt, since),
    )
    .sort((a, b) => Date.parse(b.startedAt) - Date.parse(a.startedAt))
    .map((run) => ({ run: runToLine(run, snapshot.employees), at: run.startedAt }));
}

/* --------------------------------------------------------------- the day -- */

/** What the owner has to do before anything else can move. */
export type DeskTodoKind = "approve" | "publish" | "link" | "advice" | "deal";

export type DeskTodo = {
  id: string;
  kind: DeskTodoKind;
  title: string;
  detail: string;
  at?: string;
  /** Where the row's button goes. An advice row is answered where it stands. */
  page?: "contents" | "leads";
  /** The proposal this row decides, when deciding it *is* the row. */
  suggestionId?: string;
  /**
   * The card this row is about, when the row can be settled where it stands.
   *
   * Approving a draft used to mean a trip to another page, and the owner came
   * back asking what happened next. With the id in hand the desk approves it
   * on the spot -- and publishes it on the spot too, once approved, because
   * the address and the clipboard were the only things the other page had.
   */
  contentId?: string;
  /**
   * The post the work answers, when there is one.
   *
   * A draft carries the lead it was written for and a lead carries the page it
   * came off, so a row can offer its source without a second lookup.
   */
  link?: string;
};

/**
 * The words on a to-do row: its tag, and the button that answers it.
 *
 * One table rather than a key built from the kind, for the reason every other
 * label on this surface is one table: a key assembled at the call site is a key
 * nothing checks.
 */
export const TODO_TEXT: Record<DeskTodoKind, { tag: string; go: string }> = {
  approve: { tag: "desk.todo.approve", go: "desk.todo.approveGo" },
  publish: { tag: "desk.todo.publish", go: "desk.todo.publishGo" },
  link: { tag: "desk.todo.link", go: "desk.todo.linkGo" },
  advice: { tag: "desk.todo.advice", go: "desk.todo.adviceGo" },
  deal: { tag: "desk.todo.deal", go: "desk.todo.dealGo" },
};

/** The four numbers on the day strip, in funnel order. */
export const KPI_FIELDS = ["leads", "contacted", "replied", "won"] as const;
export type DeskKpiField = (typeof KPI_FIELDS)[number];

export const KPI_TEXT: Record<DeskKpiField, string> = {
  leads: "desk.kpi.leads",
  contacted: "desk.kpi.contacted",
  replied: "desk.kpi.replied",
  won: "desk.kpi.won",
};

/** The two ways a day gets stuck. */
export const BLOCKER_TEXT: Record<"site" | "run", string> = {
  site: "desk.blocker.site",
  run: "desk.blocker.run",
};

const FENCED_BLOCK = /```[a-zA-Z]*\s*([\s\S]*?)```/g;

/**
 * The card items a step's reply filed, from the last block that parses.
 *
 * The same read the host does when a run files its cards, kept here for one
 * reason: what a step filed is also the only record of where a card came from.
 */
function itemsInReply(reply: string): Array<Record<string, unknown>> {
  const blocks = [...String(reply).matchAll(FENCED_BLOCK)].map((match) => match[1]);
  for (let index = blocks.length - 1; index >= 0; index -= 1) {
    try {
      const parsed: unknown = JSON.parse(blocks[index].trim());
      const items = (parsed as { items?: unknown }).items;
      if (Array.isArray(items)) {
        return items.filter(
          (item): item is Record<string, unknown> => !!item && typeof item === "object",
        );
      }
    } catch {
      // A block that is not JSON is prose wearing a fence; try the one before.
    }
  }
  return [];
}

/**
 * Everything waiting on a person, in one list.
 *
 * The order is the order the work is blocked in: a draft nobody approved stops
 * the round two stations earlier than a lead nobody has called, so it is read
 * first. A row disappears when it is done -- not when it is dismissed -- which
 * is why nothing here is a notification.
 */
export function buildTodos(snapshot: DeskSnapshot, t: TFunction): DeskTodo[] {
  const { contents, leads, advice, employees, runs } = snapshot;
  const who = (id?: string) =>
    employees.find((employee) => employee.id === id)?.name ?? t("desk.unknownAuthor");
  const leadOf = (id?: string) => leads.find((lead) => lead.id === id);
  const leadTitle = (lead: StaffLead) => lead.title || lead.quote || t("desk.untitled");

  /**
   * What each run filed, read once and kept: a row's source is looked up by the
   * run that wrote it, and nine rows usually share one run.
   */
  const filedByRun = new Map<string, Array<Record<string, unknown>>>();
  const filedItems = (runId?: string) => {
    if (!runId) return [];
    const cached = filedByRun.get(runId);
    if (cached) return cached;
    const run = runs.find((entry) => entry.id === runId);
    const items = Object.values(run?.reports ?? {}).flatMap((reply) => itemsInReply(reply));
    filedByRun.set(runId, items);
    return items;
  };

  /**
   * The post a card answers, from the two places it can be.
   *
   * The lead the card was filed against comes first. Rows filed before the
   * board made that join have no lead, and their url survived only in the JSON
   * block the step wrote -- so it is read back from there. The title is what
   * makes that second lookup exact: the card's title is copied verbatim out of
   * the block, so a title that matches is the same card and not a coincidence.
   */
  const sourceOf = (content: StaffContent): string | undefined => {
    const lead = leadOf(content.leadId);
    if (lead?.url) return lead.url;
    const item = filedItems(content.runId).find((entry) => entry.title === content.title);
    const url = typeof item?.leadUrl === "string" ? item.leadUrl : item?.url;
    return typeof url === "string" && url ? url : undefined;
  };

  /**
   * The lead a card answers, whether the card named it or only the post.
   *
   * Rows filed before the join carry a url and no id, so the url is matched
   * back against the board: naming the poster is worth one lookup, and a row
   * that cannot be matched says so instead of naming a stranger.
   */
  const leadFor = (content: StaffContent): StaffLead | undefined => {
    const named = leadOf(content.leadId);
    if (named) return named;
    const url = sourceOf(content);
    return url ? leads.find((entry) => entry.url === url) : undefined;
  };

  const todos: DeskTodo[] = [];

  for (const content of byNewest(
    contents.filter((item) => item.status === "draft" || item.status === "review"),
  )) {
    todos.push({
      id: "approve:" + content.id,
      kind: "approve",
      title: content.title,
      detail: t("desk.todo.approveDetail", {
        who: who(content.employeeId),
        ago: agoLabel(content.updatedAt, t),
      }),
      at: content.updatedAt,
      page: "contents",
      contentId: content.id,
      link: sourceOf(content),
    });
  }

  for (const content of byNewest(contents.filter((item) => item.status === "approved"))) {
    // A row that can name the post it answers does; one that cannot says only
    // what is left to do, rather than repeating "an open question" eleven times
    // down the page and burying the rows that could say something.
    const answered = leadFor(content);
    todos.push({
      id: "publish:" + content.id,
      kind: "publish",
      title: content.title,
      detail: answered
        ? t("desk.todo.publishDetailFrom", { where: leadTitle(answered) })
        : t("desk.todo.publishDetail"),
      at: content.updatedAt,
      page: "contents",
      contentId: content.id,
      link: sourceOf(content),
    });
  }

  for (const content of byNewest(
    contents.filter((item) => needsAddress(item)),
  )) {
    todos.push({
      id: "link:" + content.id,
      kind: "link",
      title: content.title,
      detail: t("desk.todo.linkDetail"),
      at: content.updatedAt,
      page: "contents",
      link: sourceOf(content),
    });
  }

  for (const item of byNewest(advice.filter((entry) => entry.status === "open"))) {
    todos.push({
      id: "advice:" + item.id,
      kind: "advice",
      title: item.title,
      detail: t("desk.todo.adviceDetail", { count: item.evidence.length }),
      at: item.createdAt,
      suggestionId: item.id,
    });
  }

  for (const lead of byNewest(leads.filter((entry) => entry.stage === "talking"))) {
    todos.push({
      id: "deal:" + lead.id,
      kind: "deal",
      title: leadTitle(lead),
      detail: t("desk.todo.dealDetail", { score: lead.score }),
      at: lead.updatedAt,
      page: "leads",
      link: lead.url || undefined,
    });
  }

  return todos;
}

/** A thing that is broken, as opposed to a thing that is waiting. */
export type DeskBlocker = {
  id: string;
  kind: "site" | "run";
  title: string;
  detail: string;
  channel?: string;
  runId?: string;
};

export function buildBlockers(snapshot: DeskSnapshot, t: TFunction, now = Date.now()): DeskBlocker[] {
  const blockers: DeskBlocker[] = [];
  for (const site of snapshot.sites) {
    if (!site.supported || site.signedIn) continue;
    blockers.push({
      id: "site:" + site.channel,
      kind: "site",
      title: t("desk.blocker.siteTitle", { name: channelName(site.channel, t) }),
      detail: t("desk.blocker.siteDetail"),
      channel: site.channel,
    });
  }
  for (const dead of deadRuns(snapshot, now)) {
    blockers.push({
      id: "run:" + dead.run.runId,
      kind: "run",
      title: t("desk.blocker.runTitle", { employee: dead.run.employee }),
      detail: dead.run.errorCode || runStatusLabel(dead.run.status, t),
      runId: dead.run.runId,
    });
  }
  return blockers;
}

/** The display name of a collection channel, the same one the console uses. */
export function channelName(channel: string, t: TFunction): string {
  return t(staffSourceChannelKey(channel));
}

/** One employee as the "who is running" list draws it. */
export type DeskEmployeeRow = {
  employee: StaffEmployee;
  /** "第 3/5 步 · 判商机" while it is in flight. */
  live?: string;
  ranToday: number;
  quota: number;
  lastAt?: string;
  lastStatus?: StaffRun["status"];
};

export function buildEmployeeRows(
  snapshot: DeskSnapshot,
  t: TFunction,
  now: number = Date.now(),
): DeskEmployeeRow[] {
  const today = localDayStart(now);
  const inFlight = new Map(
    (snapshot.scheduler?.inFlight ?? []).map((entry) => [entry.employeeId, entry]),
  );
  return [...snapshot.employees]
    .sort((a, b) => (a.layer === b.layer ? a.name.localeCompare(b.name) : a.layer === "manager" ? -1 : 1))
    .map((employee) => {
      const runs = snapshot.runs.filter((run) => run.employeeId === employee.id);
      const flight = inFlight.get(employee.id);
      const carried = flight ? runs.find((run) => run.id === flight.runId) : undefined;
      const stepTitle = flight && carried ? carried.stepTitles?.[flight.step - 1] : undefined;
      const last = [...runs].sort((a, b) => Date.parse(b.startedAt) - Date.parse(a.startedAt))[0];
      const quota = employee.quota.maxRunsPerDay ?? 0;
      return {
        employee,
        live: flight
          ? t("desk.employee.live", {
              step: flight.step,
              total: flight.stepCount,
              what: stepTitle ?? t("desk.employee.liveUnknown"),
            })
          : undefined,
        ranToday: runs.filter((run) => happenedSince(run.startedAt, today)).length,
        quota,
        lastAt: last?.startedAt,
        lastStatus: last?.status,
      };
    });
}

/** When the two resident loops next run, for the strip under the roster. */
export type DeskNextLoop = {
  /** The earliest employee slot. */
  dispatch?: string;
  watch?: string;
};

export function nextLoop(snapshot: DeskSnapshot): DeskNextLoop {
  const slots = snapshot.employees
    .map((employee) => employee.nextRunAt)
    .filter((value): value is string => Boolean(value));
  const dispatch = slots.length > 0 ? slots.sort((a, b) => Date.parse(a) - Date.parse(b))[0] : undefined;
  return { dispatch, watch: snapshot.watch?.nextAt };
}

/** How many employees still have room in today's quota. */
export function runnableNow(
  snapshot: DeskSnapshot,
  t: TFunction,
  now: number = Date.now(),
): DeskEmployeeRow[] {
  return buildEmployeeRows(snapshot, t, now).filter(
    (row) =>
      row.employee.status === "running" &&
      !row.live &&
      (row.quota === 0 || row.ranToday < row.quota),
  );
}