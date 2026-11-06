/**
 * Today: the one page the owner has to open.
 *
 * Everything else in the app is a view of the work; this is the view of what
 * the work is waiting for. It is built around one rule -- a row exists here
 * because something cannot move without the owner, and it leaves when the owner
 * has done it. Nothing here can be dismissed, only finished, which is the
 * difference between a to-do list and a notification tray.
 *
 * The page used to draw six blocks of equal weight, and the owner could not
 * tell which one wanted them. It now draws one: the things only a person can
 * settle. A draft is approved where it stands, and published where it stands
 * too: unfolding a publish row hands over the words, the clipboard and the
 * address on the row itself, so the last mile never sends the owner to another
 * page to fetch the draft they just read. Every row that answers a post links
 * back to it. What the machine is doing on its own -- who is on duty, the
 * loops, the newest round -- is folded into one line at the bottom, one click
 * away.
 *
 * A blocker is the exception: a site that is signed out stops every station
 * behind it, so it gets a banner above the list rather than a place in the fold.
 *
 * The strip above them counts the day the way the report page counts it, so the
 * two pages can never disagree about how many cards came in this morning.
 */
import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../lib/api";
import { useAppStore } from "../../stores/app-store";
import { Badge, Button, Textarea } from "../../components/ui";
import {
  IconCheck,
  IconChevronDown,
  IconClock,
  IconCopy,
  IconExternal,
  IconPlay,
  IconRefresh,
  IconTriangleAlert,
  IconX,
} from "../../components/icons";
import { draftRows } from "../boards/board-format";
import { BoardHeader } from "../boards/useBoardAdvice";
import { errorText } from "../staff/staff-format";
import { useDesk } from "./useDesk";
import {
  BLOCKER_TEXT,
  KPI_FIELDS,
  KPI_TEXT,
  TODO_TEXT,
  buildBlockers,
  buildEmployeeRows,
  buildTodos,
  latestRun,
  nextLoop,
  roundsToday,
  runnableNow,
  slotLabel,
  type DeskBlocker,
  type DeskTodo,
} from "./desk-model";

export function TodayPage() {
  const { t, i18n } = useTranslation();
  const showToast = useAppStore((s) => s.showToast);
  const setPage = useAppStore((s) => s.setPage);
  const { snapshot, loaded, reload } = useDesk();
  const [busy, setBusy] = useState<string | null>(null);
  const [runningAll, setRunningAll] = useState(false);
  /** The piece whose words are unfolded on its row, as they read right now. */
  const [editing, setEditing] = useState<{ id: string; body: string } | null>(null);

  const todos = useMemo(() => buildTodos(snapshot, t), [snapshot, t]);
  const blockers = useMemo(() => buildBlockers(snapshot, t), [snapshot, t]);
  const roster = useMemo(() => buildEmployeeRows(snapshot, t), [snapshot, t]);
  const loops = useMemo(() => nextLoop(snapshot), [snapshot]);
  const round = useMemo(() => latestRun(snapshot), [snapshot]);
  const runnable = useMemo(() => runnableNow(snapshot, t), [snapshot, t]);
  const rounds = useMemo(() => roundsToday(snapshot), [snapshot]);
  const today = snapshot.summary?.today;
  const onDuty = roster.filter((row) => row.employee.status === "running").length;

  const act = useCallback(
    async (id: string, work: () => Promise<unknown>) => {
      setBusy(id);
      try {
        await work();
        await reload();
      } catch (error) {
        showToast(errorText(error), { variant: "error" });
      } finally {
        setBusy(null);
      }
    },
    [reload, showToast],
  );

  /**
   * Run every employee that still has room today, one at a time.
   *
   * Sequential on purpose: nine agents starting together would open nine
   * sessions and spend nine times the tokens in the same minute, and the owner
   * pressing this button wants a round done, not a stampede.
   */
  const runAll = useCallback(async () => {
    setRunningAll(true);
    try {
      let started = 0;
      for (const row of runnable) {
        await api.runStaffEmployeeNow(row.employee.id);
        started += 1;
      }
      showToast(t("desk.todo.runAllDone", { count: started }), { variant: "success" });
      await reload();
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    } finally {
      setRunningAll(false);
    }
  }, [reload, runnable, showToast, t]);

  /**
   * Approve a draft without leaving the page.
   *
   * The complaint was that a finished draft said nothing about the step after
   * it. Settling it here puts the publish row in front of the owner the moment
   * the draft leaves, so the second step announces itself instead of being
   * discovered later on another page. The host keeps the body and stamps the
   * approval, so this is the same call the contents page makes.
   */
  const approve = (contentId: string, todoId: string) =>
    void act(todoId, () => api.setStaffContentStatus({ id: contentId, status: "approved" }));

  /**
   * The words of a card, which this page already holds for the boards.
   *
   * A row is a pointer -- id, title, where it came from -- and not a copy of
   * the card, because a page carrying twelve drafts goes stale in twelve
   * places. The body is looked up at the moment the row is unfolded.
   */
  const wordsOf = (contentId: string) =>
    snapshot.contents.find((item) => item.id === contentId)?.body ?? "";

  /**
   * Unfold a publish row: the words, so the owner can read them once more.
   *
   * The row has to answer "what am I about to say" without sending the owner
   * to the studio for a draft they have already read and approved, and without
   * a second copy of it living on this page.
   */
  const openWords = (todo: DeskTodo) => {
    const contentId = todo.contentId;
    if (!contentId) return;
    setEditing((current) =>
      current?.id === contentId ? null : { id: contentId, body: wordsOf(contentId) },
    );
  };

  /**
   * Publish a piece by handing it to the poster.
   *
   * Nothing here asks for an address, because the address belongs to the act of
   * speaking and the act of speaking is not the owner's: the second click moves
   * the piece into the poster's queue, and the poster reports back where it
   * went. The write carries the words as well, so what goes out is the version
   * the owner just read. Settling it here takes the row off the page, which is
   * how the owner knows their part is done.
   */
  const handOver = (todo: DeskTodo, body: string) => {
    const contentId = todo.contentId;
    if (!contentId) return;
    void act(todo.id, async () => {
      await api.setStaffContentStatus({ id: contentId, status: "queued", body });
      setEditing(null);
    });
  };

  /** The words onto the clipboard, for anyone posting this by hand. */
  const copyBody = useCallback(
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

  /** The way back to the post a row answers, when the desk knows where it is. */
  const sourceLink = (link?: string) =>
    link ? (
      <a className="lead-link" href={link} target="_blank" rel="noreferrer">
        <IconExternal size={12} aria-hidden />
        {t("desk.todo.openSource")}
      </a>
    ) : null;

  /**
   * The buttons on a row, which are the row's whole point.
   *
   * A draft gets three: read the post it answers, go edit it, or settle it on
   * the spot. A publish row gets four: its post, a look at the words, the way
   * to the studio for a rewrite, and the one click that hands the piece to the
   * poster. No address is asked for anywhere on this page, because the owner is
   * not the one who speaks.
   */
  const rowActions = (todo: DeskTodo) => {
    const contentId = todo.contentId;
    if (todo.kind === "approve" && contentId) {
      return (
        <div className="desk-row-actions">
          {sourceLink(todo.link)}
          <Button variant="ghost" size="sm" onClick={() => todo.page && setPage(todo.page)}>
            {t(TODO_TEXT[todo.kind].go)}
          </Button>
          <Button
            variant="primary"
            size="sm"
            disabled={busy === todo.id}
            onClick={() => approve(contentId, todo.id)}
          >
            <IconCheck size={13} aria-hidden />
            {t("desk.todo.approveNow")}
          </Button>
        </div>
      );
    }
    if (todo.kind === "publish" && contentId) {
      return (
        <div className="desk-row-actions">
          {sourceLink(todo.link)}
          <Button variant="ghost" size="sm" onClick={() => openWords(todo)}>
            {t("desk.todo.publishBody")}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => todo.page && setPage(todo.page)}>
            {t(TODO_TEXT[todo.kind].go)}
          </Button>
          <Button
            variant="primary"
            size="sm"
            disabled={busy === todo.id}
            onClick={() =>
              handOver(todo, editing?.id === contentId ? editing.body : wordsOf(contentId))
            }
          >
            <IconCheck size={13} aria-hidden />
            {t("desk.todo.publishNow")}
          </Button>
        </div>
      );
    }
    return (
      <div className="desk-row-actions">
        {sourceLink(todo.link)}
        <Button variant="secondary" size="sm" onClick={() => todo.page && setPage(todo.page)}>
          {t(TODO_TEXT[todo.kind].go)}
        </Button>
      </div>
    );
  };

  /**
   * The words, unfolded under their own row.
   *
   * The owner's last look before saying it: the text as it will go out,
   * editable, because a comma is not worth a trip to another page, and a copy
   * button for anyone who would rather post it themselves. The address field
   * that used to live here is gone: where a piece landed is the poster's line
   * to write, not the owner's.
   */
  const wordsPanel = (todo: DeskTodo) => {
    const contentId = todo.contentId;
    if (!contentId || !editing || editing.id !== contentId) return null;
    const draft = editing;
    return (
      <div className="desk-publish">
        <p className="staff-hint">{t("desk.todo.publishHint")}</p>
        <Textarea
          value={draft.body}
          rows={draftRows(draft.body)}
          onChange={(event) => setEditing({ ...draft, body: event.target.value })}
        />
        <div className="desk-row-actions">
          <Button variant="ghost" size="sm" onClick={() => void copyBody(draft.body)}>
            <IconCopy size={13} aria-hidden />
            {t("boards.contents.copy")}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => setEditing(null)}>
            {t("common.cancel")}
          </Button>
        </div>
      </div>
    );
  };

  /** A broken thing's one button: sign the site in, or go read the log. */
  const blockerActions = (blocker: DeskBlocker) => {
    const channel = blocker.channel;
    return (
      <div className="desk-row-actions">
        {channel ? (
          <Button
            variant="secondary"
            size="sm"
            disabled={busy === blocker.id}
            onClick={() => void act(blocker.id, () => api.signInStaffSite(channel))}
          >
            {t("desk.blocker.signIn")}
          </Button>
        ) : (
          <Button variant="secondary" size="sm" onClick={() => setPage("staff")}>
            {t("desk.blocker.openLog")}
          </Button>
        )}
      </div>
    );
  };

  return (
    <div className="thread-scroll">
      <div className="page-frame board-page">
        <BoardHeader
          title={t("desk.today.title")}
          subtitle={t("desk.today.subtitle")}
          actions={
            <>
              <Button variant="ghost" size="sm" onClick={() => void reload()}>
                <IconRefresh size={13} aria-hidden />
                {t("boards.refresh")}
              </Button>
              <Button
                variant="primary"
                size="sm"
                disabled={runningAll || runnable.length === 0}
                title={t("desk.todo.runAllHint", { count: runnable.length })}
                onClick={() => void runAll()}
              >
                <IconPlay size={13} aria-hidden />
                {t("desk.todo.runAll")}
              </Button>
            </>
          }
        />

        <div className="staff-strip">
          {KPI_FIELDS.map((field) => (
            <div className="staff-stat" key={field}>
              <b>{today ? today[field] : "-"}</b>
              <span>{t(KPI_TEXT[field])}</span>
            </div>
          ))}
        </div>

        <div className="desk-col">
          {blockers.map((blocker) => (
            <div className="desk-banner" key={blocker.id}>
              <span className="desk-tag desk-tag-bad">
                <IconTriangleAlert size={12} aria-hidden />
                {t(BLOCKER_TEXT[blocker.kind])}
              </span>
              <div className="desk-row-main">
                <div className="desk-row-title">{blocker.title}</div>
                <div className="desk-row-sub">{blocker.detail}</div>
              </div>
              {blockerActions(blocker)}
            </div>
          ))}

          <details className="desk-block desk-fold">
            <summary className="desk-fold-head">
              <IconChevronDown size={13} className="desk-fold-chevron" aria-hidden />
              <span className="staff-section-title">{t("desk.roster.title")}</span>
              <span className="staff-hint desk-block-hint">
                {t("desk.roster.on", { count: onDuty })} · {t("desk.roster.rounds", { count: rounds })}
              </span>
              {blockers.length === 0 ? (
                <span className="desk-chip-ok">{t("desk.roster.clear")}</span>
              ) : (
                <span className="staff-count" data-tone="bad">
                  {t("desk.roster.blocked", { count: blockers.length })}
                </span>
              )}
            </summary>

            <div className="desk-fold-body">
              <ul className="desk-roster">
                {roster.map((row) => (
                  <li className="desk-staff" key={row.employee.id}>
                    <span
                      className={
                        "staff-dot " +
                        (row.live ? "on" : row.employee.status === "running" ? "" : "warn")
                      }
                      aria-hidden
                    />
                    <span className="desk-staff-name">{row.employee.name}</span>
                    <span className="desk-staff-what">{row.live ?? t("desk.roster.waiting")}</span>
                    <span className="desk-staff-when">
                      {row.employee.nextRunAt
                        ? t("desk.roster.next", {
                            at: slotLabel(row.employee.nextRunAt, i18n.language),
                          })
                        : t("desk.roster.manual")}
                      {row.quota > 0
                        ? " · " + t("desk.roster.quota", { used: row.ranToday, total: row.quota })
                        : null}
                    </span>
                  </li>
                ))}
                {roster.length === 0 ? (
                  <li className="staff-hint">{t("desk.roster.empty")}</li>
                ) : null}
              </ul>

              <div className="desk-loops">
                <span>
                  <IconClock size={12} aria-hidden />
                  {t("desk.loop.dispatch", {
                    at: loops.dispatch ? slotLabel(loops.dispatch, i18n.language) : t("desk.loop.none"),
                  })}
                </span>
                <span>
                  <IconClock size={12} aria-hidden />
                  {t("desk.loop.watch", {
                    at: loops.watch ? slotLabel(loops.watch, i18n.language) : t("desk.loop.none"),
                  })}
                </span>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={busy === "watch"}
                  onClick={() => void act("watch", () => api.staffWatchRun())}
                >
                  {t("desk.loop.watchNow")}
                </Button>
              </div>

              <div className="desk-fold-part">
                <div className="desk-block-head">
                  <span className="staff-section-title">{t("desk.round.title")}</span>
                  {round ? (
                    <Badge>
                      {round.status === "running"
                        ? t("desk.round.running")
                        : round.status === "succeeded"
                          ? t("desk.round.done")
                          : t("desk.round.died")}
                    </Badge>
                  ) : null}
                </div>
                {!round ? (
                  <p className="staff-hint">{t("desk.round.none")}</p>
                ) : (
                  <>
                    <p className="staff-hint">
                      {t("desk.round.line", {
                        employee: round.employee,
                        at: slotLabel(round.startedAt, i18n.language),
                        steps: round.stepCount,
                      })}
                    </p>
                    <ol className="desk-steps">
                      {round.steps.map((step, index) => (
                        <li
                          className={
                            "desk-step" +
                            (index + 1 <= round.currentStep ? " on" : "") +
                            (round.status !== "running" && index + 1 > round.currentStep
                              ? " off"
                              : "")
                          }
                          key={step.title + index}
                        >
                          <span className="desk-step-name">{step.title}</span>
                          <span className="desk-step-said">
                            {step.result ?? t("desk.round.noResult")}
                          </span>
                        </li>
                      ))}
                    </ol>
                  </>
                )}
                <div className="desk-row-actions">
                  <Button variant="ghost" size="sm" onClick={() => setPage("pipeline")}>
                    {t("desk.round.openPipeline")}
                  </Button>
                </div>
              </div>
            </div>
          </details>

          <section className="desk-block">
            <div className="desk-block-head">
              <span className="staff-section-title">{t("desk.todo.title")}</span>
              {todos.length > 0 ? <span className="staff-count">{todos.length}</span> : null}
              <span className="staff-hint desk-block-hint">{t("desk.todo.hint")}</span>
            </div>
            {!loaded ? (
              <p className="staff-hint">{t("desk.loading")}</p>
            ) : todos.length === 0 ? (
              <p className="staff-hint">{t("desk.todo.none")}</p>
            ) : (
              <ul className="desk-list">
                {todos.map((todo) => (
                  <li className="desk-entry" key={todo.id} data-todo={todo.kind}>
                    <div className="desk-row">
                      <span className="desk-tag">{t(TODO_TEXT[todo.kind].tag)}</span>
                      <div className="desk-row-main">
                        <div className="desk-row-title">{todo.title}</div>
                        <div className="desk-row-sub">{todo.detail}</div>
                      </div>
                      {todo.kind === "advice" && todo.suggestionId ? (
                        <div className="desk-row-actions">
                          <Button
                            variant="primary"
                            size="sm"
                            disabled={busy === todo.id}
                            onClick={() =>
                              void act(todo.id, () =>
                                api.decideStaffAdvice({
                                  id: todo.suggestionId!,
                                  decision: "accepted",
                                }),
                              )
                            }
                          >
                            <IconCheck size={13} aria-hidden />
                            {t("boards.advice.accept")}
                          </Button>
                          <Button
                            variant="ghost"
                            size="sm"
                            disabled={busy === todo.id}
                            onClick={() =>
                              void act(todo.id, () =>
                                api.decideStaffAdvice({
                                  id: todo.suggestionId!,
                                  decision: "rejected",
                                }),
                              )
                            }
                          >
                            <IconX size={13} aria-hidden />
                            {t("boards.advice.reject")}
                          </Button>
                        </div>
                      ) : (
                        rowActions(todo)
                      )}
                    </div>
                    {wordsPanel(todo)}
                  </li>
                ))}
              </ul>
            )}
          </section>

        </div>
      </div>
    </div>
  );
}
