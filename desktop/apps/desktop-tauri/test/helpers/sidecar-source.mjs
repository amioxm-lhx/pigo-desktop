/**
 * Reads the Go agent sidecar's sources for the desktop source-contract tests.
 *
 * Until M4 these tests read
 * `packages/agent-runtime/src/{runtime,sidecar,subagent}.ts`. The agent process
 * is Go now (ADR 0251), so the same contracts are asserted against the
 * implementation that actually ships. Where the port has not carried a Node
 * behaviour across yet, the test states that explicitly instead of asserting
 * against a file that no longer exists.
 *
 * Test files are excluded: a contract is only worth pinning in the shipped
 * implementation, and `_test.go` files carry their own expectations.
 */
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const sidecarRoot = fileURLToPath(new URL("../../../../sidecar", import.meta.url));

function readGoDir(...segments) {
  return readdirSync(join(sidecarRoot, ...segments))
    .filter((name) => name.endsWith(".go") && !name.endsWith("_test.go"))
    .sort()
    .map((name) => readFileSync(join(sidecarRoot, ...segments, name), "utf8"))
    .join("\n");
}

function readGoFile(...segments) {
  return readFileSync(join(sidecarRoot, ...segments), "utf8");
}

/** The shipped sidecar: transport, protocol, host proxy, agent loop, server. */
export function readSidecarSource() {
  return [
    readGoFile("main.go"),
    readGoDir("internal", "jsonrpc"),
    readGoDir("internal", "protocol"),
    readGoDir("internal", "hostproxy"),
    readGoDir("internal", "runtime"),
    readGoDir("internal", "server"),
  ].join("\n");
}

/** The agent loop alone: prompt, turns, tools, compaction, subagents. */
export function readSidecarRuntimeSource() {
  return readGoDir("internal", "runtime");
}

/** One named Go file from the agent loop, for a narrowly-scoped assertion. */
export function readSidecarRuntimeFile(name) {
  return readGoFile("internal", "runtime", name);
}

/** The sidecar transport and dispatcher, which replaced `sidecar.ts`. */
export function readSidecarServerSource() {
  return [readGoFile("main.go"), readGoDir("internal", "server")].join("\n");
}

/** One named Go file from another sidecar package, for a narrow assertion. */
export function readSidecarFile(...segments) {
  return readGoFile(...segments);
}
