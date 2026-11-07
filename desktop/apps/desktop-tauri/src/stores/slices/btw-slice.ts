import i18n from "i18next";
import type { AgentEventEnvelope, BtwEvent } from "@pi-desktop/shared";
import { api } from "../../lib/api";
import type { AppState } from "../app-state";
import type { StoreAccess } from "./types";

/**
 * Renderer-owned /btw side thread. Nothing here touches `messages`,
 * `sessionHistory` or any host persistence: scoped events are diverted by
 * createEventsSlice before they reach the transcript reducer, and the shell
 * skips persistence for scope === "side".
 */
export function createBtwSlice({ get, set }: StoreAccess): Pick<
  AppState,
  "openBtw" | "closeBtw" | "handleBtwEvent"
> {
  const cancel = (sideId: string) => {
    void api.btwCancel(sideId).catch(() => undefined);
  };

  return {
    openBtw: async (question: string) => {
      const text = question.trim();
      const sessionId = get().activeSessionId;
      if (!sessionId) {
        get().showToast(
          i18n.t("chat.btwNeedsSession", {
            defaultValue: "Open a conversation before asking a side question.",
          }),
          { variant: "info" },
        );
        return;
      }
      if (!text) return;

      const previous = get().btwThread;
      if (previous?.running) cancel(previous.sideId);

      const sideId = newSideId();
      set({
        btwThread: {
          sideId,
          sessionId,
          question: text,
          answer: "",
          running: true,
        },
      });
      try {
        await api.btw({ sessionId, sideId, question: text });
      } catch (error) {
        set((state) =>
          state.btwThread?.sideId === sideId
            ? {
                btwThread: {
                  ...state.btwThread,
                  running: false,
                  error: errorMessage(error),
                },
              }
            : {},
        );
      }
    },

    closeBtw: () => {
      const thread = get().btwThread;
      if (thread?.running) cancel(thread.sideId);
      set({ btwThread: null });
    },

    handleBtwEvent: (envelope: AgentEventEnvelope) => {
      const event = envelope.event as unknown as BtwEvent;
      const thread = get().btwThread;
      if (!thread) return;
      const sideId = event.sideId || envelope.sideId || thread.sideId;
      // A late answer from a closed or superseded thread must not repaint the
      // card the user is currently looking at.
      if (sideId !== thread.sideId) return;

      switch (event.type) {
        case "btw_start":
          return;
        case "btw_delta":
          set((state) => {
            const current = state.btwThread;
            if (!current || current.sideId !== sideId) return {};
            return {
              btwThread: {
                ...current,
                answer: event.reset ? event.text : current.answer + event.text,
              },
            };
          });
          return;
        case "btw_end":
          set((state) => {
            const current = state.btwThread;
            if (!current || current.sideId !== sideId) return {};
            return {
              btwThread: {
                ...current,
                answer: event.text || current.answer,
                running: false,
                aborted: event.aborted,
              },
            };
          });
          return;
        case "btw_error":
          set((state) => {
            const current = state.btwThread;
            if (!current || current.sideId !== sideId) return {};
            return {
              btwThread: {
                ...current,
                running: false,
                error: event.error.message || event.error.code,
              },
            };
          });
          return;
        default:
          return;
      }
    },
  };
}

function newSideId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `btw-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
