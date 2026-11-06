/**
 * Staffing a fresh console, exercised without Electron.
 *
 * host-core decides whether a blueprint installs. This file covers the two
 * things the host must never be asked to decide: where the first key comes
 * from, and whether a build can carry one. The key is read from an injected
 * environment and an injected data directory, so the rules are asserted rather
 * than assumed -- and the log is asserted too, because a key that reaches a log
 * line has leaked just as surely as one that reaches a reply.
 */
import assert from "node:assert/strict";
import test from "node:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { register } from "node:module";

register(new URL("./helpers/ts-import-hooks.mjs", import.meta.url));
const { STAFF_BOOTSTRAP_FILE, ensureStaffPresets, readStaffBootstrap } = await import(
  "../electron/main/runtime/staff-bootstrap.ts",
);

const KEY = "sk-bootstrap-test-key";

function dataDirWith(contents) {
  const dir = mkdtempSync(join(tmpdir(), "pi-staff-bootstrap-"));
  if (contents !== undefined) {
    writeFileSync(
      join(dir, STAFF_BOOTSTRAP_FILE),
      typeof contents === "string" ? contents : JSON.stringify(contents),
      "utf8",
    );
  }
  return dir;
}

test("no key anywhere leaves the console uncredentialed", () => {
  assert.equal(readStaffBootstrap(dataDirWith(), {}), null);
  assert.equal(readStaffBootstrap(dataDirWith(), { PI_STAFF_BOOTSTRAP_API_KEY: "   " }), null);
});

test("a key file carries the endpoint and model with it", () => {
  const dir = dataDirWith({
    apiKey: KEY,
    baseUrl: "https://api.deepseek.com",
    modelId: "deepseek-flash",
    providerName: "预设模型",
  });
  const credential = readStaffBootstrap(dir, {});
  assert.equal(credential.apiKey, KEY);
  assert.equal(credential.baseUrl, "https://api.deepseek.com");
  assert.equal(credential.modelId, "deepseek-flash");
  assert.equal(credential.providerName, "预设模型");
});

test("the environment wins over the file", () => {
  const dir = dataDirWith({ apiKey: "sk-from-file", baseUrl: "https://file.example" });
  const credential = readStaffBootstrap(dir, {
    PI_STAFF_BOOTSTRAP_API_KEY: KEY,
    PI_STAFF_BOOTSTRAP_BASE_URL: "https://env.example",
  });
  assert.equal(credential.apiKey, KEY);
  assert.equal(credential.baseUrl, "https://env.example");
});

test("a malformed file does not stop the console from opening", () => {
  assert.equal(readStaffBootstrap(dataDirWith("{ not json"), {}), null);
  assert.equal(readStaffBootstrap(dataDirWith("[1,2,3]"), {}), null);
});

test("only the missing blueprints are installed, and the key stays out of the log", async () => {
  const dir = dataDirWith({ apiKey: KEY, modelId: "deepseek-flash" });
  const calls = [];
  const logged = [];
  const result = await ensureStaffPresets({
    dataDir: dir,
    logger: { app: (...args) => logged.push(args) },
    call: async (method, params) => {
      calls.push({ method, params });
      if (method === "staff.presets.list") {
        return {
          presets: [
            { id: "scout", name: "侦察员", installed: true },
            { id: "interestAnalyst", name: "选题员", installed: false },
          ],
        };
      }
      return { installed: ["选题员"], skipped: [] };
    },
  });
  assert.deepEqual(result.installed, ["选题员"]);
  assert.equal(calls.length, 2);
  assert.deepEqual(calls[1].params.ids, ["interestAnalyst"]);
  assert.equal(calls[1].params.apiKey, KEY);
  assert.equal(JSON.stringify(logged).includes(KEY), false);
  assert.equal(JSON.stringify(calls[0].params ?? {}).includes(KEY), false);
});

test("a fully staffed console makes no install call at all", async () => {
  const calls = [];
  const result = await ensureStaffPresets({
    dataDir: dataDirWith(),
    logger: { app: () => {} },
    call: async (method, params) => {
      calls.push({ method, params });
      return { presets: [{ id: "scout", name: "侦察员", installed: true }] };
    },
  });
  assert.deepEqual(result, { installed: [], skipped: [] });
  assert.equal(calls.length, 1);
});

test("an install without a key still hires, just uncredentialed", async () => {
  let installParams = null;
  await ensureStaffPresets({
    dataDir: dataDirWith(),
    logger: { app: () => {} },
    call: async (method, params) => {
      if (method === "staff.presets.list") {
        return { presets: [{ id: "scout", name: "侦察员", installed: false }] };
      }
      installParams = params;
      return { installed: ["侦察员"], skipped: [] };
    },
  });
  assert.deepEqual(installParams.ids, ["scout"]);
  assert.equal("apiKey" in installParams, false);
});
