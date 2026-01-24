//! Integration tests for the MCSim simulation framework.
//!
//! These tests verify that the various crates work together correctly.

use std::path::Path;

use mcsim_common::{EntityId, Event, EventId, EventPayload, GeoCoord, SimTime};
use mcsim_lora::{calculate_time_on_air, RadioParams};
use mcsim_model::{load_model, load_model_from_str, ModelLoader, AGENT_DIRECT_ENABLED, AGENT_CHANNEL_ENABLED, RADIO_SPREADING_FACTOR, properties::{FIRMWARE_TYPE, LOCATION_ALTITUDE_M}};

// ============================================================================
// Model Loading Integration Tests
// ============================================================================

#[test]
fn test_load_example_model_from_file() {
    // Integration tests run from the crate directory, so need to go up to workspace
    let path = Path::new("../../examples/topologies/simple.yaml");
    let model = load_model(path).expect("Failed to load example model");

    // Verify basic structure
    assert_eq!(model.nodes().len(), 5, "Expected 5 nodes in example model");

    // Count node types by firmware_type property
    let repeaters = model
        .nodes()
        .iter()
        .filter(|(_, n)| {
            let fw_type: String = n.properties().get(&FIRMWARE_TYPE);
            fw_type.to_lowercase() == "repeater"
        })
        .count();
    let companions = model
        .nodes()
        .iter()
        .filter(|(_, n)| {
            let fw_type: String = n.properties().get(&FIRMWARE_TYPE);
            fw_type.to_lowercase() == "companion"
        })
        .count();

    assert_eq!(repeaters, 3, "Expected 3 repeaters");
    assert_eq!(companions, 2, "Expected 2 companions");

    // Verify edges
    assert_eq!(model.edges().len(), 8, "Expected 8 edges");

    // Verify bidirectional links exist
    let has_alice_to_r1 = model
        .edges()
        .iter()
        .any(|(_, e)| e.from == "Alice" && e.to == "Repeater1");
    let has_r1_to_alice = model
        .edges()
        .iter()
        .any(|(_, e)| e.from == "Repeater1" && e.to == "Alice");
    assert!(
        has_alice_to_r1 && has_r1_to_alice,
        "Expected bidirectional link between Alice and Repeater1"
    );
}

#[test]
fn test_build_simulation_from_model() {
    let path = Path::new("../../examples/topologies/simple.yaml");
    let model = load_model(path).expect("Failed to load example model");

    let seed = 12345u64;
    let simulation =
        ModelLoader::build_simulation(&model, seed).expect("Failed to build simulation");

    // The built simulation should have entities (radios, firmwares, agents)
    // Each node creates: 1 radio + 1 firmware, plus optional agent
    // simple.yaml has 5 nodes (3 repeaters + 2 companions)
    assert!(simulation.entities.len() > 0, "Expected entities to be created");
}

// ============================================================================
// LoRa PHY Integration Tests
// ============================================================================

#[test]
fn test_lora_time_on_air_calculation() {
    let params = RadioParams {
        frequency_hz: 906_000_000,
        bandwidth_hz: 250_000,
        spreading_factor: 11,
        coding_rate: 5,
        tx_power_dbm: 20,
    };

    // Test various packet sizes
    let toa_empty = calculate_time_on_air(&params, 0);
    let toa_small = calculate_time_on_air(&params, 20);
    let toa_medium = calculate_time_on_air(&params, 100);
    let toa_max = calculate_time_on_air(&params, 255);

    // Time should increase with packet size
    assert!(toa_small > toa_empty);
    assert!(toa_medium > toa_small);
    assert!(toa_max > toa_medium);

    // SF11, 250kHz should be reasonable values (hundreds of ms for larger packets)
    assert!(toa_small.as_secs_f64() > 0.01);
    assert!(toa_max.as_secs_f64() < 10.0);
}

#[test]
fn test_lora_snr_sensitivity() {
    let sf = 11;

    // Get the sensitivity threshold using the library function
    let sensitivity = mcsim_lora::calculate_snr_sensitivity(sf);

    // SF11 should have sensitivity around -17 to -18 dB
    assert!(sensitivity < -10.0);
    assert!(sensitivity > -25.0);

    // Test that SF7 has a higher (less sensitive) threshold than SF12
    let sf7_sensitivity = mcsim_lora::calculate_snr_sensitivity(7);
    let sf12_sensitivity = mcsim_lora::calculate_snr_sensitivity(12);
    assert!(sf7_sensitivity > sf12_sensitivity, 
        "Higher SF should have lower (more sensitive) threshold");
}

#[test]
fn test_lora_collision_detection() {
    use mcsim_lora::{check_collision, CollisionContext, CollisionResult};

    // Two packets arriving at the same time with overlap
    let incoming = CollisionContext {
        start_time: SimTime::from_millis(100),
        end_time: SimTime::from_millis(600),
        frequency_hz: 906_000_000,
        packet_id: 2,
    };

    let existing = vec![CollisionContext {
        start_time: SimTime::from_millis(0),
        end_time: SimTime::from_millis(500),
        frequency_hz: 906_000_000,
        packet_id: 1,
    }];

    let result = check_collision(&incoming, &existing);
    assert!(
        matches!(result, CollisionResult::BothDestroyed(_)),
        "Overlapping packets should collide"
    );
}

#[test]
fn test_lora_no_collision() {
    use mcsim_lora::{check_collision, CollisionContext, CollisionResult};

    // Two packets not overlapping
    let incoming = CollisionContext {
        start_time: SimTime::from_millis(1000),
        end_time: SimTime::from_millis(1500),
        frequency_hz: 906_000_000,
        packet_id: 2,
    };

    let existing = vec![CollisionContext {
        start_time: SimTime::from_millis(0),
        end_time: SimTime::from_millis(500),
        frequency_hz: 906_000_000,
        packet_id: 1,
    }];

    let result = check_collision(&incoming, &existing);
    assert_eq!(result, CollisionResult::NoCollision);
}

// ============================================================================
// Packet Encoding Integration Tests
// ============================================================================

#[test]
fn test_packet_encode_decode_roundtrip() {
    use meshcore_packet::{AdvertPayload, MeshCorePacket, PayloadType};

    // Create an advertisement packet
    let advert = AdvertPayload::new([0xAA; 32], 1234567890, [0xBB; 64], "TestNode");
    let packet = MeshCorePacket::advert(advert);

    // Encode
    let encoded = packet.encode();
    assert!(!encoded.is_empty());

    // Decode
    let decoded = MeshCorePacket::decode(&encoded).expect("Failed to decode packet");

    // Verify
    assert_eq!(decoded.header.payload_type, PayloadType::Advert);
    assert!(decoded.header.route_type.is_flood());
}

#[test]
fn test_text_message_roundtrip() {
    use meshcore_packet::{
        EncryptedHeader, MeshCorePacket, PacketPayload, PayloadType, RouteType,
        TextMessagePayload,
    };

    // Create a text message packet
    // In MeshCore, text messages have an encrypted header (dest_hash, src_hash, mac)
    // and ciphertext
    let header = EncryptedHeader {
        dest_hash: 0xAB,
        src_hash: 0xCD,
        mac: 0x1234,
    };
    let ciphertext = b"Encrypted message data".to_vec();

    let text_payload = TextMessagePayload {
        header,
        ciphertext,
    };

    let packet = MeshCorePacket::new(RouteType::Direct, PacketPayload::TextMessage(text_payload));

    // Encode and decode
    let encoded = packet.encode();
    let decoded = MeshCorePacket::decode(&encoded).expect("Failed to decode packet");

    // Verify header
    assert_eq!(decoded.header.payload_type, PayloadType::TextMessage);
    assert_eq!(decoded.header.route_type, RouteType::Direct);

    // Verify payload
    if let PacketPayload::TextMessage(text) = &decoded.payload {
        assert_eq!(text.header.dest_hash, 0xAB);
        assert_eq!(text.header.src_hash, 0xCD);
    } else {
        panic!("Expected TextMessage payload");
    }
}

#[test]
fn test_packet_crypto_functions() {
    use meshcore_packet::crypto::{
        decrypt_data, derive_key_from_passphrase, encrypt_data, generate_random_key,
    };

    // Test encryption/decryption roundtrip
    let key = generate_random_key();
    let plaintext = b"Secret message";

    let (nonce, ciphertext) = encrypt_data(plaintext, &key).expect("Failed to encrypt");
    let decrypted = decrypt_data(&ciphertext, &key, &nonce).expect("Failed to decrypt");

    assert_eq!(decrypted, plaintext);

    // Test key derivation is deterministic
    let key1 = derive_key_from_passphrase("test_password");
    let key2 = derive_key_from_passphrase("test_password");
    assert_eq!(key1.0, key2.0);
}

// ============================================================================
// Simulation Time Tests
// ============================================================================

#[test]
fn test_simulation_time_ordering() {
    let t1 = SimTime::from_secs(10.0);
    let t2 = SimTime::from_secs(20.0);
    let t3 = SimTime::from_millis(10500);

    assert!(t1 < t2);
    assert!(t1 < t3);
    assert!(t3 < t2);

    // Test arithmetic
    let t4 = t1 + SimTime::from_secs(5.0);
    assert_eq!(t4.as_millis(), 15000);
}

#[test]
fn test_simulation_time_conversions() {
    let time = SimTime::from_secs(1.5);
    assert_eq!(time.as_millis(), 1500);
    assert_eq!(time.as_micros(), 1_500_000);
    assert!((time.as_secs_f64() - 1.5).abs() < 0.0001);
}

// ============================================================================
// Event System Tests
// ============================================================================

#[test]
fn test_event_ordering() {
    // Events should be orderable by time
    let e1 = Event {
        id: EventId(1),
        time: SimTime::from_millis(300),
        source: EntityId(0),
        targets: vec![EntityId(1)],
        payload: EventPayload::SimulationEnd,
    };

    let e2 = Event {
        id: EventId(2),
        time: SimTime::from_millis(100),
        source: EntityId(0),
        targets: vec![EntityId(2)],
        payload: EventPayload::SimulationEnd,
    };

    // Events implement Ord (reverse for min-heap)
    // e2 has earlier time, so it should be "greater" for the min-heap
    assert!(e1 < e2); // In min-heap ordering, earlier time = greater priority
}

// ============================================================================
// Geo Coordinate Tests
// ============================================================================

#[test]
fn test_geo_distance_calculation() {
    // San Francisco to Los Angeles (~560 km)
    let sf = GeoCoord::new(37.7749, -122.4194);
    let la = GeoCoord::new(34.0522, -118.2437);

    let distance = sf.distance_to(&la);
    assert!(
        distance > 500_000.0 && distance < 650_000.0,
        "SF to LA should be ~560 km, got {} m",
        distance
    );

    // Same point should have zero distance
    let same = sf.distance_to(&sf);
    assert!(same < 0.01, "Same point should have ~0 distance");
}

// ============================================================================
// Full Pipeline Test
// ============================================================================

#[test]
fn test_full_model_to_simulation_pipeline() {
    // This test verifies the full pipeline from YAML to simulation entities

    let yaml = r#"
defaults:
  node:
    radio:
      frequency_hz: 915000000
      spreading_factor: 10

nodes:
  - name: "Gateway"
    location: { lat: 37.7749, lon: -122.4194, alt: 50 }
    firmware:
      type: Repeater

  - name: "Mobile"
    location: { lat: 37.7800, lon: -122.4100 }
    firmware:
      type: Companion
    agent:
      channel:
        enabled: true
        targets: ["general"]

edges:
  - from: "Gateway"
    to: "Mobile"
    mean_snr_db_at20dbm: 15.0
    rssi_dbm: -80
  - from: "Mobile"
    to: "Gateway"
    mean_snr_db_at20dbm: 12.0
    rssi_dbm: -85
"#;

    // Parse the model
    let model = load_model_from_str(yaml).expect("Failed to parse model");

    // Verify nodes have correct spreading factor (defaults are applied to nodes)
    // We verify by checking a node's resolved property
    let gateway = model.nodes().get("Gateway").expect("Gateway node should exist");
    let spreading_factor: u8 = gateway.properties().get(&RADIO_SPREADING_FACTOR);
    assert_eq!(spreading_factor, 10);

    // Verify nodes
    assert_eq!(model.nodes().len(), 2);

    // Gateway should have altitude set
    let alt_value: Option<f64> = gateway.properties().get(&LOCATION_ALTITUDE_M);
    assert!(alt_value.is_some(), "Gateway should have altitude");

    let mobile = model.nodes().get("Mobile").expect("Mobile node should exist");
    let mobile_channel_enabled: bool = mobile.properties().get(&AGENT_CHANNEL_ENABLED);
    assert!(mobile_channel_enabled);

    // Verify edges form bidirectional link
    assert_eq!(model.edges().len(), 2);

    // Build simulation
    let seed = 42u64;
    let sim = ModelLoader::build_simulation(&model, seed).expect("Failed to build simulation");

    // Simulation adds initial timer events to start firmware
    assert!(!sim.initial_events.is_empty(), "Expected initial events for firmware startup");
}

#[test]
fn test_link_model() {
    use mcsim_lora::LinkModel;

    let mut link_model = LinkModel::new();

    let radio1 = EntityId(1);
    let radio2 = EntityId(2);
    let radio3 = EntityId(3);

    // Add bidirectional link between 1 and 2 (with default std_dev of 1.8)
    link_model.add_link(radio1, radio2, 10.0, 1.8, -85.0);
    link_model.add_link(radio2, radio1, 8.0, 1.8, -90.0);

    // Add link from 2 to 3
    link_model.add_link(radio2, radio3, 5.0, 1.8, -95.0);

    // Verify links
    let link_1_2 = link_model.get_link(radio1, radio2).unwrap();
    assert_eq!(link_1_2.mean_snr_db_at20dbm, 10.0);
    assert_eq!(link_1_2.rssi_dbm, -85.0);

    let link_2_1 = link_model.get_link(radio2, radio1).unwrap();
    assert_eq!(link_2_1.mean_snr_db_at20dbm, 8.0);

    // No direct link from 1 to 3
    assert!(link_model.get_link(radio1, radio3).is_none());

    // Get all receivers from radio2
    let receivers: Vec<_> = link_model.get_receivers(radio2).collect();
    assert_eq!(receivers.len(), 2); // radio1 and radio3
}

// ============================================================================
// Two Companion Channel Message Test
// ============================================================================

/// Integration test that verifies channel message delivery between two companions.
///
/// This test:
/// 1. Loads a simplified network with two companion nodes (no repeaters)
/// 2. Configures the first node (Sender) to send to #test channel every 5 seconds
/// 3. Configures the second node (Receiver) to receive channel messages
/// 4. Runs simulation for 2 minutes with a known seed
/// 5. Verifies messages are delivered and count is at least 23
#[test]
fn test_two_companion_channel_messages() {
    use mcsim_runner::{create_event_loop, build_simulation, load_model, SimTime, metrics_export};
    use std::sync::Arc;
    
    // Set up metrics recording for this test
    let recorder = Arc::new(metrics_export::InMemoryRecorder::new());
    // Note: set_global_recorder can only be called once per process, so this may fail
    // in subsequent test runs. We continue anyway and check metrics if available.
    if metrics::set_global_recorder(recorder.clone()).is_ok() {
        mcsim_metrics::describe_metrics();
    }
    
    // Load the test configuration
    let path = Path::new("tests/two_companions.yaml");
    let model = load_model(path).expect("Failed to load two_companions.yaml");
    
    // Verify the model structure
    assert_eq!(model.nodes().len(), 2, "Expected 2 nodes (Sender and Receiver)");
    
    // Verify both are companions (no repeaters)
    let companions = model
        .nodes()
        .iter()
        .filter(|(_, n)| {
            let fw_type: String = n.properties().get(&FIRMWARE_TYPE);
            fw_type.to_lowercase() == "companion"
        })
        .count();
    assert_eq!(companions, 2, "Expected 2 companion nodes");
    
    let repeaters = model
        .nodes()
        .iter()
        .filter(|(_, n)| {
            let fw_type: String = n.properties().get(&FIRMWARE_TYPE);
            fw_type.to_lowercase() == "repeater"
        })
        .count();
    assert_eq!(repeaters, 0, "Expected 0 repeaters");
    
    // Verify both have agents
    let agents = model
        .nodes()
        .iter()
        .filter(|(_, n)| {
            let direct_enabled: bool = n.properties().get(&AGENT_DIRECT_ENABLED);
            let channel_enabled: bool = n.properties().get(&AGENT_CHANNEL_ENABLED);
            direct_enabled || channel_enabled
        })
        .count();
    assert_eq!(agents, 2, "Expected 2 agents");
    
    // Use a known seed for deterministic behavior
    let seed = 42u64;
    
    // Build the simulation
    let simulation = build_simulation(&model, seed).expect("Failed to build simulation");
    
    // Create the event loop
    let mut event_loop = create_event_loop(simulation, seed);
    
    // Run for 2 minutes (120 seconds) of simulation time
    let duration = SimTime::from_secs(120.0);
    let stats = event_loop.run(duration).expect("Simulation failed");
    
    // Collect metrics snapshot
    let metrics_snapshot = recorder.snapshot();
    
    // Get message counts from metrics (keyed by "mcsim.dm.sent" and "mcsim.dm.acked")
    let messages_sent = metrics_snapshot.counters.get("mcsim.dm.sent").copied().unwrap_or(0);
    let messages_acked = metrics_snapshot.counters.get("mcsim.dm.acked").copied().unwrap_or(0);
    
    // Log results for debugging
    eprintln!("Simulation completed:");
    eprintln!("  Total events: {}", stats.total_events);
    eprintln!("  Messages sent: {}", messages_sent);
    eprintln!("  Messages acked: {}", messages_acked);
    eprintln!("  Simulation time: {} µs ({:.1} seconds)", 
              stats.simulation_time_us, 
              stats.simulation_time_us as f64 / 1_000_000.0);
    
    // Also show per-node metrics if available
    if !metrics_snapshot.nodes.is_empty() {
        eprintln!("\nPer-node message metrics:");
        for (node, node_metrics) in &metrics_snapshot.nodes {
            let sent = node_metrics.counters.get("mcsim.dm.sent").copied().unwrap_or(0);
            let delivered = node_metrics.counters.get("mcsim.dm.delivered").copied().unwrap_or(0);
            if sent > 0 || delivered > 0 {
                eprintln!("  {}: sent={}, delivered={}", node, sent, delivered);
            }
        }
    }
    
    // ==========================================================================
    // Core assertions for the test
    // ==========================================================================
    
    // With 5 second intervals over 120 seconds, we expect ~24 channel messages.
    // The Sender should send approximately 120/5 = 24 messages.
    // Allow some slack for initialization time, expecting at least 23 messages.
    assert!(
        messages_sent >= 23,
        "Expected at least 23 messages sent (channel messages), got {}. \
         With 5-second intervals over 120 seconds, we expect approximately 24 messages.",
        messages_sent
    );
    
    // Check that channel messages are being delivered (received by Receiver).
    // MESSAGE_DELIVERED tracks messages received by agents.
    let messages_delivered = metrics_snapshot.counters.get("mcsim.dm.delivered").copied().unwrap_or(0);
    eprintln!("Messages delivered (received): {}", messages_delivered);
    
    assert!(
        messages_delivered >= 23,
        "Expected at least 23 messages delivered (channel messages received), got {}. \
         The Receiver should receive all channel messages from Sender.",
        messages_delivered
    );
    
    // Verify no collisions (with only 2 nodes and 5s intervals, should be none)
    assert_eq!(
        stats.packets_collided, 0,
        "Expected no collisions with 2 nodes and 5-second message intervals"
    );
}
