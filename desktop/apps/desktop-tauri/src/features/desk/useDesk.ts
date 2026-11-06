/**
 * One load of everything the desk reads, kept in step with the team.
 *
 * The three pages ask different questions of the same rows, so the rows are
 * fetched once here rather than three times beside three pages. Reads are
 * event-driven: a run that files a card tells the renderer, and the desk
 * reloads because something actually happened. The timer is there for the one
 * thing no event announces -- the clock moving past an employee's next slot.
 *
 * A read that fails is not allowed to take the page down. The desk is a
 * summary of nine employees, and one of those calls being slow or absent is
 * exactly the situation the page exists to show, so a failed leg comes back as
 * empty and the page says what it could not read.
 */
import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import { EMPTY_DESK, type DeskSnapshot } from "./desk-model";

/** How often the desk re-reads the clock, in milliseconds. */
const TICK_MS = 30_000;

export type DeskState = {
  snapshot: DeskSnapshot;
  loading: boolean;
  /** True once a load has come back, so an empty desk draws as empty and not as pending. */
  loaded: boolean;
  reload: () => Promise<void>;
};

export function useDesk(): DeskState {
  const [snapshot, setSnapshot] = useState<DeskSnapshot>(EMPTY_DESK);
  const [loading, setLoading] = useState(false);
  const [loaded, setLoaded] = useState(false);

  const reload = useCallback(async () => {
    setLoading(true);
    const [
      employees,
      runs,
      contents,
      leads,
      metrics,
      advice,
      directives,
      memory,
      sites,
      watch,
      scheduler,
      summary,
    ] = await Promise.all([
      api.listStaffEmployees().catch(() => null),
      api.listStaffRuns(undefined, 200).catch(() => null),
      api.listStaffContents({ limit: 500 }).catch(() => null),
      api.listStaffLeads({ limit: 500 }).catch(() => null),
      api.listStaffMetrics().catch(() => null),
      api.listStaffAdvice({}).catch(() => null),
      api.listStaffDirectives({ limit: 200 }).catch(() => null),
      api.queryStaffMemory({ limit: 300 }).catch(() => null),
      api.listStaffSites().catch(() => null),
      api.staffWatchStatus().catch(() => null),
      api.staffSchedulerStatus().catch(() => null),
      api.staffReportSummary({ days: 14 }).catch(() => null),
    ]);
    setSnapshot({
      employees: employees?.employees ?? [],
      runs: runs?.runs ?? [],
      contents: contents?.contents ?? [],
      leads: leads?.leads ?? [],
      metrics: metrics?.metrics ?? [],
      advice: advice?.suggestions ?? [],
      directives: directives?.directives ?? [],
      memory: memory?.items ?? [],
      sites: sites?.sites ?? [],
      watch,
      scheduler,
      summary,
    });
    setLoading(false);
    setLoaded(true);
  }, []);

  useEffect(() => {
    void reload();
    const off = api.onStaffChanged(() => void reload());
    const timer = window.setInterval(() => void reload(), TICK_MS);
    return () => {
      off();
      window.clearInterval(timer);
    };
  }, [reload]);

  return { snapshot, loading, loaded, reload };
}