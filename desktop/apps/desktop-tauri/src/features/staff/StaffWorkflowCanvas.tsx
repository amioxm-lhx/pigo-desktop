/**
 * The node graph itself: SVG edges under absolutely positioned node boxes.
 *
 * No diagramming dependency: the graph is small (200 nodes / 600 edges is the
 * host's ceiling), the layout is already stored per node, and the only
 * interaction that matters is drag-to-move plus drag-from-port-to-link.
 *
 * Zoom scales one plane rather than each node, so edges, ports and hit targets
 * stay in step. The plane sits inside a constant band of empty canvas, which is
 * why zooming out never draws a node against the edge.
 */
import { useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import {
  NODE_HEIGHT,
  NODE_WIDTH,
  canConnect,
  fitZoom,
  graphExtent,
  type EditorGraph,
  type EditorNode,
} from "./workflow-graph";

/** Empty canvas kept on every side of the graph, in screen pixels. */
const CANVAS_PAD = 28;
/** The least a wrapped edge bows out of the gap between two bands. */
const BAND_DROP_MIN = 12;

export type StaffWorkflowCanvasProps = {
  graph: EditorGraph;
  selectedId: string | null;
  linkFrom: string | null;
  onChange: (graph: EditorGraph) => void;
  onSelect: (id: string | null) => void;
  onStartLink: (id: string | null) => void;
  onDeleteEdge: (from: string, to: string) => void;
  nodeSubtitle: (node: EditorNode) => string;
  kindLabel: (kind: string) => string;
  hint: string;
  linkingLabel: string;
  fitLabel: string;
  /** Changes when a different workflow (or version) is opened, and only then. */
  documentKey: string;
};

export function StaffWorkflowCanvas(props: StaffWorkflowCanvasProps) {
  const {
    graph,
    selectedId,
    linkFrom,
    onChange,
    onSelect,
    onStartLink,
    onDeleteEdge,
    nodeSubtitle,
    kindLabel,
    hint,
    linkingLabel,
    fitLabel,
    documentKey,
  } = props;
  const canvasRef = useRef<HTMLDivElement | null>(null);
  const planeRef = useRef<HTMLDivElement | null>(null);
  // The grab offset is held in graph units, so a node keeps the same point
  // under the pointer at any zoom instead of jumping to the cursor.
  const dragRef = useRef<{ id: string; offsetX: number; offsetY: number } | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const [zoom, setZoom] = useState(1);
  // A fit on one workflow should not decide the scale of the next one: each
  // document opens at actual size and the owner opts in again. Compared during
  // render rather than in an effect, so the old scale is never painted.
  const [zoomedDocument, setZoomedDocument] = useState(documentKey);
  if (documentKey !== zoomedDocument) {
    setZoomedDocument(documentKey);
    setZoom(1);
  }

  const { width, height } = graphExtent(graph);
  const byId = new Map(graph.nodes.map((node) => [node.id, node]));

  /** A client point in graph units, measured against the plane, not the page. */
  const toGraph = (clientX: number, clientY: number) => {
    const plane = planeRef.current;
    if (!plane) return null;
    const rect = plane.getBoundingClientRect();
    return { x: (clientX - rect.left) / zoom, y: (clientY - rect.top) / zoom };
  };

  const fitToWindow = () => {
    const host = canvasRef.current;
    if (!host) return;
    setZoom(fitZoom({ width: host.clientWidth, height: host.clientHeight }, { width, height }, CANVAS_PAD));
  };

  const move = (event: ReactPointerEvent<HTMLDivElement>, node: EditorNode) => {
    const drag = dragRef.current;
    if (!drag || drag.id !== node.id) return;
    const point = toGraph(event.clientX, event.clientY);
    if (!point) return;
    const x = Math.max(0, Math.round(point.x - drag.offsetX));
    const y = Math.max(0, Math.round(point.y - drag.offsetY));
    if (x === node.position.x && y === node.position.y) return;
    onChange({
      ...graph,
      nodes: graph.nodes.map((item) => (item.id === node.id ? { ...item, position: { x, y } } : item)),
    });
  };

  return (
    <div className="space-y-1.5">
      <div className="staff-canvas-wrap">
        <div className="staff-canvas" ref={canvasRef} onClick={() => onSelect(null)}>
          <div
            className="staff-canvas-inner"
            style={{ width: width * zoom + CANVAS_PAD * 2, height: height * zoom + CANVAS_PAD * 2 }}
          >
            <div
              className="staff-canvas-plane"
              ref={planeRef}
              style={{
                left: CANVAS_PAD,
                top: CANVAS_PAD,
                width,
                height,
                transform: "scale(" + zoom + ")",
              }}
            >
              <svg
                width={width}
                height={height}
                style={{ position: "absolute", inset: 0, pointerEvents: "none" }}
              >
                {graph.edges.map((edge) => {
                  const from = byId.get(edge.from);
                  const to = byId.get(edge.to);
                  if (!from || !to) return null;
                  const x1 = from.position.x + NODE_WIDTH;
                  const y1 = from.position.y + NODE_HEIGHT / 2;
                  const x2 = to.position.x;
                  const y2 = to.position.y + NODE_HEIGHT / 2;
                  // An edge that starts a new band drops out of the bottom of
                  // its source and into the top of its target. Routing it
                  // through the side ports instead would send it around the
                  // right of the band and back across every node in the row it
                  // is leaving.
                  const bend = Math.max(24, Math.abs(x2 - x1) / 2);
                  const wraps = x2 <= x1 && y2 > y1;
                  const x2t = to.position.x + NODE_WIDTH / 2;
                  const bottom = from.position.y + NODE_HEIGHT;
                  const top = to.position.y;
                  // Bowing by half the gap keeps the curve inside it however
                  // tightly the two bands are packed.
                  const drop = Math.max(BAND_DROP_MIN, (top - bottom) / 2);
                  const d = wraps
                    ? "M " + (from.position.x + NODE_WIDTH / 2) + " " + bottom + " C " +
                      (from.position.x + NODE_WIDTH / 2) + " " + (bottom + drop) + ", " +
                      x2t + " " + (top - drop) + ", " + x2t + " " + top
                    : "M " + x1 + " " + y1 + " C " + (x1 + bend) + " " + y1 + ", " +
                      (x2 - bend) + " " + y2 + ", " + x2 + " " + y2;
                  const active = linkFrom === edge.from || selectedId === edge.from;
                  return (
                    <g key={edge.from + "->" + edge.to}>
                      <path
                        d={d}
                        className="staff-edge"
                        style={{ pointerEvents: "stroke", strokeWidth: 12, strokeOpacity: 0 }}
                        onClick={(event) => {
                          event.stopPropagation();
                          onDeleteEdge(edge.from, edge.to);
                        }}
                      />
                      <path d={d} className={"staff-edge" + (active ? " on" : "")} />
                    </g>
                  );
                })}
              </svg>

              {graph.nodes.map((node) => {
                const selected = selectedId === node.id;
                const linking = linkFrom === node.id;
                return (
                  <div
                    key={node.id}
                    className={
                      "staff-node" + (selected ? " on" : "") + (linking ? " linking" : "")
                    }
                    style={{ left: node.position.x, top: node.position.y }}
                    onPointerDown={(event) => {
                      if (event.button !== 0) return;
                      const point = toGraph(event.clientX, event.clientY);
                      if (!point) return;
                      dragRef.current = {
                        id: node.id,
                        offsetX: point.x - node.position.x,
                        offsetY: point.y - node.position.y,
                      };
                      setDragging(node.id);
                      event.currentTarget.setPointerCapture(event.pointerId);
                      event.stopPropagation();
                    }}
                    onPointerMove={(event) => move(event, node)}
                    onPointerUp={(event) => {
                      if (dragRef.current?.id === node.id) {
                        dragRef.current = null;
                        setDragging(null);
                        event.currentTarget.releasePointerCapture(event.pointerId);
                      }
                    }}
                    onClick={(event) => {
                      event.stopPropagation();
                      if (linkFrom && linkFrom !== node.id) {
                        if (canConnect(graph, linkFrom, node.id)) {
                          onChange({
                            ...graph,
                            edges: [...graph.edges, { from: linkFrom, to: node.id }],
                          });
                        }
                        onStartLink(null);
                        return;
                      }
                      onSelect(node.id);
                    }}
                  >
                    <span className="staff-node-kind">{kindLabel(node.kind)}</span>
                    <span className="staff-node-title">{node.title || node.id}</span>
                    <span className="staff-node-sub">{nodeSubtitle(node)}</span>
                    <span
                      className="staff-port in"
                      aria-hidden
                      onPointerDown={(event) => event.stopPropagation()}
                    />
                    <span
                      className="staff-port out"
                      aria-hidden
                      onPointerDown={(event) => {
                        event.stopPropagation();
                        onStartLink(node.id);
                      }}
                    />
                  </div>
                );
              })}
            </div>
          </div>
        </div>
        <div className="staff-canvas-tools">
          <button type="button" className="staff-chip" onClick={fitToWindow}>
            {fitLabel}
          </button>
          {zoom === 1 ? null : (
            <button type="button" className="staff-chip" onClick={() => setZoom(1)}>
              {"100%"}
            </button>
          )}
        </div>
      </div>
      <div className="staff-hint">{dragging ? "" : linkFrom ? linkingLabel : hint}</div>
    </div>
  );
}
