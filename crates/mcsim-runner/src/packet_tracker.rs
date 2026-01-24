//! Packet delivery tracking for network-level metrics.
//!
//! This module provides a [`PacketTracker`] component that observes packet transmissions
//! across the network and tracks:
//! - Flood packet propagation (how many nodes received it, times heard)
//! - Direct message delivery (success/failure, latency, hop count)
//!
//! ## Memory Management
//!
//! For long-running simulations, the tracker supports periodic eviction of old packets
//! via [`PacketTracker::evict_old_packets`]. Packets older than the configured threshold
//! have their metrics emitted before removal.

use std::collections::{HashMap, HashSet};

use meshcore_packet::{MeshCorePacket, PayloadHash};
use mcsim_metrics::{metric_defs, metrics};

/// Tracks the kind and delivery status of a packet.
#[derive(Debug, Clone)]
pub enum PacketKind {
    /// Flood packet - broadcast to all nodes.
    Flood {
        /// Set of node names that received this packet.
        nodes_reached: HashSet<String>,
        /// Reception times for each node (sim_time in microseconds when received).
        reception_times: HashMap<String, u64>,
        /// Time when packet was first transmitted (microseconds).
        origin_time: u64,
        /// Total times this packet was heard by any node.
        times_heard: u32,
        /// Last activity time (microseconds) - updated on each reception.
        last_activity: u64,
    },
    /// Direct packet - targeted to specific destination.
    Direct {
        /// Destination node name.
        destination: String,
        /// Whether the packet was delivered.
        delivered: bool,
        /// Time when delivered in microseconds (if delivered).
        delivery_time: Option<u64>,
        /// Origin time in microseconds.
        origin_time: u64,
        /// Hop count to reach destination.
        hop_count: Option<u32>,
        /// Last activity time (microseconds) - updated on reception or delivery.
        last_activity: u64,
    },
}

/// Tracks packets across the network for delivery metrics.
///
/// # Example
///
/// ```rust,ignore
/// use mcsim_runner::packet_tracker::PacketTracker;
/// use meshcore_packet::MeshCorePacket;
///
/// let mut tracker = PacketTracker::new(10);
///
/// // Decode a packet
/// let packet = MeshCorePacket::decode(&packet_bytes).unwrap();
///
/// // Track the packet being sent
/// tracker.track_send(&packet, None, 1000);
///
/// // Track reception by various nodes
/// tracker.track_reception(&packet, "node_1", 1500, None);
/// tracker.track_reception(&packet, "node_2", 2000, None);
///
/// // Emit summary metrics at end of simulation
/// tracker.emit_flood_summaries();
/// ```
pub struct PacketTracker {
    /// Map from packet hash to tracking info.
    packets: HashMap<PayloadHash, PacketKind>,
    /// Total node count for coverage calculation.
    total_nodes: usize,
}

impl PacketTracker {
    /// Create a new packet tracker.
    ///
    /// # Arguments
    ///
    /// * `total_nodes` - Total number of nodes in the network (for coverage calculation)
    pub fn new(total_nodes: usize) -> Self {
        Self {
            packets: HashMap::new(),
            total_nodes,
        }
    }

    /// Called when a packet is first transmitted.
    ///
    /// # Arguments
    ///
    /// * `packet` - The decoded MeshCore packet being sent
    /// * `destination` - Destination node name (only for direct packets)
    /// * `origin_time` - Simulation time in microseconds when sent
    pub fn track_send(
        &mut self,
        packet: &MeshCorePacket,
        destination: Option<String>,
        origin_time: u64,
    ) {
        let payload_hash = packet.payload_hash_label();
        let is_flood = packet.is_flood();
        let route_type = packet.route_type();
        let payload_type = packet.payload_type();

        let kind = if is_flood {
            PacketKind::Flood {
                nodes_reached: HashSet::new(),
                reception_times: HashMap::new(),
                origin_time,
                times_heard: 0,
                last_activity: origin_time,
            }
        } else {
            PacketKind::Direct {
                destination: destination.unwrap_or_default(),
                delivered: false,
                delivery_time: None,
                origin_time,
                hop_count: None,
                last_activity: origin_time,
            }
        };
        self.packets.insert(payload_hash, kind);

        // Emit send metrics with breakdown labels
        let labels = [
            ("route_type", route_type.as_label().to_string()),
            ("payload_type", payload_type.as_label().to_string()),
            ("payload_hash", payload_hash.as_label()),
        ];
        if is_flood {
            metrics::counter!(metric_defs::PACKET_TX_FLOOD.name, &labels).increment(1);
        } else {
            metrics::counter!(metric_defs::PACKET_TX_DIRECT.name, &labels).increment(1);
            metrics::counter!(metric_defs::DIRECT_SENT.name, &labels).increment(1);
        }
    }

    /// Called when a packet is received by a node.
    ///
    /// # Arguments
    ///
    /// * `packet` - The decoded MeshCore packet being received
    /// * `receiver` - Name of the receiving node
    /// * `receive_time` - Simulation time in microseconds when received
    pub fn track_reception(
        &mut self,
        packet: &MeshCorePacket,
        receiver: &str,
        receive_time: u64,
    ) {
        let payload_hash = packet.payload_hash_label();
        let route_type = packet.route_type();
        let payload_type = packet.payload_type();
        let hop_count = Some(packet.path_len() as u32);

        // Build labels with packet breakdown
        // The recorder will filter to only the labels requested in metric specs
        let labels = [
            ("route_type", route_type.as_label().to_string()),
            ("payload_type", payload_type.as_label().to_string()),
            ("payload_hash", payload_hash.as_label()),
        ];

        if let Some(kind) = self.packets.get_mut(&payload_hash) {
            match kind {
                PacketKind::Flood {
                    nodes_reached,
                    reception_times,
                    times_heard,
                    last_activity,
                    ..
                } => {
                    nodes_reached.insert(receiver.to_string());
                    reception_times.insert(receiver.to_string(), receive_time);
                    *times_heard += 1;
                    *last_activity = receive_time;

                    metrics::counter!(metric_defs::PACKET_RX_FLOOD.name, &labels).increment(1);
                }
                PacketKind::Direct {
                    destination,
                    delivered,
                    delivery_time,
                    origin_time,
                    hop_count: stored_hop_count,
                    last_activity,
                } => {
                    *last_activity = receive_time;
                    if receiver == destination {
                        *delivered = true;
                        *delivery_time = Some(receive_time);
                        *stored_hop_count = hop_count;

                        let latency_ms = (receive_time - *origin_time) / 1000; // us to ms
                        metrics::counter!(metric_defs::DIRECT_DELIVERED.name, &labels).increment(1);
                        metrics::histogram!(metric_defs::DIRECT_DELIVERY_LATENCY.name, &labels)
                            .record(latency_ms as f64);
                        if let Some(hops) = hop_count {
                            metrics::histogram!(metric_defs::DIRECT_HOPS.name, &labels)
                                .record(hops as f64);
                        }
                    }
                    metrics::counter!(metric_defs::PACKET_RX_DIRECT.name, &labels).increment(1);
                }
            }
        }
    }

    /// Emit summary metrics for completed flood packets.
    ///
    /// Call this periodically or at end of simulation to record
    /// flood propagation statistics.
    pub fn emit_flood_summaries(&self) {
        for kind in self.packets.values() {
            if let PacketKind::Flood {
                nodes_reached,
                reception_times,
                origin_time,
                times_heard,
                ..
            } = kind
            {
                // Nodes reached
                metrics::histogram!(metric_defs::FLOOD_NODES_REACHED.name)
                    .record(nodes_reached.len() as f64);

                // Times heard
                metrics::histogram!(metric_defs::FLOOD_TIMES_HEARD.name)
                    .record(*times_heard as f64);

                // Propagation time (time to reach furthest node)
                if let Some(&max_time) = reception_times.values().max() {
                    let propagation_us = max_time - origin_time;
                    metrics::histogram!(metric_defs::FLOOD_PROPAGATION_TIME.name)
                        .record(propagation_us as f64);
                }

                // Coverage
                if self.total_nodes > 0 {
                    let coverage = nodes_reached.len() as f64 / self.total_nodes as f64;
                    metrics::gauge!(metric_defs::FLOOD_COVERAGE.name).set(coverage);
                }
            }
        }
    }

    /// Mark a direct message as failed (timeout/delivery failure).
    ///
    /// # Arguments
    ///
    /// * `hash` - Unique hash identifying the packet
    #[allow(dead_code)]
    pub fn mark_direct_failed(&mut self, hash: PayloadHash) {
        if let Some(PacketKind::Direct { delivered, .. }) = self.packets.get(&hash) {
            if !*delivered {
                metrics::counter!(metric_defs::DIRECT_FAILED.name).increment(1);
            }
        }
    }

    /// Get the number of tracked packets.
    #[allow(dead_code)]
    pub fn packet_count(&self) -> usize {
        self.packets.len()
    }

    /// Get tracking info for a specific packet.
    #[allow(dead_code)]
    pub fn get_packet(&self, hash: PayloadHash) -> Option<&PacketKind> {
        self.packets.get(&hash)
    }

    /// Clear all tracked packets.
    ///
    /// Useful for resetting state between simulation runs or
    /// to free memory for very long simulations.
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.packets.clear();
    }

    /// Evict packets older than the specified age threshold.
    ///
    /// This method removes packets whose last activity was more than `max_age_us`
    /// microseconds before `current_time_us`. Before removal, summary metrics are
    /// emitted for flood packets, and undelivered direct packets are marked as failed.
    ///
    /// # Arguments
    ///
    /// * `current_time_us` - Current simulation time in microseconds
    /// * `max_age_us` - Maximum packet age in microseconds before eviction
    ///
    /// # Returns
    ///
    /// Number of packets evicted
    #[allow(dead_code)]
    pub fn evict_old_packets(&mut self, current_time_us: u64, max_age_us: u64) -> usize {
        let cutoff_time = current_time_us.saturating_sub(max_age_us);

        // Collect hashes of packets to evict
        let to_evict: Vec<PayloadHash> = self
            .packets
            .iter()
            .filter_map(|(hash, kind)| {
                let last_activity = match kind {
                    PacketKind::Flood { last_activity, .. } => *last_activity,
                    PacketKind::Direct { last_activity, .. } => *last_activity,
                };
                if last_activity < cutoff_time {
                    Some(*hash)
                } else {
                    None
                }
            })
            .collect();

        let evict_count = to_evict.len();

        // Emit metrics and remove each packet
        for hash in to_evict {
            if let Some(kind) = self.packets.remove(&hash) {
                self.emit_packet_summary(&kind);
            }
        }

        evict_count
    }

    /// Emit summary metrics for a single packet.
    ///
    /// Called during eviction to ensure metrics are recorded before removal.
    fn emit_packet_summary(&self, kind: &PacketKind) {
        match kind {
            PacketKind::Flood {
                nodes_reached,
                reception_times,
                origin_time,
                times_heard,
                ..
            } => {
                // Nodes reached
                metrics::histogram!(metric_defs::FLOOD_NODES_REACHED.name)
                    .record(nodes_reached.len() as f64);

                // Times heard
                metrics::histogram!(metric_defs::FLOOD_TIMES_HEARD.name).record(*times_heard as f64);

                // Propagation time (time to reach furthest node)
                if let Some(&max_time) = reception_times.values().max() {
                    let propagation_us = max_time - origin_time;
                    metrics::histogram!(metric_defs::FLOOD_PROPAGATION_TIME.name)
                        .record(propagation_us as f64);
                }

                // Coverage
                if self.total_nodes > 0 {
                    let coverage = nodes_reached.len() as f64 / self.total_nodes as f64;
                    metrics::gauge!(metric_defs::FLOOD_COVERAGE.name).set(coverage);
                }
            }
            PacketKind::Direct { delivered, .. } => {
                // Mark undelivered direct packets as failed
                if !*delivered {
                    metrics::counter!(metric_defs::DIRECT_FAILED.name).increment(1);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a test flood packet (advert).
    fn make_flood_packet() -> MeshCorePacket {
        use meshcore_packet::{AdvertPayload, PacketPayload, RouteType};
        MeshCorePacket::new(
            RouteType::Flood,
            PacketPayload::Advert(AdvertPayload::new(
                [0u8; 32],
                12345,
                [0u8; 64],
                "test_node",
            )),
        )
    }

    /// Create a test direct packet (text message).
    fn make_direct_packet() -> MeshCorePacket {
        MeshCorePacket::text_message(0x12, 0x34, 0x5678, vec![1, 2, 3, 4])
    }

    #[test]
    fn test_flood_packet_tracking() {
        let mut tracker = PacketTracker::new(5);
        let packet = make_flood_packet();
        let hash = packet.payload_hash_label();

        // Send flood packet
        tracker.track_send(&packet, None, 1000);

        // Receive at multiple nodes
        tracker.track_reception(&packet, "node1", 1500);
        tracker.track_reception(&packet, "node2", 2000);
        tracker.track_reception(&packet, "node1", 2500); // Same node, second reception

        // Verify tracking
        let tracked = tracker.get_packet(hash).unwrap();
        if let PacketKind::Flood {
            nodes_reached,
            times_heard,
            ..
        } = tracked
        {
            assert_eq!(nodes_reached.len(), 2); // node1 and node2
            assert_eq!(*times_heard, 3); // 3 total receptions
        } else {
            panic!("Expected flood packet");
        }
    }

    #[test]
    fn test_direct_packet_tracking() {
        let mut tracker = PacketTracker::new(5);
        let packet = make_direct_packet();
        let hash = packet.payload_hash_label();

        // Send direct packet
        tracker.track_send(&packet, Some("target_node".to_string()), 1000);

        // Receive at non-target node (should not mark as delivered)
        tracker.track_reception(&packet, "other_node", 1500);

        let tracked = tracker.get_packet(hash).unwrap();
        if let PacketKind::Direct { delivered, .. } = tracked {
            assert!(!*delivered);
        } else {
            panic!("Expected direct packet");
        }

        // Receive at target node
        tracker.track_reception(&packet, "target_node", 2000);

        let tracked = tracker.get_packet(hash).unwrap();
        if let PacketKind::Direct {
            delivered,
            delivery_time,
            hop_count,
            ..
        } = tracked
        {
            assert!(*delivered);
            assert_eq!(*delivery_time, Some(2000));
            // hop_count comes from packet.path_len() which is 0 for our test packet
            assert_eq!(*hop_count, Some(0));
        } else {
            panic!("Expected direct packet");
        }
    }

    #[test]
    fn test_direct_packet_failure() {
        let mut tracker = PacketTracker::new(5);
        let packet = make_direct_packet();
        let hash = packet.payload_hash_label();

        // Send direct packet
        tracker.track_send(&packet, Some("target_node".to_string()), 1000);

        // Mark as failed without delivery
        tracker.mark_direct_failed(hash);

        // Verify still not delivered
        let tracked = tracker.get_packet(hash).unwrap();
        if let PacketKind::Direct { delivered, .. } = tracked {
            assert!(!*delivered);
        } else {
            panic!("Expected direct packet");
        }
    }

    #[test]
    fn test_tracker_clear() {
        let mut tracker = PacketTracker::new(5);
        let packet = make_flood_packet();

        tracker.track_send(&packet, None, 1000);
        assert_eq!(tracker.packet_count(), 1);

        tracker.clear();
        assert_eq!(tracker.packet_count(), 0);
    }

    #[test]
    fn test_evict_old_packets() {
        let mut tracker = PacketTracker::new(5);

        // Create and track a flood packet at time 1000us (1 second)
        let flood_packet = make_flood_packet();
        tracker.track_send(&flood_packet, None, 1_000_000); // 1 second
        tracker.track_reception(&flood_packet, "node1", 1_500_000); // last activity at 1.5s

        // Create and track a direct packet at time 9 seconds (recent)
        let direct_packet = make_direct_packet();
        tracker.track_send(&direct_packet, Some("target".to_string()), 9_000_000);
        tracker.track_reception(&direct_packet, "target", 9_500_000); // last activity at 9.5s

        assert_eq!(tracker.packet_count(), 2);

        // Evict packets older than 2 seconds, at current time 10 seconds
        // Cutoff = 10s - 2s = 8s
        // The flood packet (last activity 1.5s) should be evicted (1.5s < 8s)
        // The direct packet (last activity 9.5s) should remain (9.5s >= 8s)
        let evicted = tracker.evict_old_packets(10_000_000, 2_000_000);

        assert_eq!(evicted, 1);
        assert_eq!(tracker.packet_count(), 1);

        // Verify the direct packet is still there
        let hash = direct_packet.payload_hash_label();
        assert!(tracker.get_packet(hash).is_some());
    }

    #[test]
    fn test_evict_no_packets_when_all_recent() {
        let mut tracker = PacketTracker::new(5);
        let packet = make_flood_packet();

        tracker.track_send(&packet, None, 1_000_000);
        tracker.track_reception(&packet, "node1", 9_500_000); // Recent activity

        // Try to evict at time 10s with 2s threshold - packet should remain
        let evicted = tracker.evict_old_packets(10_000_000, 2_000_000);

        assert_eq!(evicted, 0);
        assert_eq!(tracker.packet_count(), 1);
    }
}
