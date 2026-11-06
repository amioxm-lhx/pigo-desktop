/**
 * E2E matrix (track E): asktool.
 *
 * The sidecar owns the pending question set: it emits `asktool_request`, pauses
 * the turn, and resumes with the renderer's answer through `asktool.resolve`.
 * The fake host is not involved; only the provider and the protocol are.
 */
import {
  SidecarClient,
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

const QUESTIONS = [
  { question: "Which site?", options: ["V2EX", "Juejin"], multiSelect: false },
  { question: "Which extras?", options: ["notes", "links"], multiSelect: true },
];

test("asktool pauses the turn and resumes with the selected answers", { timeout: 60000 }, async () => {
  const provider = await startFakeProvider((_body, index) =>
    index === 0
      ? { sse: toolCallTurn("asktool", { questions: QUESTIONS }) }
      : { sse: textTurn("All set.") },
  );
  const client = await spawnClient();

  try {
    await client.call("sidecar.configure", { dataDir: tempDataDir() });
    await client.call(
      "agent.prompt",
      promptParams({ baseUrl: provider.baseUrl, dataDir: tempDataDir(), content: "ask me" }),
    );

    const request = await client.waitForEvent("asktool_request");
    assert.equal(request.event.request.sessionId, "s1");
    assert.equal(request.event.request.toolCallId, "call_1");
    assert.equal(request.event.request.questions.length, 2);
    assert.equal(request.event.request.questions[0].question, QUESTIONS[0].question);
    assert.deepEqual(request.event.request.questions[0].options, QUESTIONS[0].options);
    assert.equal(request.event.request.questions[1].multiSelect, true);

    const resolved = await client.call("asktool.resolve", {
      sessionId: "s1",
      requestId: request.event.request.requestId,
      answers: [["V2EX"], ["notes", "links"]],
    });
    assert.equal(resolved.ok, true);

    await client.waitForEvent("agent_end");

    const toolEnd = client.events.find((envelope) => envelope.event?.type === "tool_end");
    assert.ok(toolEnd, "no tool_end after asktool.resolve");
    assert.ok(!toolEnd.event.isError);
    assert.deepEqual(toolEnd.event.result?.details?.answers, [["V2EX"], ["notes", "links"]]);

    assert.equal(provider.requests.length, 2);
    const toolMessage = provider.requests[1].body.messages.find((message) => message.role === "tool");
    assert.ok(toolMessage, "second provider request has no tool message");
    const output = messageText(toolMessage);
    assert.ok(output.includes("Which site?：V2EX"), output);
    assert.ok(output.includes("Which extras?：notes、links"), output);
  } finally {
    await provider.close();
  }
});

test("an answer count mismatch keeps the request pending", { timeout: 60000 }, async () => {
  const provider = await startFakeProvider((_body, index) =>
    index === 0
      ? { sse: toolCallTurn("asktool", { questions: QUESTIONS.slice(0, 1) }) }
      : { sse: textTurn("Done.") },
  );
  const client = await spawnClient();

  try {
    await client.call("sidecar.configure", { dataDir: tempDataDir() });
    await client.call(
      "agent.prompt",
      promptParams({ baseUrl: provider.baseUrl, dataDir: tempDataDir(), content: "ask once" }),
    );

    const request = await client.waitForEvent("asktool_request");
    await assert.rejects(
      client.call("asktool.resolve", {
        sessionId: "s1",
        requestId: request.event.request.requestId,
        answers: [["V2EX"], ["notes"]],
      }),
      /answer count does not match questions/,
    );

    // The pending request survived the rejected reply.
    const resolved = await client.call("asktool.resolve", {
      sessionId: "s1",
      requestId: request.event.request.requestId,
      answers: [["Juejin"]],
    });
    assert.equal(resolved.ok, true);
    await client.waitForEvent("agent_end");
    assert.equal(provider.requests.length, 2);
  } finally {
    await provider.close();
  }
});

test("resolving an unknown asktool request is refused", { timeout: 60000 }, async () => {
  const client = await spawnClient();
  await client.call("sidecar.configure", { dataDir: tempDataDir() });

  await assert.rejects(
    client.call("asktool.resolve", {
      sessionId: "missing",
      requestId: "nope",
      answers: [[]],
    }),
    /runtime not found for asktool request/,
  );
});
