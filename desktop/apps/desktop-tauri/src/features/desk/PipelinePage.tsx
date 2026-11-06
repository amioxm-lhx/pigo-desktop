/**
 * The pipeline: one round of the business, from the sweep to the delivery.
 *
 * Ten stations, drawn in the order work travels, each with the one number that
 * says whether it is moving. The colour on a card is the whole point of the
 * page: a lit card is one the owner has to act on, a red one is broken, and a
 * quiet one is an employee doing its job without needing anybody. That is the
 * sentence the owner could not get from four separate boards.
 *
 * Clicking a station opens the rows behind its number, so "6 篇稿子" becomes the
 * six titles, who each one is for and how long it has waited. The page never
 * shows a count it cannot open.
 *
 * The window at the top is the same ten stations cut three ways -- today, the
 * week, everything -- because a board that only ever shows all of history
 * cannot answer "is today going well".
 */
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../../stores/app-store";
import { Badge, Button, Select } from "../../components/ui";
import { IconExternal, IconRefresh } from "../../components/icons";
import { BoardHeader } from "../boards/useBoardAdvice";
import { runStatusLabel, whenLabel } from "../staff/staff-format";
import { useDesk } from "./useDesk";
import {
  DESK_STATIONS,
  STATION_TEXT,
  buildStations,
  latestRun,
  loopProgress,
  localDayStart,
  runToLine,
  slotLabel,
  type DeskStation,
  type DeskStationId,
} from "./desk-model";

/** The three cuts of the same ten stations. */
type Window = "today" | "week" | "all";

const DAY = 86_400_000;

export function PipelinePage() {
  const { t, i18n } = useTranslation();
  const setPage = useAppStore((s) => s.setPage);
  const { snapshot, reload, loaded } = useDesk();
  const [window, setWindow] = useState<Window>("today");
  const [selected, setSelected] = useState<DeskStationId>("approve");
  const [replayId, setReplayId] = useState<string>("");

  const now = Date.now();
  const since = window === "today" ? localDayStart(now) : window === "week" ? now - 7 * DAY : undefined;
  const stations = useMemo(
    () => buildStations(snapshot, t, now, since),
    // `now` is deliberately not a dependency: it only moves the window, and
    // re-cutting the list every render would fight the reader's selection.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [snapshot, t, since],
  );
  const progress = useMemo(() => loopProgress(stations), [stations]);
  const current = stations.find((station) => station.id === selected) ?? stations[0];

  const recentRuns = useMemo(
    () =>
      [...snapshot.runs]
        .sort((a, b) => Date.parse(b.startedAt) - Date.parse(a.startedAt))
        .slice(0, 30),
    [snapshot.runs],
  );
  const replayed = useMemo(() => {
    const run = recentRuns.find((entry) => entry.id === replayId);
    return run ? runToLine(run, snapshot.employees) : latestRun(snapshot);
  }, [recentRuns, replayId, snapshot]);

  return (
    <div className="thread-scroll">
      <div className="page-frame board-page">
        <BoardHeader
          title={t("desk.pipeline.title")}
          subtitle={t("desk.pipeline.subtitle")}
          actions={
            <>
              <Button
                variant={window === "today" ? "secondary" : "ghost"}
                size="sm"
                onClick={() => setWindow("today")}
              >
                {t("desk.pipeline.today")}
              </Button>
              <Button
                variant={window === "week" ? "secondary" : "ghost"}
                size="sm"
                onClick={() => setWindow("week")}
              >
                {t("desk.pipeline.week")}
              </Button>
              <Button
                variant={window === "all" ? "secondary" : "ghost"}
                size="sm"
                onClick={() => setWindow("all")}
              >
                {t("desk.pipeline.all")}
              </Button>
              <Button variant="ghost" size="sm" onClick={() => void reload()}>
                <IconRefresh size={13} aria-hidden />
                {t("boards.refresh")}
              </Button>
            </>
          }
        />

        <div className="desk-legend">
          <span className="desk-key desk-key-owner">{t("desk.pipeline.legendOwner")}</span>
          <span className="desk-key desk-key-machine">{t("desk.pipeline.legendMachine")}</span>
          <span className="desk-key desk-key-stuck">{t("desk.pipeline.legendStuck")}</span>
          <span className="staff-hint desk-legend-hint">{t("desk.pipeline.legendHint")}</span>
        </div>

        <div className="desk-progress">
          <div className="desk-progress-track" role="img" aria-label={t("desk.pipeline.progressLabel")}>
            {stations.map((station) => (
              <span
                key={station.id}
                className={"desk-progress-cell state-" + station.state}
                data-filled={station.count > 0 ? "yes" : "no"}
                title={t(STATION_TEXT[station.id].name) + " · " + station.count}
              />
            ))}
          </div>
          <p className="staff-hint">
            {t("desk.pipeline.progressLine", {
              filled: progress.filled,
              total: progress.total,
              furthest: progress.furthest ? t(STATION_TEXT[progress.furthest].name) : t("desk.pipeline.nowhere"),
            })}
            {progress.waiting
              ? " · " +
                t("desk.pipeline.progressWaiting", {
                  station: t(STATION_TEXT[progress.waiting].name),
                })
              : ""}
            {progress.stuck
              ? " · " + t("desk.pipeline.progressStuck", { station: t(STATION_TEXT[progress.stuck].name) })
              : ""}
          </p>
        </div>

        <div className="desk-stations">
          {stations.map((station) => (
            <StationCard
              key={station.id}
              station={station}
              active={station.id === current?.id}
              locale={i18n.language}
              onSelect={() => setSelected(station.id)}
            />
          ))}
        </div>

        {current ? (
          <section className="desk-block">
            <div className="desk-block-head">
              <span className="staff-section-title">{t(STATION_TEXT[current.id].name)}</span>
              <span className="staff-count">{current.count}</span>
              <span className="staff-hint desk-block-hint">
                {t(STATION_TEXT[current.id].out)} · {t(STATION_TEXT[current.id].who)}
              </span>
              {current.id === "approve" || current.id === "publish" ? (
                <Button variant="secondary" size="sm" onClick={() => setPage("contents")}>
                  {t("desk.pipeline.goContents")}
                </Button>
              ) : null}
              {(current.id === "talk" || current.id === "close" || current.id === "deliver") ? (
                <Button variant="secondary" size="sm" onClick={() => setPage("leads")}>
                  {t("desk.pipeline.goLeads")}
                </Button>
              ) : null}
            </div>

            {current.note ? (
              <p className="staff-hint staff-hint-warn">
                {t(current.note.key, { name: current.note.value })}
              </p>
            ) : null}

            {current.rows.length === 0 ? (
              <p className="staff-hint">{t("desk.pipeline.empty")}</p>
            ) : (
              <table className="desk-table">
                <thead>
                  <tr>
                    {STATION_TEXT[current.id].columns.map((column) => (
                      <th key={column}>{t(column)}</th>
                    ))}
                    <th aria-label={t("desk.col.open")} />
                  </tr>
                </thead>
                <tbody>
                  {current.rows.map((row) => (
                    <tr key={row.id}>
                      {row.cells.map((cell, index) => (
                        <td key={index}>{cell}</td>
                      ))}
                      <td className="desk-table-open">
                        {row.link ? (
                          <a className="lead-link" href={row.link} target="_blank" rel="noreferrer">
                            <IconExternal size={12} aria-hidden />
                          </a>
                        ) : null}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
            {current.hidden > 0 ? (
              <p className="staff-hint">{t("boards.more", { count: current.hidden })}</p>
            ) : null}
          </section>
        ) : null}

        <section className="desk-block">
          <div className="desk-block-head">
            <span className="staff-section-title">{t("desk.replay.title")}</span>
            <Select
              className="desk-replay-pick"
              value={replayId}
              onChange={(event) => setReplayId(event.target.value)}
            >
              <option value="">{t("desk.replay.latest")}</option>
              {recentRuns.map((run) => (
                <option key={run.id} value={run.id}>
                  {whenLabel(run.startedAt, i18n.language)} · {runStatusLabel(run.status, t)} ·{" "}
                  {run.id.slice(0, 6)}
                </option>
              ))}
            </Select>
          </div>
          {!replayed ? (
            <p className="staff-hint">{loaded ? t("desk.replay.none") : t("desk.loading")}</p>
          ) : (
            <>
              <p className="staff-hint">
                {t("desk.round.line", {
                  employee: replayed.employee,
                  at: slotLabel(replayed.startedAt, i18n.language),
                  steps: replayed.stepCount,
                })}
                {" · "}
                <Badge>{runStatusLabel(replayed.status, t)}</Badge>
                {replayed.errorCode ? " · " + replayed.errorCode : ""}
              </p>
              <ol className="desk-steps">
                {replayed.steps.map((step, index) => (
                  <li
                    className={"desk-step" + (index + 1 <= replayed.currentStep ? " on" : " off")}
                    key={step.title + index}
                  >
                    <span className="desk-step-name">{step.title}</span>
                    <span className="desk-step-said">{step.result ?? t("desk.round.noResult")}</span>
                    {step.report ? (
                      <details className="desk-step-report">
                        <summary>{t("desk.replay.full")}</summary>
                        <pre>{step.report}</pre>
                      </details>
                    ) : null}
                  </li>
                ))}
              </ol>
            </>
          )}
        </section>
      </div>
    </div>
  );
}

/** One station, with the number that matters and how it is lit. */
function StationCard({
  station,
  active,
  locale,
  onSelect,
}: {
  station: DeskStation;
  active: boolean;
  locale: string;
  onSelect: () => void;
}) {
  const { t } = useTranslation();
  const text = STATION_TEXT[station.id];
  return (
    <button
      type="button"
      className={"desk-station state-" + station.state + (active ? " on" : "")}
      data-station={station.id}
      onClick={onSelect}
    >
      <span className="desk-station-head">
        <span className="desk-station-name">{t(text.name)}</span>
        {station.state === "owner" ? (
          <span className="desk-station-you">{t("desk.who.you")}</span>
        ) : null}
      </span>
      <span className="desk-station-who">{t(text.who)}</span>
      <span className="desk-station-count">{station.count}</span>
      <span className="desk-station-out">{t(text.out)}</span>
      <span className="desk-station-at">
        {station.employee?.nextRunAt
          ? t("desk.roster.next", { at: slotLabel(station.employee.nextRunAt, locale) })
          : station.at
            ? t("desk.station.last", { ago: slotLabel(station.at, locale) })
            : t("desk.station.never")}
      </span>
      {station.note ? (
        <span className="desk-station-note">{t(station.note.key, { name: station.note.value })}</span>
      ) : null}
    </button>
  );
}