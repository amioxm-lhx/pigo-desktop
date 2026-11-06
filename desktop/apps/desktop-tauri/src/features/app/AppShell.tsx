import { lazy, Suspense, type CSSProperties, type ReactNode } from "react";
import { TooltipButton, cx } from "../../components/ui";
import {
  IconNewSession,
  IconPanel,
  IconPanelOpen,
} from "../../components/icons";
import { Sidebar } from "../../components/Sidebar";
import { ConversationTopbar } from "../../components/ConversationTopbar";
import { WorkPanel } from "../../components/workpanel/WorkPanel";
import { ChatSurface } from "../../components/ChatSurface";
import { SearchDialog } from "../../components/SearchDialog";
import { ToastHost } from "../../components/Toast";
import { PluginPanelHost } from "../../components/PluginPanelHost";
import { ExtensionPromptHost } from "../../components/ExtensionPromptDialog";
import { ProjectCreateDialog } from "../../components/ProjectCreateDialog";
import { UpdateBanner } from "../../components/UpdateBanner";
import { WindowControls } from "../../components/WindowControls";
import { api } from "../../lib/api";
import { CollapsedTitlebarActions, RoutePending } from "./chrome";
import { useAppShellRuntime } from "./useAppShellRuntime";

const SettingsPage = lazy(() =>
  import("../../pages/SettingsPage").then((module) => ({
    default: module.SettingsPage,
  })),
);
const PullRequestsPage = lazy(() =>
  import("../../pages/PullRequestsPage").then((module) => ({
    default: module.PullRequestsPage,
  })),
);
const ScheduledPage = lazy(() =>
  import("../../pages/ScheduledPage").then((module) => ({
    default: module.ScheduledPage,
  })),
);
const PluginsPage = lazy(() =>
  import("../../pages/PluginsPage").then((module) => ({
    default: module.PluginsPage,
  })),
);
const TodayPage = lazy(() =>
  import("../desk/TodayPage").then((module) => ({
    default: module.TodayPage,
  })),
);
const PipelinePage = lazy(() =>
  import("../desk/PipelinePage").then((module) => ({
    default: module.PipelinePage,
  })),
);
const DataPage = lazy(() =>
  import("../desk/DataPage").then((module) => ({
    default: module.DataPage,
  })),
);
const StaffPage = lazy(() =>
  import("../staff/StaffPage").then((module) => ({
    default: module.StaffPage,
  })),
);
// The boards the middle managers read. Each is its own entry in "My apps"
// rather than a tab of the console: the console answers "is my team healthy",
// and a board answers "what do I decide today".
const LeadBoardPage = lazy(() =>
  import("../boards/LeadBoardPage").then((module) => ({
    default: module.LeadBoardPage,
  })),
);
const ContentStudioPage = lazy(() =>
  import("../boards/ContentStudioPage").then((module) => ({
    default: module.ContentStudioPage,
  })),
);
const ReportBoardPage = lazy(() =>
  import("../boards/ReportBoardPage").then((module) => ({
    default: module.ReportBoardPage,
  })),
);
const ArsenalPage = lazy(() =>
  import("../boards/ArsenalPage").then((module) => ({
    default: module.ArsenalPage,
  })),
);
const StockPickerPage = lazy(() =>
  import("../stocks/StockPickerPage").then((module) => ({
    default: module.StockPickerPage,
  })),
);

export function AppShell() {
  const {
    t,
    ready,
    page,
    activeSessionId,
    subagentPanel,
    subagentPanelOpen,
    closeSubagentPanel,
    workPanelOpen,
    searchOpen,
    setSearchOpen,
    sidebarCollapsed,
    sidebarExiting,
    sidebarWidth,
    handleSidebarWidthChange,
    handleSidebarWidthCommit,
    toggleSidebar,
    reopenSidebar,
    autoCollapseSidebar,
    appShellRef,
    shellWidth,
    runMenuCommand,
    handleSidebarAnimationEnd,
    presentedWorkPanelOpen,
    workPanelExiting,
    workPanelExitGeneration,
    finishWorkPanelExit,
    togglePresentedWorkPanel,
    workPanelMaximized,
    toggleWorkPanelMaximize,
    backendDown,
    archMismatch,
    setArchMismatch,
    showSplash,
    splash,
    sidebarToggleShortcut,
    workPanelToggleTooltip,
  } = useAppShellRuntime();

  let shell: ReactNode = null;
  if (ready) {
    if (page === "settings") {
      shell = (
        <>
          <WindowControls />
          <Suspense fallback={<RoutePending />}>
            <SettingsPage />
          </Suspense>
          <SearchDialog open={searchOpen} onClose={() => setSearchOpen(false)} />
          <ToastHost />
          <ExtensionPromptHost />
          <UpdateBanner />
        </>
      );
    } else {
      shell = (
        <>
          {!sidebarCollapsed || sidebarExiting ? (
            <Sidebar
              className={sidebarExiting ? "is-exiting" : undefined}
              onAnimationEnd={handleSidebarAnimationEnd}
              onToggleSidebar={toggleSidebar}
              sidebarToggleShortcut={sidebarToggleShortcut}
              sidebarWidth={sidebarWidth}
              onWidthChange={handleSidebarWidthChange}
              onWidthCommit={handleSidebarWidthCommit}
            />
          ) : null}

          {workPanelMaximized && (
            /* Preview mode hides MainChat, which normally owns the drag band and
               the window controls. Keep the same top band with the same system
               buttons, at the window level rather than inside the panel. */
            <div
              className={cx(
                "window-chrome-row",
                !sidebarCollapsed && "sidebar-expanded",
              )}
            >
              {sidebarCollapsed && (
                <CollapsedTitlebarActions
                  onToggleSidebar={toggleSidebar}
                  onNewTask={() => void runMenuCommand("newTask")}
                  sidebarToggleShortcut={sidebarToggleShortcut}
                />
              )}
              {!sidebarCollapsed && (
                <TooltipButton
                  type="button"
                  className="title-nav-btn"
                  tooltip={t("nav.newTask")}
                  ariaLabel={t("nav.newTask")}
                  data-nav="new-task"
                  onClick={() => void runMenuCommand("newTask")}
                >
                  <IconNewSession size={13} />
                </TooltipButton>
              )}
              <div className="window-chrome-drag" aria-hidden data-tauri-drag-region />
              <WindowControls contained />
            </div>
          )}

          {!workPanelMaximized && (
          <section className="main-pane">
            <WindowControls contained />
            {page === "chat" ? (
              <ConversationTopbar
                sidebarCollapsed={sidebarCollapsed}
                workPanelOpen={presentedWorkPanelOpen}
                onToggleSidebar={toggleSidebar}
                onNewTask={() => void runMenuCommand("newTask")}
                onOpenSearch={() => setSearchOpen(true)}
              />
            ) : (
              <div
                className={cx(
                  "main-titlebar",
                  presentedWorkPanelOpen && "work-panel-open",
                )}
                data-tauri-drag-region
              >
                {sidebarCollapsed && (
                  <div className="main-titlebar-left no-drag">
                    <CollapsedTitlebarActions
                      onToggleSidebar={reopenSidebar}
                      onNewTask={() => void runMenuCommand("newTask")}
                      sidebarToggleShortcut={sidebarToggleShortcut}
                    />
                  </div>
                )}
              </div>
            )}
            <UpdateBanner />

            {backendDown && (
              <div
                className={`backend-banner no-drag ${backendDown.fatal ? "fatal" : "warn"}`}
                role="status"
              >
                <span className="backend-dot" aria-hidden />
                <span>
                  {backendDown.fatal
                    ? backendDown.message === "GLIBC_UNSUPPORTED"
                      ? t("status.unsupportedGlibc")
                      : backendDown.message === "DB_SCHEMA_TOO_NEW"
                        ? t("status.dbSchemaTooNew", {
                            found: backendDown.schema?.found ?? "?",
                            supported: backendDown.schema?.supported ?? "?",
                          })
                        : t("status.fatal")
                    : t("status.restarting")}
                </span>
                {backendDown.fatal && (
                  <button
                    type="button"
                    className="backend-action"
                    onClick={() => void api.openLogs()}
                  >
                    {t("status.openLogs")}
                  </button>
                )}
              </div>
            )}

            {archMismatch && (
              <div className="backend-banner no-drag warn" role="status">
                <span className="backend-dot" aria-hidden />
                <span>
                  {t("status.archMismatch", {
                    buildArch: t(
                      `status.archNames.${archMismatch.platform}.${archMismatch.processArch}`,
                      { defaultValue: archMismatch.processArch },
                    ),
                    machineArch: t(
                      `status.archNames.${archMismatch.platform}.${archMismatch.machineArch}`,
                      { defaultValue: archMismatch.machineArch },
                    ),
                  })}
                </span>
                <button
                  type="button"
                  className="backend-action"
                  onClick={() => setArchMismatch(null)}
                >
                  {t("status.dismissArchMismatch")}
                </button>
              </div>
            )}

            <Suspense fallback={<RoutePending />}>
              {page === "pulls" ? (
                <div className="route-surface route-page">
                  <PullRequestsPage />
                </div>
              ) : page === "scheduled" ? (
                <div className="route-surface route-page">
                  <ScheduledPage />
                </div>
              ) : page === "plugins" ? (
                <div className="route-surface route-page">
                  <PluginsPage />
                </div>
              ) : page === "today" ? (
                <div className="route-surface route-page">
                  <TodayPage />
                </div>
              ) : page === "pipeline" ? (
                <div className="route-surface route-page">
                  <PipelinePage />
                </div>
              ) : page === "staff" ? (
                <div className="route-surface route-page">
                  <StaffPage />
                </div>
              ) : page === "leads" ? (
                <div className="route-surface route-page">
                  <LeadBoardPage />
                </div>
              ) : page === "contents" ? (
                <div className="route-surface route-page">
                  <ContentStudioPage />
                </div>
              ) : page === "report" ? (
                <div className="route-surface route-page">
                  <ReportBoardPage />
                </div>
              ) : page === "data" ? (
                <div className="route-surface route-page">
                  <DataPage />
                </div>
              ) : page === "arsenal" ? (
                <div className="route-surface route-page">
                  <ArsenalPage />
                </div>
              ) : page === "stocks" ? (
                <div className="route-surface route-page">
                  <StockPickerPage />
                </div>
              ) : (
                <ChatSurface />
              )}
            </Suspense>
          </section>
          )}

          {(presentedWorkPanelOpen || workPanelExiting) && (
            <WorkPanel
              panelBlocked={searchOpen}
              exiting={workPanelExiting}
              onExitAnimationEnd={() =>
                finishWorkPanelExit(workPanelExitGeneration.current)
              }
              subagentPanel={subagentPanelOpen ? subagentPanel : null}
              onCloseSubagentPanel={closeSubagentPanel}
              containerWidth={shellWidth}
              sidebarWidth={sidebarWidth}
              sidebarCollapsed={sidebarCollapsed}
              sidebarExiting={sidebarExiting}
              onAutoCollapseSidebar={autoCollapseSidebar}
              maximized={workPanelMaximized}
              onToggleMaximize={toggleWorkPanelMaximize}
            />
          )}

          <TooltipButton
            type="button"
            className="app-work-panel-toggle no-drag"
            tooltip={workPanelToggleTooltip}
            ariaLabel={workPanelToggleTooltip}
            aria-pressed={workPanelOpen || presentedWorkPanelOpen}
            disabled={!activeSessionId && !presentedWorkPanelOpen && !workPanelExiting}
            onClick={togglePresentedWorkPanel}
          >
            <span className="app-work-panel-toggle-icon" aria-hidden>
              <IconPanel size={15} />
              <IconPanelOpen size={15} />
            </span>
          </TooltipButton>

          <SearchDialog open={searchOpen} onClose={() => setSearchOpen(false)} />
          <ToastHost />
          <ExtensionPromptHost />
        </>
      );
    }
  }

  return (
    <div
      ref={appShellRef}
      className={cx(
        "app-shell",
        !ready && "app-shell-boot",
        page === "settings" && ready && "settings-mode",
        sidebarCollapsed && "sidebar-collapsed",
        workPanelMaximized && "work-panel-maximized",
        showSplash && "is-booting",
      )}
      style={{ "--ds-sidebar-width": `${sidebarWidth}px` } as CSSProperties}
    >
      {shell}
      <ProjectCreateDialog />
      <PluginPanelHost />
      {splash}
    </div>
  );
}
