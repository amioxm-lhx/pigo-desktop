import { readFile } from "node:fs/promises";
import test from "node:test";
import assert from "node:assert/strict";

// Source-contract tests for the desktop MCP runner (src-tauri/src/mcp.rs).
// The gaps implemented from docs/mcp-local-integration.md §4 are pinned here
// the same way the UI contracts are: the Rust tree has no runnable unit harness
// in every dev environment, so the load-bearing invariants are asserted against
// the source itself and re-verified by cargo's own #[cfg(test)] module when a
// toolchain is available.
const read = (path) => readFile(new URL(path, import.meta.url), "utf8");

const mcp = await read("../src-tauri/src/mcp.rs");

test("gap 1: stdout is owned by one dedicated reader task", () => {
  assert.match(mcp, /fn spawn_stdio_reader\(/);
  assert.match(mcp, /tokio::spawn\(async move \{/);
  // The connection impl holds no inline read loop anymore: replies arrive
  // through the shared pending map, not through per-request stdout reads.
  const start = mcp.indexOf("impl StdioConnection {");
  const end = mcp.indexOf("impl HttpConnection {");
  assert.ok(start > 0 && end > start, "both connection impls are present");
  const stdioImpl = mcp.slice(start, end);
  assert.doesNotMatch(stdioImpl, /read_line|BufReader/);
});

test("gap 1: replies resolve a pending map keyed by request id", () => {
  assert.match(mcp, /type StdioPending = std::sync::Mutex<HashMap<u64, oneshot::Sender<Value>>>/);
  // Register before writing: a fast reply must never race the map insert.
  assert.match(mcp, /\.insert\(id, sender\)/);
  assert.match(mcp, /pending\.lock\(\)\.unwrap\(\)\.remove\(&id\)/);
});

test("gap 1: server-to-client requests are answered method-not-supported", () => {
  assert.match(mcp, /MessageKind::ServerRequest/);
  assert.match(mcp, /"code": -32601/);
});

test("gap 1: notifications/cancelled fails in-flight calls fast", () => {
  assert.match(mcp, /notifications\/cancelled/);
  assert.match(mcp, /cancelled\.lock\(\)\.unwrap\(\)\.insert\(id\)/);
  assert.match(mcp, /cancelled\.lock\(\)\.unwrap\(\)\.remove\(&id\)/);
});

test("gap 1: EOF fails every in-flight request instead of the caller's timeout", () => {
  assert.match(mcp, /pending\.lock\(\)\.unwrap\(\)\.clear\(\)/);
});

test("gap 2: tools/list follows cursor pagination with a page cap", () => {
  assert.match(mcp, /const MAX_LIST_PAGES: usize = 8/);
  assert.match(mcp, /fn next_cursor\(/);
  assert.match(mcp, /json!\(\{ "cursor": cursor \}\)/);
  assert.match(mcp, /for _page in 0\.\.MAX_LIST_PAGES/);
  // Both the runtime catalog and mcp/test walk pages through the shared helper.
  const callSites = mcp.match(/list_tools_paginated\(&connection, server_id\)/g) ?? [];
  assert.ok(callSites.length >= 2, "list_tools and mcp/test must share the paginated listing");
  // A page cap or tool cap that trips still warns instead of failing silently.
  assert.match(mcp, /kept sending cursors after/);
  assert.match(mcp, /tools\.truncate\(MAX_TOOLS_PER_SERVER\)/);
});

test("gap 3: Windows .cmd/.bat launchers ride cmd /c", () => {
  assert.match(mcp, /fn resolve_windows_command\(/);
  assert.match(mcp, /vec!\["\/c"/);
  // Non-Windows targets pass the command through unchanged.
  assert.match(mcp, /if !cfg!\(windows\)/);
});

test("gap 3: bare command names resolve via PATH x PATHEXT", () => {
  assert.match(mcp, /fn locate_on_path_in\(/);
  assert.match(mcp, /std::env::var\("PATHEXT"\)/);
  assert.match(mcp, /"\.COM;\.EXE;\.BAT;\.CMD"/);
});
