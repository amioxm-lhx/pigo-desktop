import { useTranslation } from "react-i18next";
import { useAppStore } from "../stores/app-store";
import {
  IconFolder,
  IconSidebar,
  IconNewSession,
  IconSearch,
} from "./icons";
import { TooltipButton } from "./ui";

function projectName(path?: string | null, name?: string | null) {
  if (name) return name;
  if (!path) return null;
  const parts = path.split(/[/\\]/).filter(Boolean);
  return parts[parts.length - 1] || path;
}

function isDefaultSessionTitle(title?: string | null) {
  const trimmed = (title || "").trim().toLowerCase();
  if (!trimmed) return true;
  return ["new task", "new chat", "新建任务", "新对话"].includes(trimmed);
}

const TOPBAR_TITLE_MAX_LENGTH = 10;

function truncateTopbarTitle(title: string) {
  const characters = Array.from(title);
  return characters.length > TOPBAR_TITLE_MAX_LENGTH
    ? `${characters.slice(0, TOPBAR_TITLE_MAX_LENGTH).join("")}…`
    : title;
}

export function ConversationTopbar({
  sidebarCollapsed,
  workPanelOpen,
  onToggleSidebar,
  onNewTask,
  onOpenSearch,
}: {
  sidebarCollapsed: boolean;
  workPanelOpen: boolean;
  onToggleSidebar: () => void;
  onNewTask: () => void;
  onOpenSearch: () => void;
}) {
  const { t } = useTranslation();
  const activeSessionId = useAppStore((s) => s.activeSessionId);
  const sessions = useAppStore((s) => s.sessions);
  const workspace = useAppStore((s) => s.workspace);

  const activeSession = sessions.find((session) => session.id === activeSessionId);

  const fullTaskTitle = isDefaultSessionTitle(activeSession?.title)
    ? t("chat.untitledTask")
    : activeSession?.title || t("chat.untitledTask");
  const taskTitle = truncateTopbarTitle(fullTaskTitle);
  const project = projectName(workspace?.path, workspace?.name);

  /**
   * Opens the work panel on the workspace folder tree. Browsing is a
   * session-owned panel surface, so the home draft materializes first; chat
   * file links keep their own preview path and never route through here.
   */
  const handleOpenWorkspaceFiles = async () => {
    const store = useAppStore.getState();
    if (!store.workspace?.path) {
      store.showToast(t("panel.files.noWorkspace"), { variant: "error" });
      return;
    }
    try {
      if (!store.activeSessionId) await store.newSession();
      useAppStore.getState().openWorkspaceFiles();
    } catch (error) {
      useAppStore
        .getState()
        .showToast(error instanceof Error ? error.message : String(error), {
          variant: "error",
        });
    }
  };

  return (
    <div
      className={`conversation-topbar${sidebarCollapsed ? " ct-collapsed" : ""}${
        workPanelOpen ? " ct-work-panel-open" : ""
      }`}
      role="toolbar"
      aria-label={t("nav.conversation")}
      // `deep` makes empty areas and the title draggable while Tauri's drag
      // script still blocks interactive descendants inside buttons.
      data-tauri-drag-region="deep"
    >
      <div className="ct-left">
        {/*
          Always mounted: the slot animates from 0 to 28px with the dock, so
          unmounting it would reintroduce the first-frame title jump. While the
          sidebar is open the slot is zero-width and hidden from AT.
        */}
        <div className="ct-lead" aria-hidden={!sidebarCollapsed}>
          <TooltipButton
            type="button"
            className="ct-icon-btn"
            tooltip={t("nav.toggleSidebar")}
            ariaLabel={t("nav.toggleSidebar")}
            tabIndex={sidebarCollapsed ? undefined : -1}
            onClick={onToggleSidebar}
          >
            <IconSidebar size={15} />
          </TooltipButton>
        </div>
        <div
          className="ct-title-wrap"
          title={project ? `${project} · ${fullTaskTitle}` : fullTaskTitle}
        >
          <span className="ct-title">{taskTitle}</span>
        </div>
      </div>

      <div className="ct-right">
        <div className="ct-actions">
          <TooltipButton
            type="button"
            className="ct-icon-btn"
            tooltip={t("nav.newTask")}
            ariaLabel={t("nav.newTask")}
            onClick={onNewTask}
          >
            <IconNewSession size={15} />
          </TooltipButton>
          <TooltipButton
            type="button"
            className="ct-icon-btn"
            tooltip={t("nav.search")}
            ariaLabel={t("nav.search")}
            onClick={onOpenSearch}
          >
            <IconSearch size={15} />
          </TooltipButton>
          <TooltipButton
            type="button"
            className="ct-icon-btn"
            tooltip={t("nav.workspaceFiles")}
            ariaLabel={t("nav.workspaceFiles")}
            onClick={() => void handleOpenWorkspaceFiles()}
          >
            <IconFolder size={15} />
          </TooltipButton>
        </div>
      </div>
    </div>
  );
}
