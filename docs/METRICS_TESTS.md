# MCSim Metrics Test Plan

This document outlines the test suite for verifying the metrics collection system. Each test uses known seeds for deterministic behavior and validates specific metrics and their labels.

## Table of Contents

1. [Test Design Philosophy](#test-design-philosophy)
2. [Test Infrastructure](#test-infrastructure)
3. [Test Scenarios](#test-scenarios)
   - [Test 1: Basic Radio Metrics](#test-1-basic-radio-metrics)
   - [Test 2: Multi-Hop Forwarding Metrics](#test-2-multi-hop-forwarding-metrics)
   - [Test 3: Direct Message Metrics](#test-3-direct-message-metrics)
   - [Test 4: Flood Propagation Metrics](#test-4-flood-propagation-metrics)
   - [Test 5: Collision and Weak Signal Metrics](#test-5-collision-and-weak-signal-metrics)
   - [Test 6: Room Server and Channel Metrics](#test-6-room-server-and-channel-metrics)
   - [Test 7: Node Type Label Aggregation](#test-7-node-type-label-aggregation)
4. [Metrics Coverage Matrix](#metrics-coverage-matrix)
5. [Label Verification](#label-verification)

---

## Test Design Philosophy

### Goals

1. **Minimize Simulation Count**: Each test should verify as many metrics as possible in a single run
2. **Reproducible Results**: Use fixed seeds for deterministic simulation behavior
3. **Tolerant Assertions**: Use expected ranges rather than exact values to accommodate minor implementation changes
4. **Label Coverage**: Verify all label combinations for each metric
5. **Short Duration**: Tests should complete quickly (typically 5-30 seconds of sim time)
6. **Known Topology**: Use simple topologies from `examples/topologies/` for predictable behavior

### Test Structure

Each test will:
1. Load a topology from `examples/topologies/`
2. Apply a behavior overlay (or use inline agents)
3. Run with `--seed <known_seed> --duration <short> --metrics-output json`
4. Parse the JSON output and verify metrics fall within expected ranges

### Determinism and Tolerance

- All tests use explicit `--seed` values for reproducible behavior
- Tests specify **expected ranges** (min, max) rather than exact values
- Ranges should be tight enough to catch regressions but tolerant of:
  - Minor timing changes in firmware logic
  - Refactors that change event ordering slightly
  - Improvements to routing efficiency
- If a test fails, investigate whether it's a bug or an acceptable behavioral change
- Seeds guarantee the same random choices; ranges accommodate implementation evolution

---

## Test Infrastructure

### Test Harness Location

```
crates/mcsim-runner/tests/
├── metrics_test.rs           # Main metrics test file
├── fixtures/
│   └── behaviors/
│       ├── single_broadcast.yaml    # Alice sends exactly 1 channel message
│       ├── single_dm.yaml           # Alice sends exactly 1 DM to Bob
│       └── burst_traffic.yaml       # Multiple nodes send traffic (collision test)
```

### Helper Functions

```rust
/// Run simulation and return parsed metrics
fn run_and_collect_metrics(
    topology: &str,
    behavior: Option<&str>,
    seed: u64,
    duration: &str,
    metric_filters: &[&str],
) -> MetricsResult;

/// Assert a counter metric falls within expected range
fn assert_counter_in_range(
    result: &MetricsResult,
    metric_name: &str,
    labels: &[(&str, &str)],
    min: u64,
    max: u64,
);

/// Assert a counter metric is at least the minimum value
fn assert_counter_min(
    result: &MetricsResult,
    metric_name: &str,
    labels: &[(&str, &str)],
    min: u64,
);

/// Assert a histogram metric has count and mean within expected ranges
fn assert_histogram_in_range(
    result: &MetricsResult,
    metric_name: &str,
    labels: &[(&str, &str)],
    count_min: u64,
    count_max: u64,
    mean_min: f64,
    mean_max: f64,
);

/// Assert a metric with specific labels exists (value > 0)
fn assert_metric_exists(
    result: &MetricsResult,
    metric_name: &str,
    labels: &[(&str, &str)],
);
```

---

## Test Scenarios

### Test 1: Basic Radio Metrics

**Objective**: Verify fundamental radio TX/RX counters and airtime tracking

**Topology**: `examples/topologies/two_peers.yaml`
```
[Alice:Companion] <---> [Repeater] <---> [Bob:Companion]
```

**Behavior**: Alice sends 1 channel message (broadcast)

**Duration**: 10s (enough for initial adverts + 1 message)

**Seed**: 42

**Metrics Verified**:

| Metric | Node | Labels | Expected Range |
|--------|------|--------|----------------|
| `mcsim.radio.tx_packets` | Alice | node_type=Companion | [2, 10] |
| `mcsim.radio.tx_packets` | Repeater | node_type=Repeater | [1, 10] |
| `mcsim.radio.rx_packets` | Repeater | node_type=Repeater | [1, 10] |
| `mcsim.radio.tx_airtime_us` | Alice | node_type=Companion | [1000, ∞) |
| `mcsim.radio.rx_airtime_us` | Repeater | node_type=Repeater | [1000, ∞) |
| `mcsim.radio.tx_packet_size_bytes` | Alice | node_type=Companion | count: [2, 10] |
| `mcsim.radio.rx_packet_size_bytes` | Repeater | node_type=Repeater | count: [1, 10] |
| `mcsim.radio.rx_snr_db` | Repeater | node_type=Repeater | mean: [10, 20] dB |
| `mcsim.radio.rx_rssi_dbm` | Repeater | node_type=Repeater | exists |

**Label Verification**:
- All metrics have `node` label with correct node name
- All metrics have `node_type` label (Companion or Repeater)
- Verify `group` label if nodes have groups defined

---

### Test 2: Multi-Hop Forwarding Metrics

**Objective**: Verify packet forwarding through repeater chain

**Topology**: `examples/topologies/simple.yaml`
```
[Alice:Companion] <---> [R1] <---> [R2] <---> [R3] <---> [Bob:Companion]
```

**Behavior**: Alice sends 1 channel message, expect propagation through all repeaters

**Duration**: 15s

**Seed**: 12345

**Metrics Verified**:

| Metric | Node | Labels | Expected Range |
|--------|------|--------|----------------|
| `mcsim.radio.tx_packets` | Alice | | [2, 10] |
| `mcsim.radio.rx_packets` | Repeater1 | | [1, 10] |
| `mcsim.radio.rx_packets` | Repeater2 | | [1, 10] |
| `mcsim.radio.rx_packets` | Repeater3 | | [1, 10] |
| `mcsim.radio.rx_packets` | Bob | | [1, 10] |
| `mcsim.packet.tx_flood` | Alice | packet_type=TXT | [1, 3] |
| `mcsim.packet.tx_flood` | Repeater1 | packet_type=TXT | [1, 3] |
| `mcsim.packet.tx_flood` | Repeater2 | packet_type=TXT | [1, 3] |
| `mcsim.packet.tx_flood` | Repeater3 | packet_type=TXT | [1, 3] |

**Label Verification**:
- `packet_type` label present on packet metrics (ADVERT, TXT, etc.)
- `route_type` label distinguishes flood vs direct

---

### Test 3: Direct Message Metrics

**Objective**: Verify direct message delivery tracking and latency

**Topology**: `examples/topologies/two_peers.yaml`
```
[Alice:Companion] <---> [Repeater] <---> [Bob:Companion]
```

**Behavior**: Alice sends 1 direct message to Bob

**Duration**: 60s (allow time for routing discovery + ACK)

**Seed**: 99999

**Metrics Verified**:

| Metric | Node | Labels | Expected Range |
|--------|------|--------|----------------|
| `mcsim.dm.sent` | Alice | dest_type=dm | [1, 1] |
| `mcsim.dm.delivered` | Bob | dest_type=dm | [1, 1] |
| `mcsim.dm.acked` | Alice | | [1, 1] |
| `mcsim.dm.delivery_latency_ms` | Alice | | count: [1, 1], mean: [100, 30000] ms |
| `mcsim.dm.ack_latency_ms` | Alice | | count: [1, 1], mean: [100, 30000] ms |
| `mcsim.packet.tx_direct` | Alice | packet_type=TXT | [1, 5] |
| `mcsim.path.sent` | | origin_node=Alice, dest_node=Bob | [1, 1] |
| `mcsim.path.delivered` | | origin_node=Alice, dest_node=Bob | [1, 1] |
| `mcsim.path.delivery_latency_ms` | | | count: [1, 1] |
| `mcsim.path.hops` | | | count: [1, 1], mean: [1, 2] |

**Label Verification**:
- `dest_type` label (dm, channel, broadcast)
- `origin_node` and `dest_node` labels on delivery metrics

---

### Test 4: Flood Propagation Metrics

**Objective**: Verify flood coverage and propagation timing

**Topology**: `examples/topologies/multi_path.yaml` (3x3 grid with 4 companions)
```
                [Charlie]
                |  |  |
        +--[R1]---[R2]---[R3]--+
        |   |      |      |   |
[Alice] --[R4]---[R5]---[R6]-- [Bob]
        |   |      |      |   |
        +--[R7]---[R8]---[R9]--+
                |  |  |
                [Dave]
```

**Behavior**: Alice sends 1 channel message (flood)

**Duration**: 20s

**Seed**: 77777

**Metrics Verified**:

| Metric | Labels | Expected Range |
|--------|--------|----------------|
| `mcsim.flood.nodes_reached` | origin_node=Alice | count: [1, 3], value: [8, 13] |
| `mcsim.flood.propagation_time_us` | origin_node=Alice | count: [1, 3] |
| `mcsim.flood.coverage` | origin_node=Alice | [0.7, 1.0] |
| `mcsim.flood.times_heard` | node=Bob | count: [1, 5] |

**Label Verification**:
- `origin_node` label on flood metrics
- Per-node `times_heard` with `node` label

---

### Test 5: Collision and Weak Signal Metrics

**Objective**: Verify collision detection and SNR threshold handling

**Topology**: `examples/topologies/diamond.yaml`
```
       [Repeater1]
      /           \
[Alice]           [Bob]
      \           /
       [Repeater2]
```

**Behavior**: Both Alice and Bob send simultaneously (causes collision at repeaters)

**Duration**: 10s

**Seed**: 54321 (chosen to ensure collision timing)

**Metrics Verified**:

| Metric | Node | Labels | Expected Range |
|--------|------|--------|----------------|
| `mcsim.radio.rx_collided` | Repeater1 | | [1, 20] |
| `mcsim.radio.rx_collided` | Repeater2 | | [1, 20] |
| `mcsim.radio.rx_packets` | Repeater1 | | [0, 20] |
| `mcsim.radio.rx_weak` | * | | [0, 10] (verify metric exists) |

**Special Setup**: Custom behavior YAML that triggers simultaneous transmission:
```yaml
nodes:
  - name: "Alice"
    agent:
      channel:
        enabled: true
        initial_delay_ms: 100  # Both send at ~same time
  - name: "Bob"
    agent:
      channel:
        enabled: true
        initial_delay_ms: 100
```

**Label Verification**:
- Collision metrics properly attributed to receiving node

---

### Test 6: Room Server and Channel Metrics

**Objective**: Verify room server specific metrics and channel messaging

**Topology**: `examples/topologies/room_server.yaml`
```
[Alice:Companion] <---> [ChatRoom:RoomServer] <---> [Bob:Companion]
```

**Behavior**: Alice sends channel message, Bob receives via room server

**Duration**: 30s

**Seed**: 11111

**Metrics Verified**:

| Metric | Node | Labels | Expected Range |
|--------|------|--------|----------------|
| `mcsim.radio.tx_packets` | Alice | node_type=Companion | [1, 10] |
| `mcsim.radio.tx_packets` | ChatRoom | node_type=RoomServer | [1, 10] |
| `mcsim.radio.rx_packets` | ChatRoom | node_type=RoomServer | [1, 10] |
| `mcsim.radio.rx_packets` | Bob | node_type=Companion | [1, 10] |
| `mcsim.dm.sent` | Alice | dest_type=channel | [1, 1] |
| `mcsim.dm.delivered` | Bob | dest_type=channel | [1, 1] |
| `mcsim.packet.tx_flood` | Alice | packet_type=TXT | [1, 3] |

**Label Verification**:
- `node_type=RoomServer` correctly applied
- `dest_type=channel` for channel messages

---

### Test 7: Node Type Label Aggregation

**Objective**: Verify metrics can be aggregated by node_type label

**Topology**: `examples/topologies/simple.yaml` (3 Repeaters, 2 Companions)

**Behavior**: Normal traffic with adverts

**Duration**: 15s

**Seed**: 33333

**Metrics Verified**:

| Aggregation Query | Expected |
|-------------------|----------|
| `sum(mcsim.radio.tx_packets{node_type="Repeater"})` | Sum of all 3 repeaters |
| `sum(mcsim.radio.tx_packets{node_type="Companion"})` | Sum of Alice + Bob |
| `count(mcsim.radio.tx_packets) by (node_type)` | 2 groups: Repeater, Companion |

**Label Verification**:
- Every metric includes `node_type`
- Values are consistent (Repeater or Companion or RoomServer)

---

## Metrics Coverage Matrix

This matrix shows which tests cover which metrics:

| Metric | Test 1 | Test 2 | Test 3 | Test 4 | Test 5 | Test 6 | Test 7 |
|--------|:------:|:------:|:------:|:------:|:------:|:------:|:------:|
| **Radio/PHY Layer** |
| `mcsim.radio.tx_packets` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `mcsim.radio.rx_packets` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `mcsim.radio.tx_airtime_us` | ✓ | ✓ | | | | | ✓ |
| `mcsim.radio.rx_airtime_us` | ✓ | ✓ | | | | | |
| `mcsim.radio.rx_collided` | | | | | ✓ | | |
| `mcsim.radio.rx_weak` | | | | | ✓ | | |
| `mcsim.radio.tx_packet_size_bytes` | ✓ | | | | | | |
| `mcsim.radio.rx_packet_size_bytes` | ✓ | | | | | | |
| `mcsim.radio.rx_snr_db` | ✓ | | | | | | |
| `mcsim.radio.rx_rssi_dbm` | ✓ | | | | | | |
| `mcsim.radio.turnaround_time_us` | | ✓ | | | | | |
| `mcsim.radio.active_receptions` | | | | | ✓ | | |
| **Packet Layer** |
| `mcsim.packet.tx_flood` | | ✓ | | ✓ | | ✓ | |
| `mcsim.packet.tx_direct` | | | ✓ | | | | |
| `mcsim.packet.rx_flood` | | ✓ | | ✓ | | | |
| `mcsim.packet.rx_direct` | | | ✓ | | | | |
| **Direct Message Layer** |
| `mcsim.dm.sent` | | | ✓ | | | ✓ | |
| `mcsim.dm.delivered` | | | ✓ | | | ✓ | |
| `mcsim.dm.acked` | | | ✓ | | | | |
| `mcsim.dm.delivery_latency_ms` | | | ✓ | | | | |
| `mcsim.dm.ack_latency_ms` | | | ✓ | | | | |
| **Flood Propagation** |
| `mcsim.flood.nodes_reached` | | | | ✓ | | | |
| `mcsim.flood.propagation_time_us` | | | | ✓ | | | |
| `mcsim.flood.coverage` | | | | ✓ | | | |
| `mcsim.flood.times_heard` | | | | ✓ | | | |
| **Path Delivery** |
| `mcsim.path.sent` | | | ✓ | | | | |
| `mcsim.path.delivered` | | | ✓ | | | | |
| `mcsim.path.failed` | | | | | ✓ | | |
| `mcsim.path.delivery_latency_ms` | | | ✓ | | | | |
| `mcsim.path.hops` | | | ✓ | | | | |

---

## Label Verification

### Required Labels (All Metrics)

| Label | Description | Verified In |
|-------|-------------|-------------|
| `node` | Node name | All tests |
| `node_type` | Firmware type | Test 1, 6, 7 |
| `group` | Custom group (if defined) | Test 7 (with groups) |
| `network` | Simulation run ID | All tests |

### Metric-Specific Labels

| Metric Category | Label | Values | Verified In |
|-----------------|-------|--------|-------------|
| Packet metrics | `packet_type` | ADVERT, TXT, ACK, TRACE, PATH, SYNC | Test 2, 3 |
| Packet metrics | `route_type` | flood, direct | Test 2, 3 |
| Message metrics | `dest_type` | dm, channel, broadcast | Test 3, 6 |
| Message metrics | `reason` | timeout, no_route, rejected | Test 5 |
| Direct delivery | `origin_node` | Node name | Test 3 |
| Direct delivery | `dest_node` | Node name | Test 3 |
| Flood propagation | `origin_node` | Node name | Test 4 |
| Turnaround | `direction` | tx_to_rx, rx_to_tx | Test 2 |

---

## Running the Tests

### Run All Metric Tests

```bash
cargo test --test metrics_test
```

### Run Individual Test

```bash
cargo test --test metrics_test test_basic_radio_metrics
```

### Debug Mode (Show Metrics Output)

```bash
METRICS_TEST_DEBUG=1 cargo test --test metrics_test -- --nocapture
```

### Regenerate Expected Values

When firmware behavior changes, regenerate expected values:

```bash
cargo run -- run examples/topologies/two_peers.yaml examples/behaviors/broadcast.yaml \
    --seed 42 --duration 10s --metrics-output json
```

---

## Notes on Expected Ranges

### Range Design Principles

1. **Tight but Tolerant**: Ranges should be narrow enough to catch bugs but wide enough to survive refactors
2. **Minimum Guarantees**: Most tests should verify minimums ("at least N packets") rather than exact counts
3. **Behavioral Invariants**: Focus on relationships ("RX ≤ TX", "delivered ≤ sent") over absolute values
4. **Order of Magnitude**: For histograms, verify values are in the right ballpark (e.g., latency 100-5000ms, not 1ms or 1 hour)

### Choosing Range Values

When setting expected ranges:

| Metric Type | Range Strategy | Example |
|-------------|----------------|----------|
| TX packets | min based on behavior | `min: 2` (1 advert + 1 message) |
| RX packets | min based on topology | `min: 1` (at least heard from neighbor) |
| Airtime | proportional to packets | `min: packet_count * min_airtime` |
| Latency | based on hop count + processing | `min: 50ms, max: 10s` |
| Delivery rate | based on link quality | `min: 0.8` for good links |

### Updating Ranges

When a test fails:

1. **Check if it's a regression**: Did behavior meaningfully change?
2. **Check if range is too tight**: Run 10x with same seed, find actual variance
3. **Widen conservatively**: Prefer widening max over lowering min
4. **Document changes**: Note why range was adjusted in commit message

### When to Use Exact Values

Some scenarios warrant exact assertions:

- Single-message tests: Exactly 1 message sent → exactly 1 `message.sent`
- Node counts: Topology has exactly N nodes → exactly N unique `node` labels
- Zero conditions: No collisions expected → `rx_collided` in range `[0, 1]`

---

## Future Test Additions

### When to Add New Tests

- New metric added to the system
- New label dimension added
- Bug fix that changes metric behavior
- Performance regression test (metric collection overhead)

### Test Naming Convention

```
test_<metric_category>_<scenario>
```

Examples:
- `test_radio_basic_transmission`
- `test_packet_flood_forwarding`
- `test_message_dm_delivery`
- `test_collision_simultaneous_tx`
