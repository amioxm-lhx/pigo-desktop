import { useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import type { FsReadResult } from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { Markdown } from "../Markdown";
import { fileDirOf } from "../../lib/chat-links";
import { TooltipButton } from "../ui";
import {
  ensureLang,
  getHighlightVersion,
  resolveLang,
  subscribeHighlighter,
  themeForMode,
  tokenizeIncremental,
  type LineCache,
} from "../../lib/shiki";
import { IconExternal, IconFileText } from "../icons";
import { WorkTabEmpty } from "./WorkTabEmpty";

const VIEWER_LINE_CAP = 5000;

const EXT_LANG: Record<string, string> = {
  ts: "typescript",
  tsx: "tsx",
  js: "javascript",
  jsx: "jsx",
  mjs: "javascript",
  cjs: "javascript",
  json: "json",
  css: "css",
  html: "html",
  md: "markdown",
  rs: "rust",
  py: "python",
  go: "go",
  sh: "shellscript",
  zsh: "shellscript",
  bash: "shellscript",
  yml: "yaml",
  yaml: "yaml",
  toml: "toml",
  sql: "sql",
  swift: "swift",
  kt: "kotlin",
  java: "java",
  c: "c",
  h: "c",
  cpp: "cpp",
  hpp: "cpp",
};

function langForPath(path: string): string | null {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  const lang = EXT_LANG[ext] ?? ext;
  return resolveLang(lang);
}

function isMarkdownPath(path: string): boolean {
  return /\.(?:md|markdown)$/i.test(path);
}

function baseNameOf(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

function formatSize(size: number): string {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

function useThemeMode(): "light" | "dark" {
  return useSyncExternalStore(
    (notify) => {
      const observer = new MutationObserver(notify);
      observer.observe(document.documentElement, {
        attributes: true,
        attributeFilter: ["data-theme"],
      });
      return () => observer.disconnect();
    },
    () => (document.documentElement.dataset.theme === "light" ? "light" : "dark"),
  );
}

function HighlightedText({ path, content }: { path: string; content: string }) {
  const mode = useThemeMode();
  const highlightVersion = useSyncExternalStore(
    subscribeHighlighter,
    getHighlightVersion,
  );
  const lang = langForPath(path);
  const lines = useMemo(() => content.split("\n"), [content]);
  const capped = lines.length > VIEWER_LINE_CAP;
  const visible = useMemo(
    () => (capped ? lines.slice(0, VIEWER_LINE_CAP).join("\n") : content),
    [capped, lines, content],
  );

  const tokens = useMemo<LineCache | null>(() => {
    void highlightVersion;
    if (!lang) return null;
    ensureLang(lang);
    return tokenizeIncremental(null, visible, lang, themeForMode(mode));
  }, [lang, visible, mode, highlightVersion]);

  // One row per source line: a sticky gutter keeps the line numbers readable
  // while the body scrolls sideways, so a long line never hides its own row.
  const rows: { key: number; gutter: number; body: React.ReactNode }[] = tokens
    ? tokens.tokens.map((row, i) => ({
        key: i,
        gutter: i + 1,
        body:
          row.length === 0
            ? "\n"
            : row.map((token, j) => (
                <span key={j} style={{ color: token.color }}>
                  {token.content}
                </span>
              )),
      }))
    : visible.split("\n").map((line, i) => ({
        key: i,
        gutter: i + 1,
        body: line || "\n",
      }));

  return (
    <pre className="file-viewer-code">
      {rows.map((row) => (
        <div className="file-viewer-line" key={row.key}>
          <span className="file-viewer-gutter" aria-hidden>
            {row.gutter}
          </span>
          <span className="file-viewer-source">{row.body}</span>
        </div>
      ))}
      {capped && <div className="file-viewer-cap">…</div>}
    </pre>
  );
}

/**
 * A single file rendered in its own work-panel tab.
 *
 * Every open file owns a `file:<path>` tab (see `fileWorkPanelTab`), so this
 * component reads its own resource instead of sharing one in-place viewer
 * with the folder tree. That is what lets several files stay open side by
 * side, each keeping its own tab label and scroll position.
 */
export function FileViewer({
  path,
  mimeType,
}: {
  /** Workspace-relative path, attachment ref, or absolute scratch path. */
  path: string;
  mimeType?: string;
}) {
  const { t } = useTranslation();
  const [file, setFile] = useState<FsReadResult | null>(null);
  const [error, setError] = useState(false);

  // The header names the file once; the muted hint is its containing folder,
  // which is empty for a root-level file and would only repeat the name.
  const name = baseNameOf(path);
  const dir = path.slice(0, Math.max(0, path.length - name.length)).replace(/[\\/]+$/, "");

  useEffect(() => {
    let cancelled = false;
    setFile(null);
    setError(false);
    api
      .fsRead(path, mimeType)
      .then((result) => {
        if (!cancelled) setFile(result);
      })
      .catch(() => {
        if (!cancelled) setError(true);
      });
    return () => {
      cancelled = true;
    };
  }, [path, mimeType]);

  return (
    <div className="file-viewer">
      <div className="file-viewer-header">
        <span className="file-viewer-name" title={path}>
          {name}
        </span>
        {dir ? (
          <span className="file-viewer-path" title={path}>
            {dir}
          </span>
        ) : null}
        {file && <span className="file-viewer-size">{formatSize(file.size)}</span>}
        <TooltipButton
          type="button"
          className="icon-btn"
          tooltip={t("panel.files.reveal")}
          ariaLabel={t("panel.files.reveal")}
          onClick={() => void api.fsReveal(path)}
        >
          <IconExternal size={14} />
        </TooltipButton>
      </div>
      <div className="file-viewer-body">
        {error ? (
          <WorkTabEmpty icon={IconFileText} title={t("panel.files.error")} />
        ) : !file ? (
          <div className="file-tree-note">{t("panel.files.loading")}</div>
        ) : file.kind === "text" && isMarkdownPath(path) ? (
          <div className="file-viewer-markdown prose-chat">
            <Markdown source={file.content ?? ""} baseDir={fileDirOf(path)} />
          </div>
        ) : file.kind === "text" ? (
          <HighlightedText path={path} content={file.content ?? ""} />
        ) : file.kind === "image" ? (
          <div className="file-viewer-image">
            <img src={file.dataUrl} alt={path} />
          </div>
        ) : (
          <WorkTabEmpty
            icon={IconFileText}
            title={
              file.kind === "tooLarge"
                ? t("panel.files.tooLarge")
                : t("panel.files.binary")
            }
          />
        )}
      </div>
    </div>
  );
}
