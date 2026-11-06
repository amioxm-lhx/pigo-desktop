/**
 * Data: what the loop produced, and what can be let go.
 *
 * The table is the answer to "where did all this come from", read one stage at
 * a time. Two things about it are deliberate. Sizes are the host's real byte
 * counts rather than a guess, because the owner confirms a cleanup against
 * this column. And a stage that keeps nothing -- the pages a sweep reads --
 * says so in words instead of showing a zero, because a zero reads like a
 * fault and this is the one stage where keeping nothing is the feature.
 *
 * The cleanup itself is a two-step: the host measures the rules first and the
 * dialog repeats those numbers back, so pressing the button is never a leap.
 * Nothing in this app deletes on a timer; a rule runs when it is picked, and
 * every rule refuses the rows something downstream still points at.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { StaffDataStats } from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { useAppStore } from "../../stores/app-store";
import { Badge, Button } from "../../components/ui";
import { IconCheck, IconDownload, IconRefresh, IconTrash, IconX } from "../../components/icons";
import { BoardHeader } from "../boards/useBoardAdvice";
import { errorText, whenLabel } from "../staff/staff-format";
import { useDesk } from "./useDesk";
import { sizeLabel } from "./desk-model";
import {
  LEDGER_TEXT,
  PRUNE_RULES,
  PRUNE_TEXT,
  buildReferences,
  brokenLink,
  ledgerRows,
  ruleOf,
  teamFootprint,
  type DeskPruneRule,
} from "./desk-ledger";

export function DataPage() {
  const { t, i18n } = useTranslation();
  const showToast = useAppStore((s) => s.showToast);
  const { snapshot, reload, loaded } = useDesk();
  const [stats, setStats] = useState<StaffDataStats | null>(null);
  const [cleaning, setCleaning] = useState(false);
  const [picked, setPicked] = useState<DeskPruneRule[]>([]);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setStats(await api.staffDataStats());
    } catch {
      setStats(null);
    }
  }, []);

  useEffect(() => {
    void load();
    const off = api.onStaffChanged(() => void load());
    return off;
  }, [load]);

  const rows = useMemo(() => ledgerRows(stats, snapshot), [stats, snapshot]);
  const references = useMemo(() => buildReferences(snapshot), [snapshot]);
  const gap = useMemo(() => brokenLink(snapshot), [snapshot]);
  const teamBytes = teamFootprint(rows);
  const restBytes = Math.max(0, (stats?.database.bytes ?? 0) - teamBytes);
  const pickedRows = rows.filter((row) => {
    const rule = ruleOf(row.id);
    return rule !== null && picked.includes(rule);
  });
  const pickedCount = pickedRows.reduce((total, row) => total + row.cleanable, 0);
  const pickedBytes = pickedRows.reduce((total, row) => total + row.cleanableBytes, 0);

  /**
   * Write the ledger out, because "what do I actually have" deserves an answer
   * that leaves.
   *
   * The host asks for the file's name and folder and says where it went: a
   * download link in the page can do neither, so the export either landed in a
   * folder nobody was told about or did not land at all.
   */
  const exportLedger = useCallback(async () => {
    const payload = {
      generatedAt: new Date().toISOString(),
      databaseBytes: stats?.database.bytes ?? 0,
      teamBytes,
      stages: rows.map((row) => ({
        id: row.id,
        rows: row.rows,
        bytes: row.bytes,
        parkedRows: row.parkedRows,
        parkedBytes: row.parkedBytes,
        cleanable: row.cleanable,
        cleanableBytes: row.cleanableBytes,
      })),
      references,
    };
    try {
      const answer = await api.exportStaffData({
        name: "pi-agent-data-" + new Date().toISOString().slice(0, 10) + ".json",
        body: JSON.stringify(payload, null, 2),
      });
      // Closing the save dialog is an answer, not a failure, so it is silent.
      if (answer.saved && answer.path) {
        showToast(t("desk.data.exported", { path: answer.path }), { variant: "success" });
      }
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    }
  }, [references, rows, showToast, stats, t, teamBytes]);

  const runPrune = useCallback(async () => {
    if (picked.length === 0) return;
    setBusy(true);
    try {
      const answer = await api.pruneStaffData({ rules: picked, dryRun: false });
      const taken = answer.taken.reduce((total, entry) => total + entry.rows, 0);
      showToast(t("desk.prune.done", { count: taken }), { variant: "success" });
      setPicked([]);
      setCleaning(false);
      await load();
      await reload();
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    } finally {
      setBusy(false);
    }
  }, [load, picked, reload, showToast, t]);

  return (
    <div className="thread-scroll">
      <div className="page-frame board-page">
        <BoardHeader
          title={t("desk.data.title")}
          subtitle={t("desk.data.subtitle")}
          actions={
            <>
              <Button variant="ghost" size="sm" onClick={() => void exportLedger()}>
                <IconDownload size={13} aria-hidden />
                {t("desk.data.export")}
              </Button>
              <Button variant="ghost" size="sm" onClick={() => void load()}>
                <IconRefresh size={13} aria-hidden />
                {t("boards.refresh")}
              </Button>
              <Button
                variant="secondary"
                size="sm"
                disabled={!stats}
                onClick={() => {
                  setPicked(
                    rows
                      .map((row) => ruleOf(row.id))
                      .filter((rule): rule is DeskPruneRule => rule !== null),
                  );
                  setCleaning(true);
                }}
              >
                <IconTrash size={13} aria-hidden />
                {t("desk.data.clean")}
              </Button>
            </>
          }
        />

        <section className="desk-block">
          <div className="desk-block-head">
            <span className="staff-section-title">{t("desk.data.byStage")}</span>
            <span className="staff-hint desk-block-hint">{t("desk.data.byStageHint")}</span>
          </div>
          {rows.length === 0 ? (
            <p className="staff-hint">{loaded ? t("desk.data.empty") : t("desk.loading")}</p>
          ) : (
            <table className="desk-table desk-table-wide">
              <thead>
                <tr>
                  <th>{t("desk.data.colStage")}</th>
                  <th>{t("desk.data.colProduce")}</th>
                  <th className="desk-num">{t("desk.data.colRows")}</th>
                  <th className="desk-num">{t("desk.data.colBytes")}</th>
                  <th>{t("desk.data.colAt")}</th>
                  <th>{t("desk.data.colUsedBy")}</th>
                  <th>{t("desk.data.colRule")}</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => (
                  <tr key={row.id} data-stage={row.id}>
                    <td>{t(LEDGER_TEXT[row.id].name)}</td>
                    <td className="desk-dim">
                      {t(LEDGER_TEXT[row.id].produce)}
                      {row.ephemeral ? (
                        <span className="desk-tag desk-tag-inline">{t("desk.data.notKept")}</span>
                      ) : null}
                      {row.parkedRows > 0 ? (
                        <span className="desk-parked">
                          {t("desk.data.parked", {
                            count: row.parkedRows,
                            size: sizeLabel(row.parkedBytes, t),
                          })}
                        </span>
                      ) : null}
                    </td>
                    <td className="desk-num">{row.rows}</td>
                    <td className="desk-num">{row.ephemeral ? "-" : sizeLabel(row.bytes, t)}</td>
                    <td className="desk-dim">
                      {row.at ? whenLabel(row.at, i18n.language) : t("desk.data.never")}
                    </td>
                    <td className="desk-dim">{t(LEDGER_TEXT[row.id].usedBy)}</td>
                    <td className="desk-dim">
                      {t(LEDGER_TEXT[row.id].rule)}
                      {row.cleanable > 0 ? (
                        <Badge tone="warning">
                          {t("desk.data.takeable", { count: row.cleanable })}
                        </Badge>
                      ) : null}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>

        <div className="desk-split">
          <section className="desk-block">
            <div className="desk-block-head">
              <span className="staff-section-title">{t("desk.data.refs")}</span>
              <span className="staff-hint desk-block-hint">{t("desk.data.refsHint")}</span>
            </div>
            <ul className="desk-refs">
              {references.map((reference) => (
                <li key={reference.id}>
                  <span className="desk-ref-name">
                    {t(LEDGER_TEXT[reference.from].name)} → {t(LEDGER_TEXT[reference.to].name)}
                  </span>
                  <span className="desk-ref-count">{reference.count}</span>
                </li>
              ))}
            </ul>
            {gap ? (
              <p className="staff-hint staff-hint-warn">
                {t(gap.key, gap.values)}
                {" · "}
                {t("desk.data.breakFix")}
              </p>
            ) : (
              <p className="staff-hint">{t("desk.data.breakNone")}</p>
            )}
          </section>

          <section className="desk-block">
            <div className="desk-block-head">
              <span className="staff-section-title">{t("desk.data.truth")}</span>
            </div>
            <p className="staff-hint">
              {t("desk.data.truthTeam", { size: sizeLabel(teamBytes, t) })}
            </p>
            <p className="staff-hint">
              {t("desk.data.truthDb", {
                size: sizeLabel(stats?.database.bytes ?? 0, t),
                rest: sizeLabel(restBytes, t),
              })}
            </p>
            <p className="staff-hint">{t("desk.data.truthWhy")}</p>
          </section>
        </div>

        {cleaning ? (
          <section className="desk-block desk-dialog">
            <div className="desk-block-head">
              <span className="staff-section-title">{t("desk.prune.title")}</span>
              <span className="staff-hint desk-block-hint">{t("desk.prune.hint")}</span>
              <Button variant="ghost" size="sm" onClick={() => setCleaning(false)}>
                <IconX size={13} aria-hidden />
                {t("common.cancel")}
              </Button>
            </div>
            <ul className="desk-rules">
              {PRUNE_RULES.map((rule) => {
                const stage = rows.find((row) => ruleOf(row.id) === rule);
                const takeable = stage?.cleanable ?? 0;
                return (
                  <li className="desk-rule" key={rule} data-rule={rule}>
                    <label className="desk-rule-pick">
                      <input
                        type="checkbox"
                        checked={picked.includes(rule)}
                        onChange={(event) =>
                          setPicked((current) =>
                            event.target.checked
                              ? [...current, rule]
                              : current.filter((entry) => entry !== rule),
                          )
                        }
                      />
                      <span className="desk-rule-name">{t(PRUNE_TEXT[rule].label)}</span>
                    </label>
                    <span className="desk-rule-hint">{t(PRUNE_TEXT[rule].hint)}</span>
                    <span className="desk-rule-count">
                      {takeable > 0
                        ? t("desk.prune.takeable", {
                            count: takeable,
                            size: sizeLabel(stage?.cleanableBytes ?? 0, t),
                          })
                        : t("desk.prune.nothing")}
                    </span>
                  </li>
                );
              })}
            </ul>
            <div className="desk-row-actions">
              <span className="staff-hint">
                {t("desk.prune.total", {
                  count: pickedCount,
                  size: sizeLabel(pickedBytes, t),
                })}
              </span>
              <Button
                variant="primary"
                size="sm"
                disabled={busy || picked.length === 0}
                onClick={() => void runPrune()}
              >
                <IconCheck size={13} aria-hidden />
                {t("desk.prune.confirm")}
              </Button>
            </div>
          </section>
        ) : null}
      </div>
    </div>
  );
}