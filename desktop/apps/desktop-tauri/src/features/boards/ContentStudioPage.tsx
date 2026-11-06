/**
 * The content studio: what we intend to say, and whether the owner has said yes.
 *
 * A draft arrives from a writer with versions side by side, because the first
 * sentence is the whole difference between an answer and an advert. Approval is
 * the owner's door and this page is where it opens: nothing an employee writes
 * can move itself into "approved", so a piece that is out there was let out by
 * a person.
 *
 * The last mile is the owner's too. This page does not post anywhere -- it
 * cannot, it holds no session for any site -- so what it does instead is make
 * the posting a single action: copy the words, open the thread, paste the
 * words, and record where they landed. A draft that claims to be published
 * without a link is a piece nobody can go back and read, so publishing asks
 * for the link rather than assuming one.
 *
 * The link is also all the return leg needs. The watch reads every published
 * piece back from that address on a timer -- the counters the site holds, and
 * whoever answered underneath -- and files the answers as cards on its own,
 * which is why the strip above the columns reports when it last ran instead
 * of asking for numbers. Typing them in stays as the fallback for a piece the
 * signed-out browser cannot read.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { StaffContent, StaffContentStatus, StaffWatchStatus } from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { useAppStore } from "../../stores/app-store";
import { Badge, Button, Field, Input, Textarea } from "../../components/ui";
import { IconCheck, IconCopy, IconExternal, IconRefresh, IconSparkles } from "../../components/icons";
import { AdviceRail } from "./AdviceRail";
import { ManagerDeck } from "./ManagerDeck";
import { BoardHeader, useBoardAdvice } from "./useBoardAdvice";
import {
  CONTENT_COLUMNS,
  CONTENT_STATUSES,
  agoLabel,
  contentStatusKey,
  draftRows,
  number,
} from "./board-format";
import { errorText, whenLabel } from "../staff/staff-format";

export function ContentStudioPage() {
  const { t, i18n } = useTranslation();
  const showToast = useAppStore((s) => s.showToast);
  const [contents, setContents] = useState<StaffContent[]>([]);
  const [counts, setCounts] = useState<Record<string, number>>({});
  const [editing, setEditing] = useState<{ id: string; body: string } | null>(null);
  /** The link a draft went out under, being typed. */
  const [publishing, setPublishing] = useState<{ id: string; url: string } | null>(null);
  /** The counters the owner read off the site, being typed. */
  const [measuring, setMeasuring] = useState<{
    id: string;
    views: string;
    replies: string;
  } | null>(null);
  /** Where each card the drafts answer actually lives, keyed by lead id. */
  const [threads, setThreads] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  /** What the return leg last read back, and when it will read again. */
  const [watch, setWatch] = useState<StaffWatchStatus | null>(null);
  const [watchBusy, setWatchBusy] = useState(false);

  const advice = useBoardAdvice("contents", (message) => showToast(message, { variant: "error" }));

  /**
   * The state of the return leg.
   *
   * A watch that cannot be read must not take the board down with it: the
   * drafts are the owner’s either way, and the cards below still work.
   */
  const loadWatch = useCallback(async () => {
    try {
      setWatch(await api.staffWatchStatus());
    } catch {
      setWatch(null);
    }
  }, []);

  const load = useCallback(async () => {
    try {
      const [answer, leads] = await Promise.all([
        api.listStaffContents({ limit: 500 }),
        api.listStaffLeads({ limit: 500 }).catch(() => ({ leads: [] })),
      ]);
      setContents(answer.contents ?? []);
      setCounts(answer.counts ?? {});
      const map: Record<string, string> = {};
      for (const lead of leads.leads ?? []) {
        if (lead.url) map[lead.id] = lead.url;
      }
      setThreads(map);
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    }
  }, [showToast]);

  useEffect(() => {
    void load();
    void loadWatch();
    const off = api.onStaffChanged(() => {
      void load();
      void loadWatch();
    });
    return off;
  }, [load, loadWatch]);

  const byStatus = useMemo(() => {
    const grouped = new Map<string, StaffContent[]>();
    for (const status of CONTENT_STATUSES) grouped.set(status, []);
    for (const content of contents) {
      const bucket = grouped.get(content.status);
      if (bucket) bucket.push(content);
    }
    return grouped;
  }, [contents]);

  // Retired drafts are kept, not shown: the column appears only once
  // something is actually in it, so a board with nothing retired reads the
  // same as one that never had anything to retire.
  const boardColumns = useMemo<StaffContentStatus[]>(
    () =>
      (counts.retired ?? 0) > 0
        ? [...CONTENT_COLUMNS, "retired"]
        : [...CONTENT_COLUMNS],
    [counts],
  );

  const replace = useCallback((content: StaffContent) => {
    setContents((current) => current.map((item) => (item.id === content.id ? content : item)));
  }, []);

  const setStatus = useCallback(
    async (content: StaffContent, status: StaffContentStatus, body: string, publishedUrl?: string) => {
      setBusy(true);
      try {
        const answer = await api.setStaffContentStatus({
          id: content.id,
          status,
          body,
          publishedUrl,
        });
        replace(answer.content);
        setEditing(null);
        setPublishing(null);
      } catch (error) {
        showToast(errorText(error), { variant: "error" });
      } finally {
        setBusy(false);
      }
    },
    [replace, showToast],
  );

  /**
   * Put the words on the clipboard, which is the half of publishing this
   * application can do without holding anybody's account.
   */
  const copy = useCallback(
    async (body: string) => {
      try {
        await navigator.clipboard.writeText(body);
        showToast(t("boards.contents.copied"), { variant: "success" });
      } catch (error) {
        showToast(errorText(error), { variant: "error" });
      }
    },
    [showToast, t],
  );

  /**
   * Read the site's counters back onto the card.
   *
   * The write carries the status the card already has, so measuring a piece
   * can never approve or publish it by accident.
   */
  const saveMeasure = useCallback(
    async (content: StaffContent, body: string) => {
      if (!measuring || measuring.id !== content.id) return;
      setBusy(true);
      try {
        const answer = await api.saveStaffContent({
          id: content.id,
          title: content.title,
          body,
          views: Number(measuring.views || 0),
          replies: Number(measuring.replies || 0),
        });
        replace(answer.content);
        setMeasuring(null);
      } catch (error) {
        showToast(errorText(error), { variant: "error" });
      } finally {
        setBusy(false);
      }
    },
    [measuring, replace, showToast],
  );

  /**
   * Read the return leg now instead of waiting for the timer.
   *
   * It is the same pass the resident loop runs, so a read the owner asked
   * for and one that happened on its own leave the same evidence behind.
   */
  const runWatch = useCallback(async () => {
    setWatchBusy(true);
    try {
      const outcome = await api.staffWatchRun();
      setWatch((current) =>
        current ? { ...current, last: outcome, lastError: undefined } : current,
      );
      await load();
      await loadWatch();
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    } finally {
      setWatchBusy(false);
    }
  }, [load, loadWatch, showToast]);

  return (
    <div className="thread-scroll">
      <div className="page-frame board-page">
        <BoardHeader
          title={t("boards.contents.title")}
          subtitle={t("boards.contents.subtitle")}
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

        <ManagerDeck board="contents" />

        {/* The return leg. The site knows who answered; this reads it back so
            the owner never types in numbers the page already holds. */}
        <div className="watch-bar" data-watch={watch?.running || watchBusy ? "running" : "idle"}>
          <div className="watch-head">
            <span className="watch-title">{t("boards.contents.watchTitle")}</span>
            <span className="watch-when">
              {watch?.last
                ? t("boards.contents.watchLast", {
                    at: whenLabel(watch.last.at, i18n.language),
                  })
                : t("boards.contents.watchNever")}
            </span>
            <span className="watch-when">
              {watch?.resident
                ? t("boards.contents.watchNext", {
                    at: whenLabel(watch.nextAt, i18n.language),
                  })
                : t("boards.contents.watchOffline")}
            </span>
            <Button
              variant="ghost"
              size="sm"
              disabled={watchBusy || watch?.running}
              onClick={() => void runWatch()}
            >
              <IconRefresh size={13} aria-hidden />
              {watch?.running ? t("boards.contents.watchReading") : t("boards.contents.watchRun")}
            </Button>
          </div>
          {watch?.last ? (
            <div className="watch-stats">
              <span>
                {t("boards.contents.watchRead", {
                  found: watch.last.found,
                  targets: watch.last.targets,
                })}
              </span>
              <span>{t("boards.contents.watchNewReplies", { count: watch.last.newReplies })}</span>
              <span>{t("boards.contents.watchLeads", { count: watch.last.leads })}</span>
            </div>
          ) : null}
          {watch?.lastError ? (
            <p className="staff-error">
              {t("boards.contents.watchError", { error: watch.lastError })}
            </p>
          ) : null}
          {watch?.last && watch.last.lines.length > 0 ? (
            <ul className="watch-lines">
              {watch.last.lines.map((line, index) => (
                <li key={line + ":" + index}>{line}</li>
              ))}
            </ul>
          ) : (
            <p className="staff-hint">{t("boards.contents.watchHint")}</p>
          )}
        </div>

        <div className="staff-strip">
          {boardColumns.map((status) => (
            <div className="staff-stat" key={status}>
              <b>{number(counts[status] ?? 0)}</b>
              <span>{t(contentStatusKey(status))}</span>
            </div>
          ))}
        </div>

        <div className="board-split">
          <div className="content-list">
            {boardColumns.map((status) => {
              const items = byStatus.get(status) ?? [];
              if (items.length === 0) return null;
              return (
                <section key={status} className="content-group" data-status={status}>
                  <div className="staff-section-title">
                    {t(contentStatusKey(status))}
                    <span className="staff-count">{items.length}</span>
                  </div>
                  {status === "queued" ? (
                    <p className="staff-hint">{t("boards.contents.queuedHint")}</p>
                  ) : null}
                  {items.map((content) => {
                    const body = editing?.id === content.id ? editing.body : content.body;
                    const settled = status === "published" || status === "retired";
                    const thread = content.leadId ? threads[content.leadId] : "";
                    return (
                      <article key={content.id} className="draft-card" data-content={content.id}>
                        <header className="draft-head">
                          <h3 className="draft-title">{content.title}</h3>
                          <div className="draft-tags">
                            {content.variant ? <Badge>{t("boards.contents.variant", { slot: content.variant })}</Badge> : null}
                            {content.channel ? <Badge>{content.channel}</Badge> : null}
                            <span className="draft-age">{agoLabel(content.updatedAt, t)}</span>
                          </div>
                        </header>
                        <Textarea
                          value={body}
                          rows={draftRows(body)}
                          readOnly={settled}
                          onChange={(event) =>
                            setEditing({ id: content.id, body: event.target.value })
                          }
                        />
                        <div className="draft-actions">
                          {status !== "published" && status !== "retired" && status !== "queued" ? (
                            <Button
                              variant="primary"
                              size="sm"
                              disabled={busy}
                              onClick={() => void setStatus(content, "approved", body)}
                            >
                              <IconCheck size={13} aria-hidden />
                              {t("boards.contents.approve")}
                            </Button>
                          ) : null}
                          <Button variant="ghost" size="sm" onClick={() => void copy(body)}>
                            <IconCopy size={13} aria-hidden />
                            {t("boards.contents.copy")}
                          </Button>
                          {status === "approved" || status === "queued" ? (
                            <Button
                              variant="ghost"
                              size="sm"
                              disabled={busy}
                              onClick={() => setPublishing({ id: content.id, url: "" })}
                            >
                              {t("boards.contents.publish")}
                            </Button>
                          ) : null}
                          {status === "published" ? (
                            <Button
                              variant="ghost"
                              size="sm"
                              disabled={busy}
                              onClick={() =>
                                setMeasuring({
                                  id: content.id,
                                  views: content.views ? String(content.views) : "",
                                  replies: content.replies ? String(content.replies) : "",
                                })
                              }
                            >
                              {t("boards.contents.measure")}
                            </Button>
                          ) : null}
                          {status === "published" && content.publishedUrl ? (
                            <a
                              className="lead-link"
                              href={content.publishedUrl}
                              target="_blank"
                              rel="noreferrer"
                            >
                              <IconExternal size={12} aria-hidden />
                              {t("boards.contents.openPublished")}
                            </a>
                          ) : null}
                          {content.views > 0 || content.replies > 0 ? (
                            <span className="draft-metrics">
                              {t("boards.contents.seen", {
                                views: content.views,
                                replies: content.replies,
                              })}
                            </span>
                          ) : null}
                        </div>

                        {publishing?.id === content.id ? (
                          <div className="draft-backfill">
                            <Field label={t("boards.contents.publishWhere")}>
                              <Input
                                value={publishing.url}
                                placeholder={t("boards.contents.publishWherePlaceholder")}
                                onChange={(event) =>
                                  setPublishing({ id: content.id, url: event.target.value })
                                }
                              />
                            </Field>
                            <div className="draft-actions">
                              {thread ? (
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  onClick={() => setPublishing({ id: content.id, url: thread })}
                                >
                                  {t("boards.contents.publishUseSource")}
                                </Button>
                              ) : null}
                              {thread ? (
                                <a className="lead-link" href={thread} target="_blank" rel="noreferrer">
                                  <IconExternal size={12} aria-hidden />
                                  {t("boards.contents.publishOpenSource")}
                                </a>
                              ) : null}
                              <Button
                                variant="primary"
                                size="sm"
                                disabled={busy}
                                onClick={() => void setStatus(content, "published", body, publishing.url.trim())}
                              >
                                <IconCheck size={13} aria-hidden />
                                {t("boards.contents.publishConfirm")}
                              </Button>
                              <Button variant="ghost" size="sm" onClick={() => setPublishing(null)}>
                                {t("common.cancel")}
                              </Button>
                            </div>
                          </div>
                        ) : null}

                        {measuring?.id === content.id ? (
                          <div className="draft-backfill">
                            <p className="staff-hint">{t("boards.contents.measureHint")}</p>
                            <div className="metric-entry">
                              <Field label={t("boards.report.field.views")}>
                                <Input
                                  value={measuring.views}
                                  inputMode="numeric"
                                  onChange={(event) =>
                                    setMeasuring({
                                      id: content.id,
                                      views: event.target.value.replace(/[^0-9]/g, ""),
                                      replies: measuring.replies,
                                    })
                                  }
                                />
                              </Field>
                              <Field label={t("boards.report.field.replies")}>
                                <Input
                                  value={measuring.replies}
                                  inputMode="numeric"
                                  onChange={(event) =>
                                    setMeasuring({
                                      id: content.id,
                                      views: measuring.views,
                                      replies: event.target.value.replace(/[^0-9]/g, ""),
                                    })
                                  }
                                />
                              </Field>
                              <Button
                                variant="ghost"
                                size="sm"
                                disabled={busy}
                                onClick={() => void saveMeasure(content, body)}
                              >
                                {t("common.save")}
                              </Button>
                              <Button variant="ghost" size="sm" onClick={() => setMeasuring(null)}>
                                {t("common.cancel")}
                              </Button>
                            </div>
                          </div>
                        ) : null}
                      </article>
                    );
                  })}
                </section>
              );
            })}
            {contents.length === 0 ? (
              <p className="staff-hint">{t("boards.contents.empty")}</p>
            ) : null}
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
