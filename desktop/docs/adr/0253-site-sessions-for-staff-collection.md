# 0253. Site sessions: a real signed-in browser instead of a forged request

* Status: Accepted for implementation

## Context

The staff employees are supposed to find demand on external platforms. The
first sweep attempt collected nothing, and the reason was not the model: the
sites put their content behind a login.

That was measured on 2026-09-20 against Zhihu with a fresh Chromium profile:

* `https://www.zhihu.com/hot` redirects to `/signin?next=%2Fhot`, so an
  anonymous fetch returns the login wall as a 200 response.
* The anonymous session receives `_xsrf`, `__zse_ck`, `_zap`, `d_c0`,
  `SESSIONID`, `JOID` and Baidu counters -- none of which unlock content.
* `z_c0`, the cookie a real login sets, is absent.
* The JSON endpoints reject even the page's own origin: `/api/v3/feed/topstory/
  hot-lists/total` answers 401 and `/api/v4/questions/<id>/answers` answers
  403, because both require the `x-zse-96` signature that only the site's own web
  bundle computes.

So the choice was not "which header recipe gets the data". It was whether this
product is willing to defeat a site's protection at all, or whether it reads
the site the way the owner's own browser does.

## Decision

### 1. One signed-in Chromium partition per site, owned by Electron main

Each supported site gets `persist:pi-staff-site-<channel>`. The login cookie is
the session; it stays in Chromium's store and is never copied into
host-core's `SecretStore`, never returned over IPC, and never logged.

### 2. The owner signs in on the site's own page

Signing in opens the site's sign-in URL in its own window, so the owner scans
the site's QR code with the site's own app. No password is typed into this
application and no credential is imported. The window closes itself once the
session carries the site's login cookie, and a timeout ends it otherwise.

The window can be raised from two places. The console's site panel is the
deliberate one. The other is a manual run: a sweep that reaches a supported
site with no session opens the sign-in window and waits, because the owner is
at the console and asked for that sweep, and a sweep with no session reads
nothing. A scheduled run never opens a window -- at 09:30 there is nobody to
scan a code -- and reports the site as skipped instead. The first version
raised it from the panel only, which left an owner who started a run waiting
for a window that never came and a run that collected nothing.

### 3. The collector reads rendered pages; it does not forge requests

A sweep loads the page inside the site's partition and reads the DOM. The
collector does not compute or replay the site's request signature, does not
solve captchas, and does not spoof a fingerprint. It removes Chromium's own
Electron token from the user agent, because that token names a client the site
has no rules for; nothing else about the request is altered.

### 4. A missing session is a reported skip, not an empty result

A probe step reports a configured but signed-out site as skipped, in the
step's own text. An unattended run must never let "we could not look" read as
"there was nothing there". A site that has no collector is treated
differently: it is a gap in the product, not a finding about this sweep, so
it stays out of the step. The console lists it as unsupported, and one
repeated line per unimplemented channel on every run would spend the model's
context on the same sentence.

### 5. The owner sets the entry point and the per-run cap

Site settings live in `staff-sites.json` in the data directory: a seed URL
(http(s) only) and a page cap that defaults to one page and is clamped to 20.
Login state is derived from cookies on every read rather than stored.

## Consequences

* No storage-schema change: sessions live in Chromium partitions, and settings
  live in a JSON file beside the database.
* The renderer gets five new channels (`staff.sites.*`). None of them can
  read a cookie or a page body; they list state, start a sign-in window, clear
  a partition, and run one manual collection for review.
* A scheduled sweep now depends on an interactive action the owner performed
  once. That is a real operational coupling and it is deliberate: the
  alternative is an app that signs in on the owner's behalf.
* Sessions expire. A sweep that finds an expired session reports a skip, and
  the console shows the site as signed out so the owner can re-scan.
* Only Zhihu has a collector. The other five channels are listed as
  unsupported, and the site registry is where the next one is added.
* A step that was handed collected pages is told to work from them and not
  to fetch those pages with a command-line client. The first sweep spent its
  turn on such fetches and ended with less than the driver was holding.

## Alternatives

* **Replay the site's API with a forged signature.** Rejected: it is a
  protection measure, it breaks whenever the site rotates its algorithm, and a
  product that ships one is shipping an exploit.
* **Ask the owner to paste cookies or a token.** Rejected: it puts a
  credential into this app's configuration and its backups, and it silently
  breaks on rotation.
* **Drive the owner's installed browser through a debug port.** Rejected: it
  reaches outside this application's data directory and its consent boundary.
* **Store the login state in host-core.** Rejected: cookies belong to the
  Chromium session that uses them; copying them into the database would create
  a second, staler copy of a credential.
* **Headless requests with a browser user agent.** Rejected by measurement:
  the redirect and the 401/403 are the same for any client without the
  signature, so the only honest client is the rendered browser.

## Validation

`apps/desktop/test/manual/staff-site-zhihu-live.cjs` (removed during the Tauri
migration) ran the collector's own modules inside Electron against the live
site, in two modes. Run anonymously
on 2026-09-20 it passed 12 checks: the sweep starts from the hot list, an
unsigned session is refused with a message rather than an empty page, an
anonymous visit to `/hot` lands on `/signin?next=%2Fhot` with no `z_c0` in
the cookie set, and the sign-in page really does render a code to scan (one
canvas at or above 80 px, beside the wording "打开知乎App ... 打开扫一扫"). The
second mode points the script at the console's own profile and asserts that a
scanned session returns items; it needs the owner, so it is not part of the
unattended suite.
