# Timing and Determinism in MCSim

This document describes the simulation timing model, determinism guarantees, and how the simulator executes MeshCore firmware in a controlled, reproducible manner.

## Overview

MCSim is designed to provide deterministic, reproducible simulations of MeshCore mesh networks. The simulator accurately models radio propagation, packet collisions, timing-dependent routing behavior, and firmware execution while maintaining the ability to scale to many nodes.

## Simulation Execution Modes

The simulator supports two execution modes:

### 1. Free-Running Mode (Default)

In free-running mode, the simulation processes events as fast as possible without regard to wall clock time. This is useful for:
- Running long simulations quickly
- Batch testing scenarios
- Performance benchmarking

```bash
cargo run -- --model network.yaml --duration 3600  # Run 1 hour of simulation time
```

### 2. Real-Time Mode

In real-time mode, simulation time is synchronized with wall clock time. Events are processed at the rate they would occur in reality. This mode is useful for:
- Interactive testing with external tools
- Debugging with human-observable timing
- Integration with external TCP connections

```bash
cargo run -- --model network.yaml  # No duration = real-time mode
```

## Determinism Guarantees

### Core Determinism

The simulation is fully deterministic when:
- The same model configuration is used
- The same random seed is provided
- No external TCP connections are active

Given these conditions, the simulation will produce identical:
- Packet transmissions and timings
- Collision events
- Routing decisions
- Final network state

### Global Random Seed

A global random seed is used to derive all random values in the simulation:

```bash
cargo run -- --model network.yaml --seed 12345
```

The seed is used to derive per-node RNG seeds, ensuring that:
- Each node has its own deterministic random sequence
- The overall simulation is reproducible
- Node ordering does not affect results

### External Connections and Non-Determinism

When TCP socket connections are enabled for UART bridges, determinism is **not guaranteed**. External data arrives based on wall clock time and user input, which cannot be reproduced across runs.

## Lockstep Time Model

### Global Time Synchronization

The simulation maintains lockstep time across all nodes to properly simulate:
- **Radio collisions**: Overlapping transmissions on the same frequency
- **Packet timing**: Time-on-air calculations for LoRa modulation
- **Retry behavior**: Timeout-based retransmission logic
- **Routing decisions**: Path selection based on timing and availability

### Time Advancement

Time only advances when all nodes are in a halted (yielded) state:

1. All nodes halt and report their next wake time
2. Coordinator determines the next global event time
3. Events are dispatched to relevant nodes
4. Nodes process events and halt again
5. Repeat

```
    ┌──────────────────────────────────────────────────────────────────┐
    │                      Event Loop Cycle                            │
    │                                                                  │
    │   ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐      │
    │   │ Node A  │    │ Node B  │    │ Node C  │    │ Node D  │      │
    │   │ HALTED  │    │ HALTED  │    │ HALTED  │    │ HALTED  │      │
    │   └────┬────┘    └────┬────┘    └────┬────┘    └────┬────┘      │
    │        │              │              │              │           │
    │        ▼              ▼              ▼              ▼           │
    │   ┌────────────────────────────────────────────────────────┐    │
    │   │          Coordinator: Dispatch Events at T=N           │    │
    │   └────────────────────────────────────────────────────────┘    │
    │        │              │                                         │
    │        ▼              ▼                                         │
    │   ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐      │
    │   │ Node A  │    │ Node B  │    │ Node C  │    │ Node D  │      │
    │   │ RUNNING │    │ RUNNING │    │ waiting │    │ waiting │      │
    │   └────┬────┘    └────┬────┘    └─────────┘    └─────────┘      │
    │        │              │                                         │
    │        ▼              ▼                                         │
    │   ┌─────────┐    ┌─────────┐                                    │
    │   │ Node A  │    │ Node B  │    (Nodes yield when blocked)      │
    │   │ HALTED  │    │ HALTED  │                                    │
    │   └─────────┘    └─────────┘                                    │
    │                                                                  │
    │   Coordinator: Advance time to T=N+1, repeat                     │
    └──────────────────────────────────────────────────────────────────┘
```

## Parallel Execution for Performance

### Thread Model

Each firmware node runs in its own thread to enable parallel execution:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Coordinator Process                             │
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
└─────────────────────────────────────────────────────────────────────────┘
```

### Parallel Execution Constraints

Nodes can run in parallel **within a single time step** as long as:
1. They have no direct dependencies (no packet currently in flight between them)
2. They are all executing against the same global time
3. Their outputs are collected before time advances

This allows the simulator to scale to many nodes while maintaining determinism.

## Firmware Simulation

### Design Philosophy

The simulation is designed to be **agnostic to firmware implementation details** while leveraging common patterns in the MeshCore firmware. This allows:
- Firmware modifications without simulator changes
- Testing different firmware versions
- Debugging firmware behavior in isolation

### Firmware Harness

The MeshCore firmware runs as native code compiled into DLLs. A minimal harness adapts the firmware for PC execution by providing implementations of:

| Hardware Interface | Simulation Implementation |
|-------------------|---------------------------|
| Clock (`millis()`) | `SimMillisClock` - coordinator-controlled time |
| Radio (`mesh::Radio`) | `SimRadio` - packet injection/extraction |
| RTC (`RTClib`) | `SimRTCClock` - coordinator-controlled real-time clock |
| Serial (`Serial`) | `SimSerial` - captured output, injected input |
| Filesystem (`SPIFFS`) | `SimFilesystem` - in-memory filesystem |
| RNG (`random()`) | `SimRNG` - seeded deterministic RNG |

### Main Loop Execution Model

MeshCore firmware follows a standard Arduino-style main loop pattern:

```cpp
void setup() {
    // One-time initialization
}

void loop() {
    // Called repeatedly
    mesh->loop();
}
```

The simulator controls when `loop()` executes and what time it observes.

### Timing in MeshCore Firmware

MeshCore firmware schedules actions using future timestamps:

```cpp
// Schedule an advertisement at a future time
nextAdvertTime = currentMillis() + ADVERT_INTERVAL;

// In the main loop, check if it's time
if (currentMillis() >= nextAdvertTime) {
    sendAdvertisement();
    nextAdvertTime = currentMillis() + ADVERT_INTERVAL;
}
```

#### Simulator Wake Time Discovery

For the simulator to efficiently advance time, it must know when each firmware instance needs to wake up. Simply polling at fixed intervals would be wasteful. Instead, the simulator must be **enlightened** about the future timestamps that firmware has scheduled.

**The Problem**: Firmware calculates future wake times internally (e.g., `nextAdvertTime`, `retryTime`, `ackTimeout`), but the simulator has no visibility into these values.

**The Solution**: Hook the `futureMillis()` function—the common function used throughout MeshCore to calculate future timestamps—and have the simulator maintain a per-node list of timestamps of interest.

#### The `futureMillis` Function

MeshCore uses the `Dispatcher::futureMillis()` function as the standard way to schedule future events:

```cpp
// From MeshCore/src/Dispatcher.cpp
unsigned long Dispatcher::futureMillis(int millis_from_now) const {
  return _ms->getMillis() + millis_from_now;
}
```

This function is called throughout the firmware to schedule:

```cpp
// Noise floor calibration (every 2 seconds)
next_floor_calib_time = futureMillis(NOISE_FLOOR_CALIB_INTERVAL);

// Airtime budget enforcement (after each TX)
next_tx_time = futureMillis(t * getAirtimeBudgetFactor());

// AGC reset interval
next_agc_reset_time = futureMillis(getAGCResetInterval());

// Inbound packet delay (score-based)
_mgr->queueInbound(pkt, futureMillis(_delay));

// CAD retry delay
next_tx_time = futureMillis(getCADFailRetryDelay());

// Outbound packet scheduling
_mgr->queueOutbound(packet, priority, futureMillis(delay_millis));

// TX timeout expiry
outbound_expiry = futureMillis(max_airtime);
```

**Hooking Strategy**: By instrumenting `futureMillis()` in the simulator harness, we can capture every scheduled wake time:

```cpp
// Simulator's instrumented version of futureMillis
unsigned long Dispatcher::futureMillis(int millis_from_now) const {
    unsigned long wake_time = _ms->getMillis() + millis_from_now;
    
    // Register this wake time with the simulator
    SIM_CTX()->wake_registry.registerWakeTime(wake_time);
    
    return wake_time;
}
```

#### Wake Time Registry

The simulator maintains a **wake time registry** for each firmware instance:

```cpp
struct WakeTimeRegistry {
    std::set<uint64_t> pending_wake_times;  // Sorted set of future timestamps
    
    void registerWakeTime(uint64_t millis) {
        pending_wake_times.insert(millis);
    }
    
    uint64_t getNextWakeTime() const {
        if (pending_wake_times.empty()) {
            return UINT64_MAX;  // No scheduled wakes
        }
        return *pending_wake_times.begin();  // Return earliest
    }
    
    void clearExpired(uint64_t current_millis) {
        // Remove timestamps that have passed
        pending_wake_times.erase(
            pending_wake_times.begin(),
            pending_wake_times.upper_bound(current_millis)
        );
    }
};
```

#### Common Wake Time Sources in MeshCore

| Source | Field/Variable | Typical Interval |
|--------|---------------|------------------|
| Noise floor calibration | `next_floor_calib_time` | 2 seconds |
| Airtime budget | `next_tx_time` | Variable (after TX) |
| AGC reset | `next_agc_reset_time` | Configurable |
| Inbound queue delay | `queueInbound()` | Score-based (50ms - 32s) |
| Outbound queue delay | `queueOutbound()` | Priority-based |
| TX timeout | `outbound_expiry` | 1.5x airtime |
| CAD retry | `next_tx_time` | 200ms |

#### Selective Node Wake-Up

**Critical optimization**: When time advances to a scheduled wake time, the coordinator should **only wake nodes for which that time is meaningful**—not all nodes.

```rust
fn advance_time_and_wake(&mut self, target_time: SimTime) {
    // Collect nodes that need to wake at this time
    let nodes_to_wake: Vec<EntityId> = self.firmware_entities
        .iter()
        .filter(|fw| fw.has_wake_at(target_time))
        .map(|fw| fw.entity_id())
        .collect();
    
    // Only step the nodes that have pending work
    for node_id in nodes_to_wake {
        self.step_node(node_id, target_time);
    }
    
    // Nodes without wake times at this instant remain halted
}
```

This provides significant performance benefits:
- A 100-node network where only 1 node needs to wake doesn't step 99 idle nodes
- Reduces unnecessary firmware execution
- Maintains determinism (nodes only execute when they have scheduled work)

#### Wake Time Coalescing

An additional optimization is **coalescing nearby wake times**. If multiple nodes (or even the same node) have wake times that are very close together, they can be combined into a single wake event:

```rust
const COALESCE_THRESHOLD_US: u64 = 1000;  // 1ms threshold

fn coalesce_wake_times(&self, wake_times: &[SimTime]) -> Vec<SimTime> {
    let mut sorted: Vec<SimTime> = wake_times.to_vec();
    sorted.sort();
    
    let mut coalesced = Vec::new();
    let mut current_group_start: Option<SimTime> = None;
    
    for wake in sorted {
        match current_group_start {
            None => current_group_start = Some(wake),
            Some(start) if (wake.as_micros() - start.as_micros()) <= COALESCE_THRESHOLD_US => {
                // Within threshold, coalesce into same group (use later time)
            }
            Some(_) => {
                // Outside threshold, start new group
                coalesced.push(wake);
                current_group_start = Some(wake);
            }
        }
    }
    
    if let Some(start) = current_group_start {
        coalesced.push(start);
    }
    
    coalesced
}
```

**Benefits of coalescing**:
- Reduces the number of discrete time steps in the simulation
- Avoids waking the same node twice for events 100μs apart
- Firmware behavior is typically insensitive to sub-millisecond timing differences
- Can dramatically reduce overhead when many nodes schedule similar periodic tasks

**Caution**: The coalescing threshold must be chosen carefully:
- Too large: May affect timing-sensitive behavior (collision detection, precise TX scheduling)
- Too small: Provides minimal benefit
- A threshold of 1ms is typically safe since MeshCore operates on millisecond granularity

#### Coordinator Wake Time Aggregation

When all active nodes yield, the coordinator determines the next global wake time:

```rust
fn calculate_next_wake_time(&self) -> SimTime {
    let mut next_wake = SimTime::MAX;
    
    // Check each firmware's wake registry
    for firmware in &self.firmware_entities {
        let wake = firmware.get_next_wake_time();
        if wake < next_wake {
            next_wake = wake;
        }
    }
    
    // Also consider pending events in the queue
    if let Some(event) = self.event_queue.peek() {
        if event.time < next_wake {
            next_wake = event.time;
        }
    }
    
    next_wake
}
```

This ensures the simulator jumps directly to the next meaningful point in time rather than stepping through empty intervals.

### Efficient Firmware Execution

Rather than advancing time in fixed small intervals (which would be inefficient), the simulator only executes firmware when necessary:

#### Running vs. Halted States

Each firmware instance is in one of two states:

- **Running**: Actively executing firmware code
- **Halted**: Waiting for an external event or time advancement

Time and externally visible state only change when firmware is halted. The simulation only advances global time when **all** nodes are halted.

#### State Transitions

```
                    ┌──────────────────┐
                    │                  │
       ┌───────────►│     HALTED       │◄───────────┐
       │            │                  │            │
       │            └────────┬─────────┘            │
       │                     │                      │
       │      Event arrives  │  (packet, timer,    │
       │      or time passes │   serial data)      │
       │                     ▼                      │
       │            ┌──────────────────┐            │
       │            │                  │            │
       │            │     RUNNING      │            │
       │            │                  │            │
       │            └────────┬─────────┘            │
       │                     │                      │
       │   Firmware blocks   │  (polling, waiting  │
       │   or outputs data   │   for TX complete)  │
       │                     │                      │
       └─────────────────────┴──────────────────────┘
```

#### Event Types That Wake Firmware

1. **Received Packet**: Radio packet arrives for this node
2. **Time Advancement**: Scheduled wake time has been reached
3. **Serial Data**: External data received via TCP bridge
4. **TX Complete**: Radio transmission finished (callback from coordinator)

### Detecting Blocked Firmware

The simulator detects when firmware is blocked and should yield control:

#### Method 1: Idle Main Loop

If the main loop completes at least twice without the firmware outputting anything (no transmissions, UART messages, etc.), the firmware is considered idle.

#### Method 2: Polling Loop Detection

If the firmware appears caught in a polling loop (e.g., repeatedly calling `isSendComplete()`), the simulator detects this via state version tracking:

```cpp
// In SimRadio
void checkForSpin() {
    if (state_version_ != last_polled_version_) {
        // State changed - reset counter
        last_polled_version_ = state_version_;
        poll_count_ = 0;
        return;
    }
    
    poll_count_++;
    if (poll_count_ >= 3) {
        // Firmware is spinning - yield to coordinator
        poll_count_ = 0;
        yield();
    }
}
```

### Yield Reasons

When firmware yields, it reports why:

| Yield Reason | Meaning |
|--------------|---------|
| `IDLE` | Waiting for wake time or external event |
| `RADIO_TX_START` | Started a radio transmission |
| `RADIO_TX_COMPLETE` | Radio transmission finished |
| `REBOOT` | Node requested reboot |
| `POWER_OFF` | Node requested power off |
| `ERROR` | An error occurred |

### Deterministic Work Per Step

**Critical for determinism**: The amount of work done by firmware in any given step must be deterministic. This means:

- The same inputs produce the same number of loop iterations
- Spin detection thresholds are consistent
- Time queries return the same value throughout a step

## Implementation Status

### Current Implementation

The current implementation provides:

✅ **Working Features**:
- Event-driven simulation with priority queue (`BinaryHeap<Event>`)
- Per-node threaded firmware execution via DLLs
- Thread-local storage for per-node global state
- Async stepping API (`sim_step_begin` / `sim_step_wait`)
- Basic spin detection for polling loops (3 consecutive polls trigger yield)
- Deterministic RNG seeding per node via `SimNodeConfig.rng_seed`
- Radio collision detection in `mcsim-lora::Radio`
- Link model with SNR/RSSI for packet propagation
- State version tracking in `SimRadio` for spin detection
- `sim_notify_state_change()` API for coordinator-driven state updates

⚠️ **Partial Implementation**:
- Firmware yields after single loop iteration (not double-loop detection)
- Fixed 100ms default wake interval when idle
- Sequential event dispatch (parallel infrastructure exists but not utilized)

### Required Changes

The following changes are needed to fully meet the design:

#### 1. Parallel Node Stepping (Priority: High)

**Current**: Nodes are stepped sequentially via synchronous `step()` calls in the event loop. The async API (`sim_step_begin`/`sim_step_wait`) exists but is not utilized for parallel execution.

**Required**: Enable parallel stepping of multiple nodes within a time slice when they have no direct dependencies.

**Files to modify**:
- `crates/mcsim-runner/src/main.rs` - `EventLoop::run()` and `run_realtime()`
- `crates/mcsim-firmware/src/lib.rs` - Firmware entity `handle_event()`

**Implementation**:
```rust
// Current (sequential):
for entity in targets {
    entity.handle_event(&event, ctx)?;
}

// Required (parallel):
// 1. Identify independent events (different targets, same time)
// 2. Call sim_step_begin() on all independent nodes
// 3. Call sim_step_wait() to collect results (can be done in parallel)
// 4. Process outputs and generate new events
```

#### 2. Double-Loop Idle Detection (Priority: Medium)

**Current**: `sim_node_base.h::threadMain()` runs exactly one `loop()` call per step and yields immediately after.

**Required**: Detect idle by running loop twice without any output (TX, serial, etc.).

**Files to modify**:
- `simulator/common/include/sim_node_base.h` - `SimNodeImpl::threadMain()`
- `simulator/common/include/sim_context.h` - Add output tracking

**Implementation**:
```cpp
// In SimNodeImpl::threadMain()
int loops_without_output = 0;
while (loops_without_output < 2) {
    size_t output_before = ctx.serial_tx_buffer.size();
    bool had_tx_before = _sim_radio_instance.hasPendingTx();
    
    loop();
    
    bool had_output = _sim_radio_instance.hasPendingTx() ||
                      ctx.serial_tx_buffer.size() > output_before ||
                      /* other output checks */;
    
    if (had_output) {
        loops_without_output = 0;
        if (_sim_radio_instance.hasPendingTx()) break; // TX needs immediate handling
    } else {
        loops_without_output++;
    }
}
```

#### 3. Firmware Wake Time Reporting (Priority: Medium)

**Current**: Fixed 100ms wake interval in `sim_node_base.h`:
```cpp
ctx.step_result.wake_millis = ctx.current_millis + 100; // Wake in 100ms
```

**Required**: Firmware should report actual next wake time based on scheduled timers.

**Files to modify**:
- `simulator/common/include/sim_node_base.h` - Wake time calculation
- `simulator/*/sim_main.cpp` - Per-firmware wake time extraction
- `MeshCore/src/Mesh.cpp` - Expose next scheduled time

**Implementation approaches**:
1. **Firmware introspection**: Extract next advert time, retry time, etc. from mesh state
2. **Callback registration**: Firmware registers wake time callbacks
3. **Pessimistic approach**: Short interval but with efficient spin detection (current partial solution)

#### 4. Deterministic Work Per Step (Priority: High)

**Current**: The amount of work per step can vary based on spin detection threshold and loop behavior.

**Required**: Guarantee deterministic work amount for reproducibility.

**Files to modify**:
- `simulator/common/src/sim_radio.cpp` - `checkForSpin()`
- `simulator/common/include/sim_node_base.h` - Loop termination logic

**Implementation**:
- Make spin detection threshold configurable and consistent
- Ensure loop count per step is deterministic given same inputs
- Add assertion/logging for unexpected work variations

#### 5. Coordinator State Version Management (Priority: Medium)

**Current**: State version is tracked locally in `SimRadio` and incremented on internal state changes. Coordinator can call `sim_notify_state_change()` but doesn't track a global version.

**Required**: Coordinator should manage and propagate state versions for consistent spin detection.

**Files to modify**:
- `crates/mcsim-firmware/src/lib.rs` - Track state version per firmware entity
- `crates/mcsim-lora/src/lib.rs` - `Radio` entity state version management

**Implementation**:
```rust
// In Radio entity
fn handle_event(&mut self, event: &Event, ctx: &mut SimContext) {
    self.state_version += 1;  // Increment on any state change
    
    // Notify attached firmware
    ctx.post_immediate(
        vec![self.attached_firmware],
        EventPayload::RadioStateChanged(RadioStateChangedEvent {
            new_state: self.state,
            state_version: self.state_version,
        }),
    );
}
```

#### 6. Real-Time Mode Improvements (Priority: Low)

**Current**: Basic 1ms sleep-based pacing in `run_realtime()`.

**Required**: More accurate real-time tracking.

**Files to modify**:
- `crates/mcsim-runner/src/main.rs` - `EventLoop::run_realtime()`

**Implementation**:
- Use `std::time::Instant` for high-resolution timing
- Implement catch-up logic when simulation falls behind
- Add `--speed` multiplier option (e.g., `--speed 2.0` for 2x)

#### 7. Determinism Test Infrastructure (Priority: Low)

**Current**: No automated determinism verification.

**Required**: Test that verifies identical outputs across runs.

**Files to create**:
- `crates/mcsim-runner/tests/determinism_test.rs`

**Implementation**:
```rust
#[test]
fn test_determinism() {
    let config = RunnerConfig {
        seed: Some(12345),
        duration: Some(60.0),
        ..default()
    };
    
    let result1 = run_simulation(config.clone())?;
    let result2 = run_simulation(config)?;
    
    assert_eq!(result1.total_events, result2.total_events);
    assert_eq!(result1.packets_transmitted, result2.packets_transmitted);
    // Compare full event traces
}

## Configuration

### Simulation Parameters

```yaml
# Example model configuration
simulation:
  seed: 12345              # Global RNG seed
  
nodes:
  - name: repeater1
    type: repeater
    rng_seed: auto         # Derived from global seed
```

### CLI Options

```bash
mcsim --model network.yaml \
      --seed 12345 \           # Deterministic seed
      --duration 3600 \        # Run for 1 hour (sim time)
      --output trace.json \    # Record event trace with full packet data
      --verbose                # Print progress
```

### Event Trace Output

The `--output` flag records a JSON trace file containing all simulation events. Each trace entry includes:

- **Basic metadata**: origin (node name), origin_id, timestamp, event type
- **Radio metrics** (for packet events):
  - TX: `RSSI` (transmit power in dBm)
  - RX: `SNR` (signal-to-noise ratio), `RSSI` (received signal strength)
- **Packet identification**:
  - `payload_hash`: 16-character hex hash identifying unique packet content (stable across routing hops)
- **Full packet data** (for TX/RX events):
  - `packet_hex`: Raw packet payload as hex-encoded string
  - `packet`: Decoded MeshCore packet structure (if decode succeeds)
- **Packet timing** (for TX and RX events):
  - `packet_start_time_s`: Transmission start time in seconds
  - `packet_end_time_s`: Transmission end time in seconds
- **Reception status** (for RX events only):
  - `reception_status`: "ok", "collided", or "weak"
- **Timer events**:
  - `timer_id`: Timer identifier (no direction, SNR, or RSSI fields)
- **Firmware log events**:
  - `type`: "LOG"
  - `line`: Single log line emitted by firmware (from `Serial.print` / debug macros)

**Example TX trace entry** (abbreviated for brevity):
```json
{
  "origin": "Alice",
  "origin_id": "1",
  "timestamp": "2025-01-01T00:00:00.500Z",
  "type": "PACKET",
  "direction": "TX",
  "RSSI": "20 dBm",
  "payload_hash": "1A2B3C4D5E6F7890",
  "packet_hex": "01a4000102030405...",
  "packet": {
    "header": {
      "route_type": "Flood",
      "payload_type": "Advert",
      "version": "V1"
    },
    "path": [],
    "payload": {
      "Advert": {
        "public_key": [...],
        "timestamp": 1234567890,
        "signature": [...],
        "alias": "Node1"
      }
    }
  },
  "packet_start_time_s": 0.500,
  "packet_end_time_s": 0.750
}
```

**Example RX trace entry** (abbreviated for brevity):
```json
{
  "origin": "Repeater1",
  "origin_id": "2",
  "timestamp": "2025-01-01T00:00:00.750Z",
  "type": "PACKET",
  "direction": "RX",
  "SNR": "15.5 dB",
  "RSSI": "-85.2 dBm",
  "payload_hash": "1A2B3C4D5E6F7890",
  "packet_hex": "01a4000102030405...",
  "packet": {
    "header": {
      "route_type": "Flood",
      "payload_type": "Advert",
      "version": "V1"
    },
    "path": [],
    "payload": {
      "Advert": {
        "public_key": [...],
        "timestamp": 1234567890,
        "signature": [...],
        "alias": "Node1"
      }
    }
  },
  "packet_start_time_s": 0.500,
  "packet_end_time_s": 0.750,
  "reception_status": "ok"
}
```

**Example LOG trace entry** (abbreviated for brevity):
```json
{
  "origin": "Repeater1",
  "origin_id": "2",
  "timestamp": "2025-01-01T00:00:01.000Z",
  "type": "LOG",
  "line": "DEBUG: recv matches no peers, src_hash=AF"
}
```

**Example Timer trace entry**:
```json
{
  "origin": "Alice",
  "origin_id": "2",
  "timestamp": "2025-01-01T00:00:02.010Z",
  "type": "TIMER",
  "timer_id": 1
}
```

Non-packet events (timers, messages) omit the packet-specific fields (direction, SNR, RSSI, payload_hash, etc.).

## Debugging Tips

### Tracing Specific Entities

Use the `--trace` flag to enable detailed logging:

```bash
mcsim --model network.yaml --trace "Alice,Bob"
mcsim --model network.yaml --trace "*"  # All entities
```

### Filtering JSON Trace Output

Use `jq` to filter the `--output` trace and keep only the fields you care about:

```bash
jq '.[] | select((.type != "TIMER"))
| {type, timestamp, origin, direction, payload_hash, packet_start_time_s, packet_end_time_s, reception_status, line}
| with_entries(select(.value != null))' trace.json
```

Example output:

```json
{
  "type": "LOG",
  "timestamp": "2025-01-01T00:02:02.000Z",
  "origin": "Repeater2",
  "line": "DEBUG: 00:00:00 - 1/1/2024 U Dispatcher::loop(): ERROR: send start failed!"
}
{
  "type": "PACKET",
  "timestamp": "2025-01-01T00:02:02.000Z",
  "origin": "Repeater3",
  "direction": "TX",
  "payload_hash": "841DBF8FD9CC7CEF",
  "packet_start_time_s": 122.0001,
  "packet_end_time_s": 122.399972
}
```

### Reproducing Issues

1. Note the seed from the run: `Using seed: 12345`
2. Re-run with same seed: `--seed 12345`
3. Enable tracing for relevant entities
4. Compare event sequences

### Common Issues

| Symptom | Likely Cause | Solution |
|---------|--------------|----------|
| Different results with same seed | External TCP connection | Disable TCP for deterministic runs |
| Firmware appears stuck | Spin detection not triggering | Check state version propagation |
| Missed packets | Collision or timing issue | Enable radio tracing |
| Slow simulation | Too many small time steps | Check idle detection logic |
