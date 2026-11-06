/**
 * The proposal and its consequence, drawn the same way on every board.
 *
 * A middle manager's output is not a notification: it is a claim with citations
 * plus a proposed next action, and it stays open until the owner answers. So
 * the card shows the sentence, the action, what it costs, and the rows the
 * claim rests on -- and the buttons are the only thing on the page that turn a
 * proposal into work.
 */
import { useTranslation } from "react-i18next";
import type { StaffDirective, StaffSuggestion } from "@pi-desktop/shared";
import { Badge, Button } from "../../components/ui";
import { IconCheck, IconTriangleAlert, IconX } from "../../components/icons";
import { decidedKey, directiveStatusKey } from "./board-format";

function directiveTone(status: StaffDirective["status"]): string {
  if (status === "done") return "ok";
  if (status === "failed") return "bad";
  if (status === "running") return "on";
  return "warn";
}

function Suggestion({
  item,
  busy,
  onDecide,
}: {
  item: StaffSuggestion;
  busy: boolean;
  onDecide: (id: string, decision: "accepted" | "rejected") => void;
}) {
  const { t } = useTranslation();
  return (
    <article className="advice-card" data-advice={item.id}>
      <header className="advice-head">
        <span className="advice-badge">
          <IconTriangleAlert size={13} aria-hidden />
          {t("boards.advice.label")}
        </span>
        <h3 className="advice-title">{item.title}</h3>
      </header>

      {item.body ? <p className="advice-body">{item.body}</p> : null}

      <dl className="advice-facts">
        {item.action ? (
          <>
            <dt>{t("boards.advice.action")}</dt>
            <dd>{item.action}</dd>
          </>
        ) : null}
        {item.impact ? (
          <>
            <dt>{t("boards.advice.impact")}</dt>
            <dd>{item.impact}</dd>
          </>
        ) : null}
        {item.costNote ? (
          <>
            <dt>{t("boards.advice.cost")}</dt>
            <dd>{item.costNote}</dd>
          </>
        ) : null}
      </dl>

      {item.evidence.length > 0 ? (
        <details className="advice-evidence">
          <summary>
            {t("boards.advice.evidence", { count: item.evidence.length })}
          </summary>
          <ul>
            {item.evidence.map((entry, index) => (
              <li key={item.id + "-e" + index}>
                <span className="advice-cite">
                  {entry.employeeName ? entry.employeeName + " · " : ""}
                  {entry.runId ? entry.runId.slice(0, 8) : ""}
                  {entry.step ? " · step " + entry.step : ""}
                </span>
                <span className="advice-label">{entry.label}</span>
                {entry.quote ? <q className="advice-quote">{entry.quote}</q> : null}
              </li>
            ))}
          </ul>
        </details>
      ) : null}

      {item.status === "open" ? (
        <div className="advice-actions">
          <Button
            variant="primary"
            size="sm"
            disabled={busy}
            onClick={() => onDecide(item.id, "accepted")}
          >
            <IconCheck size={13} aria-hidden />
            {t("boards.advice.accept")}
          </Button>
          <Button variant="ghost" size="sm" disabled={busy} onClick={() => onDecide(item.id, "rejected")}>
            <IconX size={13} aria-hidden />
            {t("boards.advice.reject")}
          </Button>
        </div>
      ) : (
        <div className="advice-outcome">
          <Badge>{t(decidedKey(item.status))}</Badge>
          {item.directive ? (
            <>
              <span className={"staff-dot " + directiveTone(item.directive.status)} aria-hidden />
              <span className="advice-directive">
                {t("boards.advice.directive", {
                  // An order that names nobody is the owner's own: approving a draft is
                  // not work any employee is allowed to do.
                  employee: item.directive.employeeName || t("boards.advice.myself"),
                  verb: item.directive.verb,
                })}
              </span>
              <Badge>{t(directiveStatusKey(item.directive.status))}</Badge>
            </>
          ) : null}
          {item.resultNote ? <p className="advice-result">{item.resultNote}</p> : null}
        </div>
      )}
    </article>
  );
}

/**
 * Every open proposal for one board, newest first, with the answered ones kept
 * below: "we decided not to" is the most useful thing a board can tell the next
 * analysis, so a rejection is shown rather than swept up.
 */
export function AdviceRail({
  suggestions,
  busyId,
  onDecide,
}: {
  suggestions: StaffSuggestion[];
  busyId?: string | null;
  onDecide: (id: string, decision: "accepted" | "rejected") => void;
}) {
  const { t } = useTranslation();
  const open = suggestions.filter((item) => item.status === "open");
  const answered = suggestions.filter((item) => item.status !== "open");

  return (
    <section className="advice-rail">
      <div className="staff-section-title">
        {t("boards.advice.title")}
        {open.length > 0 ? <span className="staff-count">{open.length}</span> : null}
      </div>
      {suggestions.length === 0 ? (
        <p className="staff-hint">{t("boards.advice.none")}</p>
      ) : (
        <>
          {open.map((item) => (
            <Suggestion key={item.id} item={item} busy={busyId === item.id} onDecide={onDecide} />
          ))}
          {answered.length > 0 ? (
            <details className="advice-answered">
              <summary>{t("boards.advice.answered", { count: answered.length })}</summary>
              {answered.map((item) => (
                <Suggestion key={item.id} item={item} busy={false} onDecide={onDecide} />
              ))}
            </details>
          ) : null}
        </>
      )}
    </section>
  );
}
