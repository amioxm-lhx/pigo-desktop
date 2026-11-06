import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { BrowserState } from "@pi-desktop/shared";
import { api } from "../../lib/api";
import {
  IconChevronLeft,
  IconChevronRight,
  IconExternal,
  IconRefresh,
  IconX,
} from "../icons";

type Rect = { x: number; y: number; width: number; height: number };

function rectOf(element: HTMLElement | null): Rect | null {
  if (!element) return null;
  const rect = element.getBoundingClientRect();
  if (rect.width < 1 || rect.height < 1) return null;
  return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
}

function errorMessage(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

/**
 * Host-owned browser chrome for the native guest (`browser.rs`).
 *
 * Plugin chrome in the Electron build was itself a plugin view layered over a
 * `WebContentsView`. This port has no bundled `pi.browser` plugin, so the shell
 * draws the address bar and positions the guest inside the surface below it.
 * The guest is a child webview, which means it paints above the renderer: the
 * empty surface is only a rect carrier and every chrome control stays outside
 * it.
 */
export function BrowserHostTab({
  location,
  title,
  blocked = false,
}: {
  location: string;
  title: string;
  blocked?: boolean;
}) {
  const { t } = useTranslation();
  const shellRef = useRef<HTMLDivElement | null>(null);
  const surfaceRef = useRef<HTMLDivElement | null>(null);
  const [state, setState] = useState<BrowserState | null>(null);
  const [draft, setDraft] = useState(location);
  const [error, setError] = useState<string | null>(null);

  const reportBounds = useCallback(() => {
    const hole = rectOf(surfaceRef.current);
    const view = rectOf(shellRef.current);
    if (!hole || !view) return;
    void api.browserSetBounds({ ...hole, view }).catch(() => undefined);
  }, []);

  // The child webview does not follow CSS layout: report the surface rect on
  // every resize and after a navigation creates the guest.
  useLayoutEffect(() => {
    reportBounds();
    const observer = new ResizeObserver(reportBounds);
    if (surfaceRef.current) observer.observe(surfaceRef.current);
    if (shellRef.current) observer.observe(shellRef.current);
    window.addEventListener("resize", reportBounds);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", reportBounds);
    };
  }, [reportBounds]);

  useEffect(() => {
    let cancelled = false;
    setDraft(location);
    setError(null);
    const off = api.onBrowserState((next) => {
      if (!cancelled) setState(next);
    });
    void (async () => {
      try {
        const current = await api.browserGetState();
        if (cancelled) return;
        if (!current || current.url !== location) {
          const next = await api.browserNavigate(location);
          if (cancelled) return;
          setState(next);
          setDraft(next.url);
        } else {
          setState(current);
          setDraft(current.url);
        }
        await api.browserSetVisible(true);
        reportBounds();
      } catch (failure) {
        if (!cancelled) setError(errorMessage(failure));
      }
    })();
    return () => {
      cancelled = true;
      off();
      // Switching tabs keeps the singleton, but it must not paint over the
      // next tab. A new mount shows it again.
      void api.browserSetVisible(false).catch(() => undefined);
    };
  }, [location, reportBounds]);

  // A modal or a blocked session takes the guest off screen; pointer-events
  // cannot reach a native webview.
  useEffect(() => {
    void api.browserSetVisible(!blocked).catch(() => undefined);
  }, [blocked]);

  const act = async (action: "back" | "forward" | "reload" | "stop") => {
    try {
      await api.browserAction(action);
    } catch (failure) {
      setError(errorMessage(failure));
    }
  };

  const go = async (raw: string) => {
    const target = raw.trim();
    if (!target) return;
    setError(null);
    try {
      const next = await api.browserNavigate(target);
      setState(next);
      setDraft(next.url);
      await api.browserSetVisible(true);
      reportBounds();
    } catch (failure) {
      setError(errorMessage(failure));
    }
  };

  return (
    <div className="work-browser" ref={shellRef} aria-label={title}>
      <form
        className="work-browser-chrome"
        onSubmit={(event) => {
          event.preventDefault();
          void go(draft);
        }}
      >
        <button
          type="button"
          className="icon-btn"
          disabled={!state?.canGoBack}
          aria-label={t("panel.browser.back")}
          onClick={() => void act("back")}
        >
          <IconChevronLeft size={15} />
        </button>
        <button
          type="button"
          className="icon-btn"
          disabled={!state?.canGoForward}
          aria-label={t("panel.browser.forward")}
          onClick={() => void act("forward")}
        >
          <IconChevronRight size={15} />
        </button>
        <button
          type="button"
          className="icon-btn"
          aria-label={
            state?.isLoading ? t("panel.browser.stop") : t("panel.browser.reload")
          }
          onClick={() => void act(state?.isLoading ? "stop" : "reload")}
        >
          {state?.isLoading ? <IconX size={15} /> : <IconRefresh size={15} />}
        </button>
        <div className="work-browser-url">
          <input
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            spellCheck={false}
            autoComplete="off"
            aria-label={t("panel.browser.address")}
            placeholder={t("panel.browser.address")}
          />
        </div>
        <button
          type="button"
          className="icon-btn"
          aria-label={t("panel.browser.openExternal")}
          onClick={() => {
            const url = state?.url || location;
            void api.browserOpenExternal(url).catch(() => undefined);
          }}
        >
          <IconExternal size={15} />
        </button>
      </form>
      {error ? (
        <div className="work-browser-error" role="alert">
          {error}
        </div>
      ) : null}
      <div className="work-browser-surface" ref={surfaceRef} />
    </div>
  );
}
