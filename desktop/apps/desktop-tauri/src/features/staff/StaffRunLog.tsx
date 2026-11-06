/**
 * Run log: what each employee actually did, newest first.
 *
 * A run that is still "running" with no update is the failure mode this page
 * exists to make visible, so the in-flight rows carry their step counter.
 */
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { StaffEmployee, StaffRun } from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { useAppStore } from "../../stores/app-store";
import { Button, Field, Select } from "../../components/ui";
import {
  errorText,
  runStatusLabel,
  runStatusTone,
  triggerLabel,
  whenLabel,
} from "./staff-format";

export function StaffRunLog({
  employees,
  refreshToken,
}: {
  employees: StaffEmployee[];
  refreshToken: number;
}) {
  const { t, i18n } = useTranslation();
  const showToast = useAppStore((s) => s.showToast);
  const selectSession = useAppStore((s) => s.selectSession);
  const [runs, setRuns] = useState<StaffRun[]>([]);
  const [employeeId, setEmployeeId] = useState("");
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = async () => {
    setBusy(true);
    try {
      const res = await api.listStaffRuns(employeeId || undefined, 100);
      setRuns(res.runs ?? []);
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    void load();
  }, [employeeId, refreshToken]);

  const nameOf = (id: string) => employees.find((item) => item.id === id)?.name ?? id;

  return (
    <div className="staff-body single">
      <div className="staff-toolbar">
        <Field label={t("staff.runs.employee")}>
          <Select value={employeeId} onChange={(event) => setEmployeeId(event.target.value)}>
            <option value="">{t("staff.runs.allEmployees")}</option>
            {employees.map((employee) => (
              <option key={employee.id} value={employee.id}>
                {employee.name}
              </option>
            ))}
          </Select>
        </Field>
        <span className="staff-spacer" />
        <Button variant="ghost" disabled={busy} onClick={() => void load()}>
          {t("staff.runs.refresh")}
        </Button>
      </div>

      {runs.length === 0 ? (
        <div className="staff-hint">{t("staff.runs.empty")}</div>
      ) : (
        <table className="staff-runs">
          <thead>
            <tr>
              <th>{t("staff.runs.status")}</th>
              <th>{t("staff.runs.employee")}</th>
              <th>{t("staff.runs.trigger")}</th>
              <th>{t("staff.runs.steps")}</th>
              <th>{t("staff.runs.started")}</th>
              <th>{t("staff.runs.ended")}</th>
              <th>{t("staff.runs.error")}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {runs.map((run) => (
              <StaffRunRow
                key={run.id}
                run={run}
                nameOf={nameOf}
                expanded={expandedId === run.id}
                onToggle={() =>
                  setExpandedId((current) => (current === run.id ? null : run.id))
                }
                onOpenSession={() => void selectSession(run.sessionId as string)}
              />
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

/**
 * One run plus its expandable result sheet. The row is the only summary an
 * unattended run gets, so clicking it opens what each finished step reported.
 */
function StaffRunRow({
  run,
  nameOf,
  expanded,
  onToggle,
  onOpenSession,
}: {
  run: StaffRun;
  nameOf: (id: string) => string;
  expanded: boolean;
  onToggle: () => void;
  onOpenSession: () => void;
}) {
  const { t, i18n } = useTranslation();
  const results = run.results ?? {};
  return (
    <>
      <tr
        className={expanded ? "staff-run-row expanded" : "staff-run-row"}
        onClick={onToggle}
      >
        <td>
          <span className={"staff-dot " + runStatusTone(run.status)} aria-hidden />{" "}
          {runStatusLabel(run.status, t)}
        </td>
        <td>{nameOf(run.employeeId)}</td>
        <td>{triggerLabel(run.trigger, t)}</td>
        <td className="staff-mono">
          {t("staff.runs.stepOf", {
            current: run.currentStep,
            total: run.stepCount,
          })}
        </td>
        <td className="staff-mono">{whenLabel(run.startedAt, i18n.language)}</td>
        <td className="staff-mono">{whenLabel(run.endedAt, i18n.language)}</td>
        <td className="staff-mono">{run.errorCode ?? ""}</td>
        <td onClick={(event) => event.stopPropagation()}>
          {run.sessionId ? (
            <Button variant="ghost" size="sm" onClick={onOpenSession}>
              {t("staff.runs.openSession")}
            </Button>
          ) : null}
        </td>
      </tr>
      {expanded ? (
        <tr className="staff-run-results">
          <td colSpan={8}>
            {Array.from({ length: run.stepCount }, (_, index) => {
              const number = index + 1;
              const title = run.stepTitles?.[index] ?? `#${number}`;
              const result = results[String(number)];
              const report = (run.reports ?? {})[String(number)];
              return (
                <div key={number} className="staff-run-result">
                  <span className="staff-run-result-title">{title}</span>
                  <div className="staff-run-result-body">
                    <span className="staff-run-result-text">{result || "—"}</span>
                    {report ? <pre className="staff-run-result-report">{report}</pre> : null}
                  </div>
                </div>
              );
            })}
          </td>
        </tr>
      ) : null}
    </>
  );
}
