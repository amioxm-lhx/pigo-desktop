import { useEffect, useState } from "react";
import i18n from "i18next";
import { useAppStore } from "../stores/app-store";
import { Button } from "./ui";

/**
 * Composer-dock card for /btw answers. It renders renderer-only state and
 * never reads or writes the session transcript: scoped agent.event envelopes
 * are diverted into btw-slice before the transcript reducer sees them.
 */
export function SideThreadPanel() {
  const thread = useAppStore((state) => state.btwThread);
  const activeSessionId = useAppStore((state) => state.activeSessionId);
  const closeBtw = useAppStore((state) => state.closeBtw);
  const openBtw = useAppStore((state) => state.openBtw);
  const [draft, setDraft] = useState("");

  // A side thread belongs to the session whose history it was asked against.
  // Leaving that session (select, project switch, delete) discards the card
  // and cancels its in-flight answer, so no stale context can resurface.
  useEffect(() => {
    if (thread && thread.sessionId !== activeSessionId) closeBtw();
  }, [thread, activeSessionId, closeBtw]);

  if (!thread || thread.sessionId !== activeSessionId) return null;

  const ask = () => {
    const question = draft.trim();
    if (!question || thread.running) return;
    setDraft("");
    void openBtw(question);
  };

  return (
    <section
      className="btw-card"
      aria-label={i18n.t("chat.btwTitle", { defaultValue: "顺便一问 /btw" })}
      style={{
        border: "1px solid rgba(127,127,127,0.28)",
        borderRadius: 12,
        background: "rgba(127,127,127,0.07)",
        color: "inherit",
        padding: "12px 14px",
        marginBottom: 8,
        display: "flex",
        flexDirection: "column",
        gap: 8,
        maxHeight: 320,
        overflowY: "auto",
      }}
    >
      <header
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 8,
          fontSize: 12,
          opacity: 0.8,
        }}
      >
        <span>{i18n.t("chat.btwTitle", { defaultValue: "顺便一问 /btw" })}</span>
        <Button variant="ghost" size="sm" onClick={closeBtw}>
          {i18n.t("common.close", { defaultValue: "关闭" })}
        </Button>
      </header>

      <div style={{ fontWeight: 600, whiteSpace: "pre-wrap" }}>
        {thread.question}
      </div>

      <div style={{ whiteSpace: "pre-wrap", lineHeight: 1.55, fontSize: 14 }}>
        {thread.answer}
        {!thread.answer && thread.running ? (
          <span style={{ opacity: 0.6 }}>
            {i18n.t("chat.btwThinking", { defaultValue: "正在回答…" })}
          </span>
        ) : null}
        {thread.error ? (
          <div style={{ color: "#d9534f", marginTop: 4 }}>{thread.error}</div>
        ) : null}
        {thread.aborted ? (
          <span style={{ opacity: 0.6 }}>
            {" "}
            {i18n.t("chat.btwAborted", { defaultValue: "（已中止）" })}
          </span>
        ) : null}
      </div>

      <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
        <input
          value={draft}
          disabled={thread.running}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              ask();
            }
          }}
          placeholder={i18n.t("chat.btwPlaceholder", {
            defaultValue: "另问一句（独立提问，不延续上一答）",
          })}
          style={{
            flex: 1,
            minWidth: 0,
            padding: "6px 10px",
            borderRadius: 8,
            border: "1px solid rgba(127,127,127,0.3)",
            background: "transparent",
            color: "inherit",
            fontSize: 13,
          }}
        />
        <Button
          variant="secondary"
          size="sm"
          onClick={ask}
          disabled={thread.running || !draft.trim()}
        >
          {i18n.t("chat.btwAsk", { defaultValue: "提问" })}
        </Button>
      </div>

      <small style={{ opacity: 0.6, fontSize: 11 }}>
        {i18n.t("chat.btwHint", {
          defaultValue:
            "旁路回答仅在此面板显示，不写入会话记录；每次提问都基于当前主对话独立发起，关闭、切换会话或重启后消失。",
        })}
      </small>
    </section>
  );
}
