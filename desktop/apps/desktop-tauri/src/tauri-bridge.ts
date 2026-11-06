/**
 * Tauri shell bridge — installs `window.piDesktop` on top of Tauri's
 * invoke/event surface so the renderer runs unmodified under the
 * `apps/desktop-tauri` shell. No-op under Electron (the preload already
 * exposed the real bridge; `__TAURI_INTERNALS__` only exists under wry).
 *
 * Channel contract is byte-identical to `electron/preload/index.ts`:
 * - invoke resolves with the shared `Result` envelope (never rejects)
 * - on returns a synchronous unlisten function
 * - channels/platform/locale are available before first paint
 */
import { IPC, IPC_WHITELIST, type Result, err, ErrorCodes } from "@pi-desktop/shared";

type TauriGlobal = {
  core?: { invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown> };
  event?: { listen: (name: string, handler: (event: { payload?: unknown }) => void) => Promise<() => void> };
  webview?: { getCurrentWebview?: () => { onDragDropEvent?: (cb: (event: { payload?: { type?: string; paths?: string[] } }) => void) => Promise<() => void> } };
};

function tauri(): TauriGlobal | null {
  if (typeof window === "undefined") return null;
  if (!("__TAURI_INTERNALS__" in window)) return null;
  return (window as unknown as { __TAURI__?: TauriGlobal }).__TAURI__ ?? null;
}

function detectPlatform(): string {
  const ua = navigator.userAgent;
  if (ua.includes("Windows")) return "win32";
  if (ua.includes("Mac")) return "darwin";
  return "linux";
}

function basename(path: string): string {
  return path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path;
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000;
  for (let offset = 0; offset < bytes.length; offset += chunk) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + chunk));
  }
  return btoa(binary);
}

/**
 * Tauri's invoke serializes through JSON, so an ArrayBuffer (the paste
 * attachment payload) would arrive as `{}`. Encode binary views as
 * `{"__bytes_b64": …}`; the Rust shell decodes that shape explicitly.
 */
function toTransport(value: unknown, seen: WeakMap<object, unknown>): unknown {
  if (value instanceof ArrayBuffer) {
    return { __bytes_b64: bytesToBase64(new Uint8Array(value)) };
  }
  if (ArrayBuffer.isView(value)) {
    const view = value as Uint8Array;
    return {
      __bytes_b64: bytesToBase64(
        new Uint8Array(view.buffer, view.byteOffset, view.byteLength),
      ),
    };
  }
  if (value && typeof value === "object") {
    const cached = seen.get(value as object);
    if (cached !== undefined) return cached;
    if (Array.isArray(value)) {
      const copy: unknown[] = [];
      seen.set(value, copy);
      value.forEach((item, index) => {
        copy[index] = toTransport(item, seen);
      });
      return copy;
    }
    const source = value as Record<string, unknown>;
    const copy: Record<string, unknown> = {};
    seen.set(value as object, copy);
    for (const [key, item] of Object.entries(source)) {
      copy[key] = toTransport(item, seen);
    }
    return copy;
  }
  return value;
}

export function installTauriBridge(): void {
  const tauriApi = tauri();
  if (!tauriApi) return;
  const core = tauriApi.core;
  const events = tauriApi.event;
  if (!core || !events) return;

  // Electron shows no native context menu on any platform; WebView2 ships a
  // default one (back/forward/reload/print). Suppress it for parity — the
  // app's own menus cover those actions.
  document.addEventListener("contextmenu", (event) => event.preventDefault());

  // ── events: one tauri listener per channel, sync fan-out to consumers ──
  //
  // Electron unsubscribed synchronously. Tauri's `listen` resolves to the
  // unlisten handle, so it has to be stored when it resolves; treating that
  // promise as fire-and-forget left a live OS listener behind on every full
  // unsubscribe (StrictMode remounts included). Each remount then added another
  // listener, and every agent event was applied once per stale listener — a
  // streaming answer grew duplicated tokens.
  //
  // The registry lives on `globalThis` so a dev HMR reload of this module
  // reuses the listeners a previous generation already registered.
  type BridgeEventRegistry = {
    listeners: Map<string, Set<(...args: unknown[]) => void>>;
    unlistenByChannel: Map<string, () => void>;
  };
  const registryKey = Symbol.for("pi.desktop.tauri-bridge.events");
  const registry: BridgeEventRegistry =
    ((globalThis as Record<PropertyKey, unknown>)[registryKey] as
      | BridgeEventRegistry
      | undefined) ?? {
      listeners: new Map(),
      unlistenByChannel: new Map(),
    };
  (globalThis as Record<PropertyKey, unknown>)[registryKey] = registry;
  const { listeners, unlistenByChannel } = registry;

  const ensureTauriListener = (channel: string) => {
    if (unlistenByChannel.has(channel)) return;
    const pending = () => {};
    unlistenByChannel.set(channel, pending); // re-entrancy guard
    void events
      .listen(channel, (event) => {
        const set = listeners.get(channel);
        if (!set) return;
        for (const listener of set) listener(event?.payload);
      })
      .then((unlisten) => {
        // The channel was released while `listen` was resolving.
        if (unlistenByChannel.get(channel) !== pending) {
          unlisten();
          return;
        }
        unlistenByChannel.set(channel, unlisten);
      });
  };

  // ── drag-drop paths: WebView2 populates File objects without paths, so
  // capture the OS paths from Tauri's drag-drop event and match by name ──
  let lastDropPaths: string[] = [];
  const webview = tauriApi.webview?.getCurrentWebview?.();
  if (webview?.onDragDropEvent) {
    void webview.onDragDropEvent((event) => {
      const payload = event?.payload;
      if (payload?.type === "drop" && Array.isArray(payload.paths)) {
        lastDropPaths = payload.paths;
      }
    });
  }

  window.piDesktop = {
    invoke: async <T = unknown>(channel: string, ...args: unknown[]): Promise<Result<T>> => {
      if (!IPC_WHITELIST.has(channel)) {
        return err(ErrorCodes.FORBIDDEN, `IPC channel not allowed: ${channel}`);
      }
      try {
        const transportArgs = args.map((arg) => toTransport(arg, new WeakMap()));
        const envelope = (await core.invoke("pi_desktop_invoke", {
          channel,
          args: transportArgs,
        })) as Result<T>;
        if (envelope && typeof envelope === "object" && "ok" in envelope) {
          return envelope;
        }
        return err(ErrorCodes.INTERNAL, "malformed envelope from shell");
      } catch (error) {
        return err(
          ErrorCodes.INTERNAL,
          error instanceof Error ? error.message : String(error),
        );
      }
    },
    on: (channel: string, listener: (...args: unknown[]) => void) => {
      let set = listeners.get(channel);
      if (!set) {
        set = new Set();
        listeners.set(channel, set);
      }
      set.add(listener);
      ensureTauriListener(channel);
      return () => {
        set?.delete(listener);
        if (set && set.size === 0) {
          listeners.delete(channel);
          unlistenByChannel.get(channel)?.();
          unlistenByChannel.delete(channel);
        }
      };
    },
    channels: IPC,
    platform: detectPlatform() as NodeJS.Platform,
    locale: navigator.language,
    getDroppedFilePath: (file: File): string | null => {
      if (lastDropPaths.length === 0) return null;
      const byName = lastDropPaths.find((path) => basename(path) === file.name);
      if (byName) return byName;
      return lastDropPaths.length === 1 ? lastDropPaths[0] : null;
    },
  };
}

installTauriBridge();
