/**
 * What a step's prompt says about the evidence the driver collected.
 *
 * The scout's first sweep used to end with the model asking a command-line
 * client for pages the driver had already read, and the site answered that
 * with the login wall it shows every client that is not its own front end.
 * These cases pin the wording that keeps the turn on the evidence it was given.
 */
import assert from "node:assert/strict";
import test from "node:test";
import { register } from "node:module";

register(new URL("./helpers/ts-import-hooks.mjs", import.meta.url));
const { composeStepPrompt, stepResultOf } = await import("../electron/main/runtime/staff-prompt.ts");

const step = {
  nodeId: "scan",
  kind: "probe",
  title: "Walk the sources",
  instruction: "Find what is worth answering today.",
  config: { sources: ["zhihu"] },
};

const compose = (notes, trace = []) =>
  composeStepPrompt({ employeeName: "侦察员", index: 0, stepCount: 4, step, notes, trace });

test("a collected page is handed over as the evidence to work from", () => {
  const prompt = compose(["- [collected:zhihu] 热榜 — https://www.zhihu.com/hot" + String.fromCharCode(10) + "  - 问题 A — https://www.zhihu.com/question/1"]);
  assert.match(prompt, /What the team already knows:/);
  assert.match(prompt, /read by this machine a moment ago/);
  assert.match(prompt, /Do not fetch those pages again with a command-line client/);
  // The refusal has to stay a refusal: the prompt never licenses a workaround.
  assert.doesNotMatch(prompt, /try a different user agent/i);
});

test("a skipped site is reported rather than papered over", () => {
  const prompt = compose(["- [collected:zhihu] skipped: no signed-in session for this site"]);
  assert.match(prompt, /has not signed that site in yet/);
  assert.match(prompt, /do not try to sign in yourself/);
});

test("a probe step is told it has no fetcher of its own", () => {
  const prompt = compose([]);
  assert.match(prompt, /the driver opens those sites in the site's own signed-in browser/i);
  assert.match(prompt, /Do not fetch the pages with a command-line client/);
});

test("a step handed nothing is told so, and told not to go looking for it", () => {
  const prompt = compose([]);
  // Run ac6038f1 step 2 was handed no memory item and no collected page, was
  // told only to assume and continue, and answered by walking the machine for
  // nine minutes. The absence is now part of the prompt.
  assert.match(prompt, /What the team already knows:/);
  assert.match(prompt, /nothing: no memory item and no collected page reached this step/);
  assert.doesNotMatch(prompt, /\[collected:/);
  assert.match(prompt, /do not read or list local files, do not run a command to gather material/);
  assert.match(prompt, /an empty answer is the right answer/);
  assert.match(prompt, /STEP RESULT:/);
});

test("what an earlier step established is handed to the next one", () => {
  const prompt = compose([], ["- [判商机] 3 条里留下 1 条：有人问权限怎么接"]);
  assert.match(prompt, /What this run has already done:/);
  assert.match(prompt, /3 条里留下 1 条/);
});

test("a run with nothing behind it claims nothing behind it", () => {
  const prompt = compose([]);
  assert.doesNotMatch(prompt, /What this run has already done:/);
});
test("a node that records a finding is asked for the finding, not for a note", () => {
  // The seeded scout filed "发布了共享信号一条 -- 本轮无可用商机..." and the analyst
  // filed a file name. Both were read back verbatim as what the team knows.
  const publish = composeStepPrompt({
    employeeName: "侦察员",
    index: 2,
    stepCount: 5,
    step: { nodeId: "file", kind: "memory", title: "存线索", instruction: "", config: {}, memory: { operation: "publish", kind: "signal" } },
    notes: [],
    trace: [],
  });
  assert.match(publish, /make that line the finding itself rather than a note about it/);
  assert.match(publish, /read back word for word/);
  assert.doesNotMatch(publish, /what actually happened/);

  // A step that only reports keeps the older ask.
  const report = composeStepPrompt({
    employeeName: "侦察员",
    index: 4,
    stepCount: 5,
    step: { nodeId: "report", kind: "notify", title: "汇总", instruction: "", config: {} },
    notes: [],
    trace: [],
  });
  assert.match(report, /what actually happened/);
});

test("the last anchored marker wins when a step quotes one earlier", () => {
  const reply = [
    "STEP RESULT: this line is quoted from the instruction, not my answer.",
    "",
    "Everything else is done.",
    "",
    "STEP RESULT: the real one.",
  ].join(String.fromCharCode(10));
  assert.equal(stepResultOf(reply), "the real one.");
});

test("a marker inside a sentence is not the step result", () => {
  // Run 6e6e5672's publish step explained this extractor in its own reply and
  // the first match anywhere stored the tail of that sentence as the signal.
  const reply = [
    "The driver stores the text after my STEP RESULT: marker (whitespace-collapsed, <=2000 chars).",
    "",
    "So the write happens after my turn.",
    "",
    "STEP RESULT: 1 条信号已写入共享记忆。",
  ].join(String.fromCharCode(10));
  assert.equal(stepResultOf(reply), "1 条信号已写入共享记忆。");
});

test("a reply that never used the marker is recorded as itself", () => {
  assert.equal(stepResultOf("no marker in this one"), "no marker in this one");
});

const boardStep = (overrides = {}) =>
  composeStepPrompt({
    employeeName: "线索主管",
    index: 0,
    stepCount: 5,
    step: {
      nodeId: "read",
      kind: "board",
      title: "读看板",
      instruction: "Read what the team filed today.",
      config: { action: "judge", board: "leads" },
    },
    notes: [],
    trace: [],
    ...overrides,
  });

test("a board step is told it decides instead of doing the errand", () => {
  const composed = boardStep();
  // The whole point of the layer: a manager that posts its own comments is a
  // worker with a title, and nobody is left to decide.
  assert.match(composed, /do not post, comment, message or email/);
  // A judgement that names nobody is a decision with no effect.
  assert.match(composed, /"dispatched"/);
  assert.match(composed, /"verdict"/);
});

test("a run carrying an order says so instead of calling itself scheduled", () => {
  const step = { nodeId: "n3", kind: "analyze", title: "选位置", instruction: "" };
  const scheduled = composeStepPrompt({
    employeeName: "地推员",
    index: 2,
    stepCount: 6,
    step,
    notes: [],
    trace: [],
  });
  assert.match(scheduled, /in a scheduled run\./);
  const ordered = composeStepPrompt({
    employeeName: "地推员",
    index: 2,
    stepCount: 6,
    step,
    notes: [],
    trace: [],
    task: { issuer: "线索主管", verb: "outreach", task: "主动触达 大胖" },
  });
  assert.match(ordered, /in a run carrying a middle manager's order\./);
  assert.doesNotMatch(ordered, /in a scheduled run\./);
});
test("an order is put in front of the standing steps", () => {
  const composed = boardStep({
    employeeName: "笔杆子",
    task: { issuer: "线索主管", verb: "produce", task: "写一篇讲权限边界的长文" },
  });
  assert.match(composed, /今天的临时任务（线索主管 派下来的，类型 produce）：/);
  assert.match(composed, /写一篇讲权限边界的长文/);
  assert.match(composed, /这件事优先于本步骤的常规动作/);
  assert.ok(
    composed.indexOf("今天的临时任务") < composed.indexOf("Read what the team filed today."),
    'the order is stated before the step it arrives with',
  );
});

test("a run carrying no order says nothing about a temporary task", () => {
  assert.doesNotMatch(compose([], []), /临时任务/);
});

test("an order that arrived without an issuer still says who asked", () => {
  const composed = boardStep({ task: { task: "写一篇讲权限边界的长文" } });
  assert.match(composed, /今天的临时任务（中层领导 派下来的）：/);
});

