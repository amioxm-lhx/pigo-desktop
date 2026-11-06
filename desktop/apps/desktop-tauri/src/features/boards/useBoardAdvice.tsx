/**
 * The proposals one board shows, and the one action that answers them.
 *
 * Every board carries the same rail, so it is loaded and decided in one place:
 * a board that accepted a proposal with different rules than its neighbour
 * would make "the owner approved this" mean two things.
 *
 * Refreshing is event-driven rather than polled. A run that files cards
 * notifies the renderer, and the board reloads because something actually
 * happened -- including a proposal a different employee just raised.
 */
import { useCallback, useEffect, useState } from "react";
import type { StaffSuggestion, StaffSuggestionBoard } from "@pi-desktop/shared";
import { api } from "../../lib/api";

export type BoardAdvice = {
  suggestions: StaffSuggestion[];
  busyId: string | null;
  analyzing: boolean;
  refresh: () => Promise<void>;
  analyze: () => Promise<void>;
  decide: (id: string, decision: "accepted" | "rejected") => Promise<void>;
};

export function useBoardAdvice(
  board: StaffSuggestionBoard,
  onError?: (message: string) => void,
): BoardAdvice {
  const [suggestions, setSuggestions] = useState<StaffSuggestion[]>([]);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [analyzing, setAnalyzing] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const answer = await api.listStaffAdvice({ board });
      setSuggestions(answer.suggestions ?? []);
    } catch {
      setSuggestions([]);
    }
  }, [board]);

  useEffect(() => {
    void refresh();
    // A run that files a card or a draft may also have raised a proposal on
    // this board, so any staff change is worth a re-read.
    const off = api.onStaffChanged(() => void refresh());
    return off;
  }, [refresh]);

  const analyze = useCallback(async () => {
    setAnalyzing(true);
    try {
      await api.deriveStaffAdvice();
      await refresh();
    } catch (error) {
      onError?.(String(error));
    } finally {
      setAnalyzing(false);
    }
  }, [onError, refresh]);

  const decide = useCallback(
    async (id: string, decision: "accepted" | "rejected") => {
      setBusyId(id);
      try {
        const answer = await api.decideStaffAdvice({ id, decision });
        setSuggestions((current) =>
          current.map((item) => (item.id === id ? answer.suggestion : item)),
        );
      } catch (error) {
        onError?.(String(error));
      } finally {
        setBusyId(null);
      }
    },
    [onError],
  );

  return { suggestions, busyId, analyzing, refresh, analyze, decide };
}

/** A board header: the same strip on every page, so they read as one product. */
export function BoardHeader({
  title,
  subtitle,
  actions,
}: {
  title: string;
  subtitle: string;
  actions?: import("react").ReactNode;
}) {
  return (
    <div className="page-header">
      <div>
        <h1 className="page-title">{title}</h1>
        <p className="staff-hint">{subtitle}</p>
      </div>
      {actions ? <div className="board-actions">{actions}</div> : null}
    </div>
  );
}
