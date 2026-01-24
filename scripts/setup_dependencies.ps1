#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Setup build dependencies for MCSim.

.DESCRIPTION
    This script downloads and sets up the external dependencies required to build
    and run MCSim:
    
    1. ITM (Irregular Terrain Model) library from NTIA
    2. Rerun visualization tool (optional)
    3. DEM (Digital Elevation Model) data from USGS (optional - AWS tiles are used by default)
    
    The MeshCore submodule must be initialized separately using:
        git submodule update --init --recursive

.PARAMETER SkipDem
    Skip downloading DEM data files (default behavior - AWS tiles are used instead).

.PARAMETER SkipItm
    Skip downloading the ITM library.

.PARAMETER SkipRerun
    Skip downloading the Rerun viewer (optional dependency).

.PARAMETER DemCsvPath
    Path to the CSV file describing DEM tiles to download.
    Default: dem_data/data.csv

.PARAMETER Force
    Force re-download of dependencies even if they already exist.

.EXAMPLE
    .\setup_dependencies.ps1
    
    Downloads all dependencies.

.EXAMPLE
    .\setup_dependencies.ps1 -SkipRerun
    
    Downloads required dependencies only, skipping optional Rerun viewer.

.EXAMPLE
    .\setup_dependencies.ps1 -Force
    
    Re-downloads all dependencies even if they exist.
#>

[CmdletBinding()]
param(
    [switch]$SkipDem = $true,
    [switch]$IncludeDem,
    [switch]$SkipItm,
    [switch]$SkipRerun,
    [string]$DemCsvPath = "",
    [switch]$Force
)

$ErrorActionPreference = "Stop"

# Get script and project directories
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "MCSim Dependency Setup" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Project root: $ProjectRoot"
Write-Host ""

# Helper function to download a file
function Download-File {
    param(
        [string]$Url,
        [string]$OutputPath,
        [string]$Description
    )
    
    Write-Host "  Downloading $Description..." -ForegroundColor Yellow
    Write-Host "    URL: $Url"
    Write-Host "    Output: $OutputPath"
    
    $parentDir = Split-Path -Parent $OutputPath
    if (-not (Test-Path $parentDir)) {
        New-Item -ItemType Directory -Path $parentDir -Force | Out-Null
    }
    
    try {
        $ProgressPreference = 'SilentlyContinue'  # Faster downloads
        Invoke-WebRequest -Uri $Url -OutFile $OutputPath -UseBasicParsing
        $ProgressPreference = 'Continue'
        Write-Host "    Done!" -ForegroundColor Green
        return $true
    }
    catch {
        Write-Host "    Failed: $_" -ForegroundColor Red
        return $false
    }
}

# Helper function to extract a zip file
function Extract-Archive {
    param(
        [string]$ArchivePath,
        [string]$DestinationPath
    )
    
    Write-Host "  Extracting to $DestinationPath..." -ForegroundColor Yellow
    
    if (-not (Test-Path $DestinationPath)) {
        New-Item -ItemType Directory -Path $DestinationPath -Force | Out-Null
    }
    
    try {
        Expand-Archive -Path $ArchivePath -DestinationPath $DestinationPath -Force
        Write-Host "    Done!" -ForegroundColor Green
        return $true
    }
    catch {
        Write-Host "    Failed: $_" -ForegroundColor Red
        return $false
    }
}

# ============================================================================
# Check MeshCore submodule
# ============================================================================
Write-Host "Checking MeshCore submodule..." -ForegroundColor Cyan
$MeshCorePath = Join-Path $ProjectRoot "MeshCore"
$MeshCoreSrc = Join-Path $MeshCorePath "src"

if (-not (Test-Path $MeshCoreSrc)) {
    Write-Host "  WARNING: MeshCore submodule not initialized!" -ForegroundColor Yellow
    Write-Host "  Run: git submodule update --init --recursive" -ForegroundColor Yellow
    Write-Host ""
}
else {
    Write-Host "  MeshCore submodule found." -ForegroundColor Green
}
Write-Host ""

# ============================================================================
# Download ITM Library
# ============================================================================
if (-not $SkipItm) {
    Write-Host "Setting up ITM (Irregular Terrain Model) library..." -ForegroundColor Cyan
    
    $ItmDir = Join-Path $ProjectRoot "itm"
    $ItmDll = Join-Path $ItmDir "itm.dll"
    
    if ((Test-Path $ItmDll) -and (-not $Force)) {
        Write-Host "  ITM library already exists at $ItmDll" -ForegroundColor Green
        Write-Host "  Use -Force to re-download" -ForegroundColor Gray
    }
    else {
        Write-Host "  ITM library source: https://github.com/NTIA/itm" -ForegroundColor Gray
        
        # Create itm directory
        if (-not (Test-Path $ItmDir)) {
            New-Item -ItemType Directory -Path $ItmDir -Force | Out-Null
        }
        
        # Download pre-built ITM DLL from GitHub releases
        # The NTIA ITM repo provides releases with pre-built binaries
        $ItmReleasesUrl = "https://api.github.com/repos/NTIA/itm/releases/latest"
        
        try {
            Write-Host "  Fetching latest ITM release info..." -ForegroundColor Yellow
            $ProgressPreference = 'SilentlyContinue'
            $releaseInfo = Invoke-RestMethod -Uri $ItmReleasesUrl -UseBasicParsing
            $ProgressPreference = 'Continue'
            
            # Find the Windows DLL asset
            $dllAsset = $releaseInfo.assets | Where-Object { $_.name -match "itm.*\.dll$" -or $_.name -match "win.*\.zip$" -or $_.name -match "windows.*\.zip$" }
            
            if ($dllAsset) {
                $downloadUrl = $dllAsset.browser_download_url
                $assetName = $dllAsset.name
                
                if ($assetName -match "\.zip$") {
                    # Download and extract zip
                    $zipPath = Join-Path $ItmDir "itm_temp.zip"
                    if (Download-File -Url $downloadUrl -OutputPath $zipPath -Description "ITM release ($assetName)") {
                        Extract-Archive -ArchivePath $zipPath -DestinationPath $ItmDir
                        Remove-Item $zipPath -Force
                    }
                }
                else {
                    # Direct DLL download
                    Download-File -Url $downloadUrl -OutputPath $ItmDll -Description "ITM DLL"
                }
            }
            else {
                Write-Host "  No pre-built Windows binary found in latest release." -ForegroundColor Yellow
                Write-Host "  You may need to build ITM manually from https://github.com/NTIA/itm" -ForegroundColor Yellow
                Write-Host "  Place the compiled itm.dll in: $ItmDir" -ForegroundColor Yellow
            }
        }
        catch {
            Write-Host "  Failed to fetch ITM release info: $_" -ForegroundColor Red
            Write-Host "  You may need to build ITM manually from https://github.com/NTIA/itm" -ForegroundColor Yellow
            Write-Host "  Place the compiled itm.dll in: $ItmDir" -ForegroundColor Yellow
        }
    }
    Write-Host ""
}

# ============================================================================
# Download Rerun Viewer (Optional)
# ============================================================================
if (-not $SkipRerun) {
    Write-Host "Setting up Rerun viewer (optional)..." -ForegroundColor Cyan
    
    $RerunDir = Join-Path $ProjectRoot "rerun"
    $RerunExe = Join-Path $RerunDir "rerun.exe"
    
    if ((Test-Path $RerunExe) -and (-not $Force)) {
        Write-Host "  Rerun viewer already exists at $RerunExe" -ForegroundColor Green
        Write-Host "  Use -Force to re-download" -ForegroundColor Gray
    }
    else {
        Write-Host "  Rerun source: https://github.com/rerun-io/rerun" -ForegroundColor Gray
        
        # Create rerun directory
        if (-not (Test-Path $RerunDir)) {
            New-Item -ItemType Directory -Path $RerunDir -Force | Out-Null
        }
        
        # Fetch latest release from GitHub
        $RerunReleasesUrl = "https://api.github.com/repos/rerun-io/rerun/releases/latest"
        
        try {
            Write-Host "  Fetching latest Rerun release info..." -ForegroundColor Yellow
            $ProgressPreference = 'SilentlyContinue'
            $releaseInfo = Invoke-RestMethod -Uri $RerunReleasesUrl -UseBasicParsing
            $ProgressPreference = 'Continue'
            
            $version = $releaseInfo.tag_name
            Write-Host "  Latest version: $version" -ForegroundColor Gray
            
            # Find Windows x86_64 asset
            $windowsAsset = $releaseInfo.assets | Where-Object { 
                $_.name -match "rerun.*windows.*x86_64.*\.zip$" -or 
                $_.name -match "rerun.*x86_64.*windows.*\.zip$" -or
                $_.name -match "rerun.*win64.*\.zip$"
            }
            
            if ($windowsAsset) {
                $downloadUrl = $windowsAsset.browser_download_url
                $zipPath = Join-Path $RerunDir "rerun_temp.zip"
                
                if (Download-File -Url $downloadUrl -OutputPath $zipPath -Description "Rerun viewer ($($windowsAsset.name))") {
                    Extract-Archive -ArchivePath $zipPath -DestinationPath $RerunDir
                    Remove-Item $zipPath -Force
                    
                    # Find and move the rerun.exe to the expected location
                    $foundExe = Get-ChildItem -Path $RerunDir -Recurse -Filter "rerun.exe" | Select-Object -First 1
                    if ($foundExe -and $foundExe.FullName -ne $RerunExe) {
                        Move-Item -Path $foundExe.FullName -Destination $RerunExe -Force
                        # Clean up extracted subdirectories
                        Get-ChildItem -Path $RerunDir -Directory | Remove-Item -Recurse -Force
                    }
                }
            }
            else {
                Write-Host "  No Windows x86_64 binary found in latest release." -ForegroundColor Yellow
                Write-Host "  You can install Rerun via cargo: cargo install rerun-cli" -ForegroundColor Yellow
            }
        }
        catch {
            Write-Host "  Failed to fetch Rerun release info: $_" -ForegroundColor Red
            Write-Host "  You can install Rerun via cargo: cargo install rerun-cli" -ForegroundColor Yellow
        }
    }
    Write-Host ""
}

# ============================================================================
# Download DEM Data (Optional - AWS tiles are used by default)
# ============================================================================
if ($IncludeDem -or (-not $SkipDem -and $IncludeDem)) {
    Write-Host "Setting up DEM (Digital Elevation Model) data..." -ForegroundColor Cyan
    Write-Host "  NOTE: DEM data is optional. AWS terrain tiles are used by default." -ForegroundColor Gray
    Write-Host "  Use --elevation-source local_dem to use local DEM files." -ForegroundColor Gray
    
    $DemDir = Join-Path $ProjectRoot "dem_data"
    
    # Determine CSV path
    if ([string]::IsNullOrEmpty($DemCsvPath)) {
        $DemCsvPath = Join-Path $DemDir "data.csv"
    }
    
    if (-not (Test-Path $DemCsvPath)) {
        Write-Host "  WARNING: DEM data CSV not found at $DemCsvPath" -ForegroundColor Yellow
        Write-Host "  Export a CSV from https://apps.nationalmap.gov/downloader/" -ForegroundColor Yellow
        Write-Host "  and save it to $DemCsvPath" -ForegroundColor Yellow
    }
    else {
        # Check if we have .tif files already
        $existingTifs = Get-ChildItem -Path $DemDir -Filter "*.tif" -ErrorAction SilentlyContinue
        
        if ($existingTifs -and $existingTifs.Count -gt 0 -and (-not $Force)) {
            Write-Host "  Found $($existingTifs.Count) existing DEM .tif files in $DemDir" -ForegroundColor Green
            Write-Host "  Use -Force to re-download" -ForegroundColor Gray
        }
        else {
            Write-Host "  Running DEM download script..." -ForegroundColor Yellow
            
            $fetchScript = Join-Path $ScriptDir "fetch_usgs_dem.py"
            
            if (Test-Path $fetchScript) {
                # Check for Python
                $python = Get-Command python -ErrorAction SilentlyContinue
                if (-not $python) {
                    $python = Get-Command python3 -ErrorAction SilentlyContinue
                }
                
                if ($python) {
                    $skipExisting = if ($Force) { "" } else { "--skip-existing" }
                    Push-Location $ProjectRoot
                    try {
                        & $python.Source $fetchScript $DemCsvPath --output-dir $DemDir $skipExisting --use-urllib
                    }
                    finally {
                        Pop-Location
                    }
                }
                else {
                    Write-Host "  Python not found. Install Python to download DEM data." -ForegroundColor Yellow
                    Write-Host "  Or manually download the .tif files from the URLs in $DemCsvPath" -ForegroundColor Yellow
                }
            }
            else {
                Write-Host "  DEM fetch script not found at $fetchScript" -ForegroundColor Yellow
            }
        }
    }
    Write-Host ""
}

# ============================================================================
# Summary
# ============================================================================
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Setup Complete!" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Dependency Status:" -ForegroundColor White

# Check MeshCore
$MeshCoreStatus = if (Test-Path (Join-Path $ProjectRoot "MeshCore\src")) { "OK" } else { "MISSING" }
$MeshCoreColor = if ($MeshCoreStatus -eq "OK") { "Green" } else { "Red" }
Write-Host "  MeshCore submodule: " -NoNewline
Write-Host $MeshCoreStatus -ForegroundColor $MeshCoreColor

# Check ITM
$ItmStatus = if (Test-Path (Join-Path $ProjectRoot "itm\itm.dll")) { "OK" } else { "MISSING" }
$ItmColor = if ($ItmStatus -eq "OK") { "Green" } else { "Red" }
Write-Host "  ITM library:        " -NoNewline
Write-Host $ItmStatus -ForegroundColor $ItmColor

# Check Rerun
$RerunStatus = if (Test-Path (Join-Path $ProjectRoot "rerun\rerun.exe")) { "OK" } else { "MISSING (optional)" }
$RerunColor = if ($RerunStatus -eq "OK") { "Green" } else { "Yellow" }
Write-Host "  Rerun viewer:       " -NoNewline
Write-Host $RerunStatus -ForegroundColor $RerunColor

# Check DEM
$DemTifs = Get-ChildItem -Path (Join-Path $ProjectRoot "dem_data") -Filter "*.tif" -ErrorAction SilentlyContinue
$DemStatus = if ($DemTifs -and $DemTifs.Count -gt 0) { "OK ($($DemTifs.Count) files)" } else { "Not downloaded (optional - AWS tiles used by default)" }
$DemColor = if ($DemTifs -and $DemTifs.Count -gt 0) { "Green" } else { "Gray" }
Write-Host "  DEM data:           " -NoNewline
Write-Host $DemStatus -ForegroundColor $DemColor

Write-Host ""
Write-Host "Next steps:" -ForegroundColor White
Write-Host "  1. Build the project: cargo build --release --features rerun"
Write-Host "  2. Run tests: cargo test"
Write-Host "  3. Create release: .\scripts\create_release.ps1"
Write-Host ""
