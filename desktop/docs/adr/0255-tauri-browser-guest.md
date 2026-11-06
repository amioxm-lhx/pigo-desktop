# ADR 0255: Host-owned browser chrome over a Tauri child webview

- Status: Accepted for implementation
- Date: 2026-11-05
- Related: [ADR 0170](0170-work-panel-browser-as-bundled-plugin.md),
  [ADR 0142](0142-allow-non-loopback-http-mcp.md),
  [Plugin API](../spec/07-plugins/03-plugin-api.md),
  [IPC protocol](../spec/03-runtime/01-ipc-protocol.md)

## Context

ADR 0170 split the work-panel browser in two: the chrome is a bundled
`pi.browser` plugin view, and the page itself is a host-owned
`WebContentsView` that the chrome positions through `pi.browser.setBounds`.
That split relies on Electron APIs (`WebContentsView`, `persist:` partition,
and a real Chrome DevTools Protocol debugger).

The Tauri port has none of those. It also does not ship the bundled
`pi.browser` plugin yet, so `browser/navigate|action|getState` returned
`UNSUPPORTED` and `setBounds|setVisible` were no-ops: a link opened from chat
fell back to the system browser and there was no guest to inspect.

Tauri can host more than one webview per window, but only through the
`unstable` multiwebview API (`Window::add_child`). WKWebView and WebKitGTK have
no Chrome DevTools Protocol; the only scriptable surface is
`evaluateJavaScript` with a completion handler.

## Decision

1. **One host-owned child webview.** `browser.rs` creates `work-browser` lazily
   as a child of the `main` window, hidden at 1×1 until a page is loaded.
   Navigation allows `http`, `https`, and `about:blank` only; a workspace
   `.html`/`.htm` path resolves through the existing loopback preview service.
2. **Chrome is host-rendered in this port.** The `pi.browser/browser` panel tab
   renders `BrowserHostTab` (address bar, back, forward, reload/stop, open
   external, error row) and reports its surface rect to the shell. The tab
   identity is unchanged, so links, preview routing, and panel contexts keep
   working without a bundled plugin. This supersedes ADR 0170 clause 1 for the
   Tauri port only.
3. **Bounds are clamped by the host.** `browser/setBounds` carries the hole and
   optionally the chrome's own view rect; the shell intersects them with each
   other and with the window client area. A hole with no visible area hides the
   guest instead of clamping to a sliver.
4. **`pi.browser.*` stays public.** Every method crosses the panel bridge,
   requires `browser.cdp`, and shares the same singleton guest. Plugin calls
   are bounds-clamped to the window because a plugin view does not report its
   own rect to this shell.
5. **CDP is an evaluation-backed allowlist.** `Runtime.evaluate` and
   `Accessibility.getFullAXTree` are implemented over script evaluation;
   `Page.captureScreenshot` uses the platform snapshot bridge described below;
   cookies, storage, target, fetch-interception, and `Page.navigate` answer
   `PERMISSION_DENIED`. Snapshot uids are `data-pi-uid` handles assigned by
   `pi.browser.snapshot`, and `click`/`fill` reject anything but `e[0-9]+`.
6. **Lifecycle.** Switching tabs hides the guest (a native child webview cannot
   be covered by CSS); the next browser tab shows it and re-navigates only when
   its location differs. The DOM is not torn down per tab, matching the
   singleton semantics of ADR 0170 §6.

## Consequences

- Opening an http(s) link in the work panel now shows the real page instead of
  always leaving the app.
- The host draws the chrome, so a future bundled `pi.browser` plugin can replace
  `BrowserHostTab` without changing the IPC or `pi.browser.*` contracts.
- `pi.browser.screenshot` and CDP `Page.captureScreenshot` are served from
  WKWebView's `takeSnapshot:withConfiguration:completionHandler:` on macOS and
  return PNG base64. Full-page capture is refused with `UNSUPPORTED` (it would
  need stitched scroll captures), and a platform without an equivalent snapshot
  bridge keeps answering `UNSUPPORTED`.
- `evaluate` is synchronous: promises are not awaited, and a page's CSP can
  reject `eval`. Snapshot/click/fill do not use `eval` and keep working.
- Relying on Tauri's `unstable` feature is a build-time dependency; the pin is
  `tauri` 2.12.5, and the feature is required for child webviews.

## Alternatives

### A separate always-on-top window as the guest

Rejected. It behaves like a separate window in Mission Control and window
switching, and cannot be clipped to the panel reliably.

### Keep the renderer iframe

Rejected. Cross-origin pages cannot be inspected, clicked, or evaluated from
the renderer, so `pi.browser.*` would remain unimplemented.

### Decline CDP entirely

Rejected. Snapshot/click/fill/evaluate are the reason the API exists; an
evaluation-backed subset preserves the useful part while keeping the
deny-by-default boundary.
