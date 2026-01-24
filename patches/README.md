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

- **Commit:** `6b52fb32301c273fc78d96183501eb23ad33c5bb`
- **Upstream:** <https://github.com/meshcore-dev/MeshCore.git>
- **Description:** Merge pull request #1310 from LitBomb/patch-22

The submodule points to our fork (<https://github.com/Brent-A/MeshCore.git>) which has these changes already applied at commit `266758fa89ddcc619d253b14f425ff5fdda0fd35`.

### Notes

- If you update the MeshCore submodule to a newer upstream version, the patch may need to be regenerated.
- The simulator's build process (`mcsim-firmware` crate) automatically applies necessary build configurations, so manual patching is typically not required for normal use.
