/**
 * The site registry, exercised without Electron.
 *
 * Everything a collector decides before it touches a browser -- which cookie
 * marks a session as signed in, which URL a sweep may open, how much of a page
 * reaches the model -- is pure, so it is asserted here rather than discovered
 * on the owner's machine.
 */
import assert from "node:assert/strict";
import test from "node:test";
import { register } from "node:module";
import { STAFF_SOURCE_CHANNELS } from "@pi-desktop/shared";

register(new URL("./helpers/ts-import-hooks.mjs", import.meta.url));
const sites = await import("../electron/main/runtime/staff-sites.ts");

test("every probe channel has a site entry, supported or not", () => {
  for (const channel of STAFF_SOURCE_CHANNELS) {
    const spec = sites.siteSpec(channel);
    assert.ok(spec, "missing site spec for " + channel);
    assert.equal(spec.channel, channel);
  }
  assert.equal(sites.siteSpec("nope"), null);
});

test("zhihu is the wired collector and keeps its own partition", () => {
  const zhihu = sites.siteSpec("zhihu");
  assert.equal(zhihu.supported, true);
  assert.equal(zhihu.signInUrl, "https://www.zhihu.com/signin");
  // No seed URL ships: a sweep reads the watchlist below, and a pinned page is
  // something the owner types to debug one, not the shape of a normal run.
  assert.equal(zhihu.defaultSeedUrl, "");
  assert.equal(zhihu.searchUrl, "https://www.zhihu.com/search?type=content&q={query}");
  assert.deepEqual(sites.supportedSiteChannels(), ["zhihu"]);
  assert.equal(sites.sitePartition("zhihu"), "persist:pi-staff-site-zhihu");
  // A channel name is never interpolated raw into a partition path.
  assert.equal(sites.sitePartition("../evil"), "persist:pi-staff-site-.._evil");
});

test("a session counts as signed in only with the site's own login cookie", () => {
  const zhihu = sites.siteSpec("zhihu");
  assert.equal(sites.signedInFromCookies(zhihu, []), false);
  assert.equal(
    sites.signedInFromCookies(zhihu, [{ name: "d_c0" }, { name: "_xsrf" }]),
    false,
  );
  assert.equal(sites.signedInFromCookies(zhihu, [{ name: "z_c0" }]), true);
  const expired = { name: "z_c0", expirationDate: Date.now() / 1000 - 60 };
  assert.equal(sites.signedInFromCookies(zhihu, [expired]), false);
  const live = { name: "z_c0", expirationDate: Date.now() / 1000 + 3600 };
  assert.equal(sites.signedInFromCookies(zhihu, [live]), true);
});

test("a seed URL is http(s) or it is refused", () => {
  assert.equal(sites.normalizeSeedUrl("https://www.zhihu.com/hot"), "https://www.zhihu.com/hot");
  assert.equal(sites.normalizeSeedUrl(" http://example.com/a "), "http://example.com/a");
  assert.equal(sites.normalizeSeedUrl("file:///etc/passwd", "fallback"), "fallback");
  assert.equal(sites.normalizeSeedUrl("javascript:alert(1)", "fallback"), "fallback");
  assert.equal(sites.normalizeSeedUrl("", "fallback"), "fallback");
  assert.equal(sites.normalizeSeedUrl(undefined, "fallback"), "fallback");
});

test("the sweep cap stays between one page and the hard ceiling", () => {
  assert.equal(sites.normalizeMaxUrls(undefined), sites.DEFAULT_MAX_URLS);
  assert.equal(sites.normalizeMaxUrls(undefined), 5);
  assert.equal(sites.normalizeMaxUrls(0), 1);
  assert.equal(sites.normalizeMaxUrls(-5), 1);
  assert.equal(sites.normalizeMaxUrls(2.7), 2);
  assert.equal(sites.normalizeMaxUrls(9999), sites.MAX_MAX_URLS);
  assert.equal(sites.normalizeMaxUrls("3"), 3);
  assert.equal(sites.normalizeMaxUrls("abc"), sites.DEFAULT_MAX_URLS);
});

test("a watchlist is what a site is read for, and it stays a search", () => {
  const zhihu = sites.siteSpec("zhihu");
  assert.ok(zhihu.keywords.length > 1, "a watchlist is a list, not a single page");
  // The list is the buyer's questions. A grievance about someone else's boss
  // is the one thing it must never be read for.
  for (const keyword of zhihu.keywords) {
    assert.ok(keyword.length > 0, "empty keyword");
    assert.ok(!/老板|领导|公司快完蛋/.test(keyword), keyword);
  }
  assert.deepEqual(sites.siteKeywords("zhihu", {}), [...zhihu.keywords]);
  assert.deepEqual(sites.siteKeywords("zhihu", { keywords: [] }), [...zhihu.keywords]);
  assert.deepEqual(sites.siteKeywords("zhihu", { keywords: ["自建清单"] }), ["自建清单"]);
});

test("keywords are cleaned, deduped and capped", () => {
  assert.deepEqual(sites.normalizeKeywords(undefined, ["fallback"]), ["fallback"]);
  assert.deepEqual(sites.normalizeKeywords([], ["fallback"]), ["fallback"]);
  assert.deepEqual(sites.normalizeKeywords(["  a  b ", "", "  ", "a b"]), ["a b"]);
  assert.deepEqual(sites.normalizeKeywords("one\ntwo"), ["one", "two"]);
  const many = Array.from({ length: sites.MAX_KEYWORDS + 5 }, (_, i) => "k" + i);
  assert.equal(sites.normalizeKeywords(many).length, sites.MAX_KEYWORDS);
});

test("a sweep reads the watchlist, and a pinned URL wins outright", () => {
  const urls = sites.sweepUrls("zhihu", {}, 0);
  assert.equal(urls.length, sites.DEFAULT_MAX_URLS);
  for (const url of urls) {
    assert.ok(url.startsWith("https://www.zhihu.com/search?type=content&q="), url);
  }
  // The keyword is encoded, never interpolated raw.
  const first = decodeURIComponent(urls[0].split("q=")[1]);
  assert.equal(first, sites.siteSpec("zhihu").keywords[0]);
  // Pinning one page is how a page is debugged; the list must not override it.
  assert.deepEqual(sites.sweepUrls("zhihu", { seedUrl: "https://www.zhihu.com/hot" }, 0), [
    "https://www.zhihu.com/hot",
  ]);
  // Rotating reaches the rest of the list instead of the same five forever.
  const rotated = sites.sweepUrls("zhihu", {}, 1).map((u) => decodeURIComponent(u.split("q=")[1]));
  assert.notDeepEqual(rotated, urls.map((u) => decodeURIComponent(u.split("q=")[1])));
  const wrapped = sites.sweepUrls("zhihu", {}, sites.siteSpec("zhihu").keywords.length);
  assert.deepEqual(wrapped, urls);
  // An owner who empties the list and pins nothing gets no pages, not the
  // shipped list back: an explicit empty list is a decision.
  assert.deepEqual(sites.sweepUrls("zhihu", { keywords: [], seedUrl: "" }, 0).length, sites.DEFAULT_MAX_URLS);
  assert.deepEqual(sites.sweepUrls("nope", {}, 0), []);
});

test("the chromium user agent loses the electron token", () => {
  const raw =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.7871.250 Electron/43.6.0 Safari/537.36";
  const cleaned = sites.desktopUserAgent(raw);
  assert.ok(!cleaned.includes("Electron"), cleaned);
  assert.ok(cleaned.includes("Chrome/150.0.7871.250"));
  assert.equal(sites.desktopUserAgent("plain"), "plain");
});

test("the sign-in wall is recognised from the path or the rendered text", () => {
  assert.equal(sites.isLoginWall({ path: "/signin" }), true);
  assert.equal(sites.isLoginWall({ path: "/hot" }), false);
  assert.equal(
    sites.isLoginWall({ path: "/hot", body: "登录/注册 扫码登录" }),
    true,
  );
  assert.equal(sites.isLoginWall({ path: "/hot", body: "热榜 今天有什么" }), false);
});

test("a collected page becomes bounded prompt bullets", () => {
  const note = sites.formatCollectedNote("zhihu", {
    url: "https://www.zhihu.com/hot",
    title: "知乎热榜",
    items: [
      { title: "一", url: "https://www.zhihu.com/question/1" },
      { title: "二", url: "https://www.zhihu.com/question/2" },
    ],
  });
  assert.match(note, /^- \[collected:zhihu\] 知乎热榜 — https:\/\/www\.zhihu\.com\/hot/m);
  assert.match(note, /  - 一 — https:\/\/www\.zhihu\.com\/question\/1/);
  assert.match(note, /  - 二 — https:\/\/www\.zhihu\.com\/question\/2/);

  const truncated = sites.formatCollectedNote(
    "zhihu",
    {
      url: "https://www.zhihu.com/hot",
      title: "知乎热榜",
      items: [{ title: "x".repeat(200), url: "https://a" }, { title: "y", url: "https://b" }],
    },
    120,
  );
  assert.match(truncated, /\[1 item omitted for length\]/);
  assert.ok(truncated.length < 400, "note stayed bounded");

  const failed = sites.formatCollectedNote("zhihu", {
    url: "https://www.zhihu.com/hot",
    title: "",
    items: [],
    error: "no signed-in session for this site",
  });
  assert.match(failed, /no signed-in session/);

  const empty = sites.formatCollectedNote("zhihu", {
    url: "https://www.zhihu.com/hot",
    title: "知乎",
    items: [],
  });
  assert.match(empty, /no list items matched/);
});

test("only a same-site https page one level down is a detail target", () => {
  assert.equal(sites.isDetailUrl("zhihu", "https://www.zhihu.com/question/12345"), true);
  assert.equal(sites.isDetailUrl("zhihu", "https://zhihu.com/question/12345"), true);
  assert.equal(sites.isDetailUrl("zhihu", "https://www.zhihu.com/question/1/answer/2"), true);
  // A link that leaves the site is never followed inside the site's session.
  assert.equal(sites.isDetailUrl("zhihu", "http://www.zhihu.com/question/1"), false);
  assert.equal(sites.isDetailUrl("zhihu", "https://evil-zhihu.com/question/1"), false);
  assert.equal(sites.isDetailUrl("zhihu", "https://zhihu.com.evil.example/question/1"), false);
  assert.equal(sites.isDetailUrl("zhihu", "file:///etc/passwd"), false);
  assert.equal(sites.isDetailUrl("zhihu", "javascript:alert(1)"), false);
  assert.equal(sites.isDetailUrl("zhihu", "not a url"), false);
  // A list page is not a detail page, and a channel with no depth has none.
  assert.equal(sites.isDetailUrl("zhihu", "https://www.zhihu.com/hot"), false);
  assert.equal(sites.isDetailUrl("zhihu", "https://www.zhihu.com/p/12345"), false);
  assert.equal(sites.isDetailUrl("juejin", "https://www.zhihu.com/question/1"), false);
});

test("a sweep follows the page's own order, once per question, and stops at the cap", () => {
  const list = [
    { url: "https://www.zhihu.com/question/1" },
    { url: "https://www.zhihu.com/hot" },
    { url: "https://www.zhihu.com/question/2#answer-9" },
    { url: "https://www.zhihu.com/question/1#answer-3" },
    { url: "https://www.zhihu.com/question/3" },
    { url: "https://www.zhihu.com/question/4" },
    { url: "https://www.zhihu.com/question/5" },
    { url: "https://www.zhihu.com/question/6" },
  ];
  assert.deepEqual(sites.detailTargets("zhihu", list), [
    "https://www.zhihu.com/question/1",
    "https://www.zhihu.com/question/2",
    "https://www.zhihu.com/question/3",
  ]);
  // The owner can ask for more, but never past the hard ceiling.
  assert.equal(sites.detailTargets("zhihu", list, 99).length, sites.MAX_DETAIL_PAGES);
  assert.equal(sites.detailTargets("zhihu", list, 1).length, 1);
  assert.deepEqual(sites.detailTargets("zhihu", list, 0), []);
  assert.deepEqual(sites.detailTargets("zhihu", [{ url: "https://www.zhihu.com/hot" }]), []);
});

test("the detail reader exists for zhihu and nowhere else", () => {
  assert.equal(sites.detailScript("juejin"), null);
  const script = sites.detailScript("zhihu");
  assert.ok(script);
  assert.doesNotThrow(() => new Function("return " + script));
  assert.match(script, /document\.querySelectorAll/);
  assert.match(script, /Comment/);
  // Comments are behind the site's own count button, and the button is
  // pressed rather than the endpoint behind it being called.
  assert.match(script, /条评论/);
  assert.match(script, /\.click\(\)/);
  // It reads the document the site already rendered; the site's JSON API
  // wants a signature this process must not forge.
  assert.doesNotMatch(script, /fetch\(|XMLHttpRequest/);
});

test("a detail page reaches the prompt with its answers and comments", () => {
  const note = sites.formatCollectedNote("zhihu", {
    url: "https://www.zhihu.com/question/1",
    title: "怎样做一人公司?",
    summary: "12 个回答 / 想找一套能自己跑起来的系统",
    items: [
      {
        kind: "answer",
        title: "张三",
        url: "https://www.zhihu.com/question/1",
        meta: "赞同 82 · 2026-09-18",
        excerpt: "我用一套定时任务顶了三个人的活",
      },
      { kind: "comment", title: "李四", url: "", excerpt: "有联系方式吗" },
    ],
  });
  assert.match(note, /^- \[collected:zhihu\] 怎样做一人公司\? — /m);
  assert.match(note, /  正文：12 个回答/);
  assert.match(note, /  - 回答｜张三 — /);
  assert.match(note, /    赞同 82 · 2026-09-18/);
  assert.match(note, /    我用一套定时任务顶了三个人的活/);
  assert.match(note, /  - 评论｜李四/);
  assert.match(note, /    有联系方式吗/);

  // A page that says a great deal is still trimmed to the same cap rather than
  // pushed whole into the next step's context window.
  const bounded = sites.formatCollectedNote(
    "zhihu",
    {
      url: "https://www.zhihu.com/question/1",
      title: "长页",
      summary: "s".repeat(4000),
      items: [{ kind: "comment", title: "c", url: "", excerpt: "x".repeat(3000) }],
    },
    6000,
  );
  assert.ok(bounded.length <= 6000, "the note stayed inside its cap: " + bounded.length);
  assert.match(bounded, /\[1 item omitted for length\]/);
});

test("the site settings file survives a hostile payload", async () => {
  const os = await import("node:os");
  const path = await import("node:path");
  const fs = await import("node:fs");
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pi-staff-sites-"));
  assert.deepEqual(sites.readSiteSettings(dir), {});

  const file = sites.siteSettingsPath(dir);
  fs.writeFileSync(file, "{ not json");
  assert.deepEqual(sites.readSiteSettings(dir), {});

  fs.writeFileSync(file, "[1,2,3]");
  assert.deepEqual(sites.readSiteSettings(dir), {});

  sites.writeSiteSettings(dir, { zhihu: { seedUrl: "https://www.zhihu.com/hot", maxUrls: 1 } });
  assert.equal(sites.readSiteSettings(dir).zhihu.seedUrl, "https://www.zhihu.com/hot");
});

test("a published link names the site the piece is on", () => {
  // The label a draft was filed under says what kind of wording it is; the
  // address says who hosts it. Reading the label as a site is how a piece
  // that really is out there came back as "no such site".
  assert.equal(sites.siteByUrl("https://zhuanlan.zhihu.com/p/1997658098362508695"), "zhihu");
  assert.equal(sites.siteByUrl("https://www.zhihu.com/question/1/answer/2"), "zhihu");
  // A subdomain counts, a lookalike domain does not.
  assert.equal(sites.siteByUrl("https://notzhihu.com/p/1"), null);
  assert.equal(sites.siteByUrl("https://zhihu.com.evil.test/p/1"), null);
  // Only https, and only a real address.
  assert.equal(sites.siteByUrl("http://zhihu.com/p/1"), null);
  assert.equal(sites.siteByUrl("javascript:alert(1)"), null);
  assert.equal(sites.siteByUrl(""), null);
  assert.equal(sites.siteByUrl("not a url"), null);
});

test("every site that can be read names the hosts it answers for", () => {
  for (const channel of STAFF_SOURCE_CHANNELS) {
    const spec = sites.siteSpec(channel);
    assert.ok(Array.isArray(spec.hosts), channel + " has no host list");
    // A collector without a host can never recognise its own pages.
    assert.equal(spec.hosts.length > 0, spec.supported, channel);
  }
});

test("the injected page script is valid JavaScript", () => {
  assert.doesNotThrow(() => new Function(sites.ZHIHU_PAGE_SCRIPT));
  assert.match(sites.ZHIHU_PAGE_SCRIPT, /document\.querySelectorAll/);
  assert.match(sites.ZHIHU_PAGE_SCRIPT, /location\.pathname/);
});
