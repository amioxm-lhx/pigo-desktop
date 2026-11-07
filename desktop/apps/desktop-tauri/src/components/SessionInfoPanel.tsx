import { Fragment, useEffect, useRef, useState } from "react";
import i18n from "i18next";
import type { MemoryStoreReport } from "@pi-desktop/shared";
import { api } from "../lib/api";
import { useAppStore } from "../stores/app-store";
import { Button } from "./ui";

type Row = { label: string; value: unknown };
type Group = { title: string; rows: Row[] };

/**
 * Composer-dock report for /session and /status (desktop parity for the REPL
 * commands). Every field comes from renderer state already held in the store,
 * so opening the card issues no host RPC and never touches the transcript.
 */
export function SessionInfoPanel() {
  const infoPanel = useAppStore((state) => state.infoPanel);
  const activeSessionId = useAppStore((state) => state.activeSessionId);
  const closeInfoPanel = useAppStore((state) => state.closeInfoPanel);
  const sessions = useAppStore((state) => state.sessions);
  const messages = useAppStore((state) => state.messages);
  const compactions = useAppStore((state) => state.sessionCompactions);
  const settings = useAppStore((state) => state.settings);
  const workspace = useAppStore((state) => state.workspace);
  const providers = useAppStore((state) => state.providers);
  const plugins = useAppStore((state) => state.plugins);
  const isRunning = useAppStore((state) => state.isRunning);
  const agentStatuses = useAppStore((state) => state.agentStatuses);
  const latestTurnResults = useAppStore((state) => state.latestTurnResults);

  // The report describes one session. Leaving that session discards the card
  // so stale figures are never shown against a different conversation.
  const openedSessionRef = useRef<string | undefined>(activeSessionId);
  useEffect(() => {
    if (!infoPanel) {
      openedSessionRef.current = activeSessionId;
      return;
    }
    if (openedSessionRef.current !== activeSessionId) closeInfoPanel();
  }, [infoPanel, activeSessionId, closeInfoPanel]);

  // /memory is the one report the renderer cannot assemble itself: the memory
  // index lives in the sidecar. Every other figure stays renderer-local.
  const [memoryReport, setMemoryReport] = useState<MemoryStoreReport | null>(null);
  const [memoryError, setMemoryError] = useState<string | null>(null);
  useEffect(() => {
    if (infoPanel !== "memory") return;
    let cancelled = false;
    setMemoryReport(null);
    setMemoryError(null);
    api
      .reportMemory()
      .then((report) => {
        if (!cancelled) setMemoryReport(report);
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setMemoryError(error instanceof Error ? error.message : String(error));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [infoPanel]);

  if (!infoPanel) return null;

  const session = activeSessionId
    ? sessions.find((item) => item.id === activeSessionId)
    : undefined;
  const compactionCount = activeSessionId
    ? (compactions[activeSessionId]?.length ?? 0)
    : 0;
  const roleCounts = messages.reduce<Record<string, number>>((acc, message) => {
    acc[message.role] = (acc[message.role] ?? 0) + 1;
    return acc;
  }, {});
  const lastUsage = [...messages]
    .reverse()
    .find((message) => message.role === "assistant" && message.usage)?.usage;
  const lastResult = activeSessionId
    ? latestTurnResults[activeSessionId]
    : undefined;
  const agentStatus = activeSessionId
    ? agentStatuses[activeSessionId]
    : undefined;
  const enabledProviders = providers.filter((provider) => provider.enabled);
  const credentialedProviders = enabledProviders.filter(
    (provider) => provider.hasSecret,
  );

  const sessionGroups: Group[] = [
    {
      title: i18n.t("chat.infoSessionGroup", { defaultValue: "Session" }),
      rows: [
        { label: "ID", value: activeSessionId ? shortId(activeSessionId) : "—" },
        { label: "Title", value: session?.title },
        { label: "Mode", value: session?.mode },
        { label: "Permission", value: session?.permissionMode },
        { label: "Thinking", value: session?.thinkingLevel },
        { label: "Provider", value: session?.providerId },
        { label: "Model", value: session?.modelId },
        { label: "Created", value: formatDate(session?.createdAt) },
        { label: "Updated", value: formatDate(session?.updatedAt) },
      ],
    },
    {
      title: i18n.t("chat.infoContextGroup", { defaultValue: "Context" }),
      rows: [
        { label: "Messages (loaded)", value: messages.length },
        {
          label: "By role",
          value: Object.entries(roleCounts)
            .map(([role, count]) => `${role} ${count}`)
            .join(" · "),
        },
        { label: "Compactions", value: compactionCount },
        { label: "Last input tokens", value: lastUsage?.inputTokens },
        { label: "Last output tokens", value: lastUsage?.outputTokens },
        { label: "Last cache read", value: lastUsage?.cacheReadTokens },
        { label: "Last total tokens", value: lastUsage?.totalTokens },
        { label: "Last turn", value: lastResult?.status },
      ],
    },
  ];

  const statusGroups: Group[] = [
    {
      title: i18n.t("chat.infoRuntimeGroup", { defaultValue: "Runtime" }),
      rows: [
        { label: "Running", value: isRunning },
        { label: "Agent phase", value: agentStatus },
        { label: "Active session", value: activeSessionId ? shortId(activeSessionId) : "—" },
        { label: "Default mode", value: settings?.defaultMode },
        { label: "Default permission", value: settings?.defaultPermissionMode },
      ],
    },
    {
      title: i18n.t("chat.infoProjectGroup", { defaultValue: "Project & environment" }),
      rows: [
        { label: "Workspace", value: workspace?.path },
        { label: "Command shell", value: settings?.defaultCommandShell },
        { label: "Enabled providers", value: enabledProviders.length },
        { label: "Providers with credentials", value: credentialedProviders.length },
        { label: "OAuth accounts", value: enabledProviders.filter((p) => p.hasOauth).length },
        { label: "Loaded plugins", value: plugins.length },
      ],
    },
    {
      title: i18n.t("chat.infoContextGroup", { defaultValue: "Context" }),
      rows: [
        { label: "Messages (loaded)", value: messages.length },
        { label: "Compactions", value: compactionCount },
        { label: "Last total tokens", value: lastUsage?.totalTokens },
      ],
    },
  ];

  const memoryGroups: Group[] = [
    {
      title: i18n.t("chat.memoryStoreGroup", {
        defaultValue: "Persistent memory",
      }),
      rows: [
        {
          label: i18n.t("chat.memoryStatus", { defaultValue: "Status" }),
          value: memoryReport
            ? memoryReport.enabled
              ? "enabled"
              : "disabled"
            : memoryError
              ? "unavailable"
              : "…",
        },
        { label: i18n.t("chat.memoryRoot", { defaultValue: "Root" }), value: memoryReport?.root },
        { label: i18n.t("chat.memoryIndex", { defaultValue: "Index" }), value: memoryReport?.dbPath },
        { label: i18n.t("chat.memoryTotal", { defaultValue: "Entries" }), value: memoryReport?.total },
        {
          label: i18n.t("chat.memoryByScope", { defaultValue: "By scope" }),
          value: memoryReport
            ? Object.entries(memoryReport.counts)
                .sort(([a], [b]) => a.localeCompare(b))
                .map(([scope, count]) => `${scope} ${count}`)
                .join(" · ")
            : undefined,
        },
        {
          label: i18n.t("chat.memoryReconciled", { defaultValue: "Reconciled" }),
          value: memoryReport
            ? `+${memoryReport.indexed} / -${memoryReport.pruned}`
            : undefined,
        },
        { label: i18n.t("chat.memoryError", { defaultValue: "Error" }), value: memoryReport?.error ?? memoryError },
      ],
    },
    {
      title: i18n.t("chat.infoContextGroup", { defaultValue: "Context" }),
      rows: [
        { label: "Messages (loaded)", value: messages.length },
        { label: "Compactions", value: compactionCount },
        { label: "Last total tokens", value: lastUsage?.totalTokens },
        { label: "Model", value: session?.modelId },
      ],
    },
  ];

  const groups =
    infoPanel === "status"
      ? statusGroups
      : infoPanel === "memory"
        ? memoryGroups
        : sessionGroups;
  const title = i18n.t(
    infoPanel === "status"
      ? "chat.infoStatusTitle"
      : infoPanel === "memory"
        ? "chat.infoMemoryTitle"
        : "chat.infoSessionTitle",
    {
      defaultValue:
        infoPanel === "status"
          ? "运行时状态 /status"
          : infoPanel === "memory"
            ? "持久记忆 /memory"
            : "会话信息 /session",
    },
  );
  const footnote =
    infoPanel === "memory"
      ? i18n.t("chat.infoMemoryFootnote", {
          defaultValue:
            "只读报告：记忆条目来自 sidecar 打开的记忆索引（先按磁盘文件重建索引再计数），不会写入会话记录。桌面端没有 CLI 的 checkpoint 自动压缩，上下文用量以上面的 compaction 与 token 为准。",
        })
      : i18n.t("chat.infoFootnote", {
          defaultValue:
            "只读报告：数据来自当前渲染器状态，不会写入会话记录。Token 为模型实际上报值，桌面端权限模式与 REPL /trust 的三态不同。",
        });

  return (
    <section
      className="info-card"
      aria-label={title}
      style={{
        border: "1px solid rgba(127,127,127,0.28)",
        borderRadius: 12,
        background: "rgba(127,127,127,0.07)",
        color: "inherit",
        padding: "12px 14px",
        marginBottom: 8,
        display: "flex",
        flexDirection: "column",
        gap: 10,
        maxHeight: 360,
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
        <span>{title}</span>
        <Button variant="ghost" size="sm" onClick={closeInfoPanel}>
          {i18n.t("common.close", { defaultValue: "关闭" })}
        </Button>
      </header>

      {groups.map((group) => (
        <div key={group.title}>
          <div style={{ fontSize: 11, fontWeight: 600, opacity: 0.7, marginBottom: 4 }}>
            {group.title}
          </div>
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "minmax(130px, auto) 1fr",
              gap: "3px 12px",
              fontSize: 12,
            }}
          >
            {group.rows.map((row) => (
              <Fragment key={row.label}>
                <span style={{ opacity: 0.65 }}>{row.label}</span>
                <span style={{ wordBreak: "break-all" }}>
                  {formatValue(row.value)}
                </span>
              </Fragment>
            ))}
          </div>
        </div>
      ))}

      <div style={{ fontSize: 11, opacity: 0.6 }}>{footnote}</div>
    </section>
  );
}

function formatValue(value: unknown): string {
  if (value === null || value === undefined || value === "") return "—";
  if (typeof value === "boolean") return value ? "yes" : "no";
  if (typeof value === "number") return value.toLocaleString();
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

function formatDate(value: string | undefined): string {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function shortId(value: string): string {
  return value.length > 16 ? `${value.slice(0, 8)}…${value.slice(-6)}` : value;
}
