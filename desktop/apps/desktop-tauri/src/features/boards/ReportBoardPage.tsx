/**
 * The daily board: what the day did, read off the rows the other boards hold.
 *
 * Nothing here is stored as a total. Every number is a count of cards that
 * already exist, so this page cannot disagree with the boards it summarizes --
 * which is the whole reason to have one: a ranking that contradicts its own
 * funnel is worse than no ranking.
 *
 * The ranking is what the next day is for. A topic that leads by a wide margin
 * is the one to write about tomorrow, and saying so is a proposal like any
 * other: it goes through the same accept-or-reject rail as the rest.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { StaffReportSummary } from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { useAppStore } from "../../stores/app-store";
import { Button, Field, Input } from "../../components/ui";
import { IconRefresh, IconSparkles } from "../../components/icons";
import { AdviceRail } from "./AdviceRail";
import { ManagerDigest } from "./ManagerDeck";
import { BoardHeader, useBoardAdvice } from "./useBoardAdvice";
import { delta, metricFieldKey, number, trendPeak } from "./board-format";
import { errorText } from "../staff/staff-format";

/** The counters an owner reads off a site's own dashboard, in order. */
const ENTRY_FIELDS = ["views", "replies", "leads", "inquiries", "deals"] as const;
type EntryField = (typeof ENTRY_FIELDS)[number];

const FUNNEL: Array<{ key: keyof StaffReportSummary["today"]; labelKey: string }> = [
  { key: "leads", labelKey: "boards.report.leads" },
  { key: "contacted", labelKey: "boards.report.contacted" },
  { key: "replied", labelKey: "boards.report.replied" },
  { key: "won", labelKey: "boards.report.won" },
];

export function ReportBoardPage() {
  const { t } = useTranslation();
  const showToast = useAppStore((s) => s.showToast);
  const [summary, setSummary] = useState<StaffReportSummary | null>(null);
  const [entry, setEntry] = useState<{ label: string } & Record<EntryField, string>>({
    label: "",
    views: "",
    replies: "",
    leads: "",
    inquiries: "",
    deals: "",
  });
  const [saving, setSaving] = useState(false);

  const advice = useBoardAdvice("report", (message) => showToast(message, { variant: "error" }));

  const load = useCallback(async () => {
    try {
      setSummary(await api.staffReportSummary({ days: 14 }));
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    }
  }, [showToast]);

  useEffect(() => {
    void load();
    const off = api.onStaffChanged(() => void load());
    return off;
  }, [load]);

  /**
   * The counters are typed in rather than scraped.
   *
   * A site prints its own view and reply counts, and nothing this application
   * does can read them without the owner's session -- so the honest instrument
   * is the owner reading them off and the board storing what they read. What
   * the board then does with them is arithmetic, which is the part it can do.
   */
  const saveEntry = useCallback(async () => {
    const label = entry.label.trim();
    if (!label) {
      showToast(t("boards.report.entryNeedsLabel"), { variant: "error" });
      return;
    }
    setSaving(true);
    try {
      await api.saveStaffMetric({
        dimension: "topic",
        label,
        views: Number(entry.views || 0),
        replies: Number(entry.replies || 0),
        leads: Number(entry.leads || 0),
        inquiries: Number(entry.inquiries || 0),
        deals: Number(entry.deals || 0),
      });
      setEntry({ label, views: "", replies: "", leads: "", inquiries: "", deals: "" });
      await load();
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    } finally {
      setSaving(false);
    }
  }, [entry, load, showToast, t]);

  const today = summary?.today;
  const yesterday = summary?.yesterday;
  const peak = trendPeak((summary?.trend ?? []).map((point) => point.views));

  return (
    <div className="thread-scroll">
      <div className="page-frame board-page">
        <BoardHeader
          title={t("boards.report.title")}
          subtitle={t("boards.report.subtitle")}
          actions={
            <>
              <Button variant="ghost" size="sm" onClick={() => void load()}>
                <IconRefresh size={13} aria-hidden />
                {t("boards.refresh")}
              </Button>
              <Button
                variant="primary"
                size="sm"
                disabled={advice.analyzing}
                onClick={() => void advice.analyze()}
              >
                <IconSparkles size={13} aria-hidden />
                {advice.analyzing ? t("boards.analyzing") : t("boards.analyze")}
              </Button>
            </>
          }
        />

        <ManagerDigest />

        <div className="staff-strip">
          {FUNNEL.map((step) => (
            <div className="staff-stat" key={String(step.key)}>
              <b>{number(today?.[step.key] as number)}</b>
              <span>{t(step.labelKey)}</span>
              <span className="stat-delta">
                {delta(
                  (today?.[step.key] as number) ?? 0,
                  (yesterday?.[step.key] as number) ?? 0,
                  t,
                )}
              </span>
            </div>
          ))}
        </div>

        <div className="board-split">
          <div className="report-main">
            <section className="report-block">
              <div className="staff-section-title">{t("boards.report.topicRank")}</div>
              {summary && summary.topics.length > 0 ? (
                <ol className="topic-rank">
                  {summary.topics.map((topic, index) => (
                    <li key={topic.label} data-rank={index + 1}>
                      <span className="topic-name">{topic.label}</span>
                      <span className="topic-numbers">
                        {t("boards.report.topicNumbers", {
                          views: topic.views,
                          replies: topic.replies,
                          inquiries: topic.inquiries,
                        })}
                      </span>
                    </li>
                  ))}
                </ol>
              ) : (
                <p className="staff-hint">{t("boards.report.noTopics")}</p>
              )}
            </section>

            <section className="report-block">
              <div className="staff-section-title">{t("boards.report.trend")}</div>
              {summary && summary.trend.length > 0 ? (
                <div className="trend-bars">
                  {summary.trend.map((point) => (
                    <div className="trend-bar" key={point.day} title={point.day}>
                      <div
                        className="trend-fill"
                        style={{ height: Math.round((point.views / peak) * 100) + "%" }}
                      />
                      <span className="trend-day">{point.day.slice(5)}</span>
                    </div>
                  ))}
                </div>
              ) : (
                <p className="staff-hint">{t("boards.report.noTrend")}</p>
              )}
            </section>

            <section className="report-block">
              <div className="staff-section-title">{t("boards.report.entry")}</div>
              <p className="staff-hint">{t("boards.report.entryHint")}</p>
              <div className="metric-entry">
                <Field label={t("boards.report.entryLabel")}>
                  <Input
                    value={entry.label}
                    placeholder={t("boards.report.entryLabelPlaceholder")}
                    onChange={(event) => setEntry({ ...entry, label: event.target.value })}
                  />
                </Field>
                {ENTRY_FIELDS.map((field) => (
                  <Field key={field} label={t(metricFieldKey(field))}>
                    <Input
                      value={entry[field]}
                      inputMode="numeric"
                      onChange={(event) =>
                        setEntry({ ...entry, [field]: event.target.value.replace(/[^0-9]/g, "") })
                      }
                    />
                  </Field>
                ))}
                <Button variant="ghost" size="sm" disabled={saving} onClick={() => void saveEntry()}>
                  {t("boards.report.entrySave")}
                </Button>
              </div>
            </section>

            <section className="report-block">
              <div className="staff-section-title">{t("boards.report.output")}</div>
              <div className="report-counters">
                <span>{t("boards.report.approved", { count: today?.approved ?? 0 })}</span>
                <span>{t("boards.report.published", { count: today?.published ?? 0 })}</span>
                <span>
                  {t("boards.report.seen", {
                    views: today?.views ?? 0,
                    replies: today?.replies ?? 0,
                  })}
                </span>
              </div>
            </section>
          </div>

          <AdviceRail
            suggestions={advice.suggestions}
            busyId={advice.busyId}
            onDecide={(id, decision) => void advice.decide(id, decision)}
          />
        </div>
      </div>
    </div>
  );
}
