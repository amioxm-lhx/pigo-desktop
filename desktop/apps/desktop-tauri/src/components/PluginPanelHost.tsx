import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { IconClose } from "./icons";
import { TooltipButton } from "./ui";
import { api } from "../lib/api";
import {
  registerPluginSurfaceFrame,
  unregisterPluginSurfaceFrame,
} from "../lib/plugin-surface-bridge";
import { usePluginPanelStore } from "../stores/plugin-panel-store";
import { useAppStore } from "../stores/app-store";

/**
 * Detached `ui.panel` surface.
 *
 * Electron opened a real window per plugin panel. The Tauri shell draws the
 * window chrome inside the renderer and hosts the plugin page in an iframe
 * served by the loopback plugin-surface service; the page gets the same
 * `window.pluginBridge` it would have in a detached window.
 */
export function PluginPanelHost() {
  const { t } = useTranslation();
  const panel = usePluginPanelStore((state) => state.panel);
  const close = usePluginPanelStore((state) => state.close);
  const show = usePluginPanelStore((state) => state.show);
  const closeForPlugin = usePluginPanelStore((state) => state.closeForPlugin);
  const frameRef = useRef<HTMLIFrameElement | null>(null);

  // Plugin code opens/closes its own panel through the reverse bridge.
  useEffect(
    () =>
      api.onPluginPanel((event) => {
        if (event.action === "close") {
          closeForPlugin(event.pluginId);
          return;
        }
        if (!event.url) return;
        show({
          pluginId: event.pluginId,
          url: event.url,
          title: event.title ?? event.pluginId,
          width: event.width ?? 520,
          height: event.height ?? 640,
          sessionId: useAppStore.getState().activeSessionId ?? undefined,
        });
      }),
    [closeForPlugin, show],
  );

  // A plugin that unloads or crashes can no longer serve its surface.
  useEffect(
    () =>
      api.onPluginChanged((event) => {
        if (!event.pluginId) return;
        if (event.reason === "unload" || event.reason === "crash") {
          closeForPlugin(event.pluginId);
        }
      }),
    [closeForPlugin],
  );

  const pluginId = panel?.pluginId;
  const url = panel?.url;
  const sessionId = panel?.sessionId;
  useEffect(() => {
    if (!pluginId || !url) return;
    const source = frameRef.current?.contentWindow ?? null;
    registerPluginSurfaceFrame(source, pluginId, sessionId);
    return () => unregisterPluginSurfaceFrame(source);
  }, [pluginId, url, sessionId]);

  useEffect(() => {
    if (!panel) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [panel, close]);

  if (!panel) return null;
  const width = Math.min(panel.width, Math.round(window.innerWidth * 0.92));
  const height = Math.min(panel.height, Math.round(window.innerHeight * 0.88));

  return (
    <div
      className="overlay plugin-panel-overlay"
      role="presentation"
      onClick={(event) => {
        if (event.target === event.currentTarget) close();
      }}
    >
      <section
        className="dialog plugin-panel-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={panel.title}
        style={{ width, height }}
      >
        <header className="plugin-panel-header">
          <h2 className="plugin-panel-title">{panel.title}</h2>
          <TooltipButton
            type="button"
            className="icon-btn plugin-panel-close"
            tooltip={t("common.close")}
            ariaLabel={t("common.close")}
            onClick={close}
          >
            <IconClose size={16} />
          </TooltipButton>
        </header>
        <iframe
          ref={frameRef}
          className="plugin-panel-frame"
          title={panel.title}
          src={panel.url}
          sandbox="allow-scripts allow-same-origin allow-forms allow-modals"
        />
      </section>
    </div>
  );
}
