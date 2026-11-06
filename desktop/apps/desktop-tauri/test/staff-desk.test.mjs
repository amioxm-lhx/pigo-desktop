/**
 * The desk's own arithmetic.
 *
 * Three pages read this file, and every number on them is a count of rows the
 * host already stores. What can go wrong here is not a crash: it is a window
 * that cuts a day in the wrong place, a station that stays quiet while the ball
 * is in the owner's court, and a cleanup that promises twelve rows and takes
 * thirteen. So the fixtures below are rows, and the assertions are counts.
 */
import assert from "node:assert/strict";
import test from "node:test";
import { register } from "node:module";

register(new URL("./helpers/ts-import-hooks.mjs", import.meta.url));
const {
  BLOCKER_TEXT,
  DESK_STATIONS,
  KPI_TEXT,
  STATION_TEXT,
  TODO_TEXT,
  buildBlockers,
  buildEmployeeRows,
  buildStations,
  buildTodos,
  deadRuns,
  happenedSince,
  latestRun,
  localDayStart,
  loopProgress,
  newestAt,
  nextLoop,
  roundsToday,
  runToLine,
  runnableNow,
  sizeLabel,
  slotLabel,
  utf8Bytes,
  within,
} = await import("../src/features/desk/desk-model.ts");
const { LEDGER_STAGES, PRUNE_RULES, brokenLink, buildReferences, ledgerRows, ruleOf, teamFootprint } =
  await import("../src/features/desk/desk-ledger.ts");
// The shared rule the rail's badge and this page both have to answer with:
// importing it here is the point -- a second copy is how two pages end up
// disagreeing about whether a piece is still waiting on the owner.
const { addresslessChannel, needsAddress } = await import("@pi-desktop/shared");

/** The key plus whatever it was handed: enough to see what a page would say. */
const t = (key, params) => (params ? `${key} ${JSON.stringify(params)}` : key);

/** A local noon, so the window assertions do not depend on the machine's zone. */
const NOON = new Date(2026, 8, 22, 12, 0, 0).getTime();
const at = (minutesAgo) => new Date(NOON - minutesAgo * 60_000).toISOString();

function snapshot(parts = {}) {
  return {
    employees: [],
    runs: [],
    contents: [],
    leads: [],
    metrics: [],
    advice: [],
    directives: [],
    memory: [],
    sites: [],
    watch: null,
    scheduler: null,
    summary: null,
    ...parts,
  };
}

const employee = (id, name, extra = {}) => ({
  id,
  name,
  layer: "worker",
  status: "running",
  quota: { maxRunsPerDay: 2 },
  nextRunAt: undefined,
  ...extra,
});

const lead = (id, extra = {}) => ({
  id,
  title: `卡 ${id}`,
  quote: "",
  site: "zhihu",
  url: "",
  score: 40,
  stage: "new",
  note: "",
  createdAt: at(30),
  updatedAt: at(30),
  ...extra,
});

const content = (id, extra = {}) => ({
  id,
  title: `稿 ${id}`,
  status: "draft",
  variant: "",
  channel: "reply",
  topic: "",
  body: "",
  publishedUrl: "",
  views: 0,
  replies: 0,
  createdAt: at(30),
  updatedAt: at(30),
  ...extra,
});

const run = (id, extra = {}) => ({
  id,
  employeeId: "e1",
  status: "succeeded",
  trigger: "schedule",
  startedAt: at(20),
  currentStep: 2,
  stepCount: 3,
  stepTitles: ["巡站", "判断", "交卷"],
  ...extra,
});

const station = (stations, id) => stations.find((entry) => entry.id === id);


/* ------------------------------------------------------------- the window -- */

test("a day is cut where the reader's day starts, not where UTC does", () => {
  assert.equal(localDayStart(NOON), new Date(2026, 8, 22).getTime());
  assert.equal(new Date(localDayStart(NOON)).getHours(), 0);
  // A moment on the same day counts; one just before it does not.
  assert.equal(happenedSince(at(0), localDayStart(NOON)), true);
  assert.equal(happenedSince(at(13 * 60), localDayStart(NOON)), false);
  // A timestamp that does not parse is not silently treated as now.
  assert.equal(happenedSince(undefined, 0), false);
  assert.equal(happenedSince("not a date", 0), false);
});

test("a window selects the rows that moved in it, not the rows that were born in it", () => {
  const since = NOON - 60 * 60_000;
  const old = lead("old", { createdAt: at(300), updatedAt: at(300) });
  // Filed last week, answered twenty minutes ago: this is work happening now.
  const touched = lead("touched", { createdAt: at(300), updatedAt: at(20) });
  assert.deepEqual(
    within([old, touched], since).map((row) => row.id),
    ["touched"],
  );
  // No window means the whole book, which is what the "everything" view reads.
  assert.equal(within([old, touched], undefined).length, 2);
});

test("the newest timestamp of a set is the one a station dates itself by", () => {
  assert.equal(newestAt([at(300), at(5), at(60)]), at(5));
  assert.equal(newestAt([undefined, ""]), undefined);
  assert.equal(newestAt([]), undefined);
});

test("a slot the owner is not reading today carries its date", () => {
  const sameDay = slotLabel(at(120), "zh-CN", NOON);
  assert.match(sameDay, /^\d{2}:\d{2}$/, sameDay);
  const tomorrow = slotLabel(new Date(NOON + 86_400_000).toISOString(), "zh-CN", NOON);
  assert.match(tomorrow, /09\/23/, tomorrow);
  assert.equal(slotLabel(undefined, "zh-CN", NOON), "");
  assert.equal(slotLabel("nonsense", "zh-CN", NOON), "");
});

test("a row's size is measured in bytes, not counted in characters", () => {
  // String.length is wrong by a third on Chinese, which is most of what these
  // tables hold, so the byte count is measured instead.
  assert.equal(utf8Bytes("中"), Buffer.byteLength(JSON.stringify("中"), "utf8"));
  assert.ok(utf8Bytes("中") > "中".length);
  assert.equal(utf8Bytes({ a: "中" }), Buffer.byteLength(JSON.stringify({ a: "中" }), "utf8"));

  assert.equal(sizeLabel(0, t), "desk.size.none");
  assert.match(sizeLabel(500, t), /^desk\.size\.bytes/);
  assert.match(sizeLabel(2048, t), /^desk\.size\.kb/);
  assert.match(sizeLabel(5 * 1024 * 1024, t), /^desk\.size\.mb/);
});

/* ----------------------------------------------------------- the stations -- */

test("the ten stations are drawn in the order the work travels", () => {
  assert.deepEqual(DESK_STATIONS, [
    "probe",
    "sift",
    "topics",
    "draft",
    "approve",
    "publish",
    "watch",
    "talk",
    "close",
    "deliver",
  ]);
  // Whole keys, never assembled at the call site: a key built from an id is a
  // key nothing checks, and three pages have to agree on what these are called.
  for (const id of DESK_STATIONS) {
    const text = STATION_TEXT[id];
    assert.match(text.name, /^desk\.station\./);
    assert.match(text.who, /^desk\.who\./);
    assert.match(text.out, /^desk\.station\./);
    assert.equal(text.columns.length, 3);
    for (const column of text.columns) assert.match(column, /^desk\.col\./);
  }
  for (const kind of Object.keys(TODO_TEXT)) {
    assert.match(TODO_TEXT[kind].tag, /^desk\.todo\./);
    assert.match(TODO_TEXT[kind].go, /^desk\.todo\./);
  }
  for (const field of Object.keys(KPI_TEXT)) assert.match(KPI_TEXT[field], /^desk\.kpi\./);
  for (const kind of Object.keys(BLOCKER_TEXT)) assert.match(BLOCKER_TEXT[kind], /^desk\.blocker\./);
});

test("the ball is drawn where it actually is", () => {
  const stations = buildStations(
    snapshot({
      employees: [employee("e-pusher", "地推员")],
      contents: [
        content("draft-1"),
        content("review-1", { status: "review" }),
        content("approved-1", { status: "approved" }),
        content("published-1", { status: "published", publishedUrl: "https://example.test/p/1" }),
      ],
      sites: [{ channel: "v2ex", supported: true, signedIn: false }],
    }),
    t,
    NOON,
  );

  // A draft awaiting approval is the owner's door, so it is lit for them --
  // and a draft a run filed at `draft` is waiting on the same yes as one it
  // filed at `review`. Reading only `review` would leave the run's own
  // drafts sitting here with nothing asking about them.
  assert.equal(station(stations, "approve").state, "owner");
  assert.equal(station(stations, "approve").count, 2);
  // So is an approved piece nobody has posted; the row names the pusher.
  assert.equal(station(stations, "publish").state, "owner");
  assert.equal(station(stations, "publish").employee.name, "地推员");
  // A published piece is the watch's job, and it stays quiet while it works.
  assert.equal(station(stations, "watch").state, "machine");
  // A site that is signed out is the one thing that is broken rather than slow.
  assert.equal(station(stations, "probe").state, "stuck");
  assert.equal(station(stations, "probe").note.key, "desk.note.siteSignedOut");

  const progress = loopProgress(stations);
  assert.equal(progress.total, 10);
  assert.equal(progress.waiting, "approve");
  assert.equal(progress.stuck, "probe");
  assert.equal(progress.furthest, "watch");
  // Four stations hold rows: draft, approve, publish, watch.
  assert.equal(progress.filled, 4);
  // 写稿 counts versions written, so it overlaps the stations the same pieces
  // moved on to: it answers "how much did the writer produce", not "what is
  // sitting here". A retired piece was not produced for anything, so it is the
  // one status this leaves out.
  assert.equal(station(stations, "draft").count, 4);
});

test("a stopped seat is the difference between slow and broken", () => {
  const replied = lead("l1", { stage: "replied" });
  const reception = employee("e-reception", "接线员");
  const running = buildStations(
    snapshot({ employees: [reception], leads: [replied] }),
    t,
    NOON,
  );
  assert.equal(station(running, "talk").state, "machine");
  assert.equal(station(running, "talk").note, undefined);

  const stopped = buildStations(
    snapshot({ employees: [{ ...reception, status: "paused" }], leads: [replied] }),
    t,
    NOON,
  );
  // Nothing is waiting for the receptionist to finish; the receptionist is off.
  assert.equal(station(stopped, "talk").state, "stuck");
  assert.equal(station(stopped, "talk").note.key, "desk.note.seatStopped");
  assert.equal(station(stopped, "talk").note.value, "接线员");
});


/* ---------------------------------------------------------------- the day -- */

test("every row on the day page has a door, and leaves when it is done", () => {
  const answered = lead("l1", { title: "凭什么让人相信一句「不可行」" });
  const todos = buildTodos(
    snapshot({
      employees: [employee("e-writer", "笔杆子")],
      contents: [
        content("review-1", { status: "review", employeeId: "e-writer" }),
        content("approved-1", { status: "approved", leadId: answered.id }),
        content("no-link", { status: "published" }),
        content("linked", { status: "published", publishedUrl: "https://example.test/p/2" }),
      ],
      leads: [answered, lead("l2", { stage: "talking", score: 75 })],
      advice: [{ id: "s1", status: "open", title: "今天主攻拍板权归谁", evidence: [{}, {}], createdAt: at(60) }],
    }),
    t,
  );

  // Read in the order the work is blocked in: a draft nobody approved stops the
  // round two stations earlier than a card nobody has called.
  assert.deepEqual(
    todos.map((todo) => todo.kind),
    ["approve", "publish", "link", "advice", "deal"],
  );
  // The row that says "publish this" names the card it answers.
  const publish = todos.find((todo) => todo.kind === "publish");
  assert.ok(publish.detail.includes(answered.title), publish.detail);
  // A piece that already carries its address has nothing to chase.
  assert.equal(todos.filter((todo) => todo.kind === "link").length, 1);
  assert.ok(todos.find((todo) => todo.kind === "link").title.includes("no-link"));
  // The advice row is answered where it stands; the others send you to a page.
  assert.equal(todos.find((todo) => todo.kind === "advice").suggestionId, "s1");
  assert.equal(todos.find((todo) => todo.kind === "advice").page, undefined);
  assert.equal(todos.find((todo) => todo.kind === "deal").page, "leads");
  assert.equal(todos.find((todo) => todo.kind === "approve").page, "contents");
});

test("a draft a run filed is asked about, not left sitting", () => {
  // The host files a run's own draft at `draft` and never at `review`: a run
  // does not submit its work for approval, it just leaves it where a person
  // will find it. Reading only `review` here left every draft a real run
  // produced invisible on this page -- which is the one thing the page exists
  // to prevent.
  const todos = buildTodos(
    snapshot({
      employees: [employee("e-writer", "笔杆子")],
      contents: [content("filed-by-a-run", { status: "draft", employeeId: "e-writer" })],
    }),
    t,
  );
  assert.equal(todos.length, 1);
  assert.equal(todos[0].kind, "approve");
  assert.equal(todos[0].title, "稿 filed-by-a-run");
  assert.equal(todos[0].page, "contents");
});

test("a row that is already handled does not come back as a to-do", () => {
  const todos = buildTodos(
    snapshot({
      contents: [content("published", { status: "published", publishedUrl: "https://e.test/1" })],
      advice: [{ id: "s1", status: "accepted", title: "已处理", evidence: [], createdAt: at(60) }],
      leads: [lead("l1", { stage: "won" })],
    }),
    t,
  );
  assert.deepEqual(todos, []);
});

test("a run that died inside the day is broken; an older one is history", () => {
  const employees = [employee("e1", "侦察员")];
  const recent = run("r-recent", { status: "failed", startedAt: at(120), errorCode: "SITE_SIGNED_OUT" });
  const ancient = run("r-ancient", { status: "failed", startedAt: at(3 * 24 * 60) });
  const fine = run("r-fine", { status: "succeeded", startedAt: at(30) });
  const dead = deadRuns(snapshot({ employees, runs: [recent, ancient, fine] }), NOON);
  assert.deepEqual(dead.map((entry) => entry.run.runId), ["r-recent"]);

  const blockers = buildBlockers(
    snapshot({
      employees,
      runs: [recent, ancient, fine],
      sites: [
        { channel: "v2ex", supported: true, signedIn: false },
        { channel: "zhihu", supported: true, signedIn: true },
        { channel: "wechat-mp", supported: false, signedIn: false },
      ],
    }),
    t,
    NOON,
  );
  // One site is signed out, one run died. A site this build cannot drive at all
  // is not a fault, and a signed-out site that is already signed in is not one
  // either.
  assert.deepEqual(blockers.map((entry) => entry.kind), ["site", "run"]);
  assert.equal(blockers[0].channel, "v2ex");
  assert.equal(blockers[1].runId, "r-recent");
  assert.equal(blockers[1].detail, "SITE_SIGNED_OUT");
});

/* -------------------------------------------------------------- the round -- */

test("the replay shows the steps that came back empty, not just the happy path", () => {
  const employees = [employee("e1", "侦察员")];
  const line = runToLine(
    run("r1", { results: { 1: "读了三页" }, reports: { 1: "整份回复" }, currentStep: 2 }),
    employees,
  );
  assert.equal(line.employee, "侦察员");
  assert.equal(line.stepCount, 3);
  assert.equal(line.steps.length, 3);
  assert.equal(line.steps[0].result, "读了三页");
  assert.equal(line.steps[0].report, "整份回复");
  // A step that has not reported yet is drawn as empty rather than as done.
  assert.equal(line.steps[1].result, undefined);
  assert.equal(line.steps[2].report, undefined);

  // A seat that no longer exists is still named by something, so the row is
  // never a blank where a person should be.
  const orphan = runToLine(run("r2", { employeeId: "0123456789abcdef" }), []);
  assert.equal(orphan.employee, "01234567");

  // The header narrates the newest run of the week, and says nothing at all
  // when the last one is older than that.
  assert.equal(latestRun(snapshot({ employees, runs: [run("old", { startedAt: at(8 * 24 * 60) })] }), NOON), null);
  assert.equal(latestRun(snapshot({ employees, runs: [run("r1")] }), NOON).runId, "r1");
  assert.equal(
    latestRun(snapshot({ employees, runs: [run("a", { startedAt: at(600) }), run("b", { startedAt: at(5) })] }), NOON).runId,
    "b",
  );
});

test("the roster counts today against the quota and names the step in flight", () => {
  const employees = [
    employee("e-writer", "笔杆子"),
    employee("e-boss", "线索主管", { layer: "manager", quota: { maxRunsPerDay: 5 } }),
  ];
  const runs = [
    run("a", { employeeId: "e-writer", startedAt: at(60) }),
    run("b", { employeeId: "e-writer", startedAt: at(120) }),
    // Yesterday's run does not spend today's quota.
    run("c", { employeeId: "e-writer", startedAt: at(30 * 60) }),
    run("d", { employeeId: "e-boss", startedAt: at(90) }),
  ];
  const rows = buildEmployeeRows(
    snapshot({
      employees,
      runs,
      scheduler: { inFlight: [{ employeeId: "e-boss", runId: "d", step: 2, stepCount: 4 }] },
    }),
    t,
    NOON,
  );
  // A manager is read first: it is the seat that hands work out.
  assert.deepEqual(rows.map((row) => row.employee.name), ["线索主管", "笔杆子"]);
  assert.equal(rows[0].ranToday, 1);
  assert.equal(rows[0].quota, 5);
  assert.match(rows[0].live, /^desk\.employee\.live /);
  assert.ok(rows[0].live.includes("判断"), rows[0].live);
  assert.equal(rows[1].ranToday, 2);
  assert.equal(rows[1].live, undefined);

  // Only a seat that is running and has room is worth a hand: the one in flight
  // is busy, and the one that spent its two runs is done for the day.
  const ready = runnableNow(
    snapshot({
      employees,
      runs,
      scheduler: { inFlight: [{ employeeId: "e-boss", runId: "d", step: 2, stepCount: 4 }] },
    }),
    t,
    NOON,
  );
  assert.equal(ready.length, 0);
  const spare = runnableNow(
    snapshot({ employees, runs: [run("a", { employeeId: "e-writer", startedAt: at(60) })] }),
    t,
    NOON,
  );
  // The manager is still read first: it is the seat that hands work out.
  assert.deepEqual(spare.map((row) => row.employee.name), ["线索主管", "笔杆子"]);
});

test("the two resident loops are read off the seats and the watch", () => {
  const loops = nextLoop(
    snapshot({
      employees: [
        employee("a", "侦察员", { nextRunAt: new Date(NOON + 90 * 60_000).toISOString() }),
        employee("b", "选题员", { nextRunAt: new Date(NOON + 30 * 60_000).toISOString() }),
      ],
      watch: { nextAt: new Date(NOON + 15 * 60_000).toISOString() },
    }),
  );
  // The dispatch loop is whichever seat is up next, not the first row.
  assert.equal(loops.dispatch, new Date(NOON + 30 * 60_000).toISOString());
  assert.equal(loops.watch, new Date(NOON + 15 * 60_000).toISOString());
  assert.deepEqual(nextLoop(snapshot()), { dispatch: undefined, watch: undefined });
});


/* ---------------------------------------------------------------- the ledger -- */

/** What the host measures, in the shape the page reads it back in. */
const stats = (stages) => ({
  stages,
  database: { bytes: 17_800_000 },
  retention: { leads: 30, contents: 30, dispatch: 90, metrics: 180, runsKept: 30, runs: 30 },
});

const stageRow = (id, extra = {}) => ({
  id,
  rows: 0,
  bytes: 0,
  parkedRows: 0,
  parkedBytes: 0,
  cleanable: 0,
  cleanableBytes: 0,
  ...extra,
});

test("the ledger is read in the order the work travels, not the order it arrived", () => {
  const rows = ledgerRows(
    stats([
      // The host may report them in any order, and may report one this build
      // does not know about yet.
      stageRow("runs", { rows: 47, bytes: 936_000 }),
      stageRow("memory", { rows: 30, bytes: 18_000 }),
      stageRow("something-new", { rows: 3 }),
      stageRow("scan"),
      stageRow("leads", { rows: 11, bytes: 12_000, cleanable: 1, cleanableBytes: 400 }),
    ]),
    snapshot(),
  );
  assert.deepEqual(rows.map((row) => row.id), ["scan", "leads", "runs", "memory"]);
  assert.equal(rows[1].cleanable, 1);
  // 巡站 keeps nothing at all, and the page draws that as a statement rather
  // than as a zero that reads like a fault.
  assert.equal(rows[0].ephemeral, true);
  assert.equal(rows[1].ephemeral, false);

  // A stage the host could not report is left out rather than drawn as a zero:
  // "we could not read it" and "there is nothing here" are different sentences.
  assert.equal(ledgerRows(null, snapshot()).length, 0);
});

test("the archived half of a stage is carried through with the live half", () => {
  const rows = ledgerRows(
    stats([
      // Sixty-eight drafts nobody wants are still rows on disk, and the row
      // has to say so: "0" alone would read as if the stage never wrote one.
      stageRow("contents", { rows: 0, bytes: 0, parkedRows: 68, parkedBytes: 16_813 }),
      stageRow("memory", { rows: 5, bytes: 3_400, parkedRows: 25, parkedBytes: 17_486 }),
    ]),
    snapshot(),
  );
  const contents = rows.find((row) => row.id === "contents");
  assert.equal(contents.rows, 0);
  assert.equal(contents.parkedRows, 68);
  assert.equal(contents.parkedBytes, 16_813);
  assert.equal(rows.find((row) => row.id === "memory").parkedRows, 25);
});

test("a stage that parks nothing reports zero rather than undefined", () => {
  const rows = ledgerRows(stats([stageRow("leads", { rows: 11 })]), snapshot());
  assert.equal(rows[0].parkedRows, 0);
  assert.equal(rows[0].parkedBytes, 0);
});

test("the team's share of the file counts what it archived as well", () => {
  const rows = ledgerRows(
    stats([
      stageRow("contents", { rows: 0, bytes: 0, parkedRows: 68, parkedBytes: 45_000 }),
      stageRow("runs", { rows: 47, bytes: 900_000 }),
    ]),
    snapshot(),
  );
  // Retiring a draft moves bytes from the live half to the archived half. The
  // file on disk has not moved, so the total must not either.
  assert.equal(teamFootprint(rows), 945_000);
  assert.equal(teamFootprint([]), 0);
});

test("a stage's last-added moment comes off the rows the page already holds", () => {
  const rows = ledgerRows(
    stats([stageRow("leads"), stageRow("contents"), stageRow("publish")]),
    snapshot({
      leads: [lead("l1", { updatedAt: at(90) }), lead("l2", { updatedAt: at(5) })],
      // A piece's own row was written when it was drafted; what the publish
      // stage dates itself by is the moment it went out.
      contents: [content("c1", { createdAt: at(300), publishedAt: at(45) })],
    }),
  );
  const byId = Object.fromEntries(rows.map((row) => [row.id, row]));
  assert.equal(byId.leads.at, at(5));
  assert.equal(byId.publish.at, at(45));
  assert.equal(byId.contents.at, at(300));
  // Nothing in hand means nothing to say, which the page draws as a dash.
  assert.equal(
    ledgerRows(stats([stageRow("leads")]), snapshot()).find((row) => row.id === "leads").at,
    undefined,
  );
});

test("each stage's cleanup rule is the one its own rows are measured against", () => {
  assert.deepEqual(ruleOf("contents"), "contents-retired");
  assert.deepEqual(ruleOf("leads"), "leads-dead");
  assert.deepEqual(ruleOf("dispatch"), "dispatch-answered");
  assert.deepEqual(ruleOf("metrics"), "metrics-aged");
  assert.deepEqual(ruleOf("runs"), "runs-reports");
  assert.deepEqual(ruleOf("memory"), "memory-expired");
  // A stage that keeps nothing, and one that is kept forever, have no rule:
  // offering one would be offering to delete something nobody agreed to.
  assert.equal(ruleOf("scan"), null);
  assert.equal(ruleOf("publish"), null);
  assert.equal(PRUNE_RULES.length, 6);
});

test("the page counts the links that exist, not the links a schema hints at", () => {
  const snap = snapshot({
    leads: [lead("l1", { stage: "won" }), lead("l2", { stage: "replied" })],
    contents: [
      content("c1", { leadId: "l1", status: "published", topic: "数据不出本地" }),
      content("c2", { status: "approved" }),
      content("c3", { status: "published" }),
    ],
    metrics: [{ id: "m1", label: "数据不出本地", day: "2026-09-22", dimension: "topic", views: 10, replies: 1, leads: 0, inquiries: 0, deals: 0 }],
  });
  const refs = Object.fromEntries(buildReferences(snap).map((ref) => [ref.id, ref.count]));
  assert.equal(refs["contents-leads"], 1);
  assert.equal(refs["publish-contents"], 1);
  assert.equal(refs["metrics-topics"], 1);
  assert.equal(refs["leads-dispatch"], 1);
});

test("the page names the link that is broken, and only when one is", () => {
  const breaks = (parts) => {
    const gap = brokenLink(snapshot(parts));
    return gap ? { key: gap.key, ...gap.values } : null;
  };

  // Drafts and not one approved: the round stops two stations before the
  // customer, and that is worth more than every byte on the page.
  assert.deepEqual(breaks({ contents: [content("c1"), content("c2")] }), {
    key: "desk.data.breakNoApprove",
    drafts: 2,
  });
  assert.deepEqual(breaks({ contents: [content("c1", { status: "approved" })] }), {
    key: "desk.data.breakNoPublish",
    approved: 1,
  });
  assert.deepEqual(
    breaks({ contents: [content("c1", { status: "published", publishedUrl: "https://e.test/1" })] }),
    { key: "desk.data.breakNoReply", published: 1 },
  );
  assert.deepEqual(
    breaks({
      contents: [content("c1", { status: "published" })],
      leads: [lead("l1", { stage: "replied" }), lead("l2", { stage: "talking" })],
    }),
    { key: "desk.data.breakNoClose", talking: 2 },
  );
  // A chain that runs end to end has nothing to fix, and the page says so
  // rather than inventing something to do.
  assert.equal(
    breaks({
      contents: [content("c1", { status: "published" })],
      leads: [lead("l1", { stage: "replied" }), lead("l2", { stage: "won" })],
    }),
    null,
  );
  // Retired drafts are not a stall: nobody is waiting on them.
  assert.equal(breaks({ contents: [content("c1", { status: "retired" })] }), null);
  assert.equal(breaks({}), null);
});


test("a row that settles a draft carries the card, and every row carries its post", () => {
  // The two things the owner asked for: a way back to the post the work answers,
  // and a draft that can be approved without hunting for the page that does it.
  const answered = lead("l1", { url: "https://example.test/q/1/answer/2" });
  const todos = buildTodos(
    snapshot({
      employees: [employee("e-writer", "笔杆子")],
      contents: [
        content("draft-1", { status: "draft", employeeId: "e-writer", leadId: answered.id }),
        content("orphan", { status: "draft", employeeId: "e-writer" }),
      ],
      leads: [answered, lead("l2", { url: "https://example.test/q/3", stage: "talking" })],
    }),
    t,
  );
  const filed = todos.find((todo) => todo.id === "approve:draft-1");
  assert.equal(filed.contentId, "draft-1");
  assert.equal(filed.link, answered.url);
  // A card written for no particular post has no post to point at, and it still
  // has to be approvable -- the id is what the button needs, not the link.
  const orphan = todos.find((todo) => todo.id === "approve:orphan");
  assert.equal(orphan.link, undefined);
  assert.equal(orphan.contentId, "orphan");
  // A conversation that came off a page carries that page as well.
  assert.equal(todos.find((todo) => todo.kind === "deal").link, "https://example.test/q/3");
});

test("the machine's round count is the day's, not the book's", () => {
  const runs = [
    run("r-now", { startedAt: at(5) }),
    run("r-morning", { startedAt: at(600) }),
    run("r-yesterday", { startedAt: at(20 * 60) }),
  ];
  assert.equal(roundsToday(snapshot({ runs }), NOON), 2);
  assert.equal(roundsToday(snapshot({ runs: [] }), NOON), 0);
});

test("a card filed before the board knew its lead still points at its post", () => {
  // Drafts written before the lead join carry no lead id, and the url they were
  // written for survives only in the JSON block the step filed. The page reads
  // it back rather than sending the owner to a card with no source.
  const block = [
    "Here is what I filed.",
    "\`\`\`json",
    JSON.stringify({
      items: [
        { title: "别的稿子", body: "x", leadUrl: "https://e.test/other" },
        { title: "稿 orphan", body: "y", leadUrl: "https://e.test/q/7" },
      ],
    }),
    "\`\`\`",
    "STEP RESULT: filed one.",
  ].join("\n");
  const todos = buildTodos(
    snapshot({
      contents: [content("orphan", { status: "draft", runId: "r-1" })],
      runs: [run("r-1", { reports: { "3": block } })],
    }),
    t,
  );
  assert.equal(todos[0].link, "https://e.test/q/7");
  // The title is the join, so a row the block never carried gets no link --
  // never a link to somebody else's post.
  const none = buildTodos(
    snapshot({
      contents: [content("orphan", { status: "draft", runId: "r-2" })],
      runs: [run("r-2", { reports: { "3": "no block here" } })],
    }),
    t,
  );
  assert.equal(none[0].link, undefined);
});

test("a publish row names its post when it can, and stays short when it cannot", () => {
  const named = lead("l1", { title: "为什么不写留痕", url: "https://e.test/q/1" });
  const byId = buildTodos(
    snapshot({
      contents: [content("c1", { status: "approved", leadId: named.id })],
      leads: [named],
    }),
    t,
  );
  assert.match(byId[0].detail, /^desk\.todo\.publishDetailFrom /);
  assert.ok(byId[0].detail.includes("为什么不写留痕"), byId[0].detail);

  // The url alone is enough to name the poster: a row filed before the board
  // kept the lead id still points at somebody rather than at nobody.
  const block = [
    "\`\`\`json",
    JSON.stringify({ items: [{ title: "稿 c2", leadUrl: "https://e.test/q/1" }] }),
    "\`\`\`",
  ].join("\n");
  const byUrl = buildTodos(
    snapshot({
      contents: [content("c2", { status: "approved", runId: "r-1" })],
      leads: [named],
      runs: [run("r-1", { reports: { "2": block } })],
    }),
    t,
  );
  assert.match(byUrl[0].detail, /^desk\.todo\.publishDetailFrom /);
  assert.equal(byUrl[0].link, "https://e.test/q/1");

  // A row that answers nobody in particular says only what is left to do.
  const plain = buildTodos(snapshot({ contents: [content("c3", { status: "approved" })] }), t);
  assert.equal(plain[0].detail, "desk.todo.publishDetail");
  assert.equal(plain[0].link, undefined);
});

test("a handed-over piece leaves the owner's list and joins the poster's queue", () => {
  // The owner's second click is their last one: the piece is now the poster's
  // job, so it must not sit on the day page as work waiting for a person --
  // and the station that draws "not out yet" still shows it, as the machine's.
  const queued = content("q1", { status: "queued" });
  assert.deepEqual(buildTodos(snapshot({ contents: [queued] }), t), []);

  const stations = buildStations(snapshot({ contents: [queued] }), t);
  assert.equal(station(stations, "publish").count, 1);
  assert.equal(station(stations, "publish").state, "machine");

  // With anything still waiting for the owner's click the ball is theirs, even
  // with a queue sitting behind it.
  const both = buildStations(
    snapshot({ contents: [content("a1", { status: "approved" }), queued] }),
    t,
  );
  assert.equal(station(both, "publish").state, "owner");
  assert.equal(station(both, "publish").count, 2);
});

test("a queue nobody has posted from yet is still the broken link", () => {
  // Handing a piece over is not saying it: the chain still breaks at
  // "decided, not out", and the page has to say so rather than showing an
  // empty gap where the next step should be.
  const breaks = (contents) => brokenLink(snapshot({ contents }));
  assert.deepEqual(breaks([content("c1", { status: "queued" })]), {
    key: "desk.data.breakNoPublish",
    values: { approved: 1 },
  });
  assert.deepEqual(
    breaks([content("c1", { status: "approved" }), content("c2", { status: "queued" })]),
    { key: "desk.data.breakNoPublish", values: { approved: 2 } },
  );
  // Once it is out, the next link is the one the page looks for.
  assert.deepEqual(
    breaks([content("c1", { status: "published", publishedUrl: "https://e.test/1" })]),
    { key: "desk.data.breakNoReply", values: { published: 1 } },
  );
});

test("a publish row carries its card, so the last mile happens on the row", () => {
  // Publishing used to mean a trip to the studio for the piece and the field
  // for its address. The row carries the id, so the day page unfolds the words
  // and the address where the owner already is.
  const todos = buildTodos(snapshot({ contents: [content("c1", { status: "approved" })] }), t);
  assert.equal(todos[0].kind, "publish");
  assert.equal(todos[0].contentId, "c1");
});

test("a private message is published without chasing an address it cannot have", () => {
  // A direct piece is sent, not posted: there is no page to read it back from,
  // so a row asking for its link would chase an address that cannot exist.
  // Anything that lands where a watch can revisit it still asks -- and a
  // channel typed with stray space or case is the same channel.
  const todos = buildTodos(
    snapshot({
      contents: [
        content("dm", { status: "published", channel: "direct" }),
        content("posted", { status: "published", channel: "reply" }),
        content("spaced", { status: "published", channel: "  Direct " }),
      ],
    }),
    t,
  );
  assert.deepEqual(
    todos.map((todo) => todo.kind),
    ["link"],
  );
  assert.ok(todos[0].title.includes("posted"), todos[0].title);
});

test('the badge and the desk ask "does this need an address" with one rule', () => {
  assert.equal(addresslessChannel("direct"), true);
  assert.equal(addresslessChannel("  Direct "), true);
  assert.equal(addresslessChannel("reply"), false);
  assert.equal(addresslessChannel(undefined), false);

  const unlinked = { status: "published", publishedUrl: "", channel: "reply" };
  const dm = { status: "published", publishedUrl: "", channel: "direct" };
  assert.equal(needsAddress({ status: "approved", publishedUrl: "", channel: "reply" }), false);
  assert.equal(needsAddress({ status: "published", publishedUrl: "https://e.test/1", channel: "reply" }), false);
  assert.equal(needsAddress(unlinked), true);
  assert.equal(needsAddress(dm), false);

  // And whatever the rule says, the desk draws exactly that: one row per piece
  // it calls a task, so the number on the rail and the list here are one list.
  const todos = buildTodos(snapshot({ contents: [content("a", unlinked), content("b", dm)] }), t);
  assert.deepEqual(
    todos.map((todo) => todo.title),
    ["稿 a"],
  );
});
