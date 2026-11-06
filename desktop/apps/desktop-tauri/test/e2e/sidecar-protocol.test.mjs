/**
 * N0 smoke: the sidecar speaks the shell's NDJSON protocol end to end.
 *
 * These tests spawn the real binary, so they cover process startup, the
 * configure/health handshake, streaming events, and the host.proxy tool path
 * without host-core or a GUI. That is the regression net the rest of the
 * integration work builds on.
 */
import {
  SidecarClient,
  defaultHost,
  ensureSidecarBinary,
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

test("sidecar answers the shell handshake", { timeout: 60000 }, async () => {
  const client = await spawnClient();
  const configure = await client.call("sidecar.configure", { dataDir: tempDataDir() });
  const health = await client.call("sidecar.health", {});

  assert.equal(configure.ok, true);
  assert.equal(configure.mode, "host-proxy");
  assert.equal(health.ok, true);
  assert.equal(typeof health.version, "string");
  assert.equal(health.runtimes, 0);
});

test("one text turn streams the full event lifecycle", { timeout: 60000 }, async () => {
  const provider = await startFakeProvider(() => ({ sse: textTurn("Hello world") }));
  const client = await spawnClient();

  try {
    await client.call("sidecar.configure", { dataDir: tempDataDir() });
    const accepted = await client.call(
      "agent.prompt",
      promptParams({ baseUrl: provider.baseUrl, dataDir: tempDataDir() }),
    );
    assert.equal(accepted.accepted, true);
    assert.equal(accepted.turnId, "t1");

    const end = await client.waitForEvent("agent_end");
    const types = client.eventTypes();

    for (const type of ["agent_start", "turn_start", "message_start", "message_update", "message_end", "turn_end"]) {
      assert.ok(types.includes(type), `missing ${type}; saw [${types.join(", ")}]`);
    }
    assert.ok(types.indexOf("agent_start") < types.indexOf("turn_start"));
    assert.ok(types.indexOf("message_end") < types.indexOf("agent_end"));

    const messageEnd = client.events.find((envelope) => envelope.event?.type === "message_end");
    assert.equal(messageEnd.event.message.role, "assistant");
    assert.equal(messageEnd.event.message.content, "Hello world");
    assert.equal(messageEnd.event.message.status, "complete");
    assert.deepEqual(end.event.messageIds, [messageEnd.event.message.id]);

    const deltas = client.events
      .filter((envelope) => envelope.event?.type === "message_update")
      .map((envelope) => envelope.event.deltaText ?? "");
    assert.ok(deltas.join("").includes("Hello world"), `deltas: ${JSON.stringify(deltas)}`);

    const status = await client.call("agent.getStatus", { sessionId: "s1" });
    assert.equal(status.status.isRunning, false);
    assert.equal(status.status.sessionId, "s1");

    assert.equal(provider.requests.length, 1);
    assert.ok(provider.requests[0].path.endsWith("/chat/completions"));
  } finally {
    await provider.close();
  }
});

test("a host tool call round-trips through host.proxy", { timeout: 60000 }, async () => {
  let toolOutput;
  const host = (method, params) => {
    if (method === "tools.execute" && params.toolName === "Read") {
      toolOutput = params;
      return { ok: true, content: JSON.stringify("README body") };
    }
    return defaultHost(method, params);
  };
  const provider = await startFakeProvider((_body, index) =>
    index === 0
      ? { sse: toolCallTurn("Read", { file_path: "README.md" }) }
      : { sse: textTurn("Done.") },
  );
  const client = await spawnClient({ host });

  try {
    await client.call("sidecar.configure", { dataDir: tempDataDir() });
    await client.call(
      "agent.prompt",
      promptParams({ baseUrl: provider.baseUrl, dataDir: tempDataDir(), content: "read it" }),
    );
    await client.waitForEvent("agent_end");

    const types = client.eventTypes();
    assert.ok(types.includes("tool_start"), `saw [${types.join(", ")}]`);
    assert.ok(types.includes("tool_end"));

    const start = client.events.find((envelope) => envelope.event?.type === "tool_start");
    assert.equal(start.event.toolName, "Read");
    assert.equal(start.event.toolCallId, "call_1");

    // The shell contract uses `path`; the model's `file_path` alias is
    // normalised before tools.execute.
    assert.ok(toolOutput, "tools.execute was not called");
    assert.deepEqual(toolOutput.args, { path: "README.md" });
    assert.equal(toolOutput.sessionId, "s1");
    assert.equal(toolOutput.turnId, "t1");

    // The second model request carries the tool result, so the loop closes.
    assert.equal(provider.requests.length, 2);
    const toolMessage = provider.requests[1].body.messages.find((message) => message.role === "tool");
    assert.ok(toolMessage, "second request has no tool message");
    assert.equal(toolMessage.tool_call_id, "call_1");
  } finally {
    await provider.close();
  }
});

test("stop and dispose answer the shell's lifecycle contract", { timeout: 60000 }, async () => {
  const client = await spawnClient();
  await client.call("sidecar.configure", { dataDir: tempDataDir() });

  // No runtime yet: stop must say nothing was requested instead of claiming it
  // stopped an agent.
  const stop = await client.call("agent.stop", { sessionId: "missing" });
  assert.deepEqual(stop, { requested: false });

  const dispose = await client.call("agent.disposeSession", { sessionId: "missing" });
  assert.equal(dispose.ok, true);

  const status = await client.call("agent.getStatus", { sessionId: "missing" });
  assert.equal(status.status.sessionId, "missing");
  assert.equal(status.status.isRunning, false);
});
