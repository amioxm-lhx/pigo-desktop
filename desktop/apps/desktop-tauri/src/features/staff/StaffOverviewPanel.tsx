/**
 * The overview tab: what the company did today, and what is waiting on you.
 *
 * An unattended system is only unattended if exceptions surface by themselves,
 * so this view answers one question first -- does anything need the owner? --
 * and only then shows the roster. Everything is derived from the runs the
 * console already reads plus the scheduler status it already polls, so the
 * exceptions become visible without a new host call.
 *
 * The one thing it does ask the host for is the blueprints. A seeded employee
 * whose graph no longer matches the copy this build ships is holding a bug the
 * owner cannot see and cannot fix by hand, so the card carrying it offers the
 * newer copy -- as a new version, never as a rewrite.
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
  StaffEmployee,
  StaffPreset,
  StaffRun,
  StaffSchedulerStatus,
  StaffSuggestionBoard,
} from "@pi-desktop/shared";
import { boardNameKey } from "../boards/board-format";
import { api } from "../../lib/api";
import { useAppStore } from "../../stores/app-store";
import { Badge, Button } from "../../components/ui";
import {
  collectAttention,
  latestRunByEmployee,
  OVERVIEW_ATTENTION_LIMIT,
  OVERVIEW_RUN_LIMIT,
  staleBlueprints,
  summarizeRuns,
  type StaffAttention,
} from "./staff-overview";
import {
  employeeStatusLabel,
  employeeStatusTone,
  errorText,
  runStatusLabel,
  runStatusTone,
  whenLabel,
} from "./staff-format";

export function StaffOverviewPanel({
  employees,
  status,
  refreshToken,
}: {
  employees: StaffEmployee[];
  status: StaffSchedulerStatus | null;
  refreshToken: number;
}) {
  const { t, i18n } = useTranslation();
  const showToast = useAppStore((s) => s.showToast);
  const selectSession = useAppStore((s) => s.selectSession);
  const [runs, setRuns] = useState<StaffRun[]>([]);
  const [presets, setPresets] = useState<StaffPreset[]>([]);
  const [adopting, setAdopting] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [runsRes, presetsRes] = await Promise.all([
        api.listStaffRuns(undefined, OVERVIEW_RUN_LIMIT),
        api.listStaffPresets(),
      ]);
      setRuns(runsRes.runs ?? []);
      setPresets(presetsRes.presets ?? []);
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    }
  }, [showToast]);

  useEffect(() => {
    void load();
  }, [load, refreshToken]);

  /**
   * Hand the named employees this build's copy of their blueprint.
   *
   * The reply names who moved, so the confirmation counts employees rather than
   * ids, and the roster is reloaded from the host afterwards instead of being
   * patched here: the version numbers on the cards are the host's to report.
   */
  const adopt = useCallback(
    async (ids: string[], key: string) => {
      setAdopting(key);
      try {
        const result = await api.adoptStaffBlueprint(ids);
        showToast(t("staff.overview.blueprintAdopted", { count: (result.updated ?? []).length }));
        await load();
      } catch (error) {
        showToast(errorText(error), { variant: "error" });
      } finally {
        setAdopting(null);
      }
    },
    [load, showToast, t],
  );

  /**
   * One employee's card, drawn the same way in both groups.
   *
   * A manager's card also says which board it owns: the rank is what makes the
   * roster readable, and the board is the only thing that tells two managers of
   * the same rank apart.
   */
  const renderEmployee = (employee: StaffEmployee) => {
    const last = latestByEmployee.get(employee.id);
    return (
      <div className="staff-memory-item" key={employee.id}>
              <div className="staff-memory-head">
                <span className={"staff-dot " + employeeStatusTone(employee.status)} aria-hidden />
                <b>{employee.name}</b>
                {employee.layer === "manager" && employee.board ? (
                  <span className="staff-tag on">
                    {t(boardNameKey(employee.board as StaffSuggestionBoard))}
                  </span>
                ) : null}
                <Badge tone="neutral">{employeeStatusLabel(employee.status, t)}</Badge>
              </div>
              <div className="staff-memory-meta">
                <span>
                  {t("staff.overview.nextRun")}{" "}
                  {whenLabel(employee.nextRunAt, i18n.language) || t("staff.never")}
                </span>
                <span>
                  {t("staff.overview.workflowVersion")}{" "}
                  {employee.workflowVersion ? "v" + employee.workflowVersion : "-"}
                </span>
                <span>{employee.modelId || t("staff.employees.modelInherit")}</span>
              </div>
              {stale.has(employee.name) ? (
                <div className="staff-memory-meta">
                  <span className="staff-tag warn">{t("staff.overview.blueprintStale")}</span>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={adopting !== null}
                    onClick={() => void adopt([stale.get(employee.name) as string], employee.id)}
                  >
                    {t("staff.overview.blueprintAdopt")}
                  </Button>
                </div>
              ) : null}
              {last ? (
                <div className="staff-memory-meta">
                  <span className={"staff-dot " + runStatusTone(last.status)} aria-hidden />
                  <span>
                    {t("staff.overview.lastRun")}{" "}
                    {runStatusLabel(last.status, t)}
                    {" · "}
                    {whenLabel(last.startedAt, i18n.language)}
                  </span>
                  {last.errorCode ? (
                    <span className="staff-tag warn">{last.errorCode}</span>
                  ) : null}
                </div>
              ) : (
                <div className="staff-hint">{t("staff.overview.noRuns")}</div>
              )}
            </div>
    );
  };

  const summary = summarizeRuns(runs);
  const latestByEmployee = latestRunByEmployee(runs);
  const managers = employees.filter((employee) => employee.layer === "manager");
  const workers = employees.filter((employee) => employee.layer !== "manager");
  const attention = collectAttention({ employees, runs, status });
  const stale = staleBlueprints(presets);
  const staleIds = [...new Set(stale.values())];

  /** One exception, in words: an employee to look at, and why. */
  const describe = (item: StaffAttention): { title: string; detail: string } => {
    if (item.kind === "scheduler") {
      return {
        title: t("staff.overview.schedulerError"),
        detail: item.message ?? "",
      };
    }
    if (item.kind === "employee") {
      return {
        title: item.employeeName ?? "",
        detail: t("staff.overview.employeePaused"),
      };
    }
    return {
      title: item.employeeName ?? "",
      detail:
        (item.gate
          ? t("staff.overview.gateWaiting")
          : t("staff.overview.runFailed")) +
        (item.errorCode ? " · " + item.errorCode : ""),
    };
  };

  return (
    <div className="staff-body single">
      <div className="staff-strip">
        <div className="staff-stat">
          <b>{employees.length}</b>
          <span>{t("staff.overview.employees")}</span>
        </div>
        <div className="staff-stat">
          <b>{summary.todayTotal}</b>
          <span>{t("staff.overview.todayRuns")}</span>
        </div>
        <div className="staff-stat">
          <b>{summary.successRate === null ? "-" : summary.successRate + "%"}</b>
          <span>{t("staff.overview.successRate")}</span>
        </div>
        <div className="staff-stat">
          <b>{attention.length}</b>
          <span>{t("staff.overview.waiting")}</span>
        </div>
      </div>

      <div className="staff-section-title">{t("staff.overview.attention")}</div>
      {attention.length === 0 ? (
        <div className="staff-ok">{t("staff.overview.allClear")}</div>
      ) : (
        <div className="staff-list">
          {attention.slice(0, OVERVIEW_ATTENTION_LIMIT).map((item) => {
            const line = describe(item);
            return (
              <div className="staff-list-item" key={item.key}>
                <span className={"staff-dot " + item.tone} aria-hidden />
                <span className="staff-list-main">
                  <b>{line.title}</b>
                  <span className="staff-list-sub">{line.detail}</span>
                </span>
                {item.sessionId ? (
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => void selectSession(item.sessionId as string)}
                  >
                    {t("staff.overview.openSession")}
                  </Button>
                ) : null}
              </div>
            );
          })}
        </div>
      )}

      {stale.size > 0 ? (
        <>
          <div className="staff-section-title">{t("staff.overview.blueprintTitle")}</div>
          <div className="staff-list">
            <div className="staff-list-item">
              <span className="staff-dot warn" aria-hidden />
              <span className="staff-list-main">
                <b>{`${stale.size}`}</b>
                <span className="staff-list-sub">{t("staff.overview.blueprintHint")}</span>
              </span>
              <Button
                variant="ghost"
                size="sm"
                disabled={adopting !== null}
                onClick={() => void adopt(staleIds, "all")}
              >
                {t("staff.overview.blueprintAdoptAll", { count: stale.size })}
              </Button>
            </div>
          </div>
        </>
      ) : null}

      <div className="staff-section-title">
        {t("staff.overview.managersTitle")}
        <span className="staff-count">{managers.length}</span>
      </div>
      {managers.length === 0 ? (
        <p className="staff-hint">{t("staff.overview.noManagers")}</p>
      ) : (
        <div className="staff-grid">{managers.map(renderEmployee)}</div>
      )}

      <div className="staff-section-title">
        {t("staff.overview.workersTitle")}
        <span className="staff-count">{workers.length}</span>
      </div>
      <div className="staff-grid">{workers.map(renderEmployee)}</div>
    </div>
  );
}
