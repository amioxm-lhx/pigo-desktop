/**
 * What a step's reply is allowed to file on a board.
 *
 * The hand-off from an employee to a board is a JSON block the model was asked
 * for, which means the failure mode is not "the model is wrong" but "the model
 * wrote something almost right". Every case here is a reply that looks like
 * data and must not become a card: a half-parsed row would be a claim about a
 * real person that nobody made.
 */
import assert from "node:assert/strict";
import test from "node:test";
import { register } from "node:module";

register(new URL("./helpers/ts-import-hooks.mjs", import.meta.url));
const { boardActionOf, cardArtifactOf, cardContract, fileCards, parseCards, MAX_CARDS_PER_STEP } = await import(
  "../electron/main/runtime/staff-cards.ts"
);

const stepOf = (kind, config) => ({ nodeId: "n1", kind, title: "Step", instruction: "", config });

const fence = (body) =>
  ["Here is what I found.", "```json", body, "```", "STEP RESULT: filed three."].join("\n");

test("only the kinds and artifacts that file rows ask for a contract", () => {
  assert.equal(cardArtifactOf(stepOf("produce", { artifact: "content" })), "content");
  assert.equal(cardArtifactOf(stepOf("analyze", { artifact: "leads" })), "leads");
  // A notify step that filed rows would make the summary the source of record.
  assert.equal(cardArtifactOf(stepOf("notify", { artifact: "leads" })), null);
  // A memory step already has somewhere to put what it learned.
  assert.equal(cardArtifactOf(stepOf("memory", { artifact: "content" })), null);
  // An artifact nobody implements is a step that would run and file nothing.
  assert.equal(cardArtifactOf(stepOf("produce", { artifact: "留言草稿" })), null);
});

test("the contract asks for the block and names the fields", () => {
  const contract = cardContract(stepOf("produce", { artifact: "content" }));
  assert.match(contract, /before the final STEP RESULT line/);
  assert.match(contract, /"body"/);
  assert.match(contract, /empty list is the right answer/);
});

test("the last fenced block wins, because that is the filled-in one", () => {
  const reply = [
    "I first considered this shape:",
    "```json",
    '{"items": []}',
    "```",
    fence('{"items":[{"title":"真实那条","quote":"原话"}]}'),
  ].join("\n");
  const cards = parseCards(reply);
  assert.equal(cards.length, 1);
  assert.equal(cards[0].title, "真实那条");
});

test("a fence that is not JSON is prose wearing a fence", () => {
  const reply = fence("this is not json at all");
  assert.deepEqual(parseCards(reply), []);
});

test("an unfenced object is accepted, because the data is what matters", () => {
  const reply = 'Notes first.\n\n{"items":[{"title":"没有围栏"}]}\n\nSTEP RESULT: filed one.';
  assert.equal(parseCards(reply)[0].title, "没有围栏");
});

test("a reply with no block at all files nothing rather than guessing", () => {
  assert.deepEqual(parseCards("I looked at the pages and nothing matched."), []);
});

test("an object without an items list is not a card block", () => {
  assert.deepEqual(parseCards(fence('{"title":"single card"}')), []);
});

test("a step that returns more than a board can read is cut, not filed whole", () => {
  const items = Array.from({ length: MAX_CARDS_PER_STEP + 5 }, (_, index) => ({
    title: "card " + index,
  }));
  const cards = parseCards(fence(JSON.stringify({ items })));
  assert.equal(cards.length, MAX_CARDS_PER_STEP);
});

test("entries that are not objects are dropped rather than half-read", () => {
  const cards = parseCards(fence('{"items":["a string",null,{"title":"kept"}]}'));
  assert.equal(cards.length, 1);
  assert.equal(cards[0].title, "kept");
});

const boardStep = (action, board = "leads") => stepOf("board", { action, board });

test("a board node files what its action produces, and a read files nothing", () => {
  assert.equal(cardArtifactOf(boardStep("judge")), "decision");
  assert.equal(cardArtifactOf(boardStep("dispatch")), "dispatch");
  assert.equal(cardArtifactOf(boardStep("report")), "advice");
  // A read is answered by the driver from the board's own rows, so asking the
  // model for a block would be asking it to hand back what it was just given.
  assert.equal(cardArtifactOf(boardStep("read")), null);
  // A node whose action nobody filled in files nothing rather than guessing.
  assert.equal(cardArtifactOf(stepOf("board", {})), null);
  // The action is typed into a form by hand, so padding is not a new action.
  assert.equal(boardActionOf(stepOf("board", { action: " dispatch " })), "dispatch");
  assert.equal(cardArtifactOf(stepOf("board", { action: " judge " })), "decision");
});

test("a judgement is filed as one decision carrying the orders it decided on", async () => {
  const calls = [];
  const context = {
    employeeId: "emp-manager",
    employeeName: "线索主管",
    runId: "run-1",
    board: "leads",
    call: async (method, params) => {
      calls.push({ method, params });
      return {};
    },
  };
  const reply = fence(
    JSON.stringify({
      items: [
        {
          title: "今天先答这三条",
          verdict: "三条都在问同一个权限边界。",
          inputs: ["lead-1 的原话"],
          dispatched: [
            { to: "笔杆子", verb: "produce", task: "写一篇讲权限边界的长文", deadlineHours: 6 },
          ],
        },
      ],
    }),
  );
  const outcome = await fileCards(context, boardStep("judge"), reply);
  assert.deepEqual(outcome, { artifact: "decision", filed: 1 });
  assert.equal(calls.length, 1);
  assert.equal(calls[0].method, "staff.decision.record");
  assert.equal(calls[0].params.employeeId, "emp-manager");
  assert.equal(calls[0].params.board, "leads");
  assert.equal(calls[0].params.title, "今天先答这三条");
  assert.equal(calls[0].params.dispatched.length, 1);
});

test("orders are filed one at a time, and a refused one is reported", async () => {
  const calls = [];
  const context = {
    employeeId: "emp-manager",
    employeeName: "线索主管",
    runId: "run-1",
    board: "leads",
    call: async (method, params) => {
      calls.push({ method, params });
      // The host refuses an order the blueprint does not allow, or one over
      // the day's quota. That refusal is the manager's business, not a crash.
      if (params.employeeName === "地推员") throw new Error("员工今天的额度用完了");
      return {};
    },
  };
  const reply = fence(
    JSON.stringify({
      items: [
        { to: "笔杆子", verb: "produce", task: "写一篇" },
        { to: "地推员", verb: "outreach", task: "去 V2EX 留言", deadlineHours: 3 },
      ],
    }),
  );
  const outcome = await fileCards(context, boardStep("dispatch"), reply);
  assert.equal(outcome.artifact, "dispatch");
  assert.equal(outcome.filed, 1);
  assert.match(outcome.reason, /额度/);
  assert.deepEqual(
    calls.map((call) => call.method),
    ["staff.directive.assign", "staff.directive.assign"],
  );
  assert.equal(calls[1].params.deadlineHours, 3);
});

const writerStep = stepOf("produce", { artifact: "content" });

test("a draft that answers a post is filed against that lead, by url", async () => {
  const calls = [];
  const context = {
    employeeId: "emp-writer",
    employeeName: "笔杆子",
    runId: "run-1",
    board: "contents",
    call: async (method, params) => {
      calls.push({ method, params });
      if (method === "staff.leads.list") {
        return { leads: [{ id: "lead-1", url: "https://e.test/q/1" }] };
      }
      return {};
    },
  };
  const reply = fence(
    JSON.stringify({
      items: [
        { title: "答第一条", body: "正文一", leadUrl: "https://e.test/q/1" },
        { title: "答没人归档的那条", body: "正文二", leadUrl: "https://e.test/q/9" },
      ],
    }),
  );
  const outcome = await fileCards(context, writerStep, reply);
  assert.deepEqual(outcome, { artifact: "content", filed: 2 });
  const saves = calls.filter((call) => call.method === "staff.content.save");
  assert.equal(saves.length, 2);
  assert.equal(saves[0].params.leadId, "lead-1");
  // A url nobody filed is left off rather than pointed at the nearest card: an
  // unlinked draft is a gap, a mislinked one is a claim about the wrong person.
  assert.equal("leadId" in saves[1].params, false);
});

test("the lead board is read once, and only when a draft answers somebody", async () => {
  const calls = [];
  const context = {
    employeeId: "emp-writer",
    employeeName: "笔杆子",
    runId: "run-1",
    board: "contents",
    call: async (method, params) => {
      calls.push({ method, params });
      return { leads: [{ id: "lead-1", url: "https://e.test/q/1" }] };
    },
  };
  const reply = fence(
    JSON.stringify({
      items: [
        { title: "答一", body: "正文一", leadUrl: "https://e.test/q/1" },
        { title: "答二", body: "正文二", leadUrl: "https://e.test/q/1" },
      ],
    }),
  );
  await fileCards(context, writerStep, reply);
  assert.equal(calls.filter((call) => call.method === "staff.leads.list").length, 1);

  // Internal copy -- a script for somebody already in the conversation, or a
  // version written for no post in particular -- has no url to join, and the
  // board it would join against is not read at all.
  const plain = [];
  await fileCards(
    {
      ...context,
      call: async (method, params) => {
        plain.push({ method, params });
        return {};
      },
    },
    writerStep,
    fence(JSON.stringify({ items: [{ title: "卡点1｜推进话术", body: "正文" }] })),
  );
  assert.deepEqual(plain.map((call) => call.method), ["staff.content.save"]);
});
