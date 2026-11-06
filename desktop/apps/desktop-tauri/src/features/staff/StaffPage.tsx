/**
 * The staff console.
 *
 * One page, six tabs, and a header that answers the only question that
 * matters about an unattended system: is the driver alive right now, and what
 * is it running? The header polls; the tabs subscribe to staffChanged. The
 * first tab exists to answer the second question -- does anything need me?
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { StaffEmployee, StaffSchedulerStatus } from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { useAppStore } from "../../stores/app-store";
import { StaffCredentialPanel } from "./StaffCredentialPanel";
import { StaffEmployeeEditor } from "./StaffEmployeeEditor";
import { StaffMemoryPanel } from "./StaffMemoryPanel";
import { StaffOverviewPanel } from "./StaffOverviewPanel";
import { StaffRunLog } from "./StaffRunLog";
import { StaffSitePanel } from "./StaffSitePanel";
import { StaffWorkflowEditor } from "./StaffWorkflowEditor";
import { errorText, whenLabel } from "./staff-format";

type Tab =
  | "overview"
  | "employees"
  | "sites"
  | "workflows"
  | "memory"
  | "credentials"
  | "runs";

const TABS: Array<{ id: Tab; labelKey: string }> = [
  { id: "overview", labelKey: "staff.tabs.overview" },
  { id: "employees", labelKey: "staff.tabs.employees" },
  { id: "sites", labelKey: "staff.tabs.sites" },
  { id: "workflows", labelKey: "staff.tabs.workflows" },
  { id: "memory", labelKey: "staff.tabs.memory" },
  { id: "credentials", labelKey: "staff.tabs.credentials" },
  { id: "runs", labelKey: "staff.tabs.runs" },
];

export function StaffPage() {
  const { t, i18n } = useTranslation();
  const showToast = useAppStore((s) => s.showToast);
  const [tab, setTab] = useState<Tab>("overview");
  const [status, setStatus] = useState<StaffSchedulerStatus | null>(null);
  const [employees, setEmployees] = useState<StaffEmployee[]>([]);
  const [refreshToken, setRefreshToken] = useState(0);

  const refreshStatus = useCallback(async () => {
    try {
      setStatus(await api.staffSchedulerStatus());
    } catch {
      setStatus(null);
    }
  }, []);

  const refreshRoster = useCallback(async () => {
    try {
      const res = await api.listStaffEmployees();
      setEmployees(res.employees ?? []);
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    }
  }, [showToast]);

  const refreshAll = useCallback(() => {
    void refreshStatus();
    void refreshRoster();
    setRefreshToken((value) => value + 1);
  }, [refreshRoster, refreshStatus]);

  useEffect(() => {
    refreshAll();
  }, [refreshAll]);

  useEffect(() => {
    const off = api.onStaffChanged(() => refreshAll());
    return off;
  }, [refreshAll]);

  useEffect(() => {
    const timer = window.setInterval(() => void refreshStatus(), 5000);
    return () => window.clearInterval(timer);
  }, [refreshStatus]);

  const resident = status?.resident ?? false;
  const seconds = status ? Math.round(status.pollMs / 1000) : 0;

  return (
    <div className="thread-scroll">
      <div className="page-frame staff-page">
        <div className="page-header">
          <div>
            <h1 className="page-title">{t("staff.title")}</h1>
            <p className="staff-hint">{t("staff.subtitle")}</p>
          </div>
        </div>

        <div className="staff-strip">
          <div className="staff-stat">
            <b>
              <span
                className={"staff-dot " + (resident ? "on" : "bad")}
                aria-hidden
              />{" "}
              {resident ? t("staff.residentOn") : t("staff.residentOff")}
            </b>
            <span>{t("staff.residentLabel")}</span>
          </div>
          <div className="staff-stat">
            <b>{status ? String(status.inFlight.length) : "-"}</b>
            <span>{t("staff.inFlight")}</span>
          </div>
          <div className="staff-stat">
            <b>{(status?.claimedTotal ?? 0).toString()}</b>
            <span>{t("staff.claimedTotal")}</span>
          </div>
          <div className="staff-stat">
            <b>{whenLabel(status?.lastTickAt, i18n.language) || t("staff.never")}</b>
            <span>{t("staff.lastTick")}</span>
          </div>
          <div className="staff-stat">
            <b>{seconds > 0 ? seconds + "s" : "-"}</b>
            <span>{t("staff.pollInterval")}</span>
          </div>
        </div>

        {status?.lastError ? (
          <div className="staff-error staff-mono">
            {t("staff.lastError")}
            {": "}
            {status.lastError}
          </div>
        ) : null}

        {status && status.inFlight.length > 0 ? (
          <div className="staff-toolbar">
            {status.inFlight.map((flight) => (
              <span key={flight.runId} className="staff-tag">
                {flight.employeeName}
                {" · "}
                {t("staff.runs.stepOf", { current: flight.step, total: flight.stepCount })}
              </span>
            ))}
          </div>
        ) : null}

        <div className="staff-tabs">
          {TABS.map((item) => (
            <button
              key={item.id}
              type="button"
              className={"staff-tab" + (tab === item.id ? " on" : "")}
              onClick={() => setTab(item.id)}
            >
              {t(item.labelKey)}
            </button>
          ))}
        </div>

        {tab === "overview" ? (
          <StaffOverviewPanel
            employees={employees}
            status={status}
            refreshToken={refreshToken}
          />
        ) : null}
        {tab === "employees" ? (
          <StaffEmployeeEditor
            employees={employees}
            onChanged={refreshAll}
            refreshToken={refreshToken}
          />
        ) : null}
        {tab === "sites" ? <StaffSitePanel /> : null}
        {tab === "workflows" ? <StaffWorkflowEditor employees={employees} /> : null}
        {tab === "memory" ? <StaffMemoryPanel employees={employees} /> : null}
        {tab === "credentials" ? <StaffCredentialPanel /> : null}
        {tab === "runs" ? (
          <StaffRunLog employees={employees} refreshToken={refreshToken} />
        ) : null}
      </div>
    </div>
  );
}
