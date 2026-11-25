# Build ATOM's Windows installers (.msi + .exe) from source.
#
# Requirements:
#   - Rust for MSVC (https://rustup.rs) with the "Desktop development with C++"
#     workload from Visual Studio Build Tools
#   - Bun (https://bun.sh) and Node.js on PATH
#   - WebView2 runtime (preinstalled on Windows 10/11; the installer bootstraps
#     it otherwise)
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts\build-desktop.ps1

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$bundles = if ($args.Count -ge 1) { $args[0] } else { "nsis,msi" }

Write-Host "==> Building the interface"
Push-Location "$root\frontend"
try {
    bun install --frozen-lockfile
    bun run build
} finally {
    Pop-Location
}

Write-Host "==> Compiling the desktop app and bundling: $bundles"
Push-Location "$root\apps\desktop"
try {
    npx --yes "@tauri-apps/cli@2" build --bundles $bundles
} finally {
    Pop-Location
}

Write-Host ""
Write-Host "Artifacts in apps\desktop\target\release\bundle:"
Get-ChildItem -Path "$root\apps\desktop\target\release\bundle" -Recurse -File |
    Where-Object { $_.Extension -in ".msi", ".exe" } |
    ForEach-Object { "  $($_.FullName)" }
