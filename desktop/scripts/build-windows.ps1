# Build an installable Windows application (NSIS/MSI per tauri.conf targets).
#
# Must run on Windows: the Rust host-core, the Go sidecar, and the WebView2
# window all have to be native. The macOS lane lives in build-macos.sh.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-windows.ps1
#
# Environment:
#   TAURI_CLI   Override the Tauri CLI invocation, e.g. "pnpm dlx @tauri-apps/cli@^2".
#
# Both backends are copied into bin/ next to pigo-desktop.exe by
# tauri.windows.conf.json, which is where boot.rs looks for them.

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Step([string]$Message) {
    Write-Host "`n==> $Message"
}

function Fail([string]$Message) {
    Write-Host "error: $Message" -ForegroundColor Red
    exit 1
}

foreach ($tool in @("cargo", "go", "node", "pnpm")) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        Fail "missing required tool: $tool"
    }
}

if (-not (Test-Path (Join-Path $root "node_modules"))) {
    Step "Installing workspace dependencies"
    & pnpm install --frozen-lockfile
    if ($LASTEXITCODE -ne 0) { Fail "pnpm install failed" }
}

Step "Building the Rust host-core (release)"
& pnpm build:host
if ($LASTEXITCODE -ne 0) { Fail "host-core build failed" }

Step "Building the Go agent sidecar"
& pnpm build:agent
if ($LASTEXITCODE -ne 0) { Fail "agent sidecar build failed" }

Step "Building the renderer and workspace packages"
& pnpm build:js
if ($LASTEXITCODE -ne 0) { Fail "renderer build failed" }

Step "Checking backend binaries"
$hostBinary = Join-Path $root "target/release/pi-desktop-host-core.exe"
$agentBinary = Join-Path $root "sidecar/build/pi-go-desktop-agent.exe"
foreach ($binary in @($hostBinary, $agentBinary)) {
    if (-not (Test-Path $binary)) { Fail "expected backend binary was not produced: $binary" }
}

Step "Bundling the installer"
if ($env:TAURI_CLI) {
    $parts = $env:TAURI_CLI.Split(" ")
    $exe = $parts[0]
    $cliArgs = @()
    if ($parts.Length -gt 1) { $cliArgs = $parts[1..($parts.Length - 1)] }
    Push-Location (Join-Path $root "apps/desktop-tauri")
    try {
        & $exe @cliArgs build
        if ($LASTEXITCODE -ne 0) { Fail "tauri build failed" }
    } finally {
        Pop-Location
    }
} else {
    $hasCargoTauri = $null -ne (Get-Command cargo-tauri -ErrorAction SilentlyContinue)
    Push-Location (Join-Path $root "apps/desktop-tauri")
    try {
        if ($hasCargoTauri) {
            & cargo tauri build
        } else {
            & pnpm dlx "@tauri-apps/cli@^2" build
        }
        if ($LASTEXITCODE -ne 0) { Fail "tauri build failed" }
    } finally {
        Pop-Location
    }
}

Step "Verifying the installer"
$bundleRoot = Join-Path $root "target/release/bundle"
$installers = Get-ChildItem -Path $bundleRoot -Recurse -Include *.msi, *.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match "\\bundle\\(nsis|msi)\\" }
if (-not $installers) { Fail "no Windows installer found under $bundleRoot" }

Step "Artifacts"
$installers | Select-Object -ExpandProperty FullName

Write-Host @"

This lane does not sign the installer. Authenticode signing requires a
certificate configured in bundle.windows (certificateThumbprint or
signCommand) or in CI; unsigned installers trigger SmartScreen.
"@
