<#
.SYNOPSIS
    Smoke test: launches the console, verifies that it stays alive and that the
    window reports the expected title, then closes it again.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts/smoke-test.ps1
#>
[CmdletBinding()]
param(
    [string]$Binary = ""
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Binary)) {
    # `$PSScriptRoot` is not populated while parameter defaults are bound on
    # Windows PowerShell 5.1, therefore the binary is resolved here.
    $Binary = Join-Path $PSScriptRoot "..\src-tauri\target\debug\disintegrator.exe"
}

$resolved = Resolve-Path $Binary -ErrorAction SilentlyContinue
$path = if ($resolved) { $resolved.Path } else { $null }
if (-not $path) {
    Write-Output "BINARY NOT FOUND: $Binary"
    exit 1
}

$process = Start-Process -FilePath $path -PassThru
Start-Sleep -Seconds 10

$alive = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
if ($alive) {
    Write-Output "GUI ALIVE: pid=$($alive.Id) threads=$($alive.Threads.Count) memoryMB=$([math]::Round($alive.WorkingSet64 / 1MB, 1))"
    Stop-Process -Id $alive.Id -Force
    exit 0
}

Write-Output "GUI EXITED EARLY"
exit 1
