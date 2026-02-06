# MCSim - MeshCore Simulator

MCSim is a simulation framework for [MeshCore](https://github.com/liamcottle/meshcore) LoRa mesh networking firmware. It enables testing and analysis of mesh network behavior, radio propagation, and protocol performance.

## Features

- Full simulation of MeshCore firmware (repeaters, companions, room servers)
- Compatible with MeshCore apps and scripts using node UARTs over TCP
- Real-time visualization with [Rerun](https://rerun.io)
- Configurable network topologies via YAML files
- Metrics collection and analysis with packet decoding
- Full packet export to JSON with decoded MeshCore packet structure
- Deterministic execution
- Full mesh lockstep timing for accurate collision analysis
- Realistic radio propagation using the NTIA Irregular Terrain Model (ITM)
- Digital Elevation Model (DEM) data support for terrain-aware propagation
- MeshCore key generation with selectable public prefix
- Model generation from real-world advert data

## Use Cases

- Test changes to firmware implementations
- Test changes to applications or bots
- Compare expected behavior of mesh routing, timing, or flow control algorithms
- Compare mesh behavior when traffic patterns are changed
- Analyze mesh networks

## Quick Start

### Prerequisites

- **Rust** toolchain (install from https://rustup.rs)
- **LLVM/Clang** with `clang-cl` (install from https://releases.llvm.org)
- **Git** (for submodules)

### Setup

1. **Clone with submodules:**
   ```bash
   git clone --recursive https://github.com/Brent-A/mcsim.git
   cd mcsim
   ```

   Or if already cloned:
   ```bash
   git submodule update --init --recursive
   ```

2. **Download dependencies:**
   ```powershell
   # PowerShell (recommended)
   .\scripts\setup_dependencies.ps1
   
   # Or use the batch wrapper
   .\scripts\setup_dependencies.bat

   # Or use on Linux
   .\scripts\setup_dependencies.sh
   ```

   This downloads:
   - ITM library (radio propagation model)
   - Rerun viewer (optional, for visualization)
   - DEM data (terrain elevation, from USGS)

3. **Build:**
   ```bash
   cargo build --release
   ```

### Run a Simulation

```bash
# Basic simulation in realtime mode
cargo run --release -- run examples/topologies/simple.yaml

# With visualization
cargo run --release --features rerun -- run examples/topologies/simple.yaml --rerun
```

### Run a Simulation with activity

```bash
# Basic simulation in realtime mode
cargo run --release -- run examples/topologies/simple.yaml examples/behaviors/chatter.yaml --duration 10m
```

## Project Structure

```
mcsim/
├── crates/                    # Rust crates
│   ├── mcsim-firmware/        # MeshCore firmware DLL wrapper
│   ├── mcsim-runner/          # Simulation runner
│   ├── mcsim-model/           # Simulation model
│   ├── mcsim-itm/             # ITM library bindings
│   ├── mcsim-dem/             # DEM data handling
│   └── ...
├── MeshCore/                  # MeshCore firmware (git submodule)
├── simulator/                 # C++ simulation interface code
├── examples/                  # Example simulation configs
├── scripts/                   # Build and setup scripts
├── itm/                       # ITM library (itm.dll)
├── rerun/                     # Rerun viewer (rerun.exe)
└── dem_data/                  # DEM terrain data (.tif files)
```

## Dependencies

### Required

| Dependency | Description | Source |
|------------|-------------|--------|
| MeshCore | Mesh networking firmware | Git submodule |
| ITM | Radio propagation model | https://github.com/NTIA/itm |

### Optional

| Dependency | Description | Source |
|------------|-------------|--------|
| Rerun | Real-time visualization | https://github.com/rerun-io/rerun |
| DEM Data | Terrain elevation data | https://apps.nationalmap.gov/downloader/ |

## Scripts

### `scripts/setup_dependencies.ps1`

Downloads and configures external dependencies:

```powershell
# Download all dependencies
.\scripts\setup_dependencies.ps1

# Skip optional Rerun viewer
.\scripts\setup_dependencies.ps1 -SkipRerun

# Force re-download existing files
.\scripts\setup_dependencies.ps1 -Force

# Skip specific dependencies
.\scripts\setup_dependencies.ps1 -SkipDem -SkipItm
```

### `scripts/create_release.ps1`

Creates a distributable release package:

```powershell
# Full release build
.\scripts\create_release.ps1

# Create zip archive
.\scripts\create_release.ps1 -ZipPackage

# Minimal package (no DEM data)
.\scripts\create_release.ps1 -SkipDem

# Use existing build
.\scripts\create_release.ps1 -SkipBuild
```

The release package includes:
- `mcsim.exe` - Main executable
- `meshcore_*.dll` - Firmware libraries
- `itm/itm.dll` - ITM library
- `rerun/rerun.exe` - Visualization viewer (optional)
- `examples/` - Configuration files
- `dem_data/` - Terrain data (optional)

## Configuration

Simulations are configured via YAML files. See `examples/` for samples.

```yaml
# Example: examples/topologies/simple.yaml
simulation:
  duration: 60s
  time_scale: 1.0

nodes:
  - name: repeater1
    type: repeater
    location: { lat: 47.6062, lon: -122.3321, alt: 100 }
    
  - name: companion1  
    type: companion
    location: { lat: 47.6072, lon: -122.3331, alt: 50 }
```

## Development

### Build Commands

```bash
# Debug build
cargo build

# Release build  
cargo build --release

# Run tests
cargo test

# Run specific crate tests
cargo test -p mcsim-firmware
```

### Rerun Feature

The `rerun` feature enables real-time visualization but adds significant build time, so it is disabled by default. To enable it:

```bash
cargo build --release --features rerun
```

### IDE Setup

For VS Code, install:
- rust-analyzer extension
- CodeLLDB (for debugging)

## License

MIT License - See LICENSE file for details.

## Acknowledgments

- [MeshCore](https://github.com/meshcore-dev/MeshCore) - LoRa mesh firmware
- [NTIA ITM](https://github.com/NTIA/itm) - Radio propagation model
- [Rerun](https://rerun.io) - Visualization framework
- [USGS 3DEP](https://www.usgs.gov/3d-elevation-program) - Elevation data
