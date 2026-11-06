/**
 * What the board's middle manager decided today.
 *
 * The board below this deck is what the manager's orders eventually produced.
 * The deck is the reasoning: what it read, what it concluded, who it handed
 * work to, and which hand-outs were refused. Without it a board that filled up
 * looks the same whether it was worked by a manager or by luck.
 *
 * A board with no manager still draws -- the cards are the owner's either way --
 * and says so instead of showing an empty section.
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
  StaffDecision,
  StaffDirective,
  StaffEmployee,
  StaffSuggestionBoard,
} from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { Badge } from "../../components/ui";
import {
  agoLabel,
  boardManagerOf,
  directiveStatusKey,
  liveOrders,
  settledOrders,
} from "./board-format";
import { whenLabel } from "../staff/staff-format";

const DECISION_LIMIT = 3;

export function ManagerDeck({ board }: { board: StaffSuggestionBoard }) {
  const { t, i18n } = useTranslation();
  const [manager, setManager] = useState<StaffEmployee | null>(null);
  const [decisions, setDecisions] = useState<StaffDecision[]>([]);
  const [orders, setOrders] = useState<StaffDirective[]>([]);

  const load = useCallback(async () => {
    try {
      const employees = (await api.listStaffEmployees()).employees ?? [];
      const found = boardManagerOf(employees, board) ?? null;
      setManager(found);
      const [decisionAnswer, orderAnswer] = await Promise.all([
        api.listStaffDecisions({ board, limit: DECISION_LIMIT }),
        found
          ? api.listStaffDirectives({ issuerEmployeeId: found.id, limit: 20 })
          : Promise.resolve({ directives: [] as StaffDirective[] }),
      ]);
      setDecisions(decisionAnswer.decisions ?? []);
      setOrders(orderAnswer.directives ?? []);
    } catch {
      // A board is the owner's board first: a manager that cannot be read must
      // not take the cards down with it.
      setDecisions([]);
      setOrders([]);
    }
  }, [board]);

  useEffect(() => {
    void load();
    const off = api.onStaffChanged(() => void load());
    return off;
  }, [load]);

  if (!manager) {
    return (
      <section className="manager-deck">
        <p className="staff-hint">{t("boards.manager.none")}</p>
      </section>
    );
  }

  const latest = decisions[0];
  const earlier = decisions.slice(1);
  const live = liveOrders(orders);
  const done = settledOrders(orders);

  const card = (decision: StaffDecision, compact: boolean) => (
    <article className="manager-decision" key={decision.id}>
      <div className="manager-decision-head">
        <b>{decision.title}</b>
        <span className="staff-hint">{agoLabel(decision.createdAt, t)}</span>
      </div>
      {decision.verdict ? <p className="manager-decision-verdict">{decision.verdict}</p> : null}
      {!compact && decision.inputs.length > 0 ? (
        <div className="manager-decision-inputs">
          <span className="staff-hint">{t("boards.manager.lookingAt")}</span>
          {decision.inputs.slice(0, 6).map((line, index) => (
            <span className="staff-tag" key={index}>
              {line}
            </span>
          ))}
        </div>
      ) : null}
      {decision.dispatched.length > 0 ? (
        <div className="manager-decision-orders">
          {decision.dispatched.map((order) => (
            <span className="staff-tag on" key={order.id}>
              {t("boards.manager.orderTo", {
                name: order.employeeName || order.employeeId || "",
                verb: order.verb,
              })}
            </span>
          ))}
        </div>
      ) : null}
      {decision.note ? (
        <p className="manager-decision-note">{t("boards.manager.refused") + "：" + decision.note}</p>
      ) : null}
    </article>
  );

  return (
    <section className="manager-deck">
      <div className="staff-section-title">
        {t("boards.manager.thinking", { name: manager.name })}
        <span className="staff-count">{decisions.length}</span>
      </div>
      {latest ? (
        card(latest, false)
      ) : (
        <p className="staff-hint">
          {t("boards.manager.noDecision", {
            when: whenLabel(manager.nextRunAt, i18n.language) || t("staff.never"),
          })}
        </p>
      )}
      {earlier.length > 0 ? (
        <details className="manager-earlier">
          <summary>{t("boards.manager.earlier", { count: earlier.length })}</summary>
          {earlier.map((decision) => card(decision, true))}
        </details>
      ) : null}

      <div className="staff-section-title">
        {t("boards.manager.orders")}
        {live.length > 0 ? <span className="staff-count">{live.length}</span> : null}
      </div>
      {orders.length === 0 ? (
        <p className="staff-hint">{t("boards.manager.noOrders")}</p>
      ) : (
        <>
          <div className="manager-orders">
            {live.map((order) => (
              <div className="manager-order" key={order.id}>
                <Badge tone="neutral">{t(directiveStatusKey(order.status))}</Badge>
                <span className="manager-order-to">
                  {order.employeeName || order.employeeId}
                  {" · "}
                  {order.verb}
                </span>
                <span className="manager-order-task">{taskOf(order)}</span>
                {order.deadlineAt ? (
                  <span className="staff-hint">
                    {t("boards.manager.deadline", { when: whenLabel(order.deadlineAt, i18n.language) })}
                  </span>
                ) : null}
              </div>
            ))}
          </div>
          {done.length > 0 ? (
            <details className="manager-earlier">
              <summary>{t("boards.manager.ordersDone", { count: done.length })}</summary>
              {done.map((order) => (
                <div className="manager-order" key={order.id}>
                  <Badge tone="neutral">{t(directiveStatusKey(order.status))}</Badge>
                  <span className="manager-order-to">
                    {order.employeeName || order.employeeId}
                    {" · "}
                    {order.verb}
                  </span>
                  <span className="manager-order-task">{order.resultNote || taskOf(order)}</span>
                </div>
              ))}
            </details>
          ) : null}
        </>
      )}
    </section>
  );
}

/** The errand a manager wrote, which travels in the order's payload. */
function taskOf(order: StaffDirective): string {
  const task = order.payload?.task;
  return typeof task === "string" ? task : "";
}


/**
 * Every middle manager's latest judgement, for the report board.
 *
 * The owner reads the report to decide what to do next, and the managers have
 * already read it and decided: showing their calls here, with the board each
 * one belongs to, is what turns the report from a dashboard into a briefing.
 */
export function ManagerDigest() {
  const { t } = useTranslation();
  const [decisions, setDecisions] = useState<StaffDecision[]>([]);
  const [managers, setManagers] = useState<StaffEmployee[]>([]);

  const load = useCallback(async () => {
    try {
      const [employeeAnswer, decisionAnswer] = await Promise.all([
        api.listStaffEmployees(),
        api.listStaffDecisions({ limit: 6 }),
      ]);
      setManagers((employeeAnswer.employees ?? []).filter((employee) => employee.layer === "manager"));
      setDecisions(decisionAnswer.decisions ?? []);
    } catch {
      setManagers([]);
      setDecisions([]);
    }
  }, []);

  useEffect(() => {
    void load();
    const off = api.onStaffChanged(() => void load());
    return off;
  }, [load]);

  if (managers.length === 0) return null;

  return (
    <section className="manager-deck">
      <div className="staff-section-title">
        {t("boards.manager.digest")}
        <span className="staff-count">{decisions.length}</span>
      </div>
      {decisions.length === 0 ? (
        <p className="staff-hint">{t("boards.manager.digestEmpty")}</p>
      ) : (
        <div className="manager-orders">
          {decisions.map((decision) => (
            <div className="manager-order" key={decision.id}>
              <Badge tone="neutral">{decision.employeeName}</Badge>
              <span className="manager-order-to">{decision.title}</span>
              <span className="manager-order-task">
                {decision.verdict || t("boards.manager.noVerdict")}
              </span>
              {decision.note ? (
                <span className="staff-hint">{t("boards.manager.refused") + "：" + decision.note}</span>
              ) : null}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
