/**
 * The data page: what each station's output is, how much of it there is, and
 * who would miss it if it were deleted.
 *
 * The ledger answers a question about the board, not about the disk. This
 * team's own tables come to about a megabyte; the conversation transcript
 * beside them is more than ten times that. So a stage is worth clearing when
 * it drowns the board it is read on -- and the honest thing the page can say
 * about the file size is which part of it is not the team at all.
 *
 * Sizes and "what a cleanup would take" come from the host, which measures both
 * with the same predicate the delete uses. Nothing on this page is estimated:
 * a number the owner confirms against has to be the number of rows that
 * actually go.
 */
import type { StaffDataStats, StaffLedgerStage } from "@pi-desktop/shared";
import { newestAt, type DeskSnapshot } from "./desk-model";

export const LEDGER_STAGES = [
  "scan",
  "leads",
  "contents",
  "publish",
  "metrics",
  "dispatch",
  "runs",
  "memory",
] as const;
export type DeskLedgerStageId = (typeof LEDGER_STAGES)[number];

/** The words for each stage, whole and in one table. */
export const LEDGER_TEXT: Record<
  DeskLedgerStageId,
  { name: string; produce: string; usedBy: string; rule: string }
> = {
  scan: {
    name: "desk.ledger.scan.name",
    produce: "desk.ledger.scan.produce",
    usedBy: "desk.ledger.scan.usedBy",
    rule: "desk.ledger.scan.rule",
  },
  leads: {
    name: "desk.ledger.leads.name",
    produce: "desk.ledger.leads.produce",
    usedBy: "desk.ledger.leads.usedBy",
    rule: "desk.ledger.leads.rule",
  },
  contents: {
    name: "desk.ledger.contents.name",
    produce: "desk.ledger.contents.produce",
    usedBy: "desk.ledger.contents.usedBy",
    rule: "desk.ledger.contents.rule",
  },
  publish: {
    name: "desk.ledger.publish.name",
    produce: "desk.ledger.publish.produce",
    usedBy: "desk.ledger.publish.usedBy",
    rule: "desk.ledger.publish.rule",
  },
  metrics: {
    name: "desk.ledger.metrics.name",
    produce: "desk.ledger.metrics.produce",
    usedBy: "desk.ledger.metrics.usedBy",
    rule: "desk.ledger.metrics.rule",
  },
  dispatch: {
    name: "desk.ledger.dispatch.name",
    produce: "desk.ledger.dispatch.produce",
    usedBy: "desk.ledger.dispatch.usedBy",
    rule: "desk.ledger.dispatch.rule",
  },
  runs: {
    name: "desk.ledger.runs.name",
    produce: "desk.ledger.runs.produce",
    usedBy: "desk.ledger.runs.usedBy",
    rule: "desk.ledger.runs.rule",
  },
  memory: {
    name: "desk.ledger.memory.name",
    produce: "desk.ledger.memory.produce",
    usedBy: "desk.ledger.memory.usedBy",
    rule: "desk.ledger.memory.rule",
  },
};

/**
 * The cleanups the host knows how to run.
 *
 * The ids carry hyphens because they are wire values; their labels are whole
 * keys here, because an i18n key built from an id with a hyphen in it is a key
 * the renderer's catalog check cannot see.
 */
export const PRUNE_RULES = [
  "contents-retired",
  "leads-dead",
  "dispatch-answered",
  "metrics-aged",
  "runs-reports",
  "memory-expired",
] as const;
export type DeskPruneRule = (typeof PRUNE_RULES)[number];

export const PRUNE_TEXT: Record<DeskPruneRule, { label: string; hint: string }> = {
  "contents-retired": {
    label: "desk.prune.contentsRetired",
    hint: "desk.prune.contentsRetiredHint",
  },
  "leads-dead": { label: "desk.prune.leadsDead", hint: "desk.prune.leadsDeadHint" },
  "dispatch-answered": {
    label: "desk.prune.dispatchAnswered",
    hint: "desk.prune.dispatchAnsweredHint",
  },
  "metrics-aged": { label: "desk.prune.metricsAged", hint: "desk.prune.metricsAgedHint" },
  "runs-reports": { label: "desk.prune.runsReports", hint: "desk.prune.runsReportsHint" },
  "memory-expired": { label: "desk.prune.memoryExpired", hint: "desk.prune.memoryExpiredHint" },
};

/** One drawn line of the ledger: the host's numbers, plus the words for them. */
export type DeskLedgerRow = {
  id: DeskLedgerStageId;
  rows: number;
  bytes: number;
  /** Written once, still on disk, nothing using it any more. */
  parkedRows: number;
  parkedBytes: number;
  cleanable: number;
  cleanableBytes: number;
  /** When this stage last gained anything, as far as the loaded rows show. */
  at?: string;
  /** True when the stage keeps nothing at all, and the row is a statement. */
  ephemeral: boolean;
};

/**
 * Join the host's measurements to the words, in loop order.
 *
 * A stage the host did not report is left out rather than drawn as a zero: a
 * zero would read as "nothing here", and "we could not read it" is a different
 * sentence that the page has to be able to say.
 */
export function ledgerRows(
  stats: StaffDataStats | null,
  snapshot: DeskSnapshot,
): DeskLedgerRow[] {
  if (!stats) return [];
  const last = {
    leads: newestAt(snapshot.leads.map((row) => row.updatedAt ?? row.createdAt)),
    contents: newestAt(snapshot.contents.map((row) => row.createdAt)),
    publish: newestAt(snapshot.contents.map((row) => row.publishedAt)),
    metrics: newestAt(snapshot.metrics.map((row) => row.updatedAt ?? row.createdAt)),
    dispatch: newestAt([
      ...snapshot.advice.map((row) => row.createdAt),
      ...snapshot.directives.map((row) => row.createdAt),
    ]),
    runs: newestAt(snapshot.runs.map((row) => row.startedAt)),
    memory: newestAt(snapshot.memory.map((row) => row.createdAt)),
  } as Record<string, string | undefined>;
  const order = new Map(LEDGER_STAGES.map((id, index) => [id, index]));
  return [...stats.stages]
    .filter((stage): stage is StaffLedgerStage => order.has(stage.id as DeskLedgerStageId))
    .sort((a, b) => order.get(a.id as DeskLedgerStageId)! - order.get(b.id as DeskLedgerStageId)!)
    .map((stage) => ({
      id: stage.id as DeskLedgerStageId,
      rows: stage.rows,
      bytes: stage.bytes,
      parkedRows: stage.parkedRows,
      parkedBytes: stage.parkedBytes,
      cleanable: stage.cleanable,
      cleanableBytes: stage.cleanableBytes,
      at: last[stage.id],
      ephemeral: stage.id === "scan",
    }));
}

/**
 * How much of the database file this team owns, archived rows included.
 *
 * A retired draft and a forgotten fact are bytes on disk like any other, so
 * they count: a total that skipped them would drop every time something was
 * retired, while the file on disk had not moved at all.
 */
export function teamFootprint(rows: DeskLedgerRow[]): number {
  return rows.reduce((total, row) => total + row.bytes + row.parkedBytes, 0);
}

/** What a stage's cleanup would take, for the rule of the same name. */
export function ruleOf(stage: DeskLedgerStageId): DeskPruneRule | null {
  if (stage === "contents") return "contents-retired";
  if (stage === "leads") return "leads-dead";
  if (stage === "dispatch") return "dispatch-answered";
  if (stage === "metrics") return "metrics-aged";
  if (stage === "runs") return "runs-reports";
  if (stage === "memory") return "memory-expired";
  return null;
}

/** What points at what, so a cleanup can see which row it is about to strand. */
export type DeskReference = {
  id: string;
  from: DeskLedgerStageId;
  to: DeskLedgerStageId;
  count: number;
};

/**
 * The links that actually exist between these tables.
 *
 * Written out rather than derived from a schema dump: these are the joins the
 * code performs, and a join nothing performs is not a reference.
 */
export function buildReferences(snapshot: DeskSnapshot): DeskReference[] {
  const { contents, leads, metrics } = snapshot;
  const withLead = contents.filter((content) => Boolean(content.leadId));
  return [
    { id: "contents-leads", from: "contents", to: "leads", count: withLead.length },
    {
      id: "publish-contents",
      from: "publish",
      to: "contents",
      count: withLead.filter((content) => content.status === "published").length,
    },
    {
      id: "metrics-topics",
      from: "metrics",
      to: "contents",
      count: metrics.filter((metric) =>
        contents.some((content) => content.topic === metric.label),
      ).length,
    },
    {
      id: "leads-dispatch",
      from: "leads",
      to: "dispatch",
      count: leads.filter((lead) => lead.stage === "won").length,
    },
  ];
}

/**
 * Where the chain breaks.
 *
 * A page that only lists sizes answers the wrong question. The reason a board
 * is full of drafts and empty of customers is a missing link, and the links are
 * these counts -- so the first broken one is the only suggestion the page
 * makes, and it is worth more than every byte it is standing next to.
 */
export function brokenLink(
  snapshot: DeskSnapshot,
): { key: string; values: Record<string, number> } | null {
  const staged = snapshot.contents.filter((content) => content.status !== "retired");
  // "Decided" is two statuses now: a piece waiting for the owner's publish
  // click, and a piece the poster is holding. Neither is out in the world, so
  // both are the link this page is looking for when it says nothing has gone
  // out yet.
  const approved = staged.filter(
    (content) => content.status === "approved" || content.status === "queued",
  );
  const published = staged.filter((content) => content.status === "published");
  const answered = snapshot.leads.filter(
    (lead) => lead.stage === "replied" || lead.stage === "talking",
  );
  const won = snapshot.leads.filter((lead) => lead.stage === "won");

  if (staged.length > 0 && approved.length + published.length === 0) {
    return { key: "desk.data.breakNoApprove", values: { drafts: staged.length } };
  }
  if (approved.length > 0) {
    return { key: "desk.data.breakNoPublish", values: { approved: approved.length } };
  }
  if (published.length > 0 && answered.length === 0) {
    return { key: "desk.data.breakNoReply", values: { published: published.length } };
  }
  if (answered.length > 0 && won.length === 0) {
    return { key: "desk.data.breakNoClose", values: { talking: answered.length } };
  }
  return null;
}