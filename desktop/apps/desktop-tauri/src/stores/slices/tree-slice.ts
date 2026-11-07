import type { AppState } from "../app-state";
import type { StoreAccess } from "./types";

/**
 * Renderer-only state for the /tree and /rewind composer-dock panel. The data
 * is a snapshot of the host transcript built when a command runs; opening the
 * card itself never writes a session record.
 */
export function createTreeSlice({ set }: StoreAccess): Pick<
  AppState,
  "openTreePanel" | "closeTreePanel"
> {
  return {
    openTreePanel: (data) => set({ treePanel: data }),
    closeTreePanel: () => set({ treePanel: null }),
  };
}
