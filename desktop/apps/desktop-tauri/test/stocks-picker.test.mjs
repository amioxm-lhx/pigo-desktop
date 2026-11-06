import { readFile } from "node:fs/promises";
import test from "node:test";
import assert from "node:assert/strict";
import { loadStyles } from "./helpers/styles.mjs";

/**
 * 选股神器 wiring contract: the app rail entry, the shell route, the shared
 * IPC channels, the main-process engine and the locale catalogs must all
 * agree, or the page renders with missing wording or dead channels.
 */

const read = (path) => readFile(new URL(path, import.meta.url), "utf8");

const [
  appsNav,
  appShell,
  appState,
  api,
  protocol,
  engine,
  stocksIpc,
  registerIpc,
  page,
  styles,
  en,
  zhCN,
  zhTW,
  de,
  es,
  fr,
  ko,
  tr,
] = await Promise.all([
  read("../src/features/boards/AppsNav.tsx"),
  read("../src/features/app/AppShell.tsx"),
  read("../src/stores/app-state.ts"),
  read("../src/lib/api.ts"),
  read("../../../packages/shared/src/protocol.ts"),
  read("../electron/main/stocks-engine.ts"),
  read("../electron/main/ipc/stocks-ipc.ts"),
  read("../electron/main/ipc/register.ts"),
  read("../src/features/stocks/StockPickerPage.tsx"),
  loadStyles(),
  read("../../../packages/i18n/src/locales/en/index.ts"),
  read("../../../packages/i18n/src/locales/zh-CN/index.ts"),
  read("../../../packages/i18n/src/locales/zh-TW/index.ts"),
  read("../../../packages/i18n/src/locales/de/index.ts"),
  read("../../../packages/i18n/src/locales/es/index.ts"),
  read("../../../packages/i18n/src/locales/fr/index.ts"),
  read("../../../packages/i18n/src/locales/ko/index.ts"),
  read("../../../packages/i18n/src/locales/tr/index.ts"),
]);

/** The `stocks:` catalog section of a locale, sliced to the next top-level key. */
function stocksSection(catalog) {
  const start = catalog.indexOf("  stocks: {");
  assert.ok(start >= 0, "stocks section missing");
  const end = catalog.indexOf("\n  },\n", start);
  assert.ok(end > start, "stocks section unterminated");
  return catalog.slice(start, end);
}

test("the stock picker is a first-class app: rail entry, route and page union agree", () => {
  assert.match(appState, /\|\s"stocks"(;|\s*\|)/);
  assert.match(appsNav, /page:\s"stocks"/);
  assert.match(appsNav, /labelKey:\s"stocks\.nav"/);
  assert.match(appShell, /page === "stocks" \? \(/);
  assert.match(appShell, /import\("\.\.\/stocks\/StockPickerPage"\)/);
});

test("every renderer call has a shared channel and a main-process handler", () => {
  const channels = [
    "stocksStatus",
    "stocksApiKeySet",
    "stocksOverview",
    "stocksPlan",
    "stocksWatchList",
    "stocksWatchAdd",
    "stocksWatchRemove",
  ];
  for (const channel of channels) {
    assert.match(protocol, new RegExp(`${channel}:\\s"pi-desktop/stocks/`), `${channel} missing in protocol`);
    assert.match(api, new RegExp(`IPC\\.invoke\\.${channel}[,)]`), `${channel} missing in api`);
    assert.match(stocksIpc, new RegExp(`IPC\\.invoke\\.${channel}\\b`), `${channel} missing in stocks-ipc`);
  }
  // The engine is a resident instance (upstream caches must survive calls).
  assert.match(registerIpc, /registerStocksIpc\(/);
  assert.match(registerIpc, /stocksEngine \?\?= createStocksEngine\(\{ dataDir \}\)/);
});

test("the engine keeps to the upstream rules: keyed, throttled, no silent mock fill-ins", () => {
  assert.match(engine, /HITHINK_FINANCE_API_KEY/);
  assert.match(engine, /UPSTREAM_GAP_MS = 150/);
  assert.match(engine, /class MissingApiKeyError/);
  // Upstream failures surface to the user instead of producing fake rows.
  assert.match(engine, /catch/);
});

test("the page renders advice from structure, never from hardcoded sentences", () => {
  assert.match(page, /entryText\(/);
  assert.match(page, /reasonText\(/);
  assert.match(page, /t\(`stocks\.phase\.\$\{sentiment\.phase\}`\)/);
  assert.match(page, /t\(`stocks\.plan\.\$\{item\.sessionKey\}`\)/);
  // The disclaimer stays on the page: scoring is reference-only.
  assert.match(page, /stocks\.disclaimer/);
});

test("the styles partial is imported and stays on ds tokens", () => {
  assert.match(styles, /\.stock-phase-climax/);
  assert.doesNotMatch(styles, /color-mix\(in srgb/);
  assert.match(styles, /var\(--ds-/);
});

test("all eight locales carry the full stocks section", () => {
  const catalogs = [en, zhCN, zhTW, de, es, fr, ko, tr];
  const requiredLeaves = [
    "nav:",
    "navHint:",
    "freeze:",
    "climax:",
    "auction:",
    "sentimentCheck:",
    "stopHit:",
    "targetHit:",
    "track:",
    "watchEmpty:",
    "disclaimer:",
    "weekend:",
    "dragon:",
  ];
  for (const catalog of catalogs) {
    const section = stocksSection(catalog);
    for (const leaf of requiredLeaves) {
      assert.ok(section.includes(leaf), `stocks section missing ${leaf}`);
    }
  }
  // The app rail reads these two; a missing one renders a raw key.
  for (const catalog of catalogs) {
    const section = stocksSection(catalog);
    assert.match(section, /nav:\s*"/);
    assert.match(section, /navHint:\s*"/);
  }
});
