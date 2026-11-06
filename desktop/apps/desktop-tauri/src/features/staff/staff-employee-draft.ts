/**
 * Editor draft state for one employee.
 *
 * Kept apart from the form so the wire shape the host expects lives in one
 * readable place: what is null, what is a number, and which fields a schedule
 * may not combine.
 */
import type { StaffEmployee } from "@pi-desktop/shared";

export type Draft = {
  name: string;
  role: string;
  status: StaffEmployee["status"];
  credentialProfileId: string;
  providerId: string;
  modelId: string;
  thinkingLevel: string;
  workflowId: string;
  workflowVersion: string;
  everyMinutes: string;
  jitterMinutes: string;
  timesOfDay: string;
  utcOffsetMinutes: string;
  maxRunsPerDay: string;
};

export const EMPTY_DRAFT: Draft = {
  name: "",
  role: "",
  status: "paused",
  credentialProfileId: "",
  providerId: "",
  modelId: "",
  thinkingLevel: "",
  workflowId: "",
  workflowVersion: "",
  everyMinutes: "",
  jitterMinutes: "",
  timesOfDay: "",
  utcOffsetMinutes: "",
  maxRunsPerDay: "",
};

/** "09:00, 20:00" -> ["09:00", "20:00"]; blanks and stray commas are dropped. */
function parseTimes(value: string): string[] {
  return value
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean);
}

export function draftOf(employee: StaffEmployee): Draft {
  const schedule = employee.schedule ?? {};
  return {
    name: employee.name,
    role: employee.role,
    status: employee.status,
    credentialProfileId: employee.credentialProfileId ?? "",
    providerId: employee.providerId ?? "",
    modelId: employee.modelId ?? "",
    thinkingLevel: employee.thinkingLevel ?? "",
    workflowId: employee.workflowId ?? "",
    workflowVersion: employee.workflowVersion == null ? "" : String(employee.workflowVersion),
    everyMinutes: schedule.everyMinutes == null ? "" : String(schedule.everyMinutes),
    jitterMinutes: schedule.jitterMinutes == null ? "" : String(schedule.jitterMinutes),
    timesOfDay: (schedule.timesOfDay ?? []).join(", "),
    utcOffsetMinutes:
      schedule.utcOffsetMinutes == null ? "" : String(schedule.utcOffsetMinutes),
    maxRunsPerDay:
      employee.quota?.maxRunsPerDay == null ? "" : String(employee.quota.maxRunsPerDay),
  };
}

export function employeePayload(draft: Draft): Record<string, unknown> {
  const times = parseTimes(draft.timesOfDay);
  const schedule: Record<string, unknown> = {};
  if (times.length > 0) schedule.timesOfDay = times;
  else if (draft.everyMinutes.trim()) schedule.everyMinutes = Number(draft.everyMinutes);
  if (draft.jitterMinutes.trim()) schedule.jitterMinutes = Number(draft.jitterMinutes);
  if (draft.utcOffsetMinutes.trim()) schedule.utcOffsetMinutes = Number(draft.utcOffsetMinutes);
  const quota: Record<string, unknown> = {};
  if (draft.maxRunsPerDay.trim()) quota.maxRunsPerDay = Number(draft.maxRunsPerDay);

  return {
    name: draft.name.trim(),
    role: draft.role.trim(),
    status: draft.status,
    credentialProfileId: draft.credentialProfileId || null,
    providerId: draft.providerId || null,
    modelId: draft.modelId.trim() || null,
    thinkingLevel: draft.thinkingLevel || null,
    workflowId: draft.workflowId || null,
    workflowVersion: draft.workflowVersion ? Number(draft.workflowVersion) : null,
    schedule,
    quota,
  };
}

