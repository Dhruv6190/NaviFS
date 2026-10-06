<#
.SYNOPSIS
NaviFS Universal 1-Click Plug & Play Installer for Windows.
Installs navifs.exe to %LOCALAPPDATA%\Programs\navifs\bin, updates PATH, and auto-configures AI agents.
#>

param(
    [switch]$All,
    [string]$Client,
    [string]$Version = "latest",
    [switch]$BuildFromSource,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

Write-Host "========================================================" -ForegroundColor Cyan
Write-Host "       NaviFS Universal Plug & Play Installer           " -ForegroundColor Cyan
Write-Host "========================================================" -ForegroundColor Cyan

$Repo = "Dhruv6190/NaviFS"
$InstallDir = if ($env:LOCALAPPDATA) {
    Join-Path $env:LOCALAPPDATA "Programs\navifs\bin"
} else {
    Join-Path $HOME ".navifs\bin"
}

if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

$DestExe = Join-Path $InstallDir "navifs.exe"

# 1. Locate or obtain navifs.exe
$Installed = $false

# Candidate A: Local release build in current repo
$LocalCandidate1 = Join-Path $PSScriptRoot "target\release\navifs.exe"
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "" }
$LocalCandidate2 = if ($TargetDir) { Join-Path $TargetDir "release\navifs.exe" } else { "" }

if (-not $BuildFromSource -and (Test-Path $LocalCandidate1)) {
    Write-Host "[OK] Using locally built binary: $LocalCandidate1" -ForegroundColor Green
    Copy-Item $LocalCandidate1 $DestExe -Force
    $Installed = $true
} elseif (-not $BuildFromSource -and $LocalCandidate2 -and (Test-Path $LocalCandidate2)) {
    Write-Host "[OK] Using locally built binary: $LocalCandidate2" -ForegroundColor Green
    Copy-Item $LocalCandidate2 $DestExe -Force
    $Installed = $true
} elseif ($BuildFromSource) {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw "Rust/Cargo is not installed on PATH. Cannot build from source."
    }
    Write-Host "Building release binary via cargo..." -ForegroundColor Yellow
    cargo build --release -p navifs-daemon
    if (Test-Path $LocalCandidate1) {
        Copy-Item $LocalCandidate1 $DestExe -Force
        $Installed = $true
    } elseif ($LocalCandidate2 -and (Test-Path $LocalCandidate2)) {
        Copy-Item $LocalCandidate2 $DestExe -Force
        $Installed = $true
    }
}

if (-not $Installed) {
    # Candidate B: Download latest release from GitHub
    Write-Host "Fetching latest release binary for Windows (x86_64)..." -ForegroundColor Cyan
    $BaseUrl = if ($Version -eq "latest") {
        "https://github.com/$Repo/releases/latest/download"
    } else {
        "https://github.com/$Repo/releases/download/$Version"
    }

    $ArchiveName = "navifs-x86_64-pc-windows-msvc.zip"
    $DownloadUrl = "$BaseUrl/$ArchiveName"
    $ChecksumUrl = "$BaseUrl/checksums.txt"
    $TempZip = Join-Path ([System.IO.Path]::GetTempPath()) "navifs-$PID.zip"
    $TempChecksum = Join-Path ([System.IO.Path]::GetTempPath()) "checksums-$PID.txt"

    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12 -bor [Net.SecurityProtocolType]::Tls13
        Write-Host "Downloading $DownloadUrl..." -ForegroundColor Gray
        Invoke-WebRequest -Uri $DownloadUrl -OutFile $TempZip -UseBasicParsing

        # Verify SHA-256 checksum if checksums.txt is available
        try {
            Invoke-WebRequest -Uri $ChecksumUrl -OutFile $TempChecksum -UseBasicParsing -ErrorAction SilentlyContinue
            if (Test-Path $TempChecksum) {
                $ExpectedHash = (Get-Content $TempChecksum | Select-String -Pattern $ArchiveName | ForEach-Object { ($_ -split '\s+')[0] })
                if ($ExpectedHash) {
                    $ActualHash = (Get-FileHash -Path $TempZip -Algorithm SHA256).Hash.ToLower()
                    if ($ActualHash -ne $ExpectedHash.ToLower()) {
                        throw "Integrity verification failed for $ArchiveName (expected $ExpectedHash, got $ActualHash)"
                    }
                    Write-Host "[OK] SHA-256 checksum verified: $ActualHash" -ForegroundColor Green
                }
            }
        } catch {
            Write-Host "Note: Checksum verification skipped ($($_.Exception.Message))" -ForegroundColor Gray
        }

        # Extract binary
        $TempExtract = Join-Path ([System.IO.Path]::GetTempPath()) "navifs-extract-$PID"
        Expand-Archive -Path $TempZip -DestinationPath $TempExtract -Force
        $ExtractedExe = Join-Path $TempExtract "navifs.exe"
        if (-not (Test-Path $ExtractedExe)) {
            $Found = Get-ChildItem -Path $TempExtract -Filter "navifs.exe" -Recurse | Select-Object -First 1
            if ($Found) { $ExtractedExe = $Found.FullName }
        }

        if (Test-Path $ExtractedExe) {
            Copy-Item $ExtractedExe $DestExe -Force
            $Installed = $true
        } else {
            throw "Failed to find navifs.exe inside downloaded release archive."
        }
    } catch {
        # Fallback to cargo if user has it installed locally
        if (Get-Command cargo -ErrorAction SilentlyContinue) {
            Write-Host "Download failed ($($_.Exception.Message)). Falling back to local 'cargo build --release'..." -ForegroundColor Yellow
            cargo build --release -p navifs-daemon
            if (Test-Path $LocalCandidate1) {
                Copy-Item $LocalCandidate1 $DestExe -Force
                $Installed = $true
            } elseif ($LocalCandidate2 -and (Test-Path $LocalCandidate2)) {
                Copy-Item $LocalCandidate2 $DestExe -Force
                $Installed = $true
            }
        } else {
            throw "Failed to install NaviFS: $($_.Exception.Message)"
        }
    } finally {
        Remove-Item -Path $TempZip, $TempChecksum -Force -ErrorAction SilentlyContinue
        Remove-Item -Path $TempExtract -Recurse -Force -ErrorAction SilentlyContinue
    }
}

if (-not (Test-Path $DestExe)) {
    throw "Installation failed: $DestExe does not exist."
}

Write-Host "[OK] Installed binary to: $DestExe" -ForegroundColor Green

# 2. Add to User PATH if missing
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$InstallDir;$UserPath", "User")
    $env:Path = "$InstallDir;$env:Path"
    Write-Host "[OK] Added $InstallDir to User PATH." -ForegroundColor Green
} else {
    Write-Host "[OK] $InstallDir is already in User PATH." -ForegroundColor Gray
}

# 3. Launch interactive or automated setup
$setupArgs = @()
if ($All) { $setupArgs += "--all" }
if ($Client) { $setupArgs += "--client", $Client }
if ($DryRun) { $setupArgs += "--dry-run" }

Write-Host "`nLaunching NaviFS Auto-Configurator..." -ForegroundColor Cyan
& $DestExe setup @setupArgs
