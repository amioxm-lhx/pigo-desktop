/**
 * Black-box harness for the Go agent sidecar's NDJSON JSON-RPC protocol.
 *
 * The sidecar ships as a process, so a protocol smoke has to own both sides of
 * the stream: a fake shell that answers `host.proxy` and a fake provider that
 * streams OpenAI Chat Completions chunks. That keeps this suite independent of
 * host-core, Tauri, and any network account.
 */
import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdtempSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const desktopRoot = fileURLToPath(new URL("../../../../..", import.meta.url));
const binaryName = process.platform === "win32" ? "pi-go-desktop-agent.exe" : "pi-go-desktop-agent";

export { desktopRoot };

/** Build (or locate) the sidecar binary the shell would spawn. */
export function ensureSidecarBinary() {
  const override = process.env.PI_DESKTOP_AGENT_BIN;
  if (override && existsSync(override)) return override;

  const binary = join(desktopRoot, "sidecar", "build", binaryName);
  if (process.env.PI_E2E_SKIP_BUILD !== "1") {
    execFileSync("node", [join(desktopRoot, "scripts", "build-agent.mjs")], {
      cwd: desktopRoot,
      stdio: "pipe",
    });
  }
  if (!existsSync(binary)) {
    throw new Error(`sidecar binary missing: ${binary}; run node scripts/build-agent.mjs`);
  }
  return binary;
}

export function tempDataDir(prefix = "pigo-e2e-") {
  return mkdtempSync(join(tmpdir(), prefix));
}

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function readLines(stream, onLine) {
  let buffer = "";
  stream.setEncoding("utf8");
  stream.on("data", (chunk) => {
    buffer += chunk;
    let index;
    while ((index = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, index).trim();
      buffer = buffer.slice(index + 1);
      if (line) onLine(line);
    }
  });
}

const READ_TOOL = {
  name: "Read",
  description: "Read one file",
  risk: "low",
  parameters: { type: "object", properties: { path: { type: "string" } } },
};
const BASH_TOOL = {
  name: "Bash",
  description: "Run one command",
  risk: "medium",
  parameters: { type: "object", properties: { command: { type: "string" } } },
};
const WRITE_TOOL = {
  name: "Write",
  description: "Write one file",
  risk: "medium",
  parameters: { type: "object", properties: { path: { type: "string" }, content: { type: "string" } } },
};
const EDIT_TOOL = {
  name: "Edit",
  description: "Edit one file",
  risk: "medium",
  parameters: { type: "object", properties: { path: { type: "string" } } },
};

/** Host side of the protocol: only the methods a single turn needs. */
export function defaultHost(method) {
  switch (method) {
    case "tools.list":
      return { tools: [READ_TOOL, BASH_TOOL, WRITE_TOOL, EDIT_TOOL] };
    case "session.get":
      return { session: { messages: [], messageCount: 0 } };
    case "tools.execute":
      return { ok: true, content: JSON.stringify("fake tool output") };
    case "plans.abort":
    case "session.appendCompaction":
      return { ok: true };
    default:
      return { ok: true };
  }
}

/** One sidecar process plus the JSON-RPC client used to drive it. */
export class SidecarClient {
  constructor(binary, { host = defaultHost } = {}) {
    this.host = host;
    this.events = [];
    this.hostCalls = [];
    this.stderr = "";
    this.pending = new Map();
    this.nextId = 1;

    this.child = spawn(binary, [], { stdio: ["pipe", "pipe", "pipe"] });
    readLines(this.child.stdout, (line) => this.#handleLine(line));
    this.child.stderr.setEncoding("utf8");
    this.child.stderr.on("data", (chunk) => {
      this.stderr += chunk;
    });
    this.exited = new Promise((resolve) => this.child.on("exit", resolve));
  }

  #handleLine(line) {
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      return;
    }
    if (message.method === "agent.event") {
      this.events.push(message.params);
      return;
    }
    if (message.method === "host.proxy") {
      void this.#answerHost(message);
      return;
    }
    if (message.id === undefined) return;
    const waiter = this.pending.get(message.id);
    if (!waiter) return;
    this.pending.delete(message.id);
    if (message.error) waiter.reject(new Error(message.error.message ?? "JSON-RPC error"));
    else waiter.resolve(message.result);
  }

  async #answerHost(message) {
    const { method, params } = message.params ?? {};
    this.hostCalls.push({ method, params });
    let result;
    try {
      result = await this.host(method, params, this);
    } catch (error) {
      this.#write({
        jsonrpc: "2.0",
        id: message.id,
        error: { code: -32000, message: String(error?.message ?? error) },
      });
      return;
    }
    this.#write({ jsonrpc: "2.0", id: message.id, result: result ?? { ok: true } });
  }

  #write(message) {
    if (!this.child.stdin.destroyed) this.child.stdin.write(`${JSON.stringify(message)}\n`);
  }

  call(method, params, timeoutMs = 15000) {
    const id = String(this.nextId++);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`JSON-RPC timeout: ${method}`));
      }, timeoutMs);
      this.pending.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          resolve(value);
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        },
      });
      this.#write({ jsonrpc: "2.0", id, method, params });
    });
  }

  async waitForEvent(type, timeoutMs = 20000, fromIndex = 0) {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
      const found = this.events.slice(fromIndex).find((envelope) => envelope.event?.type === type);
      if (found) return found;
      await delay(25);
    }
    const seen = this.events.slice(fromIndex).map((envelope) => envelope.event?.type).join(", ");
    throw new Error(`timed out waiting for ${type}; saw [${seen}]; stderr=${this.stderr}`);
  }

  async waitForHostCall(method, timeoutMs = 20000) {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
      const found = this.hostCalls.find((call) => call.method === method);
      if (found) return found;
      await delay(25);
    }
    const seen = this.hostCalls.map((call) => call.method).join(", ");
    throw new Error(`timed out waiting for host call ${method}; saw [${seen}]; stderr=${this.stderr}`);
  }

  eventTypes() {
    return this.events.map((envelope) => envelope.event?.type);
  }

  async close() {
    if (this.child.exitCode !== null) return;
    this.child.stdin.end();
    const closed = await Promise.race([this.exited.then(() => true), delay(2000).then(() => false)]);
    if (!closed) {
      this.child.kill("SIGKILL");
      await this.exited;
    }
  }
}

/** A local OpenAI-compatible SSE endpoint; `responder` returns the raw body. */
export async function startFakeProvider(responder) {
  const requests = [];
  const server = createServer((request, response) => {
    let body = "";
    request.setEncoding("utf8");
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", async () => {
      let parsed = {};
      try {
        parsed = JSON.parse(body);
      } catch {
        parsed = { raw: body };
      }
      requests.push({ path: request.url, body: parsed });
      let reply;
      try {
        reply = await responder(parsed, requests.length - 1);
      } catch (error) {
        response.writeHead(500, { "content-type": "text/plain" });
        response.end(String(error?.message ?? error));
        return;
      }
      if (reply.status && reply.status !== 200) {
        response.writeHead(reply.status, { "content-type": "application/json" });
        response.end(reply.body ?? "{}");
        return;
      }
      response.writeHead(200, {
        "content-type": "text/event-stream",
        "cache-control": "no-cache",
        connection: "keep-alive",
      });
      response.end(reply.sse ?? reply);
    });
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  return {
    baseUrl: `http://127.0.0.1:${port}/v1`,
    requests,
    close: () => new Promise((resolve) => server.close(resolve)),
  };
}

export function sse(items) {
  return items
    .map((item) => `data: ${typeof item === "string" ? item : JSON.stringify(item)}\n\n`)
    .join("");
}

export function textTurn(text) {
  return sse([
    { id: "c1", model: "fake", choices: [{ delta: { role: "assistant" } }] },
    { id: "c1", model: "fake", choices: [{ delta: { content: text } }] },
    { id: "c1", model: "fake", choices: [{ delta: {}, finish_reason: "stop" }] },
    { choices: [], usage: { prompt_tokens: 10, completion_tokens: 2 } },
    "[DONE]",
  ]);
}

export function toolCallTurn(name, args) {
  return sse([
    { id: "c1", model: "fake", choices: [{ delta: { role: "assistant" } }] },
    {
      id: "c1",
      model: "fake",
      choices: [
        {
          delta: {
            tool_calls: [
              {
                index: 0,
                id: "call_1",
                type: "function",
                function: { name, arguments: JSON.stringify(args) },
              },
            ],
          },
        },
      ],
    },
    { id: "c1", model: "fake", choices: [{ delta: {}, finish_reason: "tool_calls" }] },
    { choices: [], usage: { prompt_tokens: 11, completion_tokens: 8 } },
    "[DONE]",
  ]);
}

/** The prompt binding the shell resolves before agent.prompt. */
export function promptParams({
  baseUrl,
  dataDir,
  sessionId = "s1",
  turnId = "t1",
  content = "hi",
  mode = "agent",
  attachments = [],
  supportsImages = false,
}) {
  return {
    sessionId,
    mode,
    turnId,
    thinkingLevel: "off",
    provider: {
      id: "fake",
      name: "Fake",
      baseUrl,
      modelId: "fake-model",
      apiKey: "test-key",
      apiStyle: "openai",
      modelConfig: {
        contextWindow: 128000,
        maxTokens: 1024,
        toolCall: true,
        input: supportsImages ? ["text", "image"] : ["text"],
      },
    },
    commandShell: { id: "bash", dialect: "posix", available: true },
    content,
    userMessageId: "u1",
    attachments,
    projectPath: dataDir,
    scratchDir: join(dataDir, "scratch"),
    projectMemory: "",
    projectInstructions: [],
  };
}

/** One 1x1 PNG; enough to prove the image block reached the provider. */
export const TINY_PNG =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

/** Flatten an OpenAI message's string-or-array content into text parts. */
export function messageText(message) {
  if (typeof message?.content === "string") return message.content;
  if (!Array.isArray(message?.content)) return "";
  return message.content
    .filter((part) => part?.type === "text")
    .map((part) => part.text)
    .join("\n");
}

