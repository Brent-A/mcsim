# MeshCore Simulator

This directory contains a simulation framework for MeshCore firmware. It builds the firmware as Windows DLLs that can be loaded and controlled by an external coordinator (intended to be implemented in Rust).

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                         Rust Coordinator Process                        │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │                     Thread Pool (one per node)                   │   │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐                  │   │
│  │  │ Thread 1   │  │ Thread 2   │  │ Thread 3   │                  │   │
│  │  │ repeater   │  │ room_svr   │  │ companion  │                  │   │
│  │  │ .dll       │  │ .dll       │  │ .dll       │                  │   │
│  │  └─────┬──────┘  └─────┬──────┘  └─────┬──────┘                  │   │
│  │        │               │               │                         │   │
│  │        ▼               ▼               ▼                         │   │
│  │  ┌─────────────────────────────────────────────────────────────┐ │   │
│  │  │              Barrier Sync Point (all threads wait)          │ │   │
│  │  └─────────────────────────────────────────────────────────────┘ │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────────────┐  │
│  │ Time Engine │  │ Radio Sim   │  │ TCP Server (serial bridges)     │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

## Building

The simulator DLLs are built automatically as part of the Rust build process via the `mcsim-firmware` crate's build script.

### Prerequisites

- LLVM/Clang (with `clang-cl` on Windows)
- Rust toolchain

### Build Commands

```bash
# Build from project root
cargo build -p mcsim-firmware
```

This produces in `target/debug/build/mcsim-firmware-*/out/`:

- `meshcore_repeater.dll` - Repeater node firmware
- `meshcore_room_server.dll` - Room server node firmware  
- `meshcore_companion.dll` - Companion radio node firmware

## C API

Each DLL exports the same C API defined in `common/include/sim_api.h`:

### Lifecycle

```c
// Create a node with the given configuration
SimNodeHandle sim_create(const SimNodeConfig* config);

// Destroy a node
void sim_destroy(SimNodeHandle node);

// Reboot a node (preserves filesystem)
void sim_reboot(SimNodeHandle node, const SimNodeConfig* config);
```

### Async Stepping

```c
// Begin a simulation step (non-blocking, wakes node thread)
void sim_step_begin(SimNodeHandle node, uint64_t sim_millis, uint32_t sim_rtc_secs);

// Wait for step to complete (blocking)
SimStepResult sim_step_wait(SimNodeHandle node);

// Combined step (blocking convenience function)
SimStepResult sim_step(SimNodeHandle node, uint64_t sim_millis, uint32_t sim_rtc_secs);
```

### Event Injection

```c
// Inject a received radio packet
void sim_inject_radio_rx(SimNodeHandle node, const uint8_t* data, size_t len,
                         float rssi, float snr);

// Inject serial data (from TCP bridge)
void sim_inject_serial_rx(SimNodeHandle node, const uint8_t* data, size_t len);

// Notify that a TX completed
void sim_notify_tx_complete(SimNodeHandle node);
```

### Filesystem Access

```c
// Write a file to the node's in-memory filesystem
int sim_fs_write(SimNodeHandle node, const char* path, 
                 const uint8_t* data, size_t len);

// Read a file
int sim_fs_read(SimNodeHandle node, const char* path,
                uint8_t* data, size_t max_len);

// Check if file exists
int sim_fs_exists(SimNodeHandle node, const char* path);

// Delete a file
int sim_fs_remove(SimNodeHandle node, const char* path);
```

## Simulation Flow

1. **Coordinator creates nodes** via `sim_create()` with configuration
2. **Coordinator calls `sim_step_begin()`** on all nodes with the current simulation time
3. **Coordinator calls `sim_step_wait()`** on each node to get results
4. **Coordinator processes results**:
   - `SIM_YIELD_IDLE`: Node is waiting, note `wake_millis` for scheduling
   - `SIM_YIELD_RADIO_TX_START`: Node is transmitting, simulate propagation
   - `SIM_YIELD_REBOOT`: Node requested reboot
   - `SIM_YIELD_POWER_OFF`: Node requested power off
5. **Coordinator advances time** to the next event (min of all wake times)
6. **Coordinator injects events** (radio RX for any node that should receive the TX)
7. **Repeat from step 2**

## Determinism

For reproducible simulations:

- RNG is seeded via `SimNodeConfig.rng_seed`
- Time is externally controlled (no real-time dependencies)
- All I/O is captured and can be replayed

## Thread Model

Each node runs in its own thread within the coordinator process:

- Thread-local storage (TLS) provides per-node global state
- Nodes block on a condition variable between steps
- The coordinator wakes nodes and waits for them to yield

This allows loading multiple instances of the same DLL for simulating multiple nodes of the same type.
