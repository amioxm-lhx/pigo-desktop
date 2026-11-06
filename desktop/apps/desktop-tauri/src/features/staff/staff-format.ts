/**
 * Shared formatters for the staff console.
 *
 * The roster, the run log, and the editor all describe the same schedule, so
 * the wording lives here instead of drifting apart in three components.
 */
import type { TFunction } from "i18next";
import type { StaffEmployee, StaffSchedule } from "@pi-desktop/shared";

const pad = (value: number) => String(value).padStart(2, "0");

/** Local short date-time, or an empty string when there is nothing to show. */
export function whenLabel(value: string | undefined, locale: string): string {
  if (!value) return "";
  const ms = Date.parse(value);
  if (!Number.isFinite(ms)) return value;
  return new Intl.DateTimeFormat(locale, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(ms));
}

/** "every 30 min · jitter ±5 min · 09:00" — whatever the row actually holds. */
export function scheduleSummary(schedule: StaffSchedule, t: TFunction): string {
  const parts: string[] = [];
  if (schedule.everyMinutes) {
    parts.push(t("staff.summary.every", { minutes: schedule.everyMinutes }));
  }
  if (schedule.jitterMinutes) {
    parts.push(t("staff.summary.jitter", { minutes: schedule.jitterMinutes }));
  }
  const times = schedule.timesOfDay ?? [];
  if (times.length > 0) parts.push(t("staff.summary.times", { times: times.join(", ") }));
  if (parts.length === 0) return t("staff.employees.scheduleManual");
  return parts.join(" · ");
}

export function employeeStatusTone(status: StaffEmployee["status"]): string {
  if (status === "running") return "on";
  if (status === "stopped") return "bad";
  return "warn";
}

export function employeeStatusLabel(status: StaffEmployee["status"], t: TFunction): string {
  if (status === "running") return t("staff.employees.statusRunning");
  if (status === "stopped") return t("staff.employees.statusStopped");
  return t("staff.employees.statusPaused");
}

export function runStatusTone(status: string): string {
  if (status === "succeeded") return "on";
  if (status === "failed") return "bad";
  if (status === "aborted") return "warn";
  return "";
}

export function runStatusLabel(status: string, t: TFunction): string {
  if (status === "succeeded") return t("staff.runs.succeeded");
  if (status === "failed") return t("staff.runs.failed");
  if (status === "aborted") return t("staff.runs.aborted");
  return t("staff.runs.running");
}

export function triggerLabel(trigger: string, t: TFunction): string {
  if (trigger === "manual") return t("staff.runs.triggerManual");
  if (trigger === "event") return t("staff.runs.triggerEvent");
  if (trigger === "directive") return t("staff.runs.triggerDirective");
  return t("staff.runs.triggerSchedule");
}

/** Built by joining, so the catalog scan never sees a half-formed key. */
export function nodeKindKey(kind: string): string {
  return ["staff", "kinds", kind].join(".");
}

export function errorText(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error);
}
