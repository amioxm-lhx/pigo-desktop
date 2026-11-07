import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = (path) => readFile(new URL(path, import.meta.url), "utf8");

const [composerRs, dispatch, submitHook, composerType, specEn, specZh] =
  await Promise.all([
    read("../src-tauri/src/composer.rs"),
    read("../src/lib/commands.ts"),
    read("../src/features/chat/composer/hooks/useComposerSubmit.ts"),
    read("../../../packages/shared/src/types/composer.ts"),
    read("../../../docs/spec/04-ux/04-builtin-commands.md"),
    read("../../../docs/zh-CN/spec/04-ux/04-builtin-commands.md"),
  ]);

const aliasBlock = composerRs.slice(
  composerRs.indexOf("const BUILTIN_ALIASES"),
  composerRs.indexOf("pub async fn commands"),
);
const ids = [...aliasBlock.matchAll(/\bid:\s*"([^"]+)"/g)].map((m) => m[1]);
const aliases = [...aliasBlock.matchAll(/\bname:\s*"([^"]+)"/g)].map((m) => m[1]);
const argModes = [...aliasBlock.matchAll(/\barg_mode:\s*"([^"]+)"/g)].map(
  (m) => m[1],
);

const actionBlock = dispatch.slice(
  dispatch.indexOf("const BUILTIN_ARG_ACTIONS"),
  dispatch.indexOf("async function runModeWithPrompt"),
);
const argActionIds = [
  ...actionBlock.matchAll(/^\s*"(builtin\.[^"]+)":/gm),
].map((m) => m[1]);
const caseIds = [...dispatch.matchAll(/case "([^"]+)"/g)].map((m) => m[1]);

const expectedIds = [
  "builtin.session.new",
  "builtin.agent.btw",
  "builtin.agent.compact",
  "builtin.session.copy",
  "builtin.session.info",
  "builtin.app.status",
  "builtin.mode.agent",
  "builtin.mode.plan",
  "builtin.mode.goal",
  "builtin.session.export",
  "builtin.agent.rebuild",
  "builtin.agent.goal",
  "builtin.session.tree",
  "builtin.session.rewind",
  "builtin.memory.report",
  "builtin.agent.dream",
];
const expectedAliases = [
  "new",
  "btw",
  "compact",
  "copy",
  "session",
  "status",
  "agent-mode",
  "plan-mode",
  "goal-mode",
  "export",
  "rebuild",
  "goal",
  "tree",
  "rewind",
  "memory",
  "dream",
];

const removedIds = [
  "builtin.session.delete",
  "builtin.agent.abort",
  "builtin.project.open",
  "builtin.project.clear",
  "builtin.settings.open",
  "builtin.settings.providers",
  "builtin.settings.import",
  "builtin.plugins.open",
  "builtin.plugins.loadDev",
  "builtin.logs.open",
  "builtin.session.rename",
  "builtin.commandPalette.show",
  "builtin.app.reloadWindow",
  "builtin.app.toggleDevtools",
];

test("Rust builtin registry is the single source of command metadata", () => {
  assert.deepEqual(ids, expectedIds);
  assert.deepEqual(aliases, expectedAliases);
  assert.equal(argModes.length, ids.length);
  assert.equal(new Set(ids).size, ids.length);
  assert.equal(new Set(aliases).size, aliases.length);
  for (const mode of argModes) {
    assert.ok(
      ["none", "optional", "required"].includes(mode),
      `unknown argMode ${mode}`,
    );
  }
  assert.match(composerRs, /"argMode": alias\.arg_mode/);
  assert.match(composerType, /argMode\?: "none" \| "optional" \| "required"/);
});

test("every builtin has an execution path and argument policies are honoured", () => {
  for (const id of ids) {
    assert.ok(
      caseIds.includes(id) || argActionIds.includes(id),
      `${id} has no palette case and no argument action`,
    );
  }
  ids.forEach((id, index) => {
    const mode = argModes[index];
    if (mode === "required" || mode === "optional") {
      assert.ok(
        argActionIds.includes(id),
        `${id} declares argMode ${mode} but has no argument action`,
      );
    }
  });
  for (const id of removedIds) {
    assert.doesNotMatch(aliasBlock, new RegExp(`"${id.replaceAll(".", "\\.")}"`));
    assert.doesNotMatch(dispatch, new RegExp(`case "${id.replaceAll(".", "\\.")}"`));
  }
});

test("composer submit dispatches through metadata, not per-command id branches", () => {
  assert.match(submitHook, /dispatchComposerCommand\(command, \{/);
  assert.doesNotMatch(submitHook, /command\.id === "builtin\./);
  assert.doesNotMatch(submitHook, /isModeCommand/);
  // Templates, skills and unknown aliases still fall through to the prompt path.
  assert.match(submitHook, /if \(handled\) return;/);
  assert.match(dispatch, /return false;/);
});

test("spec documents the current builtin catalog", () => {
  for (const id of ids) {
    assert.match(specEn, new RegExp("`" + id.replaceAll(".", "\\.") + "`"));
    assert.match(specZh, new RegExp("`" + id.replaceAll(".", "\\.") + "`"));
  }
  for (const alias of aliases) {
    assert.match(specEn, new RegExp("`/" + alias + "`"));
    assert.match(specZh, new RegExp("`/" + alias + "`"));
  }
  for (const id of removedIds) {
    assert.doesNotMatch(specEn, new RegExp("`" + id.replaceAll(".", "\\.") + "`"));
    assert.doesNotMatch(specZh, new RegExp("`" + id.replaceAll(".", "\\.") + "`"));
  }
});
