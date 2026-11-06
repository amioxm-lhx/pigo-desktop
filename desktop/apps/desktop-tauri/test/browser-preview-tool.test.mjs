import { readAppSource, readMainSource } from "./helpers/source-contracts.mjs";
import { readSidecarRuntimeSource } from "./helpers/sidecar-source.mjs";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const sidecarSource = await readFile(
  new URL("../electron/main/agent-sidecar.ts", import.meta.url),
  "utf8",
);
const mainSource = await readMainSource();
const appSource = await readAppSource();
const apiSource = await readFile(
  new URL("../src/lib/api.ts", import.meta.url),
  "utf8",
);
const protocolSource = await readFile(
  new URL("../../../packages/shared/src/protocol.ts", import.meta.url),
  "utf8",
);
const runtimeSource = readSidecarRuntimeSource();

test("sidecar routes main-local tools before the host-core proxy", () => {
  // Local tools short-circuit tools.execute; other methods still proxy.
  assert.match(sidecarSource, /setLocalTool\(name: string, handler: LocalToolHandler\)/);
  assert.match(
    sidecarSource,
    /method === "tools\.execute"\s*\?\s*this\.localTools\.get/,
  );
  // Host availability is only required on the proxy path, after local
  // dispatch — a local tool must work even if host-core is restarting.
  const proxyBranch = sidecarSource.slice(
    sidecarSource.indexOf('msg.method === "host.proxy"'),
  );
  assert.ok(
    proxyBranch.indexOf("this.localTools.get") <
      proxyBranch.indexOf('throw new Error("host unavailable")'),
  );
});

test("main serves BrowserPreview from its originating session workspace", () => {
  assert.match(mainSource, /s\.setLocalTool\("BrowserPreview"/);
  // The local tool receives session identity from the sidecar and resolves the
  // workspace from that durable session, never from the visible shell project.
  const handler = mainSource.slice(
    mainSource.indexOf('s.setLocalTool("BrowserPreview"'),
    mainSource.indexOf("sidecar = s;"),
  );
  assert.match(handler, /async \(\{ args, sessionId \}\)/);
  assert.match(handler, /host\?\.call\("session\.get", \{ id: sessionId \}\)/);
  assert.match(handler, /res\?\.session\?\.projectPath/);
  assert.doesNotMatch(handler, /workspace\.get/);
  assert.match(handler, /browserHost\.previewWorkspaceFile\(sessionId, raw, root\)/);
  assert.ok(
    handler.indexOf("resolveLocalFile(raw, root)") <
      handler.indexOf("sendToRenderer(IPC.event.browserPreview"),
  );
  assert.doesNotMatch(handler, /browserPane\.navigate/);
  assert.match(
    handler,
    /sendToRenderer\(IPC\.event\.browserPreview, \{\s*sessionId,\s*path: raw,\s*\}\)/,
  );
  assert.match(protocolSource, /browserPreview: "pi-desktop\/browser\/event\/preview"/);
});

test("renderer routes browser preview events to the originating session", () => {
  assert.match(
    apiSource,
    /onBrowserPreview:[\s\S]*event: \{ sessionId: string; path\?: string; url\?: string \}/,
  );
  const previewHandler =
    appSource.match(/api\.onBrowserPreview\([\s\S]*?\n\s*\}\);/)?.[0] ?? "";
  assert.ok(previewHandler, "browser preview renderer handler exists");
  assert.match(
    previewHandler,
    /openWorkPanelTabForSession\((?:event\.)?sessionId,[\s\S]*browserPluginTab/,
  );
  assert.doesNotMatch(
    appSource,
    /api\.onBrowserPreview\(\(\) => \{\s*useAppStore\.getState\(\)\.openWorkPanelTab/,
  );
  assert.match(appSource, /offBrowserPreview\(\);/);
});

test("the agent runtime exposes BrowserPreview in every mode, with the live-reload promise", () => {
  // The Go registry appends BrowserPreview beside the host catalogue in every
  // mode, because the tool only renders an existing workspace file.
  assert.match(runtimeSource, /tools = append\(tools, browserPreviewTool\(session\)\)/);
  // `path` carries an alias alongside it (D273), so assert the entry and its
  // canonical argument rather than one exact line of source.
  assert.match(
    runtimeSource,
    /"path": map\[string\]any\{\s*"type":\s+"string",\s*"description": "File to preview; workspace-relative\.",\s*\},\s*"file_path": map\[string\]any\{\s*"type":\s+"string",\s*"description": "Alias for `path`\.",\s*\},/,
  );
  assert.match(runtimeSource, /live-reloads/);
  // The preview guidance the Node default prompt carried (user-visible HTML
  // work, the one call per page rule, skipping generated files) is not ported
  // yet: the Go prompt is the product contract plus the mode framing
  // (PORTING-PI-GO.md, M3 "What remains").
  assert.doesNotMatch(runtimeSource, /user-visible HTML pages/);
  assert.doesNotMatch(
    runtimeSource,
    /Skip generated, test-only, and non-visual HTML files/,
  );
});
