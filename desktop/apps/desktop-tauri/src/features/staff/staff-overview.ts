/**
 * The numbers and the exception list behind the overview tab.
 *
 * Pure, so the console can be judged without a DOM: "is anything waiting on
 * me?" is the question an unattended system has to answer correctly, and it is
 * cheaper to pin down here than through a rendered page. Nothing in this module
 * knows about wording -- kinds come out, the panel turns them into sentences.
 */
import type {
  StaffEmployee,
  StaffPreset,
  StaffRun,
  StaffSchedulerStatus,
} from "@pi-desktop/shared";

/** Enough history to judge today without paging; the run log is the long view. */
export const OVERVIEW_RUN_LIMIT = 200;
/** How many exception rows the overview shows before it becomes a wall. */
export const OVERVIEW_ATTENTION_LIMIT = 8;

export type StaffAttention = {
  key: string;
  kind: "scheduler" | "employee" | "run";
  /** bad = broken, warn = waiting on a decision or a schedule. */
  tone: "warn" | "bad";
  employeeId?: string;
  employeeName?: string;
  /** Present for a run; a gate block is aborted with HUMAN_GATE. */
  errorCode?: string;
  gate?: boolean;
  sessionId?: string;
  /** The scheduler's own text, already fit to show. */
  message?: string;
};

export type StaffRunSummary = {
  todayTotal: number;
  todayFinished: number;
  todaySucceeded: number;
  /** Percentage of today's finished runs that succeeded, or null with no data. */
  successRate: number | null;
};

export function startOfDay(now: number): number {
  const midnight = new Date(now);
  midnight.setHours(0, 0, 0, 0);
  return midnight.getTime();
}

export function summarizeRuns(runs: StaffRun[], now: number = Date.now()): StaffRunSummary {
  const since = startOfDay(now);
  const today = runs.filter((run) => Date.parse(run.startedAt) >= since);
  const finished = today.filter((run) => run.status !== "running");
  const succeeded = finished.filter((run) => run.status === "succeeded");
  return {
    todayTotal: today.length,
    todayFinished: finished.length,
    todaySucceeded: succeeded.length,
    successRate:
      finished.length > 0
        ? Math.round((succeeded.length / finished.length) * 100)
        : null,
  };
}

/**
 * The newest run per employee. Compared by time rather than taken from the
 * first row, so a caller that hands over an unsorted list still gets the right
 * answer instead of a silently stale one.
 */
/**
 * Blueprints whose hired employee runs an older copy, keyed by employee name.
 *
 * Keyed by name because the roster is what the console has: a preset knows the
 * name it was hired under, and the card that offers the update is the card for
 * that employee.
 */
export function staleBlueprints(presets: StaffPreset[]): Map<string, string> {
  const stale = new Map<string, string>();
  for (const preset of presets) {
    if (preset.installed && preset.blueprintChanged) stale.set(preset.name, preset.id);
  }
  return stale;
}

export function latestRunByEmployee(runs: StaffRun[]): Map<string, StaffRun> {
  const latest = new Map<string, StaffRun>();
  for (const run of runs) {
    const current = latest.get(run.employeeId);
    if (!current || Date.parse(run.startedAt) > Date.parse(current.startedAt)) {
      latest.set(run.employeeId, run);
    }
  }
  return latest;
}

function nameOf(employees: StaffEmployee[], id: string): string {
  return employees.find((employee) => employee.id === id)?.name ?? id;
}

/**
 * Everything that waiting cannot fix: a scheduler fault, an employee that is
 * not scheduled to run, and every employee whose newest run failed or stopped
 * on a human gate. A failure the employee already recovered from is history and
 * belongs in the run log: an unattended system that keeps asking about problems
 * it solved trains its owner to ignore it.
 */
export function collectAttention(input: {
  employees: StaffEmployee[];
  runs: StaffRun[];
  status?: StaffSchedulerStatus | null;
}): StaffAttention[] {
  const { employees, runs, status } = input;
  const out: StaffAttention[] = [];
  if (status?.lastError) {
    out.push({ key: "scheduler", kind: "scheduler", tone: "bad", message: status.lastError });
  }
  for (const employee of employees) {
    if (employee.status === "running") continue;
    out.push({
      key: "employee-" + employee.id,
      kind: "employee",
      tone: "warn",
      employeeId: employee.id,
      employeeName: employee.name,
    });
  }
  // One row per employee, newest problem first, so a recovered employee is
  // silent and a repeated failure does not stack up.
  const blocked = [...latestRunByEmployee(runs).values()]
    .filter((run) =>
      run.status === "failed" ||
      (run.status === "aborted" && run.errorCode === "HUMAN_GATE"),
    )
    .sort((left, right) => Date.parse(right.startedAt) - Date.parse(left.startedAt));
  for (const run of blocked) {
    const gate = run.status === "aborted" && run.errorCode === "HUMAN_GATE";
    out.push({
      key: "run-" + run.id,
      kind: "run",
      tone: gate ? "warn" : "bad",
      employeeId: run.employeeId,
      employeeName: nameOf(employees, run.employeeId),
      ...(run.errorCode ? { errorCode: run.errorCode } : {}),
      ...(gate ? { gate: true } : {}),
      ...(run.sessionId ? { sessionId: run.sessionId } : {}),
    });
  }
  return out;
}
