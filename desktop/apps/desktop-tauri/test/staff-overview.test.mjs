/**
 * The overview tab's arithmetic, without a DOM.
 *
 * "Does anything need me?" is the question an unattended system has to get
 * right, so the count, the success rate, and the exception list are pinned down
 * here rather than by looking at the page.
 */
import assert from "node:assert/strict";
import test from "node:test";
import { register } from "node:module";

register(new URL("./helpers/ts-import-hooks.mjs", import.meta.url));
const {
  collectAttention,
  latestRunByEmployee,
  staleBlueprints,
  startOfDay,
  summarizeRuns,
} = await import("../src/features/staff/staff-overview.ts");

/** Local wall-clock strings, so the day boundary is the reader's, not UTC's. */
const NOW = Date.parse("2026-09-19T12:00:00");

function run(overrides = {}) {
  return {
    id: "run-1",
    employeeId: "emp-1",
    trigger: "schedule",
    status: "succeeded",
    stepCount: 2,
    currentStep: 2,
    startedAt: "2026-09-19T09:00:00",
    ...overrides,
  };
}

function employee(overrides = {}) {
  return {
    id: "emp-1",
    name: "Scout",
    role: "scout",
    status: "running",
    schedule: {},
    quota: {},
    createdAt: "2026-09-01T00:00:00",
    updatedAt: "2026-09-01T00:00:00",
    ...overrides,
  };
}

test("the day window is local midnight, and in-flight runs are not judged", () => {
  assert.equal(startOfDay(NOW), Date.parse("2026-09-19T00:00:00"));

  const summary = summarizeRuns(
    [
      run({ id: "a", startedAt: "2026-09-19T09:00:00", status: "succeeded" }),
      run({ id: "b", startedAt: "2026-09-19T10:00:00", status: "failed", errorCode: "PROVIDER_QUOTA" }),
      run({ id: "c", startedAt: "2026-09-19T11:00:00", status: "running" }),
      run({ id: "d", startedAt: "2026-09-18T23:30:00", status: "failed" }),
    ],
    NOW,
  );
  assert.deepEqual(summary, {
    todayTotal: 3,
    todayFinished: 2,
    todaySucceeded: 1,
    successRate: 50,
  });

  // Yesterday's failure stays out of today's rate.
  assert.equal(summarizeRuns([run({ startedAt: "2026-09-18T23:30:00" })], NOW).todayTotal, 0);
  // Nothing finished yet is not a 0% day.
  assert.equal(summarizeRuns([run({ status: "running" })], NOW).successRate, null);
  assert.equal(summarizeRuns([], NOW).successRate, null);
});

test("the roster card shows the newest run per employee, in any input order", () => {
  const latest = latestRunByEmployee([
    run({ id: "new", employeeId: "emp-1", startedAt: "2026-09-19T11:00:00" }),
    run({ id: "old", employeeId: "emp-1", startedAt: "2026-09-19T08:00:00" }),
    run({ id: "other", employeeId: "emp-2", startedAt: "2026-09-19T07:00:00" }),
  ]);
  assert.equal(latest.get("emp-1").id, "new");
  assert.equal(latest.get("emp-2").id, "other");
  assert.equal(latest.size, 2);
});

test("a healthy company asks for nothing", () => {
  const attention = collectAttention({
    employees: [employee(), employee({ id: "emp-2", name: "Writer" })],
    runs: [run({ id: "a" }), run({ id: "b", employeeId: "emp-2", status: "running" })],
    status: { resident: true, pollMs: 30_000, claimedTotal: 4, inFlight: [] },
  });
  assert.deepEqual(attention, []);
});

test("the scheduler fault is first, and a paused employee is an exception", () => {
  const attention = collectAttention({
    employees: [employee(), employee({ id: "emp-2", name: "Writer", status: "paused" })],
    runs: [],
    status: {
      resident: true,
      pollMs: 30_000,
      claimedTotal: 1,
      inFlight: [],
      lastError: "Error: host closed",
    },
  });
  assert.deepEqual(
    attention.map((item) => [item.key, item.kind, item.tone]),
    [
      ["scheduler", "scheduler", "bad"],
      ["employee-emp-2", "employee", "warn"],
    ],
  );
  assert.equal(attention[0].message, "Error: host closed");
  assert.equal(attention[1].employeeName, "Writer");
});

test("failures collapse to the newest run per employee, and a gate is a warning", () => {
  const attention = collectAttention({
    employees: [employee(), employee({ id: "emp-2", name: "Writer" })],
    runs: [
      run({ id: "f1", status: "failed", errorCode: "PROVIDER_QUOTA", startedAt: "2026-09-19T11:00:00" }),
      run({ id: "f2", status: "failed", errorCode: "TURN_TIMEOUT", startedAt: "2026-09-19T10:00:00" }),
      run({
        id: "g1",
        employeeId: "emp-2",
        status: "aborted",
        errorCode: "HUMAN_GATE",
        sessionId: "session-emp-2",
        startedAt: "2026-09-19T09:30:00",
      }),
    ],
    status: null,
  });

  assert.deepEqual(
    attention.map((item) => item.key),
    ["run-f1", "run-g1"],
  );
  assert.equal(attention[0].tone, "bad");
  assert.equal(attention[0].errorCode, "PROVIDER_QUOTA");
  assert.equal(attention[0].employeeName, "Scout");
  assert.equal(attention[1].tone, "warn");
  assert.equal(attention[1].gate, true);
  assert.equal(attention[1].sessionId, "session-emp-2");
});

test("an employee that already recovered is not an exception", () => {
  const attention = collectAttention({
    employees: [employee()],
    runs: [
      run({ id: "ok", status: "succeeded", startedAt: "2026-09-19T11:00:00" }),
      run({ id: "old-fail", status: "failed", startedAt: "2026-09-19T09:00:00" }),
    ],
    status: null,
  });
  // The newest run succeeded: the older failure stays in the run log.
  assert.deepEqual(attention, []);
});

test("a gate that is still the newest run keeps asking, whatever the input order", () => {
  const attention = collectAttention({
    employees: [employee()],
    runs: [
      run({ id: "old-ok", status: "succeeded", startedAt: "2026-09-19T08:00:00" }),
      run({
        id: "gate",
        status: "aborted",
        errorCode: "HUMAN_GATE",
        startedAt: "2026-09-19T10:00:00",
      }),
    ],
    status: null,
  });
  assert.deepEqual(attention.map((item) => item.key), ["run-gate"]);
  assert.equal(attention[0].gate, true);
});
test("only an employee running an older blueprint is offered an update", () => {
  const stale = staleBlueprints([
    { id: "scout", name: "Scanner", installed: true, blueprintChanged: true },
    { id: "writer", name: "Scribe", installed: true, blueprintChanged: false },
    { id: "closer", name: "Closer", installed: true },
    // A blueprint nobody has hired has no employee to move.
    { id: "poster", name: "Poster", installed: false, blueprintChanged: true },
  ]);
  assert.deepEqual([...stale], [["Scanner", "scout"]]);
  assert.deepEqual([...staleBlueprints([])], []);
});
