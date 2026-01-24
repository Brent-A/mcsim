#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Create a release package for MCSim.

.DESCRIPTION
    This script builds MCSim in release mode and creates a distributable package
    containing:
    
    - mcsim.exe (main executable)
    - Firmware DLLs (meshcore_repeater.dll, meshcore_companion.dll, meshcore_room_server.dll)
    - itm.dll (Irregular Terrain Model library)
    - rerun.exe (visualization viewer, optional)
    - examples/ folder (simulation configuration files)
    - DEM data .tif files (optional - AWS terrain tiles are fetched on demand by default)
    
    The release package is created in the 'release/' directory.

.PARAMETER OutputDir
    Output directory for the release package.
    Default: release

.PARAMETER SkipBuild
    Skip the cargo build step (use existing build artifacts).

.PARAMETER SkipDem
    Skip including DEM data files (default - AWS tiles are fetched on demand).

.PARAMETER IncludeDem
    Include DEM data files in the release package.

.PARAMETER SkipRerun
    Skip including rerun.exe (optional dependency).

.PARAMETER ZipPackage
    Create a zip archive of the release package.

.PARAMETER Configuration
    Build configuration: 'release' or 'debug'.
    Default: release

.EXAMPLE
    .\create_release.ps1
    
    Build and create a full release package.

.EXAMPLE
    .\create_release.ps1 -SkipBuild -ZipPackage
    
    Create a release package from existing build, and create a zip archive.

.EXAMPLE
    .\create_release.ps1 -SkipDem -SkipRerun
    
    Create a minimal release package without DEM data or Rerun viewer.
#>

[CmdletBinding()]
param(
    [Parameter()]
    [string]$OutputDir = "release",
    
    [Parameter()]
    [switch]$SkipBuild,
    
    [Parameter()]
    [switch]$SkipDem = $true,
    
    [Parameter()]
    [switch]$IncludeDem,
    
    [Parameter()]
    [switch]$SkipRerun,
    
    [Parameter()]
    [switch]$ZipPackage,
    
    [Parameter()]
    [ValidateSet("release", "debug")]
    [string]$Configuration = "release"
)

$ErrorActionPreference = "Stop"

# Get script and project directories
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "MCSim Release Builder" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Project root: $ProjectRoot"
Write-Host "Configuration: $Configuration"
Write-Host ""

# Resolve output directory to absolute path
if (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $ProjectRoot $OutputDir
}

Write-Host "Release directory: $OutputDir"
Write-Host ""

# ============================================================================
# Build the project
# ============================================================================
if (-not $SkipBuild) {
    Write-Host "Building MCSim ($Configuration)..." -ForegroundColor Cyan
    
    Push-Location $ProjectRoot
    try {
        $cargoArgs = @("build")
        
        if ($Configuration -eq "release") {
            $cargoArgs += "--release"
        }
        
        if (-not $SkipRerun) {
            $cargoArgs += "--features"
            $cargoArgs += "rerun"
        }
        
        Write-Host "  cargo $($cargoArgs -join ' ')" -ForegroundColor Gray
        & cargo @cargoArgs
        
        if ($LASTEXITCODE -ne 0) {
            Write-Host "Build failed!" -ForegroundColor Red
            exit 1
        }
        Write-Host "Build completed successfully." -ForegroundColor Green
    }
    finally {
        Pop-Location
    }
    Write-Host ""
}

# ============================================================================
# Determine paths
# ============================================================================
$TargetDir = Join-Path $ProjectRoot "target\$Configuration"
$FirmwareOutDir = Join-Path $TargetDir "build"

# Find the firmware DLL output directory (it's in a build subdirectory with a hash)
$FirmwareBuildDir = Get-ChildItem -Path $FirmwareOutDir -Directory -Filter "mcsim-firmware-*" | 
    Where-Object { Test-Path (Join-Path $_.FullName "out") } |
    Select-Object -First 1

if ($FirmwareBuildDir) {
    $FirmwareDllDir = Join-Path $FirmwareBuildDir.FullName "out"
}
else {
    # Fallback: search for DLLs in the target directory
    $FirmwareDllDir = $TargetDir
}

Write-Host "Source paths:" -ForegroundColor Gray
Write-Host "  Target:   $TargetDir"
Write-Host "  Firmware: $FirmwareDllDir"
Write-Host ""

# ============================================================================
# Create release directory structure
# ============================================================================
Write-Host "Creating release package..." -ForegroundColor Cyan

# Clean and create output directory
if (Test-Path $OutputDir) {
    Write-Host "  Cleaning existing release directory..." -ForegroundColor Yellow
    Remove-Item -Path $OutputDir -Recurse -Force
}
New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null

# Create subdirectories
$DemOutputDir = Join-Path $OutputDir "dem_data"
$ExamplesOutputDir = Join-Path $OutputDir "examples"

# ============================================================================
# Copy main executable
# ============================================================================
Write-Host "  Copying mcsim.exe..." -ForegroundColor Yellow
$McSimExe = Join-Path $TargetDir "mcsim.exe"

if (Test-Path $McSimExe) {
    Copy-Item -Path $McSimExe -Destination $OutputDir
    Write-Host "    OK" -ForegroundColor Green
}
else {
    Write-Host "    ERROR: mcsim.exe not found at $McSimExe" -ForegroundColor Red
    Write-Host "    Make sure to build the project first: cargo build --release" -ForegroundColor Yellow
    exit 1
}

# ============================================================================
# Copy firmware DLLs
# ============================================================================
Write-Host "  Copying firmware DLLs..." -ForegroundColor Yellow

$FirmwareDlls = @(
    "meshcore_repeater.dll",
    "meshcore_companion.dll",
    "meshcore_room_server.dll"
)

$foundDlls = 0
foreach ($dll in $FirmwareDlls) {
    $dllPath = Join-Path $FirmwareDllDir $dll
    
    if (Test-Path $dllPath) {
        Copy-Item -Path $dllPath -Destination $OutputDir
        Write-Host "    $dll - OK" -ForegroundColor Green
        $foundDlls++
    }
    else {
        # Try to find it elsewhere in the target directory
        $foundDll = Get-ChildItem -Path $TargetDir -Recurse -Filter $dll -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($foundDll) {
            Copy-Item -Path $foundDll.FullName -Destination $OutputDir
            Write-Host "    $dll - OK (found at $($foundDll.FullName))" -ForegroundColor Green
            $foundDlls++
        }
        else {
            Write-Host "    $dll - NOT FOUND" -ForegroundColor Red
        }
    }
}

if ($foundDlls -lt $FirmwareDlls.Count) {
    Write-Host "    WARNING: Not all firmware DLLs were found!" -ForegroundColor Yellow
}

# ============================================================================
# Copy ITM DLL
# ============================================================================
Write-Host "  Copying ITM library..." -ForegroundColor Yellow

$ItmDll = Join-Path $ProjectRoot "itm\itm.dll"

if (Test-Path $ItmDll) {
    Copy-Item -Path $ItmDll -Destination $OutputDir
    Write-Host "    itm.dll - OK" -ForegroundColor Green
}
else {
    Write-Host "    itm.dll - NOT FOUND (required for radio propagation)" -ForegroundColor Red
    Write-Host "    Run .\scripts\setup_dependencies.ps1 to download" -ForegroundColor Yellow
}

# ============================================================================
# Copy Rerun viewer (optional)
# ============================================================================
if (-not $SkipRerun) {
    Write-Host "  Copying Rerun viewer..." -ForegroundColor Yellow
    
    $RerunExe = Join-Path $ProjectRoot "rerun\rerun.exe"
    
    if (Test-Path $RerunExe) {
        Copy-Item -Path $RerunExe -Destination $OutputDir
        Write-Host "    rerun.exe - OK" -ForegroundColor Green
    }
    else {
        Write-Host "    rerun.exe - NOT FOUND (optional)" -ForegroundColor Yellow
    }
}

# ============================================================================
# Copy examples
# ============================================================================
Write-Host "  Copying examples..." -ForegroundColor Yellow

$ExamplesDir = Join-Path $ProjectRoot "examples"

if (Test-Path $ExamplesDir) {
    Copy-Item -Path $ExamplesDir -Destination $ExamplesOutputDir -Recurse
    $exampleCount = (Get-ChildItem -Path $ExamplesOutputDir -Recurse -File).Count
    Write-Host "    Copied $exampleCount example files - OK" -ForegroundColor Green
}
else {
    Write-Host "    examples/ directory not found" -ForegroundColor Yellow
}

# ============================================================================
# Copy DEM data (optional - AWS tiles are used by default)
# ============================================================================
if ($IncludeDem) {
    Write-Host "  Copying DEM data..." -ForegroundColor Yellow
    
    $DemDir = Join-Path $ProjectRoot "dem_data"
    $DemTifs = Get-ChildItem -Path $DemDir -Filter "*.tif" -ErrorAction SilentlyContinue
    
    if ($DemTifs -and $DemTifs.Count -gt 0) {
        New-Item -ItemType Directory -Path $DemOutputDir -Force | Out-Null
        
        foreach ($tif in $DemTifs) {
            Copy-Item -Path $tif.FullName -Destination $DemOutputDir
        }
        
        # Also copy the data.csv for reference
        $DemCsv = Join-Path $DemDir "data.csv"
        if (Test-Path $DemCsv) {
            Copy-Item -Path $DemCsv -Destination $DemOutputDir
        }
        
        $totalSize = ($DemTifs | Measure-Object -Property Length -Sum).Sum / 1MB
        Write-Host "    Copied $($DemTifs.Count) DEM files ($([math]::Round($totalSize, 1)) MB) - OK" -ForegroundColor Green
    }
    else {
        Write-Host "    No DEM .tif files found in $DemDir" -ForegroundColor Yellow
        Write-Host "    Run .\scripts\setup_dependencies.ps1 to download" -ForegroundColor Yellow
    }
}

# ============================================================================
# Create README
# ============================================================================
Write-Host "  Creating README..." -ForegroundColor Yellow

$ReadmeContent = @"
# MCSim Release Package

This package contains the MCSim (MeshCore Simulator) application and its dependencies.

## Contents

- ``mcsim.exe`` - Main simulator executable
- ``meshcore_repeater.dll`` - Repeater firmware library
- ``meshcore_companion.dll`` - Companion firmware library  
- ``meshcore_room_server.dll`` - Room server firmware library
- ``itm.dll`` - NTIA Irregular Terrain Model library (radio propagation)
- ``rerun.exe`` - Rerun visualization viewer (optional)
- ``examples/`` - Example simulation configuration files

## Quick Start

1. Run a simulation:
   ``````
   .\mcsim.exe run examples\seattle\sea.yaml
   ``````

2. With visualization (requires rerun.exe):
   ``````
   .\mcsim.exe run examples\seattle\sea.yaml --rerun
   ``````

3. Predict link quality between two points:
   ``````
   .\mcsim.exe predict-link 47.6062 -122.3321 47.7 -122.5
   ``````
   
   Terrain elevation data is automatically fetched from AWS Open Data
   and cached locally in ./elevation_cache/

## Elevation Data

By default, MCSim fetches terrain elevation tiles on demand from the AWS
Terrain Tiles dataset (https://registry.opendata.aws/terrain-tiles/).
Tiles are cached locally in ``./elevation_cache/`` for reuse.

To use local USGS DEM files instead:
   ``````
   .\mcsim.exe predict-link ... --elevation-source local_dem --dem-dir .\dem_data
   ``````

## Requirements

- Windows 10/11 x64
- Visual C++ Redistributable (usually already installed)
- Internet connection (for fetching terrain tiles on first use)

## Documentation

See the project repository for full documentation:
https://github.com/example/mcsim

## Dependencies

- ITM library: https://github.com/NTIA/itm
- Rerun viewer: https://github.com/rerun-io/rerun
- AWS Terrain Tiles: https://registry.opendata.aws/terrain-tiles/
"@

$ReadmePath = Join-Path $OutputDir "README.md"
$ReadmeContent | Out-File -FilePath $ReadmePath -Encoding UTF8
Write-Host "    OK" -ForegroundColor Green

# ============================================================================
# Create zip archive (optional)
# ============================================================================
if ($ZipPackage) {
    Write-Host ""
    Write-Host "Creating zip archive..." -ForegroundColor Cyan
    
    # Get version from Cargo.toml if possible
    $version = "0.1.0"
    $cargoToml = Join-Path $ProjectRoot "Cargo.toml"
    if (Test-Path $cargoToml) {
        $content = Get-Content $cargoToml -Raw
        if ($content -match 'version\s*=\s*"([^"]+)"') {
            $version = $Matches[1]
        }
    }
    
    $zipName = "mcsim-$version-windows-x64.zip"
    $zipPath = Join-Path $ProjectRoot $zipName
    
    if (Test-Path $zipPath) {
        Remove-Item $zipPath -Force
    }
    
    Compress-Archive -Path "$OutputDir\*" -DestinationPath $zipPath
    
    $zipSize = (Get-Item $zipPath).Length / 1MB
    Write-Host "  Created: $zipName ($([math]::Round($zipSize, 1)) MB)" -ForegroundColor Green
}

# ============================================================================
# Summary
# ============================================================================
Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Release Package Complete!" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Location: $OutputDir" -ForegroundColor White
Write-Host ""

# List contents with sizes
Write-Host "Contents:" -ForegroundColor White
$items = Get-ChildItem -Path $OutputDir
foreach ($item in $items) {
    if ($item.PSIsContainer) {
        $dirSize = (Get-ChildItem -Path $item.FullName -Recurse -File | Measure-Object -Property Length -Sum).Sum
        $sizeStr = if ($dirSize -gt 1MB) { "$([math]::Round($dirSize / 1MB, 1)) MB" } else { "$([math]::Round($dirSize / 1KB, 1)) KB" }
        $fileCount = (Get-ChildItem -Path $item.FullName -Recurse -File).Count
        Write-Host "  $($item.Name)/ ($fileCount files, $sizeStr)" -ForegroundColor Gray
    }
    else {
        $sizeStr = if ($item.Length -gt 1MB) { "$([math]::Round($item.Length / 1MB, 1)) MB" } else { "$([math]::Round($item.Length / 1KB, 1)) KB" }
        Write-Host "  $($item.Name) ($sizeStr)" -ForegroundColor Gray
    }
}

$totalSize = (Get-ChildItem -Path $OutputDir -Recurse -File | Measure-Object -Property Length -Sum).Sum / 1MB
Write-Host ""
Write-Host "Total size: $([math]::Round($totalSize, 1)) MB" -ForegroundColor White
Write-Host ""
Write-Host "To test the release:" -ForegroundColor White
Write-Host "  cd $OutputDir"
Write-Host "  .\mcsim.exe --help"
Write-Host ""
