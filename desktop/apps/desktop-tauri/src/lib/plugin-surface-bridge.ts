import i18n from "i18next";
import { isThemeColorScheme } from "@pi-desktop/shared";
import { api } from "./api";
import { useAppStore } from "../stores/app-store";

/**
 * Renderer half of `window.pluginBridge` for plugin surfaces.
 *
 * A plugin view or panel is an iframe served from the shell's loopback
 * `plugin_surface` endpoint. That page is a different origin, so the only
 * channel it has back to the shell is `postMessage` to the renderer that owns
 * the iframe. This module validates which frame is talking, then forwards the
 * call to the Rust panel bridge, where permissions, fs scopes and the net
 * allowlist are enforced.
 */

type Appearance = {
  theme: string;
  base: "light" | "dark" | "system";
  locale: string;
  pluginTheme: { id: string; base: "light" | "dark"; css: string } | null;
};

type SurfaceFrame = { pluginId: string; sessionId?: string };

const frames = new Map<MessageEventSource, SurfaceFrame>();
let installed = false;

function currentAppearance(): Appearance {
  const state = useAppStore.getState();
  const preference = state.settings?.theme ?? "system";
  const pluginTheme = preference.startsWith("plugin:")
    ? state.pluginThemes.find((entry) => entry.id === preference)
    : undefined;
  const base: Appearance["base"] = pluginTheme
    ? pluginTheme.base
    : isThemeColorScheme(preference)
      ? preference
      : "system";
  return {
    theme: preference,
    base,
    locale: i18n.language || "en",
    pluginTheme: pluginTheme
      ? { id: pluginTheme.id, base: pluginTheme.base, css: pluginTheme.css }
      : null,
  };
}

function post(source: MessageEventSource, message: unknown): void {
  try {
    (source as Window).postMessage(message, "*");
  } catch {
    // The frame navigated away or closed; its next open re-registers it.
  }
}

function sendAppearance(source: MessageEventSource): void {
  post(source, {
    __piPluginHost: true,
    kind: "event",
    event: "appearance:changed",
    payload: currentAppearance(),
  });
}

/**
 * Register the iframe's window so bridge requests are attributed to the
 * plugin that owns this surface. Call as soon as the element exists (not on
 * load) so a page script's first `invoke` is answered.
 */
export function registerPluginSurfaceFrame(
  source: MessageEventSource | null,
  pluginId: string,
  sessionId?: string,
): void {
  if (!source) return;
  installListener();
  frames.set(source, { pluginId, sessionId });
  sendAppearance(source);
}

export function unregisterPluginSurfaceFrame(
  source: MessageEventSource | null,
): void {
  if (!source) return;
  frames.delete(source);
}

/** Push the current palette to every open surface after a settings change. */
export function broadcastPluginAppearance(): void {
  const appearance = currentAppearance();
  for (const source of frames.keys()) {
    post(source, {
      __piPluginHost: true,
      kind: "event",
      event: "appearance:changed",
      payload: appearance,
    });
  }
}

function installListener(): void {
  if (installed) return;
  installed = true;
  window.addEventListener("message", (event) => {
    void handleMessage(event);
  });
}

async function handleMessage(event: MessageEvent): Promise<void> {
  const data = event.data as
    | {
        __piPluginBridge?: boolean;
        kind?: string;
        pluginId?: string;
        id?: number;
        channel?: string;
        payload?: unknown;
      }
    | null;
  if (!data || data.__piPluginBridge !== true) return;
  const source = event.source;
  if (!source) return;
  const frame = frames.get(source);
  if (!frame || data.pluginId !== frame.pluginId) return;

  if (data.kind === "hello") {
    sendAppearance(source);
    return;
  }
  if (data.kind !== "request") return;

  try {
    const result = await api.pluginBridgeInvoke({
      pluginId: frame.pluginId,
      channel: String(data.channel ?? ""),
      payload: data.payload,
      sessionId: frame.sessionId,
    });
    post(source, {
      __piPluginHost: true,
      kind: "reply",
      id: data.id,
      ok: true,
      result,
    });
  } catch (error) {
    post(source, {
      __piPluginHost: true,
      kind: "reply",
      id: data.id,
      ok: false,
      error: { message: error instanceof Error ? error.message : String(error) },
    });
  }
}
