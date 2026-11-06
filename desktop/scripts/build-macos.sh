#!/usr/bin/env bash
#
# Build an installable macOS application: pigo-desktop.app plus the
# pigo-desktop_<version>_<arch>.dmg that wraps it.
#
# This lane must run on macOS. The Tauri bundler shells out to macOS-only
# tooling (codesign, hdiutil, iconutil) and the Rust host-core links against
# the macOS SDK, so the bundle cannot be produced from Windows or Linux.
#
# Usage:
#   scripts/build-macos.sh
#
# Environment:
#   MAC_ARCH                  arm64 (Apple silicon) or x64 (Intel). Defaults to
#                             the host and must match it: the Rust host-core, the
#                             Go agent sidecar and the webview all have to be the
#                             same architecture, so each one is built natively.
#   APPLE_SIGNING_IDENTITY    Optional "Developer ID Application: <Name> (<TEAMID>)".
#                             When unset the app is built unsigned, which is the
#                             default lane.
#   APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID
#                             Optional notarization credentials. Tauri notarizes
#                             and staples the app when all three are set; they
#                             only take effect together with a signing identity.
#                             APPLE_APP_SPECIFIC_PASSWORD is accepted as a
#                             legacy alias for APPLE_PASSWORD.
#   APPLE_API_KEY, APPLE_API_ISSUER, APPLE_API_KEY_PATH
#                             App Store Connect API-key alternative to the
#                             Apple ID credentials.
#   TAURI_CLI                 Override the Tauri CLI invocation, e.g.
#                             TAURI_CLI="pnpm dlx @tauri-apps/cli@^2".
#
# Both backends are copied into Contents/Resources/bin by bundle.macOS.files.
# The Tauri bundler does not add those files to its own signing list, so this
# lane signs them before bundling: the bundler copies the signed Mach-O files
# and then seals them with the app signature. Without that step notarization
# rejects the unsigned nested executables.
#
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

step() { printf '\n==> %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || die "this lane builds macOS bundles and must run on macOS (host is $(uname -s))"

# --- architecture ----------------------------------------------------------
case "$(uname -m)" in
  arm64)  host_arch="arm64" ;;
  x86_64) host_arch="x64" ;;
  *) die "unsupported macOS host architecture: $(uname -m)" ;;
esac

if [ -n "${MAC_ARCH:-}" ] && [ "$MAC_ARCH" != "$host_arch" ]; then
  die "MAC_ARCH=$MAC_ARCH does not match this $host_arch host. Build each architecture on its own machine so the Rust host-core stays native."
fi
arch="$host_arch"

# --- prerequisites ---------------------------------------------------------
for tool in cargo go node pnpm; do
  command -v "$tool" >/dev/null 2>&1 || die "missing required tool: $tool"
done
xcode-select -p >/dev/null 2>&1 || die "Xcode command line tools are required; run: xcode-select --install"

if [ ! -d node_modules ]; then
  step "Installing workspace dependencies"
  pnpm install --frozen-lockfile
fi

printf 'Building pigo-desktop for macOS %s\n' "$arch"
if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
  printf 'Signing identity: %s\n' "$APPLE_SIGNING_IDENTITY"
else
  printf 'Signing identity: none (unsigned build)\n'
fi

# Tauri reads APPLE_PASSWORD; keep the older name working for existing lanes.
if [ -z "${APPLE_PASSWORD:-}" ] && [ -n "${APPLE_APP_SPECIFIC_PASSWORD:-}" ]; then
  export APPLE_PASSWORD="$APPLE_APP_SPECIFIC_PASSWORD"
fi

# --- backend binaries ------------------------------------------------------
# tauri.conf.json ships these into Contents/Resources/bin.
step "Building the Rust host-core (release)"
pnpm build:host

step "Building the Go agent sidecar"
pnpm build:agent

# --- renderer --------------------------------------------------------------
step "Building the renderer and workspace packages"
pnpm build:js

host_bin="target/release/pi-desktop-host-core"
agent_bin="sidecar/build/pi-go-desktop-agent"
for bin in "$host_bin" "$agent_bin"; do
  [ -f "$bin" ] || die "expected backend binary was not produced: $bin"
  [ -x "$bin" ] || die "backend binary is not executable: $bin"
done

# Sign the nested binaries before the bundler copies them: the bundler signs
# the app but not files copied through bundle.macOS.files, and notarization
# validates every Mach-O in the bundle.
if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
  step "Signing backend binaries"
  for bin in "$host_bin" "$agent_bin"; do
    codesign --force --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$bin"
    codesign --verify --strict --verbose=2 "$bin"
  done
fi

# --- bundle ----------------------------------------------------------------
if [ -n "${TAURI_CLI:-}" ]; then
  read -r -a tauri_cli <<<"$TAURI_CLI"
elif cargo tauri --version >/dev/null 2>&1; then
  tauri_cli=(cargo tauri)
else
  # No Tauri CLI in the workspace: use the published one without touching the
  # lockfile. `tauri build` enables tauri/custom-protocol on its own, so the
  # packaged app loads the bundled renderer instead of the dev server.
  tauri_cli=(pnpm dlx "@tauri-apps/cli@^2")
fi

step "Bundling the .app and .dmg"
(cd apps/desktop-tauri && "${tauri_cli[@]}" build)

# --- verify bundle ---------------------------------------------------------
cd "$root"
step "Verifying the bundled backends"
app_bundle="$(find target/release/bundle/macos -maxdepth 1 -name '*.app' -print -quit 2>/dev/null || true)"
[ -n "$app_bundle" ] || die "the bundler did not produce a .app"

for bin in pi-desktop-host-core pi-go-desktop-agent; do
  binary="$app_bundle/Contents/Resources/bin/$bin"
  [ -x "$binary" ] || die "backend binary missing from the bundle: $binary"
  if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
    codesign --verify --strict --verbose=2 "$binary"
  fi
done

if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
  codesign --verify --deep --strict --verbose=2 "$app_bundle"
fi

notarized=0
if [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_PASSWORD:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ]; then
  notarized=1
fi
if [ -n "${APPLE_API_KEY:-}" ] && [ -n "${APPLE_API_ISSUER:-}" ] && [ -n "${APPLE_API_KEY_PATH:-}" ]; then
  notarized=1
fi
if [ "$notarized" = 1 ]; then
  xcrun stapler validate "$app_bundle"
  dmg_bundle="$(find target/release/bundle/dmg -maxdepth 1 -name '*.dmg' -print -quit 2>/dev/null || true)"
  if [ -n "$dmg_bundle" ]; then
    xcrun stapler validate "$dmg_bundle"
  fi
  spctl --assess --type execute --verbose=2 "$app_bundle"
fi

# --- report ----------------------------------------------------------------
step "Artifacts"
find target/release/bundle -maxdepth 3 \( -name '*.app' -o -name '*.dmg' \) -print 2>/dev/null || true

if [ -z "${APPLE_SIGNING_IDENTITY:-}" ]; then
  cat <<'NOTE'

This build is unsigned. macOS quarantines a downloaded unsigned app, so a user
who copies it from another machine may have to clear the attribute once:

  xattr -r -d com.apple.quarantine /Applications/pigo-desktop.app

An app built and opened on the same machine is not quarantined and needs no
extra step.
NOTE
fi
