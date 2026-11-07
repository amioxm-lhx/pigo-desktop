import { useEffect, type CSSProperties } from "react";
import i18n from "i18next";
import {
  flattenSessionForest,
  type SessionTreeNode,
} from "../lib/session-tree";
import {
  branchFromTreeNode,
  openSessionTreePanel,
  rewindToRestorePoint,
} from "../lib/commands";
import { useAppStore } from "../stores/app-store";
import { Button } from "./ui";

const cardStyle: CSSProperties = {
  border: "1px solid rgba(127,127,127,0.28)",
  borderRadius: 12,
  background: "rgba(127,127,127,0.07)",
  color: "inherit",
  padding: "12px 14px",
  marginBottom: 8,
  display: "flex",
  flexDirection: "column",
  gap: 10,
  maxHeight: 380,
  overflowY: "auto",
};

const rowStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 8,
  fontSize: 12,
  padding: "4px 0",
  borderTop: "1px solid rgba(127,127,127,0.16)",
};

const sectionStyle: CSSProperties = {
  fontSize: 11,
  letterSpacing: 0.4,
  textTransform: "uppercase",
  opacity: 0.55,
  marginTop: 4,
};

const lineageLinkStyle: CSSProperties = {
  flex: 1,
  textAlign: "left",
  background: "none",
  border: "none",
  color: "inherit",
  font: "inherit",
  fontSize: 12,
  padding: 0,
  cursor: "pointer",
  wordBreak: "break-all",
};

const badgeStyle: CSSProperties = {
  fontSize: 10,
  padding: "1px 6px",
  borderRadius: 999,
  border: "1px solid rgba(127,127,127,0.4)",
  opacity: 0.75,
  whiteSpace: "nowrap",
};

/**
 * Composer-dock view for `/tree` and `/rewind`. The tree view has two parts:
 * the fork lineage across sessions (schema v22 records which session and
 * message a branch was cut from) and the numbered nodes of the current
 * transcript with their regenerate variant families. The rewind view lists the
 * file-edit restore points.
 *
 * The desktop transcript still has no message-level parent pointer, so the
 * lineage is a session tree, not REPL's single-file branch diagram; the card
 * says so instead of drawing a topology the data cannot support.
 */
export function SessionTreePanel() {
  const treePanel = useAppStore((state) => state.treePanel);
  const activeSessionId = useAppStore((state) => state.activeSessionId);
  const closeTreePanel = useAppStore((state) => state.closeTreePanel);
  const activateMessageRevision = useAppStore(
    (state) => state.activateMessageRevision,
  );
  const messages = useAppStore((state) => state.messages);
  const selectSession = useAppStore((state) => state.selectSession);

  // The snapshot belongs to one session; never show stale nodes against another.
  useEffect(() => {
    if (treePanel && treePanel.sessionId !== activeSessionId) closeTreePanel();
  }, [treePanel, activeSessionId, closeTreePanel]);

  const switchVariant = async (node: SessionTreeNode, index: number) => {
    await activateMessageRevision(node.revisionRootId ?? node.id, index);
    useAppStore
      .getState()
      .showToast(i18n.t("chat.treeVariantSwitched", { index }), {
        variant: "success",
      });
    await openSessionTreePanel("tree");
  };

  const switchSession = async (sessionId: string) => {
    if (sessionId === activeSessionId) return;
    await selectSession(sessionId);
    // Rebuild against the new session so the panel keeps tracking the branch
    // the user just walked into instead of closing on the stale snapshot.
    await openSessionTreePanel("tree");
  };

  if (!treePanel) return null;

  const { view, nodes, points, forest } = treePanel;
  const title =
    view === "tree"
      ? i18n.t("chat.treeTitle")
      : i18n.t("chat.rewindTitle");
  const hint =
    view === "tree"
      ? i18n.t("chat.treeHint")
      : i18n.t("chat.rewindHint");
  const empty =
    view === "tree"
      ? i18n.t("chat.treeEmpty")
      : i18n.t("chat.rewindEmpty");
  const rows = view === "tree" ? nodes.length : points.length;

  return (
    <section className="info-card" aria-label={title} style={cardStyle}>
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
        <span>{title}</span>
        <Button variant="ghost" size="sm" onClick={closeTreePanel}>
          {i18n.t("common.close", { defaultValue: "关闭" })}
        </Button>
      </header>

      {view === "tree" ? (
        <>
          <div style={sectionStyle}>{i18n.t("chat.treeLineageTitle")}</div>
          {forest.length === 0 ? (
            <div style={{ fontSize: 12, opacity: 0.7 }}>
              {i18n.t("chat.treeLineageEmpty")}
            </div>
          ) : (
            flattenSessionForest(forest).map((session) => (
              <div
                key={`lineage-${session.id}`}
                style={{
                  ...rowStyle,
                  paddingLeft: (session.depth - 1) * 14,
                }}
              >
                <span style={{ opacity: 0.55, minWidth: 18 }}>
                  {session.depth > 1 ? "└" : "●"}
                </span>
                <button
                  type="button"
                  style={lineageLinkStyle}
                  onClick={() => void switchSession(session.id)}
                >
                  {session.title}
                </button>
                {session.orphaned ? (
                  <span style={badgeStyle}>
                    {i18n.t("chat.treeLineageOrphaned")}
                  </span>
                ) : null}
                {session.isActive ? (
                  <span style={badgeStyle}>
                    {i18n.t("chat.treeLineageCurrent")}
                  </span>
                ) : null}
              </div>
            ))
          )}

          <div style={sectionStyle}>{i18n.t("chat.treeNodesTitle")}</div>
          {rows === 0 ? (
            <div style={{ fontSize: 12, opacity: 0.7 }}>{empty}</div>
          ) : (
            nodes.map((node) => (
              <div key={node.id} style={rowStyle}>
                <span style={{ opacity: 0.55, minWidth: 26 }}>
                  #{node.index}
                </span>
                <span style={{ minWidth: 62, opacity: 0.75 }}>{node.role}</span>
                <span style={{ flex: 1, wordBreak: "break-all" }}>
                  {node.label}
                  {node.toolName ? ` · ${node.toolName}` : ""}
                  {node.subagent ? " · subagent" : ""}
                </span>
                {node.revisionCount && node.revisionCount > 1 ? (
                  <select
                    aria-label={i18n.t("chat.treeVariantLabel")}
                    disabled={!messages.some((message) => message.id === node.id)}
                    value={node.activeRevision ?? 1}
                    onChange={(event) =>
                      void switchVariant(node, Number(event.target.value))
                    }
                    style={{ fontSize: 11, maxWidth: 96 }}
                  >
                    {Array.from(
                      { length: node.revisionCount },
                      (_, offset) => (
                        <option key={offset + 1} value={offset + 1}>
                          {i18n.t("chat.treeVariantLabel")} {offset + 1}/
                          {node.revisionCount}
                        </option>
                      ),
                    )}
                  </select>
                ) : null}
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => void branchFromTreeNode(node.index)}
                >
                  {i18n.t("chat.treeBranchButton")}
                </Button>
              </div>
            ))
          )}
        </>
      ) : rows === 0 ? (
        <div style={{ fontSize: 12, opacity: 0.7 }}>{empty}</div>
      ) : (
        points.map((point) => (
          <div key={`${point.startIndex}-${point.createdAt}`} style={rowStyle}>
            <span style={{ opacity: 0.55, minWidth: 26 }}>
              #{point.index}
            </span>
            <span style={{ opacity: 0.7, minWidth: 72 }}>
              {formatWhen(point.createdAt)}
            </span>
            <span style={{ flex: 1, wordBreak: "break-all" }}>
              {point.label}
              <span style={{ opacity: 0.6 }}>
                {" "}
                · {point.changes.length} file(s)
              </span>
            </span>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void rewindToRestorePoint(point.index)}
            >
              {i18n.t("chat.rewindButton")}
            </Button>
          </div>
        ))
      )}

      <div style={{ fontSize: 11, opacity: 0.6, lineHeight: 1.5 }}>
        {hint}
      </div>
    </section>
  );
}

function formatWhen(value: string): string {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleTimeString();
}
