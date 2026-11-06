#!/usr/bin/env node
/**
 * Build the pi-go agent sidecar for the current platform.
 *
 * Electron main resolves the packaged binary at
 * <resources>/bin/pi-go-desktop-agent[.exe] (see
 * apps/desktop/electron/main/agent-sidecar.ts), which electron-builder fills
 * from sidecar/build through extraResources. `pack` and `dist` therefore run
 * this before electron-builder, so a released app ships the Go sidecar instead
 * of silently falling back to the Node one.
 *
 * Usage: node scripts/build-agent.mjs
 * Env:   PI_GO_AGENT_TARGET (optional GOOS/GOARCH pair, e.g. darwin/arm64)
 */
import { execFileSync } from "node:child_process";
import { mkdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const sidecarDir = join(root, "sidecar");
const executable = process.platform === "win32" ? ".exe" : "";
const outputName = `pi-go-desktop-agent${executable}`;
const output = join("build", outputName);
const binary = join(sidecarDir, output);

mkdirSync(join(sidecarDir, "build"), { recursive: true });

const environment = { ...process.env };
const target = environment.PI_GO_AGENT_TARGET?.trim();
if (target) {
  const [goos, goarch] = target.split("/");
  if (!goos || !goarch) {
    console.error(`PI_GO_AGENT_TARGET must be GOOS/GOARCH, got ${target}`);
    process.exit(1);
  }
  environment.GOOS = goos;
  environment.GOARCH = goarch;
  // A cross build cannot overwrite the host binary in place: electron only
  // needs the host's own file, so the cross-built name is what ships.
  environment.CGO_ENABLED = "0";
}

// The sidecar is a pure-Go binary so a build needs nothing but the toolchain.
execFileSync("go", ["build", "-o", output, "."], {
  cwd: sidecarDir,
  stdio: "inherit",
  env: environment,
});

const { size } = statSync(binary);
if (size === 0) {
  console.error(`agent sidecar build produced an empty binary: ${binary}`);
  process.exit(1);
}
console.log(`built ${binary} (${(size / 1024 / 1024).toFixed(1)} MB)`);
