//! Trace output integration tests for the MCSim simulation framework.
//!
//! These tests verify that the --output trace functionality correctly
//! captures packet transmission and reception events with full packet data.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

use serde::Deserialize;

// ============================================================================
// JSON Deserialization Types for Trace Output
// ============================================================================

/// A trace entry from the output file.
/// 
/// Due to the flattened tagged enum structure, we deserialize into a more
/// permissive structure where optional fields may or may not be present
/// depending on the event type.
#[derive(Debug, Deserialize, Clone)]
struct TraceEntry {
    origin: String,
    origin_id: String,
    timestamp: String,
    /// Event type: "PACKET", "TIMER", "MESSAGE"
    #[serde(rename = "type")]
    entry_type: String,
    /// Direction (TX/RX) - present on PACKET and MESSAGE events
    #[serde(default)]
    direction: Option<String>,
    /// Signal-to-noise ratio - present on RX PACKET events
    #[serde(rename = "SNR", default)]
    snr: Option<String>,
    /// Signal strength or TX power - present on PACKET events
    #[serde(rename = "RSSI", default)]
    rssi: Option<String>,
    /// Payload hash (16-char hex) - present on PACKET events
    #[serde(default)]
    payload_hash: Option<String>,
    /// Timer ID - present on TIMER events
    #[serde(default)]
    timer_id: Option<u64>,
    /// Raw packet payload (hex-encoded) - present on PACKET events
    #[serde(default)]
    packet_hex: Option<String>,
    /// Decoded packet data - present on PACKET events
    #[serde(default)]
    packet: Option<serde_json::Value>,
    /// Reception status - present on RX PACKET events
    #[serde(default)]
    reception_status: Option<String>,
    /// Packet start time in seconds - present on PACKET events
    #[serde(default)]
    packet_start_time_s: Option<f64>,
    /// Packet end time in seconds - present on PACKET events
    #[serde(default)]
    packet_end_time_s: Option<f64>,
    /// Message destination - present on MESSAGE TX events
    #[serde(default)]
    destination: Option<String>,
}

// ============================================================================
// Test Helper Functions
// ============================================================================

/// Run a simulation and collect trace output.
///
/// # Arguments
/// * `topology` - Path to the topology YAML file
/// * `seed` - Random seed for deterministic simulation
/// * `duration` - Duration string (e.g., "10s", "1m")
///
/// # Returns
/// Vector of trace entries from the output file
fn run_and_collect_trace(
    topology: &str,
    seed: u64,
    duration: &str,
) -> Vec<TraceEntry> {
    // CARGO_BIN_EXE_mcsim is set by cargo when running tests for this crate
    let binary = env!("CARGO_BIN_EXE_mcsim");

    // Create temporary directory for output
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let output_path = temp_dir.path().join("trace.json");

    // Build command - run from workspace root (two levels up from crate)
    let mut cmd = Command::new(binary);
    cmd.current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    cmd.arg("run");
    cmd.arg(topology);
    cmd.arg("--seed");
    cmd.arg(seed.to_string());
    cmd.arg("--duration");
    cmd.arg(duration);
    cmd.arg("--output");
    cmd.arg(&output_path);

    // Run the simulation
    let output = cmd.output().expect("Failed to execute mcsim");

    // Check for successful execution
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        panic!(
            "Simulation failed:\nstdout: {}\nstderr: {}",
            stdout, stderr
        );
    }

    // Read and parse the trace output
    let trace_json = fs::read_to_string(&output_path)
        .expect("Failed to read trace output file");
    
    serde_json::from_str(&trace_json)
        .expect("Failed to parse trace JSON")
}

// ============================================================================
// Integration Tests
// ============================================================================

#[test]
fn test_trace_output_contains_packet_data() {
    // Run a short simulation with the two companions topology
    let trace = run_and_collect_trace(
        "crates/mcsim-runner/tests/two_companions.yaml",
        42,
        "10s",
    );

    // Verify we got some trace entries
    assert!(!trace.is_empty(), "Expected trace entries but got none");

    // Find packet events (TX or RX)
    let packet_events: Vec<_> = trace
        .iter()
        .filter(|e| e.entry_type == "PACKET")
        .collect();

    assert!(
        !packet_events.is_empty(),
        "Expected at least one PACKET event in trace"
    );

    // Verify packet events have the expected fields
    for event in &packet_events {
        // All packet events should have hex payload
        assert!(
            event.packet_hex.is_some(),
            "PACKET event missing packet_hex field: {:?}",
            event
        );

        // All packet events should have payload_hash
        assert!(
            event.payload_hash.is_some(),
            "PACKET event missing payload_hash field: {:?}",
            event
        );

        // Verify payload_hash is valid (16 hex characters or "unknown")
        let hash = event.payload_hash.as_ref().unwrap();
        assert!(
            hash == "unknown" || (hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit())),
            "payload_hash should be 16 hex chars or 'unknown', got: {}",
            hash
        );

        // Verify hex string is valid (even length, hex characters)
        let hex = event.packet_hex.as_ref().unwrap();
        assert!(
            hex.len() % 2 == 0,
            "packet_hex should have even length: {}",
            hex
        );
        assert!(
            hex.chars().all(|c| c.is_ascii_hexdigit()),
            "packet_hex should only contain hex digits: {}",
            hex
        );

        // Most packets should decode successfully (may fail for corrupted/test data)
        // But we won't assert this as it depends on the packet content
    }
}

#[test]
fn test_trace_output_contains_reception_status() {
    // Run a short simulation
    let trace = run_and_collect_trace(
        "crates/mcsim-runner/tests/two_companions.yaml",
        42,
        "10s",
    );

    // Find RX packet events
    let rx_events: Vec<_> = trace
        .iter()
        .filter(|e| e.entry_type == "PACKET" && e.direction.as_deref() == Some("RX"))
        .collect();

    assert!(
        !rx_events.is_empty(),
        "Expected at least one RX PACKET event in trace"
    );

    // Verify RX events have reception status
    for event in &rx_events {
        assert!(
            event.reception_status.is_some(),
            "RX PACKET event missing reception_status: {:?}",
            event
        );

        let status = event.reception_status.as_ref().unwrap();
        assert!(
            status == "ok" || status == "collided" || status == "weak",
            "reception_status should be 'ok', 'collided', or 'weak', got: {}",
            status
        );
    }
}

#[test]
fn test_trace_output_tx_events_have_no_reception_status() {
    // Run a short simulation
    let trace = run_and_collect_trace(
        "crates/mcsim-runner/tests/two_companions.yaml",
        42,
        "10s",
    );

    // Find TX packet events
    let tx_events: Vec<_> = trace
        .iter()
        .filter(|e| e.entry_type == "PACKET" && e.direction.as_deref() == Some("TX"))
        .collect();

    assert!(
        !tx_events.is_empty(),
        "Expected at least one TX PACKET event in trace"
    );

    // Verify TX events do NOT have reception status
    for event in &tx_events {
        assert!(
            event.reception_status.is_none(),
            "TX PACKET event should not have reception_status: {:?}",
            event
        );
    }
}

#[test]
fn test_trace_output_non_packet_events() {
    // Run a short simulation
    let trace = run_and_collect_trace(
        "crates/mcsim-runner/tests/two_companions.yaml",
        42,
        "10s",
    );

    // Find non-packet events (MESSAGE, TIMER, etc.)
    let non_packet_events: Vec<_> = trace
        .iter()
        .filter(|e| e.entry_type != "PACKET")
        .collect();

    // Non-packet events should not have packet data or reception status
    for event in &non_packet_events {
        assert!(
            event.packet_hex.is_none(),
            "Non-packet event should not have packet_hex: {:?}",
            event
        );
        assert!(
            event.packet.is_none(),
            "Non-packet event should not have packet field: {:?}",
            event
        );
        assert!(
            event.reception_status.is_none(),
            "Non-packet event should not have reception_status: {:?}",
            event
        );
    }
}

#[test]
fn test_trace_output_timer_events() {
    // Run a short simulation
    let trace = run_and_collect_trace(
        "crates/mcsim-runner/tests/two_companions.yaml",
        42,
        "10s",
    );

    // Find TIMER events
    let timer_events: Vec<_> = trace
        .iter()
        .filter(|e| e.entry_type == "TIMER")
        .collect();

    assert!(
        !timer_events.is_empty(),
        "Expected at least one TIMER event in trace"
    );

    // Verify TIMER events have timer_id field
    for event in &timer_events {
        assert!(
            event.timer_id.is_some(),
            "TIMER event missing timer_id field: {:?}",
            event
        );
        
        // TIMER events should not have direction, SNR, or RSSI
        assert!(
            event.direction.is_none(),
            "TIMER event should not have direction: {:?}",
            event
        );
        assert!(
            event.snr.is_none(),
            "TIMER event should not have SNR: {:?}",
            event
        );
        assert!(
            event.rssi.is_none(),
            "TIMER event should not have RSSI: {:?}",
            event
        );
    }
}

#[test]
fn test_trace_output_has_node_names() {
    // Run a short simulation
    let trace = run_and_collect_trace(
        "crates/mcsim-runner/tests/two_companions.yaml",
        42,
        "10s",
    );

    // Verify we got some trace entries
    assert!(!trace.is_empty(), "Expected trace entries but got none");

    // Verify that origin contains node names, not Entity_N format
    for event in &trace {
        assert!(
            !event.origin.starts_with("Entity_"),
            "Origin should be node name, not Entity_N format: {}",
            event.origin
        );
    }
    
    // Verify expected node names are present (the two_companions topology uses "Sender" and "Receiver")
    let origins: std::collections::HashSet<_> = trace.iter().map(|e| e.origin.as_str()).collect();
    assert!(
        origins.contains("Sender") || origins.contains("Receiver"),
        "Expected to find Sender or Receiver in trace origins: {:?}",
        origins
    );
}

#[test]
fn test_trace_output_tx_packet_timing() {
    // Run a short simulation
    let trace = run_and_collect_trace(
        "crates/mcsim-runner/tests/two_companions.yaml",
        42,
        "10s",
    );

    // Find TX packet events
    let tx_events: Vec<_> = trace
        .iter()
        .filter(|e| e.entry_type == "PACKET" && e.direction.as_deref() == Some("TX"))
        .collect();

    assert!(
        !tx_events.is_empty(),
        "Expected at least one TX PACKET event in trace"
    );

    // Verify TX events have packet timing information
    for event in &tx_events {
        assert!(
            event.packet_start_time_s.is_some(),
            "TX PACKET event should have packet_start_time_s: {:?}",
            event
        );
        assert!(
            event.packet_end_time_s.is_some(),
            "TX PACKET event should have packet_end_time_s: {:?}",
            event
        );

        // Verify end time is after start time
        let start = event.packet_start_time_s.unwrap();
        let end = event.packet_end_time_s.unwrap();
        assert!(
            end > start,
            "packet_end_time_s ({}) should be after packet_start_time_s ({})",
            end, start
        );
    }

    // Verify RX events also have packet timing information
    let rx_events: Vec<_> = trace
        .iter()
        .filter(|e| e.entry_type == "PACKET" && e.direction.as_deref() == Some("RX"))
        .collect();

    assert!(
        !rx_events.is_empty(),
        "Expected at least one RX PACKET event in trace"
    );

    for event in &rx_events {
        assert!(
            event.packet_start_time_s.is_some(),
            "RX PACKET event should have packet_start_time_s: {:?}",
            event
        );
        assert!(
            event.packet_end_time_s.is_some(),
            "RX PACKET event should have packet_end_time_s: {:?}",
            event
        );

        // Verify end time is after start time
        let start = event.packet_start_time_s.unwrap();
        let end = event.packet_end_time_s.unwrap();
        assert!(
            end > start,
            "packet_end_time_s ({}) should be after packet_start_time_s ({})",
            end, start
        );
    }
}
