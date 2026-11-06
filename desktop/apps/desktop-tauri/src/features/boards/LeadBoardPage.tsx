/**
 * The business board: every opportunity somebody said out loud, in columns.
 *
 * The card is the quote. A lead exists because a person wrote a sentence we
 * can answer, so the sentence is what the board shows first and the score is
 * filing -- an owner who reads the words can disagree with the number, and
 * that disagreement is the point.
 *
 * Moving a card is the owner's alone. The employees file cards and the board
 * says which ones nobody has touched; deciding that a card is worth pursuing
 * is a human judgement, and the column is where it is recorded.
 *
 * The one thing the employees cannot do at all is hear the answer. A private
 * message, a comment, a name added on a chat app -- none of that reaches this
 * machine, so the card that matters most, the one where a stranger finally
 * said something back, is the card nobody can file. "Record a reply" is that
 * door: the owner pastes the sentence and the board holds it, after which the
 * receptionist and the closer are ordinary employees working an ordinary card.
 * A board that only ever gains cards from sweeps is a board where the closest
 * thing to a sale never appears.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { StaffLead, StaffLeadStage } from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { useAppStore } from "../../stores/app-store";
import { Badge, Button, Field, Input, Textarea } from "../../components/ui";
import { IconCheck, IconExternal, IconPlus, IconRefresh, IconSparkles } from "../../components/icons";
import { AdviceRail } from "./AdviceRail";
import { ManagerDeck } from "./ManagerDeck";
import { BoardHeader, useBoardAdvice } from "./useBoardAdvice";
import { LEAD_COLUMNS, LEAD_OPEN_COLUMNS, agoLabel, leadStageKey, number } from "./board-format";
import { errorText } from "../staff/staff-format";

/** Cards beyond this per column are counted, not drawn. */
const COLUMN_LIMIT = 30;

/** Where a reply with no source typed in is filed. */
const MANUAL_SITE = "手动录入";

const EMPTY_REPLY = { title: "", quote: "", url: "", author: "", site: "" };

export function LeadBoardPage() {
  const { t } = useTranslation();
  const showToast = useAppStore((s) => s.showToast);
  const [leads, setLeads] = useState<StaffLead[]>([]);
  const [counts, setCounts] = useState<Record<string, number>>({});
  const [selected, setSelected] = useState<string | null>(null);
  const [record, setRecord] = useState<typeof EMPTY_REPLY | null>(null);
  const [wonFor, setWonFor] = useState<{ id: string; note: string } | null>(null);
  const [busy, setBusy] = useState(false);

  const advice = useBoardAdvice("leads", (message) => showToast(message, { variant: "error" }));

  const load = useCallback(async () => {
    try {
      const answer = await api.listStaffLeads({ limit: 500 });
      setLeads(answer.leads ?? []);
      setCounts(answer.counts ?? {});
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    }
  }, [showToast]);

  useEffect(() => {
    void load();
    const off = api.onStaffChanged(() => void load());
    return off;
  }, [load]);

  const byStage = useMemo(() => {
    const grouped = new Map<string, StaffLead[]>();
    for (const stage of LEAD_COLUMNS) grouped.set(stage, []);
    for (const lead of leads) {
      const bucket = grouped.get(lead.stage);
      if (bucket) bucket.push(lead);
    }
    return grouped;
  }, [leads]);

  const open = LEAD_OPEN_COLUMNS.reduce((total, stage) => total + (counts[stage] ?? 0), 0);

  // The pipeline plus its one win column, and the two dead ends only once
  // something is actually in them: eight columns cannot share a page width
  // without each becoming too narrow to read a headline in.
  const boardStages = useMemo<StaffLeadStage[]>(
    () => [
      ...LEAD_OPEN_COLUMNS,
      "won",
      ...(["lost", "dropped"] as StaffLeadStage[]).filter((stage) => (counts[stage] ?? 0) > 0),
    ],
    [counts],
  );

  const move = useCallback(
    async (lead: StaffLead, stage: StaffLeadStage, note?: string) => {
      setBusy(true);
      try {
        const answer = await api.moveStaffLead({ id: lead.id, stage, note });
        setLeads((current) => current.map((item) => (item.id === lead.id ? answer.lead : item)));
        setCounts((current) => ({
          ...current,
          [lead.stage]: Math.max(0, (current[lead.stage] ?? 1) - 1),
          [stage]: (current[stage] ?? 0) + 1,
        }));
        setWonFor(null);
      } catch (error) {
        showToast(errorText(error), { variant: "error" });
      } finally {
        setBusy(false);
      }
    },
    [showToast],
  );

  /**
   * File the answer a person actually gave.
   *
   * A reply about a thread already on the board is not a second card: it is
   * the same person moving, so the existing card is moved instead and the
   * sentence is kept in its note. Anything else becomes a card of its own, in
   * "has replied", because a stranger who spoke to us has already cleared the
   * bar every other column is a guess about.
   */
  const saveReply = useCallback(async () => {
    if (!record) return;
    const quote = record.quote.trim();
    if (!quote) {
      showToast(t("boards.leads.recordNeedsQuote"), { variant: "error" });
      return;
    }
    const url = record.url.trim();
    const site = record.site.trim() || t("boards.leads.recordSiteDefault");
    const title = record.title.trim() || quote.slice(0, 30);
    const author = record.author.trim();
    setBusy(true);
    try {
      const known = url ? leads.find((lead) => lead.url === url) : undefined;
      if (known && LEAD_OPEN_COLUMNS.includes(known.stage) && known.stage !== "replied") {
        const note = [known.note, t("boards.leads.recordAppend", { quote })].filter(Boolean).join("\n");
        const answer = await api.moveStaffLead({ id: known.id, stage: "replied", note });
        setLeads((current) => current.map((item) => (item.id === known.id ? answer.lead : item)));
        setCounts((current) => ({
          ...current,
          [known.stage]: Math.max(0, (current[known.stage] ?? 1) - 1),
          replied: (current.replied ?? 0) + 1,
        }));
        showToast(t("boards.leads.recordMerged"), { variant: "success" });
      } else {
        const answer = await api.saveStaffLead({
          title,
          quote,
          url,
          author,
          site,
          topic: title,
          stage: "replied",
          replied: true,
          score: 60,
          tags: [t("boards.leads.recordTag")],
        });
        setLeads((current) => [answer.lead, ...current]);
        setCounts((current) => ({ ...current, replied: (current.replied ?? 0) + 1 }));
        showToast(t("boards.leads.recordSaved"), { variant: "success" });
      }
      setRecord(null);
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    } finally {
      setBusy(false);
    }
  }, [leads, record, showToast, t]);

  return (
    <div className="thread-scroll">
      <div className="page-frame board-page">
        <BoardHeader
          title={t("boards.leads.title")}
          subtitle={t("boards.leads.subtitle")}
          actions={
            <>
              <Button variant="ghost" size="sm" onClick={() => void load()}>
                <IconRefresh size={13} aria-hidden />
                {t("boards.refresh")}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setRecord(record ? null : { ...EMPTY_REPLY })}
              >
                <IconPlus size={13} aria-hidden />
                {t("boards.leads.record")}
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

        <ManagerDeck board="leads" />

        {record ? (
          <section className="lead-intake">
            <div className="staff-section-title">{t("boards.leads.recordTitle")}</div>
            <p className="staff-hint">{t("boards.leads.recordHint")}</p>
            <div className="lead-intake-grid">
              <Field label={t("boards.leads.recordQuote")}>
                <Textarea
                  value={record.quote}
                  rows={3}
                  placeholder={t("boards.leads.recordQuotePlaceholder")}
                  onChange={(event) => setRecord({ ...record, quote: event.target.value })}
                />
              </Field>
              <Field label={t("boards.leads.recordUrl")}>
                <Input
                  value={record.url}
                  placeholder={t("boards.leads.recordUrlPlaceholder")}
                  onChange={(event) => setRecord({ ...record, url: event.target.value })}
                />
              </Field>
              <Field label={t("boards.leads.recordAuthor")}>
                <Input
                  value={record.author}
                  placeholder={t("boards.leads.recordAuthorPlaceholder")}
                  onChange={(event) => setRecord({ ...record, author: event.target.value })}
                />
              </Field>
              <Field label={t("boards.leads.recordSite")}>
                <Input
                  value={record.site}
                  placeholder={t("boards.leads.recordSiteDefault")}
                  onChange={(event) => setRecord({ ...record, site: event.target.value })}
                />
              </Field>
            </div>
            <div className="draft-actions">
              <Button variant="primary" size="sm" disabled={busy} onClick={() => void saveReply()}>
                <IconCheck size={13} aria-hidden />
                {t("boards.leads.recordSave")}
              </Button>
              <Button variant="ghost" size="sm" onClick={() => setRecord(null)}>
                {t("common.cancel")}
              </Button>
            </div>
          </section>
        ) : null}

        <div className="staff-strip">
          <div className="staff-stat">
            <b>{number(open)}</b>
            <span>{t("boards.leads.open")}</span>
          </div>
          <div className="staff-stat">
            <b>{number(counts.replied ?? 0)}</b>
            <span>{t("boards.leads.waiting")}</span>
          </div>
          <div className="staff-stat">
            <b>{number(counts.won ?? 0)}</b>
            <span>{t("boards.leads.won")}</span>
          </div>
          <div className="staff-stat">
            <b>{number(leads.length)}</b>
            <span>{t("boards.leads.all")}</span>
          </div>
        </div>

        <div className="board-split">
          <div className="board-columns">
            {boardStages.map((stage) => {
              const cards = byStage.get(stage) ?? [];
              return (
                <section className="board-column" key={stage} data-stage={stage}>
                  <header className="board-column-head">
                    <span>{t(leadStageKey(stage))}</span>
                    <span className="staff-count">{cards.length}</span>
                  </header>
                  {cards.slice(0, COLUMN_LIMIT).map((lead) => (
                    <article
                      key={lead.id}
                      className={"lead-card" + (selected === lead.id ? " on" : "")}
                      data-lead={lead.id}
                      onClick={() => setSelected(selected === lead.id ? null : lead.id)}
                    >
                      <div className="lead-card-top">
                        <span className="lead-site">{lead.site || t("boards.leads.unknownSite")}</span>
                        {lead.score > 0 ? <Badge>{t("boards.leads.score", { score: lead.score })}</Badge> : null}
                      </div>
                      <h3 className="lead-title">{lead.title}</h3>
                      {lead.quote ? <q className="lead-quote">{lead.quote}</q> : null}
                      <div className="lead-foot">
                        <span>{lead.author || t("boards.leads.unknownAuthor")}</span>
                        <span>{agoLabel(lead.createdAt, t)}</span>
                      </div>
                      {selected === lead.id ? (
                        <div className="lead-detail" onClick={(event) => event.stopPropagation()}>
                          {lead.angle ? <p className="lead-angle">{lead.angle}</p> : null}
                          {lead.url ? (
                            <a className="lead-link" href={lead.url} target="_blank" rel="noreferrer">
                              <IconExternal size={12} aria-hidden />
                              {t("boards.leads.openSource")}
                            </a>
                          ) : null}
                          {/* Where the card came from. A reply the watch read back
                              and one the owner typed in look alike otherwise. */}
                          {lead.tags.length > 0 ? (
                            <div className="lead-tags">
                              {lead.tags.map((tag) => (
                                <Badge key={tag}>{tag}</Badge>
                              ))}
                            </div>
                          ) : null}
                          <div className="lead-moves">
                            {LEAD_COLUMNS.filter((target) => target !== stage).map((target) => (
                              <Button
                                key={target}
                                variant="ghost"
                                size="sm"
                                disabled={busy}
                                onClick={() =>
                                  target === "won"
                                    ? setWonFor({ id: lead.id, note: lead.note })
                                    : void move(lead, target)
                                }
                              >
                                {t(leadStageKey(target))}
                              </Button>
                            ))}
                          </div>
                          {wonFor?.id === lead.id ? (
                            <div className="lead-won">
                              <p className="staff-hint">{t("boards.leads.wonHint")}</p>
                              <Field label={t("boards.leads.wonNote")}>
                                <Input
                                  value={wonFor.note}
                                  placeholder={t("boards.leads.wonNotePlaceholder")}
                                  onChange={(event) =>
                                    setWonFor({ id: lead.id, note: event.target.value })
                                  }
                                />
                              </Field>
                              <div className="draft-actions">
                                <Button
                                  variant="primary"
                                  size="sm"
                                  disabled={busy}
                                  onClick={() => void move(lead, "won", wonFor.note)}
                                >
                                  <IconCheck size={13} aria-hidden />
                                  {t("boards.leads.wonConfirm")}
                                </Button>
                                <Button variant="ghost" size="sm" onClick={() => setWonFor(null)}>
                                  {t("common.cancel")}
                                </Button>
                              </div>
                            </div>
                          ) : null}
                        </div>
                      ) : null}
                    </article>
                  ))}
                  {cards.length > COLUMN_LIMIT ? (
                    <p className="staff-hint">
                      {t("boards.more", { count: cards.length - COLUMN_LIMIT })}
                    </p>
                  ) : null}
                  {cards.length === 0 ? <p className="staff-hint">{t("boards.leads.empty")}</p> : null}
                </section>
              );
            })}
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
