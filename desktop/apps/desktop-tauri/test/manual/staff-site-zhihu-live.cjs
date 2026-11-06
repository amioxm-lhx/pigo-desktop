/**
 * Live check of the Zhihu collector against the real site.
 *
 * An employee has to read Zhihu through the owner's own account, because the
 * site answers an anonymous visitor with its sign-in wall and signs its JSON
 * endpoints with a header only its own bundle can compute. This script proves
 * the two halves of that arrangement against the live site rather than against
 * a fixture: that an unsigned session is refused (and refused out loud), and
 * that the session the console's QR scan wrote really does return a list and
 * the question pages that list links to, with their answers and comments.
 *
 * It runs inside Electron because only Electron has the Chromium that holds
 * the site partitions. Its output goes to stdout and to a JSON report beside
 * the script's temp file, since a Windows GUI process does not always get a
 * console.
 *
 * Usage (PowerShell):
 *   $electron = "$env:TEMP\electron-436\electron.exe"   # or any Electron 4x
 *
 *   # 1. anonymous: no login anywhere, so the gate must be reported
 *   & $electron apps/desktop/test/manual/staff-site-zhihu-live.cjs
 *
 *   # 2. signed in: reads the partition the console wrote after a QR scan.
 *   #    Close the app first -- Chromium keeps a lock on that profile.
 *   $env:PI_STAFF_SITE_USER_DATA = "$env:APPDATA\PI-Agent-Go-Desktop"
 *   & $electron apps/desktop/test/manual/staff-site-zhihu-live.cjs
 */
const { mkdtempSync, rmSync, writeFileSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { join } = require('node:path');
const { pathToFileURL } = require('node:url');
const { BrowserWindow, app } = require('electron');

// The main-process sources use bundler-style extensionless relative imports,
// which plain ESM resolution rejects; the hook retries them with .ts so this
// script loads the very modules the app ships rather than a copy of them.
require('node:module').register(pathToFileURL(join(__dirname, '..', 'helpers', 'ts-import-hooks.mjs')).href);

const moduleUrl = (relative) => pathToFileURL(join(__dirname, relative)).href;
const reportPath = join(tmpdir(), 'pi-staff-site-zhihu-live.json');
const report = { checks: [], facts: {}, ok: false };
const say = (line) => { console.log(line); if (report.lines) report.lines.push(line); else report.lines = [line]; };
const ok = [];
const check = (label, condition, detail) => {
  const line = label + (condition ? '' : ' -- ' + (detail === undefined ? 'failed' : detail));
  ok.push({ label, passed: Boolean(condition) });
  say((condition ? 'ok   ' : 'FAIL ') + line);
  if (!condition) throw new Error('check failed: ' + label + (detail ? ' -- ' + detail : ''));
};
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
/**
 * Whether the sign-in page is showing a code the owner's phone can read.
 *
 * The code is drawn either as a canvas or as an image, and its size is what
 * tells it apart from the page's icons and logos; the wording around it is
 * checked too, so a page that merely holds a large illustration is not
 * counted as scannable.
 */
const QR_SCRIPT = [
  '(() => {',
  '  const canvases = Array.from(document.querySelectorAll("canvas"));',
  '  const images = Array.from(document.images);',
  '  const bigCanvas = canvases.some((c) => c.width >= 80 && c.height >= 80);',
  '  const bigImage = images.some((i) => i.naturalWidth >= 80 && i.naturalHeight >= 80);',
  '  const text = document.body ? document.body.innerText : "";',
  '  const saysScan = text.includes("扫码") || /scan/i.test(text);',
  '  const flat = text.split(String.fromCharCode(10)).join(" ").slice(0, 120);',
  '  return { hasCode: (bigCanvas || bigImage) && saysScan, canvases: canvases.length, images: images.length, saysScan, sample: flat };',
  '})()',
].join('');


const reusedProfile = (process.env.PI_STAFF_SITE_USER_DATA || '').trim();
const tempProfile = reusedProfile ? null : mkdtempSync(join(tmpdir(), 'pi-staff-site-zhihu-'));
// Partitions live under userData, so pointing userData at the console's own
// directory is what makes this script read the very session a scan produced.
app.setPath('userData', reusedProfile || tempProfile);

const quietLogger = { app: () => {} };

app.whenReady().then(async () => {
  const sites = await import(moduleUrl('../../electron/main/runtime/staff-sites.ts'));
  const { createStaffSiteBrowser, } = await import(moduleUrl('../../electron/main/runtime/staff-browser.ts'));
  const dataDir = mkdtempSync(join(tmpdir(), 'pi-staff-site-settings-'));
  const browser = createStaffSiteBrowser({ dataDir, logger: quietLogger });
  try {
    say('profile: ' + (reusedProfile ? reusedProfile : tempProfile + ' (throwaway)'));
    const spec = sites.siteSpec('zhihu');
    check('zhihu has a collector', Boolean(spec && spec.supported));
    check('the sweep starts from the hot list', spec.defaultSeedUrl === 'https://www.zhihu.com/hot', spec.defaultSeedUrl);
    check('a real login is recognised by z_c0', spec.authCookies.join(',') === 'z_c0', spec.authCookies.join(','));

    const listed = (await browser.list()).find((entry) => entry.channel === 'zhihu');
    check('the console lists zhihu', Boolean(listed));
    check('a sweep reads one page unless the owner raises the cap', listed.maxUrls === 1, String(listed.maxUrls));
    check('the gate is a login, not nothing', listed.gate === 'login', listed.gate);
    report.facts.signedIn = listed.signedIn;
    report.facts.seedUrl = listed.seedUrl;

    // A sweep reads the page it was given and then the pages that list linked
    // to, because a list page is titles: the question's own text, the answers
    // with their vote counts and the comments under them are one level down.
    const pages = await browser.collect('zhihu', [listed.seedUrl]);
    const seed = pages[0];
    const details = pages.slice(1);
    check('the sweep starts from the page it was given', Boolean(seed) && seed.url.indexOf('/hot') > 0, seed && seed.url);
    check('a sweep never follows more than five detail pages', details.length <= sites.MAX_DETAIL_PAGES, String(details.length));
    report.facts.collected = { url: seed.url, title: seed.title, itemCount: seed.itemCount, error: seed.error || null };
    report.facts.detailPages = details.map((entry) => entry.url);
    say('collected: ' + JSON.stringify(report.facts.collected));
    say('detail pages: ' + details.length);

    if (listed.signedIn) {
      check('a signed-in session reads the page', !seed.error, seed.error);
      check('the hot list came back with items', seed.itemCount > 0, String(seed.itemCount));
      for (const item of seed.items.slice(0, 5)) say('  - ' + item.title + '  ' + item.url);
      for (const detail of details) {
        say('detail: ' + detail.url + ' items=' + detail.itemCount);
        if (detail.summary) say('  body: ' + detail.summary.slice(0, 160));
        for (const item of detail.items.slice(0, 6)) {
          say('  ' + (item.kind || 'item') + ' | ' + item.title + (item.meta ? ' | ' + item.meta : ''));
          if (item.excerpt) say('      ' + item.excerpt.slice(0, 120));
        }
      }
      // Depth is the whole point of this half: without it the step only ever
      // sees headlines, and a business signal cannot be judged on a headline.
      check(
        'the detail pages came back with answers or comments',
        details.some((detail) => detail.items.some((item) => item.kind === 'answer' || item.kind === 'comment')),
        details.map((detail) => detail.itemCount).join(','),
      );
    } else {
      check('an unsigned session is refused out loud', Boolean(seed.error), JSON.stringify(seed));
      check('and never reads as an empty page', seed.itemCount === 0 && seed.error.indexOf('sign') >= 0, seed.error);
      check('and no detail page is opened without a session', details.length === 0, String(details.length));

      // The refusal above is the driver's own gate. The next step asks the
      // site itself, so the gate is checked against the live answer and not
      // only against our own cookie list.
      const partition = sites.sitePartition('zhihu');
      const win = new BrowserWindow({
        show: false,
        width: 1280,
        height: 900,
        webPreferences: { partition, contextIsolation: true, nodeIntegration: false, sandbox: true, backgroundThrottling: false },
      });
      try {
        await win.loadURL(spec.defaultSeedUrl);
        await delay(4000);
        const raw = await win.webContents.executeJavaScript(sites.ZHIHU_PAGE_SCRIPT, true);
        const cookies = await win.webContents.session.cookies.get({});
        report.facts.landed = raw.url;
        report.facts.anonymousCookies = sites.cookieNames(cookies);
        say('landed on: ' + raw.url);
        say('anonymous cookies: ' + report.facts.anonymousCookies.join(', '));
        check('the site answers an anonymous visitor with its login page', sites.isLoginWall(raw), JSON.stringify({ url: raw.url, itemCount: raw.itemCount }));
        check('the anonymous session has no login cookie', !cookies.some((cookie) => cookie.name === 'z_c0'));

        // Only the owner's phone can finish a scan, but whether the page
        // offers a code at all is answerable here, and that is the half which
        // breaks quietly when a site redesigns its sign-in page.
        await win.loadURL(spec.signInUrl);
        await delay(5000);
        const qr = await win.webContents.executeJavaScript(QR_SCRIPT, true);
        report.facts.signInPage = qr;
        say('sign-in page: ' + JSON.stringify(qr));
        check('the sign-in page offers a scannable code', qr.hasCode, JSON.stringify(qr));
      } finally {
        if (!win.isDestroyed()) win.destroy();
      }
    }

    report.ok = true;
  } catch (error) {
    report.error = String((error && error.stack) || error);
    say('probe failed: ' + report.error);
    process.exitCode = 1;
  } finally {
    browser.dispose();
    try { rmSync(dataDir, { recursive: true, force: true }); } catch {}
    writeFileSync(reportPath, JSON.stringify(report, null, 2));
    say('report: ' + reportPath);
    if (!report.ok) say('FAILED -- ' + ok.filter((entry) => !entry.passed).length + ' check(s) failed');
    else say('ZHIHU SITE LIVE OK: ' + ok.length + ' checks');
    delay(200).then(() => app.exit(report.ok ? 0 : 1));
  }
});