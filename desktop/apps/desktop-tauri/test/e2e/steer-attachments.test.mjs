/**
 * N0 matrix: steering and attachments.
 *
 * Steering is only pulled after a turn's tool execution, so the host holds the
 * tool response open until `agent.steer` has been accepted. Attachments are
 * asserted on the provider request: an inlined image becomes an `image_url`
 * block, while a file attachment stays out of the model context.
 */
import {
  SidecarClient,
  TINY_PNG,
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

test("steer reaches the running turn after its tool call", { timeout: 60000 }, async () => {
  let releaseTool;
  const toolGate = new Promise((resolve) => {
    releaseTool = resolve;
  });
  const host = (method, params) => {
    if (method === "tools.execute") {
      return toolGate.then(() => ({ ok: true, content: JSON.stringify("file body") }));
    }
    return defaultHost(method, params);
  };
  const provider = await startFakeProvider((_body, index) =>
    index === 0
      ? { sse: toolCallTurn("Read", { path: "README.md" }) }
      : { sse: textTurn("Done after steer.") },
  );
  const client = await spawnClient({ host });

  try {
    await client.call("sidecar.configure", { dataDir: tempDataDir() });
    await client.call(
      "agent.prompt",
      promptParams({ baseUrl: provider.baseUrl, dataDir: tempDataDir(), content: "read it" }),
    );

    // The tool is in flight; this is the window the renderer steers in.
    await client.waitForHostCall("tools.execute");
    const steer = await client.call("agent.steer", {
      sessionId: "s1",
      expectedTurnId: "t1",
      content: "also check the tests",
      message: {
        id: "steer-1",
        role: "user",
        content: "also check the tests",
        createdAt: "2026-11-05T00:00:00Z",
        steering: true,
      },
    });

    assert.deepEqual(steer, { accepted: true, turnId: "t1", messageId: "steer-1" });

    const row = client.events.find(
      (envelope) => envelope.event?.type === "message_end" && envelope.event.message?.steering === true,
    );
    assert.ok(row, "no steering message_end was emitted");
    assert.equal(row.event.message.content, "also check the tests");
    assert.equal(row.event.message.id, "steer-1");

    releaseTool();
    await client.waitForEvent("agent_end");

    // The next model request carries both the tool result and the steer row.
    assert.equal(provider.requests.length, 2);
    const second = provider.requests[1].body.messages;
    const toolMessage = second.find((message) => message.role === "tool");
    assert.ok(toolMessage, "second request has no tool message");
    const steered = second.find(
      (message) => message.role === "user" && messageText(message).includes("also check the tests"),
    );
    assert.ok(steered, `steer text missing from second request: ${JSON.stringify(second)}`);
  } finally {
    await provider.close();
  }
});

test("steer without an active turn reports TURN_NOT_FOUND", { timeout: 60000 }, async () => {
  const client = await spawnClient();
  await client.call("sidecar.configure", { dataDir: tempDataDir() });

  await assert.rejects(
    client.call("agent.steer", {
      sessionId: "missing",
      expectedTurnId: "t1",
      content: "hello",
      message: { id: "steer-1", role: "user", content: "hello" },
    }),
    /No active turn to steer/,
  );
});

test("an inlined image reaches the provider and file attachments do not", { timeout: 60000 }, async () => {
  const provider = await startFakeProvider(() => ({ sse: textTurn("Seen.") }));
  const client = await spawnClient();

  try {
    await client.call("sidecar.configure", { dataDir: tempDataDir() });
    await client.call(
      "agent.prompt",
      promptParams({
        baseUrl: provider.baseUrl,
        dataDir: tempDataDir(),
        content: "describe the image",
        supportsImages: true,
        attachments: [
          {
            path: "/tmp/shot.png",
            name: "shot.png",
            kind: "image",
            mimeType: "image/png",
            data: TINY_PNG,
          },
          {
            path: "/tmp/notes.md",
            name: "notes.md",
            kind: "file",
            mimeType: "text/markdown",
            size: 12,
          },
        ],
      }),
    );
    await client.waitForEvent("agent_end");

    assert.equal(provider.requests.length, 1);
    const user = provider.requests[0].body.messages.find((message) => message.role === "user");
    assert.ok(user, "provider request has no user message");
    assert.ok(Array.isArray(user.content), `expected multimodal content, got ${JSON.stringify(user.content)}`);

    const images = user.content.filter((part) => part.type === "image_url");
    assert.equal(images.length, 1);
    assert.equal(images[0].image_url.url, `data:image/png;base64,${TINY_PNG}`);

    const serialized = JSON.stringify(user.content);
    assert.ok(!serialized.includes("notes.md"), "file attachment leaked into the model context");
  } finally {
    await provider.close();
  }
});
