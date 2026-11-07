import i18n from "i18next";
import { api } from "./api";
import {
  buildRewindPoints,
  buildSessionForest,
  buildSessionTreeNodes,
  lineageForSession,
  rewindChangesFrom,
} from "./session-tree";
import { withReviewChangeState } from "./workspace-review";
import { useAppStore } from "../stores/app-store";
import {
  trustedExtensionCommandName,
  type ComposerCommand,
  type Mode,
} from "@pi-desktop/shared";

/**
 * First-party command execution shared by the command palette and the
 * composer "/" dispatch (D123). Builtin ids run app actions locally; other
 * ids round-trip to the plugin runtime via commandPalette/execute.
 */
export async function runPaletteCommand(commandId: string): Promise<void> {
  const store = useAppStore.getState();
  switch (commandId) {
    case "builtin.session.new":
      await store.newSession();
      break;
    case "builtin.agent.compact":
      await store.compactContext();
      break;
    case "builtin.session.copy":
      await copyLastAssistantReply();
      break;
    case "builtin.session.info":
      store.openInfoPanel("session");
      break;
    case "builtin.app.status":
      store.openInfoPanel("status");
      break;
    case "builtin.agent.btw":
      // The palette carries no argument; the composer form `/btw <question>`
      // is the one that opens a side thread.
      store.showToast(
        i18n.t("chat.btwNeedsQuestion", {
          defaultValue:
            "Type /btw <question> in the composer to ask a side question.",
        }),
        { variant: "info" },
      );
      break;
    case "builtin.mode.agent":
    case "builtin.mode.plan":
    case "builtin.mode.goal": {
      const mode: Mode = commandId.endsWith("plan")
        ? "plan"
        : commandId.endsWith("goal")
          ? "goal"
          : "agent";
      const activeSession = store.activeSessionId
        ? store.sessions.find((session) => session.id === store.activeSessionId)
        : undefined;
      if (activeSession) {
        await store.configureActiveSession({
          mode,
          providerId: activeSession.providerId,
          modelId: activeSession.modelId,
          thinkingLevel: activeSession.thinkingLevel,
        });
      } else if (store.settings) {
        // Keep the command useful from the empty home, where no session has
        // been created yet. The next session will inherit this default.
        const next = { ...store.settings, defaultMode: mode };
        await api.setSettings(next);
        useAppStore.setState({ settings: next });
      }
      break;
    }
    case "builtin.session.export":
      await exportActiveSession();
      break;
    case "builtin.agent.rebuild":
      await rebuildActiveSession();
      break;
    case "builtin.agent.goal":
      await showGoalStatus();
      break;
    case "builtin.session.tree":
      await openSessionTreePanel("tree");
      break;
    case "builtin.session.rewind":
      await openSessionTreePanel("rewind");
      break;
    case "builtin.memory.report":
      store.openInfoPanel("memory");
      break;
    case "builtin.agent.dream":
      await runDreamPass();
      break;
    default: {
      // Trusted extension commands run inside the active session's sidecar
      // (spec 16 §8); without a session there is nothing to run them in.
      const extensionCommand = trustedExtensionCommandName(commandId);
      if (extensionCommand !== undefined) {
        await runExtensionCommand(extensionCommand, "");
        return;
      }
      await api.executeCommand(commandId);
    }
  }
}

/** Copy the newest assistant answer as plain text (REPL `/copy` parity). */
async function copyLastAssistantReply(): Promise<void> {
  const store = useAppStore.getState();
  const lastReply = [...store.messages]
    .reverse()
    .find((message) => message.role === "assistant" && message.content.trim());
  if (!lastReply) {
    store.showToast(
      i18n.t("chat.copyNoReply", {
        defaultValue: "No assistant reply to copy yet.",
      }),
      { variant: "info" },
    );
    return;
  }
  try {
    await navigator.clipboard.writeText(lastReply.content.trim());
    store.showToast(
      i18n.t("chat.copyDone", { defaultValue: "Copied the last reply." }),
      { variant: "success" },
    );
  } catch (error) {
    store.showToast(error instanceof Error ? error.message : String(error), {
      variant: "error",
    });
  }
}

/**
 * CLI /export parity. The host writes pigo-compatible JSONL through its own
 * save dialog; the renderer only supplies the active session id and reports
 * the outcome (including a cancelled dialog, which is not an error).
 */
async function exportActiveSession(): Promise<void> {
  const store = useAppStore.getState();
  const sessionId = store.activeSessionId;
  if (!sessionId) {
    store.showToast(i18n.t("chat.exportNeedsSession"), { variant: "info" });
    return;
  }
  const result = await api.exportSession(sessionId);
  if (!result.saved) {
    store.showToast(i18n.t("chat.exportCancelled"), { variant: "info" });
    return;
  }
  store.showToast(`${i18n.t("chat.exportDone")} ${result.path ?? ""}`.trim(), {
    variant: "success",
  });
}

/**
 * CLI /rebuild parity. The sidecar owns the live AgentContext, so dropping and
 * reloading it is enough; no transcript is rewritten on the host side.
 */
async function rebuildActiveSession(): Promise<void> {
  const store = useAppStore.getState();
  const sessionId = store.activeSessionId;
  if (!sessionId) {
    store.showToast(i18n.t("chat.rebuildNeedsSession"), { variant: "info" });
    return;
  }
  const result = await api.rebuildSession(sessionId);
  store.showToast(
    i18n.t(result.rebuilt ? "chat.rebuildDone" : "chat.rebuildIdle"),
    { variant: result.rebuilt ? "success" : "info" },
  );
}

/**
 * CLI /dream parity: one memory-consolidation pass over the global and project
 * memory scopes, using the session's own model.
 *
 * The desktop has no `pigo --dream` binary to spawn, so the sidecar runs the
 * pass in-process and answers when it finishes. The pass can take minutes
 * (several model calls), which is why the toast announces it up front instead
 * of leaving the composer looking idle.
 */
async function runDreamPass(dryRun = false): Promise<void> {
  const store = useAppStore.getState();
  const sessionId = store.activeSessionId;
  if (!sessionId) {
    store.showToast(i18n.t("chat.dreamNeedsSession"), { variant: "info" });
    return;
  }
  store.showToast(i18n.t("chat.dreamRunning", { defaultValue: "Consolidating memory…" }), {
    variant: "info",
  });
  try {
    const { report } = await api.runDream(sessionId, dryRun);
    store.showToast(
      i18n.t(dryRun ? "chat.dreamDryRunDone" : "chat.dreamDone", {
        merged: report.merged,
        deduped: report.deduped,
        pruned: report.pruned,
        distilled: report.distilled,
      }),
      { variant: "success" },
    );
  } catch (error) {
    store.showToast(error instanceof Error ? error.message : String(error), {
      variant: "error",
    });
  }
}

/** Current goal/planning state, read from the store plus a best-effort RPC. */async function showGoalStatus(): Promise<void> {
  const store = useAppStore.getState();
  const session = store.activeSessionId
    ? store.sessions.find((item) => item.id === store.activeSessionId)
    : undefined;
  const mode = session?.mode ?? store.settings?.defaultMode ?? "agent";
  let detail = "";
  if (store.activeSessionId) {
    try {
      const { status } = await api.getStatus(store.activeSessionId);
      const phase = status.planningState ?? "idle";
      detail = ` · ${phase}${status.pendingPlanId ? ` (${status.pendingPlanId})` : ""}`;
    } catch {
      // The sidecar may not be running yet; the local mode is still useful.
    }
  }
  store.showToast(
    `${i18n.t("chat.goalStatusPrefix")} ${mode}${detail} · ${i18n.t("chat.goalUsage")}`,
    { variant: "info" },
  );
}

/**
 * Desktop /goal. The desktop runtime executes one approved goal turn rather
 * than the REPL's autonomous loop, so `pause` / `resume` / token budgets are
 * deliberately reported as unsupported instead of being faked. Starting a
 * goal, inspecting it and leaving Goal mode are real actions.
 */
async function runGoalWithPrompt(
  _commandId: string,
  ctx: ComposerCommandContext,
): Promise<void> {
  const store = useAppStore.getState();
  const body = ctx.body.trim();
  const verb = body.toLowerCase().split(/\s+/)[0];

  if (verb === "clear") {
    await runPaletteCommand("builtin.mode.agent");
    store.showToast(i18n.t("chat.goalCleared"), { variant: "success" });
    ctx.clearDraft();
    return;
  }
  if (verb === "status" || verb === "") {
    await showGoalStatus();
    ctx.clearDraft();
    return;
  }
  if (["pause", "resume", "tokens", "--tokens", "-t"].includes(verb)) {
    store.showToast(i18n.t("chat.goalUnsupported"), { variant: "info" });
    ctx.clearDraft();
    return;
  }

  await runPaletteCommand("builtin.mode.goal");
  const accepted = await ctx.sendPrompt();
  if (accepted) ctx.clearDraft();
}

type SessionTreeViewKind = "tree" | "rewind";

/**
 * Read the full host transcript once and hand a snapshot to the composer-dock
 * panel. `session.get` (without a window) is authoritative, so node numbers and
 * restore points stay valid even when the renderer only holds a message page.
 */
export async function openSessionTreePanel(
  view: SessionTreeViewKind,
): Promise<void> {
  const store = useAppStore.getState();
  const sessionId = store.activeSessionId;
  if (!sessionId) {
    store.showToast(i18n.t("chat.treeNeedsSession"), { variant: "info" });
    return;
  }
  try {
    const { session } = await api.getSession(sessionId);
    if (!session || useAppStore.getState().activeSessionId !== sessionId) return;
    const messages = session.messages ?? [];
    // The lineage comes from session metadata, so a failure to list sessions
    // still leaves the node list and restore points usable.
    const listed = await api.listSessions().catch(() => null);
    const forest = buildSessionForest(listed?.sessions ?? [], sessionId);
    store.openTreePanel({
      view,
      sessionId,
      nodes: buildSessionTreeNodes(messages),
      points: buildRewindPoints(messages),
      // Only the tree the open session belongs to: the rest belongs to the
      // sidebar, not to a branch view.
      forest: lineageForSession(forest, sessionId),
    });
  } catch (error) {
    store.showToast(error instanceof Error ? error.message : String(error), {
      variant: "error",
    });
  }
}

function selectionToIndex(
  selection: string | number,
  count: number,
): number | null {
  const value = Number(String(selection).trim());
  if (!Number.isInteger(value) || value < 1 || value > count) return null;
  return value;
}

/** Panel buttons call these commands directly, so failures must not escape. */
function reportCommandFailure(error: unknown): void {
  useAppStore.getState().showToast(
    error instanceof Error ? error.message : String(error),
    { variant: "error" },
  );
}

/**
 * `/tree <n>` — desktop equivalent of switching the REPL's active leaf. A
 * fork through node `n` becomes the active conversation; the original session
 * is left untouched because the desktop stores no parent/child lineage.
 */
export async function branchFromTreeNode(
  selection: string | number,
): Promise<void> {
  try {
    await doBranchFromTreeNode(selection);
  } catch (error) {
    reportCommandFailure(error);
  }
}

async function doBranchFromTreeNode(
  selection: string | number,
): Promise<void> {
  const store = useAppStore.getState();
  const sessionId = store.activeSessionId;
  if (!sessionId) {
    store.showToast(i18n.t("chat.treeNeedsSession"), { variant: "info" });
    return;
  }
  const { session } = await api.getSession(sessionId);
  const nodes = buildSessionTreeNodes(session?.messages ?? []);
  const index = selectionToIndex(selection, nodes.length);
  if (index === null) {
    store.showToast(i18n.t("chat.treeInvalid", { count: nodes.length }), {
      variant: "info",
    });
    return;
  }
  const branched = await store.branchFromMessage(nodes[index - 1].id);
  if (!branched) {
    store.showToast(i18n.t("chat.treeBranchFailed"), { variant: "error" });
    return;
  }
  const current = useAppStore.getState();
  current.closeTreePanel();
  current.showToast(i18n.t("chat.treeBranchDone", { index }), {
    variant: "success",
  });
}

/**
 * `/rewind <n>` — restore the journaled Write/Edit mutations from point `n`
 * onward, newest first (the host refuses to overwrite a file whose post-tool
 * hash changed), then fork the conversation to the node before that turn.
 * Shell/Bash side effects are not journaled and are deliberately out of scope,
 * exactly as in the REPL's rewind v1.
 */
export async function rewindToRestorePoint(
  selection: string | number,
): Promise<void> {
  try {
    await doRewindToRestorePoint(selection);
  } catch (error) {
    reportCommandFailure(error);
  }
}

async function doRewindToRestorePoint(
  selection: string | number,
): Promise<void> {
  const store = useAppStore.getState();
  const sessionId = store.activeSessionId;
  if (!sessionId) {
    store.showToast(i18n.t("chat.treeNeedsSession"), { variant: "info" });
    return;
  }
  const { session } = await api.getSession(sessionId);
  const points = buildRewindPoints(session?.messages ?? []);
  const index = selectionToIndex(selection, points.length);
  if (index === null) {
    store.showToast(i18n.t("chat.rewindInvalid", { count: points.length }), {
      variant: "info",
    });
    return;
  }
  const point = points[index - 1];
  let restored = 0;
  let skipped = 0;
  const rolledBackIds = new Set<string>();
  for (const change of rewindChangesFrom(points, index)) {
    try {
      const result = await api.workspaceReviewRollback({
        sessionId,
        snapshotId: change.snapshotId,
      });
      if (
        result.status === "rolledBack" ||
        result.status === "alreadyRolledBack"
      ) {
        restored += 1;
        rolledBackIds.add(change.messageId);
      } else {
        // conflict / unavailable: never claim a file was restored.
        skipped += 1;
      }
    } catch {
      skipped += 1;
    }
  }
  if (rolledBackIds.size > 0) {
    useAppStore.setState((state) => ({
      messages: state.messages.map((message) =>
        rolledBackIds.has(message.id)
          ? withReviewChangeState(message, "rolledBack")
          : message,
      ),
    }));
  }

  const before = point.beforeMessageId;
  let note = "";
  if (before) {
    const branched = await useAppStore.getState().branchFromMessage(before);
    if (!branched) note = i18n.t("chat.treeBranchFailed");
  } else {
    note = i18n.t("chat.rewindFirstTurn");
  }
  const current = useAppStore.getState();
  current.closeTreePanel();
  current.showToast(
    `${i18n.t("chat.rewindDone", { restored, skipped })}${note ? ` · ${note}` : ""}`,
    { variant: skipped > 0 ? "warning" : "success" },
  );
}

export async function runExtensionCommand(name: string, args: string): Promise<void> {
  const store = useAppStore.getState();
  if (!store.activeSessionId) {
    throw new Error(i18n.t("plugins.agentExtension.commandNeedsSession"));
  }
  await api.runExtensionCommand({ sessionId: store.activeSessionId, name, args });
}

/** Dependencies the composer submit hook supplies for a slash invocation. */
export type ComposerCommandContext = {
  /** Text after "/name ", with inline file references serialized. */
  body: string;
  /** Send the visible argument as a normal prompt; false when rejected. */
  sendPrompt: () => Promise<boolean>;
  clearDraft: () => void;
};

type BuiltinArgAction = (
  commandId: string,
  ctx: ComposerCommandContext,
) => Promise<void>;

/**
 * Builtin commands that consume a slash argument. A new argument-taking
 * builtin only needs a row here plus a `BUILTIN_ALIASES` row in Rust — no
 * changes to the composer submit hook.
 */
const BUILTIN_ARG_ACTIONS: Record<string, BuiltinArgAction> = {
  "builtin.agent.btw": async (_commandId, ctx) => {
    await useAppStore.getState().openBtw(ctx.body);
    ctx.clearDraft();
  },
  "builtin.mode.agent": runModeWithPrompt,
  "builtin.mode.plan": runModeWithPrompt,
  "builtin.mode.goal": runModeWithPrompt,
  "builtin.agent.goal": runGoalWithPrompt,
  "builtin.session.tree": async (_commandId, ctx) => {
    await branchFromTreeNode(ctx.body);
    ctx.clearDraft();
  },
  "builtin.session.rewind": async (_commandId, ctx) => {
    await rewindToRestorePoint(ctx.body);
    ctx.clearDraft();
  },
  "builtin.agent.dream": async (_commandId, ctx) => {
    // `/dream dry-run` analyzes without writing, exactly like the CLI flag.
    const dryRun = ["dry-run", "--dry-run", "-n"].includes(
      ctx.body.trim().toLowerCase(),
    );
    ctx.clearDraft();
    await runDreamPass(dryRun);
  },
};

async function runModeWithPrompt(
  commandId: string,
  ctx: ComposerCommandContext,
): Promise<void> {
  await runPaletteCommand(commandId);
  const accepted = await ctx.sendPrompt();
  if (accepted) ctx.clearDraft();
}

/**
 * Single dispatch point for a resolved composer command. Returns true when
 * the input was consumed as a command; false leaves it on the prompt path.
 * `argMode` comes from the Rust builtin registry, so adding no-argument
 * commands never requires touching this control flow.
 */
export async function dispatchComposerCommand(
  command: ComposerCommand,
  ctx: ComposerCommandContext,
): Promise<boolean> {
  const store = useAppStore.getState();
  const argMode = command.argMode ?? "none";
  const id = command.id;

  if (argMode === "required" && !ctx.body) {
    store.showToast(
      i18n.t("chat.commandNeedsArgument", {
        defaultValue: "/{{name}} needs an argument.",
        name: command.name,
      }),
      { variant: "info" },
    );
    return true;
  }

  try {
    const argAction = ctx.body && id ? BUILTIN_ARG_ACTIONS[id] : undefined;
    if (argAction && id) {
      await argAction(id, ctx);
      return true;
    }
    if (command.kind === "extension") {
      await runExtensionCommand(command.name, ctx.body);
      ctx.clearDraft();
      return true;
    }
    if (!ctx.body && id) {
      if (command.kind === "builtin") await runPaletteCommand(id);
      else await api.executeCommand(id);
      ctx.clearDraft();
      return true;
    }
  } catch (error) {
    store.showToast(error instanceof Error ? error.message : String(error), {
      variant: "error",
    });
    return true;
  }
  // A command that declares no argument but received one keeps the legacy
  // behaviour of falling through as ordinary prompt text.
  return false;
}
