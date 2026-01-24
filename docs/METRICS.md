# MCSim Metrics Collection

This document describes the metrics collection system for MCSim, using the [metrics crate](https://docs.rs/metrics/). The design enables aggregation at multiple levels: whole network, node type groups, custom groups, and individual nodes.

## Table of Contents

1. [Design Principles](#design-principles)
2. [Label Hierarchy](#label-hierarchy)
3. [Metric Categories](#metric-categories)
   - [Radio/PHY Layer Metrics](#radiphy-layer-metrics)
   - [Packet/Network Layer Metrics](#packetnetwork-layer-metrics)
   - [Direct Message Layer Metrics](#direct-message-layer-metrics)
   - [Timing Metrics](#timing-metrics)
4. [Instrumentation Points](#instrumentation-points)
5. [Example Queries](#example-queries)
6. [Implementation Notes](#implementation-notes)
7. [Rerun.io Integration](#rerunio-integration)
8. [Next Steps](#next-steps)

---

## Design Principles

1. **Observe, Don't Instrument Firmware**: All metrics are collected by observing radio transmissions and simulation events. The MeshCore firmware DLLs remain untouched - we parse packets at the radio layer to classify them.

2. **Hierarchical Labels**: Every metric includes labels that allow filtering/aggregation at different levels:
   - `node` - Individual node name
   - `node_type` - Node type (Repeater, Companion, RoomServer)
   - `group` - Optional custom group name from YAML model

3. **Metric Types**: Use appropriate metric types:
   - `Counter` - Monotonically increasing counts (packets sent, bytes transmitted)
   - `Gauge` - Point-in-time values (active receptions, queue depth)
   - `Histogram` - Distributions (latency, packet size)

4. **Naming Convention**: Metrics follow the pattern `mcsim.<layer>.<noun>.<unit>` where applicable:
   - `mcsim.radio.airtime.microseconds`
   - `mcsim.packet.transmitted.count`
   - `mcsim.dm.delivery_latency_ms`

---

## Label Hierarchy

All metrics include the following labels for aggregation:

| Label | Description | Example Values |
|-------|-------------|----------------|
| `node` | Individual node name | `"Alice"`, `"Repeater1"` |
| `node_type` | Firmware type | `"Repeater"`, `"Companion"`, `"RoomServer"` |
| `group` | Custom group from YAML | `"backbone"`, `"clients"`, `"region_west"` |

### Example YAML Group Definition

```yaml
nodes:
  - name: "Repeater1"
    groups: ["backbone", "region_west"]
    # ...
  
  - name: "Repeater2"
    groups: ["backbone", "region_east"]
    # ...

  - name: "Alice"
    groups: ["clients", "region_west"]
    # ...
```

---

## Metric Categories

### Radio/PHY Layer Metrics

These metrics are captured in the `mcsim-lora` crate at the Radio entity level.

| Metric Name | Type | Unit | Labels | Description |
|-------------|------|------|--------|-------------|
| `mcsim.radio.tx_airtime_us` | Counter | µs | node, node_type, group | Total transmit airtime consumed |
| `mcsim.radio.rx_airtime_us` | Counter | µs | node, node_type, group | Total receive airtime consumed |
| `mcsim.radio.tx_packets` | Counter | count | node, node_type, group | Packets transmitted |
| `mcsim.radio.rx_packets` | Counter | count | node, node_type, group | Packets successfully received |
| `mcsim.radio.rx_collided` | Counter | count | node, node_type, group | Packets lost to collision |
| `mcsim.radio.rx_weak` | Counter | count | node, node_type, group | Packets lost due to low SNR |
| `mcsim.radio.turnaround_time_us` | Histogram | µs | node, node_type, group, direction | TX↔RX turnaround time |
| `mcsim.radio.active_receptions` | Gauge | count | node, node_type, group | Currently active receptions |
| `mcsim.radio.tx_packet_size_bytes` | Histogram | bytes | node, node_type, group | Distribution of transmitted packet sizes |
| `mcsim.radio.rx_packet_size_bytes` | Histogram | bytes | node, node_type, group | Distribution of received packet sizes |
| `mcsim.radio.rx_snr_db` | Histogram | dB | node, node_type, group | Distribution of received SNR values |
| `mcsim.radio.rx_rssi_dbm` | Histogram | dBm | node, node_type, group | Distribution of received RSSI values |

**Instrumentation Location**: `crates/mcsim-lora/src/lib.rs` - `Radio` entity

```rust
// In Radio::start_transmission()
counter!("mcsim.radio.tx_packets", &labels).increment(1);
counter!("mcsim.radio.tx_airtime_us", &labels).increment(airtime_us);
histogram!("mcsim.radio.tx_packet_size_bytes", &labels).record(packet.payload.len() as f64);

// In Radio::complete_reception()
if collided {
    counter!("mcsim.radio.rx_collided", &labels).increment(1);
} else if snr < threshold {
    counter!("mcsim.radio.rx_weak", &labels).increment(1);
} else {
    counter!("mcsim.radio.rx_packets", &labels).increment(1);
    histogram!("mcsim.radio.rx_snr_db", &labels).record(snr_db);
    histogram!("mcsim.radio.rx_rssi_dbm", &labels).record(rssi_dbm);
}
```

---

### Packet/Network Layer Metrics

These metrics are derived by parsing packets at the radio layer (in `mcsim-lora` and `mcsim-runner`). We decode MeshCore packets from the raw radio transmissions to classify them without instrumenting the firmware.

| Metric Name | Type | Unit | Labels | Description |
|-------------|------|------|--------|-------------|
| `mcsim.packet.tx_flood` | Counter | count | node, node_type, group, route_type, payload_hash | Flood packets transmitted |
| `mcsim.packet.tx_direct` | Counter | count | node, node_type, group, route_type, payload_hash | Direct packets transmitted |
| `mcsim.packet.rx_flood` | Counter | count | node, node_type, group, route_type, payload_hash | Flood packets received |
| `mcsim.packet.rx_direct` | Counter | count | node, node_type, group, route_type, payload_hash | Direct packets received |

**Additional Labels for Packet Metrics**:
| Label | Description | Example Values |
|-------|-------------|----------------|
| `route_type` | Routing mode (from packet header) | `"flood"`, `"direct"`, `"transport_flood"`, `"transport_direct"` |
| `payload_type` | MeshCore payload type (decoded from header) | `"advert"`, `"txt_msg"`, `"ack"`, `"trace"`, `"grp_txt"` |
| `payload_hash` | Unique hash identifying the packet payload (16 hex chars) | `"ACBE2BF9922B5221"` |

> **Note**: Use `RouteType::as_label()`, `PayloadType::as_label()`, and `PayloadHash::as_label()` from the `meshcore-packet` crate to get consistent label values.

**Instrumentation Location**: `crates/mcsim-runner/src/main.rs` - parsing packets from `TransmitAir` and `RadioRxPacket` events

```rust
// In EventLoop::update_stats() - parse packets from radio events
EventPayload::TransmitAir(tx) => {
    // Decode the MeshCore packet from raw radio payload
    if let Ok(packet) = MeshCorePacket::decode(&tx.packet.payload) {
        let labels = get_node_labels(tx.radio_id)
            .with("route_type", packet.route_type().as_label().to_string())
            .with("payload_type", packet.payload_type().as_label().to_string())
            .with("payload_hash", packet.payload_hash_label().as_label());
        
        if packet.is_flood() {
            counter!("mcsim.packet.tx_flood", &labels).increment(1);
        } else {
            counter!("mcsim.packet.tx_direct", &labels).increment(1);
        }
    }
}
```

**Note**: Metrics like `dropped_dup`, `dropped_ttl`, `queue_depth`, and `forwarded` would require firmware instrumentation and are not collected. We can infer forwarding behavior by observing the same packet hash transmitted by different nodes.

---

### Direct Message Layer Metrics

These metrics track end-to-end direct message delivery, captured in `mcsim-agents` and `mcsim-runner`.

| Metric Name | Type | Unit | Labels | Description |
|-------------|------|------|--------|-------------|
| `mcsim.dm.sent` | Counter | count | node, node_type, group, dest_type | Direct messages sent |
| `mcsim.dm.delivered` | Counter | count | node, node_type, group, dest_type | Direct messages successfully delivered |
| `mcsim.dm.failed` | Counter | count | node, node_type, group, dest_type, reason | Direct message delivery failures |
| `mcsim.dm.acked` | Counter | count | node, node_type, group | Direct messages acknowledged |
| `mcsim.dm.delivery_latency_ms` | Histogram | ms | node, node_type, group, dest_type | Time from send to delivery |
| `mcsim.dm.ack_latency_ms` | Histogram | ms | node, node_type, group | Time from send to ACK received |
| `mcsim.dm.hop_count` | Histogram | hops | node, node_type, group | Distribution of hop counts |

**Additional Labels**:
| Label | Description | Example Values |
|-------|-------------|----------------|
| `dest_type` | Destination type | `"dm"`, `"channel"`, `"broadcast"` |
| `reason` | Failure reason | `"timeout"`, `"no_route"`, `"rejected"` |

**Instrumentation Location**: `crates/mcsim-runner/src/main.rs` and `crates/mcsim-agents/src/lib.rs`

```rust
// In EventLoop::update_stats() for message events
EventPayload::MessageSend(msg) => {
    counter!("mcsim.dm.sent", &labels).increment(1);
    // Store send time for latency calculation
    self.pending_messages.insert(msg.id, ctx.time());
}

EventPayload::MessageAcknowledged(ack) => {
    counter!("mcsim.dm.acked", &labels).increment(1);
    if let Some(send_time) = self.pending_messages.remove(&ack.original_hash) {
        let latency = ctx.time() - send_time;
        histogram!("mcsim.dm.ack_latency_ms", &labels).record(latency.as_millis() as f64);
    }
}
```

---

### Packet Delivery Tracking

Both flood and direct messages require tracking packet hashes across the network to determine delivery success. This is implemented via a `PacketTracker` in `mcsim-runner`.

#### Flood Propagation Metrics

| Metric Name | Type | Unit | Labels | Description |
|-------------|------|------|--------|-------------|
| `mcsim.flood.nodes_reached` | Histogram | count | origin_node, group | Nodes that received a flood message |
| `mcsim.flood.times_heard` | Histogram | count | node, node_type, group | Times a flood packet was heard (incl. duplicates) |
| `mcsim.flood.propagation_time_us` | Histogram | ms | origin_node, group | Time to reach furthest node |
| `mcsim.flood.coverage` | Gauge | ratio | origin_node, group | Fraction of reachable nodes covered |

#### Path Message Delivery Metrics

| Metric Name | Type | Unit | Labels | Description |
|-------------|------|------|--------|-------------|
| `mcsim.path.sent` | Counter | count | origin_node, dest_node, group | Path messages sent |
| `mcsim.path.delivered` | Counter | count | origin_node, dest_node, group | Path messages received at destination |
| `mcsim.path.failed` | Counter | count | origin_node, dest_node, group, reason | Path messages that failed delivery |
| `mcsim.path.delivery_latency_ms` | Histogram | ms | origin_node, dest_node, group | Time from send to delivery |
| `mcsim.path.hops` | Histogram | count | origin_node, dest_node, group | Hop count for delivered messages |

**Additional Labels**:
| Label | Description | Example Values |
|-------|-------------|----------------|
| `reason` | Failure reason | `"timeout"`, `"collision"`, `"no_route"` |

**Instrumentation**: Requires tracking packet hashes across the network for both flood and direct packets:

```rust
/// Tracks packet propagation across the network.
struct PacketTracker {
    /// Map from packet_hash -> PacketStats
    packets: HashMap<[u8; 6], PacketStats>,
    /// Timeout after which undelivered direct messages are marked failed
    direct_timeout: SimTime,
}

#[derive(Debug)]
enum PacketKind {
    Flood { 
        nodes_reached: HashSet<EntityId>,
        reception_times: Vec<(EntityId, SimTime)>,
    },
    Direct {
        destination: EntityId,
        delivered: bool,
        delivery_time: Option<SimTime>,
        hop_count: Option<u8>,
    },
}

struct PacketStats {
    origin_node: EntityId,
    origin_time: SimTime,
    kind: PacketKind,
}

impl PacketTracker {
    /// Called when a packet is transmitted (from TransmitAir event)
    fn track_send(&mut self, packet_hash: [u8; 6], origin: EntityId, time: SimTime, 
                  is_flood: bool, destination: Option<EntityId>) {
        let kind = if is_flood {
            PacketKind::Flood {
                nodes_reached: HashSet::new(),
                reception_times: Vec::new(),
            }
        } else {
            PacketKind::Direct {
                destination: destination.expect("direct packet must have destination"),
                delivered: false,
                delivery_time: None,
                hop_count: None,
            }
        };
        
        self.packets.insert(packet_hash, PacketStats {
            origin_node: origin,
            origin_time: time,
            kind,
        });
    }
    
    /// Called when a packet is received (from RadioRxPacket event)
    fn track_reception(&mut self, packet_hash: [u8; 6], receiver: EntityId, 
                       time: SimTime, hop_count: u8) {
        if let Some(stats) = self.packets.get_mut(&packet_hash) {
            match &mut stats.kind {
                PacketKind::Flood { nodes_reached, reception_times } => {
                    nodes_reached.insert(receiver);
                    reception_times.push((receiver, time));
                }
                PacketKind::Direct { destination, delivered, delivery_time, hop_count: hops } => {
                    if receiver == *destination && !*delivered {
                        *delivered = true;
                        *delivery_time = Some(time);
                        *hops = Some(hop_count);
                    }
                }
            }
        }
    }
    
    /// Called at simulation end to emit summary metrics
    fn emit_summary_metrics(&self, node_labels: &HashMap<EntityId, MetricLabels>) {
        for (hash, stats) in &self.packets {
            let origin_labels = node_labels.get(&stats.origin_node);
            
            match &stats.kind {
                PacketKind::Flood { nodes_reached, reception_times } => {
                    histogram!("mcsim.flood.nodes_reached", ...).record(nodes_reached.len() as f64);
                    
                    // Calculate propagation time to furthest node
                    if let Some((_, max_time)) = reception_times.iter().max_by_key(|(_, t)| t) {
                        let propagation = *max_time - stats.origin_time;
                        histogram!("mcsim.flood.propagation_time_us", ...).record(propagation.as_millis() as f64);
                    }
                }
                PacketKind::Direct { destination, delivered, delivery_time, hop_count } => {
                    if *delivered {
                        counter!("mcsim.path.delivered", ...).increment(1);
                        if let Some(dt) = delivery_time {
                            let latency = *dt - stats.origin_time;
                            histogram!("mcsim.path.delivery_latency_ms", ...).record(latency.as_millis() as f64);
                        }
                        if let Some(hops) = hop_count {
                            histogram!("mcsim.path.hops", ...).record(*hops as f64);
                        }
                    } else {
                        counter!("mcsim.path.failed", &[("reason", "timeout")]).increment(1);
                    }
                }
            }
        }
    }
}

---

### Timing Metrics

| Metric Name | Type | Unit | Labels | Description |
|-------------|------|------|--------|-------------|
| `mcsim.timing.tx_delay_us` | Histogram | ms | node, node_type, group | Delay before transmission (jitter/backoff) |
| `mcsim.timing.rx_process_delay_us` | Histogram | µs | node, node_type, group | Time to process received packet |
| `mcsim.timing.queue_wait_us` | Histogram | ms | node, node_type, group | Time packet waits in TX queue |

---

## Instrumentation Points

### Summary Table

| Crate | File | Instrumentation Point |
|-------|------|----------------------|
| `mcsim-lora` | `src/lib.rs` | `Radio::start_transmission()` - TX metrics |
| `mcsim-lora` | `src/lib.rs` | `Radio::handle_receive_air()` - RX start |
| `mcsim-lora` | `src/lib.rs` | `Radio::complete_reception()` - RX complete, collision |
| `mcsim-runner` | `src/main.rs` | `EventLoop::update_stats()` - Packet parsing & classification |
| `mcsim-runner` | `src/main.rs` | `PacketTracker` - Delivery tracking across network |
| `mcsim-agents` | `src/lib.rs` | Message send/receive events |

### Detailed Instrumentation

#### 1. Radio Entity (`mcsim-lora/src/lib.rs`)

**`Radio::start_transmission()`** (~line 520)
```rust
fn start_transmission(&mut self, packet: LoraPacket, ctx: &mut SimContext) {
    let airtime = calculate_time_on_air(&self.config.params, packet.payload.len());
    
    // METRICS: Record transmission
    let labels = self.metric_labels();
    counter!("mcsim.radio.tx_packets", &labels).increment(1);
    counter!("mcsim.radio.tx_airtime_us", &labels).increment(airtime.as_micros());
    histogram!("mcsim.radio.tx_packet_size_bytes", &labels).record(packet.payload.len() as f64);
    
    // ... existing code ...
}
```

**`Radio::handle_receive_air()`** (~line 545)
```rust
fn handle_receive_air(&mut self, rx_event: &ReceiveAirEvent, ctx: &mut SimContext) {
    // METRICS: Track active receptions
    let labels = self.metric_labels();
    gauge!("mcsim.radio.active_receptions", &labels).increment(1.0);
    
    // ... existing collision detection ...
}
```

**`Radio::complete_reception()`** (in timer handling, ~line 590)
```rust
fn complete_reception(&mut self, reception: ActiveReception, ctx: &mut SimContext) {
    let labels = self.metric_labels();
    gauge!("mcsim.radio.active_receptions", &labels).decrement(1.0);
    
    if reception.collided {
        counter!("mcsim.radio.rx_collided", &labels).increment(1);
    } else {
        // Check SNR threshold
        let threshold = calculate_snr_sensitivity(self.config.params.spreading_factor);
        if reception.snr_db < threshold {
            counter!("mcsim.radio.rx_weak", &labels).increment(1);
        } else {
            counter!("mcsim.radio.rx_packets", &labels).increment(1);
            histogram!("mcsim.radio.rx_snr_db", &labels).record(reception.snr_db);
            histogram!("mcsim.radio.rx_rssi_dbm", &labels).record(reception.rssi_dbm);
            histogram!("mcsim.radio.rx_packet_size_bytes", &labels).record(reception.packet.payload.len() as f64);
            
            let airtime = calculate_time_on_air(&self.config.params, reception.packet.payload.len());
            counter!("mcsim.radio.rx_airtime_us", &labels).increment(airtime.as_micros());
        }
    }
}
```

#### 2. Event Loop (`mcsim-runner/src/main.rs`)

**Add PacketTracker to EventLoop**
```rust
pub struct EventLoop {
    // ... existing fields ...
    packet_tracker: PacketTracker,
}

impl EventLoop {
    fn update_stats(&mut self, event: &Event) {
        match &event.payload {
            EventPayload::TransmitAir(tx) => {
                // Track packet send for delivery tracking
                if let Ok(packet) = MeshCorePacket::decode(&tx.packet.payload) {
                    let hash = packet.packet_hash();
                    let is_flood = packet.is_flood();
                    let destination = if is_flood { 
                        None 
                    } else { 
                        packet.destination_entity_id() // Map dest pubkey -> EntityId
                    };
                    self.packet_tracker.track_send(
                        hash,
                        tx.radio_id,
                        event.time,
                        is_flood,
                        destination,
                    );
                }
                // ... existing stats ...
            }
            EventPayload::RadioRxPacket(rx) => {
                // Track packet reception for delivery tracking
                if let Ok(packet) = MeshCorePacket::decode(&rx.packet.payload) {
                    let hash = packet.packet_hash();
                    let hop_count = packet.hop_count();
                    for target in &event.targets {
                        self.packet_tracker.track_reception(
                            hash,
                            *target,
                            event.time,
                            hop_count,
                        );
                    }
                }
                // ... existing stats ...
            }
            // ... other cases ...
        }
    }
    
    fn finalize_stats(&mut self) {
        // Emit delivery summary metrics
        self.packet_tracker.emit_summary_metrics(&self.node_labels);
    }
}
```

---

## Metric Specification Syntax

When using the `--metric` CLI flag, you can specify which metrics to export and how to break them down using a simple specification syntax:

```text
<metric-pattern>[/<breakdown1>[/<breakdown2>...]]
<metric-pattern>/*
```

### Basic Patterns

- `mcsim.radio.rx_packets` - Export a specific metric (totals only)
- `mcsim.radio.*` - Export all metrics matching the glob pattern
- `mcsim.*` - Export all metrics

### Breakdown Labels

Add `/label` after the pattern to break down metrics by that label:

- `mcsim.radio.rx_packets/node` - Breakdown by individual node
- `mcsim.radio.*/node_type` - All radio metrics broken down by node type
- `mcsim.packet.tx_flood/packet_type/node` - Nested breakdown: by packet type, then by node

**Available breakdown labels:**
| Label | Aliases | Description |
|-------|---------|-------------|
| `node` | | Individual node name |
| `node_type` | `type`, `nodetype` | Node type (Repeater, Companion, etc.) |
| `group` | `groups` | Custom group from YAML model |
| `route_type` | `route`, `routetype` | Routing mode (flood, direct, etc.) |
| `payload_type` | `payloadtype` | Payload type |
| `payload_hash` | `hash`, `payloadhash` | Unique packet identifier |

### Wildcard Breakdown

Use `/*` to automatically break down by **all labels present** in the metric data:

```bash
# Break down rx_packets by all available labels
--metric "mcsim.radio.rx_packets/*"

# All radio metrics, each broken down by all their labels
--metric "mcsim.radio.*/*"
```

This is useful when you want to see all available dimensions without knowing them in advance.

### CLI Examples

```bash
# Export all metrics with totals only
cargo run -- run model.yaml --metrics-output json

# Export radio metrics broken down by node
cargo run -- run model.yaml --metrics-output json --metric "mcsim.radio.*/node"

# Export packet metrics with nested breakdowns
cargo run -- run model.yaml --metrics-output json \
    --metric "mcsim.packet.tx_flood/payload_type/node" \
    --metric "mcsim.radio.rx_packets/node_type"

# Export with automatic breakdown by all labels
cargo run -- run model.yaml --metrics-output json --metric "mcsim.radio.tx_packets/*"
```

---

## Example Queries

With metrics exported to a time-series database or using the `metrics-exporter-prometheus`, you can query:

### Total Network Airtime
```promql
sum(mcsim_radio_tx_airtime_total)
```

### Airtime by Node Type
```promql
sum by (node_type) (mcsim_radio_tx_airtime_total)
```

### Per-Node Packet Success Rate
```promql
mcsim_radio_rx_packets_total / (mcsim_radio_rx_packets_total + mcsim_radio_rx_collided_total + mcsim_radio_rx_weak_total)
```

### Repeater Forwarding Efficiency
```promql
sum(mcsim_packet_forwarded_total{node_type="Repeater"}) / sum(mcsim_packet_rx_flood_total{node_type="Repeater"})
```

### Message Delivery Rate
```promql
sum(mcsim_message_delivered_total) / sum(mcsim_message_sent_total)
```

### P95 Message Latency
```promql
histogram_quantile(0.95, sum(rate(mcsim_message_delivery_latency_bucket[5m])) by (le))
```

### Flood Coverage (Average Nodes Reached)
```promql
histogram_avg(mcsim_flood_nodes_reached)
```

---

## Implementation Notes

### 1. Creating the `mcsim-metrics` Crate

The empty `crates/mcsim-metrics` directory should contain:

```
mcsim-metrics/
├── Cargo.toml
└── src/
    └── lib.rs
```

**`Cargo.toml`**:
```toml
[package]
name = "mcsim-metrics"
version = "0.1.0"
edition = "2021"

[dependencies]
metrics = "0.23"
metrics-exporter-prometheus = { version = "0.15", optional = true }

[features]
default = []
prometheus = ["metrics-exporter-prometheus"]
```

**`src/lib.rs`**:
```rust
//! Metrics collection for MCSim.

use metrics::{counter, gauge, histogram, describe_counter, describe_gauge, describe_histogram, Label};

/// Standard labels for all metrics.
#[derive(Clone)]
pub struct MetricLabels {
    pub node: String,
    pub node_type: String,
    pub groups: Vec<String>,
}

impl MetricLabels {
    /// Convert to metrics crate labels.
    pub fn to_labels(&self) -> Vec<Label> {
        let mut labels = vec![
            Label::new("node", self.node.clone()),
            Label::new("node_type", self.node_type.clone()),
        ];
        // Add first group as primary (for PromQL simplicity)
        if let Some(group) = self.groups.first() {
            labels.push(Label::new("group", group.clone()));
        }
        labels
    }
    
    /// Create labels with an additional key-value pair.
    pub fn with(&self, key: &str, value: String) -> Vec<Label> {
        let mut labels = self.to_labels();
        labels.push(Label::new(key, value));
        labels
    }
}

/// Initialize metric descriptions (call once at startup).
pub fn describe_metrics() {
    // Radio layer
    describe_counter!("mcsim.radio.tx_packets", "Total packets transmitted");
    describe_counter!("mcsim.radio.rx_packets", "Total packets successfully received");
    describe_counter!("mcsim.radio.rx_collided", "Packets lost to collision");
    describe_counter!("mcsim.radio.rx_weak", "Packets lost due to low SNR");
    describe_counter!("mcsim.radio.tx_airtime_us", "Total transmit airtime in microseconds");
    describe_counter!("mcsim.radio.rx_airtime_us", "Total receive airtime in microseconds");
    describe_histogram!("mcsim.radio.tx_packet_size_bytes", "Distribution of transmitted packet sizes");
    describe_histogram!("mcsim.radio.rx_snr_db", "Distribution of received SNR values");
    describe_gauge!("mcsim.radio.active_receptions", "Currently active receptions");
    
    // Packet layer
    describe_counter!("mcsim.packet.tx_flood", "Flood packets originated");
    describe_counter!("mcsim.packet.tx_direct", "Direct packets originated");
    describe_counter!("mcsim.packet.forwarded", "Packets forwarded by repeater");
    describe_counter!("mcsim.packet.dropped_dup", "Packets dropped as duplicates");
    describe_gauge!("mcsim.packet.queue_depth", "Current TX queue depth");
    
    // Message layer
    describe_counter!("mcsim.dm.sent", "Direct messages sent");
    describe_counter!("mcsim.dm.delivered", "Direct messages delivered");
    describe_counter!("mcsim.dm.acked", "Direct messages acknowledged");
    describe_histogram!("mcsim.dm.delivery_latency_ms", "Direct message delivery latency in milliseconds");
    describe_histogram!("mcsim.dm.ack_latency_ms", "Direct message ACK latency in milliseconds");
    
    // Flood propagation
    describe_histogram!("mcsim.flood.nodes_reached", "Number of nodes that received a flood message");
    describe_histogram!("mcsim.flood.times_heard", "Times a flood packet was heard at a node");
}
```

### 2. Adding Labels to Entities

Each entity that emits metrics needs a way to get its labels:

```rust
// In Radio entity
impl Radio {
    fn metric_labels(&self) -> MetricLabels {
        // This requires storing node metadata in the Radio struct
        MetricLabels {
            node: self.node_name.clone(),
            node_type: self.node_type.clone(),
            groups: self.groups.clone(),
            network: self.network_id.clone(),
        }
    }
}
```

### 3. Model YAML Extension

Add `groups` field to node definitions:

```yaml
nodes:
  - name: "Repeater1"
    groups: ["backbone", "region_west"]
    # ...
```

Update `crates/mcsim-model/src/lib.rs` to parse groups and pass them through to entity creation.

### 4. Metrics Export Options

For simulation runs, consider:

1. **In-memory accumulator**: Collect all metrics, dump JSON at end
2. **Prometheus**: Real-time scraping for long-running or realtime sims
3. **CSV export**: For spreadsheet analysis

The `metrics` crate allows swapping exporters without changing instrumentation code.

---

## Rerun.io Integration

All metrics should be visible through the [Rerun](https://rerun.io/) visualization integration. This provides real-time, interactive visualization of metrics alongside the spatial simulation view.

### Architecture

Metrics are logged to Rerun as time-series data using the Rerun SDK. The `mcsim-runner` crate maintains a `RerunRecorder` that:

1. Logs metrics as scalar time-series attached to entity paths
2. Uses Rerun's timeline system to synchronize metrics with simulation time
3. Groups metrics hierarchically for organized viewing in the Rerun UI

### Entity Path Hierarchy

Metrics are attached to Rerun entity paths that mirror the simulation structure:

```
/mcsim
├── /network
│   ├── total_airtime          # Aggregate network metrics
│   ├── total_packets
│   └── delivery_rate
├── /nodes
│   ├── /{node_name}
│   │   ├── /radio
│   │   │   ├── tx_packets      # Per-node radio metrics
│   │   │   ├── rx_packets
│   │   │   ├── tx_airtime
│   │   │   ├── rx_collided
│   │   │   └── snr             # Histogram as scalar series
│   │   ├── /packets
│   │   │   ├── tx_flood
│   │   │   ├── tx_direct
│   │   │   └── forwarded
│   │   └── /messages
│   │       ├── sent
│   │       ├── delivered
│   │       └── latency
├── /groups
│   ├── /{group_name}
│   │   ├── tx_airtime          # Aggregated group metrics
│   │   ├── rx_packets
│   │   └── delivery_rate
└── /node_types
    ├── /Repeater
    │   └── ...                 # Aggregated by node type
    ├── /Companion
    └── /RoomServer
```

### Logging Metrics to Rerun

When a metric is recorded, it is simultaneously logged to Rerun:

```rust
use rerun::{RecordingStream, Scalar, SeriesLine};

pub struct RerunMetricsLogger {
    rec: RecordingStream,
}

impl RerunMetricsLogger {
    /// Log a counter increment
    pub fn log_counter(&self, metric: &str, labels: &MetricLabels, value: u64, time: SimTime) {
        let entity_path = self.build_entity_path(metric, labels);
        
        self.rec.set_time_nanos("sim_time", time.as_nanos() as i64);
        self.rec.log(
            &entity_path,
            &Scalar::new(value as f64),
        ).ok();
    }
    
    /// Log a gauge value
    pub fn log_gauge(&self, metric: &str, labels: &MetricLabels, value: f64, time: SimTime) {
        let entity_path = self.build_entity_path(metric, labels);
        
        self.rec.set_time_nanos("sim_time", time.as_nanos() as i64);
        self.rec.log(
            &entity_path,
            &Scalar::new(value),
        ).ok();
    }
    
    /// Log a histogram observation (as individual points)
    pub fn log_histogram(&self, metric: &str, labels: &MetricLabels, value: f64, time: SimTime) {
        let entity_path = self.build_entity_path(metric, labels);
        
        self.rec.set_time_nanos("sim_time", time.as_nanos() as i64);
        self.rec.log(
            &entity_path,
            &Scalar::new(value),
        ).ok();
    }
    
    fn build_entity_path(&self, metric: &str, labels: &MetricLabels) -> String {
        // Convert metric name to entity path
        // e.g., "mcsim.radio.tx_packets" with node="Alice" 
        //       -> "/mcsim/nodes/Alice/radio/tx_packets"
        let parts: Vec<&str> = metric.split('.').collect();
        if parts.len() >= 3 {
            let layer = parts[1];  // "radio", "packet", "message", etc.
            let name = parts[2..].join("/");
            format!("/mcsim/nodes/{}/{}/{}", labels.node, layer, name)
        } else {
            format!("/mcsim/metrics/{}", metric.replace('.', "/"))
        }
    }
}
```

### Integration with metrics Crate

To ensure all metrics recorded via the `metrics` crate are also sent to Rerun, we implement a custom `Recorder`:

```rust
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Recorder, SharedString, Unit};
use std::sync::Arc;

pub struct DualRecorder {
    /// The primary recorder (e.g., in-memory or Prometheus)
    primary: Arc<dyn Recorder>,
    /// The Rerun logger for visualization
    rerun_logger: Arc<RerunMetricsLogger>,
    /// Current simulation time (updated by runner)
    current_time: Arc<AtomicU64>,
}

impl Recorder for DualRecorder {
    fn register_counter(&self, key: &Key, metadata: &metrics::Metadata<'_>) -> Counter {
        self.primary.register_counter(key, metadata)
    }
    
    fn register_gauge(&self, key: &Key, metadata: &metrics::Metadata<'_>) -> Gauge {
        self.primary.register_gauge(key, metadata)
    }
    
    fn register_histogram(&self, key: &Key, metadata: &metrics::Metadata<'_>) -> Histogram {
        self.primary.register_histogram(key, metadata)
    }
    
    // ... implement methods that also log to Rerun ...
}
```

### Rerun Blueprint Configuration

For consistent visualization, we configure a Rerun blueprint that organizes the metric panels:

```rust
fn create_metrics_blueprint(rec: &RecordingStream) {
    use rerun::blueprint::*;
    
    let blueprint = Blueprint::new()
        .with_panel(
            TimeSeriesView::new("Network Overview")
                .with_entity("/mcsim/network/**")
        )
        .with_panel(
            TimeSeriesView::new("Per-Node Radio Metrics")
                .with_entity("/mcsim/nodes/*/radio/**")
        )
        .with_panel(
            TimeSeriesView::new("Message Delivery")
                .with_entity("/mcsim/nodes/*/messages/**")
        )
        .with_panel(
            Spatial3DView::new("Network Topology")
                .with_entity("/mcsim/spatial/**")
        );
    
    rec.send_blueprint(blueprint).ok();
}
```

### Aggregated Metrics

For group and node-type aggregations, the `RerunMetricsLogger` maintains running totals that are periodically flushed:

```rust
impl RerunMetricsLogger {
    /// Called periodically (e.g., every 100ms sim time) to log aggregates
    pub fn flush_aggregates(&self, time: SimTime) {
        // Aggregate by group
        for (group, metrics) in &self.group_aggregates {
            let path = format!("/mcsim/groups/{}/tx_airtime", group);
            self.rec.set_time_nanos("sim_time", time.as_nanos() as i64);
            self.rec.log(&path, &Scalar::new(metrics.tx_airtime as f64)).ok();
        }
        
        // Aggregate by node type
        for (node_type, metrics) in &self.type_aggregates {
            let path = format!("/mcsim/node_types/{}/tx_packets", node_type);
            self.rec.set_time_nanos("sim_time", time.as_nanos() as i64);
            self.rec.log(&path, &Scalar::new(metrics.tx_packets as f64)).ok();
        }
    }
}
```

### CLI Integration

Enable Rerun metrics logging via CLI flag:

```bash
# Log metrics to Rerun viewer (spawns viewer)
cargo run -- --model examples/simple_network.yaml --rerun

# Log metrics to Rerun file for later viewing
cargo run -- --model examples/simple_network.yaml --rerun-save metrics.rrd

# Connect to existing Rerun viewer
cargo run -- --model examples/simple_network.yaml --rerun-connect 127.0.0.1:9876
```

### Viewing Metrics in Rerun

Once the simulation runs with Rerun enabled:

1. **Time Series Panel**: Shows counters and gauges as line plots over simulation time
2. **Selection Panel**: Click on a node in the 3D view to filter metrics to that node
3. **Timeline Scrubber**: Scrub through simulation time to see metric values at any point
4. **Entity Tree**: Navigate the `/mcsim/nodes/...` hierarchy to find specific metrics
5. **Comparison**: Select multiple nodes to overlay their metrics in the same plot

### Spatial Correlation

Metrics can be correlated with spatial events:

- Radio transmission metrics appear as the node transmits (visible as expanding circles in 3D view)
- Collision metrics spike when overlapping transmissions occur
- Message delivery latency can be traced visually along the path through the network

This tight integration between spatial visualization and metrics provides deep insight into network behavior that neither view alone can offer.

---

## Next Steps

1. Create `mcsim-metrics` crate with label helpers
2. Add `groups` field to YAML model schema
3. Instrument `Radio` entity in `mcsim-lora`
4. Add `PacketTracker` to `mcsim-runner` with packet decoding
5. Add metrics export CLI flag (`--metrics-output json|prometheus`)
6. Implement `RerunMetricsLogger` for Rerun integration
7. Add Rerun blueprint configuration for metrics visualization

