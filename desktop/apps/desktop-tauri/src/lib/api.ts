import type {
  ActivationScope,
  AgentCapabilityQuery,
  AgentEventEnvelope,
  AgentCompactRequest,
  AgentCompactResponse,
  AgentPromptRequest,
  AgentSteerRequest,
  UiMessage,
  MessageRevisionSummary,
  AgentPromptResponse,
  AgentExecuteApprovedPlanRequest,
  AgentExecuteApprovedPlanResponse,
  PromptEnhancementRequest,
  PromptEnhancementResponse,
  SessionSummarizeTitleRequest,
  SessionSummarizeTitleResponse,
  DreamReport,
  MemoryStoreReport,
  AgentStopResponse,
  AgentQueueChangedEvent,
  AgentQueuePushRequest,
  QueuedTurnSummary,
  AgentStatus,
  AskToolResolution,
  AgentInstructionFile,
  AppSettings,
  CommandShellCatalog,
  AppVersionInfo,
  BrowserAction,
  BrowserState,
  CommandItem,
  ComposerCommand,
  ComposerPasteFile,
  ComposerPastedFile,
  FsEntry,
  FsImageDataUrlResult,
  FsIndexResult,
  FsReadResult,
  HostHealth,
  HostStatusEvent,
  MarketSource,
  McpCatalogEntry,
  SkillCatalogEntry,
  ModelInfo,
  McpServerInput,
  McpServerRecord,
  McpServerStatus,
  OnboardingState,
  OAuthLoginEvent,
  OAuthRespondInput,
  OAuthStartResult,
  OAuthVendor,
  PluginSummary,
  PluginSettingDefinition,
  PluginServiceStatus,
  PluginViewMeta,
  PluginTheme,
  MarketPluginSummary,
  MarketPluginDetail,
  PluginInstallResult,
  ProjectRecord,
  ProjectGroupRecord,
  ProjectMemory,
  ProjectMemoryEntry,
  ProjectWorkspace,
  PullRequestSummary,
  ScheduledTask,
  StaffCredentialProfile,
  StaffDataExportResult,
  StaffDataStats,
  StaffDecision,
  StaffDispatchScope,
  StaffEmployee,
  StaffGraph,
  StaffGraphValidation,
  StaffMemoryItem,
  StaffContent,
  StaffDirective,
  StaffLead,
  StaffMetric,
  StaffReportSummary,
  StaffPreset,
  StaffPresetInstallResult,
  StaffPruneResult,
  StaffRun,
  StaffSchedulerStatus,
  StaffWatchOutcome,
  StaffWatchStatus,
  StaffSuggestion,
  StaffSite,
  StaffSiteSignInResult,
  StaffCollectedPage,
  StaffWorkflow,
  StaffWorkflowVersion,
  StockAdviceItem,
  StockApiKeyStatus,
  StockPlan,
  StockRecommendResult,
  StockStrategy,
  StockWatchListResult,
  ProviderCreateInput,
  ProviderPublic,
  ProviderUpdateInput,
  Result,
  SessionDetail,
  SessionSummary,
  SessionCollaborationSummary,
  ToolPermissionResolution,
  UserSkillInput,
  UserSkillRecord,
  UserSubagentInput,
  UserSubagentRecord,
  SubagentDefinition,
  WorkspaceDiff,
  AppMenuCommand,
  AppNotification,
  NativeMenuAction,
  NotificationListResult,
  ReviewRollbackResult,
  PlanProposal,
  PlanResolveRequest,
  PlanResolutionResult,
  PlanningStateEvent,
  PlansPendingResult,
  UpdateState,
  WindowControlAction,
  CloseBehavior,
  TrustedExtensionStatusEvent,
  TrustedExtensionUiPrompt,
  TrustedExtensionUiPromptResponse,
} from "@pi-desktop/shared";
import {
  defaultCommandShellForPlatform,
  IPC,
  isCommandShellId,
  normalizeLargePasteThreshold,
  normalizeMode,
  normalizeNetworkProxy,
  resolveFontScale,
  validateNetworkProxy,
} from "@pi-desktop/shared";

export type ImportSource = "claude-code" | "opencode" | "codex" | "pi";
// One definition, owned by the shared package (the host and sidecar use the
// same shape); re-exported so existing renderer imports keep working.
import type {
  ModelConfigImportCandidate,
  ModelConfigImportSource,
} from "@pi-desktop/shared";
export type { ModelConfigImportCandidate, ModelConfigImportSource };

export interface ImportCandidate {
  source: ImportSource;
  externalId: string;
  title: string;
  projectPath: string | null;
  model: string | null;
  createdAt: string;
  updatedAt: string;
  /** null when the source file was too large to scan without sampling. */
  messageCount: number | null;
}

export interface ImportRunResult {
  imported: number;
  skipped: number;
  failed: number;
}

declare global {
  interface Window {
    piDesktop?: {
      invoke: <T = unknown>(channel: string, ...args: unknown[]) => Promise<Result<T>>;
      on: (channel: string, listener: (...args: unknown[]) => void) => () => void;
      channels: typeof IPC;
      platform: NodeJS.Platform;
      /** Authoritative OS locale passed from the main process at window creation. */
      locale?: string;
      /** Resolve a native dropped File to its source path. */
      getDroppedFilePath?: (file: File) => string | null;
    };
  }
}

/**
 * The staff calls that change rows, named one by one.
 *
 * The resident driver announces its own work, but a write the owner makes on a
 * page happens inside that page and announces nothing: the page that made it
 * refetches, while every other surface counting the same rows -- the badge on
 * the rail, the board next door -- keeps its old number until its own timer
 * comes round. Half a minute of a number that disagrees with the list under it
 * is long enough to read as a bug, so a write that lands raises a signal in
 * this window and `onStaffChanged` listens for it as well as for the host.
 *
 * Named rather than derived from the verb on the end: "status" is a write for a
 * piece of content and a read for the scheduler, and a list that confused the
 * two would refetch the whole console every time a page asked the time.
 */
const STAFF_WRITE_CHANNELS = new Set<string>([
  IPC.invoke.staffPresetsInstall,
  IPC.invoke.staffProfileCreate,
  IPC.invoke.staffProfileUpdate,
  IPC.invoke.staffProfileSetSecret,
  IPC.invoke.staffProfileDelete,
  IPC.invoke.staffWorkflowCreate,
  IPC.invoke.staffWorkflowUpdate,
  IPC.invoke.staffWorkflowDelete,
  IPC.invoke.staffEmployeeCreate,
  IPC.invoke.staffEmployeeUpdate,
  IPC.invoke.staffEmployeeDelete,
  IPC.invoke.staffEmployeeRunNow,
  IPC.invoke.staffMemoryWrite,
  IPC.invoke.staffMemoryPublish,
  IPC.invoke.staffMemorySupersede,
  IPC.invoke.staffMemoryRetire,
  IPC.invoke.staffSiteSignIn,
  IPC.invoke.staffSiteSignOut,
  IPC.invoke.staffSiteUpdate,
  IPC.invoke.staffLeadSave,
  IPC.invoke.staffLeadMove,
  IPC.invoke.staffContentSave,
  IPC.invoke.staffContentStatus,
  IPC.invoke.staffMetricSave,
  IPC.invoke.staffAdviceDerive,
  IPC.invoke.staffAdviceDecide,
  IPC.invoke.staffDirectiveUpdate,
  IPC.invoke.staffDataPrune,
  IPC.invoke.staffWatchRun,
]);

/** Raised in this window when a write the owner made here has landed. */
const STAFF_WRITE_EVENT = "pi:staff-write";

async function invoke<T>(channel: string, ...args: unknown[]): Promise<T> {
  if (!window.piDesktop?.invoke) {
    throw new Error("piDesktop preload bridge unavailable");
  }
  const result = await window.piDesktop.invoke<T>(channel, ...args);
  if (!result.ok) {
    const error = new Error(result.error.message) as Error & {
      code?: string;
      details?: unknown;
    };
    error.code = result.error.code;
    error.details = result.error.details;
    throw error;
  }
  // A failed call changed nothing, so nothing is announced: only a write that
  // actually landed moves the numbers the other pages are holding.
  if (STAFF_WRITE_CHANNELS.has(channel)) {
    window.dispatchEvent(new Event(STAFF_WRITE_EVENT));
  }
  return result.data;
}

function normalizeSession(session: SessionSummary): SessionSummary {
  return {
    ...session,
    mode: normalizeMode((session as { mode?: unknown }).mode),
  };
}

function normalizeSessionDetail(detail: SessionDetail | null): SessionDetail | null {
  return detail
    ? {
        ...detail,
        mode: normalizeMode((detail as { mode?: unknown }).mode),
      }
    : null;
}

export type SessionHistoryReadOptions = {
  /** Return the newest page ending before this zero-based message offset. */
  messageBefore?: number;
  /** Maximum number of messages in the returned page. */
  messageLimit?: number;
  /** Maximum characters per displayed message/value field. */
  contentLimit?: number;
};

export function normalizeSettings(settings: AppSettings): AppSettings {
  return {
    ...settings,
    defaultMode: normalizeMode((settings as { defaultMode?: unknown }).defaultMode),
    defaultCommandShell: isCommandShellId(
      (settings as { defaultCommandShell?: unknown }).defaultCommandShell,
    )
      ? (settings as { defaultCommandShell: AppSettings["defaultCommandShell"] })
          .defaultCommandShell
      : defaultCommandShellForPlatform(window.piDesktop?.platform ?? ""),
    largePasteThreshold: normalizeLargePasteThreshold(
      (settings as { largePasteThreshold?: unknown }).largePasteThreshold,
    ),
    fontScale: resolveFontScale(settings),
    networkProxy: normalizeNetworkProxy(
      (settings as { networkProxy?: unknown }).networkProxy,
    ),
  };
}

export function validateSettingsWrite(settings: AppSettings): AppSettings {
  const value = settings as AppSettings & {
    defaultCommandShell?: unknown;
    largePasteThreshold?: unknown;
    fontScale?: unknown;
    networkProxy?: unknown;
  };
  if (
    Object.prototype.hasOwnProperty.call(value, "defaultCommandShell") &&
    !isCommandShellId(value.defaultCommandShell)
  ) {
    throw Object.assign(new Error("defaultCommandShell is invalid"), {
      errorCode: "COMMAND_SHELL_INVALID",
    });
  }
  if (
    Object.prototype.hasOwnProperty.call(value, "largePasteThreshold") &&
    normalizeLargePasteThreshold(value.largePasteThreshold) !==
      value.largePasteThreshold
  ) {
    throw Object.assign(new Error("largePasteThreshold is invalid"), {
      errorCode: "INVALID_PARAMS",
    });
  }
  if (
    Object.prototype.hasOwnProperty.call(value, "fontScale") &&
    resolveFontScale({ fontScale: value.fontScale }) !== value.fontScale
  ) {
    throw Object.assign(new Error("fontScale is invalid"), {
      errorCode: "INVALID_PARAMS",
    });
  }
  if (Object.prototype.hasOwnProperty.call(value, "networkProxy")) {
    const proxy = validateNetworkProxy(value.networkProxy);
    if (!proxy.ok) {
      throw Object.assign(new Error(proxy.error), {
        errorCode: "INVALID_ARGUMENT",
      });
    }
    value.networkProxy = proxy.value;
  }
  return settings;
}

function normalizePlanProposal(proposal: PlanProposal): PlanProposal {
  return { ...proposal };
}

function normalizePendingPlans(result: PlansPendingResult): PlansPendingResult {
  return {
    ...result,
    plans: result.plans.map(normalizePlanProposal),
  };
}

function normalizePlansChangedEvent(value: unknown): PlanningStateEvent {
  const raw = (value ?? {}) as PlanningStateEvent & {
    execution?: {
      id?: unknown;
      proposalId?: unknown;
      state?: unknown;
    };
  };
  const execution = raw.execution;
  if (!execution) return raw;
  const executionState =
    execution.state === "queued" ||
    execution.state === "running" ||
    execution.state === "completed" ||
    execution.state === "interrupted"
      ? execution.state
      : undefined;
  return {
    ...raw,
    ...(raw.proposalId || typeof execution.proposalId !== "string"
      ? {}
      : { proposalId: execution.proposalId }),
    ...(raw.executionId || typeof execution.id !== "string"
      ? {}
      : { executionId: execution.id }),
    ...(raw.executionState || !executionState
      ? {}
      : { executionState }),
  };
}

export const api = {
  getVersion: () => invoke<AppVersionInfo>(IPC.invoke.appGetVersion),
  health: () => invoke<HostHealth>(IPC.invoke.appHealth),
  getOnboarding: () => invoke<OnboardingState>(IPC.invoke.appGetOnboarding),
  dismissOnboarding: () => invoke(IPC.invoke.appDismissOnboarding),
  updatesGetState: () => invoke<UpdateState>(IPC.invoke.updatesGetState),
  updatesCheck: () => invoke<UpdateState>(IPC.invoke.updatesCheck),
  updatesDownload: () => invoke<UpdateState>(IPC.invoke.updatesDownload),
  updatesInstall: () => invoke(IPC.invoke.updatesInstall),
  updatesOpenReleases: () => invoke(IPC.invoke.updatesOpenReleases),
  listNotifications: (input?: { unreadOnly?: boolean; limit?: number }) =>
    invoke<NotificationListResult>(IPC.invoke.notificationList, input ?? {}),
  markNotificationRead: (id: string) =>
    invoke<{ ok: boolean }>(IPC.invoke.notificationMarkRead, { id }),
  markAllNotificationsRead: () =>
    invoke<{ ok: boolean }>(IPC.invoke.notificationMarkAllRead),
  clearNotifications: () =>
    invoke<{ ok: boolean }>(IPC.invoke.notificationClear),
  showNativeNotification: (input: {
    id: string;
    sessionId: string;
    kind: "task" | "interactive";
    title: string;
    body: string;
  }) => invoke<{ shown: boolean }>(IPC.invoke.notificationShowNative, input),
  setNotificationViewingSession: (sessionId: string | null) =>
    invoke<{ ok: boolean }>(IPC.invoke.notificationSetViewingSession, {
      sessionId,
    }),
  listSessions: () =>
    invoke<{ sessions: SessionSummary[] }>(IPC.invoke.sessionList).then((result) => ({
      ...result,
      sessions: result.sessions.map(normalizeSession),
    })),
  createSession: (input?: Partial<SessionSummary>) =>
    invoke<{ session: SessionSummary }>(IPC.invoke.sessionCreate, input ?? {}).then(
      (result) => ({ ...result, session: normalizeSession(result.session) }),
    ),
  forkSession: (sessionId: string, title?: string, throughMessageId?: string) =>
    invoke<{ session: SessionDetail }>(IPC.invoke.sessionFork, {
      sessionId,
      title,
      throughMessageId,
    }).then((result) => ({ ...result, session: normalizeSessionDetail(result.session)! })),
  getSession: (id: string, options?: SessionHistoryReadOptions) =>
    invoke<{ session: SessionDetail | null }>(IPC.invoke.sessionGet, {
      id,
      ...(options ?? {}),
    }).then((result) => ({
      ...result,
      session: normalizeSessionDetail(result.session),
    })),
  getSessionCollaboration: (sessionId: string) =>
    invoke<SessionCollaborationSummary>(IPC.invoke.sessionCollaboration, { sessionId }),
  deleteSession: (id: string) => invoke(IPC.invoke.sessionDelete, id),
  getSessionScratchPath: (sessionId: string) =>
    invoke<{ path: string }>(IPC.invoke.sessionGetScratchPath, { sessionId }),
  openSessionScratchPath: (sessionId: string) =>
    invoke<{ ok: boolean; path: string }>(IPC.invoke.sessionOpenScratchPath, {
      sessionId,
    }),
  openProjectFolder: (path: string) =>
    invoke<{ ok: boolean; path: string }>(IPC.invoke.projectOpenFolder, path),
  renameSession: (id: string, title: string) =>
    invoke<{ ok: boolean }>(IPC.invoke.sessionRename, id, title),
  moveSessionProject: (sessionId: string, projectPath: string) =>
    invoke<{ session: SessionSummary }>(IPC.invoke.sessionMoveProject, {
      sessionId,
      projectPath,
    }).then((result) => ({ ...result, session: normalizeSession(result.session) })),
  summarizeSessionTitle: (req: SessionSummarizeTitleRequest) =>
    invoke<SessionSummarizeTitleResponse>(IPC.invoke.sessionSummarizeTitle, req),
  configureSession: (
    id: string,
    config: Pick<SessionSummary, "mode" | "providerId" | "modelId"> &
      Partial<Pick<SessionSummary, "thinkingLevel" | "permissionMode">>,
  ) =>
    invoke<{ session: SessionSummary }>(
      IPC.invoke.sessionConfigure,
      id,
      config,
    ).then((result) => ({ ...result, session: normalizeSession(result.session) })),
  scanImportSessions: () =>
    invoke<{ sessions: ImportCandidate[] }>(IPC.invoke.sessionImportScan),
  runImportSessions: (items: ImportCandidate[]) =>
    invoke<ImportRunResult>(IPC.invoke.sessionImportRun, items),
  /**
   * CLI /export parity: the host writes the session as pigo-compatible JSONL
   * through its own save dialog, so the renderer only supplies the session id.
   */
  exportSession: (sessionId: string) =>
    invoke<{
      saved: boolean;
      path?: string;
      bytes?: number;
      messages?: number;
    }>(IPC.invoke.sessionExport, { sessionId }),
  scanImportModelConfigs: () =>
    invoke<{ providers: ModelConfigImportCandidate[] }>(
      IPC.invoke.modelConfigImportScan,
    ),
  runImportModelConfigs: (items: ModelConfigImportCandidate[]) =>
    invoke<ImportRunResult>(IPC.invoke.modelConfigImportRun, items),
  getSettings: () => invoke<AppSettings>(IPC.invoke.settingsGet).then(normalizeSettings),
  setSettings: (settings: AppSettings) =>
    invoke(IPC.invoke.settingsSet, validateSettingsWrite(settings)),
  testNetworkProxy: (settings: unknown) =>
    invoke<{ ok: boolean; error?: string }>(IPC.invoke.networkProxyTest, settings),
  /** Installed system font families (Electron main, cached briefly). */
  listSystemFonts: () => invoke<string[]>(IPC.invoke.systemFontsList),
  listCommandShells: () =>
    invoke<CommandShellCatalog>(IPC.invoke.commandShellList),
  listProviders: () =>
    invoke<{ providers: ProviderPublic[] }>(IPC.invoke.providersList),
  createProvider: (input: ProviderCreateInput) =>
    invoke<{ provider: ProviderPublic }>(IPC.invoke.providersCreate, input),
  updateProvider: (input: ProviderUpdateInput) =>
    invoke<{ provider: ProviderPublic | null }>(
      IPC.invoke.providersUpdate,
      input,
    ),
  deleteProvider: (id: string) => invoke(IPC.invoke.providersDelete, id),
  testProvider: (id: string) => invoke(IPC.invoke.providersTest, id),
  /**
   * Discover models from the provider's own endpoint. Saved providers pass
   * providerId (stored secret is reused); the setup form may pass raw config
   * before the provider exists.
   *
   * `source` reports where the list came from: `remote` is the service's own
   * answer, `catalog` means the endpoint published nothing and models.dev was
   * used instead, `cache` is the local table, `fallback` is just the configured
   * model id.
   */
  listProviderModels: (input: {
    providerId?: string;
    baseUrl?: string;
    apiKey?: string;
    apiStyle?: string;
    headers?: Record<string, string>;
    source?: "cache" | "refresh";
  }) =>
    invoke<{
      models: ModelInfo[];
      source: "cache" | "remote" | "catalog" | "fallback";
      error?: string;
    }>(IPC.invoke.providersListModels, input),
  /** Force-refresh models.dev for the running process; release snapshots are bundled. */
  refreshModelCatalog: () =>
    invoke<{
      refreshed: boolean;
      status: {
        loaded: boolean;
        source: "bundled" | "remote" | "empty";
        catalogPath: string;
        fetchedAt?: string;
        providerCount: number;
        modelCount: number;
        lastError?: string;
      };
    }>(IPC.invoke.providersRefreshModelCatalog),
  /** models.dev snapshot status for the settings footer; performs no network I/O. */
  modelCatalogStatus: () =>
    invoke<{
      status: {
        loaded: boolean;
        source: "bundled" | "remote" | "empty";
        catalogPath: string;
        fetchedAt?: string;
        providerCount: number;
        modelCount: number;
        lastError?: string;
      };
    }>(IPC.invoke.providersModelCatalogStatus),
  /** Vendor catalog plus every locally configured account for each vendor. */
  listOauthVendors: () =>
    invoke<{ vendors: OAuthVendor[] }>(IPC.invoke.providersOauthVendors),
  /** Begin a login; progress arrives through `onOauthLogin`. */
  startOauthLogin: (vendorId: string) =>
    invoke<OAuthStartResult>(IPC.invoke.providersOauthStart, vendorId),
  /** Answer a prompt; omitting the value cancels the login. */
  respondOauthLogin: (input: OAuthRespondInput) =>
    invoke<{ ok: boolean }>(IPC.invoke.providersOauthRespond, input),
  cancelOauthLogin: (loginId: string) =>
    invoke<{ ok: boolean }>(IPC.invoke.providersOauthCancel, loginId),
  deleteOauthAccount: (providerId: string) =>
    invoke<{ ok: boolean }>(IPC.invoke.providersOauthDelete, providerId),
  getProject: () =>
    invoke<{ workspace: ProjectWorkspace | null }>(IPC.invoke.projectGet),
  listProjects: () =>
    invoke<{ projects: ProjectRecord[] }>(IPC.invoke.projectList),
  listProjectGroups: () =>
    invoke<{ groups: ProjectGroupRecord[] }>(IPC.invoke.projectGroupList),
  createProjectGroup: (name: string, folders: string[]) =>
    invoke<{ group: ProjectGroupRecord }>(IPC.invoke.projectGroupCreate, { name, folders }),
  renameProjectGroup: (groupId: string, name: string) =>
    invoke<{ group: ProjectGroupRecord }>(IPC.invoke.projectGroupRename, { groupId, name }),
  updateProjectGroup: (groupId: string, name: string, folders: string[]) =>
    invoke<{ group: ProjectGroupRecord }>(IPC.invoke.projectGroupUpdate, {
      groupId,
      name,
      folders,
    }),
  getProjectGroupMemory: (groupId: string) =>
    invoke<{ memory: ProjectMemory }>(IPC.invoke.projectGroupMemoryGet, { groupId }),
  saveProjectGroupMemory: (groupId: string, entries: ProjectMemory["entries"]) =>
    invoke<{ memory: ProjectMemory }>(IPC.invoke.projectGroupMemorySave, { groupId, entries }),
  getProjectGroupInstructions: (groupId: string) =>
    invoke<{ content: string }>(IPC.invoke.projectGroupInstructionsGet, { groupId }),
  saveProjectGroupInstructions: (groupId: string, content: string) =>
    invoke<{ content: string }>(IPC.invoke.projectGroupInstructionsSave, { groupId, content }),
  openProject: () =>
    invoke<{ workspace: ProjectWorkspace | null; canceled?: boolean }>(
      IPC.invoke.projectOpen,
    ),
  pickProjectFolders: () =>
    invoke<{ folders: string[]; canceled?: boolean }>(IPC.invoke.projectPickFolders),
  getProjectMemory: (projectPath: string) =>
    invoke<{ memory: ProjectMemory }>(IPC.invoke.projectMemoryGet, { projectPath }),
  saveProjectMemory: (projectPath: string, entries: ProjectMemoryEntry[]) =>
    invoke<{ memory: ProjectMemory }>(IPC.invoke.projectMemorySave, {
      projectPath,
      entries,
    }),
  cloneProject: (url: string) =>
    invoke<{ workspace: ProjectWorkspace | null; canceled?: boolean }>(
      IPC.invoke.projectClone,
      { url },
    ),
  pickFiles: () =>
    invoke<{ token: string | null; canceled?: boolean }>(IPC.invoke.composerPickFiles),
  getDroppedFilePath: (file: File) =>
    window.piDesktop?.getDroppedFilePath?.(file) ?? null,
  pickPhotos: () =>
    invoke<{ token: string | null; canceled?: boolean }>(IPC.invoke.composerPickPhotos),
  importFiles: (sessionId: string, token: string) =>
    invoke<{ files: ComposerPastedFile[] }>(IPC.invoke.composerImportFiles, {
      sessionId,
      token,
    }),
  pasteFiles: (sessionId: string, files: ComposerPasteFile[]) =>
    invoke<{ files: ComposerPastedFile[] }>(IPC.invoke.composerPasteFiles, {
      sessionId,
      files,
    }),
  recordClipboardPaste: (text: string) =>
    invoke<{ ok: boolean }>(IPC.invoke.clipboardRecordPaste, { text }),
  clearProject: () => invoke(IPC.invoke.projectClear),
  setProject: (path: string) =>
    invoke<{ workspace: ProjectWorkspace | null }>(IPC.invoke.projectSet, path),
  listPullRequests: () =>
    invoke<{ pulls: PullRequestSummary[]; error?: string }>(IPC.invoke.pullsList),
  listScheduled: () =>
    invoke<{ tasks: ScheduledTask[] }>(IPC.invoke.scheduledList),
  createScheduled: (input: {
    title?: string;
    prompt: string;
    cadence?: ScheduledTask["cadence"];
    enabled?: boolean;
  }) => invoke<{ task: ScheduledTask }>(IPC.invoke.scheduledCreate, input),
  updateScheduled: (input: Partial<ScheduledTask> & { id: string }) =>
    invoke<{ task: ScheduledTask }>(IPC.invoke.scheduledUpdate, input),
  deleteScheduled: (id: string) => invoke(IPC.invoke.scheduledDelete, id),
  runScheduled: (id: string) =>
    invoke<{ sessionId: string; prompt: string; task: ScheduledTask }>(
      IPC.invoke.scheduledRun,
      id,
    ),
  getStocksStatus: () => invoke<StockApiKeyStatus>(IPC.invoke.stocksStatus),
  setStocksApiKey: (key: string) =>
    invoke<StockApiKeyStatus>(IPC.invoke.stocksApiKeySet, { key }),
  getStocksOverview: (strategy: StockStrategy) =>
    invoke<StockRecommendResult>(IPC.invoke.stocksOverview, { strategy }),
  getStocksPlan: () => invoke<StockPlan>(IPC.invoke.stocksPlan),
  listStocksWatchlist: () => invoke<StockWatchListResult>(IPC.invoke.stocksWatchList),
  addStocksWatch: (input: { query: string; note?: string; stop?: number; target?: number }) =>
    invoke<{ added: boolean; code: string; name: string; reason?: string }>(
      IPC.invoke.stocksWatchAdd,
      input,
    ),
  removeStocksWatch: (code: string) =>
    invoke<{ removed: boolean }>(IPC.invoke.stocksWatchRemove, { code }),
  listStaffProfiles: () =>
    invoke<{ profiles: StaffCredentialProfile[] }>(IPC.invoke.staffProfilesList),
  createStaffProfile: (input: {
    label: string;
    providerId: string;
    baseUrl?: string;
    secretValue?: string;
  }) => invoke<{ profile: StaffCredentialProfile }>(IPC.invoke.staffProfileCreate, input),
  updateStaffProfile: (input: {
    id: string;
    label?: string;
    baseUrl?: string;
  }) => invoke<{ profile: StaffCredentialProfile }>(IPC.invoke.staffProfileUpdate, input),
  setStaffProfileSecret: (id: string, secretValue: string) =>
    invoke<{ profile: StaffCredentialProfile }>(IPC.invoke.staffProfileSetSecret, {
      id,
      secretValue,
    }),
  deleteStaffProfile: (id: string) =>
    invoke<{ deleted: boolean }>(IPC.invoke.staffProfileDelete, { id }),
  listStaffWorkflows: () =>
    invoke<{ workflows: StaffWorkflow[] }>(IPC.invoke.staffWorkflowsList),
  getStaffWorkflow: (id: string, version?: number) =>
    invoke<{ workflow: StaffWorkflow; version: number; graph: StaffGraph | null }>(
      IPC.invoke.staffWorkflowGet,
      version === undefined ? { id } : { id, version },
    ),
  createStaffWorkflow: (input: {
    name: string;
    description?: string;
    status?: StaffWorkflow["status"];
    graph: StaffGraph;
  }) => invoke<{ workflow: StaffWorkflow }>(IPC.invoke.staffWorkflowCreate, input),
  updateStaffWorkflow: (input: {
    id: string;
    name?: string;
    description?: string;
    status?: StaffWorkflow["status"];
    graph?: StaffGraph;
  }) => invoke<{ workflow: StaffWorkflow }>(IPC.invoke.staffWorkflowUpdate, input),
  listStaffWorkflowVersions: (id: string) =>
    invoke<{ versions: StaffWorkflowVersion[] }>(IPC.invoke.staffWorkflowVersions, { id }),
  deleteStaffWorkflow: (id: string) =>
    invoke<{ deleted: boolean }>(IPC.invoke.staffWorkflowDelete, { id }),
  validateStaffWorkflow: (graph: StaffGraph) =>
    invoke<StaffGraphValidation>(IPC.invoke.staffWorkflowValidate, { graph }),
  listStaffPresets: () => invoke<{ presets: StaffPreset[] }>(IPC.invoke.staffPresetsList),
  /** Move hired employees onto this build's copy of their blueprint. */
  adoptStaffBlueprint: (ids: string[]) =>
    invoke<StaffPresetInstallResult>(IPC.invoke.staffPresetsInstall, {
      ids,
      adoptBlueprint: true,
    }),
  listStaffEmployees: () => invoke<{ employees: StaffEmployee[] }>(IPC.invoke.staffEmployeesList),
  getStaffEmployee: (id: string) =>
    invoke<{ employee: StaffEmployee }>(IPC.invoke.staffEmployeeGet, { id }),
  createStaffEmployee: (input: Record<string, unknown>) =>
    invoke<{ employee: StaffEmployee }>(IPC.invoke.staffEmployeeCreate, input),
  updateStaffEmployee: (input: Record<string, unknown> & { id: string }) =>
    invoke<{ employee: StaffEmployee }>(IPC.invoke.staffEmployeeUpdate, input),
  deleteStaffEmployee: (id: string) =>
    invoke<{ deleted: boolean }>(IPC.invoke.staffEmployeeDelete, { id }),
  listStaffRuns: (employeeId?: string, limit = 50) =>
    invoke<{ runs: StaffRun[] }>(IPC.invoke.staffEmployeeRuns, {
      ...(employeeId ? { employeeId } : {}),
      limit,
    }),
  runStaffEmployeeNow: (employeeId: string) =>
    invoke<{ run: StaffRun; sessionId?: string }>(IPC.invoke.staffEmployeeRunNow, employeeId, {}),
  refreshStaffSchedule: (id: string) =>
    invoke<{ nextRunAt: string | null }>(IPC.invoke.staffEmployeeScheduleRefresh, { id }),
  staffSchedulerStatus: () =>
    invoke<StaffSchedulerStatus>(IPC.invoke.staffSchedulerStatus),
  staffWatchStatus: () =>
    invoke<StaffWatchStatus>(IPC.invoke.staffWatchStatus),
  staffWatchRun: () => invoke<StaffWatchOutcome>(IPC.invoke.staffWatchRun),
  listStaffSites: () => invoke<{ sites: StaffSite[] }>(IPC.invoke.staffSitesList),
  signInStaffSite: (channel: string) =>
    invoke<StaffSiteSignInResult>(IPC.invoke.staffSiteSignIn, channel),
  signOutStaffSite: (channel: string) =>
    invoke<{ signedIn: false }>(IPC.invoke.staffSiteSignOut, channel),
  updateStaffSite: (
    channel: string,
    patch: { seedUrl?: string; maxUrls?: number; keywords?: string[] },
  ) =>
    invoke<StaffSite>(IPC.invoke.staffSiteUpdate, channel, patch),
  collectStaffSite: (channel: string) =>
    invoke<{ pages: StaffCollectedPage[] }>(IPC.invoke.staffSiteCollect, channel),
  queryStaffMemory: (filter: {
    scope?: string;
    ownerId?: string;
    kind?: string;
    limit?: number;
  }) => invoke<{ items: StaffMemoryItem[] }>(IPC.invoke.staffMemoryQuery, filter),
  writeStaffMemory: (input: Record<string, unknown>) =>
    invoke<{ item: StaffMemoryItem }>(IPC.invoke.staffMemoryWrite, input),
  publishStaffMemory: (input: Record<string, unknown>) =>
    invoke<{ item: StaffMemoryItem }>(IPC.invoke.staffMemoryPublish, input),
  retireStaffMemory: (id: string) =>
    invoke<{ retired: boolean }>(IPC.invoke.staffMemoryRetire, { id }),
  listStaffLeads: (filter: { stage?: string; site?: string; limit?: number } = {}) =>
    invoke<{ leads: StaffLead[]; counts: Record<string, number> }>(
      IPC.invoke.staffLeadsList,
      filter,
    ),
  saveStaffLead: (input: Record<string, unknown>) =>
    invoke<{ lead: StaffLead }>(IPC.invoke.staffLeadSave, input),
  moveStaffLead: (input: { id: string; stage: string; note?: string; ownerId?: string | null }) =>
    invoke<{ lead: StaffLead }>(IPC.invoke.staffLeadMove, input),
  listStaffContents: (
    filter: { status?: string; channel?: string; topic?: string; leadId?: string; limit?: number } = {},
  ) =>
    invoke<{ contents: StaffContent[]; counts: Record<string, number> }>(
      IPC.invoke.staffContentsList,
      filter,
    ),
  saveStaffContent: (input: Record<string, unknown>) =>
    invoke<{ content: StaffContent }>(IPC.invoke.staffContentSave, input),
  setStaffContentStatus: (input: {
    id: string;
    status: string;
    body?: string;
    channel?: string;
    publishedUrl?: string;
  }) => invoke<{ content: StaffContent }>(IPC.invoke.staffContentStatus, input),
  listStaffMetrics: (filter: { dimension?: string; since?: string; day?: string } = {}) =>
    invoke<{ metrics: StaffMetric[] }>(IPC.invoke.staffMetricsList, filter),
  saveStaffMetric: (input: Record<string, unknown>) =>
    invoke<{ metric: StaffMetric }>(IPC.invoke.staffMetricSave, input),
  staffReportSummary: (input: { day?: string; days?: number } = {}) =>
    invoke<StaffReportSummary>(IPC.invoke.staffReportSummary, input),
  /** What each station in the loop holds, in bytes and in rows. */
  staffDataStats: () => invoke<StaffDataStats>(IPC.invoke.staffDataStats),
  /**
   * Take the stages the owner picked. A dry run measures and reports without
   * deleting, which is what the page offers first.
   */
  pruneStaffData: (input: { rules: string[]; dryRun?: boolean }) =>
    invoke<StaffPruneResult>(IPC.invoke.staffDataPrune, input),
  /**
   * Write the ledger out through the host's own save dialog.
   *
   * A download link in the page cannot say which folder to use, so the file
   * either lands somewhere nobody looked or does not land at all; asking the
   * main process answers with the path it wrote.
   */
  exportStaffData: (input: { name: string; body: string }) =>
    invoke<StaffDataExportResult>(IPC.invoke.staffDataExport, input),
  listStaffAdvice: (filter: { board?: string; status?: string } = {}) =>
    invoke<{ suggestions: StaffSuggestion[] }>(IPC.invoke.staffAdviceList, filter),
  deriveStaffAdvice: () =>
    invoke<{ raised: StaffSuggestion[] }>(IPC.invoke.staffAdviceDerive, {}),
  decideStaffAdvice: (input: {
    id: string;
    decision: "accepted" | "rejected";
    note?: string;
    directive?: { employeeId?: string; employeeName?: string; verb?: string };
  }) => invoke<{ suggestion: StaffSuggestion }>(IPC.invoke.staffAdviceDecide, input),
  listStaffDirectives: (
    // `issuerEmployeeId` is the one that answers "what did this manager hand
    // out": an employee's own orders and the orders it gave are different lists.
    filter: {
      status?: string;
      employeeId?: string;
      board?: string;
      issuerEmployeeId?: string;
      limit?: number;
    } = {},
  ) =>
    invoke<{ directives: StaffDirective[] }>(IPC.invoke.staffDirectivesList, filter),
  updateStaffDirective: (input: {
    id: string;
    status?: string;
    resultNote?: string;
  }) => invoke<{ directive: StaffDirective }>(IPC.invoke.staffDirectiveUpdate, input),
  listStaffDecisions: (filter: { board?: string; employeeId?: string; limit?: number } = {}) =>
    invoke<{ decisions: StaffDecision[] }>(IPC.invoke.staffDecisionsList, filter),
  staffDispatchScope: (employeeId: string) =>
    invoke<StaffDispatchScope>(IPC.invoke.staffDispatchScope, { employeeId }),
  supersedeStaffMemory: (input: Record<string, unknown> & { id: string }) =>
    invoke<{ item: StaffMemoryItem }>(IPC.invoke.staffMemorySupersede, input),
  replaceSessionMessages: (sessionId: string, messages: UiMessage[]) =>
    invoke(IPC.invoke.sessionReplaceMessages, { sessionId, messages }),
  saveSessionRevision: (input: {
    sessionId: string;
    rootUserId: string;
    messages: UiMessage[];
    makeActive?: boolean;
  }) =>
    invoke<{ revision: MessageRevisionSummary }>(IPC.invoke.sessionSaveRevision, input),
  listSessionRevisions: (sessionId: string, rootUserId: string) =>
    invoke<{ revisions: MessageRevisionSummary[] }>(IPC.invoke.sessionListRevisions, {
      sessionId,
      rootUserId,
    }),
  activateSessionRevision: (input: {
    sessionId: string;
    rootUserId: string;
    revisionIndex: number;
    prefix: UiMessage[];
  }) =>
    invoke<{ messages: UiMessage[] }>(IPC.invoke.sessionActivateRevision, input),
  steer: (req: AgentSteerRequest) =>
    invoke<AgentPromptResponse>(IPC.invoke.agentSteer, req),
  prompt: (req: AgentPromptRequest) =>
    invoke<AgentPromptResponse>(IPC.invoke.agentPrompt, req),
  executeApprovedPlan: (req: AgentExecuteApprovedPlanRequest) =>
    invoke<AgentExecuteApprovedPlanResponse>(
      IPC.invoke.agentExecuteApprovedPlan,
      req,
    ),
  enhancePrompt: (req: PromptEnhancementRequest) =>
    invoke<PromptEnhancementResponse>(IPC.invoke.promptEnhance, req),
  compact: (req: AgentCompactRequest) =>
    invoke<AgentCompactResponse>(IPC.invoke.agentCompact, req),
  abort: (sessionId: string) =>
    invoke(IPC.invoke.agentAbort, { sessionId }),
  stop: (sessionId: string) =>
    invoke<AgentStopResponse>(IPC.invoke.agentStop, { sessionId }),
  queuePrompt: (req: AgentQueuePushRequest) =>
    invoke<QueuedTurnSummary>(IPC.invoke.agentQueuePush, req),
  listQueuedPrompts: (sessionId: string) =>
    invoke<{ entries: QueuedTurnSummary[] }>(IPC.invoke.agentQueueList, { sessionId }),
  removeQueuedPrompt: (turnId: string) =>
    invoke(IPC.invoke.agentQueueRemove, { turnId }),
  prioritizeQueuedPrompt: (turnId: string) =>
    invoke(IPC.invoke.agentQueuePrioritize, { turnId }),
  getStatus: (sessionId: string) =>
    invoke<{ status: AgentStatus }>(IPC.invoke.agentGetStatus, sessionId),
  /** CLI /rebuild parity: discard and reload the live runtime's context. */
  rebuildSession: (sessionId: string) =>
    invoke<{ ok: boolean; rebuilt: boolean }>(IPC.invoke.agentRebuild, {
      sessionId,
    }),
  /**
   * CLI /memory parity: the persistent-memory store state and per-scope counts.
   * Read-only — the sidecar reconciles the on-disk files into the index first.
   */
  reportMemory: () =>
    invoke<MemoryStoreReport>(IPC.invoke.memoryReport, {}),
  /**
   * CLI /dream parity: one memory-consolidation pass with the session's own
   * model. Slow (several model calls); the caller shows a busy state.
   */
  runDream: (sessionId: string, dryRun = false) =>
    invoke<{ ok: boolean; report: DreamReport }>(IPC.invoke.agentDream, {
      sessionId,
      dryRun,
    }),
  getAgentInstructions: (projectPath?: string) =>
    invoke<{ global: AgentInstructionFile; project?: AgentInstructionFile }>(
      IPC.invoke.agentInstructionsGet,
      projectPath === undefined ? {} : { projectPath },
    ),
  saveAgentInstructions: (
    scope: AgentInstructionFile["scope"],
    content: string,
    projectPath?: string,
  ) =>
    invoke<{ file: AgentInstructionFile }>(IPC.invoke.agentInstructionsSave, {
      scope,
      content,
      ...(projectPath === undefined ? {} : { projectPath }),
    }),
  resolvePermission: (resolution: ToolPermissionResolution) =>
    invoke(IPC.invoke.toolResolvePermission, resolution),
  resolveAskTool: (resolution: AskToolResolution) =>
    invoke(IPC.invoke.askToolResolve, resolution),
  /** Ask one out-of-band /btw question; the answer arrives as scoped events. */
  btw: (input: { sessionId: string; sideId: string; question: string }) =>
    invoke<{ accepted: boolean; sideId: string }>(IPC.invoke.agentBtw, input),
  btwCancel: (sideId: string) =>
    invoke<{ cancelled: boolean }>(IPC.invoke.agentBtwCancel, { sideId }),
  pendingPlans: (sessionId?: string) =>
    invoke<PlansPendingResult>(
      IPC.invoke.plansPending,
      sessionId ? { sessionId } : {},
    ).then(normalizePendingPlans),
  resolvePlan: (resolution: PlanResolveRequest) =>
    invoke<PlanResolutionResult>(IPC.invoke.plansResolve, resolution),
  listPlugins: () =>
    invoke<{ plugins: PluginSummary[] }>(IPC.invoke.pluginList),
  loadDevPlugin: () =>
    invoke<{ canceled?: boolean; plugin?: unknown }>(IPC.invoke.pluginLoadDev),
  reloadPlugin: (id: string) => invoke(IPC.invoke.pluginReload, id),
  createPluginFromTemplate: (template: string) =>
    invoke<{
      canceled?: boolean;
      id?: string;
      name?: string;
      dir?: string;
      files?: string[];
    }>(IPC.invoke.pluginCreateFromTemplate, { template }),
  installPluginFromPath: () => invoke(IPC.invoke.pluginInstallFromPath),
  installPluginFromPackage: () => invoke(IPC.invoke.pluginInstallFromPackage),
  enablePlugin: (id: string) => invoke(IPC.invoke.pluginEnable, id),
  disablePlugin: (id: string) => invoke(IPC.invoke.pluginDisable, id),
  uninstallPlugin: (id: string) => invoke(IPC.invoke.pluginUninstall, id),
  setPluginAutoUpdate: (id: string, enabled: boolean) =>
    invoke(IPC.invoke.pluginSetAutoUpdate, { id, enabled }),
  getPluginSettings: (id: string) =>
    invoke<{ settings: PluginSettingDefinition[] }>(IPC.invoke.pluginSettingsGet, id),
  setPluginSettings: (id: string, settings: Record<string, unknown>) =>
    invoke<{ settings: PluginSettingDefinition[] }>(IPC.invoke.pluginSettingsSet, {
      id,
      settings,
    }),
  /**
   * Where a plugin's contributions apply. Separate from enable/disable so
   * narrowing a plugin to two projects and then switching it off keeps the list.
   */
  setPluginScope: (id: string, scope: ActivationScope) =>
    invoke<{ plugin?: PluginSummary }>(IPC.invoke.pluginSetScope, { id, scope }),

  // --- MCP servers the user owns -------------------------------------------
  listMcpServers: (query?: AgentCapabilityQuery) =>
    invoke<{ servers: McpServerRecord[]; statuses: McpServerStatus[] }>(
      IPC.invoke.mcpList,
      query,
    ),
  /** Create or replace a server; the id decides which level-local file. */
  upsertMcpServer: (server: McpServerInput) =>
    invoke<{ server: McpServerRecord }>(IPC.invoke.mcpUpsert, server),
  removeMcpServer: (
    id: string,
    query?: Partial<AgentCapabilityQuery>,
  ) => invoke(IPC.invoke.mcpRemove, { id, ...query }),
  setMcpServerEnabled: (
    id: string,
    enabled: boolean,
    query?: Partial<AgentCapabilityQuery>,
  ) => invoke(IPC.invoke.mcpSetEnabled, { id, enabled, ...query }),
  setMcpServerScope: (id: string, scope: ActivationScope) =>
    invoke(IPC.invoke.mcpSetScope, { id, scope }),
  /** Force one handshake and report what happened, for the editor's test button. */
  testMcpServer: (id: string, query?: Partial<AgentCapabilityQuery>) =>
    invoke<{ status: McpServerStatus }>(IPC.invoke.mcpTest, { id, ...query }),
  /** Accept a pasted `mcpServers` block; bad entries are reported, not fatal. */
  importMcpServers: (text: string) =>
    invoke<{
      imported: McpServerRecord[];
      failed: Array<{ id: string; reason: string }>;
    }>(IPC.invoke.mcpImport, { text }),
  /** Query the configured market sources; `failedSources` names dead ones. */
  searchMcpMarketRegistry: (query: string, sources: MarketSource[], options?: { more?: boolean }) =>
    invoke<{ entries: McpCatalogEntry[]; failedSources?: string[]; exhausted?: boolean }>(
      IPC.invoke.mcpMarketSearch,
      { query, sources, ...options },
    ),

  // --- Skill market ----------------------------------------------------------
  searchSkillMarket: (query: string, sources: { id: string; name: string; url: string }[]) =>
    invoke<{ entries: SkillCatalogEntry[]; failedSources?: string[] }>(
      IPC.invoke.skillMarketSearch,
      { query, sources },
    ),
  /** Fetch one catalog document (frontmatter split off) for preview/install. */
  fetchSkillMarketDocument: (entry: SkillCatalogEntry) =>
    invoke<{ name?: string; description?: string; body: string; resources?: Array<{ path: string; body: string }> }>(
      IPC.invoke.skillMarketFetch,
      { entry },
    ),

  // --- Skills the user owns -------------------------------------------------
  listUserSkills: (query?: AgentCapabilityQuery) =>
    invoke<{ skills: UserSkillRecord[] }>(IPC.invoke.skillList, query),
  createUserSkill: (skill: UserSkillInput) =>
    invoke<{ skill: UserSkillRecord }>(IPC.invoke.skillCreate, skill),
  /** Opens a native picker for one file; `canceled` when the user backed out. */
  importUserSkill: (query?: AgentCapabilityQuery) =>
    invoke<{ canceled?: boolean; skill?: UserSkillRecord }>(IPC.invoke.skillImport, query),
  updateUserSkill: (id: string, skill: Omit<UserSkillInput, "id">) =>
    invoke<{ skill: UserSkillRecord }>(IPC.invoke.skillUpdate, { id, ...skill }),
  /** The record plus the document body, for the editor. */
  readUserSkill: (id: string, query?: Partial<AgentCapabilityQuery>) =>
    invoke<{ skill: UserSkillRecord | null; body?: string }>(IPC.invoke.skillRead, {
      id,
      ...query,
    }),
  removeUserSkill: (id: string, query?: Partial<AgentCapabilityQuery>) =>
    invoke(IPC.invoke.skillRemove, { id, ...query }),
  setUserSkillEnabled: (
    id: string,
    enabled: boolean,
    query?: Partial<AgentCapabilityQuery>,
  ) => invoke(IPC.invoke.skillSetEnabled, { id, enabled, ...query }),
  setUserSkillScope: (id: string, scope: ActivationScope) =>
    invoke(IPC.invoke.skillSetScope, { id, scope }),
  /**
   * Level and project must travel with the id: a project skill has no global
   * counterpart to fall back to, so resolving by id alone would miss it.
   */
  revealUserSkill: (id: string, query?: Partial<AgentCapabilityQuery>) =>
    invoke(IPC.invoke.skillReveal, { id, ...query }),

  // --- Subagents the user owns ----------------------------------------------
  listUserSubagents: (query?: Pick<AgentCapabilityQuery, "level">) =>
    invoke<{ subagents: UserSubagentRecord[] }>(IPC.invoke.subagentList, query),
  /** What `Task` would offer right now, merged across all three sources. */
  subagentCatalog: () =>
    invoke<{
      subagents: SubagentDefinition[];
      diagnostics: string[];
      projectPath: string | null;
    }>(IPC.invoke.subagentCatalog),
  createUserSubagent: (subagent: UserSubagentInput) =>
    invoke<{ subagent: UserSubagentRecord }>(IPC.invoke.subagentCreate, subagent),
  updateUserSubagent: (id: string, subagent: Omit<UserSubagentInput, "id">) =>
    invoke<{ subagent: UserSubagentRecord }>(IPC.invoke.subagentUpdate, {
      id,
      ...subagent,
    }),
  /** The record plus the document body, for the editor. */
  readUserSubagent: (id: string) =>
    invoke<{ subagent: UserSubagentRecord | null; body?: string }>(
      IPC.invoke.subagentRead,
      id,
    ),
  removeUserSubagent: (id: string) => invoke(IPC.invoke.subagentRemove, id),
  setUserSubagentEnabled: (id: string, enabled: boolean) =>
    invoke(IPC.invoke.subagentSetEnabled, { id, enabled }),
  setUserSubagentScope: (id: string, scope: ActivationScope) =>
    invoke(IPC.invoke.subagentSetScope, { id, scope }),
  /** Registry entries reveal by id; project documents pass their own path. */
  revealSubagent: (target: { id?: string; path?: string }) =>
    invoke(IPC.invoke.subagentReveal, target),
  openPluginPanel: (id: string) => invoke(IPC.invoke.pluginOpenPanel, id),
  togglePluginLauncher: () => invoke(IPC.invoke.pluginLauncherToggle),
  dismissPluginLauncher: () => invoke(IPC.invoke.pluginLauncherDismiss),
  listPluginThemes: () => invoke<PluginTheme[]>(IPC.invoke.pluginThemes),
  listPluginServices: () => invoke<PluginServiceStatus[]>(IPC.invoke.pluginServices),
  /**
   * Work panel views, already filtered by permission, activation scope, and
   * entry existence, with titles resolved for the active locale (ADR 0104).
   */
  listPluginViews: () => invoke<PluginViewMeta[]>(IPC.invoke.pluginViews),
  /** Create or reuse the view's web contents. Does not show it. */
  pluginViewOpen: (
    pluginId: string,
    viewId: string,
    extra?: { sessionId?: string; location?: string },
  ) => invoke(IPC.invoke.pluginViewOpen, { pluginId, viewId, ...extra }),
  pluginViewClose: (pluginId: string, viewId: string) =>
    invoke(IPC.invoke.pluginViewClose, { pluginId, viewId }),
  pluginViewSetBounds: (bounds: {
    x: number;
    y: number;
    width: number;
    height: number;
  }) => invoke(IPC.invoke.pluginViewSetBounds, bounds),
  pluginViewSetVisible: (
    pluginId: string,
    viewId: string,
    visible: boolean,
    sessionId?: string,
  ) =>
    invoke(IPC.invoke.pluginViewSetVisible, {
      pluginId,
      viewId,
      visible,
      sessionId,
    }),
  /** Forward one plugin panel/view bridge call to the permission-checked host. */
  pluginBridgeInvoke: (input: {
    pluginId: string;
    channel: string;
    payload?: unknown;
    sessionId?: string;
  }) => invoke<unknown>(IPC.invoke.pluginBridgeInvoke, input),
  marketRefresh: (force = true) =>
    invoke<{
      providerId: string;
      name?: string;
      homepage?: string;
      updatedAt?: string;
      pluginCount: number;
      sourceUrl: string;
    }>(IPC.invoke.marketRefresh, { force }),
  marketSearch: (query = "", category = "") =>
    invoke<{
      plugins: MarketPluginSummary[];
      providerId: string;
      sourceUrl: string;
    }>(IPC.invoke.marketSearch, { query, category }),
  marketGetDetail: (id: string) =>
    invoke<{ plugin: MarketPluginDetail }>(IPC.invoke.marketGetDetail, id),
  marketInstall: (input: {
    id: string;
    version?: string;
    enable?: boolean;
    autoUpdate?: boolean;
    grantedPermissions?: string[];
  }) =>
    invoke<{ result: PluginInstallResult }>(IPC.invoke.marketInstall, input),
  marketCheckUpdates: (refreshRemote = true) =>
    invoke<{ updates: unknown[]; plugins: PluginSummary[] }>(
      IPC.invoke.marketCheckUpdates,
      { refreshRemote },
    ),
  marketApplyUpdates: (onlyAuto = true) =>
    invoke<{ results: PluginInstallResult[]; plugins: PluginSummary[] }>(
      IPC.invoke.marketApplyUpdates,
      { onlyAuto },
    ),
  /** Import a pi CLI extension file or directory as a development plugin (spec 16 §3). */
  importPiExtension: () =>
    invoke<
      | { canceled: true }
      | {
          canceled: false;
          id: string;
          path: string;
          entries: string[];
          dependencies:
            | { state: "skipped"; reason: "no-package-json" | "no-dependencies" }
            | { state: "installed" }
            | { state: "failed"; error: string };
        }
    >(IPC.invoke.pluginImportExtension),
  runExtensionCommand: (input: { sessionId: string; name: string; args: string }) =>
    invoke<{ ok: boolean }>(IPC.invoke.extensionsCommandRun, input),
  respondExtensionPrompt: (response: TrustedExtensionUiPromptResponse) =>
    invoke<{ ok: boolean }>(IPC.invoke.extensionsUiRespond, response),
  searchCommands: (query: string) =>
    invoke<{ commands: CommandItem[] }>(
      IPC.invoke.commandPaletteSearch,
      query,
    ),
  executeCommand: (commandId: string) =>
    invoke(IPC.invoke.commandPaletteExecute, commandId),
  openLogs: () => invoke(IPC.invoke.logOpenFolder),
  /** Toggles the devtools console; rejects unless developer mode is on. */
  toggleDevTools: (open?: boolean) =>
    invoke<{ open: boolean }>(IPC.invoke.devtoolsToggle, { open }),
  workspaceDiff: () => invoke<WorkspaceDiff>(IPC.invoke.workspaceDiff),
  workspaceReviewRollback: (input: {
    sessionId: string;
    snapshotId: string;
  }) => invoke<ReviewRollbackResult>(IPC.invoke.workspaceReviewRollback, input),
  browserNavigate: (url: string, sessionId?: string) =>
    invoke<BrowserState>(IPC.invoke.browserNavigate, { url, sessionId }),
  browserAction: (action: BrowserAction) =>
    invoke(IPC.invoke.browserAction, { action }),
  browserSetBounds: (bounds: {
    x: number;
    y: number;
    width: number;
    height: number;
    /** The chrome's own rect, so the guest is clamped to it. */
    view?: { x: number; y: number; width: number; height: number };
  }) => invoke(IPC.invoke.browserSetBounds, bounds),
  browserSetVisible: (visible: boolean) =>
    invoke(IPC.invoke.browserSetVisible, { visible }),
  browserOpenExternal: (url?: string) =>
    invoke(IPC.invoke.browserOpenExternal, url ? { url } : {}),
  browserGetState: () =>
    invoke<BrowserState | null>(IPC.invoke.browserGetState),
  browserClose: () => invoke(IPC.invoke.browserClose),
  fsList: (path?: string) =>
    invoke<{ entries: FsEntry[] }>(IPC.invoke.fsList, { path: path ?? "" }),
  fsRead: (path: string, mimeType?: string) =>
    invoke<FsReadResult>(IPC.invoke.fsRead, {
      path,
      ...(mimeType ? { mimeType } : {}),
    }),
  fsReadImageDataUrl: (ref: string, mimeType?: string) =>
    invoke<FsImageDataUrlResult>(IPC.invoke.fsReadImageDataUrl, {
      ref,
      ...(mimeType ? { mimeType } : {}),
    }),
  fsReveal: (path: string) => invoke(IPC.invoke.fsReveal, { path }),
  fsOpen: (path: string) => invoke(IPC.invoke.fsOpen, { path }),
  fsIndex: () => invoke<FsIndexResult>(IPC.invoke.fsIndex),
  composerCommands: () =>
    invoke<{ commands: ComposerCommand[] }>(IPC.invoke.composerCommands),
  setWorkPanelReservation: (width: number) =>
    invoke<{ requested: number; reserved: number }>(
      IPC.invoke.windowSetWorkPanelReservation,
      { width },
    ),
  setWorkPanelChatWidth: (width: number) =>
    invoke<{ requested: number; applied: number }>(
      IPC.invoke.windowSetWorkPanelChatWidth,
      { width },
    ),
  /**
   * Tell the shell the current palette. `themeSource` carries whether the
   * palette follows the system or was pinned; macOS steers the window
   * appearance with it so native sidebar vibrancy matches the renderer.
   */
  setWindowBackgroundColor: (
    theme: "light" | "dark",
    color?: string,
    themeSource?: "system" | "light" | "dark",
  ) =>
    invoke<{ applied: boolean; theme: "light" | "dark"; color?: string }>(
      IPC.invoke.windowSetBackgroundColor,
      { theme, color, themeSource },
    ),
  windowControl: (action: WindowControlAction) =>
    invoke<{ maximized: boolean }>(IPC.invoke.windowControl, { action }),
  getCloseBehavior: () =>
    invoke<{ behavior: CloseBehavior; supported: boolean }>(
      IPC.invoke.closeBehaviorGet,
    ),
  setCloseBehavior: (behavior: CloseBehavior) =>
    invoke<{ behavior: CloseBehavior }>(IPC.invoke.closeBehaviorSet, {
      behavior,
    }),
  getTokenUsageHistory: (query?: { startDate?: number; endDate?: number; bucket?: "day" | "week" | "month" }) =>
    invoke<import("@pi-desktop/shared").TokenUsageHistoryResult>(
      IPC.invoke.statsGetTokenUsageHistory,
      query,
    ),
  menuRendererReady: () =>
    invoke<{ ready: boolean }>(IPC.invoke.menuRendererReady),
  nativeMenuAction: (action: NativeMenuAction) =>
    invoke<{ maximized: boolean; fullScreen: boolean }>(
      IPC.invoke.nativeMenuAction,
      { action },
    ),
  onWindowMaximized: (listener: (event: { maximized: boolean }) => void) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.windowMaximized, (payload) =>
      listener(payload as { maximized: boolean }),
    );
  },
  onWindowFullScreen: (listener: (event: { fullScreen: boolean }) => void) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.windowFullScreen, (payload) =>
      listener(payload as { fullScreen: boolean }),
    );
  },
  onWorkPanelResize: (
    listener: (event: {
      phase: "preview" | "commit";
      panelWidth: number;
    }) => void,
  ) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.windowWorkPanelResize, (payload) =>
      listener(
        payload as {
          phase: "preview" | "commit";
          panelWidth: number;
        },
      ),
    );
  },
  onMenuCommand: (listener: (command: AppMenuCommand) => void) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.menuCommand, (payload) =>
      listener((payload as { command: AppMenuCommand }).command),
    );
  },
  onBrowserState: (listener: (state: BrowserState) => void) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.browserState, (payload) =>
      listener(payload as BrowserState),
    );
  },
  onBrowserPreview: (
    listener: (event: { sessionId: string; path?: string; url?: string }) => void,
  ) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.browserPreview, (payload) =>
      listener(payload as { sessionId: string; path?: string; url?: string }),
    );
  },
  onAgentEvent: (listener: (event: AgentEventEnvelope) => void) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.agentMessage, (payload) =>
      listener(payload as AgentEventEnvelope),
    );
  },
  onAgentQueueChanged: (listener: (event: AgentQueueChangedEvent) => void) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.agentQueueChanged, (payload) =>
      listener(payload as AgentQueueChangedEvent),
    );
  },
  onPlansChanged: (listener: (event: PlanningStateEvent) => void) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.plansChanged, (payload) =>
      listener(normalizePlansChangedEvent(payload)),
    );
  },
  onOauthLogin: (listener: (event: OAuthLoginEvent) => void) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.providersOauth, (payload) =>
      listener(payload as OAuthLoginEvent),
    );
  },
  onExtensionPrompt: (listener: (prompt: TrustedExtensionUiPrompt) => void) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.extensionsUiPrompt, (payload) =>
      listener(payload as TrustedExtensionUiPrompt),
    );
  },
  onExtensionStatus: (listener: (event: TrustedExtensionStatusEvent) => void) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.extensionsStatus, (payload) =>
      listener(payload as TrustedExtensionStatusEvent),
    );
  },
  onToast: (listener: (message: string) => void) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.toast, (payload) =>
      listener((payload as { message: string }).message),
    );
  },
  onHostStatus: (listener: (status: HostStatusEvent) => void) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.hostStatus, (payload) =>
      listener(payload as HostStatusEvent),
    );
  },
  onNotificationChanged: (
    listener: (notification: AppNotification) => void,
  ) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.notificationChanged, (payload) =>
      listener((payload as { notification: AppNotification }).notification),
    );
  },
  onSessionsChanged: (
    listener: (event: {
      reason?: string;
      pluginId?: string;
      projectPath?: string | null;
      selectSessionId?: string;
    }) => void,
  ) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.sessionsChanged, (payload) =>
      listener(
        (payload ?? {}) as {
          reason?: string;
          pluginId?: string;
          projectPath?: string | null;
          selectSessionId?: string;
        },
      ),
    );
  },
  onNotificationActivated: (
    listener: (event: { id: string; sessionId: string }) => void,
  ) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.notificationActivated, (payload) =>
      listener(payload as { id: string; sessionId: string }),
    );
  },
  onUpdateState: (listener: (state: UpdateState) => void) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.updatesState, (payload) =>
      listener(payload as UpdateState),
    );
  },
  onPluginChanged: (
    listener: (event: { reason?: string; pluginId?: string }) => void,
  ) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.pluginChanged, (payload) =>
      listener((payload ?? {}) as { reason?: string; pluginId?: string }),
    );
  },
  /**
   * `pi.ui.openPanel` / `closePanel` from plugin code. Renderer-initiated
   * opens get the same payload from `openPluginPanel`'s reply instead.
   */
  onPluginPanel: (
    listener: (event: {
      action: "open" | "close";
      pluginId: string;
      url?: string;
      title?: string;
      width?: number;
      height?: number;
    }) => void,
  ) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on("pi-desktop/plugin/event/panel", (payload) =>
      listener(
        payload as {
          action: "open" | "close";
          pluginId: string;
          url?: string;
          title?: string;
          width?: number;
          height?: number;
        },
      ),
    );
  },
  onSettingsChanged: (listener: (patch: Record<string, unknown>) => void) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.settingsChanged, (payload) =>
      listener((payload ?? {}) as Record<string, unknown>),
    );
  },
  onPluginLauncherShown: (listener: () => void) => {
    if (!window.piDesktop?.on) return () => undefined;
    return window.piDesktop.on(IPC.event.pluginLauncherShown, () => listener());
  },
  /**
   * The resident driver announces roster, run, and memory changes. One reason
   * string is enough: the console refetches whatever tab it is showing.
   *
   * A write made on a page in this window arrives by the other door, as the
   * event `invoke` raises when it lands, so the rail's badge and the board
   * next door refetch with the page that made the change rather than half a
   * minute after it.
   */
  onStaffChanged: (
    listener: (event: { reason?: string; employeeId?: string; runId?: string }) => void,
  ) => {
    const onWrite = () => listener({ reason: "owner" });
    window.addEventListener(STAFF_WRITE_EVENT, onWrite);
    const offLocal = () => window.removeEventListener(STAFF_WRITE_EVENT, onWrite);
    if (!window.piDesktop?.on) return offLocal;
    const offRemote = window.piDesktop.on(IPC.event.staffChanged, (payload) =>
      listener(
        (payload ?? {}) as { reason?: string; employeeId?: string; runId?: string },
      ),
    );
    return () => {
      offLocal();
      offRemote();
    };
  },
};
