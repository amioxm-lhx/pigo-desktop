/**
 * Shared wording and shape for the boards.
 *
 * The four boards read the same rows and answer the same question -- what does
 * the owner decide today -- so the labels live here rather than in each page.
 * A column named one thing on the lead board and another on the report board
 * would make the same funnel read as two.
 */
import type {
  StaffDirective,
  StaffEmployee,
  StaffLeadStage,
  StaffContentStatus,
  StaffSuggestionBoard,
} from "@pi-desktop/shared";

/** The columns of the business board, in the order they are drawn. */
export const LEAD_COLUMNS: StaffLeadStage[] = [
  "new",
  "qualified",
  "contacted",
  "replied",
  "talking",
  "won",
  "lost",
  "dropped",
];

/** The columns a card can still be worked in; the rest are history. */
export const LEAD_OPEN_COLUMNS: StaffLeadStage[] = [
  "new",
  "qualified",
  "contacted",
  "replied",
  "talking",
];

export const CONTENT_STATUSES: StaffContentStatus[] = [
  "draft",
  "review",
  "approved",
  "queued",
  "published",
  "retired",
];

/** The statuses a header shows a count for. */
export const CONTENT_COLUMNS: StaffContentStatus[] = [
  "draft",
  "review",
  "approved",
  "queued",
  "published",
];

export const BOARD_ORDER: StaffSuggestionBoard[] = ["leads", "contents", "report", "arsenal"];

/** The i18n key for a board's own name, as the sidebar already calls it. */
export function boardNameKey(board: StaffSuggestionBoard): string {
  return "boards." + board + ".nav";
}

/**
 * The employee that manages one board, if anyone does.
 *
 * A board and its manager are two rows that have to agree: the board is where
 * the manager's evidence lives, and the manager is the only employee whose
 * workflow may dispatch from it. Matching them here keeps the page from having
 * to know which name was shipped for which board.
 */
export function boardManagerOf(
  employees: StaffEmployee[],
  board: StaffSuggestionBoard,
): StaffEmployee | undefined {
  return employees.find((employee) => employee.layer === "manager" && employee.board === board);
}

/** The orders a manager has not finished handing out yet. */
export function liveOrders(orders: StaffDirective[]): StaffDirective[] {
  return orders.filter((order) => order.status === "queued" || order.status === "running");
}

/** The orders that are over, one way or the other. */
export function settledOrders(orders: StaffDirective[]): StaffDirective[] {
  return orders.filter((order) => order.status !== "queued" && order.status !== "running");
}

/** The i18n key for a lead's column. */
export function leadStageKey(stage: string): string {
  return "boards.stage." + stage;
}

/** The i18n key for a draft's status. */
export function contentStatusKey(status: string): string {
  return "boards.contentStatus." + status;
}

/**
 * The i18n key for a proposal's answer, and for the work it became.
 *
 * Built here rather than at the call site: the renderer's key check only sees
 * dotted literals, so a key assembled inside JSX is a key nothing verifies.
 */
export function decidedKey(status: string): string {
  return "boards.advice.decided." + status;
}

export function directiveStatusKey(status: string): string {
  return "boards.directiveStatus." + status;
}

/**
 * The i18n key for a counter the owner reads off a site's own dashboard.
 *
 * Built here for the same reason the status keys are: a key assembled at the
 * call site is a key the renderer check cannot see.
 */
export function metricFieldKey(field: string): string {
  return "boards.report.field." + field;
}

/** Days between two instants, rounded down, as a whole number of hours. */
export function hoursSince(value: string | undefined, now: number = Date.now()): number | null {
  if (!value) return null;
  const at = Date.parse(value);
  if (Number.isNaN(at)) return null;
  return Math.max(0, Math.floor((now - at) / 3_600_000));
}

/** A short label for how long ago something happened. */
export function agoLabel(
  value: string | undefined,
  t: (key: string, options?: Record<string, unknown>) => string,
): string {
  const hours = hoursSince(value);
  if (hours === null) return "";
  if (hours < 1) return t("boards.ago.minutes");
  if (hours < 24) return t("boards.ago.hours", { count: hours });
  return t("boards.ago.days", { count: Math.floor(hours / 24) });
}

/** A whole number with thousands separators, or a dash when it is missing. */
export function number(value: number | undefined | null): string {
  if (value === undefined || value === null) return "-";
  return value.toLocaleString();
}

/** How a day compares with the one before it, in words rather than a colour. */
export function delta(
  today: number,
  yesterday: number,
  t: (key: string, options?: Record<string, unknown>) => string,
): string {
  const change = today - yesterday;
  if (change === 0) return t("boards.delta.flat");
  return change > 0
    ? t("boards.delta.up", { count: change })
    : t("boards.delta.down", { count: Math.abs(change) });
}

/**
 * How tall a draft's editor opens.
 *
 * A draft is one paragraph with no newlines, so counting lines would give every
 * one of them the same three rows and bury the rest behind a textarea scrollbar.
 * The estimate spans the paragraph and caps it: enough to read the draft and
 * judge it, not so much that one long reply pushes the next card off the page.
 */
export function draftRows(body: string): number {
  const wrapped = Math.ceil(displayWidth(body) / DRAFT_LINE_WIDTH);
  const written = body.split("\n").length;
  return Math.min(14, Math.max(3, Math.max(wrapped, written)));
}

/** Roughly how many half-width characters fit on one line of a draft editor. */
const DRAFT_LINE_WIDTH = 58;

/**
 * How much room the text needs, counting a CJK character as two.
 *
 * The drafts are written in Chinese, where a fixed characters-per-line estimate
 * is wrong by half: a card sized for 44 characters fits roughly 22 of these, so
 * every draft opened one line short of its own text.
 */
function displayWidth(text: string): number {
  let width = 0;
  for (const char of text) {
    width += (char.codePointAt(0) ?? 0) > 0x2e80 ? 2 : 1;
  }
  return width;
}

/** The tallest bar in a trend series, floored at one so a flat week still draws. */
export function trendPeak(values: number[]): number {
  return Math.max(1, ...values);
}
