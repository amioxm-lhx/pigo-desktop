import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { FsEntry } from "@pi-desktop/shared";
import { useAppStore } from "../../stores/app-store";
import { api } from "../../lib/api";
import { cx } from "../ui";
import { IconChevronRight, IconFileText, IconFolder } from "../icons";
import { WorkTabEmpty } from "./WorkTabEmpty";

type DirState = { entries: FsEntry[]; error?: boolean };

const FILE_TAB_PREFIX = "file:";

/**
 * The workspace folder tree.
 *
 * This tab never renders file contents: clicking a file opens (or focuses) a
 * `file:<path>` tab beside it, the way an editor opens a document. The tree is
 * therefore a pure navigator, and the panel keeps it mounted while another tab
 * is active so folder expansion and scroll position survive a detour into a
 * file.
 */
export function FilesTab() {
  const { t } = useTranslation();
  const workspace = useAppStore((s) => s.workspace);
  const activeTabId = useAppStore((s) => s.activeWorkPanelTabId);
  const openFileInWorkPanel = useAppStore((s) => s.openFileInWorkPanel);
  const root = workspace?.path ?? null;
  const selected =
    activeTabId?.startsWith(FILE_TAB_PREFIX)
      ? activeTabId.slice(FILE_TAB_PREFIX.length)
      : null;

  const [dirs, setDirs] = useState<Record<string, DirState>>({});
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  // Workspace switches reset all browsing state. Guarded so it only fires on
  // an actual root change: an unconditional [root] effect also runs on the
  // StrictMode remount, wiping a tree the user had already opened.
  const prevRoot = useRef(root);
  useEffect(() => {
    if (prevRoot.current === root) return;
    prevRoot.current = root;
    setDirs({});
    setExpanded(new Set());
  }, [root]);

  const loadDir = useCallback(
    async (rel: string) => {
      if (!root) return;
      try {
        const res = await api.fsList(rel);
        setDirs((prev) => ({ ...prev, [rel]: { entries: res.entries } }));
      } catch {
        setDirs((prev) => ({ ...prev, [rel]: { entries: [], error: true } }));
      }
    },
    [root],
  );

  useEffect(() => {
    if (root && !dirs[""]) void loadDir("");
  }, [root, dirs, loadDir]);

  const toggleDir = useCallback(
    (rel: string) => {
      setExpanded((prev) => {
        const next = new Set(prev);
        if (next.has(rel)) {
          next.delete(rel);
        } else {
          next.add(rel);
        }
        return next;
      });
      if (!dirs[rel]) void loadDir(rel);
    },
    [dirs, loadDir],
  );

  const renderDir = (rel: string, depth: number): React.ReactNode => {
    const state = dirs[rel];
    if (!state) {
      return (
        <div
          className="file-tree-note"
          style={{ paddingLeft: 12 + depth * 14 }}
          key={`${rel}:loading`}
        >
          {t("panel.files.loading")}
        </div>
      );
    }
    if (state.entries.length === 0) {
      return (
        <div
          className="file-tree-note"
          style={{ paddingLeft: 12 + depth * 14 }}
          key={`${rel}:empty`}
        >
          {state.error ? t("panel.files.error") : t("panel.files.empty")}
        </div>
      );
    }
    return state.entries.map((entry) => {
      const childRel = rel ? `${rel}/${entry.name}` : entry.name;
      if (entry.kind === "dir") {
        const open = expanded.has(childRel);
        return (
          <div key={childRel}>
            <button
              type="button"
              className="file-tree-row"
              style={{ paddingLeft: 12 + depth * 14 }}
              onClick={() => toggleDir(childRel)}
            >
              <span className={cx("file-tree-caret", open && "open")} aria-hidden>
                <IconChevronRight size={12} />
              </span>
              <IconFolder size={14} />
              <span className="file-tree-name">{entry.name}</span>
            </button>
            {open && renderDir(childRel, depth + 1)}
          </div>
        );
      }
      return (
        <button
          key={childRel}
          type="button"
          className={cx("file-tree-row", selected === childRel && "active")}
          style={{ paddingLeft: 12 + depth * 14 + 16 }}
          onClick={() => openFileInWorkPanel(childRel)}
          title={childRel}
        >
          <IconFileText size={14} />
          <span className="file-tree-name">{entry.name}</span>
        </button>
      );
    });
  };

  if (!root) {
    return (
      <WorkTabEmpty
        icon={IconFolder}
        title={t("panel.files.noWorkspace")}
        body={t("panel.files.noWorkspaceHint")}
      />
    );
  }

  return <div className="file-tree">{renderDir("", 0)}</div>;
}
