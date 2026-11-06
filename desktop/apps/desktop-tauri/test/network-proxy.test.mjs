import {
  readSidecarFile,
  readSidecarRuntimeSource,
} from "./helpers/sidecar-source.mjs";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const electronProxy = await readFile(
  new URL("../electron/main/network-proxy.ts", import.meta.url),
  "utf8",
);
// The Go sidecar replaced both the Node sidecar and its undici proxy shim.
const sidecarRuntime = readSidecarRuntimeSource();
const sidecarProtocol = readSidecarFile("internal", "protocol", "protocol.go");
const hostProcess = await readFile(
  new URL("../electron/main/host-process.ts", import.meta.url),
  "utf8",
);
const hostProxy = await readFile(
  new URL("../../../crates/host-core/src/network_proxy.rs", import.meta.url),
  "utf8",
);

test("Electron main applies Chromium proxy and net.fetch", () => {
  assert.match(electronProxy, /ses\.setProxy\(config\)/);
  assert.match(electronProxy, /net\.fetch\.bind\(net\)/);
  assert.match(electronProxy, /session-created/);
  assert.match(electronProxy, /pi-desktop\/network\/testProxy|PROXY_TEST_URL/);
  assert.match(electronProxy, /PI_DESKTOP_PROXY_JSON/);
});

test("the Go sidecar takes the proxy from the environment it inherits", () => {
  // The Node sidecar rebuilt an undici dispatcher in-process and could
  // reconfigure it without a restart. The Go sidecar has no such path: Electron
  // main applies the proxy to Chromium and exports it into its own environment,
  // the spawned sidecar inherits that environment, and its provider requests go
  // through http.DefaultClient, which is what honours HTTP_PROXY/HTTPS_PROXY.
  assert.match(electronProxy, /applyProxyEnvAssignments\(proxyEnvAssignments\(next\), process\.env\)/);
  assert.doesNotMatch(sidecarRuntime, /ProxyAgent|socks5Connect|setGlobalDispatcher/);
  // The file's proxy block still crosses the wire, so the sidecar can report it
  // back to the settings pane even though it does not install one itself.
  assert.match(sidecarProtocol, /NetworkProxy\s+json\.RawMessage/);
});

test("host-core marketplace curl uses --proxy and Bash does not inherit env", () => {
  assert.match(hostProxy, /curl_proxy_args/);
  assert.match(hostProxy, /"--proxy"/);
  assert.match(hostProcess, /stripProxyEnv\(process\.env\)/);
});
