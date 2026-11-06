import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  applyCompletion,
  compareMatches,
  detectTrigger,
  fileReferenceLabel,
  formatCommandInsert,
  formatFileInsert,
  fuzzyMatchCommand,
  fuzzyMatchPath,
  selectBestMatches,
  type ComposerCommand,
  type ComposerTrigger,
  type FsIndexEntry,
  type FuzzyMatch,
} from "@pi-desktop/shared";
import { api } from "../lib/api";
import { useAppStore } from "../stores/app-store";

/**
 * Composer autocomplete state machine (D123–D125): trigger detection over
 * draft+cursor, lazily fetched command/file sources, local fuzzy filtering,
 * and insert-on-accept. IME freezing and key routing live in the Composer;
 * this hook only refuses to update while `composing` is true.
 */

const MAX_FILE_ITEMS = 50;
const SOURCE_TTL_MS = 10_000;

export type AutocompleteItem =
  | { kind: "command"; command: ComposerCommand; match: FuzzyMatch }
  | {
      kind: "path";
      entry: FsIndexEntry;
      match: FuzzyMatch;
      /** Nesting level in the workspace tree; the menu indents by this. */
      depth: number;
      /** False for ancestors kept only to carry directory hierarchy. */
      matched: boolean;
    };

/** Module-level TTL caches so re-triggering stays IPC-free. */
let commandsCache: { key: string; at: number; commands: ComposerCommand[] } | null =
  null;
let filesCache: {
  key: string;
  at: number;
  entries: FsIndexEntry[];
  truncated: boolean;
} | null = null;

const COMMAND_GROUP_ORDER = {
  template: 0,
  builtin: 1,
  plugin: 2,
  extension: 3,
  skill: 4,
} as const;

function filterCommands(
  commands: ComposerCommand[],
  query: string,
): AutocompleteItem[] {
  const matched: Array<{
    command: ComposerCommand;
    match: FuzzyMatch;
    sortText: string;
  }> = [];
  for (const command of commands) {
    const byName = fuzzyMatchCommand(query, command.name);
    if (byName) {
      matched.push({ command, match: byName, sortText: command.name });
      continue;
    }
    // Title/description hits keep the row findable, without name highlights.
    const byTitle =
      fuzzyMatchCommand(query, command.title) ??
      (command.description
        ? fuzzyMatchCommand(query, command.description)
        : null);
    if (byTitle) {
      matched.push({
        command,
        match: { score: Math.max(0, byTitle.score - 20), ranges: [] },
        sortText: command.name,
      });
    }
  }
  matched.sort((a, b) => {
    const groupDelta =
      COMMAND_GROUP_ORDER[a.command.kind] - COMMAND_GROUP_ORDER[b.command.kind];
    if (groupDelta !== 0) return groupDelta;
    return compareMatches(
      { score: a.match.score, text: a.sortText },
      { score: b.match.score, text: b.sortText },
    );
  });
  return matched.map(({ command, match }) => ({ kind: "command", command, match }));
}

/**
 * File mode paints the workspace as a directory tree instead of one flat list.
 * A query without "/" fuzzy-matches every path and keeps each hit's ancestor
 * directories as context rows, so two identically named leaves stay
 * distinguishable by nesting. A query with "/" scopes to the typed directory
 * (accepting a directory inserts `@dir/`), and only that level's direct
 * children are listed — drilling down instead of dumping the whole subtree.
 */
type FileRow = {
  kind: "path";
  entry: FsIndexEntry;
  match: FuzzyMatch;
  depth: number;
  matched: boolean;
};

function indexPath(entry: FsIndexEntry): string {
  return entry.path.replace(/\\/g, "/");
}

function parentPath(path: string): string {
  const at = path.lastIndexOf("/");
  return at < 0 ? "" : path.slice(0, at);
}

function leafPath(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

/** Tree preorder: parent before child, directories before files, then A→Z. */
function compareFileRows(left: FileRow, right: FileRow): number {
  const leftParent = parentPath(indexPath(left.entry)).toLowerCase();
  const rightParent = parentPath(indexPath(right.entry)).toLowerCase();
  if (leftParent !== rightParent) return leftParent < rightParent ? -1 : 1;
  if (left.entry.kind !== right.entry.kind) {
    return left.entry.kind === "dir" ? -1 : 1;
  }
  const leftLeaf = leafPath(indexPath(left.entry)).toLowerCase();
  const rightLeaf = leafPath(indexPath(right.entry)).toLowerCase();
  return leftLeaf < rightLeaf ? -1 : leftLeaf > rightLeaf ? 1 : 0;
}

function selectScopedRows(
  entries: FsIndexEntry[],
  scope: string,
  filter: string,
): FileRow[] {
  const lowered = scope.toLowerCase();
  const rows: FileRow[] = [];
  for (const entry of entries) {
    const path = indexPath(entry);
    if (!path.toLowerCase().startsWith(lowered)) continue;
    // Direct children only: everything below this level comes from drilling.
    if (path.slice(scope.length).includes("/")) continue;
    const match = fuzzyMatchPath(filter, path, entry.kind);
    if (!match) continue;
    rows.push({
      kind: "path",
      entry: { ...entry, path },
      match,
      depth: path.split("/").length - 1,
      matched: true,
    });
  }
  return rows.sort(compareFileRows).slice(0, MAX_FILE_ITEMS);
}

function selectGlobalRows(entries: FsIndexEntry[], query: string): FileRow[] {
  const matched: Array<{ entry: FsIndexEntry; match: FuzzyMatch }> = [];
  for (const entry of entries) {
    const path = indexPath(entry);
    const match = fuzzyMatchPath(query, path, entry.kind);
    if (match) matched.push({ entry: { ...entry, path }, match });
  }
  // The index can hold thousands of entries while the menu shows at most
  // MAX_FILE_ITEMS, so only the bounded top slice is ever ordered.
  const best = selectBestMatches(matched, MAX_FILE_ITEMS, ({ entry, match }) => ({
    score: match.score,
    text: entry.path,
  }));

  const rows = new Map<string, FileRow>();
  for (const { entry, match } of best) {
    const segments = entry.path.split("/");
    for (let index = 1; index < segments.length; index += 1) {
      const ancestor = segments.slice(0, index).join("/");
      if (rows.has(ancestor)) continue;
      rows.set(ancestor, {
        kind: "path",
        entry: { path: ancestor, kind: "dir" },
        match: { score: 0, ranges: [] },
        depth: index - 1,
        matched: false,
      });
    }
    // A matched directory replaces the context row an earlier hit created.
    rows.set(entry.path, {
      kind: "path",
      entry,
      match,
      depth: segments.length - 1,
      matched: true,
    });
  }
  return [...rows.values()].sort(compareFileRows);
}

function selectFileRows(entries: FsIndexEntry[], query: string): AutocompleteItem[] {
  const normalized = query.replace(/\\/g, "/").replace(/^\/+/, "");
  const slash = normalized.lastIndexOf("/");
  if (slash >= 0) {
    return selectScopedRows(
      entries,
      normalized.slice(0, slash + 1),
      normalized.slice(slash + 1),
    );
  }
  // A bare "@" starts at the workspace root: the top level only, so opening
  // the menu is browsing the tree rather than dumping every indexed file.
  if (normalized === "") return selectScopedRows(entries, "", "");
  return selectGlobalRows(entries, normalized);
}

/**
 * Resolve a typed "/name" against the merged command and skill list at send
 * time (builtin/plugin dispatch and skill validation); templates and unknown
 * names return as-is/null and stay on the prompt path. Reuses the menu's TTL
 * cache when warm.
 */
export async function resolveComposerCommand(
  name: string,
): Promise<ComposerCommand | null> {
  const key = useAppStore.getState().workspace?.path ?? "";
  if (
    !commandsCache ||
    commandsCache.key !== key ||
    Date.now() - commandsCache.at > SOURCE_TTL_MS
  ) {
    try {
      const res = await api.composerCommands();
      commandsCache = { key, at: Date.now(), commands: res.commands };
    } catch {
      return null;
    }
  }
  return commandsCache.commands.find((c) => c.name === name) ?? null;
}

export function useComposerAutocomplete({
  value,
  cursor,
  composing,
  enabled,
}: {
  value: string;
  cursor: number;
  composing: boolean;
  enabled: boolean;
}) {
  const workspaceKey = useAppStore((s) => s.workspace?.path ?? "");
  const hasWorkspace = workspaceKey !== "";
  const [commands, setCommands] = useState<ComposerCommand[] | null>(null);
  const [files, setFiles] = useState<{
    entries: FsIndexEntry[];
    truncated: boolean;
  } | null>(null);
  const [highlight, setHighlight] = useState(0);
  const [dismissedKey, setDismissedKey] = useState<string | null>(null);
  const frozenRef = useRef<ComposerTrigger | null>(null);

  const liveTrigger = useMemo(
    () => (enabled ? detectTrigger(value, cursor) : null),
    [enabled, value, cursor],
  );
  // During IME composition the menu freezes: no opening, closing, or
  // re-filtering until compositionend re-evaluates (D125).
  const trigger = composing ? frozenRef.current : liveTrigger;
  useEffect(() => {
    if (!composing) frozenRef.current = liveTrigger;
  }, [composing, liveTrigger]);

  const triggerKey = trigger ? `${trigger.mode}:${trigger.tokenStart}` : null;
  const dismissed = triggerKey !== null && triggerKey === dismissedKey;

  // Escape-dismissal clears once the trigger token goes away.
  useEffect(() => {
    if (dismissedKey && triggerKey !== dismissedKey) setDismissedKey(null);
  }, [triggerKey, dismissedKey]);

  // Lazy source fetch with a short TTL, keyed by workspace.
  useEffect(() => {
    if (!trigger || dismissed) return;
    const now = Date.now();
    if (trigger.mode === "slash") {
      if (
        commandsCache &&
        commandsCache.key === workspaceKey &&
        now - commandsCache.at < SOURCE_TTL_MS
      ) {
        setCommands(commandsCache.commands);
        return;
      }
      let cancelled = false;
      void api
        .composerCommands()
        .then((res) => {
          commandsCache = { key: workspaceKey, at: Date.now(), commands: res.commands };
          if (!cancelled) setCommands(res.commands);
        })
        .catch(() => {
          if (!cancelled) setCommands([]);
        });
      return () => {
        cancelled = true;
      };
    }
    if (!hasWorkspace) {
      setFiles({ entries: [], truncated: false });
      return;
    }
    if (
      filesCache &&
      filesCache.key === workspaceKey &&
      now - filesCache.at < SOURCE_TTL_MS
    ) {
      setFiles({ entries: filesCache.entries, truncated: filesCache.truncated });
      return;
    }
    let cancelled = false;
    void api
      .fsIndex()
      .then((res) => {
        filesCache = {
          key: workspaceKey,
          at: Date.now(),
          entries: res.entries,
          truncated: res.truncated,
        };
        if (!cancelled) setFiles({ entries: res.entries, truncated: res.truncated });
      })
      .catch(() => {
        if (!cancelled) setFiles({ entries: [], truncated: false });
      });
    return () => {
      cancelled = true;
    };
  }, [trigger?.mode, dismissed, workspaceKey, hasWorkspace]);

  const items = useMemo<AutocompleteItem[]>(() => {
    if (!trigger || dismissed) return [];
    if (trigger.mode === "slash") {
      return commands ? filterCommands(commands, trigger.query) : [];
    }
    return files ? selectFileRows(files.entries, trigger.query) : [];
  }, [trigger, dismissed, commands, files]);

  // New query or mode restarts keyboard navigation at the top hit — never on
  // a context ancestor that exists only to paint the directory tree.
  const itemsKey = trigger ? `${trigger.mode}:${trigger.query}` : "";
  useEffect(() => {
    const firstHit = items.findIndex(
      (item) => item.kind === "command" || item.matched,
    );
    setHighlight(firstHit < 0 ? 0 : firstHit);
  }, [itemsKey, items]);

  const sourceReady =
    !!trigger &&
    (trigger.mode === "slash" ? commands !== null : files !== null);
  const open = !!trigger && !dismissed && sourceReady;

  const close = useCallback(() => {
    if (triggerKey) setDismissedKey(triggerKey);
  }, [triggerKey]);

  const accept = useCallback(
    (
      index: number,
    ):
      | {
          value: string;
          cursor: number;
          fileReference?: { path: string; name: string };
        }
      | null => {
      if (!trigger) return null;
      const item = items[index];
      if (!item) return null;
      if (item.kind === "path" && item.entry.kind === "file") {
        return {
          ...applyCompletion(value, trigger, ""),
          fileReference: {
            path: item.entry.path,
            name: fileReferenceLabel(item.entry.path),
          },
        };
      }
      const insert =
        item.kind === "command"
          ? formatCommandInsert(item.command.name)
          : formatFileInsert(item.entry.path, item.entry.kind);
      return applyCompletion(value, trigger, insert);
    },
    [trigger, items, value],
  );

  return {
    open,
    mode: open && trigger ? trigger.mode : null,
    query: open && trigger ? trigger.query : "",
    items: open ? items : [],
    hasItems: open && items.length > 0,
    highlight,
    setHighlight,
    truncated: open && trigger?.mode === "file" ? (files?.truncated ?? false) : false,
    noWorkspace: open && trigger?.mode === "file" && !hasWorkspace,
    close,
    accept,
  };
}
