# One-shot Windows build for pigo-desktop.
# Run from a NORMAL terminal (not from inside the WorkBuddy agent session):
#   powershell -NoProfile -ExecutionPolicy Bypass -File build-windows-local.ps1
# or just double-click build-windows-local.cmd
param(
    [string]$Bundles = "nsis"
)

$ErrorActionPreference = "Stop"
$Root = "D:\Workspace\golang\pigo-desktop\desktop"
$Log  = Join-Path $Root "build-windows-local.log"
Set-Location $Root
Set-Content -Path $Log -Value "Build started $(Get-Date -Format o)" -Encoding UTF8

function Step([string]$Message) {
    Write-Host "`n==> $Message" -ForegroundColor Cyan
    Add-Content -Path $Log -Value "`n==> $Message" -Encoding UTF8
}

function Fail([string]$Message) {
    Write-Host "`nERROR: $Message" -ForegroundColor Red
    Add-Content -Path $Log -Value "ERROR: $Message" -Encoding UTF8
    Write-Host "Full log: $Log" -ForegroundColor Yellow
    exit 1
}

# ---------- toolchain ----------
$CargoBin = "C:\Users\Administrator\.cargo\bin"
$GoBin    = "D:\go\go-1.27.3\bin"
$env:PATH = "$CargoBin;$GoBin;" + $env:PATH
$env:RUSTUP_TOOLCHAIN = "stable-x86_64-pc-windows-msvc"

$VcVars = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if (Test-Path $VcVars) {
    Step "使用官方 VS Build Tools (MSVC + Windows SDK)"
    # Import the environment produced by vcvars64.bat into this process.
    $lines = & cmd.exe /c "`"$VcVars`" >nul && set"
    foreach ($line in $lines) {
        if ($line -match '^([^=]+)=(.*)$') {
            [Environment]::SetEnvironmentVariable($Matches[1], $Matches[2], "Process")
        }
    }
} else {
    Step "官方 VS 未安装完成，使用 .msvc 自建最小工具链"
    $Mini = Join-Path $Root ".msvc"
    # Rust compilation does not need the C SDK headers; it only needs the
    # linker/import libs. The SQLite bundled-C path is bypassed via .cargo/config.toml
    # (LIBSQLITE3_SYS_USE_PKG_CONFIG=1), so windows.h is NOT required.
    foreach ($required in @("bin\cl.exe", "bin\link.exe", "bin\lib.exe", "lib\sqlite3.lib")) {
        if (-not (Test-Path (Join-Path $Mini $required))) {
            Fail "自建工具链缺少 $required。请先完成 VS Build Tools 的 C++ 工具链安装（UAC 弹窗点“是”），或运行：setup.exe modify --installPath `"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools`" --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --quiet --norestart"
        }
    }
    $env:PATH = "$Mini\bin;" + $env:PATH
    $env:LIB = "$Mini\lib"
    $env:INCLUDE = "$Mini\include"
    $env:CC_x86_64_pc_windows_msvc = "$Mini\bin\cl.exe"
    $env:AR_x86_64_pc_windows_msvc = "$Mini\bin\lib.exe"
}

# Native tools write warnings to stderr, and PowerShell 5.1 turns a merged
# stderr line into a terminating NativeCommandError under the "Stop" preference
# above — one Rust warning would abort the whole build. Every native call below
# is judged by $LASTEXITCODE instead, so relax the preference from here on.
$ErrorActionPreference = "Continue"

# ---------- 1/4 Rust host-core ----------
Step "[1/4] cargo build --release -p host-core"
& cargo.exe build --release -p host-core 2>&1 | Tee-Object -FilePath $Log -Append
if ($LASTEXITCODE -ne 0) {
    Fail "host-core 编译失败 (exit $LASTEXITCODE)。若日志出现 'os error 5 / 拒绝访问'，说明安全软件拦截了 build-script 启动子进程：请退出 360 主动防御，或把 $Root\target 加入白名单后重试。"
}
$HostExe = Join-Path $Root "target\release\pi-desktop-host-core.exe"
if (-not (Test-Path $HostExe)) { Fail "未生成 $HostExe" }

# ---------- 2/4 Go sidecar ----------
$AgentExe = Join-Path $Root "sidecar\build\pi-go-desktop-agent.exe"
if (-not (Test-Path $AgentExe)) {
    Step "[2/4] 构建 Go sidecar"
    & node.exe "scripts\build-agent.mjs" 2>&1 | Tee-Object -FilePath $Log -Append
    if ($LASTEXITCODE -ne 0) { Fail "sidecar 构建失败 (exit $LASTEXITCODE)" }
} else {
    Step "[2/4] Go sidecar 已存在，跳过"
}

# ---------- 3/4 renderer ----------
$Dist = Join-Path $Root "apps\desktop-tauri\dist\index.html"
if (-not (Test-Path $Dist)) {
    Step "[3/4] 构建前端渲染器"
    & pnpm.cmd --filter "@pi-desktop/desktop..." build 2>&1 | Tee-Object -FilePath $Log -Append
    if ($LASTEXITCODE -ne 0) { Fail "前端构建失败 (exit $LASTEXITCODE)" }
} else {
    Step "[3/4] 前端 dist 已存在，跳过"
}

# ---------- 3.5/4 Windows resources ----------
# The minimal .msvc toolchain has no mt.exe / rc.exe, so tauri-winres is
# patched out (.vendor/tauri-build). Without an RT_MANIFEST the loader binds
# comctl32 v5 and the exe dies at startup with 0xC0000139
# ("TaskDialogIndirect not found"). Inject the manifest + window icon directly
# into the PE resource section instead.
Step "[3.5/4] 注入 Windows 资源 (Common-Controls v6 manifest + 窗口图标)"
$MainExe  = Join-Path $Root "target\release\pigo-desktop.exe"
$Manifest = Join-Path $Root "apps\desktop-tauri\src-tauri\pigo-desktop.exe.manifest"
$Icon     = Join-Path $Root "apps\desktop-tauri\build\icon.ico"
$Embed    = Join-Path $Root "scripts\embed-manifest.py"
foreach ($f in @($Manifest, $Icon, $Embed)) {
    if (-not (Test-Path $f)) { Fail "资源注入所需文件不存在：$f" }
}
$Python = (Get-Command python -ErrorAction SilentlyContinue).Source
if (-not $Python) { $Python = (Get-Command py -ErrorAction SilentlyContinue).Source }
if (-not $Python) { Fail "未找到 python.exe / py.exe，Windows 资源注入需要 Python 3" }

# ---------- 3.5/4 + 4/4 inject, bundle, verify ----------
# `tauri build` compiles the crate with its own environment, so it relinks
# whenever the renderer bundle changed — and that relink wipes the injected
# .rsrc. There is no separate cargo invocation that leaves it idle. So: inject,
# bundle, check; if the link ate the resources, inject the freshly linked
# binary and bundle once more (the second pass finds cargo already up to date).
$BundledWithResources = $false
for ($attempt = 1; $attempt -le 2; $attempt++) {
    if ($attempt -eq 1) {
        Step "[3.5/4] 注入 Windows 资源 (Common-Controls v6 manifest + 窗口图标)"
    } else {
        Step "[3.5/4] 资源被重链接覆盖，重新注入后再次打包（第 2 次）"
        # A relink produced a newer binary, so the saved pristine copy is an
        # older build; the injector restores <exe>.bak first, which would ship
        # the previous version. Drop it so the current binary is the baseline.
        Remove-Item "$MainExe.bak" -Force -ErrorAction SilentlyContinue
    }
    & $Python $Embed $MainExe $Manifest --icon $Icon 2>&1 | Tee-Object -FilePath $Log -Append
    if ($LASTEXITCODE -ne 0) { Fail "Windows 资源注入失败 (exit $LASTEXITCODE)" }

    Step "[4/4] Tauri 打包 (bundles: $Bundles)  -- 首次会下载 NSIS，请耐心等待"
    Push-Location (Join-Path $Root "apps\desktop-tauri")
    try {
        & pnpm.cmd dlx "@tauri-apps/cli@^2" build --bundles $Bundles 2>&1 | Tee-Object -FilePath $Log -Append
        if ($LASTEXITCODE -ne 0) { Fail "Tauri 打包失败 (exit $LASTEXITCODE)" }
    } finally {
        Pop-Location
    }

    # ---------- verify resources survived the bundle ----------
    $check = (& $Python $Embed $MainExe $Manifest --check 2>&1) -join "`n"
    Add-Content -Path $Log -Value $check -Encoding UTF8
    if ($check -match 'HAS resource') {
        $BundledWithResources = $true
        break
    }
    Write-Host "打包阶段 cargo 重新链接覆盖了 .rsrc，本次安装包不可用，正在重试…" -ForegroundColor Yellow
    Add-Content -Path $Log -Value "retry: .rsrc lost during bundle" -Encoding UTF8
}
if (-not $BundledWithResources) {
    Fail "两次尝试后 .rsrc 仍被覆盖（会复现 TaskDialogIndirect 错误）。请检查 target/release/pigo-desktop.exe 是否有安全软件改写。"
}

# ---------- result ----------
$BundleRoot = Join-Path $Root "target\release\bundle"
$Installers = Get-ChildItem -Path $BundleRoot -Recurse -Include *.msi,*.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\bundle\\(nsis|msi)\\' }
if (-not $Installers) { Fail "未在 $BundleRoot 下找到安装包" }

Step "构建完成，安装包："
$Installers | ForEach-Object {
    $line = "  {0}  ({1:N1} MB)" -f $_.FullName, ($_.Length / 1MB)
    Write-Host $line -ForegroundColor Green
    Add-Content -Path $Log -Value $line -Encoding UTF8
}
Write-Host "`nFull log: $Log" -ForegroundColor Yellow
