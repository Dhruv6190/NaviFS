<#
.SYNOPSIS
NaviFS Universal 1-Click Plug & Play Installer for Windows.
Installs navifs.exe to %USERPROFILE%\.navifs\bin, updates PATH, and auto-configures AI agents.
#>

param(
    [switch]$All,
    [string]$Client
)

$ErrorActionPreference = "Stop"

Write-Host "========================================================" -ForegroundColor Cyan
Write-Host "       NaviFS Universal Plug & Play Installer           " -ForegroundColor Cyan
Write-Host "========================================================" -ForegroundColor Cyan

$NaviDir = Join-Path $HOME ".navifs\bin"
if (-not (Test-Path $NaviDir)) {
    New-Item -ItemType Directory -Path $NaviDir -Force | Out-Null
}

$DestExe = Join-Path $NaviDir "navifs.exe"

# 1. Locate binary (local build or repo artifact)
$LocalCandidate = "C:\Users\dhruv\.cargo-target\NaviFS\x86_64-pc-windows-gnu\release\navifs.exe"
$RepoCandidate = Join-Path $PSScriptRoot "target\release\navifs.exe"

if (Test-Path $LocalCandidate) {
    Copy-Item $LocalCandidate $DestExe -Force
} elseif (Test-Path $RepoCandidate) {
    Copy-Item $RepoCandidate $DestExe -Force
} else {
    Write-Host "Building release binary..." -ForegroundColor Yellow
    cargo build --release -p navifs-daemon
    Copy-Item $LocalCandidate $DestExe -Force
}

Write-Host "[OK] Installed binary to: $DestExe" -ForegroundColor Green

# 2. Add to User PATH if missing
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$NaviDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$NaviDir;$UserPath", "User")
    $env:Path = "$NaviDir;$env:Path"
    Write-Host "[OK] Added $NaviDir to User PATH." -ForegroundColor Green
} else {
    Write-Host "[OK] $NaviDir is already in User PATH." -ForegroundColor Gray
}

# 3. Launch interactive or automated setup
$setupArgs = @()
if ($All) { $setupArgs += "--all" }
if ($Client) { $setupArgs += "--client", $Client }
if ($args) { $setupArgs += $args }

Write-Host "`nLaunching NaviFS Auto-Configurator..." -ForegroundColor Cyan
& $DestExe setup @setupArgs
