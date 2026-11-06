/**
 * Graph model for the workflow editor.
 *
 * Pure data in, pure data out: nothing here talks to the host. Validation stays
 * the host's job (staff.workflow.validate) so the rules live in one place; the
 * only checks repeated here are the ones that would otherwise let the pointer
 * draw a broken graph in the first place.
 */
import {
  STAFF_NODE_KINDS,
  type StaffGraph,
  type StaffGraphEdge,
  type StaffGraphNode,
  type StaffNodeKind,
  type StaffNodePosition,
} from "@pi-desktop/shared";

export const NODE_WIDTH = 176;
export const NODE_HEIGHT = 62;
export const NODE_GAP_X = 64;
export const NODE_GAP_Y = 26;

/** Where an auto-laid graph starts, in graph units. This band is what keeps the
    first column and the first row off the canvas edge. */
export const ORIGIN_X = 40;
export const ORIGIN_Y = 32;
/** Slack kept past the last column and the last row, in graph units. */
const SLACK = 72;
const MIN_EXTENT_WIDTH = 640;
const MIN_EXTENT_HEIGHT = 360;
/**
 * Columns a band holds before the layout wraps onto the next one.
 *
 * A column is 240 graph units wide and a row 88 tall, so filling a canvas that
 * is a little wider than it is tall works out at roughly three quarters of a
 * column for every row of a square block. The floor of three is what the five
 * to seven step flows this console ships need: they land on two bands instead
 * of one row that only reads after a heavy zoom-out.
 */
const BAND_COLUMNS_MIN = 3;
const BAND_COLUMNS_GROWTH = 0.75;
/** The column pitch the seeded blueprints were written with, before the
    console laid them out itself. */
const LEGACY_COLUMN_PITCH = 220;
/** The smallest and largest scale `fitZoom` will ever return. */
export const MIN_ZOOM = 0.3;
export const MAX_ZOOM = 1.5;

export type EditorNode = StaffGraphNode & { position: StaffNodePosition };
export type EditorGraph = { nodes: EditorNode[]; edges: StaffGraphEdge[] };

/** Parameter fields a node kind needs before the host will accept it. */
export const REQUIRED_CONFIG: Record<StaffNodeKind, string[]> = {
  trigger: ["mode"],
  probe: ["channel"],
  analyze: ["focus"],
  produce: ["artifact"],
  agent: ["instruction"],
  memory: ["operation"],
  condition: ["expression"],
  handoff: ["employeeId"],
  humanGate: ["reason"],
  notify: ["channel"],
  // A board node declares what it does and, when it dispatches, the workers and
  // the kinds of work its manager is allowed to name.
  board: ["action"],
};

/** Kinds that may appear only once; the host rejects a second one. */
export const SINGLETON_KINDS: StaffNodeKind[] = ["trigger"];

export function configOf(node: StaffGraphNode): Record<string, unknown> {
  const value = node.config;
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

export function configText(node: StaffGraphNode, key: string): string {
  const value = configOf(node)[key];
  if (Array.isArray(value)) return value.filter((item) => typeof item === "string").join(" · ");
  return typeof value === "string" ? value : value == null ? "" : String(value);
}

/** A config entry that holds a list of strings, e.g. a probe's channels. */
export function configList(node: StaffGraphNode, key: string): string[] {
  const value = configOf(node)[key];
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
}

function placed(position: StaffNodePosition | undefined): position is StaffNodePosition {
  return Boolean(position) && Number.isFinite(position!.x) && Number.isFinite(position!.y);
}

function incomingOf(nodes: EditorNode[], edges: StaffGraphEdge[]): Map<string, string[]> {
  const incoming = new Map<string, string[]>();
  for (const node of nodes) incoming.set(node.id, []);
  for (const edge of edges) incoming.get(edge.to)?.push(edge.from);
  return incoming;
}

export function depthsOf(nodes: EditorNode[], edges: StaffGraphEdge[]): Map<string, number> {
  const incoming = incomingOf(nodes, edges);
  const depth = new Map<string, number>();
  const visit = (id: string, seen: Set<string>): number => {
    const known = depth.get(id);
    if (known !== undefined) return known;
    if (seen.has(id)) return 0;
    seen.add(id);
    const parents = incoming.get(id) ?? [];
    const value =
      parents.length === 0 ? 0 : Math.max(...parents.map((parent) => visit(parent, seen) + 1));
    depth.set(id, value);
    return value;
  };
  for (const node of nodes) visit(node.id, new Set());
  return depth;
}

/** Columns a band holds for a graph of this size. */
function bandColumns(count: number): number {
  return Math.max(BAND_COLUMNS_MIN, Math.round(BAND_COLUMNS_GROWTH * Math.sqrt(count)));
}

/**
 * Deterministic layered layout: depth sets the column, arrival order the row,
 * and a flow wider than a band wraps onto the next one.
 *
 * One row per depth reads well right up to the point where the flow is longer
 * than the canvas, at which point fitting it in costs every bit of the text. A
 * band is never split across two rows, so a point in the flow where several
 * things happen at once stays together on one line.
 */
export function autoLayout(nodes: EditorNode[], edges: StaffGraphEdge[]): EditorNode[] {
  const depth = depthsOf(nodes, edges);
  const columns = new Map<number, EditorNode[]>();
  for (const node of nodes) {
    const column = depth.get(node.id) ?? 0;
    const bucket = columns.get(column);
    if (bucket) bucket.push(node);
    else columns.set(column, [node]);
  }
  const columnCount = columns.size === 0 ? 0 : Math.max(...columns.keys()) + 1;
  const perBand = bandColumns(nodes.length);
  // The tallest column in a band sets that band's height, so bands never
  // overlap whatever shape the graph is.
  const bandTop: number[] = [];
  let top = ORIGIN_Y;
  for (let band = 0; band * perBand < columnCount; band += 1) {
    bandTop.push(top);
    let rows = 0;
    for (let column = band * perBand; column < (band + 1) * perBand; column += 1) {
      rows = Math.max(rows, columns.get(column)?.length ?? 0);
    }
    top += Math.max(1, rows) * (NODE_HEIGHT + NODE_GAP_Y);
  }
  const rowIn = new Map<number, number>();
  return nodes.map((node) => {
    const column = depth.get(node.id) ?? 0;
    const row = rowIn.get(column) ?? 0;
    rowIn.set(column, row + 1);
    return {
      ...node,
      position: {
        x: ORIGIN_X + (column % perBand) * (NODE_WIDTH + NODE_GAP_X),
        y: (bandTop[Math.floor(column / perBand)] ?? ORIGIN_Y) + row * (NODE_HEIGHT + NODE_GAP_Y),
      },
    };
  });
}

/** The graph's bounding box plus its slack, in graph units. */
export function graphExtent(graph: EditorGraph): { width: number; height: number } {
  let width = MIN_EXTENT_WIDTH;
  let height = MIN_EXTENT_HEIGHT;
  for (const node of graph.nodes) {
    width = Math.max(width, node.position.x + NODE_WIDTH + SLACK);
    height = Math.max(height, node.position.y + NODE_HEIGHT + SLACK);
  }
  return { width, height };
}

/**
 * The scale that shows the whole graph inside `viewport`, keeping `margin`
 * screen pixels of empty canvas on every side.
 *
 * Floored to whole percents rather than rounded: a fit that overshoots by a
 * percent is a scrollbar the owner did not ask for.
 */
export function fitZoom(
  viewport: { width: number; height: number },
  graph: { width: number; height: number },
  margin: number,
): number {
  if (graph.width <= 0 || graph.height <= 0) return 1;
  const usableWidth = Math.max(1, viewport.width - margin * 2);
  const usableHeight = Math.max(1, viewport.height - margin * 2);
  const raw = Math.min(usableWidth / graph.width, usableHeight / graph.height);
  const clamped = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, raw));
  return Math.floor(clamped * 100) / 100;
}

export function nextNodeId(kind: StaffNodeKind, nodes: EditorNode[]): string {
  const taken = new Set(nodes.map((node) => node.id));
  for (let index = 1; index < 10_000; index += 1) {
    const candidate = kind + index;
    if (!taken.has(candidate)) return candidate;
  }
  return kind + "-" + Date.now();
}

export function createNode(
  kind: StaffNodeKind,
  nodes: EditorNode[],
  position?: StaffNodePosition,
): EditorNode {
  // Below everything already on the canvas: a palette click must not drop the
  // new node on top of a band the layout has already filled.
  let lowest = ORIGIN_Y;
  for (const node of nodes) {
    lowest = Math.max(lowest, node.position.y + NODE_HEIGHT + NODE_GAP_Y);
  }
  return {
    id: nextNodeId(kind, nodes),
    kind,
    title: "",
    instruction: "",
    config: {},
    position: position ?? { x: ORIGIN_X, y: lowest },
  };
}

export function toGraph(graph: EditorGraph): StaffGraph {
  return {
    nodes: graph.nodes.map((node) => ({
      id: node.id,
      kind: node.kind,
      ...(node.title ? { title: node.title } : {}),
      ...(node.instruction ? { instruction: node.instruction } : {}),
      config: configOf(node),
      position: { x: Math.round(node.position.x), y: Math.round(node.position.y) },
    })),
    edges: graph.edges.map((edge) => ({ from: edge.from, to: edge.to })),
  };
}

/**
 * Whether a stored graph is still the single row the blueprints shipped.
 *
 * An exact ladder is a machine's work, not an owner's: dragging leaves pixel
 * precision behind, so a graph that still sits on the old pitch has never been
 * arranged by hand. Those are the graphs worth laying out again now that long
 * flows wrap onto bands instead of running off the right of the canvas.
 */
function isShippedRow(nodes: EditorNode[]): boolean {
  if (nodes.length < 2) return false;
  const { y } = nodes[0].position;
  return nodes.every(
    (node, index) =>
      node.position.y === y && node.position.x === index * LEGACY_COLUMN_PITCH,
  );
}

export function fromGraph(graph: StaffGraph | null | undefined): EditorGraph {
  const rawNodes = graph?.nodes ?? [];
  const edges = (graph?.edges ?? []).filter(
    (edge) => edge && typeof edge.from === "string" && typeof edge.to === "string",
  );
  const unplaced = new Set<string>();
  const nodes: EditorNode[] = rawNodes.map((node) => {
    if (!placed(node.position)) unplaced.add(node.id);
    return { ...node, config: configOf(node), position: node.position ?? { x: 0, y: 0 } };
  });
  const shippedRow = isShippedRow(nodes);
  if (unplaced.size === 0 && !shippedRow) return { nodes, edges };
  // A graph written before the editor existed has no layout, and one still on
  // the shipped row has a layout nobody chose. Both are laid out rather than
  // shown: without this a database seeded with the row keeps it forever.
  const laid = new Map(autoLayout(nodes, edges).map((node) => [node.id, node.position]));
  return {
    nodes: nodes.map((node) => {
      const position = laid.get(node.id);
      if (!position || !(shippedRow || unplaced.has(node.id))) return node;
      return { ...node, position };
    }),
    edges,
  };
}

export function canConnect(graph: EditorGraph, from: string, to: string): boolean {
  if (from === to) return false;
  if (!graph.nodes.some((node) => node.id === from)) return false;
  if (!graph.nodes.some((node) => node.id === to)) return false;
  return !graph.edges.some((edge) => edge.from === from && edge.to === to);
}

export function isKind(value: unknown): value is StaffNodeKind {
  return typeof value === "string" && (STAFF_NODE_KINDS as readonly string[]).includes(value);
}

/** Minimal YAML for this exact shape; a general emitter would be overkill. */
export function toYaml(graph: EditorGraph): string {
  const lines: string[] = ["nodes:"];
  for (const node of graph.nodes) {
    lines.push("  - id: " + node.id);
    lines.push("    kind: " + node.kind);
    if (node.title) lines.push("    title: " + quote(node.title));
    if (node.instruction) lines.push("    instruction: " + quote(node.instruction));
    const entries = Object.entries(configOf(node));
    if (entries.length === 0) {
      lines.push("    config: {}");
    } else {
      lines.push("    config:");
      for (const [key, value] of entries) lines.push("      " + key + ": " + scalar(value));
    }
    lines.push(
      "    position: { x: " +
        Math.round(node.position.x) +
        ", y: " +
        Math.round(node.position.y) +
        " }",
    );
  }
  if (graph.edges.length === 0) {
    lines.push("edges: []");
  } else {
    lines.push("edges:");
    for (const edge of graph.edges) {
      lines.push("  - from: " + edge.from);
      lines.push("    to: " + edge.to);
    }
  }
  return lines.join("\n");
}

function quote(value: string): string {
  return '"' + value.replace(/"/g, '\\"') + '"';
}

function scalar(value: unknown): string {
  if (typeof value === "string") return quote(value);
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return quote(JSON.stringify(value ?? null));
}
