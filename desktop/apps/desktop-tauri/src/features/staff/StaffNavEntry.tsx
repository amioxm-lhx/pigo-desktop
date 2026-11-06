/**
 * Sidebar entry for "My apps": where the owner's day starts.
 *
 * It lives in the left rail rather than behind the search dialog because the
 * resident driver's health is something the user should be able to glance at,
 * not something they have to remember to look up -- and the same is now true of
 * what the team is waiting on. The rail itself is drawn by `AppsNav`, which
 * owns the one read all the rows share; this component owns only the thing the
 * rail cannot know on its own, the driver's status.
 *
 * The label belongs to the group, not to the row: everything below it is one
 * section, and the section is what "My apps" names.
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { StaffSchedulerStatus } from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { AppsNav } from "../boards/AppsNav";

export function StaffNavEntry() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<StaffSchedulerStatus | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStatus(await api.staffSchedulerStatus());
    } catch {
      setStatus(null);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const off = api.onStaffChanged(() => void refresh());
    const timer = window.setInterval(() => void refresh(), 20_000);
    return () => {
      off();
      window.clearInterval(timer);
    };
  }, [refresh]);

  return (
    <div className="sidebar-apps no-drag">
      <div className="sidebar-apps-label">{t("staff.navSection")}</div>
      <AppsNav running={status?.inFlight.length ?? 0} />
    </div>
  );
}