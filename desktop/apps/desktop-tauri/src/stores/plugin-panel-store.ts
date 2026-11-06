import { create } from "zustand";
import { api } from "../lib/api";
import { useAppStore } from "./app-store";

/** The host-drawn panel window a plugin's `ui.panel` entry renders into. */
export type PluginPanelSurface = {
  pluginId: string;
  url: string;
  title: string;
  width: number;
  height: number;
  /** Session the panel was opened from; `getLlmContext` reads it. */
  sessionId?: string;
};

type PluginPanelStore = {
  panel: PluginPanelSurface | null;
  /** Ask the shell to resolve `ui.panel` and show it. */
  open: (pluginId: string) => Promise<void>;
  /** Push a panel the shell opened on behalf of plugin code. */
  show: (panel: PluginPanelSurface) => void;
  close: () => void;
  closeForPlugin: (pluginId: string) => void;
};

export const usePluginPanelStore = create<PluginPanelStore>((set, get) => ({
  panel: null,
  open: async (pluginId) => {
    const resolved = (await api.openPluginPanel(pluginId)) as Omit<
      PluginPanelSurface,
      "sessionId"
    >;
    set({
      panel: {
        ...resolved,
        sessionId: useAppStore.getState().activeSessionId ?? undefined,
      },
    });
  },
  show: (panel) => set({ panel }),
  close: () => set({ panel: null }),
  closeForPlugin: (pluginId) => {
    if (get().panel?.pluginId === pluginId) {
      set({ panel: null });
    }
  },
}));
