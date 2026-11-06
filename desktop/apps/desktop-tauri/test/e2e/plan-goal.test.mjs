/**
 * N0 matrix: plan and goal contract modes.
 *
 * The shell owns the approval UI and the durable proposal; the sidecar owns the
 * mode tools and the planning_state event. These tests pin that split: submit
 * reaches the host, the awaiting_approval state is published, and an approved
 * execution runs as an internal Agent turn.
 */
import {
  SidecarClient,
  defaultHost,
  ensureSidecarBinary,
  messageText,
  promptParams,
  startFakeProvider,
  tempDataDir,
  textTurn,
  toolCallTurn,
} from "./helpers/sidecar-harness.mjs";
import assert from "node:assert/strict";
import test, { after, before } from "node:test";

let binary;
const running = [];

before(() => {
  binary = ensureSidecarBinary();
});

after(async () => {
  await Promise.all(running.map((client) => client.close()));
});

async function spawnClient(options) {
  const client = new SidecarClient(binary, options);
  running.push(client);
  return client;
}

const PLAN_MARKDOWN = "# Plan\n\n1. Read the file.\n2. Report back.";

function pendingProposal(kind, id) {
  return {
    id,
    sessionId: "s1",
    turnId: "t1",
    toolCallId: "call_1",
    kind,
    title: `${kind} title`,
    markdown: PLAN_MARKDOWN,
    question: "Approve?",
    artifact: {
      relativePath: `.pi-desktop/plans/${id}.md`,
      sha256: `sha256-${id}`,
      sizeBytes: PLAN_MARKDOWN.length,
    },
    version: 1,
    status: "pending",
    createdAt: "2026-11-05T00:00:00Z",
    updatedAt: "2026-11-05T00:00:00Z",
    plan: PLAN_MARKDOWN,
  };
}

function planningEvents(client) {
  return client.events
    .filter((envelope) => envelope.event?.type === "planning_state")
    .map((envelope) => envelope.event);
}

test("plan mode submits a contract, then executes the approved plan", { timeout: 60000 }, async () => {
  const proposal = pendingProposal("plan", "p1");
  const host = (method, params) => {
    if (method === "plans.submit") return { status: "pending", proposal };
    return defaultHost(method, params);
  };
  const provider = await startFakeProvider((_body, index) =>
    index === 0
      ? {
          sse: toolCallTurn("SubmitPlan", {
            title: proposal.title,
            markdown: proposal.markdown,
            question: proposal.question,
          }),
        }
      : { sse: textTurn("Executed the approved plan.") },
  );
  const client = await spawnClient({ host });

  try {
    await client.call("sidecar.configure", { dataDir: tempDataDir() });
    await client.call(
      "agent.prompt",
      promptParams({ baseUrl: provider.baseUrl, dataDir: tempDataDir(), mode: "plan", content: "plan it" }),
    );
    await client.waitForEvent("agent_end");

    const states = planningEvents(client);
    assert.equal(states[0]?.state, "planning");
    assert.equal(states[0]?.kind, "plan");
    assert.equal(states.at(-1)?.state, "awaiting_approval");
    assert.equal(states.at(-1)?.proposalId, "p1");
    assert.equal(states.at(-1)?.markdown, PLAN_MARKDOWN);

    const submit = client.hostCalls.find((call) => call.method === "plans.submit");
    assert.ok(submit, "plans.submit was not called");
    assert.equal(submit.params.kind, "plan");
    assert.equal(submit.params.title, proposal.title);

    const status = await client.call("agent.getStatus", { sessionId: "s1" });
    assert.equal(status.status.planningState, "awaiting_approval");
    assert.equal(status.status.pendingPlanId, "p1");

    // Submission terminates the contract turn: the model is not called again.
    assert.equal(provider.requests.length, 1);
    assert.equal(client.eventTypes().filter((type) => type === "turn_start").length, 1);

    const mark = client.events.length;
    const execution = {
      id: "e1",
      proposalId: "p1",
      sessionId: "s1",
      kind: "plan",
      plan: PLAN_MARKDOWN,
      title: proposal.title,
      question: proposal.question,
      artifact: proposal.artifact,
      targetPermissionMode: "agent",
      state: "approved",
    };
    await client.call("agent.executeApprovedPlan", {
      ...promptParams({ baseUrl: provider.baseUrl, dataDir: tempDataDir() }),
      execution,
    });
    await client.waitForEvent("agent_end", 20000, mark);

    assert.equal(provider.requests.length, 2);
    const executionPrompt = provider.requests[1].body.messages
      .filter((message) => message.role === "user")
      .map((message) => messageText(message))
      .join("\n");
    assert.ok(executionPrompt.includes("Execute the approved plan now."), executionPrompt);
    assert.ok(executionPrompt.includes("<approved-plan-markdown>"), executionPrompt);
    assert.ok(executionPrompt.includes(PLAN_MARKDOWN), executionPrompt);

    // An approved execution is internal: no user transcript row is streamed.
    const executionRows = client.events
      .slice(mark)
      .filter((envelope) => envelope.event?.type === "message_end")
      .map((envelope) => envelope.event.message);
    assert.ok(executionRows.length > 0);
    assert.ok(executionRows.every((message) => message.role === "assistant"));

    const after = await client.call("agent.getStatus", { sessionId: "s1" });
    assert.equal(after.status.planningState, "inactive");
    assert.ok(!after.status.pendingPlanId);
  } finally {
    await provider.close();
  }
});

test("goal mode publishes a goal contract awaiting approval", { timeout: 60000 }, async () => {
  const proposal = pendingProposal("goal", "g1");
  const host = (method, params) => {
    if (method === "plans.submit") return { status: "pending", proposal };
    return defaultHost(method, params);
  };
  const provider = await startFakeProvider(() => ({
    sse: toolCallTurn("SubmitGoal", {
      title: proposal.title,
      markdown: proposal.markdown,
      question: proposal.question,
    }),
  }));
  const client = await spawnClient({ host });

  try {
    await client.call("sidecar.configure", { dataDir: tempDataDir() });
    await client.call(
      "agent.prompt",
      promptParams({ baseUrl: provider.baseUrl, dataDir: tempDataDir(), mode: "goal", content: "goal it" }),
    );
    await client.waitForEvent("agent_end");

    const states = planningEvents(client);
    assert.equal(states[0]?.state, "planning");
    assert.equal(states[0]?.kind, "goal");
    assert.equal(states.at(-1)?.state, "awaiting_approval");
    assert.equal(states.at(-1)?.kind, "goal");
    assert.equal(states.at(-1)?.proposalId, "g1");

    const submit = client.hostCalls.find((call) => call.method === "plans.submit");
    assert.ok(submit);
    assert.equal(submit.params.kind, "goal");

    const status = await client.call("agent.getStatus", { sessionId: "s1" });
    assert.equal(status.status.planningState, "awaiting_approval");
    assert.equal(status.status.pendingPlanId, "g1");
  } finally {
    await provider.close();
  }
});

test("Write is blocked after a turn switches into plan mode", { timeout: 60000 }, async () => {
  const provider = await startFakeProvider((_body, index) => {
    if (index === 0) return { sse: toolCallTurn("EnterPlanMode", {}) };
    if (index === 1) return { sse: toolCallTurn("Write", { path: "README.md", content: "overwritten" }) };
    return { sse: textTurn("Blocked, as expected.") };
  });
  const client = await spawnClient();

  try {
    await client.call("sidecar.configure", { dataDir: tempDataDir() });
    await client.call(
      "agent.prompt",
      promptParams({ baseUrl: provider.baseUrl, dataDir: tempDataDir(), content: "plan, then write" }),
    );
    await client.waitForEvent("agent_end");

    assert.ok(
      !client.hostCalls.some((call) => call.method === "tools.execute"),
      "plan mode forwarded Write to the host",
    );

    const blocked = client.events.find(
      (envelope) =>
        envelope.event?.type === "tool_end" &&
        JSON.stringify(envelope.event.result ?? "").includes("not available in Plan mode"),
    );
    assert.ok(blocked, `no blocked Write result; events: ${JSON.stringify(client.events)}`);
    assert.equal(blocked.event.isError, true);
  } finally {
    await provider.close();
  }
});

test("EnterPlanMode switches an Agent turn into plan mode", { timeout: 60000 }, async () => {
  const provider = await startFakeProvider((_body, index) =>
    index === 0 ? { sse: toolCallTurn("EnterPlanMode", {}) } : { sse: textTurn("Plan mode ready.") },
  );
  const client = await spawnClient();

  try {
    await client.call("sidecar.configure", { dataDir: tempDataDir() });
    await client.call(
      "agent.prompt",
      promptParams({ baseUrl: provider.baseUrl, dataDir: tempDataDir(), content: "switch to plan" }),
    );
    await client.waitForEvent("agent_end");

    const enter = client.hostCalls.find((call) => call.method === "plans.enter");
    assert.ok(enter, "plans.enter was not called");
    assert.equal(enter.params.kind, "plan");
    assert.equal(enter.params.sessionId, "s1");

    const states = planningEvents(client);
    assert.ok(states.some((state) => state.state === "planning" && state.kind === "plan"));
    const status = await client.call("agent.getStatus", { sessionId: "s1" });
    assert.equal(status.status.planningState, "planning");

    // The turn continues after the mode switch, so the model ran twice.
    assert.equal(provider.requests.length, 2);
  } finally {
    await provider.close();
  }
});
