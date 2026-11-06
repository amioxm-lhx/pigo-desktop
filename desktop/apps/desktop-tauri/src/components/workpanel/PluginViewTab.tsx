import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../lib/api";
import { pluginViewIcon } from "../../lib/plugin-view-icons";
import {
  registerPluginSurfaceFrame,
  unregisterPluginSurfaceFrame,
} from "../../lib/plugin-surface-bridge";
import { BROWSER_PLUGIN_TAB } from "../../lib/work-panel-tabs";
import { IconPlug } from "../icons";
import { BrowserHostTab } from "./BrowserHostTab";
import { WorkTabEmpty } from "./WorkTabEmpty";

/**
 * A plugin-contributed work panel view (ADR 0104).
 *
 * Electron composited a main-process `WebContentsView` above this placeholder.
 * The Tauri shell has no native overlay, so the view is an iframe served by
 * the loopback plugin-surface service — the same shape as the browser tab.
 * The host draws the tab chrome; the plugin page lays itself out and talks
 * back through `window.pluginBridge`.
 */
export function PluginViewTab({
  pluginId,
  viewId,
  title,
  icon,
  blocked = false,
  sessionId,
  location,
}: {
  pluginId: string;
  viewId: string;
  title: string;
  icon?: string;
  blocked?: boolean;
  sessionId?: string;
  location?: string;
}) {
  const { t } = useTranslation();
  const frameRef = useRef<HTMLIFrameElement | null>(null);
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  // The host-owned browser preview is a renderer iframe fed by the shell's
  // loopback preview service; it is not a plugin surface.
  const isBrowserPreview =
    pluginId === BROWSER_PLUGIN_TAB.pluginId && viewId === BROWSER_PLUGIN_TAB.viewId;

  // Create the surface, and re-create it whenever the plugin's lifecycle
  // changed underneath us: a crash, a development reload, or a re-enable all
  // revoke the previous URL while this tab stays open.
  useEffect(() => {
    if (isBrowserPreview) return;
    let current = true;
    const open = () => {
      void api.pluginViewOpen(pluginId, viewId, { sessionId, location }).then(
        (result) => {
          if (!current) return;
          setUrl((result as { url: string }).url);
          setFailed(false);
        },
        () => {
          if (current) setFailed(true);
        },
      );
    };
    open();
    const off = api.onPluginChanged((event) => {
      if (event?.pluginId && event.pluginId !== pluginId) return;
      open();
    });
    return () => {
      current = false;
      off();
    };
  }, [pluginId, viewId, sessionId, location, isBrowserPreview]);

  useEffect(() => {
    if (isBrowserPreview || !url) return;
    const source = frameRef.current?.contentWindow ?? null;
    registerPluginSurfaceFrame(source, pluginId, sessionId);
    return () => unregisterPluginSurfaceFrame(source);
  }, [pluginId, url, sessionId, isBrowserPreview]);

  if (isBrowserPreview) {
    if (!location) {
      return (
        <div className="work-plugin-view">
          <WorkTabEmpty
            icon={pluginViewIcon(icon) ?? IconPlug}
            title={t("panel.browser.emptyTitle")}
            body={t("panel.browser.empty")}
          />
        </div>
      );
    }
    return (
      <BrowserHostTab location={location} title={title} blocked={blocked} />
    );
  }

  if (failed) {
    return (
      <div className="work-plugin-view">
        <WorkTabEmpty
          icon={pluginViewIcon(icon) ?? IconPlug}
          title={title}
          body={t("panel.pluginView.failed")}
        />
      </div>
    );
  }

  return (
    <div className="work-plugin-view">
      {url ? (
        <iframe
          ref={frameRef}
          className="work-plugin-view-frame"
          title={title}
          src={url}
          sandbox="allow-scripts allow-same-origin allow-forms allow-modals"
          style={{
            width: "100%",
            height: "100%",
            border: 0,
            pointerEvents: blocked ? "none" : undefined,
          }}
        />
      ) : (
        <div className="work-plugin-view-surface" />
      )}
    </div>
  );
}
