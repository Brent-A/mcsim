# MeshCore Patches

This directory contains patches for the MeshCore firmware submodule.

## meshcore-simulator-changes.patch

This patch modifies the MeshCore firmware to enable compilation as a shared library for use with the simulator. The changes include:

- Build system modifications for DLL/shared library output
- Simulation-specific adaptations for timing and hardware abstraction

### Applying the Patch

From the repository root:

```bash
cd MeshCore
git apply ../patches/meshcore-simulator-changes.patch
```

### Reverting the Patch

```bash
cd MeshCore
git checkout .
```

### Base Version

This patch is based on upstream MeshCore commit:

- **Commit:** `e738a7477737964464038a87b3113a7a0ce7ebbc`
- **Upstream:** <https://github.com/meshcore-dev/MeshCore.git>
- **Description:** Merge branch 'dev' (v1.12.0)

The submodule points to our fork (<https://github.com/Brent-A/MeshCore.git>) which has these changes already applied at commit `980693bd29eeae03c6bdeffdf376d50f0b793e5e`.

### Notes

- If you update the MeshCore submodule to a newer upstream version, the patch may need to be regenerated.
- The simulator's build process (`mcsim-firmware` crate) automatically applies necessary build configurations, so manual patching is typically not required for normal use.
