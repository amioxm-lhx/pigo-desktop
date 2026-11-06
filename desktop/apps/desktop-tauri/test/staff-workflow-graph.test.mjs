/**
 * The workflow editor is pure data in, pure data out, so the parts worth
 * testing without a browser are the layout, the round trip, and the YAML the
 * parameters view shows. Host-side validation is covered in host-core.
 */
import assert from "node:assert/strict";
import test from "node:test";
import {
  MAX_ZOOM,
  MIN_ZOOM,
  NODE_HEIGHT,
  NODE_WIDTH,
  ORIGIN_X,
  ORIGIN_Y,
  autoLayout,
  canConnect,
  createNode,
  depthsOf,
  fitZoom,
  fromGraph,
  graphExtent,
  toGraph,
  toYaml,
} from "../src/features/staff/workflow-graph.ts";

const chain = () => [
  { id: "trigger1", kind: "trigger", config: { mode: "schedule" }, position: { x: 0, y: 0 } },
  { id: "probe1", kind: "probe", config: { channel: "juejin" }, position: { x: 0, y: 0 } },
  { id: "notify1", kind: "notify", config: { channel: "email" }, position: { x: 0, y: 0 } },
];

/** A chain of `count` steps, still on the pitch the blueprints shipped with. */
const ladder = (count) =>
  Array.from({ length: count }, (_, index) => ({
    id: "n" + index,
    kind: "agent",
    config: {},
    position: { x: index * 220, y: 0 },
  }));

const chainEdges = (nodes) =>
  nodes.slice(1).map((node, index) => ({ from: nodes[index].id, to: node.id }));

function overlaps(a, b) {
  return (
    Math.abs(a.position.x - b.position.x) < NODE_WIDTH &&
    Math.abs(a.position.y - b.position.y) < NODE_HEIGHT
  );
}

test("depth follows the longest path, not the arrival order", () => {
  const nodes = chain();
  const edges = [
    { from: "trigger1", to: "probe1" },
    { from: "trigger1", to: "notify1" },
    { from: "probe1", to: "notify1" },
  ];
  const depth = depthsOf(nodes, edges);
  assert.equal(depth.get("trigger1"), 0);
  assert.equal(depth.get("probe1"), 1);
  assert.equal(depth.get("notify1"), 2);
});

test("auto layout puts one depth per column and keeps widths stable", () => {
  const laid = autoLayout(chain(), [{ from: "trigger1", to: "probe1" }, { from: "probe1", to: "notify1" }]);
  assert.ok(laid[1].position.x - laid[0].position.x > NODE_WIDTH);
  assert.ok(laid[2].position.x > laid[1].position.x);
  assert.equal(laid[0].position.y, laid[1].position.y);
  assert.ok(NODE_HEIGHT > 0);
});

test("a graph without positions is laid out instead of stacked on the origin", () => {
  const graph = fromGraph({
    nodes: [
      { id: "trigger1", kind: "trigger", config: { mode: "manual" } },
      { id: "agent1", kind: "agent", config: {} },
    ],
    edges: [{ from: "trigger1", to: "agent1" }],
  });
  const [first, second] = graph.nodes;
  assert.notDeepEqual(first.position, second.position);
  assert.ok(second.position.x > first.position.x);
});

test("a stored position survives the round trip", () => {
  const original = {
    nodes: [{ id: "trigger1", kind: "trigger", title: "Start", config: { mode: "event" }, position: { x: 12, y: 34 } }],
    edges: [],
  };
  const back = toGraph(fromGraph(original));
  assert.deepEqual(back.nodes[0].position, { x: 12, y: 34 });
  assert.equal(back.nodes[0].title, "Start");
  assert.deepEqual(back.nodes[0].config, { mode: "event" });
});

test("yaml carries the fields the host parses", () => {
  const yaml = toYaml(fromGraph({
    nodes: [
      { id: "notify1", kind: "notify", instruction: "say: hi", config: { channel: "email" }, position: { x: 1, y: 2 } },
    ],
    edges: [],
  }));
  assert.match(yaml, /^nodes:/);
  assert.match(yaml, /kind: notify/);
  assert.match(yaml, /channel: "email"/);
  assert.match(yaml, /instruction: "say: hi"/);
  assert.match(yaml, /edges: \[\]/);
});

test("linking refuses self, duplicates, and unknown nodes", () => {
  const graph = fromGraph({
    nodes: [
      { id: "trigger1", kind: "trigger", config: { mode: "manual" }, position: { x: 0, y: 0 } },
      { id: "probe1", kind: "probe", config: { channel: "v2ex" }, position: { x: 300, y: 0 } },
    ],
    edges: [{ from: "trigger1", to: "probe1" }],
  });
  assert.equal(canConnect(graph, "trigger1", "trigger1"), false);
  assert.equal(canConnect(graph, "trigger1", "probe1"), false);
  assert.equal(canConnect(graph, "trigger1", "missing"), false);
  assert.equal(canConnect(graph, "probe1", "trigger1"), true);
});

test("generated ids stay unique as nodes are added", () => {
  const first = createNode("probe", []);
  const second = createNode("probe", [first]);
  assert.notEqual(first.id, second.id);
  assert.equal(first.kind, "probe");
});

test("auto layout keeps a band of empty canvas before the first node", () => {
  const laid = autoLayout(chain(), [{ from: "trigger1", to: "probe1" }, { from: "probe1", to: "notify1" }]);
  for (const node of laid) {
    assert.ok(node.position.x >= ORIGIN_X, node.id + " x=" + node.position.x);
    assert.ok(node.position.y >= ORIGIN_Y, node.id + " y=" + node.position.y);
  }
  assert.ok(ORIGIN_X > 0 && ORIGIN_Y > 0);
});

test("created nodes also start inside the band, never on the origin", () => {
  const node = createNode("probe", []);
  assert.ok(node.position.x >= ORIGIN_X);
  assert.ok(node.position.y >= ORIGIN_Y);
});

test("a flow longer than a band wraps onto the next band instead of one row", () => {
  const nodes = ladder(6);
  const laid = autoLayout(nodes, chainEdges(nodes));
  assert.equal(laid[2].position.y, laid[0].position.y, "the first band holds three");
  assert.ok(laid[3].position.y > laid[2].position.y, "the fourth step starts a band");
  assert.equal(laid[3].position.x, laid[0].position.x, "and wraps back to the first column");
  assert.ok(laid[3].position.y - laid[2].position.y >= NODE_HEIGHT, "the bands do not touch");
  for (let i = 0; i < laid.length; i += 1) {
    for (let j = i + 1; j < laid.length; j += 1) {
      assert.ok(!overlaps(laid[i], laid[j]), laid[i].id + " overlaps " + laid[j].id);
    }
  }
});

test("a step where several things happen at once stays on one band", () => {
  const nodes = ["a", "b1", "b2", "b3", "c"].map((id) => ({
    id,
    kind: "agent",
    config: {},
    position: { x: 0, y: 0 },
  }));
  const laid = autoLayout(nodes, [
    { from: "a", to: "b1" },
    { from: "a", to: "b2" },
    { from: "a", to: "b3" },
    { from: "b1", to: "c" },
  ]);
  const branch = laid.filter((node) => node.id.startsWith("b"));
  assert.equal(new Set(branch.map((node) => node.position.x)).size, 1, "one column");
  assert.equal(new Set(branch.map((node) => node.position.y)).size, 3, "three rows");
  for (let i = 0; i < branch.length; i += 1) {
    for (let j = i + 1; j < branch.length; j += 1) {
      assert.ok(!overlaps(branch[i], branch[j]), "branches stay apart");
    }
  }
});

test("the wrapped flow fits a canvas the shipped row did not", () => {
  const nodes = ladder(6);
  const edges = chainEdges(nodes);
  const viewport = { width: 871, height: 573 };
  const row = fitZoom(viewport, graphExtent({ nodes, edges }), 28);
  const wrapped = fitZoom(viewport, graphExtent({ nodes: autoLayout(nodes, edges), edges }), 28);
  assert.ok(wrapped > row, "wrapping buys scale: " + wrapped + " vs " + row);
  assert.ok(wrapped > 1, "and the whole flow fits without shrinking: " + wrapped);
});

test("a new node is placed below the ones already on the canvas", () => {
  const nodes = ladder(6);
  const laid = autoLayout(nodes, chainEdges(nodes));
  const node = createNode("probe", laid);
  assert.equal(node.position.x, ORIGIN_X);
  for (const existing of laid) {
    assert.ok(node.position.y >= existing.position.y + NODE_HEIGHT, "clear of " + existing.id);
  }
});

test("a graph still on the shipped row is laid out again when it opens", () => {
  const nodes = ladder(6);
  const graph = fromGraph({ nodes, edges: chainEdges(nodes) });
  assert.equal(new Set(graph.nodes.map((node) => node.position.y)).size, 2, "two bands");
  assert.equal(graph.nodes[3].position.x, graph.nodes[0].position.x, "wrapped");
});

test("a graph the owner arranged opens exactly as it was left", () => {
  const placed = [
    { id: "a", kind: "agent", config: {}, position: { x: 12, y: 34 } },
    { id: "b", kind: "probe", config: {}, position: { x: 300, y: 90 } },
    { id: "c", kind: "notify", config: {}, position: { x: 640, y: 12 } },
  ];
  const graph = fromGraph({ nodes: placed, edges: [] });
  assert.deepEqual(
    graph.nodes.map((node) => node.position),
    placed.map((node) => node.position),
  );
});

test("the extent leaves slack past the last column and row", () => {
  const graph = fromGraph({
    nodes: [{ id: "agent1", kind: "agent", config: {}, position: { x: 300, y: 200 } }],
    edges: [],
  });
  const extent = graphExtent(graph);
  assert.ok(extent.width >= 300 + NODE_WIDTH, "width " + extent.width);
  assert.ok(extent.height >= 200 + NODE_HEIGHT, "height " + extent.height);
});

test("an empty graph still has a canvas to drop the first node onto", () => {
  const extent = graphExtent({ nodes: [], edges: [] });
  assert.ok(extent.width >= NODE_WIDTH && extent.height >= NODE_HEIGHT);
});

test("fit to window shows the whole graph and keeps the margin", () => {
  const viewport = { width: 871, height: 573 };
  const graph = { width: 836, height: 846 };
  const margin = 28;
  const zoom = fitZoom(viewport, graph, margin);

  assert.ok(zoom < 1, "a graph taller than the viewport scales down: " + zoom);
  assert.ok(graph.width * zoom <= viewport.width - margin * 2, "width still fits");
  assert.ok(graph.height * zoom <= viewport.height - margin * 2, "height still fits");
  // The margin is the point: the fit must not consume it.
  assert.ok(graph.height * zoom > viewport.height - margin * 4, "uses most of the room");
});

test("fit magnifies a small graph but stops at the ceiling", () => {
  const zoom = fitZoom({ width: 871, height: 573 }, { width: 640, height: 360 }, 28);
  assert.ok(zoom > 1, "a graph smaller than the viewport grows: " + zoom);
  assert.ok(zoom <= MAX_ZOOM, "but never past the ceiling");
});

test("fit shrinks an enormous graph but stops at the floor", () => {
  const zoom = fitZoom({ width: 871, height: 573 }, { width: 50000, height: 50000 }, 28);
  assert.equal(zoom, MIN_ZOOM);
});

test("fit survives a viewport smaller than its own margin", () => {
  const zoom = fitZoom({ width: 10, height: 10 }, { width: 800, height: 600 }, 28);
  assert.ok(zoom >= MIN_ZOOM && zoom <= MAX_ZOOM, "zoom " + zoom);
});

test("fit snaps to whole percents so the readout is round", () => {
  const zoom = fitZoom({ width: 871, height: 573 }, { width: 837, height: 846 }, 28);
  assert.equal(Math.round(zoom * 100), zoom * 100);
});
