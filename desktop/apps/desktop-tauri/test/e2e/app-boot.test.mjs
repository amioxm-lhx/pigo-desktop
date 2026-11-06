/**
 * N0 smoke: the real Tauri process boots both backends.
 *
 * Launching a GUI is environment-dependent, so this test only runs when
 * PI_E2E_APP_BIN points at a built pigo-desktop binary. In a capable desktop
 * environment it proves the shell spawns host-core and the Go sidecar, performs
 * the host handshake, configures the sidecar, and starts the resident staff
 * driver — the same chain the renderer relies on for the first prompt.
 *
 * Point it at a packaged app to verify the externalBin layout end to end:
 *
 *   PI_E2E_APP_BIN="<app>/Contents/MacOS/pigo-desktop" PI_E2E_PACKAGED=1 \
 *     pnpm test:e2e
 *
 * Without PI_E2E_PACKAGED the test injects the workspace binaries through
 * PI_DESKTOP_HOST_BIN / PI_DESKTOP_AGENT_BIN, which is the development lane.
 */
import { desktopRoot, ensureSidecarBinary, tempDataDir } from "./helpers/sidecar-harness.mjs";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

const appBinary = process.env.PI_E2E_APP_BIN;
const packaged = process.env.PI_E2E_PACKAGED === "1";
const skipReason = appBinary
  ? false
  : "set PI_E2E_APP_BIN=/path/to/pigo-desktop to run the GUI boot smoke";

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

test("the shell boots host-core, configures the sidecar, and starts the staff driver", { skip: skipReason, timeout: 120000 }, async () => {
  assert.ok(existsSync(appBinary), `PI_E2E_APP_BIN does not exist: ${appBinary}`);

  const env = {
    ...process.env,
    PI_DESKTOP_DATA_DIR: tempDataDir("pigo-app-e2e-"),
    RUST_LOG: "info",
  };

  if (!packaged) {
    const hostBinary =
      process.env.PI_DESKTOP_HOST_BIN ?? join(desktopRoot, "target", "debug", "pi-desktop-host-core");
    assert.ok(existsSync(hostBinary), `host-core binary missing: ${hostBinary}; run cargo build -p host-core`);
    env.PI_DESKTOP_HOST_BIN = hostBinary;
    env.PI_DESKTOP_AGENT_BIN = ensureSidecarBinary();
  } else {
    // Packaged verification must resolve both backends from the bundle itself.
    delete env.PI_DESKTOP_HOST_BIN;
    delete env.PI_DESKTOP_AGENT_BIN;
  }

  const child = spawn(appBinary, [], { env, stdio: ["ignore", "pipe", "pipe"] });

  let output = "";
  const record = (chunk) => {
    output += chunk.toString();
  };
  child.stdout.on("data", record);
  child.stderr.on("data", record);

  const markers = [
    "[host] spawned",
    "[sidecar] spawned",
    "sidecar.configure hostBinary=",
    "ready version=",
    "staff resident driver polling",
    "staff watch resident loop",
  ];

  const deadline = Date.now() + 60000;
  while (Date.now() < deadline) {
    if (markers.every((marker) => output.includes(marker))) break;
    if (child.exitCode !== null) {
      throw new Error(`app exited before boot completed (code ${child.exitCode}):\n${output}`);
    }
    await delay(100);
  }

  const missing = markers.filter((marker) => !output.includes(marker));
  if (missing.length > 0) {
    child.kill("SIGKILL");
    throw new Error(`boot markers missing: ${missing.join(", ")}\n${output}`);
  }

  const stopped = new Promise((resolve) => child.on("exit", resolve));
  child.kill("SIGTERM");
  await Promise.race([stopped, delay(5000)]);
  if (child.exitCode === null) {
    child.kill("SIGKILL");
    await stopped;
  }

  assert.ok(!output.includes("host spawn failed"), output);
  assert.ok(!output.includes("sidecar spawn failed"), output);
});
