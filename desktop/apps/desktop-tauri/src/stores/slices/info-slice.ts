import type { AppState } from "../app-state";
import type { StoreAccess } from "./types";

/**
 * Renderer-only /session and /status report. Both views read existing store
 * state (session summary, transcript, compactions, settings, providers) and
 * issue no host RPC, so opening one never changes the conversation.
 */
export function createInfoSlice({ set }: StoreAccess): Pick<
  AppState,
  "openInfoPanel" | "closeInfoPanel"
> {
  return {
    openInfoPanel: (kind) => set({ infoPanel: kind }),
    closeInfoPanel: () => set({ infoPanel: null }),
  };
}
