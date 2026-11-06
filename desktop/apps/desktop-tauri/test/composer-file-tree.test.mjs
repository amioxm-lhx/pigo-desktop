import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = (path) => readFile(new URL(path, import.meta.url), "utf8");

const [hook, autocomplete, styles, fsops] = await Promise.all([
  read("../src/hooks/use-composer-autocomplete.ts"),
  read("../src/components/ComposerAutocomplete.tsx"),
  read("../src/styles/composer-autocomplete.css"),
  read("../src-tauri/src/fsops.rs"),
]);

test("@ file mode lists one directory level at a time", () => {
  // A typed "/" scopes the menu to that directory and keeps only its direct
  // children, so accepting a directory drills in instead of dumping the
  // whole subtree as one flat list.
  assert.match(hook, /function selectScopedRows/);
  assert.match(
    hook,
    /if \(path\.slice\(scope\.length\)\.includes\("\/"\)\) continue;/,
  );
  assert.match(hook, /normalized\.lastIndexOf\("\/"\)/);
  // A bare "@" starts at the workspace root instead of matching every path.
  assert.match(
    hook,
    /if \(normalized === ""\) return selectScopedRows\(entries, "", ""\);/,
  );
});

test("@ file mode renders hits inside their directory tree", () => {
  // Hits without a "/" query keep their ancestors as context rows so two
  // identically named leaves stay distinguishable by nesting.
  assert.match(hook, /function selectGlobalRows/);
  assert.match(hook, /matched: false/);
  assert.match(hook, /matched: true/);
  assert.match(hook, /function compareFileRows/);
  assert.match(hook, /left\.entry\.kind === "dir" \? -1 : 1/);
  // Keyboard navigation never starts on an ancestor kept only for hierarchy.
  assert.match(hook, /item\.kind === "command" \|\| item\.matched/);
});

test("file rows indent by depth and context rows read as context", () => {
  assert.match(autocomplete, /data-ac-depth=\{item\.depth\}/);
  assert.match(autocomplete, /"--ac-depth": item\.depth/);
  assert.match(
    styles,
    /\.composer-ac-item\[data-ac-depth\]\s*\{[\s\S]*?padding-inline-start:/,
  );
  assert.match(styles, /\[data-ac-context="true"\]/);
});

test("fs/index returns the shared { path, kind } entry shape", () => {
  // The renderer reads `entry.kind`; the shell used to send `dir: bool`, so
  // directories accepted as files and the menu could not offer drill-down.
  assert.match(
    fsops,
    /json!\(\{ "path": rel, "kind": if is_dir \{ "dir" \} else \{ "file" \} \}\)/,
  );
  assert.doesNotMatch(fsops, /json!\(\{ "path": rel, "dir": is_dir \}\)/);
});
