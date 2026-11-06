/**
 * "My apps": the console, the two pages the owner reads daily, and the boards
 * the employees feed.
 *
 * A badge here is not traffic: it is work waiting on the owner. Today counts
 * everything that cannot move without them -- a draft to approve, a piece to
 * publish, a proposal to answer, a conversation ready to close -- and each
 * board counts its own share, so a red number on a row and the list behind it
 * are the same list. An entry with nothing to decide shows a resting dot
 * instead of a zero, so a quiet day reads as quiet rather than as an error.
 *
 * The whole rail shares one read. Four separate components each polling their
 * own count would put eight calls a minute on the host to draw six numbers,
 * and the numbers would drift apart while they loaded.
 */
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { needsAddress } from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { useAppStore, type AppState } from "../../stores/app-store";
import {
  IconActivity,
  IconBot,
  IconBookOpen,
  IconDatabase,
  IconFileText,
  IconListChecks,
  IconSparkles,
  IconTarget,
  IconWorkflow,
} from "../../components/icons";

type AppPage = Extract<
  AppState["page"],
  "today" | "pipeline" | "staff" | "leads" | "contents" | "report" | "data" | "arsenal" | "stocks"
>;

/** The rows below the label, in the order the owner reads them. */
const ENTRIES: Array<{ page: AppPage; labelKey: string; hintKey: string; icon: ReactNode }> = [
  {
    page: "today",
    labelKey: "desk.nav.today",
    hintKey: "desk.nav.todayHint",
    icon: <IconListChecks size={13} aria-hidden />,
  },
  {
    page: "pipeline",
    labelKey: "desk.nav.pipeline",
    hintKey: "desk.nav.pipelineHint",
    icon: <IconWorkflow size={13} aria-hidden />,
  },
  {
    page: "staff",
    labelKey: "staff.nav",
    hintKey: "staff.navHint",
    icon: <IconBot size={13} aria-hidden />,
  },
  {
    page: "leads",
    labelKey: "boards.leads.nav",
    hintKey: "boards.leads.navHint",
    icon: <IconTarget size={13} aria-hidden />,
  },
  {
    page: "contents",
    labelKey: "boards.contents.nav",
    hintKey: "boards.contents.navHint",
    icon: <IconFileText size={13} aria-hidden />,
  },
  {
    page: "report",
    labelKey: "boards.report.nav",
    hintKey: "boards.report.navHint",
    icon: <IconSparkles size={13} aria-hidden />,
  },
  {
    page: "data",
    labelKey: "boards.data.nav",
    hintKey: "boards.data.navHint",
    icon: <IconDatabase size={13} aria-hidden />,
  },
  {
    page: "arsenal",
    labelKey: "boards.arsenal.nav",
    hintKey: "boards.arsenal.navHint",
    icon: <IconBookOpen size={13} aria-hidden />,
  },
  {
    page: "stocks",
    labelKey: "stocks.nav",
    hintKey: "stocks.navHint",
    icon: <IconActivity size={13} aria-hidden />,
  },
];

type Counts = Partial<Record<AppPage, number>>;

export function AppsNav({ running }: { running: number }) {
  const { t } = useTranslation();
  const page = useAppStore((s) => s.page);
  const setPage = useAppStore((s) => s.setPage);
  const [counts, setCounts] = useState<Counts>({});

  const refresh = useCallback(async () => {
    const [leads, contents, published, advice] = await Promise.all([
      api.listStaffLeads({ limit: 1 }).catch(() => null),
      api.listStaffContents({ limit: 1 }).catch(() => null),
      api.listStaffContents({ status: "published", limit: 200 }).catch(() => null),
      api.listStaffAdvice({ status: "open" }).catch(() => null),
    ]);
    const leadCounts = leads?.counts ?? {};
    const contentCounts = contents?.counts ?? {};
    const open = advice?.suggestions ?? [];
    // A piece that went out without its address is an owner task as well: the
    // return leg reads the replies back from that link, so a missing one is a
    // piece nobody will ever hear about again. A private message is the
    // exception the host and the desk both make, and it has to be made here
    // too -- a badge that counts work the desk already considers done is a
    // number that never clears.
    const unlinked = (published?.contents ?? []).filter((item) => needsAddress(item)).length;
    setCounts({
      today:
        (contentCounts.review ?? 0) +
        (contentCounts.approved ?? 0) +
        unlinked +
        (leadCounts.talking ?? 0) +
        open.length,
      leads: (leadCounts.new ?? 0) + (leadCounts.replied ?? 0),
      contents: contentCounts.review ?? 0,
      report: open.filter((item) => item.board === "report").length,
      arsenal: open.filter((item) => item.board === "arsenal").length,
    });
  }, []);

  useEffect(() => {
    void refresh();
    const off = api.onStaffChanged(() => void refresh());
    const timer = window.setInterval(() => void refresh(), 30_000);
    return () => {
      off();
      window.clearInterval(timer);
    };
  }, [refresh]);

  return (
    <>
      {ENTRIES.map((entry) => {
        const waiting = entry.page === "staff" ? running : (counts[entry.page] ?? 0);
        const active = page === entry.page;
        return (
          <button
            key={entry.page}
            type="button"
            className={"sidebar-app-row" + (active ? " on" : "")}
            data-nav={entry.page}
            aria-current={active ? "page" : undefined}
            title={t(entry.hintKey)}
            onClick={() => setPage(entry.page)}
          >
            <span className="sidebar-app-icon">{entry.icon}</span>
            <span className="sidebar-app-name">{t(entry.labelKey)}</span>
            {waiting > 0 ? (
              <span className="sidebar-app-count" data-waiting={waiting}>
                {waiting}
              </span>
            ) : (
              <span className="staff-dot" aria-hidden />
            )}
          </button>
        );
      })}
    </>
  );
}