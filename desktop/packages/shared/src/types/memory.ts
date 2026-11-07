/**
 * Persistent-memory (/memory) and memory-consolidation (/dream) payloads.
 *
 * Both mirror the pigo Go types verbatim: /memory reports what the sidecar's
 * memory index holds, and /dream returns the CLI's Report so a desktop run and
 * `pigo --dream` describe the same pass the same way.
 */

/** One /memory report: the store state plus per-scope entry counts. */
export type MemoryStoreReport = {
  /** False when the memory root cannot be resolved at all. */
  enabled: boolean;
  /** Memory root directory (global/projects/sessions layout). */
  root?: string;
  /** SQLite index stored next to that root. */
  dbPath?: string;
  /** Entry counts keyed by scope: global / projects / sessions / cc. */
  counts: Record<string, number>;
  total: number;
  /** Reconcile deltas applied before counting. */
  indexed: number;
  pruned: number;
  /** Non-fatal open/reconcile failure; the card still renders. */
  error?: string;
};

/**
 * One /dream change report. Field names follow the CLI's JSON contract
 * (`pigo --dream` prints this object on one line), so the desktop never
 * renames a counter.
 */
export type DreamReport = {
  merged: number;
  deduped: number;
  paths_cleaned: number;
  pruned: number;
  distilled: number;
  bytes_before: number;
  bytes_after: number;
  files_before: number;
  files_after: number;
  dry_run: boolean;
  notes?: string[];
  reconciled: { indexed: number; pruned: number };
};
