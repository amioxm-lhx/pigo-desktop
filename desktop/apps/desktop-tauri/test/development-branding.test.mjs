import {
  readMainModule,
  readMainSource,
  readSettingsSource,
} from "./helpers/source-contracts.mjs";
import assert from "node:assert/strict";
import {
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const devScriptUrl = new URL(
  "../../../scripts/dev-electron.mjs",
  import.meta.url,
);

const mainSource = await readMainSource();
const mainIndexSource = await readMainModule("index.ts");
const brandingSource = await readMainModule("bootstrap/app-lifecycle.ts");
const windowSource = await readMainModule("bootstrap/window.ts");
const startupSource = await readMainModule("bootstrap/startup.ts");
const iconScriptSource = await readFile(
  new URL("../../../scripts/make-icon.py", import.meta.url),
  "utf8",
);
const devScriptSource = await readFile(
  devScriptUrl,
  "utf8",
);
const packageJson = JSON.parse(
  await readFile(new URL("../package.json", import.meta.url), "utf8"),
);
const protocolSource = await readFile(
  new URL("../../../packages/shared/src/protocol.ts", import.meta.url),
  "utf8",
);
const windowsIcon = await readFile(
  new URL("../build/icon.ico", import.meta.url),
);

test("Windows runtime registers the canonical native application identity", () => {
  const appId = protocolSource.match(/APP_ID = "([^"]+)"/)?.[1];
  assert.equal(appId, packageJson.build.appId);
  assert.ok(startupSource.includes("app.whenReady()"), "main process readiness hook");
  assert.match(mainIndexSource, /app\.setName\(APP_NAME\)/);
  assert.match(
    mainIndexSource,
    /process\.platform === "win32"[\s\S]*app\.setAppUserModelId\(APP_ID\)/,
  );
});

// A port that reused the upstream executable and shortcut names would be
// indistinguishable in the Start menu and, worse, NSIS would treat the
// upstream install as the same product.
/**
 * The port has to be able to say which engine it runs. "Go" alone does not name
 * pi-go, and a build that answers nothing is indistinguishable from the
 * upstream Node one, so the About row carries engine, implementation, version
 * and binary - and every shipped locale has the label for it.
 */
test("Settings -> About reports the pi-go engine, in every shipped locale", async () => {
  const appIpc = await readMainModule("ipc/app-ipc.ts");
  const sidebarIpc = await readMainModule("agent-sidecar.ts");
  const settingsPage = await readSettingsSource();

  assert.match(sidebarIpc, /export const AGENT_ENGINE = "pi-go";/);
  assert.match(sidebarIpc, /async engineInfo\(\): Promise<AgentEngineInfo>/);
  assert.match(
    sidebarIpc,
    /this\.call<\{ version\?: string \}>\(["']sidecar\.health["']\)/,
    "the sidecar's own health answer is the version source",
  );
  assert.match(sidebarIpc, /binary: basename\(this\.binaryPath\)/);
  assert.match(appIpc, /getSidecar: \(\) => AgentSidecar \| null;/);
  assert.match(appIpc, /const agent = sidecar \? await sidecar\.engineInfo\(\) : undefined;/);
  assert.match(appIpc, /^\s+agent,$/m, "appGetVersion carries the engine to the renderer");
  // The document title is what the OS task switcher and window manager show,
  // so it has to name the port too, not the shell it was forked from.
  const indexHtml = await readFile(
    new URL("../index.html", import.meta.url),
    "utf8",
  );
  assert.match(indexHtml, /<title>pigo-desktop<\/title>/);
  assert.match(settingsPage, /t\("settings\.agentEngine"\)/);
  assert.match(settingsPage, /data-agent-engine=\{version\?\.agent\?\.engine\}/);
  assert.match(
    settingsPage,
    /\[\s*version\.agent\.engine,[\s\S]*?`\$\{version\.agent\.impl\} sidecar`,[\s\S]*?version\.agent\.version,[\s\S]*?version\.agent\.binary,[\s\S]*?\]\s*\.filter\(Boolean\)\s*\.join\(" · "\)/,
    "the row shows engine, implementation, version and binary",
  );

  for (const locale of [
    "en",
    "zh-CN",
    "zh-TW",
    "de",
    "es",
    "fr",
    "ko",
    "tr",
  ]) {
    const catalog = await readFile(
      new URL(`../../../packages/i18n/src/locales/${locale}/index.ts`, import.meta.url),
      "utf8",
    );
    assert.match(
      catalog,
      /"?agentEngine"?: "/,
      `${locale} is missing the engine label`,
    );
  }
});

test("Windows packages pin PI-Agent-Go-Desktop executable and shortcut names", () => {
  assert.equal(packageJson.build.win.executableName, "PI-Agent-Go-Desktop");
  assert.equal(packageJson.build.nsis.shortcutName, "PI-Agent-Go-Desktop");
});

test("the Windows installer carries its own identity, never upstream's", () => {
  // `appId` is what electron-builder hashes into the NSIS uninstall GUID and
  // writes next to the Start menu entry. Shipping this port under upstream's
  // `com.pi-desktop.app` made installing it close an already installed
  // PI-Desktop and then overwrite its directory, uninstall entry and shortcut
  // (commit a376959), so the appId and every name derived from it stay pinned.
  assert.equal(packageJson.build.appId, "com.pi-agent-go.desktop");
  assert.equal(packageJson.build.productName, "PI-Agent-Go-Desktop");
  assert.equal(packageJson.build.nsis.artifactName, "pi-agent-go-desktop-setup.${ext}");
  // An assisted installer with a directory page is machine-wide by
  // construction: electron-builder defines INSTALL_MODE_PER_ALL_USERS_REQUIRED
  // whenever `oneClick` is false, so `perMachine` cannot make it per-user. The
  // isolation therefore rests on the identity above, and the port must keep
  // installing next to upstream rather than over it.
  assert.equal(packageJson.build.nsis.oneClick, false);
  assert.equal(packageJson.build.nsis.allowToChangeInstallationDirectory, true);
});

test("Windows packages and windows use the canonical pi-agent-go icon", () => {
  assert.equal(packageJson.build.win.icon, "build/icon.ico");
  assert.deepEqual(
    packageJson.build.win.extraResources.find((resource) => resource.to === "app-icon.ico"),
    {
      from: "build/icon.ico",
      to: "app-icon.ico",
    },
  );
  assert.deepEqual([...windowsIcon.subarray(0, 4)], [0, 0, 1, 0]);
  assert.ok(windowsIcon.readUInt16LE(4) >= 4, "ICO must contain multiple sizes");
  assert.match(iconScriptSource, /windows_icon = BUILD \/ "icon\.ico"/);
  assert.match(iconScriptSource, /format="ICO"/);
  assert.match(windowSource, /function windowsIconPath\(\)/);
  assert.match(windowSource, /app\.isPackaged\s*\n?\s*\?\s*process\.resourcesPath/);
  assert.match(windowSource, /app-icon\.ico/);
  assert.match(windowSource, /icon: windowsIconPath\(\)/);
});

test("Linux packages align the desktop entry with the Wayland app identity", () => {
  assert.equal(packageJson.desktopName, "pi-agent-go-desktop.desktop");
  assert.equal(packageJson.build.linux.syncDesktopName, true);
});

test("macOS development uses the canonical pi-agent-go Dock icon", () => {
  assert.match(
    brandingSource,
    /process\.platform !== "darwin" \|\| !isDevelopmentBuild \|\| !app\.dock/,
  );
  assert.match(
    brandingSource,
    /join\(app\.getAppPath\(\), "build", "icon_1024\.png"\)/,
  );
  assert.match(brandingSource, /nativeImage\.createFromPath\(iconPath\)/);
  assert.match(brandingSource, /if \(icon\.isEmpty\(\)\)/);
  assert.match(brandingSource, /app\.dock\.setIcon\(icon\)/);
  // Branding is the first thing readiness does, after the only statement that
  // may precede it: the bail for a launch that lost the single-instance lock
  // and must not touch the running app's Dock tile.
  assert.match(
    startupSource,
    /if \(!hasSingleInstanceLock\) return;\s+applyDevelopmentBranding\(\);/,
  );
});

test("macOS icon derivation preserves the canonical renderer asset", () => {
  assert.match(iconScriptSource, /SOURCE = BUILD \/ "icon_1024\.png"/);
  assert.match(iconScriptSource, /with Image\.open\(SOURCE\) as source/);
  assert.doesNotMatch(
    iconScriptSource,
    /\.save\(BUILD \/ "icon_1024\.png"\)/,
  );
});

test("macOS development launches from a branded host bundle", () => {
  assert.equal(packageJson.scripts.dev, "node ../../scripts/dev-electron.mjs");
  assert.match(devScriptSource, /process\.platform === "darwin"/);
  assert.match(devScriptSource, /PI_DESKTOP_DEV: "1"/);
  assert.match(devScriptSource, /ELECTRON_EXEC_PATH/);
  assert.match(devScriptSource, /CFBundleDisplayName", APP_NAME/);
  assert.match(devScriptSource, /CFBundleName", APP_NAME/);
  assert.match(devScriptSource, /CFBundleExecutable", APP_NAME/);
  assert.match(devScriptSource, /CFBundleIconFile", "icon\.icns"/);
  assert.match(
    devScriptSource,
    /copyFileSync\(iconPath, join\(resources, "icon\.icns"\)\)/,
  );
  assert.match(devScriptSource, /verbatimSymlinks: true/);
  assert.match(devScriptSource, /join\(ROOT, "\.cache", "electron-dev"\)/);
  assert.doesNotMatch(devScriptSource, /node_modules.*Info\.plist/);
});

test(
  "development launcher can be imported without starting Electron",
  async () => {
    const launcher = await import(devScriptUrl.href);
    assert.equal(typeof launcher.prepareMacDevelopmentBundle, "function");
  },
);

test(
  "macOS development bundle rewrites native identity and reuses its cache",
  { skip: process.platform !== "darwin" },
  async () => {
    const root = await mkdtemp(join(tmpdir(), "pi-desktop-dev-bundle-"));
    const sourceBundle = join(root, "Electron.app");
    const contents = join(sourceBundle, "Contents");
    const macos = join(contents, "MacOS");
    const resources = join(contents, "Resources");
    const executable = join(macos, "Electron");
    const iconPath = join(root, "source.icns");
    const cacheRoot = join(root, "cache");

    try {
      await mkdir(macos, { recursive: true });
      await mkdir(resources, { recursive: true });
      await writeFile(executable, "electron-host");
      await writeFile(iconPath, "canonical-icon");
      await writeFile(
        join(contents, "Info.plist"),
        `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleDisplayName</key><string>Electron</string>
<key>CFBundleName</key><string>Electron</string>
<key>CFBundleExecutable</key><string>Electron</string>
<key>CFBundleIdentifier</key><string>com.github.Electron</string>
<key>CFBundleIconFile</key><string>electron.icns</string>
</dict></plist>`,
      );

      const { prepareMacDevelopmentBundle } = await import(devScriptUrl.href);
      const options = {
        electronExecutable: executable,
        electronVersion: "test-version",
        iconPath,
        cacheRoot,
        sign: false,
      };
      const brandedExecutable = prepareMacDevelopmentBundle(options);
      const brandedContents = join(
        brandedExecutable,
        "..",
        "..",
      );
      const plist = await readFile(join(brandedContents, "Info.plist"), "utf8");

      assert.equal(await readFile(brandedExecutable, "utf8"), "electron-host");
      assert.equal(
        await readFile(join(brandedContents, "Resources", "icon.icns"), "utf8"),
        "canonical-icon",
      );
      assert.match(plist, /<string>PI-Agent-Go-Desktop<\/string>/);
      assert.match(plist, /<string>com\.pi-agent-go\.desktop\.dev<\/string>/);
      assert.equal(prepareMacDevelopmentBundle(options), brandedExecutable);
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  },
);
