import { readFile } from "node:fs/promises";
import test from "node:test";
import assert from "node:assert/strict";

const read = (path) => readFile(new URL(path, import.meta.url), "utf8");

const page = await read("../src/components/settings/AgentSkillsPage.tsx");

// Regression: the project group's empty state shipped without an action, so a
// project-scoped skill could only be created by finding the toolbar button and
// switching the filter first.
test("both level groups offer a create action in their empty state", () => {
  assert.match(
    page,
    /visible\.global\.length === 0 \?[\s\S]*?action=\{newSkillButton\("global"\)\}/,
  );
  assert.match(
    page,
    /visible\.project\.length === 0 \?[\s\S]*?action=\{newSkillButton\("project"\)\}/,
  );
});

// The old global empty state reused the filter-driven openCreate, so with
// filter=project it created into the project level from the global group.
test("group actions pin their level instead of reusing the filter target", () => {
  assert.match(page, /const newSkillButton = \(level: AgentCapabilityLevel\)/);
  assert.match(page, /onClick=\{\(\) => openCreateAt\(level\)\}/);
  assert.doesNotMatch(page, /<CapabilityButton variant="primary" onClick=\{openCreate\}>/);
});

test("project-scoped creation still guards on a selected project", () => {
  assert.match(page, /const openCreateAt = \(level: AgentCapabilityLevel\) =>/);
  assert.match(
    page,
    /if \(level === "project" && !selectedProjectPath\) \{[\s\S]*?settings\.selectProjectFirst/,
  );
});
