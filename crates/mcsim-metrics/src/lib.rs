//! Metrics infrastructure for MeshCore simulator.
//!
//! This crate provides metric label helpers and describes all metrics used in the simulator.
//! It re-exports the `metrics` crate for convenience and defines all metrics as structured
//! [`Metric`] constants to avoid typos and provide rich metadata.
//!
//! # Example
//!
//! ```rust,ignore
//! use mcsim_metrics::{MetricLabels, metric_defs, describe_metrics};
//!
//! // Initialize metrics descriptions at startup
//! describe_metrics();
//!
//! // Create labels for a node
//! let labels = MetricLabels::new("node_001", "repeater", "test_network")
//!     .with_groups(vec!["group_a".to_string()]);
//!
//! // Use labels with metrics
//! metrics::counter!(metric_defs::RADIO_TX_PACKETS.name, &labels.to_labels()).increment(1);
//! ```
//!
//! # Metric Type
//!
//! The [`Metric`] type provides a structured way to declare metrics with their metadata:
//!
//! ```rust
//! use mcsim_metrics::{Metric, MetricKind};
//! use metrics::Unit;
//!
//! const MY_COUNTER: Metric = Metric::counter("my.counter")
//!     .with_description("A counter metric")
//!     .with_unit(Unit::Count)
//!     .with_labels(&["node", "direction"]);
//!
//! // Register the metric description
//! MY_COUNTER.describe();
//!
//! // Use with metrics crate
//! metrics::counter!(MY_COUNTER.name).increment(1);
//! ```

pub use metrics;

use metrics::{describe_counter, describe_gauge, describe_histogram, Unit};

/// The kind of metric (counter, gauge, or histogram).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricKind {
    /// A monotonically increasing counter.
    Counter,
    /// A gauge that can go up and down.
    Gauge,
    /// A histogram for recording distributions.
    Histogram,
}

impl MetricKind {
    /// Returns the kind as a lowercase string.
    pub const fn as_str(&self) -> &'static str {
        match self {
            MetricKind::Counter => "counter",
            MetricKind::Gauge => "gauge",
            MetricKind::Histogram => "histogram",
        }
    }
}

impl std::fmt::Display for MetricKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A metric declaration with its metadata.
///
/// This type allows declaring metrics with their name, description, unit, and expected labels
/// in a structured way. Use the const constructors to create metrics at compile time.
///
/// # Example
///
/// ```rust
/// use mcsim_metrics::{Metric, MetricKind};
/// use metrics::Unit;
///
/// const PACKETS_SENT: Metric = Metric::counter("mcsim.radio.packets_sent")
///     .with_description("Total packets sent")
///     .with_unit(Unit::Count)
///     .with_labels(&["node", "packet_type"]);
///
/// // Get the metric name for use with the metrics crate
/// assert_eq!(PACKETS_SENT.name, "mcsim.radio.packets_sent");
/// assert_eq!(PACKETS_SENT.kind, MetricKind::Counter);
/// ```
#[derive(Debug, Clone)]
pub struct Metric {
    /// The metric name (e.g., "mcsim.radio.tx_packets").
    pub name: &'static str,
    /// The kind of metric (counter, gauge, histogram).
    pub kind: MetricKind,
    /// Human-readable description of the metric.
    pub description: &'static str,
    /// The unit of measurement (optional).
    pub unit: Option<Unit>,
    /// Expected label keys for this metric.
    pub labels: &'static [&'static str],
}

impl Metric {
    /// Creates a new counter metric with the given name.
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcsim_metrics::Metric;
    ///
    /// const MY_COUNTER: Metric = Metric::counter("my.counter");
    /// ```
    pub const fn counter(name: &'static str) -> Self {
        Self {
            name,
            kind: MetricKind::Counter,
            description: "",
            unit: None,
            labels: &[],
        }
    }

    /// Creates a new gauge metric with the given name.
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcsim_metrics::Metric;
    ///
    /// const MY_GAUGE: Metric = Metric::gauge("my.gauge");
    /// ```
    pub const fn gauge(name: &'static str) -> Self {
        Self {
            name,
            kind: MetricKind::Gauge,
            description: "",
            unit: None,
            labels: &[],
        }
    }

    /// Creates a new histogram metric with the given name.
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcsim_metrics::Metric;
    ///
    /// const MY_HISTOGRAM: Metric = Metric::histogram("my.histogram");
    /// ```
    pub const fn histogram(name: &'static str) -> Self {
        Self {
            name,
            kind: MetricKind::Histogram,
            description: "",
            unit: None,
            labels: &[],
        }
    }

    /// Sets the description for the metric.
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcsim_metrics::Metric;
    ///
    /// const MY_COUNTER: Metric = Metric::counter("my.counter")
    ///     .with_description("Counts important things");
    /// ```
    pub const fn with_description(mut self, description: &'static str) -> Self {
        self.description = description;
        self
    }

    /// Sets the unit for the metric.
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcsim_metrics::Metric;
    /// use metrics::Unit;
    ///
    /// const MY_COUNTER: Metric = Metric::counter("my.counter")
    ///     .with_unit(Unit::Bytes);
    /// ```
    pub const fn with_unit(mut self, unit: Unit) -> Self {
        self.unit = Some(unit);
        self
    }

    /// Sets the expected label keys for the metric.
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcsim_metrics::Metric;
    ///
    /// const MY_COUNTER: Metric = Metric::counter("my.counter")
    ///     .with_labels(&["node", "direction"]);
    /// ```
    pub const fn with_labels(mut self, labels: &'static [&'static str]) -> Self {
        self.labels = labels;
        self
    }

    /// Registers this metric's description with the metrics recorder.
    ///
    /// This should be called once at startup for each metric.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use mcsim_metrics::Metric;
    ///
    /// const MY_COUNTER: Metric = Metric::counter("my.counter")
    ///     .with_description("Counts things");
    ///
    /// MY_COUNTER.describe();
    /// ```
    pub fn describe(&self) {
        match (self.kind, self.unit) {
            (MetricKind::Counter, Some(unit)) => {
                describe_counter!(self.name, unit, self.description);
            }
            (MetricKind::Counter, None) => {
                describe_counter!(self.name, self.description);
            }
            (MetricKind::Gauge, Some(unit)) => {
                describe_gauge!(self.name, unit, self.description);
            }
            (MetricKind::Gauge, None) => {
                describe_gauge!(self.name, self.description);
            }
            (MetricKind::Histogram, Some(unit)) => {
                describe_histogram!(self.name, unit, self.description);
            }
            (MetricKind::Histogram, None) => {
                describe_histogram!(self.name, self.description);
            }
        }
    }

    /// Returns the unit as a human-readable string.
    pub fn unit_str(&self) -> &'static str {
        match self.unit {
            Some(Unit::Count) => "count",
            Some(Unit::Percent) => "percent",
            Some(Unit::Seconds) => "seconds",
            Some(Unit::Milliseconds) => "milliseconds",
            Some(Unit::Microseconds) => "microseconds",
            Some(Unit::Nanoseconds) => "nanoseconds",
            Some(Unit::Tebibytes) => "tebibytes",
            Some(Unit::Gibibytes) => "gibibytes",
            Some(Unit::Mebibytes) => "mebibytes",
            Some(Unit::Kibibytes) => "kibibytes",
            Some(Unit::Bytes) => "bytes",
            Some(Unit::TerabitsPerSecond) => "terabits/second",
            Some(Unit::GigabitsPerSecond) => "gigabits/second",
            Some(Unit::MegabitsPerSecond) => "megabits/second",
            Some(Unit::KilobitsPerSecond) => "kilobits/second",
            Some(Unit::BitsPerSecond) => "bits/second",
            Some(Unit::CountPerSecond) => "count/second",
            None => "",
        }
    }
}

/// All metric definitions for the simulator.
///
/// Each metric is defined as a const [`Metric`] with its name, kind, description,
/// unit, and expected labels.
pub mod metric_defs {
    use super::{Metric, Unit};

    // ========================================================================
    // Standard Label Keys
    // ========================================================================
    
    /// Standard labels present on all node-scoped metrics.
    pub const STANDARD_LABELS: &[&str] = &["node", "node_type"];
    
    /// Labels for packet-type breakdown (added to radio metrics).
    pub const PACKET_TYPE_LABELS: &[&str] = &["payload_type", "route_type", "payload_hash"];

    // ========================================================================
    // Radio/PHY Layer Metrics
    // ========================================================================

    /// Total transmit airtime in microseconds.
    /// 
    /// Labels: node, node_type, payload_type, route_type, payload_hash
    pub const RADIO_TX_AIRTIME: Metric = Metric::counter("mcsim.radio.tx_airtime_us")
        .with_description("Total transmit airtime in microseconds")
        .with_unit(Unit::Microseconds)
        .with_labels(&["node", "node_type", "payload_type", "route_type", "payload_hash"]);

    /// Total receive airtime in microseconds.
    /// 
    /// Labels: node, node_type, payload_type, route_type, payload_hash
    pub const RADIO_RX_AIRTIME: Metric = Metric::counter("mcsim.radio.rx_airtime_us")
        .with_description("Total receive airtime in microseconds")
        .with_unit(Unit::Microseconds)
        .with_labels(&["node", "node_type", "payload_type", "route_type", "payload_hash"]);

    /// Total packets transmitted.
    /// 
    /// Labels: node, node_type, payload_type, route_type, payload_hash
    /// 
    /// Use `route_type=flood` or `route_type=direct` to filter by routing mode.
    /// Use `payload_type=advert|txt_msg|ack|...` to filter by packet type.
    /// Use `payload_hash` to track individual packets across the network.
    pub const RADIO_TX_PACKETS: Metric = Metric::counter("mcsim.radio.tx_packets")
        .with_description("Total packets transmitted")
        .with_unit(Unit::Count)
        .with_labels(&["node", "node_type", "payload_type", "route_type", "payload_hash"]);

    /// Total packets successfully received.
    /// 
    /// Labels: node, node_type, payload_type, route_type, payload_hash
    /// 
    /// Use `route_type=flood` or `route_type=direct` to filter by routing mode.
    /// Use `payload_type=advert|txt_msg|ack|...` to filter by packet type.
    /// Use `payload_hash` to track individual packets across the network.
    pub const RADIO_RX_PACKETS: Metric = Metric::counter("mcsim.radio.rx_packets")
        .with_description("Total packets successfully received")
        .with_unit(Unit::Count)
        .with_labels(&["node", "node_type", "payload_type", "route_type", "payload_hash"]);

    /// Packets lost due to collision.
    /// 
    /// Labels: node, node_type, payload_type, route_type, payload_hash
    pub const RADIO_RX_COLLIDED: Metric = Metric::counter("mcsim.radio.rx_collided")
        .with_description("Packets lost due to collision")
        .with_unit(Unit::Count)
        .with_labels(&["node", "node_type", "payload_type", "route_type", "payload_hash"]);

    /// Packets lost due to low SNR.
    /// 
    /// Labels: node, node_type, payload_type, route_type, payload_hash
    pub const RADIO_RX_WEAK: Metric = Metric::counter("mcsim.radio.rx_weak")
        .with_description("Packets lost due to low SNR")
        .with_unit(Unit::Count)
        .with_labels(&["node", "node_type", "payload_type", "route_type", "payload_hash"]);

    /// TX to RX turnaround time in microseconds.
    /// 
    /// Labels: node, node_type (no packet type - measured per state transition)
    pub const RADIO_TURNAROUND_TIME: Metric = Metric::histogram("mcsim.radio.turnaround_time_us")
        .with_description("TX to RX turnaround time in microseconds")
        .with_unit(Unit::Microseconds)
        .with_labels(&["node", "node_type"]);

    /// Number of currently active receptions.
    /// 
    /// Labels: node, node_type (no packet type - measured per radio state)
    pub const RADIO_ACTIVE_RECEPTIONS: Metric = Metric::gauge("mcsim.radio.active_receptions")
        .with_description("Number of currently active receptions")
        .with_unit(Unit::Count)
        .with_labels(&["node", "node_type"]);

    /// Transmitted packet size in bytes.
    /// 
    /// Labels: node, node_type, payload_type, route_type, payload_hash
    pub const RADIO_TX_PACKET_SIZE: Metric = Metric::histogram("mcsim.radio.tx_packet_size_bytes")
        .with_description("Transmitted packet size in bytes")
        .with_unit(Unit::Bytes)
        .with_labels(&["node", "node_type", "payload_type", "route_type", "payload_hash"]);

    /// Received packet size in bytes.
    /// 
    /// Labels: node, node_type, payload_type, route_type, payload_hash
    pub const RADIO_RX_PACKET_SIZE: Metric = Metric::histogram("mcsim.radio.rx_packet_size_bytes")
        .with_description("Received packet size in bytes")
        .with_unit(Unit::Bytes)
        .with_labels(&["node", "node_type", "payload_type", "route_type", "payload_hash"]);

    /// Received signal-to-noise ratio in dB.
    /// 
    /// Labels: node, node_type, payload_type, route_type, payload_hash
    pub const RADIO_RX_SNR: Metric = Metric::histogram("mcsim.radio.rx_snr_db")
        .with_description("Received signal-to-noise ratio in dB")
        .with_labels(&["node", "node_type", "payload_type", "route_type", "payload_hash"]);

    /// Received signal strength in dBm.
    /// 
    /// Labels: node, node_type, payload_type, route_type, payload_hash
    pub const RADIO_RX_RSSI: Metric = Metric::histogram("mcsim.radio.rx_rssi_dbm")
        .with_description("Received signal strength in dBm")
        .with_labels(&["node", "node_type", "payload_type", "route_type", "payload_hash"]);

    // ========================================================================
    // Packet/Network Layer Metrics
    // ========================================================================
    //
    // NOTE: These metrics are now redundant with the radio metrics that include
    // payload_type and route_type labels. Use mcsim.radio.tx_packets{route_type=flood}
    // instead of mcsim.packet.tx_flood. These metrics are kept for backward
    // compatibility and will be removed in a future version.

    /// Flood packets transmitted.
    /// 
    /// **Deprecated**: Use `mcsim.radio.tx_packets{route_type=flood}` instead.
    /// 
    /// Labels: route_type, payload_type, payload_hash
    pub const PACKET_TX_FLOOD: Metric = Metric::counter("mcsim.packet.tx_flood")
        .with_description("Flood packets transmitted (use mcsim.radio.tx_packets{route_type=flood})")
        .with_unit(Unit::Count)
        .with_labels(&["route_type", "payload_type", "payload_hash"]);

    /// Direct packets transmitted.
    /// 
    /// **Deprecated**: Use `mcsim.radio.tx_packets{route_type=direct}` instead.
    /// 
    /// Labels: route_type, payload_type, payload_hash
    pub const PACKET_TX_DIRECT: Metric = Metric::counter("mcsim.packet.tx_direct")
        .with_description("Direct packets transmitted (use mcsim.radio.tx_packets{route_type=direct})")
        .with_unit(Unit::Count)
        .with_labels(&["route_type", "payload_type", "payload_hash"]);

    /// Flood packets received.
    /// 
    /// **Deprecated**: Use `mcsim.radio.rx_packets{route_type=flood}` instead.
    /// 
    /// Labels: route_type, payload_type, payload_hash
    pub const PACKET_RX_FLOOD: Metric = Metric::counter("mcsim.packet.rx_flood")
        .with_description("Flood packets received (use mcsim.radio.rx_packets{route_type=flood})")
        .with_unit(Unit::Count)
        .with_labels(&["route_type", "payload_type", "payload_hash"]);

    /// Direct packets received.
    /// 
    /// **Deprecated**: Use `mcsim.radio.rx_packets{route_type=direct}` instead.
    /// 
    /// Labels: route_type, payload_type, payload_hash
    pub const PACKET_RX_DIRECT: Metric = Metric::counter("mcsim.packet.rx_direct")
        .with_description("Direct packets received (use mcsim.radio.rx_packets{route_type=direct})")
        .with_unit(Unit::Count)
        .with_labels(&["route_type", "payload_type", "payload_hash"]);

    // ========================================================================
    // Message/Application Layer Metrics
    // ========================================================================

    /// Direct messages sent.
    pub const MESSAGE_SENT: Metric = Metric::counter("mcsim.dm.sent")
        .with_description("Direct messages sent")
        .with_unit(Unit::Count);

    /// Direct messages successfully delivered.
    pub const MESSAGE_DELIVERED: Metric = Metric::counter("mcsim.dm.delivered")
        .with_description("Direct messages successfully delivered")
        .with_unit(Unit::Count);

    /// Direct message delivery failures.
    pub const MESSAGE_FAILED: Metric = Metric::counter("mcsim.dm.failed")
        .with_description("Direct message delivery failures")
        .with_unit(Unit::Count);

    /// Direct messages acknowledged.
    pub const MESSAGE_ACKED: Metric = Metric::counter("mcsim.dm.acked")
        .with_description("Direct messages acknowledged")
        .with_unit(Unit::Count);

    /// Direct message delivery latency in milliseconds.
    pub const MESSAGE_DELIVERY_LATENCY: Metric = Metric::histogram("mcsim.dm.delivery_latency_ms")
        .with_description("Direct message delivery latency in milliseconds")
        .with_unit(Unit::Milliseconds);

    /// Direct message acknowledgement latency in milliseconds.
    pub const MESSAGE_ACK_LATENCY: Metric = Metric::histogram("mcsim.dm.ack_latency_ms")
        .with_description("Direct message acknowledgement latency in milliseconds")
        .with_unit(Unit::Milliseconds);

    /// Direct message hop count distribution.
    pub const MESSAGE_HOP_COUNT: Metric = Metric::histogram("mcsim.dm.hop_count")
        .with_description("Direct message hop count distribution")
        .with_unit(Unit::Count);

    // Flood Propagation

    /// Number of nodes that received a flood message.
    pub const FLOOD_NODES_REACHED: Metric = Metric::histogram("mcsim.flood.nodes_reached")
        .with_description("Number of nodes that received a flood message")
        .with_unit(Unit::Count);

    /// Times a flood packet was heard by any node.
    pub const FLOOD_TIMES_HEARD: Metric = Metric::histogram("mcsim.flood.times_heard")
        .with_description("Times a flood packet was heard by any node")
        .with_unit(Unit::Count);

    /// Time to reach furthest node in microseconds.
    pub const FLOOD_PROPAGATION_TIME: Metric = Metric::histogram("mcsim.flood.propagation_time_us")
        .with_description("Time to reach furthest node in microseconds")
        .with_unit(Unit::Microseconds);

    /// Fraction of reachable nodes covered by flood.
    pub const FLOOD_COVERAGE: Metric = Metric::gauge("mcsim.flood.coverage")
        .with_description("Fraction of reachable nodes covered by flood");

    // Direct Message Delivery

    /// Path messages sent.
    pub const DIRECT_SENT: Metric = Metric::counter("mcsim.path.sent")
        .with_description("Path messages sent")
        .with_unit(Unit::Count);

    /// Path messages successfully delivered.
    pub const DIRECT_DELIVERED: Metric = Metric::counter("mcsim.path.delivered")
        .with_description("Path messages successfully delivered")
        .with_unit(Unit::Count);

    /// Path messages that failed delivery.
    pub const DIRECT_FAILED: Metric = Metric::counter("mcsim.path.failed")
        .with_description("Path messages that failed delivery")
        .with_unit(Unit::Count);

    /// Path message delivery latency in milliseconds.
    pub const DIRECT_DELIVERY_LATENCY: Metric = Metric::histogram("mcsim.path.delivery_latency_ms")
        .with_description("Path message delivery latency in milliseconds")
        .with_unit(Unit::Milliseconds);

    /// Hop count for delivered path messages.
    pub const DIRECT_HOPS: Metric = Metric::histogram("mcsim.path.hops")
        .with_description("Hop count for delivered path messages")
        .with_unit(Unit::Count);

    // Timing

    /// Delay before transmission in microseconds.
    pub const TIMING_TX_DELAY: Metric = Metric::histogram("mcsim.timing.tx_delay_us")
        .with_description("Delay before transmission in microseconds")
        .with_unit(Unit::Microseconds);

    /// Time to process received packet in microseconds.
    pub const TIMING_RX_PROCESS_DELAY: Metric = Metric::histogram("mcsim.timing.rx_process_delay_us")
        .with_description("Time to process received packet in microseconds")
        .with_unit(Unit::Microseconds);

    /// Time packet waits in TX queue in microseconds.
    pub const TIMING_QUEUE_WAIT: Metric = Metric::histogram("mcsim.timing.queue_wait_us")
        .with_description("Time packet waits in TX queue in microseconds")
        .with_unit(Unit::Microseconds);

    // Simulation Performance

    /// Wall-clock time to execute a simulation step for an entity.
    /// 
    /// Labels: node, node_type
    pub const SIMULATION_STEP_TIME: Metric = Metric::histogram("mcsim.simulation.step_time_us")
        .with_description("Wall-clock time to execute a simulation step in microseconds")
        .with_unit(Unit::Microseconds)
        .with_labels(&["node", "node_type"]);

    /// Returns a slice of all defined metrics.
    pub const ALL: &[&Metric] = &[
        // Radio/PHY Layer
        &RADIO_TX_AIRTIME,
        &RADIO_RX_AIRTIME,
        &RADIO_TX_PACKETS,
        &RADIO_RX_PACKETS,
        &RADIO_RX_COLLIDED,
        &RADIO_RX_WEAK,
        &RADIO_TURNAROUND_TIME,
        &RADIO_ACTIVE_RECEPTIONS,
        &RADIO_TX_PACKET_SIZE,
        &RADIO_RX_PACKET_SIZE,
        &RADIO_RX_SNR,
        &RADIO_RX_RSSI,
        // Packet/Network Layer
        &PACKET_TX_FLOOD,
        &PACKET_TX_DIRECT,
        &PACKET_RX_FLOOD,
        &PACKET_RX_DIRECT,
        // Message/Application Layer
        &MESSAGE_SENT,
        &MESSAGE_DELIVERED,
        &MESSAGE_FAILED,
        &MESSAGE_ACKED,
        &MESSAGE_DELIVERY_LATENCY,
        &MESSAGE_ACK_LATENCY,
        &MESSAGE_HOP_COUNT,
        // Flood Propagation
        &FLOOD_NODES_REACHED,
        &FLOOD_TIMES_HEARD,
        &FLOOD_PROPAGATION_TIME,
        &FLOOD_COVERAGE,
        // Direct Message Delivery
        &DIRECT_SENT,
        &DIRECT_DELIVERED,
        &DIRECT_FAILED,
        &DIRECT_DELIVERY_LATENCY,
        &DIRECT_HOPS,
        // Timing
        &TIMING_TX_DELAY,
        &TIMING_RX_PROCESS_DELAY,
        &TIMING_QUEUE_WAIT,
        // Simulation Performance
        &SIMULATION_STEP_TIME,
    ];
}

/// Metric labels for identifying and grouping metrics by node and network.
///
/// # Example
///
/// ```rust
/// use mcsim_metrics::MetricLabels;
///
/// let labels = MetricLabels::new("node_001", "repeater")
///     .with_groups(vec!["region_a".to_string(), "cluster_1".to_string()]);
///
/// let label_vec = labels.to_labels();
/// assert_eq!(label_vec.len(), 3);
/// ```
#[derive(Debug, Clone)]
pub struct MetricLabels {
    /// Individual node identifier
    pub node: String,
    /// Type of node (repeater, room_server, client, sensor)
    pub node_type: String,
    /// Custom grouping tags
    pub groups: Vec<String>,
}

impl MetricLabels {
    /// Creates a new `MetricLabels` instance with the given node and node type.
    ///
    /// # Arguments
    ///
    /// * `node` - Individual node identifier
    /// * `node_type` - Type of node (repeater, room_server, client, sensor)
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcsim_metrics::MetricLabels;
    ///
    /// let labels = MetricLabels::new("node_001", "repeater");
    /// assert_eq!(labels.node, "node_001");
    /// assert_eq!(labels.node_type, "repeater");
    /// ```
    pub fn new(
        node: impl Into<String>,
        node_type: impl Into<String>,
    ) -> Self {
        Self {
            node: node.into(),
            node_type: node_type.into(),
            groups: Vec::new(),
        }
    }

    /// Adds custom grouping tags to the labels.
    ///
    /// # Arguments
    ///
    /// * `groups` - A vector of custom grouping tags
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcsim_metrics::MetricLabels;
    ///
    /// let labels = MetricLabels::new("node_001", "repeater")
    ///     .with_groups(vec!["group_a".to_string(), "region_1".to_string()]);
    /// assert_eq!(labels.groups.len(), 2);
    /// ```
    pub fn with_groups(mut self, groups: Vec<String>) -> Self {
        self.groups = groups;
        self
    }

    /// Converts the labels to the metrics crate label format.
    ///
    /// Returns a vector of (key, value) pairs suitable for use with the `metrics` crate.
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcsim_metrics::MetricLabels;
    ///
    /// let labels = MetricLabels::new("node_001", "repeater")
    ///     .with_groups(vec!["group_a".to_string()]);
    /// let label_vec = labels.to_labels();
    ///
    /// assert!(label_vec.iter().any(|(k, v)| *k == "node" && v == "node_001"));
    /// assert!(label_vec.iter().any(|(k, v)| *k == "node_type" && v == "repeater"));
    /// assert!(label_vec.iter().any(|(k, v)| *k == "groups" && v == "group_a"));
    /// ```
    pub fn to_labels(&self) -> Vec<(&'static str, String)> {
        let mut labels = vec![
            ("node", self.node.clone()),
            ("node_type", self.node_type.clone()),
        ];

        if !self.groups.is_empty() {
            labels.push(("groups", self.groups.join(",")));
        }

        labels
    }

    /// Returns labels with additional key-value pairs.
    ///
    /// # Arguments
    ///
    /// * `extra` - Additional key-value pairs to include
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcsim_metrics::MetricLabels;
    ///
    /// let labels = MetricLabels::new("node_001", "repeater");
    /// let extended = labels.with(&[("direction", "tx".to_string())]);
    ///
    /// assert!(extended.iter().any(|(k, v)| *k == "direction" && v == "tx"));
    /// ```
    pub fn with(&self, extra: &[(&'static str, String)]) -> Vec<(&'static str, String)> {
        let mut labels = self.to_labels();
        labels.extend_from_slice(extra);
        labels
    }
}

/// Describes all metrics used in the simulator.
///
/// This function should be called once at startup to register all metric descriptions
/// with the metrics recorder. This enables better documentation and introspection
/// of metrics in exporters like Prometheus.
///
/// # Example
///
/// ```rust,ignore
/// use mcsim_metrics::describe_metrics;
///
/// fn main() {
///     // Initialize your metrics recorder first
///     // ...
///
///     // Then describe all metrics
///     describe_metrics();
/// }
/// ```
pub fn describe_metrics() {
    for metric in metric_defs::ALL {
        metric.describe();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_labels_new() {
        let labels = MetricLabels::new("node_001", "repeater");
        assert_eq!(labels.node, "node_001");
        assert_eq!(labels.node_type, "repeater");
        assert!(labels.groups.is_empty());
    }

    #[test]
    fn test_metric_labels_with_groups() {
        let labels = MetricLabels::new("node_001", "repeater")
            .with_groups(vec!["group_a".to_string(), "group_b".to_string()]);

        assert_eq!(labels.groups.len(), 2);
        assert_eq!(labels.groups[0], "group_a");
        assert_eq!(labels.groups[1], "group_b");
    }

    #[test]
    fn test_to_labels_without_groups() {
        let labels = MetricLabels::new("node_001", "repeater");
        let label_vec = labels.to_labels();

        assert_eq!(label_vec.len(), 2);
        assert!(label_vec.contains(&("node", "node_001".to_string())));
        assert!(label_vec.contains(&("node_type", "repeater".to_string())));
    }

    #[test]
    fn test_to_labels_with_groups() {
        let labels = MetricLabels::new("node_001", "repeater")
            .with_groups(vec!["group_a".to_string(), "group_b".to_string()]);
        let label_vec = labels.to_labels();

        assert_eq!(label_vec.len(), 3);
        assert!(label_vec.contains(&("groups", "group_a,group_b".to_string())));
    }

    #[test]
    fn test_with_extra_labels() {
        let labels = MetricLabels::new("node_001", "repeater");
        let extended = labels.with(&[("direction", "tx".to_string()), ("channel", "1".to_string())]);

        assert_eq!(extended.len(), 4);
        assert!(extended.contains(&("direction", "tx".to_string())));
        assert!(extended.contains(&("channel", "1".to_string())));
    }

    #[test]
    fn test_metric_definitions() {
        // Verify some metric definitions are correctly defined
        assert_eq!(metric_defs::RADIO_TX_AIRTIME.name, "mcsim.radio.tx_airtime_us");
        assert_eq!(metric_defs::RADIO_TX_AIRTIME.kind, MetricKind::Counter);
        assert_eq!(metric_defs::RADIO_TX_AIRTIME.unit, Some(Unit::Microseconds));

        assert_eq!(metric_defs::RADIO_RX_PACKETS.name, "mcsim.radio.rx_packets");
        assert_eq!(metric_defs::MESSAGE_DELIVERED.name, "mcsim.dm.delivered");
        assert_eq!(metric_defs::FLOOD_COVERAGE.name, "mcsim.flood.coverage");
        assert_eq!(metric_defs::FLOOD_COVERAGE.kind, MetricKind::Gauge);
        assert_eq!(metric_defs::DIRECT_HOPS.name, "mcsim.path.hops");
        assert_eq!(metric_defs::DIRECT_HOPS.kind, MetricKind::Histogram);
        assert_eq!(metric_defs::TIMING_QUEUE_WAIT.name, "mcsim.timing.queue_wait_us");
    }

    #[test]
    fn test_all_metrics_count() {
        // Verify we have all 36 metrics in the ALL slice
        assert_eq!(metric_defs::ALL.len(), 36);
    }

    #[test]
    fn test_metric_counter() {
        const TEST_COUNTER: Metric = Metric::counter("test.counter")
            .with_description("A test counter")
            .with_unit(Unit::Count)
            .with_labels(&["node", "direction"]);

        assert_eq!(TEST_COUNTER.name, "test.counter");
        assert_eq!(TEST_COUNTER.kind, MetricKind::Counter);
        assert_eq!(TEST_COUNTER.description, "A test counter");
        assert_eq!(TEST_COUNTER.unit, Some(Unit::Count));
        assert_eq!(TEST_COUNTER.labels, &["node", "direction"]);
    }

    #[test]
    fn test_metric_gauge() {
        const TEST_GAUGE: Metric = Metric::gauge("test.gauge")
            .with_description("A test gauge")
            .with_unit(Unit::Bytes);

        assert_eq!(TEST_GAUGE.name, "test.gauge");
        assert_eq!(TEST_GAUGE.kind, MetricKind::Gauge);
        assert_eq!(TEST_GAUGE.description, "A test gauge");
        assert_eq!(TEST_GAUGE.unit, Some(Unit::Bytes));
    }

    #[test]
    fn test_metric_histogram() {
        const TEST_HISTOGRAM: Metric = Metric::histogram("test.histogram")
            .with_description("A test histogram")
            .with_unit(Unit::Microseconds)
            .with_labels(&["node"]);

        assert_eq!(TEST_HISTOGRAM.name, "test.histogram");
        assert_eq!(TEST_HISTOGRAM.kind, MetricKind::Histogram);
        assert_eq!(TEST_HISTOGRAM.description, "A test histogram");
        assert_eq!(TEST_HISTOGRAM.unit, Some(Unit::Microseconds));
        assert_eq!(TEST_HISTOGRAM.labels, &["node"]);
    }

    #[test]
    fn test_metric_minimal() {
        // Test that a metric with only a name works
        const MINIMAL: Metric = Metric::counter("minimal");

        assert_eq!(MINIMAL.name, "minimal");
        assert_eq!(MINIMAL.kind, MetricKind::Counter);
        assert_eq!(MINIMAL.description, "");
        assert_eq!(MINIMAL.unit, None);
        assert_eq!(MINIMAL.labels, &[] as &[&str]);
    }
}
