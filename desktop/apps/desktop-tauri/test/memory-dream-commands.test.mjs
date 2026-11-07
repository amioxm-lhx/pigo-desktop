import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = (path) => readFile(new URL(path, import.meta.url), "utf8");

const [
  composerRs,
  commands,
  api,
  sharedProtocol,
  appState,
  panel,
  proxyRs,
  routerRs,
  sidecarProtocol,
  sidecarDream,
  sidecarMemory,
  hostproxyRs,
  ...locales
] = await Promise.all([
  read("../src-tauri/src/composer.rs"),
  read("../src/lib/commands.ts"),
  read("../src/lib/api.ts"),
  read("../../../packages/shared/src/protocol.ts"),
  read("../src/stores/app-state.ts"),
  read("../src/components/SessionInfoPanel.tsx"),
  read("../src-tauri/src/proxy.rs"),
  read("../src-tauri/src/router.rs"),
  read("../../../sidecar/internal/protocol/protocol.go"),
  read("../../../sidecar/internal/runtime/dream.go"),
  read("../../../sidecar/internal/runtime/memory.go"),
  read("../../../sidecar/internal/hostproxy/hostproxy.go"),
  ...[
    "en",
    "zh-CN",
    "zh-TW",
    "de",
    "es",
    "tr",
    "fr",
    "ko",
  ].map((locale) =>
    read(`../../../packages/i18n/src/locales/${locale}/index.ts`),
  ),
]);

const memoryKeys = [
  "infoMemoryTitle",
  "infoMemoryFootnote",
  "memoryStoreGroup",
  "memoryStatus",
  "memoryRoot",
  "memoryIndex",
  "memoryTotal",
  "memoryByScope",
  "memoryReconciled",
  "memoryError",
  "dreamNeedsSession",
  "dreamRunning",
  "dreamDone",
  "dreamDryRunDone",
];

test("Rust registry exposes /memory and /dream", () => {
  const aliasBlock = composerRs.slice(
    composerRs.indexOf("const BUILTIN_ALIASES"),
    composerRs.indexOf("pub async fn commands"),
  );
  assert.match(
    aliasBlock,
    /name: "memory",\s*id: "builtin\.memory\.report",[\s\S]*?arg_mode: "none"/,
  );
  assert.match(
    aliasBlock,
    /name: "dream",\s*id: "builtin\.agent\.dream",[\s\S]*?arg_mode: "optional"/,
  );
});

test("/memory opens a sidecar-backed report card", () => {
  assert.match(commands, /case "builtin\.memory\.report":/);
  assert.match(commands, /store\.openInfoPanel\("memory"\)/);
  assert.match(appState, /export type InfoPanelKind = "session" \| "status" \| "memory"/);
  // The report is the one panel figure the renderer cannot assemble itself.
  assert.match(panel, /api\s*\.reportMemory\(\)|api\.reportMemory\(\)/);
  assert.match(panel, /infoPanel !== "memory"/);
  assert.match(panel, /chat\.infoMemoryFootnote/);
});

test("/dream runs the consolidation pass with the session model", () => {
  assert.match(commands, /case "builtin\.agent\.dream":/);
  assert.match(commands, /async function runDreamPass/);
  assert.match(commands, /await api\.runDream\(sessionId, dryRun\)/);
  // The dry-run body mirrors the CLI flag instead of being ignored.
  assert.match(commands, /\["dry-run", "--dry-run", "-n"\]\.includes/);
  // A missing session is a hint, not an empty report.
  assert.match(commands, /chat\.dreamNeedsSession/);
});

test("IPC channels exist on both sides of the boundary", () => {
  assert.match(sharedProtocol, /memoryReport: "pi-desktop\/memory\/report"/);
  assert.match(sharedProtocol, /agentDream: "pi-desktop\/agent\/dream"/);
  assert.match(api, /reportMemory: \(\) =>/);
  assert.match(api, /runDream: \(sessionId: string, dryRun = false\) =>/);
  // /memory is a verbatim forward; /dream is wrapped because it needs the
  // session's launch binding (provider, model, credential).
  assert.match(proxyRs, /\("pi-desktop\/memory\/report", "memory\.report"\)/);
  // /dream is wrapped, so it is dispatched in the router, never a proxy row.
  assert.match(routerRs, /"pi-desktop\/agent\/dream" => agent_dream/);
  assert.doesNotMatch(proxyRs, /agent\/dream/);
  assert.match(routerRs, /async fn agent_dream/);
  assert.match(routerRs, /call_timeout\("agent\.dream", request, DREAM_TIMEOUT\)/);
  // A consolidation pass is not a turn.
  assert.doesNotMatch(
    routerRs.slice(
      routerRs.indexOf("async fn agent_dream"),
      routerRs.indexOf("async fn agent_compact"),
    ),
    /session\.beginTurn/,
  );
});

test("the sidecar owns both implementations", () => {
  assert.match(sidecarProtocol, /MethodMemoryReport = "memory\.report"/);
  assert.match(sidecarProtocol, /MethodAgentDream = "agent\.dream"/);
  // The pass runs in-process (no binary to spawn) and is single-flight.
  assert.match(sidecarDream, /dream\.NewLLMConsolidator/);
  assert.match(sidecarDream, /dream\.Runner\{/);
  assert.doesNotMatch(sidecarDream, /exec\.Command/);
  assert.match(sidecarDream, /DREAM_BUSY/);
  // Desktop transcripts are the distillation source.
  assert.match(sidecarDream, /"session\.list"/);
  assert.match(sidecarDream, /"session\.get"/);
  assert.match(hostproxyRs, /"session\.list":\s*true/);
  // /memory reconciles before counting so the numbers describe the files.
  assert.match(sidecarMemory, /store\.Reconcile\(\)/);
  assert.match(sidecarMemory, /store\.CountByScope\(\)/);
  assert.match(sidecarMemory, /type MemoryReport struct/);
});

test("every locale carries the memory and dream copy with one interpolation set", () => {
  for (const locale of locales) {
    for (const key of memoryKeys) {
      assert.match(locale, new RegExp(`\\b${key}: "`), `missing ${key}`);
    }
    const interpolated = [...locale.matchAll(/\bdream(?:DryRun)?Done: "([^"]+)"/g)];
    assert.equal(interpolated.length, 2);
    for (const [, text] of interpolated) {
      for (const placeholder of ["merged", "deduped", "pruned", "distilled"]) {
        assert.ok(
          text.includes(`{{${placeholder}}}`),
          `dreamDone is missing {{${placeholder}}}`,
        );
      }
    }
  }
});
