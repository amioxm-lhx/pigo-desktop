/**
 * N0 smoke: the shell's startup contract.
 *
 * A real GUI launch is environment-dependent, so this half pins the wiring that
 * decides whether the app can start at all: which binaries the bundle ships,
 * where boot.rs looks for them, and which channels the setup hook installs.
 * The optional app-boot test exercises the running process when PI_E2E_APP_BIN
 * points at a build.
 */
import { desktopRoot } from "./helpers/sidecar-harness.mjs";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

const tauriRoot = join(desktopRoot, "apps", "desktop-tauri");
const shellRoot = join(tauriRoot, "src-tauri");
const read = (...segments) => readFileSync(join(...segments), "utf8");

test("the bundle ships both backends where boot.rs probes them", () => {
  const config = JSON.parse(read(shellRoot, "tauri.conf.json"));
  const files = config.bundle?.macOS?.files ?? {};

  assert.equal(files["Resources/bin/pi-desktop-host-core"], "../../../target/release/pi-desktop-host-core");
  assert.equal(files["Resources/bin/pi-go-desktop-agent"], "../../../sidecar/build/pi-go-desktop-agent");
  assert.equal(config.bundle?.externalBin, undefined);
  assert.equal(config.bundle?.resources?.["../resources/models.dev/api.json"], "models.dev/api.json");
  // The desktop tree is LGPL-3.0-or-later and the pigo kernel is MIT; a release
  // must carry both texts next to the binaries it covers.
  assert.equal(config.bundle?.resources?.["../../../LICENSE"], "licenses/LGPL-3.0-or-later.txt");
  assert.equal(config.bundle?.resources?.["../../../../LICENSE"], "licenses/MIT.txt");

  const boot = read(shellRoot, "src", "boot.rs");
  // macOS Resources/bin, Windows bin/ next to the exe, then externalBin layout.
  assert.match(boot, /Resources"\)\.join\("bin"\)\.join\(name\)/);
  assert.match(boot, /exe_dir\.join\("bin"\)\.join\(name\)/);
  assert.match(boot, /exe_dir\.join\(name\)/);
  assert.match(boot, /PI_DESKTOP_HOST_BIN/);
  assert.match(boot, /PI_DESKTOP_AGENT_BIN/);
  assert.match(boot, /join\("sidecar"\)/);
});

test("the Windows lane ships both backends under bin/", () => {
  const windows = JSON.parse(read(shellRoot, "tauri.windows.conf.json"));
  const resources = windows.bundle?.resources ?? {};

  assert.equal(resources["../../../target/release/pi-desktop-host-core.exe"], "bin/pi-desktop-host-core.exe");
  assert.equal(resources["../../../sidecar/build/pi-go-desktop-agent.exe"], "bin/pi-go-desktop-agent.exe");
});

test("the macOS lane signs nested backends before bundling", () => {
  const script = read(desktopRoot, "scripts", "build-macos.sh");
  const signAt = script.indexOf("codesign --force --options runtime --timestamp");
  const bundleAt = script.indexOf("Bundling the .app");

  assert.ok(signAt >= 0, "build-macos.sh does not pre-sign the nested backends");
  assert.ok(signAt < bundleAt, "nested binaries must be signed before tauri bundles the app");
  assert.match(script, /Contents\/Resources\/bin/);
  assert.match(script, /stapler validate/);
  assert.match(script, /spctl --assess/);
});

test("the setup hook installs the host/proxy/boot chain before running", () => {
  const main = read(shellRoot, "src", "main.rs");
  for (const call of [
    "events::install_sidecar_proxy",
    "events::dispatch",
    "boot::boot",
    "staff_driver::start",
    "staff_watch::start",
    "router::pi_desktop_invoke",
  ]) {
    assert.ok(main.includes(call), `main.rs does not install ${call}`);
  }
  const boot = read(shellRoot, "src", "boot.rs");
  assert.match(boot, /sidecar\.spawn\(\)/);
  assert.match(boot, /sidecar\.configure/);
  assert.match(boot, /app\.handshake/);
});

test("the host proxy allowlist covers one full turn", () => {
  const proxy = read(shellRoot, "src", "proxy.rs");
  for (const method of [
    "tools.list",
    "tools.execute",
    "session.get",
    "session.appendMessage",
    "session.appendCompaction",
    "plans.enter",
    "plans.submit",
    "plans.abort",
  ]) {
    assert.ok(proxy.includes(`"${method}"`), `host proxy allowlist is missing ${method}`);
  }
});

test("the packaged sidecar name matches the build script", () => {
  const build = read(desktopRoot, "scripts", "build-agent.mjs");
  assert.match(build, /pi-go-desktop-agent/);
  assert.match(build, /join\(sidecarDir, output\)/);
});

test("the e2e entry point exists and is separate from the legacy suite", () => {
  const root = JSON.parse(read(desktopRoot, "package.json"));
  assert.equal(root.scripts["test:e2e"], "node --test apps/desktop-tauri/test/e2e/*.test.mjs");
  assert.ok(root.scripts["package:mac"], "package:mac lane is missing");
  assert.ok(root.scripts["package:win"], "package:win lane is missing");

  const app = JSON.parse(read(tauriRoot, "package.json"));
  assert.equal(app.scripts.test, "node --test test/*.test.mjs");
});
