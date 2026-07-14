# build.ps1 — Build Photyx (no installer bundle) and launch it if the build succeeds.
# Place this file in the project root (J:\github\Photyx\) and run it from there:
#   .\build.ps1

$ErrorActionPreference = "Stop"

# ── vcpkg paths (cfitsio) ──────────────────────────────────────────────────
$env:PKG_CONFIG_PATH = "J:\vcpkg\installed\x64-windows\lib\pkgconfig"
$env:PATH = "J:\vcpkg\installed\x64-windows\tools\pkgconf;J:\vcpkg\installed\x64-windows\bin;$env:PATH"

# ── Project root = folder this script lives in ──────────────────────────────
$ProjectRoot = $PSScriptRoot
Set-Location $ProjectRoot

Write-Host "== Photyx build (no bundle) ==" -ForegroundColor Cyan
Write-Host "Project root:    $ProjectRoot"
Write-Host "PKG_CONFIG_PATH: $env:PKG_CONFIG_PATH"
Write-Host ""

# ── Build ────────────────────────────────────────────────────────────────────
npm run tauri build -- --no-bundle
$buildExitCode = $LASTEXITCODE

if ($buildExitCode -ne 0) {
    Write-Host ""
    Write-Host "Build failed (exit code $buildExitCode). Not launching." -ForegroundColor Red
    exit $buildExitCode
}

Write-Host ""
Write-Host "Build succeeded." -ForegroundColor Green

# ── Locate the built executable ───────────────────────────────────────────────
# Checks both possible target locations, since workspace layout can put it
# at the project root or under src-tauri, depending on config.
$candidates = @(
    (Join-Path $ProjectRoot "target\release\photyx.exe"),
    (Join-Path $ProjectRoot "src-tauri\target\release\photyx.exe")
)

$exePath = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1

if (-not $exePath) {
    Write-Host "Build reported success, but photyx.exe was not found in either expected location:" -ForegroundColor Red
    $candidates | ForEach-Object { Write-Host "  $_" }
    exit 1
}

Write-Host "Launching: $exePath" -ForegroundColor Cyan
Start-Process -FilePath $exePath

# ----------------------------------------------------------------------
# ----------------------------------------------------------------------
# ----------------------------------------------------------------------
