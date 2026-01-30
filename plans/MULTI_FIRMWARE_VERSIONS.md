# Multi-Firmware Version Support

## Problem Statement

We want to simulate heterogeneous networks where nodes run different firmware versions. For example:
- Testing network behavior during rolling firmware upgrades
- Simulating backward compatibility between 1.11 and 1.12 nodes
- Analyzing how protocol changes affect mixed-version networks

Currently, the simulator builds a single set of DLLs from the MeshCore submodule at build time. All nodes in a simulation use the same firmware version.

## Current Architecture

### Build Process

1. **Submodule**: MeshCore source lives in `MeshCore/` as a git submodule (currently at v1.12.0)
2. **Build Script**: `crates/mcsim-firmware/build.rs` compiles C++ sources into DLLs:
   - `meshcore_repeater.dll`
   - `meshcore_companion.dll`
   - `meshcore_room_server.dll`
3. **DLL Location**: Built DLLs go to `target/{debug,release}/build/mcsim-firmware-*/out/`
4. **Loading**: `FirmwareDll::load()` searches for DLLs by type at runtime via `find_dll_path()`

### Runtime Loading

```rust
// Current loading - single version per firmware type
pub fn load(firmware_type: FirmwareType) -> Result<Self, DllError> {
    let dll_path = find_dll_path(firmware_type)?;
    Self::load_from_path(&dll_path, firmware_type)
}
```

### Configuration

Nodes specify firmware type in YAML:
```yaml
nodes:
  - name: "Alice"
    firmware:
      type: Companion
```

---

## Proposed Solution

### Overview

Use a pre-built DLL cache with version embedded in filenames, plus fallback to current build output.

1. **Primary**: Check `firmware_cache/<platform>/` for pre-built versioned DLLs
2. **Fallback**: If version is null/unspecified, use current build output
3. **Scripts**: Provide scripts to build and cache specific versions

### YAML Schema

```yaml
defaults:
  firmware:
    version: "1.12.0"  # Default for all nodes

nodes:
  - name: "OldNode"
    firmware:
      type: Companion
      version: "1.11.0"  # Override to older version
  - name: "NewNode"
    firmware:
      type: Companion
      # Uses default 1.12.0
```

### DLL Naming Convention

Version is embedded in the filename for debugger-friendliness:

```
firmware_cache/
  windows/
    meshcore_repeater_1.11.0.dll
    meshcore_repeater_1.12.0.dll
    meshcore_companion_1.11.0.dll
    meshcore_companion_1.12.0.dll
    meshcore_room_server_1.11.0.dll
    meshcore_room_server_1.12.0.dll
  linux/
    libmeshcore_repeater_1.11.0.so
    libmeshcore_repeater_1.12.0.so
    ...
  macos/
    libmeshcore_repeater_1.11.0.dylib
    ...
```

Benefits:
- **Debugger-friendly**: When multiple firmware versions are loaded in the same process, each DLL has a unique name, making it easy to identify which version you're debugging
- **Simpler directory structure**: Flat per-platform directory
- **Clear at a glance**: `ls firmware_cache/windows/` shows all available versions immediately

---

## Implementation

### Phase 1: Infrastructure (Core Changes)

1. **Add `firmware/version` property**:

   ```rust
   // In crates/mcsim-model/src/properties/definitions.rs
   pub const FIRMWARE_VERSION: Property<Option<String>, NodeScope> = Property::new(
       "firmware/version",
       "Firmware version to use (e.g., '1.11.0'). If null, uses current build.",
       PropertyDefault::Null,
   );
   ```

2. **Modify `FirmwareDll` loading**:

   ```rust
   impl FirmwareType {
       /// Get the library filename for this firmware type with optional version.
       pub fn dll_name_versioned(&self, version: Option<&str>) -> String {
           let base = match self {
               FirmwareType::Repeater => "meshcore_repeater",
               FirmwareType::RoomServer => "meshcore_room_server",
               FirmwareType::Companion => "meshcore_companion",
           };
           let versioned = match version {
               Some(v) => format!("{}_{}", base, v),
               None => base.to_string(),
           };
           #[cfg(target_os = "windows")]
           return format!("{}.dll", versioned);
           #[cfg(target_os = "linux")]
           return format!("lib{}.so", versioned);
           #[cfg(target_os = "macos")]
           return format!("lib{}.dylib", versioned);
       }
   }
   
   impl FirmwareDll {
       /// Load firmware DLL with optional version.
       pub fn load_versioned(
           firmware_type: FirmwareType,
           version: Option<&str>,
       ) -> Result<Self, DllError> {
           let dll_path = find_versioned_dll_path(firmware_type, version)?;
           Self::load_from_path(&dll_path, firmware_type)
       }
   }
   
   fn find_versioned_dll_path(
       firmware_type: FirmwareType,
       version: Option<&str>,
   ) -> Result<PathBuf, DllError> {
       let dll_name = firmware_type.dll_name_versioned(version);
       
       if version.is_some() {
           // Check firmware_cache/ for versioned DLLs
           let cache_path = PathBuf::from("firmware_cache").join(&dll_name);
           if cache_path.exists() {
               return Ok(cache_path);
           }
           return Err(DllError::NotFound(dll_name));
       }
       // Fall back to current build for unversioned
       find_dll_path(firmware_type)
   }
   ```

3. **Update DLL cache in runner**:
   ```rust
   // In crates/mcsim-runner/src/node_thread.rs
   // Change from HashMap<FirmwareType, Arc<FirmwareDll>>
   // To: HashMap<(FirmwareType, Option<String>), Arc<FirmwareDll>>
   ```

### Phase 2: Build Scripts

The build scripts are **separate from `build.rs`** and used to populate the firmware cache:

- **`build.rs`** (unchanged): Builds unversioned DLLs from current MeshCore submodule during `cargo build`. Used for day-to-day development.
- **`build_firmware_version.ps1/sh`**: Standalone scripts to build and cache specific versions. Run manually or in CI.

The scripts would:
1. Save current MeshCore submodule state
2. Checkout MeshCore to a specific git tag (e.g., `companion-v1.11.0`)
3. **Apply the simulator patch** from `patches/meshcore-simulator-changes.patch`
4. Run `cargo build --release -p mcsim-firmware` (reuses build.rs logic)
5. Copy output to `firmware_cache/<platform>/` with versioned filenames
6. Restore MeshCore submodule to original state

**Note:** The MeshCore submodule normally points to our fork with simulator patches pre-applied. When building from arbitrary upstream tags, we must apply `patches/meshcore-simulator-changes.patch` to add the necessary simulator support. If the patch fails to apply cleanly (due to upstream changes), it may need to be regenerated for that version.

```powershell
# scripts/build_firmware_version.ps1
param(
    [Parameter(Mandatory=$true)]
    [string]$Version
)

$OriginalCommit = git -C MeshCore rev-parse HEAD
try {
    # Checkout the upstream tag
    git -C MeshCore fetch origin --tags
    git -C MeshCore checkout "companion-v$Version"
    
    # Apply simulator patch
    Write-Host "Applying simulator patch..."
    git -C MeshCore apply ../patches/meshcore-simulator-changes.patch
    if ($LASTEXITCODE -ne 0) {
        throw "Patch failed to apply cleanly. May need to regenerate patch for v$Version"
    }
    
    # Force rebuild of mcsim-firmware
    cargo build --release -p mcsim-firmware
    
    # Copy and rename outputs to cache
    $Platform = "windows"
    New-Item -ItemType Directory -Force "firmware_cache/$Platform"
    $OutDir = Get-ChildItem "target/release/build/mcsim-firmware-*/out" | Select-Object -First 1
    foreach ($dll in Get-ChildItem "$OutDir/meshcore_*.dll") {
        $NewName = $dll.BaseName + "_$Version" + $dll.Extension
        Copy-Item $dll.FullName "firmware_cache/$Platform/$NewName"
    }
    
    Write-Host "Successfully built firmware v$Version"
} finally {
    # Restore original state (discards patch)
    git -C MeshCore checkout .
    git -C MeshCore checkout $OriginalCommit
}
```

### Phase 3: Testing & Documentation

1. Add integration tests with mixed versions
2. Document version compatibility matrix
3. Add CI workflow to build and cache releases

---

## Implementation Details

### DLL Cache Location

Search order:
1. `MCSIM_FIRMWARE_CACHE` environment variable (if set)
2. `firmware_cache/` relative to working directory
3. Current build output (for unversioned/latest)

### Version String Format

Use semantic versioning without `v` prefix:
- `"1.11.0"` (not `"v1.11.0"`)
- Maps to git tag `companion-v1.11.0`, `repeater-v1.11.0`, etc.

### YAML Validation

When loading a model:
1. Collect all unique `(firmware_type, version)` pairs
2. Verify all required DLLs exist
3. Fail fast with clear error message listing missing versions

### API Version Compatibility

Track API version in DLL exports:
```c
// sim_api.h
#define SIM_API_VERSION 2  // Bump when API changes

// Exported function
extern "C" uint32_t sim_get_api_version() {
    return SIM_API_VERSION;
}
```

At load time, verify API version compatibility:
```rust
// If API version mismatch, warn or error
let api_version = dll.get_api_version();
if api_version != EXPECTED_API_VERSION {
    return Err(DllError::ApiVersionMismatch { expected, found: api_version });
}
```

---

## Migration Path

1. **Add Property (Non-Breaking)**: Add `firmware/version` property with null default. Existing YAML files continue to work unchanged.

2. **Implement Versioned Loading**: Modify DLL loading to check cache first. Fall back to current behavior for null version.

3. **Add Build Scripts**: Scripts to checkout, build, and cache versions. Documentation for adding new versions.

4. **Populate Cache**: Build and distribute DLLs for 1.11.0, 1.12.0.

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| API breaking changes between versions | High | Version the sim_api.h and check at load time |
| Build reproducibility | Medium | Pin build tools, document build environment |
| Large repo size from cached DLLs | Low | Don't commit DLLs to git; use release artifacts or separate storage |
| Complex error handling | Medium | Clear error messages, version availability checks at startup |

---

## Success Criteria

1. Can run simulation with nodes on 1.11.0 and 1.12.0 simultaneously
2. Clear error message when requested version not available
3. Build scripts work on Windows, Linux, macOS
4. Documentation for adding new firmware versions
5. No performance regression for single-version simulations
