<#
.SYNOPSIS
    Collects the DISINTEGRATOR release artefacts and their SHA256 checksums.

.DESCRIPTION
    The script stages the portable executable and the NSIS installer into
    `release-artifacts/` and writes a `SHA256SUMS.txt` manifest that can be
    verified with `sha256sum -c` or `Get-FileHash`.

.PARAMETER Version
    The release version. Defaults to the version inside `package.json`.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts/collect-release.ps1
#>
[CmdletBinding()]
param(
    [string]$Version = "",
    [string]$Root = (Split-Path -Parent $PSScriptRoot)
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = (Get-Content (Join-Path $Root "package.json") | ConvertFrom-Json).version
}

$stage = Join-Path $Root "release-artifacts"
New-Item -ItemType Directory -Force -Path $stage | Out-Null

$portable = Join-Path $Root "src-tauri\target\release\disintegrator.exe"
if (-not (Test-Path $portable)) {
    throw "portable executable not found - run 'npm run tauri:build' first"
}
Copy-Item $portable (Join-Path $stage "Disintegrator_v${Version}_x64.exe") -Force

$installer = Get-ChildItem (Join-Path $Root "src-tauri\target\release\bundle\nsis") -Filter "*-setup.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($installer) {
    Copy-Item $installer.FullName (Join-Path $stage "Disintegrator_v${Version}_x64-setup.exe") -Force
} else {
    Write-Warning "NSIS installer not found - only the portable executable was staged"
}

$checksums = Get-ChildItem $stage -File | Where-Object { $_.Name -ne "SHA256SUMS.txt" } | ForEach-Object {
    $hash = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
    "$hash  $($_.Name)"
}
$checksums | Out-File -FilePath (Join-Path $stage "SHA256SUMS.txt") -Encoding utf8

Write-Host "Release artefacts staged in $stage"
Get-ChildItem $stage | Select-Object Name, Length, LastWriteTime
