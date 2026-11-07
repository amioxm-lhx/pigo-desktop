import type {
  ReviewChangeState,
  SessionSummary,
  UiMessage,
} from "@pi-desktop/shared";
import { reviewChangeFromMessage } from "./workspace-review";

/**
 * Pure data builders for the desktop equivalents of REPL `/tree` and
 * `/rewind`.
 *
 * The desktop transcript is a linear host-owned JSONL file: sessions have no
 * parent/child lineage and messages have no parent pointer (ADR 0023). What
 * does exist is a regenerate variant family per user turn (`revisionRootId` /
 * `revisionCount` / `activeRevision`) and message-owned file snapshots for the
 * `Write` / `Edit` tools (ADR 0043). These builders only describe that real
 * data; they never invent a branch topology.
 */

/** One numbered row in the `/tree` node list. `index` is 1-based. */
export type SessionTreeNode = {
  index: number;
  id: string;
  role: string;
  label: string;
  toolName?: string;
  revisionRootId?: string;
  revisionCount?: number;
  activeRevision?: number;
  subagent?: boolean;
};

/** One reversible file mutation attached to a tool message. */
export type RewindChange = {
  messageId: string;
  snapshotId: string;
  path: string;
  state: ReviewChangeState;
};

/**
 * A restore point: one turn that produced at least one journaled Write/Edit
 * mutation. Matches the REPL's "file edits create restore points" rule.
 * `index` is 1-based and oldest-first.
 */
export type RewindPoint = {
  index: number;
  label: string;
  createdAt: string;
  /** Position of the turn's first message in the full transcript. */
  startIndex: number;
  /** Message id immediately before this turn, or null for the first turn. */
  beforeMessageId: string | null;
  changes: RewindChange[];
};

/**
 * One session in the fork lineage. A branch is a real session on the host, so
 * the tree is built from `session.list` metadata rather than from message
 * pointers: the desktop transcript has no parent ids (ADR 0023), but schema v22
 * records which session and message a fork was cut from.
 */
export type SessionForestNode = {
  id: string;
  title: string;
  /** Message the branch was cut at, when the fork was message-scoped. */
  cutAtMessageId?: string;
  /** 1 for a root session, 2 for its direct branches, and so on. */
  depth: number;
  isActive: boolean;
  /** The recorded parent is gone (deleted elsewhere or filtered out). */
  orphaned: boolean;
  children: SessionForestNode[];
};

type ForestInput = Pick<
  SessionSummary,
  "id" | "title" | "parentSessionId" | "forkedFromMessageId"
>;

/**
 * Assemble the fork lineage of every session.
 *
 * A session whose parent is missing from the input is promoted to a root and
 * flagged `orphaned`, so a deleted source never hides its branches. Cycles are
 * impossible through the fork path, but a hand-edited database could still
 * produce one; any node left unvisited after the first pass is promoted too,
 * which keeps the list lossless instead of silently dropping sessions.
 */
export function buildSessionForest(
  sessions: ForestInput[],
  activeSessionId?: string | null,
): SessionForestNode[] {
  const nodes = new Map<string, SessionForestNode>();
  for (const session of sessions) {
    nodes.set(session.id, {
      id: session.id,
      title: session.title || session.id,
      ...(session.forkedFromMessageId
        ? { cutAtMessageId: session.forkedFromMessageId }
        : {}),
      depth: 1,
      isActive: session.id === activeSessionId,
      orphaned: false,
      children: [],
    });
  }

  const roots: SessionForestNode[] = [];
  for (const session of sessions) {
    const node = nodes.get(session.id);
    if (!node) continue;
    const parent = session.parentSessionId
      ? nodes.get(session.parentSessionId)
      : undefined;
    if (parent && parent !== node) {
      parent.children.push(node);
      continue;
    }
    if (session.parentSessionId) node.orphaned = true;
    roots.push(node);
  }

  const visited = new Set<string>();
  const assignDepth = (node: SessionForestNode, depth: number) => {
    node.depth = depth;
    visited.add(node.id);
    for (const child of node.children) {
      if (visited.has(child.id)) continue;
      assignDepth(child, depth + 1);
    }
  };
  for (const root of roots) assignDepth(root, 1);
  for (const session of sessions) {
    const node = nodes.get(session.id);
    if (!node || visited.has(node.id)) continue;
    node.orphaned = true;
    roots.push(node);
    assignDepth(node, 1);
  }
  return roots;
}

/** Flatten a forest depth-first for rendering, keeping the indentation level. */
export function flattenSessionForest(
  forest: SessionForestNode[],
): SessionForestNode[] {
  const out: SessionForestNode[] = [];
  const walk = (node: SessionForestNode) => {
    out.push(node);
    for (const child of node.children) walk(child);
  };
  for (const root of forest) walk(root);
  return out;
}

/**
 * The one tree the open session belongs to: its ancestor chain plus every
 * branch hanging off it. Rendering the whole forest would turn `/tree` into a
 * second sidebar, and the unrelated sessions are already reachable there.
 */
export function lineageForSession(
  forest: SessionForestNode[],
  sessionId: string | null | undefined,
): SessionForestNode[] {
  if (!sessionId) return [];
  const contains = (node: SessionForestNode): boolean =>
    node.id === sessionId || node.children.some(contains);
  const root = forest.find(contains);
  return root ? [root] : [];
}

/** Collapse whitespace and truncate, mirroring `rewindLabel` in the REPL. */
export function shortLabel(text: string, max = 60): string {
  const collapsed = text.replace(/\s+/g, " ").trim();
  if (!collapsed) return "…";
  return collapsed.length > max
    ? `${collapsed.slice(0, max - 1)}…`
    : collapsed;
}

export function buildSessionTreeNodes(
  messages: UiMessage[],
): SessionTreeNode[] {
  return messages.map((message, position) => ({
    index: position + 1,
    id: message.id,
    role: message.role,
    label: shortLabel(
      message.content || message.toolName || message.toolCallId || "",
    ),
    ...(message.toolName ? { toolName: message.toolName } : {}),
    ...(message.revisionRootId
      ? { revisionRootId: message.revisionRootId }
      : {}),
    ...(message.revisionCount ? { revisionCount: message.revisionCount } : {}),
    ...(message.activeRevision
      ? { activeRevision: message.activeRevision }
      : {}),
    ...(message.parentToolCallId ? { subagent: true } : {}),
  }));
}

/**
 * Group the linear transcript into user turns and keep those carrying review
 * snapshots. A leading stretch without a user message is still scanned so a
 * tool-only prefix is not silently dropped.
 */
export function buildRewindPoints(messages: UiMessage[]): RewindPoint[] {
  const points: RewindPoint[] = [];
  let turnStart = messages.length > 0 ? 0 : -1;

  const flush = (end: number) => {
    if (turnStart < 0 || end <= turnStart) return;
    const turn = messages.slice(turnStart, end);
    const changes: RewindChange[] = [];
    let firstChangeAt = "";
    for (const message of turn) {
      const change = reviewChangeFromMessage(message);
      if (!change) continue;
      if (!firstChangeAt) firstChangeAt = message.createdAt;
      changes.push({
        messageId: message.id,
        snapshotId: change.snapshotId,
        path: change.path,
        state: change.state,
      });
    }
    if (changes.length === 0) return;
    const prompt = turn.find((message) => message.role === "user")?.content;
    points.push({
      index: points.length + 1,
      label: shortLabel(prompt ?? messages[turnStart]?.content ?? ""),
      createdAt: firstChangeAt || messages[turnStart]?.createdAt || "",
      startIndex: turnStart,
      beforeMessageId:
        turnStart > 0 ? (messages[turnStart - 1]?.id ?? null) : null,
      changes,
    });
  };

  for (let position = 0; position < messages.length; position += 1) {
    if (messages[position].role === "user" && position > 0) {
      flush(position);
      turnStart = position;
    }
  }
  flush(messages.length);
  return points;
}

/**
 * Changes that `/rewind <n>` must restore, newest first. Reverse order matters:
 * `review.rollback` refuses to overwrite a file whose post-tool hash no longer
 * matches, so later edits have to be undone before earlier ones.
 */
export function rewindChangesFrom(
  points: RewindPoint[],
  index: number,
): RewindChange[] {
  return points
    .slice(Math.max(0, index - 1))
    .flatMap((point) => point.changes)
    .filter((change) => change.state === "active")
    .reverse();
}
