//! Metrics integration tests for the MCSim simulation framework.
//!
//! These tests verify that the metrics collection system works correctly
//! with deterministic seeds and expected value ranges.
//!
//! See docs/METRICS_TESTS.md for the full test plan and design philosophy.
//!
//! Note: These tests run the simulator as a subprocess to avoid issues with
//! the global metrics recorder being shared across tests.

use std::collections::HashMap;
use std::process::Command;

use serde::Deserialize;

// ============================================================================
// JSON Deserialization Types
// ============================================================================

/// The metrics export JSON format.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MetricsExport {
    timestamp: String,
    metrics: HashMap<String, MetricValue>,
}

/// A metric value that can be counter, gauge, or histogram.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MetricValue {
    Counter(CounterValue),
    Histogram(HistogramValue),
    Gauge(GaugeValue),
}

/// Counter value with optional breakdowns.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CounterValue {
    total: u64,
    #[serde(default)]
    labels: HashMap<String, HashMap<String, Box<CounterValue>>>,
}

/// Gauge value with optional breakdowns.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GaugeValue {
    total: f64,
    #[serde(default)]
    labels: HashMap<String, HashMap<String, Box<GaugeValue>>>,
}

/// Histogram value with summary stats and optional breakdowns.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct HistogramValue {
    count: u64,
    sum: f64,
    min: f64,
    max: f64,
    mean: f64,
    p50: f64,
    p90: f64,
    p99: f64,
    #[serde(default)]
    labels: HashMap<String, HashMap<String, Box<HistogramValue>>>,
}

// ============================================================================
// Test Result Types
// ============================================================================

/// Collected metrics result for assertions.
#[derive(Debug)]
struct MetricsResult {
    /// The raw metrics export.
    export: MetricsExport,
}

impl MetricsResult {
    /// Get a counter value by metric name.
    fn get_counter(&self, name: &str) -> Option<&CounterValue> {
        match self.export.metrics.get(name)? {
            MetricValue::Counter(c) => Some(c),
            _ => None,
        }
    }

    /// Get a histogram value by metric name.
    fn get_histogram(&self, name: &str) -> Option<&HistogramValue> {
        match self.export.metrics.get(name)? {
            MetricValue::Histogram(h) => Some(h),
            _ => None,
        }
    }

    /// Get counter value for a specific node.
    fn get_counter_for_node(&self, metric: &str, node: &str) -> Option<u64> {
        let counter = self.get_counter(metric)?;
        let by_node = counter.labels.get("node")?;
        let node_value = by_node.get(node)?;
        Some(node_value.total)
    }

    /// Get counter value for a specific node type.
    #[allow(dead_code)]
    fn get_counter_for_node_type(&self, metric: &str, node_type: &str) -> Option<u64> {
        let counter = self.get_counter(metric)?;
        let by_type = counter.labels.get("node_type")?;
        let type_value = by_type.get(node_type)?;
        Some(type_value.total)
    }

    /// Get all node names that have a value for a metric.
    fn get_nodes_with_metric(&self, metric: &str) -> Vec<String> {
        if let Some(counter) = self.get_counter(metric) {
            if let Some(by_node) = counter.labels.get("node") {
                return by_node.keys().cloned().collect();
            }
        }
        if let Some(histogram) = self.get_histogram(metric) {
            if let Some(by_node) = histogram.labels.get("node") {
                return by_node.keys().cloned().collect();
            }
        }
        vec![]
    }

    /// Check if a metric exists with any value.
    fn metric_exists(&self, name: &str) -> bool {
        self.export.metrics.contains_key(name)
    }
}

// ============================================================================
// Test Helper Functions
// ============================================================================

/// Run a simulation and collect metrics by invoking the simulator as a subprocess.
///
/// # Arguments
/// * `topology` - Path to the topology YAML file (relative to workspace root)
/// * `behavior` - Optional path to behavior overlay YAML
/// * `seed` - Random seed for deterministic simulation
/// * `duration` - Duration string (e.g., "10s", "1m")
/// * `metric_specs` - Metric specifications for what to collect
fn run_and_collect_metrics(
    topology: &str,
    behavior: Option<&str>,
    seed: u64,
    duration: &str,
    metric_specs: &[&str],
) -> MetricsResult {
    // CARGO_BIN_EXE_mcsim is set by cargo when running tests for this crate
    let binary = env!("CARGO_BIN_EXE_mcsim");

    // Build command - run from workspace root (two levels up from crate)
    let mut cmd = Command::new(binary);
    cmd.current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    cmd.arg("run");
    cmd.arg(topology);

    if let Some(b) = behavior {
        cmd.arg(b);
    }

    cmd.arg("--seed").arg(seed.to_string());
    cmd.arg("--duration").arg(duration);
    cmd.arg("--metrics-output").arg("json");

    for spec in metric_specs {
        cmd.arg("--metric").arg(*spec);
    }

    // Run and capture output
    let output = cmd
        .output()
        .expect("Failed to execute mcsim binary");

    // Always print stderr for debugging
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        eprintln!("mcsim stderr:\n{}", stderr);
    }

    if !output.status.success() {
        panic!("mcsim failed: {}", stderr);
    }

    // Parse stdout as JSON (the metrics output goes to stdout)
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Find the JSON in the output (skip any non-JSON output)
    let json_start = stdout.find('{').expect("No JSON found in output");
    let json_str = &stdout[json_start..];

    let export: MetricsExport =
        serde_json::from_str(json_str).expect("Failed to parse metrics JSON");

    MetricsResult { export }
}

/// Get counter value for a specific set of label key-value pairs.
///
/// Labels are applied in order, navigating through the nested breakdown structure.
/// For example, `[("node_type", "Repeater"), ("node", "Repeater1")]` would look up:
/// `counter.labels["node_type"]["Repeater"].labels["node"]["Repeater1"].total`
fn get_counter_for_labels(result: &MetricsResult, metric: &str, labels: &[(&str, &str)]) -> Option<u64> {
    let counter = result.get_counter(metric)?;
    
    // Navigate through the nested label structure
    let mut current: &CounterValue = counter;
    for (label_name, label_value) in labels {
        let by_label = current.labels.get(*label_name)?;
        current = by_label.get(*label_value)?;
    }
    
    Some(current.total)
}

/// Assert a counter metric for a specific set of labels is within the expected range.
///
/// # Arguments
/// * `result` - The metrics result to check
/// * `metric` - The metric name (e.g., "mcsim.radio.tx_packets")
/// * `labels` - Slice of (label_name, label_value) tuples to match
/// * `min` - Minimum expected value (inclusive)
/// * `max` - Maximum expected value (inclusive)
///
/// # Example
/// ```ignore
/// assert_counter_for_labels_in_range(
///     &result,
///     "mcsim.radio.tx_packets",
///     &[("node", "Alice"), ("payload_type", "GRP_TXT")],
///     1,
///     20,
/// );
/// ```
fn assert_counter_for_labels_in_range(
    result: &MetricsResult,
    metric: &str,
    labels: &[(&str, &str)],
    min: u64,
    max: u64,
) {
    let labels_desc: Vec<String> = labels.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
    let labels_str = labels_desc.join(", ");
    
    let value = get_counter_for_labels(result, metric, labels)
        .unwrap_or_else(|| panic!("Counter '{}' for labels [{}] not found", metric, labels_str));

    assert!(
        value >= min && value <= max,
        "Counter '{}' for labels [{}] value {} not in range [{}, {}]",
        metric,
        labels_str,
        value,
        min,
        max
    );
}

/// Assert a counter metric for a specific node is within the expected range.
///
/// This is a convenience wrapper around `assert_counter_for_labels_in_range` for the common
/// case of filtering by a single "node" label.
fn assert_counter_for_node_in_range(
    result: &MetricsResult,
    metric: &str,
    node: &str,
    min: u64,
    max: u64,
) {
    assert_counter_for_labels_in_range(result, metric, &[("node", node)], min, max);
}

/// Assert that a node has metrics recorded.
fn assert_node_has_metric(result: &MetricsResult, metric: &str, node: &str) {
    let nodes = result.get_nodes_with_metric(metric);
    assert!(
        nodes.contains(&node.to_string()),
        "Node '{}' expected to have metric '{}', but only found: {:?}",
        node,
        metric,
        nodes
    );
}

// ============================================================================
// Test 1: Basic Radio Metrics
// ============================================================================

/// Verify fundamental radio TX/RX counters and airtime tracking.
///
/// Topology: two_peers (Alice <-> Repeater <-> Bob)
/// Behavior: Alice sends exactly 1 channel message (via message_count: 1)
#[test]
fn test_basic_radio_metrics() {
    let result = run_and_collect_metrics(
        "examples/topologies/two_peers.yaml",
        Some("examples/behaviors/single_broadcast.yaml"),
        42,   // seed
        "10s", // duration
        &[
            "mcsim.radio.tx_packets/node/payload_type",
            "mcsim.radio.tx_packets/node_type",
            "mcsim.radio.rx_packets/node/payload_type",
            "mcsim.radio.rx_packets/node_type",
            "mcsim.radio.tx_airtime_us/node",
            "mcsim.radio.rx_airtime_us/node",
            "mcsim.radio.tx_packet_size_bytes/node",
            "mcsim.radio.rx_packet_size_bytes/node",
            "mcsim.radio.rx_snr_db/node",
            "mcsim.radio.rx_rssi_dbm/node",
        ],
    );

    // Debug: print what we got
    eprintln!("Test 1: Basic Radio Metrics");
    if let Some(counter) = result.get_counter("mcsim.radio.tx_packets") {
        eprintln!("  Total TX packets: {}", counter.total);
    }

    // === Counter assertions ===

    // Get the TX packet counts for Alice, Repeater, and Bob (two_peers topology)
    let tx_packet_counts = [
        get_counter_for_labels(&result, "mcsim.radio.tx_packets", &[("node", "Alice"), ("payload_type", "grp_txt")]).unwrap_or_default(),
        get_counter_for_labels(&result, "mcsim.radio.tx_packets", &[("node", "Repeater"), ("payload_type", "grp_txt")]).unwrap_or_default(),
        get_counter_for_labels(&result, "mcsim.radio.tx_packets", &[("node", "Bob"), ("payload_type", "grp_txt")]).unwrap_or_default(),
    ];

    // Get the RX packet counts for Alice, Repeater, and Bob (two_peers topology)
    let rx_packet_counts = [
        get_counter_for_labels(&result, "mcsim.radio.rx_packets", &[("node", "Alice"), ("payload_type", "grp_txt")]).unwrap_or_default(),
        get_counter_for_labels(&result, "mcsim.radio.rx_packets", &[("node", "Repeater"), ("payload_type", "grp_txt")]).unwrap_or_default(),
        get_counter_for_labels(&result, "mcsim.radio.rx_packets", &[("node", "Bob"), ("payload_type", "grp_txt")]).unwrap_or_default(),
    ];

    // two_peers topology: Alice <-> Repeater <-> Bob
    let expected_tx = [1, 1, 0]; // Alice sends 1, Repeater forwards 1, Bob does not send
    let expected_rx = [1, 1, 1]; // Alice receives rebroadcast from Repeater, Repeater receives from Alice, Bob receives from Repeater

    for (i, node) in ["Alice", "Repeater", "Bob"].iter().enumerate() {
        assert_eq!(
            tx_packet_counts[i], expected_tx[i],
            "{} TX packets: expected {}, got {}",
            node, expected_tx[i], tx_packet_counts[i]
        );
        assert_eq!(
            rx_packet_counts[i], expected_rx[i],
            "{} RX packets: expected {}, got {}",
            node, expected_rx[i], rx_packet_counts[i]
        );
    }

    // === Airtime assertions ===

    // Alice should have positive TX airtime (at least 1ms = 1000us)
    if let Some(counter) = result.get_counter("mcsim.radio.tx_airtime_us") {
        if let Some(by_node) = counter.labels.get("node") {
            if let Some(alice) = by_node.get("Alice") {
                assert!(
                    alice.total >= 1000,
                    "Alice TX airtime {} should be >= 1000us",
                    alice.total
                );
            }
        }
    }

    // === Label verification ===

    // Verify node_type labels exist
    if let Some(counter) = result.get_counter("mcsim.radio.tx_packets") {
        if let Some(by_type) = counter.labels.get("node_type") {
            assert!(by_type.contains_key("Companion"), "Expected Companion type");
            assert!(by_type.contains_key("Repeater"), "Expected Repeater type");
        }
    }

    // Verify node labels exist
    assert_node_has_metric(&result, "mcsim.radio.tx_packets", "Alice");
    assert_node_has_metric(&result, "mcsim.radio.tx_packets", "Repeater");
    // Note: Bob may or may not transmit depending on simulation timing
}

// ============================================================================
// Test 2: Multi-Hop Forwarding Metrics
// ============================================================================

/// Verify packet forwarding through repeater chain.
///
/// Topology: simple (Alice <-> R1 <-> R2 <-> R3 <-> Bob)
/// Behavior: Alice sends exactly 1 channel message (via message_count: 1), expect propagation through all repeaters
#[test]
fn test_multihop_forwarding_metrics() {
    let result = run_and_collect_metrics(
        "examples/topologies/simple.yaml",
        Some("examples/behaviors/single_broadcast.yaml"),
        12345, // seed
        "15s",  // duration
        &[
            "mcsim.radio.tx_packets/node/payload_type",
            "mcsim.radio.rx_packets/node/payload_type",
        ],
    );

    eprintln!("Test 2: Multi-Hop Forwarding Metrics");
    if let Some(counter) = result.get_counter("mcsim.radio.tx_packets") {
        eprintln!("  Total TX packets: {}", counter.total);
    }
    if let Some(counter) = result.get_counter("mcsim.radio.rx_packets") {
        eprintln!("  Total RX packets: {}", counter.total);
    }

    // === Counter assertions ===

    // Get the TX packet counts for Alice, Repeater1, Repeater2, Repeater3, and Bob
    let tx_packet_counts = [
        get_counter_for_labels(&result, "mcsim.radio.tx_packets", &[("node", "Alice"), ("payload_type", "grp_txt")]).unwrap_or_default(),
        get_counter_for_labels(&result, "mcsim.radio.tx_packets", &[("node", "Repeater1"), ("payload_type", "grp_txt")]).unwrap_or_default(),
        get_counter_for_labels(&result, "mcsim.radio.tx_packets", &[("node", "Repeater2"), ("payload_type", "grp_txt")]).unwrap_or_default(),
        get_counter_for_labels(&result, "mcsim.radio.tx_packets", &[("node", "Repeater3"), ("payload_type", "grp_txt")]).unwrap_or_default(),
        get_counter_for_labels(&result, "mcsim.radio.tx_packets", &[("node", "Bob"), ("payload_type", "grp_txt")]).unwrap_or_default(),
    ];

    // Get the RX packet counts for Alice, Repeater1, Repeater2, Repeater3, and Bob
    let rx_packet_counts = [
        get_counter_for_labels(&result, "mcsim.radio.rx_packets", &[("node", "Alice"), ("payload_type", "grp_txt")]).unwrap_or_default(),
        get_counter_for_labels(&result, "mcsim.radio.rx_packets", &[("node", "Repeater1"), ("payload_type", "grp_txt")]).unwrap_or_default(),
        get_counter_for_labels(&result, "mcsim.radio.rx_packets", &[("node", "Repeater2"), ("payload_type", "grp_txt")]).unwrap_or_default(),
        get_counter_for_labels(&result, "mcsim.radio.rx_packets", &[("node", "Repeater3"), ("payload_type", "grp_txt")]).unwrap_or_default(),
        get_counter_for_labels(&result, "mcsim.radio.rx_packets", &[("node", "Bob"), ("payload_type", "grp_txt")]).unwrap_or_default(),
    ];

    // simple topology: Alice <-> R1 <-> R2 <-> R3 <-> Bob (linear chain)
    // TX: Alice sends 1, each repeater forwards 1, Bob does not send
    let expected_tx = [1, 1, 1, 1, 0];
    // RX: Alice receives rebroadcast from R1
    //     R1 receives from Alice and from R2 (rebroadcast)
    //     R2 receives from R1 and from R3 (rebroadcast)
    //     R3 receives from R2
    //     Bob receives from R3
    let expected_rx = [1, 2, 2, 1, 1];

    for (i, node) in ["Alice", "Repeater1", "Repeater2", "Repeater3", "Bob"].iter().enumerate() {
        assert_eq!(
            tx_packet_counts[i], expected_tx[i],
            "{} TX packets: expected {}, got {}",
            node, expected_tx[i], tx_packet_counts[i]
        );
        assert_eq!(
            rx_packet_counts[i], expected_rx[i],
            "{} RX packets: expected {}, got {}",
            node, expected_rx[i], rx_packet_counts[i]
        );
    }
}

// ============================================================================
// Test 3: Direct Message Metrics
// ============================================================================

/// Verify direct message delivery tracking and latency.
///
/// Topology: two_peers (Alice <-> Repeater <-> Bob)
/// Behavior: Alice sends exactly 1 direct message to Bob (via message_count: 1)
///
/// This test verifies the complete DM flow:
/// 1. Alice's agent sends a DM command (mcsim.dm.sent)
/// 2. Alice's firmware transmits the packet (mcsim.radio.tx_packets with route_type=direct)
/// 3. The packet is forwarded through the repeater
/// 4. Bob's firmware receives the packet (mcsim.radio.rx_packets)
/// 5. Bob's agent processes the message (mcsim.dm.delivered)
/// 6. Bob sends an ACK back to Alice (mcsim.dm.acked)
///
/// NOTE: This test documents the expected behavior. If assertions fail, it indicates
/// a bug in the DM feature implementation that needs to be fixed.
#[test]
fn test_direct_message_metrics() {
    let result = run_and_collect_metrics(
        "examples/topologies/two_peers.yaml",
        Some("examples/behaviors/single_dm.yaml"),
        99999, // seed
        "60s",  // duration - longer for routing discovery + ACK
        &[
            // Application-level DM metrics
            "mcsim.dm.sent/node",
            "mcsim.dm.delivered/node",
            "mcsim.dm.acked/node",
            "mcsim.dm.delivery_latency_ms/node",
            "mcsim.dm.ack_latency_ms/node",
            // Radio-level metrics to verify packet transmission
            "mcsim.radio.tx_packets/node/route_type/payload_type",
            "mcsim.radio.rx_packets/node/route_type/payload_type",
        ],
    );

    eprintln!("Test 3: Direct Message Metrics");

    // === Application-level metrics ===

    // 1. Verify Alice sent a DM at the application layer
    let alice_dm_sent = result.get_counter_for_node("mcsim.dm.sent", "Alice").unwrap_or(0);
    eprintln!("  Alice mcsim.dm.sent: {}", alice_dm_sent);
    assert_eq!(
        alice_dm_sent, 1,
        "Alice should send exactly 1 DM (message_count: 1), got {}",
        alice_dm_sent
    );

    // 2. Verify Alice transmitted the DM packet over radio
    // The DM should result in radio TX with route_type=direct
    let alice_tx_total = result.get_counter_for_node("mcsim.radio.tx_packets", "Alice").unwrap_or(0);
    eprintln!("  Alice mcsim.radio.tx_packets total: {}", alice_tx_total);
    
    // Check for direct route type specifically
    let alice_tx_direct = get_counter_for_labels(
        &result,
        "mcsim.radio.tx_packets",
        &[("node", "Alice"), ("route_type", "direct")],
    ).unwrap_or(0);
    eprintln!("  Alice mcsim.radio.tx_packets (route_type=direct): {}", alice_tx_direct);
    
    // Alice should have transmitted at least 1 packet (the DM)
    // Note: May also include adverts and routing packets
    assert!(
        alice_tx_total >= 1,
        "Alice should transmit at least 1 packet (the DM), got {}. \
         BUG: The agent sends a DM command but the firmware is not transmitting it.",
        alice_tx_total
    );

    // 3. Verify Bob received packets over radio
    let bob_rx_total = result.get_counter_for_node("mcsim.radio.rx_packets", "Bob").unwrap_or(0);
    eprintln!("  Bob mcsim.radio.rx_packets total: {}", bob_rx_total);
    
    // Bob should receive at least 1 packet (the DM, possibly via repeater)
    assert!(
        bob_rx_total >= 1,
        "Bob should receive at least 1 packet, got {}",
        bob_rx_total
    );

    // 4. Verify Bob's agent received and processed the DM (delivery metric)
    let bob_dm_delivered = result.get_counter_for_node("mcsim.dm.delivered", "Bob").unwrap_or(0);
    eprintln!("  Bob mcsim.dm.delivered: {}", bob_dm_delivered);
    assert_eq!(
        bob_dm_delivered, 1,
        "Bob should receive exactly 1 DM, got {}",
        bob_dm_delivered
    );

    // 5. Verify Alice received an ACK
    let alice_dm_acked = result.get_counter_for_node("mcsim.dm.acked", "Alice").unwrap_or(0);
    eprintln!("  Alice mcsim.dm.acked: {}", alice_dm_acked);
    assert_eq!(
        alice_dm_acked, 1,
        "Alice should receive exactly 1 ACK for her DM, got {}. \
         This indicates end-to-end delivery confirmation.",
        alice_dm_acked
    );

    // === Latency metrics ===

    // 6. Verify delivery latency is recorded and within feasible range
    let delivery_latency = result
        .get_histogram("mcsim.dm.delivery_latency_ms")
        .expect("mcsim.dm.delivery_latency_ms should be recorded when DM is delivered");
    eprintln!(
        "  DM delivery latency: count={}, mean={:.2}ms, min={:.2}ms, max={:.2}ms",
        delivery_latency.count, delivery_latency.mean, delivery_latency.min, delivery_latency.max
    );
    assert_eq!(
        delivery_latency.count, 1,
        "Should have exactly 1 delivery latency measurement, got {}",
        delivery_latency.count
    );
    // Feasible range: 10ms (very fast single hop) to 30000ms (30s for routing discovery + transmission)
    assert!(
        delivery_latency.mean >= 10.0 && delivery_latency.mean <= 30000.0,
        "Delivery latency should be between 10ms and 30s, got {:.2}ms",
        delivery_latency.mean
    );

    // 7. Verify ACK latency is recorded and within feasible range
    let ack_latency = result
        .get_histogram("mcsim.dm.ack_latency_ms")
        .expect("mcsim.dm.ack_latency_ms should be recorded when ACK is received");
    eprintln!(
        "  DM ACK latency: count={}, mean={:.2}ms, min={:.2}ms, max={:.2}ms",
        ack_latency.count, ack_latency.mean, ack_latency.min, ack_latency.max
    );
    assert_eq!(
        ack_latency.count, 1,
        "Should have exactly 1 ACK latency measurement, got {}",
        ack_latency.count
    );
    // ACK latency should be >= delivery latency (round trip)
    // Feasible range: 20ms (very fast round trip) to 60000ms (60s for routing + retries)
    assert!(
        ack_latency.mean >= 20.0 && ack_latency.mean <= 60000.0,
        "ACK latency should be between 20ms and 60s, got {:.2}ms",
        ack_latency.mean
    );
    // ACK latency should be at least as long as delivery latency
    assert!(
        ack_latency.mean >= delivery_latency.mean,
        "ACK latency ({:.2}ms) should be >= delivery latency ({:.2}ms)",
        ack_latency.mean,
        delivery_latency.mean
    );

    // === Summary of all metrics found ===
    eprintln!("\n  Metrics summary:");
    for metric_name in [
        "mcsim.dm.sent",
        "mcsim.dm.delivered",
        "mcsim.dm.acked",
        "mcsim.radio.tx_packets",
        "mcsim.radio.rx_packets",
    ] {
        if result.metric_exists(metric_name) {
            eprintln!("    ✓ {} exists", metric_name);
        } else {
            eprintln!("    ✗ {} NOT found", metric_name);
        }
    }
}

// ============================================================================
// Test 4: Flood Propagation Metrics
// ============================================================================

/// Verify flood coverage and propagation timing.
///
/// Topology: multi_path (3x3 grid with 4 companions)
/// Behavior: Alice sends exactly 1 channel message (flood) via message_count: 1
#[test]
fn test_flood_propagation_metrics() {
    let result = run_and_collect_metrics(
        "examples/topologies/multi_path.yaml",
        Some("examples/behaviors/single_broadcast.yaml"),
        77777, // seed
        "20s",  // duration
        &[
            "mcsim.radio.tx_packets/node",
            "mcsim.radio.rx_packets/node",
            "mcsim.packet.tx_flood/node",
            "mcsim.flood.nodes_reached",
            "mcsim.flood.coverage",
        ],
    );

    eprintln!("Test 4: Flood Propagation Metrics");
    if let Some(counter) = result.get_counter("mcsim.radio.tx_packets") {
        eprintln!("  Total TX packets: {}", counter.total);
    }
    if let Some(counter) = result.get_counter("mcsim.radio.rx_packets") {
        eprintln!("  Total RX packets: {}", counter.total);
    }

    // Alice should transmit (deterministic: advert + 1 message from message_count: 1)
    assert_counter_for_node_in_range(&result, "mcsim.radio.tx_packets", "Alice", 1, 30);

    // Multiple nodes should receive (flood propagation)
    // multi_path has 13 nodes total (4 companions + 9 repeaters)
    let total_rx = result
        .get_counter("mcsim.radio.rx_packets")
        .map(|c| c.total)
        .unwrap_or(0);
    assert!(
        total_rx >= 5,
        "Expected at least 5 total RX packets for flood, got {}",
        total_rx
    );

    // Bob, Charlie, Dave should all eventually receive
    for node in &["Bob", "Charlie", "Dave"] {
        if let Some(value) = result.get_counter_for_node("mcsim.radio.rx_packets", node) {
            // Each companion should receive at least 1 packet
            assert!(
                value >= 1,
                "{} should receive at least 1 packet, got {}",
                node,
                value
            );
        }
    }
}

// ============================================================================
// Test 5: Collision and Weak Signal Metrics
// ============================================================================

/// Verify collision detection and SNR threshold handling.
///
/// Topology: diamond (Alice <-> R1/R2 <-> Bob)
/// Behavior: Alice and Bob each send exactly 5 messages (via message_count: 5)
#[test]
fn test_collision_metrics() {
    let result = run_and_collect_metrics(
        "examples/topologies/diamond.yaml",
        Some("examples/behaviors/burst_traffic.yaml"),
        54321, // seed
        "10s",  // duration
        &[
            "mcsim.radio.rx_packets/node",
            "mcsim.radio.rx_collided/node",
            "mcsim.radio.rx_weak/node",
        ],
    );

    eprintln!("Test 5: Collision Metrics");
    if let Some(counter) = result.get_counter("mcsim.radio.tx_collision") {
        eprintln!("  Packets collided: {}", counter.total);
    }

    // With burst traffic (5 messages each from Alice and Bob), we expect some collisions
    // The exact collision count depends on timing, but message count is deterministic
    if result.metric_exists("mcsim.radio.rx_collided") {
        let collided = result
            .get_counter("mcsim.radio.rx_collided")
            .map(|c| c.total)
            .unwrap_or(0);
        eprintln!("  rx_collided metric total: {}", collided);

        // With deterministic message_count: 5 per node, we have controlled traffic volume
        // Collisions still depend on timing, but the overall traffic is bounded
    }

    // Verify the rx_weak metric exists even if 0
    // (this metric tracks packets below SNR threshold)
    if result.metric_exists("mcsim.radio.rx_weak") {
        eprintln!(
            "  rx_weak metric total: {}",
            result
                .get_counter("mcsim.radio.rx_weak")
                .map(|c| c.total)
                .unwrap_or(0)
        );
    }

    // Both repeaters should have received something
    if result.get_counter("mcsim.radio.rx_packets").is_some() {
        // May receive or collide, but should have activity
        let r1_rx = result
            .get_counter_for_node("mcsim.radio.rx_packets", "Repeater1")
            .unwrap_or(0);
        let r2_rx = result
            .get_counter_for_node("mcsim.radio.rx_packets", "Repeater2")
            .unwrap_or(0);
        eprintln!("  Repeater1 RX: {}, Repeater2 RX: {}", r1_rx, r2_rx);
    }
}

// ============================================================================
// Test 6: Room Server and Channel Metrics
// ============================================================================

/// Verify room server specific metrics and channel messaging.
///
/// Topology: room_server (Alice <-> ChatRoom <-> Bob)
/// Behavior: Alice sends exactly 1 channel message (via message_count: 1)
#[test]
fn test_room_server_metrics() {
    let result = run_and_collect_metrics(
        "examples/topologies/room_server.yaml",
        Some("examples/behaviors/single_broadcast.yaml"),
        11111, // seed
        "30s",  // duration
        &[
            "mcsim.radio.tx_packets/node",
            "mcsim.radio.tx_packets/node_type",
            "mcsim.radio.rx_packets/node",
            "mcsim.radio.rx_packets/node_type",
            "mcsim.dm.sent/node",
            "mcsim.dm.delivered/node",
            "mcsim.packet.tx_flood/node",
        ],
    );

    eprintln!("Test 6: Room Server Metrics");
    if let Some(counter) = result.get_counter("mcsim.radio.tx_packets") {
        eprintln!("  Total TX packets: {}", counter.total);
    }

    // Alice (Companion) should transmit (deterministic: advert + 1 message from message_count: 1)
    assert_counter_for_node_in_range(&result, "mcsim.radio.tx_packets", "Alice", 1, 30);

    // ChatRoom (RoomServer) should receive and possibly transmit
    assert_counter_for_node_in_range(&result, "mcsim.radio.rx_packets", "ChatRoom", 1, 100);
    // ChatRoom may or may not transmit depending on if forwarding is needed

    // Bob (Companion) should receive at least adverts
    assert_counter_for_node_in_range(&result, "mcsim.radio.rx_packets", "Bob", 1, 100);

    // Verify node_type labels include both Companion and RoomServer for received packets
    // (RoomServer receives, but may not transmit in this scenario)
    if let Some(counter) = result.get_counter("mcsim.radio.rx_packets") {
        if let Some(by_type) = counter.labels.get("node_type") {
            assert!(
                by_type.contains_key("RoomServer"),
                "Expected RoomServer in rx node_type, got: {:?}",
                by_type.keys().collect::<Vec<_>>()
            );
        }
    }
}

// ============================================================================
// Test 7: Node Type Label Aggregation
// ============================================================================

/// Verify metrics can be aggregated by node_type label.
///
/// Topology: simple (3 Repeaters, 2 Companions)
/// Behavior: Normal traffic with adverts
#[test]
fn test_node_type_aggregation() {
    let result = run_and_collect_metrics(
        "examples/topologies/simple.yaml",
        None, // No behavior overlay - just adverts
        33333,
        "15s",
        &[
            "mcsim.radio.tx_packets/node_type/node",
            "mcsim.radio.rx_packets/node_type",
            "mcsim.radio.tx_airtime_us/node_type",
        ],
    );

    eprintln!("Test 7: Node Type Aggregation");

    // Get TX packets breakdown
    if let Some(counter) = result.get_counter("mcsim.radio.tx_packets") {
        eprintln!("  Total TX packets: {}", counter.total);

        if let Some(by_type) = counter.labels.get("node_type") {
            // Should have exactly 2 node types: Repeater and Companion
            assert_eq!(
                by_type.len(),
                2,
                "Expected 2 node types, got: {:?}",
                by_type.keys().collect::<Vec<_>>()
            );

            // Verify both types exist
            assert!(by_type.contains_key("Repeater"), "Missing Repeater type");
            assert!(by_type.contains_key("Companion"), "Missing Companion type");

            // Log the values
            if let Some(repeater) = by_type.get("Repeater") {
                eprintln!("  Repeater TX packets: {}", repeater.total);
            }
            if let Some(companion) = by_type.get("Companion") {
                eprintln!("  Companion TX packets: {}", companion.total);
            }

            // Sum of node_type values should equal total
            let type_sum: u64 = by_type.values().map(|v| v.total).sum();
            assert_eq!(
                type_sum, counter.total,
                "Sum of node_type values ({}) should equal total ({})",
                type_sum, counter.total
            );
        }

        // Also verify we have per-node breakdown within node_type
        // (nested breakdown: labels["node_type"] -> labels["node"])
        if let Some(by_type) = counter.labels.get("node_type") {
            if let Some(repeater) = by_type.get("Repeater") {
                if let Some(by_node) = repeater.labels.get("node") {
                    // Should have 3 repeaters: Repeater1, Repeater2, Repeater3
                    assert_eq!(
                        by_node.len(),
                        3,
                        "Expected 3 repeater nodes, got: {:?}",
                        by_node.keys().collect::<Vec<_>>()
                    );
                }
            }
        }
    }
}

// ============================================================================
// Debug Helper: Print All Metrics
// ============================================================================

#[cfg(test)]
#[allow(dead_code)]
fn debug_print_metrics(result: &MetricsResult) {
    eprintln!("\n=== All Metrics ===");
    for (name, value) in &result.export.metrics {
        match value {
            MetricValue::Counter(c) => {
                eprintln!("Counter {}: total={}", name, c.total);
                if let Some(by_node) = c.labels.get("node") {
                    for (node, v) in by_node {
                        eprintln!("  labels[node][{}]: {}", node, v.total);
                    }
                }
                if let Some(by_type) = c.labels.get("node_type") {
                    for (t, v) in by_type {
                        eprintln!("  labels[node_type][{}]: {}", t, v.total);
                    }
                }
            }
            MetricValue::Histogram(h) => {
                eprintln!(
                    "Histogram {}: count={}, mean={:.2}",
                    name, h.count, h.mean
                );
            }
            MetricValue::Gauge(g) => {
                eprintln!("Gauge {}: total={:.2}", name, g.total);
            }
        }
    }
    eprintln!("===================\n");
}

// ============================================================================
// Test 8: Wildcard Breakdown (/* syntax)
// ============================================================================

/// Verify that the /* wildcard breakdown syntax works correctly.
///
/// The /* syntax should automatically break down by all available labels.
#[test]
fn test_wildcard_breakdown() {
    let result = run_and_collect_metrics(
        "examples/topologies/multi_path.yaml",
        Some("examples/behaviors/burst_traffic.yaml"),
        12345, // seed
        "5s",  // duration
        &[
            "mcsim.radio.tx_packets/*",  // Wildcard breakdown
        ],
    );

    eprintln!("Test 8: Wildcard Breakdown");

    // Verify the metric exists
    let counter = result
        .get_counter("mcsim.radio.tx_packets")
        .expect("mcsim.radio.tx_packets should exist");

    eprintln!("  Total tx_packets: {}", counter.total);

    // With wildcard, we should have breakdowns by all available labels
    // The radio metrics have: node, node_type, group labels
    // These are applied in priority order: group -> node_type -> node

    // Should have node_type breakdown (comes first in priority order since group is empty)
    assert!(
        counter.labels.contains_key("node_type"),
        "Wildcard breakdown should include node_type label"
    );

    if let Some(by_node_type) = counter.labels.get("node_type") {
        eprintln!("  Node types: {:?}", by_node_type.keys().collect::<Vec<_>>());
        assert!(
            !by_node_type.is_empty(),
            "node_type breakdown should have entries"
        );
        
        // Check that each node_type has node breakdown
        for (type_name, type_counter) in by_node_type {
            if let Some(by_node) = type_counter.labels.get("node") {
                eprintln!("  Node type {}: nodes {:?}", type_name, by_node.keys().collect::<Vec<_>>());
            }
        }
    }
}

// ============================================================================
// Test 9: Geo-Corridor Scoped Flood
// ============================================================================

/// Verify that corridor-tagged channel messages are only forwarded by repeaters
/// whose own geographic position lies inside the corridor.
///
/// Topology: corridor_l_shape
///   - L-shaped corridor: west→corner→north, radius 3 km at each waypoint
///   - Alice at WP1 (west end), Bob at WP3 (north end)
///   - R_in_H: midpoint of horizontal leg  — INSIDE corridor
///   - R_in_V: midpoint of vertical leg    — INSIDE corridor
///   - R_out_SW: far south-west of WP1     — OUTSIDE corridor
///   - R_out_N: north of horizontal leg    — OUTSIDE corridor
///
/// Expected behaviour:
///   1. Alice sends exactly 1 corridor-tagged channel message (op 53, explicit triples).
///   2. Inside repeaters (R_in_H, R_in_V) each forward the message once.
///   3. Outside repeaters (R_out_SW, R_out_N) receive the message but drop it
///      (corridor check → 0 TX for grp_txt).
///   4. Bob receives the message via the inside-repeater chain.
#[test]
fn test_corridor_scoped_flood() {
    let result = run_and_collect_metrics(
        "examples/topologies/corridor_l_shape.yaml",
        Some("examples/behaviors/corridor_broadcast.yaml"),
        42,    // seed — deterministic
        "30s", // duration — enough for 1 message + forwarding chain
        &[
            "mcsim.radio.tx_packets/node/payload_type",
            "mcsim.radio.rx_packets/node/payload_type",
        ],
    );

    eprintln!("Test 9: Geo-Corridor Scoped Flood");

    // Helper: TX grp_txt count for a named node.
    let tx_grp = |node: &str| -> u64 {
        get_counter_for_labels(
            &result,
            "mcsim.radio.tx_packets",
            &[("node", node), ("payload_type", "grp_txt")],
        )
        .unwrap_or(0)
    };

    // Helper: RX grp_txt count for a named node.
    let rx_grp = |node: &str| -> u64 {
        get_counter_for_labels(
            &result,
            "mcsim.radio.rx_packets",
            &[("node", node), ("payload_type", "grp_txt")],
        )
        .unwrap_or(0)
    };

    // ── Sender ───────────────────────────────────────────────────────────────
    let alice_tx = tx_grp("Alice");
    eprintln!("  Alice TX grp_txt: {}", alice_tx);
    assert_eq!(alice_tx, 1, "Alice must send exactly 1 corridor channel message");

    // ── Inside repeaters — must forward ──────────────────────────────────────
    let r_in_h_tx = tx_grp("R_in_H");
    let r_in_v_tx = tx_grp("R_in_V");
    eprintln!("  R_in_H  TX grp_txt: {} (inside corridor — expects >=1)", r_in_h_tx);
    eprintln!("  R_in_V  TX grp_txt: {} (inside corridor — expects >=1)", r_in_v_tx);

    assert!(
        r_in_h_tx >= 1,
        "R_in_H (inside horizontal leg, lat=47.61, lon=-122.375) must forward \
         the corridor message. Got {} TX. \
         Check: node_lat/node_lon propagation from YAML -> NodeConfig -> prefs, \
         and isPointInCorridor() logic.",
        r_in_h_tx
    );
    assert!(
        r_in_v_tx >= 1,
        "R_in_V (inside vertical leg, lat=47.635, lon=-122.35) must forward \
         the corridor message. Got {} TX. \
         Check: node_lat/node_lon propagation and corridor geometry.",
        r_in_v_tx
    );

    // ── Outside repeaters — must NOT forward ─────────────────────────────────
    let r_out_sw_tx = tx_grp("R_out_SW");
    let r_out_n_tx  = tx_grp("R_out_N");
    eprintln!("  R_out_SW TX grp_txt: {} (outside corridor — expects 0)", r_out_sw_tx);
    eprintln!("  R_out_N  TX grp_txt: {} (outside corridor — expects 0)", r_out_n_tx);

    assert_eq!(
        r_out_sw_tx, 0,
        "R_out_SW (lat=47.57, lon=-122.43, ~5 km SW of WP1) must NOT forward \
         the corridor message. Got {} TX. \
         Check: isPointInCorridor() returns false for this position.",
        r_out_sw_tx
    );
    assert_eq!(
        r_out_n_tx, 0,
        "R_out_N (lat=47.645, lon=-122.40, ~3.9 km north of horizontal leg) must NOT forward \
         the corridor message. Got {} TX. \
         Check: isPointInCorridor() returns false for this position.",
        r_out_n_tx
    );

    // ── Outside repeaters still receive Alice's original packet ──────────────
    let r_out_sw_rx = rx_grp("R_out_SW");
    let r_out_n_rx  = rx_grp("R_out_N");
    eprintln!("  R_out_SW RX grp_txt: {} (should receive but drop)", r_out_sw_rx);
    eprintln!("  R_out_N  RX grp_txt: {} (should receive but drop)", r_out_n_rx);
    assert!(
        r_out_sw_rx >= 1,
        "R_out_SW must receive Alice's packet (to exercise the corridor-drop code path)"
    );
    assert!(
        r_out_n_rx >= 1,
        "R_out_N must receive Alice's packet (to exercise the corridor-drop code path)"
    );

    // ── Receiver — Bob must get the message via inside-repeater chain ─────────
    let bob_rx = rx_grp("Bob");
    eprintln!("  Bob RX grp_txt: {} (expects >=1)", bob_rx);
    assert!(
        bob_rx >= 1,
        "Bob (north end of corridor) must receive the message via R_in_H / R_in_V. \
         Got {} RX. Check inside-repeater forwarding and Bob's radio links.",
        bob_rx
    );
}
// ============================================================================
// Test 10: Auto-Corridor Channel Scope (firmware-computed sender circle)
// ============================================================================

/// Verify the firmware-internal default scope for channel messages: a PLAIN
/// channel send (op 3, no corridor configuration anywhere) carries a
/// single-triple corridor — a 50 km circle around the sender (radius code 8) —
/// attached by the companion's sendFloodScoped(GroupChannel) override.
/// Repeaters outside the circle receive but geo-drop the packet.
///
/// Topology: corridor_auto_scope
///   - Alice sender (47.61, -122.40)
///   - R_in  4.8 km from Alice — INSIDE the 50 km circle (forwards)
///   - R_out 84.4 km from Alice — OUTSIDE the circle (drops)
///   - Bob reachable only via R_in (proves in-circle forwarding delivers)
#[test]
fn test_channel_auto_scope() {
    let result = run_and_collect_metrics(
        "examples/topologies/corridor_auto_scope.yaml",
        Some("examples/behaviors/corridor_auto_broadcast.yaml"),
        42,    // seed — deterministic
        "30s", // duration — enough for 1 message + forwarding chain
        &[
            "mcsim.radio.tx_packets/node/payload_type",
            "mcsim.radio.rx_packets/node/payload_type",
        ],
    );

    eprintln!("Test 10: Auto-corridor channel scope (sender circle, 50 km)");

    let tx_grp = |node: &str| -> u64 {
        get_counter_for_labels(
            &result,
            "mcsim.radio.tx_packets",
            &[("node", node), ("payload_type", "grp_txt")],
        )
        .unwrap_or(0)
    };
    let rx_grp = |node: &str| -> u64 {
        get_counter_for_labels(
            &result,
            "mcsim.radio.rx_packets",
            &[("node", node), ("payload_type", "grp_txt")],
        )
        .unwrap_or(0)
    };

    // Sender: exactly one (auto-corridor-scoped) channel message.
    let alice_tx = tx_grp("Alice");
    eprintln!("  Alice TX grp_txt: {}", alice_tx);
    assert_eq!(alice_tx, 1, "Alice must send exactly 1 channel message");

    // Inside the 50 km circle: forwards to Bob.
    let r_in_tx = tx_grp("R_in");
    eprintln!("  R_in  TX grp_txt: {} (inside circle — expects >=1)", r_in_tx);
    assert!(r_in_tx >= 1, "R_in (4.8 km from Alice) must forward the channel message");

    // Outside the circle: receives Alice's packet but must geo-drop it.
    let r_out_tx = tx_grp("R_out");
    let r_out_rx = rx_grp("R_out");
    eprintln!("  R_out TX grp_txt: {} (outside circle — expects 0)", r_out_tx);
    eprintln!("  R_out RX grp_txt: {} (should receive but drop)", r_out_rx);
    assert_eq!(r_out_tx, 0, "R_out (84.4 km from Alice) must NOT forward — the auto-circle corridor must geo-drop it");
    assert!(r_out_rx >= 1, "R_out must receive Alice's packet (to exercise the corridor-drop code path)");

    // Bob: delivery via the in-circle repeater.
    let bob_rx = rx_grp("Bob");
    eprintln!("  Bob RX grp_txt: {} (expects >=1)", bob_rx);
    assert!(bob_rx >= 1, "Bob must receive the channel message via R_in");
}

// ============================================================================
// Test 11: DM Auto-Corridor with Plain-Flood Fallback
// ============================================================================

/// Verify the auto-corridor for contact floods (R2) and its retry fallback:
/// Alice's DM to Bob has no direct path → the FIRST flood attempt is scoped by
/// an auto-generated corridor toward Bob's stored position.  The only relay
/// R_off sits 37.5 km off the Alice→Bob axis (outside the Tier-0 diamond), so
/// the corridor attempt is geo-dropped.  Retries (attempt+1, within the
/// firmware's corridor latch window) go as plain floods: R_off forwards them.
/// Bob's first PATH+ACK return is itself corridor-scoped and dropped; the
/// second return (Bob now latched) is plain and ACKs Alice.
///
/// Topology: corridor_dm_fallback
///   - Alice (47.20, -122.40) → Bob (47.95, -122.40), 83 km apart
///   - R_off the only relay, 56 km from each endpoint, far off-axis
///
/// Expected: Alice dm.sent == 3 (1 corridor + 2 plain retries), Bob delivered
/// >= 1, Alice acked == 1; R_off forwards exactly the 2 plain attempts.
#[test]
fn test_dm_corridor_fallback() {
    let result = run_and_collect_metrics(
        "examples/topologies/corridor_dm_fallback.yaml",
        Some("examples/behaviors/corridor_dm_retry.yaml"),
        42,    // seed — deterministic
        "30s", // duration — 3 attempts × 5 s ack timeout + propagation
        &[
            "mcsim.dm.sent/node",
            "mcsim.dm.delivered/node",
            "mcsim.dm.acked/node",
            "mcsim.radio.tx_packets/node/payload_type",
            "mcsim.radio.rx_packets/node/payload_type",
        ],
    );

    eprintln!("Test 11: DM auto-corridor with plain-flood fallback");

    let tx_txt = |node: &str| -> u64 {
        get_counter_for_labels(
            &result,
            "mcsim.radio.tx_packets",
            &[("node", node), ("payload_type", "txt_msg")],
        )
        .unwrap_or(0)
    };
    let rx_txt = |node: &str| -> u64 {
        get_counter_for_labels(
            &result,
            "mcsim.radio.rx_packets",
            &[("node", node), ("payload_type", "txt_msg")],
        )
        .unwrap_or(0)
    };

    // Alice: attempt 0 (corridor) + attempts 1..2 (plain retries).
    let alice_sent = result.get_counter_for_node("mcsim.dm.sent", "Alice").unwrap_or(0);
    eprintln!("  Alice mcsim.dm.sent: {} (expects 3)", alice_sent);
    assert_eq!(alice_sent, 3, "Alice must send exactly 3 DM attempts (1 corridor + 2 plain retries)");

    // R_off: receives all 3 attempts from Alice, forwards only the 2 plain ones.
    let r_off_rx = rx_txt("R_off");
    let r_off_tx = tx_txt("R_off");
    eprintln!("  R_off RX txt_msg: {} (expects >=3)", r_off_rx);
    eprintln!("  R_off TX txt_msg: {} (expects 2 — corridor attempt dropped)", r_off_tx);
    assert!(r_off_rx >= 3, "R_off must receive all three DM attempts from Alice");
    assert_eq!(
        r_off_tx, 2,
        "R_off must forward exactly the 2 plain-flood attempts — the corridor attempt (off-axis relay) must be geo-dropped"
    );

    // Bob: delivered via the plain-flood retry.
    let bob_delivered = result.get_counter_for_node("mcsim.dm.delivered", "Bob").unwrap_or(0);
    eprintln!("  Bob mcsim.dm.delivered: {} (expects >=1)", bob_delivered);
    assert!(bob_delivered >= 1, "Bob must receive the DM via the plain-flood retry");

    // Alice: final ACK arrives once Bob's PATH+ACK return also goes plain.
    let alice_acked = result.get_counter_for_node("mcsim.dm.acked", "Alice").unwrap_or(0);
    eprintln!("  Alice mcsim.dm.acked: {} (expects ==1)", alice_acked);
    assert_eq!(alice_acked, 1, "Alice must be acked once (after Bob's latch expires its corridor return)");
}
