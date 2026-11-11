import { readFile } from "node:fs/promises";
import test from "node:test";
import assert from "node:assert/strict";

const read = (path) => readFile(new URL(path, import.meta.url), "utf8");

const page = await read("../src/components/settings/AgentMcpPage.tsx");

// Regression: the project group's empty state shipped without an action, so the
// only way to add a project-scoped server was to notice the toolbar button and
// switch the filter first. Each group must offer its own entry now.
test("both level groups offer an add action in their empty state", () => {
  assert.match(
    page,
    /visible\.global\.length === 0 \?[\s\S]*?action=\{addButtonFor\("global"\)\}/,
  );
  assert.match(
    page,
    /visible\.project\.length === 0 \?[\s\S]*?action=\{addButtonFor\("project"\)\}/,
  );
});

// A group's empty state must not follow the filter: with filter=all the
// toolbar target is global, and reusing that button in the project group
// created the server in the wrong level.
test("group actions pin their level instead of reusing the filter target", () => {
  assert.match(page, /const addButtonFor = \(level: AgentCapabilityLevel\)/);
  assert.match(page, /onClick=\{\(\) => openCreateAt\(level\)\}/);
  assert.match(page, /const addButton = addButtonFor\(targetLevel\)/);
});

test("project-scoped creation still guards on a selected project", () => {
  assert.match(page, /const openCreateAt = \(level: AgentCapabilityLevel\) =>/);
  assert.match(
    page,
    /if \(level === "project" && !selectedProjectPath\) \{[\s\S]*?settings\.selectProjectFirst/,
  );
});
