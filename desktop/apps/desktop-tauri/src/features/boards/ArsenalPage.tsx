/**
 * The arsenal: what every employee reads before it starts.
 *
 * This board owns no new storage. The knowledge base is the shared memory the
 * employees already read and write, and the safe holds the credential profiles
 * each employee runs on -- both already existed, and both were reachable only
 * from inside the console. What is new here is the framing: this is the page
 * where changing one sentence changes what every employee says next round, so
 * it belongs next to the boards rather than three tabs deep in a settings page.
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { StaffEmployee } from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { useAppStore } from "../../stores/app-store";
import { Button } from "../../components/ui";
import { IconRefresh, IconSparkles } from "../../components/icons";
import { AdviceRail } from "./AdviceRail";
import { BoardHeader, useBoardAdvice } from "./useBoardAdvice";
import { StaffCredentialPanel } from "../staff/StaffCredentialPanel";
import { StaffMemoryPanel } from "../staff/StaffMemoryPanel";
import { errorText } from "../staff/staff-format";

export function ArsenalPage() {
  const { t } = useTranslation();
  const showToast = useAppStore((s) => s.showToast);
  const [employees, setEmployees] = useState<StaffEmployee[]>([]);
  const [refreshToken, setRefreshToken] = useState(0);

  const advice = useBoardAdvice("arsenal", (message) => showToast(message, { variant: "error" }));

  const load = useCallback(async () => {
    try {
      const answer = await api.listStaffEmployees();
      setEmployees(answer.employees ?? []);
      setRefreshToken((value) => value + 1);
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    }
  }, [showToast]);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="thread-scroll">
      <div className="page-frame board-page">
        <BoardHeader
          title={t("boards.arsenal.title")}
          subtitle={t("boards.arsenal.subtitle")}
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

        <div className="board-split">
          <div className="arsenal-main">
            <section className="arsenal-block" key={"knowledge-" + refreshToken}>
              <div className="staff-section-title">{t("boards.arsenal.knowledge")}</div>
              <p className="staff-hint">{t("boards.arsenal.knowledgeHint")}</p>
              <StaffMemoryPanel employees={employees} />
            </section>
            <section className="arsenal-block">
              <div className="staff-section-title">{t("boards.arsenal.safe")}</div>
              <p className="staff-hint">{t("boards.arsenal.safeHint")}</p>
              <StaffCredentialPanel />
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
