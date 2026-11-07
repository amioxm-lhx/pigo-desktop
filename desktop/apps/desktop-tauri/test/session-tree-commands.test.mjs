import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = (path) => readFile(new URL(path, import.meta.url), "utf8");

const [
  composerRs,
  commands,
  builders,
  sessionSlice,
  appStore,
  composerTsx,
  panel,
  hostSessions,
  hostSchema,
  ...locales
] = await Promise.all([
  read("../src-tauri/src/composer.rs"),
  read("../src/lib/commands.ts"),
  read("../src/lib/session-tree.ts"),
  read("../src/stores/slices/session-slice.ts"),
  read("../src/stores/app-store.ts"),
  read("../src/components/Composer.tsx"),
  read("../src/components/SessionTreePanel.tsx"),
  read("../../../crates/host-core/src/sessions.rs"),
  read("../../../crates/host-core/src/db/schema.rs"),
  ...[
    "en",
    "zh-CN",
    "zh-TW",
    "de",
    "es",
    "tr",
    "fr",
    "ko",
  ].map((locale) =>
    read(`../../../packages/i18n/src/locales/${locale}/index.ts`),
  ),
]);

const treeKeys = [
  "treeTitle",
  "rewindTitle",
  "treeHint",
  "rewindHint",
  "treeEmpty",
  "rewindEmpty",
  "treeNeedsSession",
  "treeInvalid",
  "rewindInvalid",
  "treeBranchButton",
  "rewindButton",
  "treeVariantLabel",
  "treeVariantSwitched",
  "treeBranchDone",
  "treeBranchFailed",
  "treeLineageTitle",
  "treeLineageEmpty",
  "treeLineageCurrent",
  "treeLineageOrphaned",
  "treeNodesTitle",
  "rewindDone",
  "rewindFirstTurn",
];

test("Rust registry exposes /tree and /rewind as optional-argument commands", () => {
  const aliasBlock = composerRs.slice(
    composerRs.indexOf("const BUILTIN_ALIASES"),
    composerRs.indexOf("pub async fn commands"),
  );
  assert.match(
    aliasBlock,
    /name: "tree",\s*id: "builtin\.session\.tree",[\s\S]*?arg_mode: "optional"/,
  );
  assert.match(
    aliasBlock,
    /name: "rewind",\s*id: "builtin\.session\.rewind",[\s\S]*?arg_mode: "optional"/,
  );
});

test("transcript builders only describe data that really exists", () => {
  // Nodes are a numbered projection of the linear transcript: the desktop has
  // no message-level parent pointer, so the per-session list stays flat.
  assert.match(builders, /export function buildSessionTreeNodes/);
  assert.match(builders, /messages\.map\(\(message, position\)/);
  // Restore points come from message-owned review snapshots only.
  assert.match(builders, /reviewChangeFromMessage\(message\)/);
  assert.match(builders, /if \(changes\.length === 0\) return/);
  assert.match(builders, /messages\[position\]\.role === "user"/);
  assert.match(builders, /turnStart > 0/);
  // Rollback must run newest-first and skip already-restored changes.
  assert.match(builders, /\.reverse\(\)/);
  assert.match(builders, /change\.state === "active"/);
  assert.match(builders, /points\s*\.slice\(Math\.max\(0, index - 1\)\)/);
});

test("fork lineage is built from session metadata, not invented", () => {
  assert.match(builders, /export function buildSessionForest/);
  // A branch is a real session; the parent comes from the persisted column.
  assert.match(builders, /session\.parentSessionId/);
  assert.match(builders, /session\.forkedFromMessageId/);
  // A branch whose source is gone must surface, not disappear.
  assert.match(builders, /node\.orphaned = true/);
  // A cycle can never hide a session: leftovers are promoted to roots.
  assert.match(builders, /visited\.has\(node\.id\)/);
  assert.match(builders, /export function flattenSessionForest/);
});

test("/rewind restores files before forking the conversation", () => {
  assert.match(commands, /export async function openSessionTreePanel/);
  assert.match(commands, /export async function branchFromTreeNode/);
  assert.match(commands, /export async function rewindToRestorePoint/);
  assert.match(commands, /api\.getSession\(sessionId\)/);
  assert.match(commands, /api\.workspaceReviewRollback\(/);
  assert.match(commands, /rewindChangesFrom\(points, index\)/);
  assert.match(commands, /branchFromMessage\(before\)/);
  assert.match(commands, /withReviewChangeState\(message, "rolledBack"\)/);
  const rollbackAt = commands.indexOf("api.workspaceReviewRollback(");
  const forkAt = commands.indexOf("branchFromMessage(before)");
  assert.ok(
    rollbackAt > 0 && forkAt > rollbackAt,
    "file rollback must finish while the source session is still active",
  );
  // A failed/conflicting rollback is counted, never reported as restored.
  assert.match(commands, /skipped \+= 1/);
});

test("branching reuses the host fork and the panel is wired into the dock", () => {
  assert.match(sessionSlice, /branchFromMessage: async \(messageId\)/);
  assert.match(sessionSlice, /api\.forkSession\(/);
  assert.match(appStore, /createTreeSlice\(\{ get, set \}\)/);
  assert.match(composerTsx, /<SessionTreePanel \/>/);
  // A snapshot never leaks into another conversation.
  assert.match(panel, /treePanel\.sessionId !== activeSessionId/);
  assert.match(panel, /chat\.rewindFirstTurn|chat\.treeVariantLabel/);
});

test("host-core persists where a branch was cut from", () => {
  // The prompt/fork response is not the source of truth: the tree view reads
  // the session list, so the columns must be on the table, in the projection,
  // and written by the fork itself.
  assert.match(
    hostSchema,
    /parent_session_id\s+TEXT REFERENCES sessions\(id\) ON DELETE SET NULL/,
  );
  assert.match(hostSchema, /forked_from_message_id\s+TEXT/);
  assert.match(hostSessions, /s\.parent_session_id, s\.forked_from_message_id/);
  assert.match(hostSessions, /parent_session_id, forked_from_message_id/);
  assert.match(hostSessions, /forked_from_message_id: row\.get\(12\)\?/);
  // A cut point is only recorded when the fork was message-scoped.
  assert.match(hostSessions, /forked_from_message_id: cut_at\.map\(str::to_string\)/);
});

test("the panel renders the lineage and can walk into a branch", () => {
  assert.match(commands, /buildSessionForest\(listed\?\.sessions \?\? \[\], sessionId\)/);
  // Only the open session's own tree: unrelated sessions belong to the sidebar.
  assert.match(commands, /lineageForSession\(forest, sessionId\)/);
  // A lineage read failure must not take the node list and restore points down.
  assert.match(commands, /api\.listSessions\(\)\.catch\(\(\) => null\)/);
  assert.match(panel, /flattenSessionForest\(forest\)/);
  assert.match(panel, /switchSession\(session\.id\)/);
  assert.match(panel, /chat\.treeLineageTitle/);
  assert.match(panel, /session\.depth - 1\) \* 14/);
});

test("all eight locales carry identical /tree and /rewind keys and interpolations", () => {
  const interpolation = (source, key) => {
    const line = source.split("\n").find((row) => row.includes(`    ${key}:`));
    assert.ok(line, `${key} missing`);
    return [...line.matchAll(/\{\{(\w+)\}\}/g)].map((match) => match[1]).sort();
  };
  for (const key of treeKeys) {
    const expected = interpolation(locales[0], key);
    for (const [index, source] of locales.entries()) {
      assert.ok(source.includes(`    ${key}:`), `locale ${index} lacks ${key}`);
      assert.deepEqual(
        interpolation(source, key),
        expected,
        `locale ${index} has different {{...}} placeholders for ${key}`,
      );
    }
  }
});
