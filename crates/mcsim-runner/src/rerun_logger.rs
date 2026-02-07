//! Rerun visualization logger for MCSim simulation events.
//!
//! This module provides integration with [rerun.io](https://rerun.io) for
//! visualizing simulation events in real-time or from recordings.
//!
//! When the `rerun` feature is disabled, this module provides no-op stub
//! implementations that compile but do nothing, avoiding the heavy rerun
//! dependency for faster compile times during development.

use mcsim_common::{Event, GeoCoord, SimTime};

/// Information about a node for visualization purposes.
#[derive(Debug, Clone)]
pub struct VisNodeInfo {
    /// Node name.
    pub name: String,
    /// Node type (Repeater, Companion, RoomServer).
    pub node_type: String,
    /// Firmware entity ID.
    pub firmware_entity_id: u64,
    /// Radio entity ID.
    pub radio_entity_id: u64,
    /// Geographic location.
    pub location: GeoCoord,
}

/// Information about a link between two nodes.
#[derive(Debug, Clone)]
pub struct VisLinkInfo {
    /// Source node name.
    pub from: String,
    /// Destination node name.
    pub to: String,
    /// Source node location.
    pub from_location: GeoCoord,
    /// Destination node location.
    pub to_location: GeoCoord,
    /// Mean signal-to-noise ratio in dB (at 20 dBm TX power).
    pub mean_snr_db_at20dbm: f64,
}

// ============================================================================
// Real implementation (when rerun feature is enabled)
// ============================================================================

#[cfg(feature = "rerun")]
mod real_impl {
    use super::*;
    use crate::metric_spec::{CounterValue, GaugeValue, HistogramValue, MetricValue};
    use mcsim_common::EventPayload;
    use meshcore_packet::{MeshCorePacket, PacketPayload};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Minimum duration (in microseconds) that highlights should remain visible.
    const HIGHLIGHT_DURATION_US: u64 = 100_000; // 100ms

    /// Format a decoded packet for display in rerun logs.
    /// Returns a string with packet hash, type, path info, and payload details.
    fn format_packet_decode(packet_bytes: &[u8]) -> String {
        // Calculate payload hash from raw bytes (full 64-bit hash for consistency)
        let hash = MeshCorePacket::payload_hash_from_bytes(packet_bytes)
            .map(|h| format!("{:016X}", h))
            .unwrap_or_else(|| "????????????????".to_string());

        // Try to decode the packet
        match MeshCorePacket::decode(packet_bytes) {
            Ok(packet) => {
                let route_type = packet.header.route_type;
                let payload_type = packet.header.payload_type;
                
                // Format path info
                let path_info = if packet.path.is_empty() {
                    "no path".to_string()
                } else {
                    let path_hashes: Vec<String> = packet.path.iter()
                        .map(|b| format!("{:02X}", b))
                        .collect();
                    format!("path=[{}]", path_hashes.join("→"))
                };

                // Format payload-specific details
                let payload_details = match &packet.payload {
                    PacketPayload::Advert(advert) => {
                        let name = advert.name.as_deref().unwrap_or("<unnamed>");
                        let node_type = if advert.flags.is_repeater {
                            "repeater"
                        } else if advert.flags.is_room_server {
                            "room_server"
                        } else if advert.flags.is_chat_node {
                            "chat_node"
                        } else if advert.flags.is_sensor {
                            "sensor"
                        } else {
                            "unknown"
                        };
                        format!("name=\"{}\" type={}", name, node_type)
                    }
                    PacketPayload::Ack(ack) => {
                        format!("checksum={:08X}", ack.checksum)
                    }
                    PacketPayload::TextMessage(txt) => {
                        format!("dest={:02X} src={:02X}", txt.header.dest_hash, txt.header.src_hash)
                    }
                    PacketPayload::Request(req) => {
                        format!("dest={:02X} src={:02X}", req.header.dest_hash, req.header.src_hash)
                    }
                    PacketPayload::Response(resp) => {
                        format!("dest={:02X} src={:02X}", resp.header.dest_hash, resp.header.src_hash)
                    }
                    PacketPayload::Path(path) => {
                        format!("dest={:02X} src={:02X}", path.header.dest_hash, path.header.src_hash)
                    }
                    PacketPayload::AnonRequest(anon) => {
                        format!("dest={:02X}", anon.dest_hash)
                    }
                    PacketPayload::GroupText(grp) | PacketPayload::GroupData(grp) => {
                        format!("channel={:02X}", grp.channel_hash)
                    }
                    PacketPayload::Control(ctrl) => {
                        format!("subtype={:?}", ctrl.sub_type)
                    }
                    PacketPayload::Trace(_) => "trace".to_string(),
                    PacketPayload::Multipart(_) => "multipart".to_string(),
                    PacketPayload::Raw(_) => "raw".to_string(),
                };

                format!(
                    "hash={} {} {} {} | {}",
                    hash, route_type, payload_type, path_info, payload_details
                )
            }
            Err(_) => {
                // Failed to decode, show raw info
                format!("hash={} [decode failed, {} bytes]", hash, packet_bytes.len())
            }
        }
    }

    /// Sanitize a node name for use in a rerun entity path.
    /// Rerun entity paths use `/` as a hierarchy separator, so we need to
    /// replace slashes and other problematic characters with underscores.
    fn sanitize_for_entity_path(name: &str) -> String {
        name.chars()
            .map(|c| match c {
                '/' | '\\' | '*' | '?' | '"' | '<' | '>' | '|' | ':' => '_',
                c if c.is_control() => '_',
                c => c,
            })
            .collect()
    }

    /// Rerun logger for simulation visualization.
    pub struct RerunLogger {
        rec: rerun::RecordingStream,
        /// Map from entity ID to node info for looking up node names.
        entity_to_node: HashMap<u64, VisNodeInfo>,
        /// Map from radio entity ID to node name.
        radio_id_to_name: HashMap<u64, String>,
        /// All links in the network (keyed by "from->to" for easy lookup).
        links: HashMap<String, VisLinkInfo>,
        /// Node positions by name.
        node_positions: HashMap<String, GeoCoord>,
        /// Highlighted links with their expiry time (canonical_key -> expiry_time_us).
        highlighted_links: HashMap<String, u64>,
        /// Highlighted nodes with their expiry time (node_name -> expiry_time_us).
        highlighted_nodes: HashMap<String, u64>,
        /// Flag indicating if logging is disabled (e.g., viewer disconnected).
        disabled: AtomicBool,
        /// Counter for consecutive errors - used to detect disconnection.
        error_count: std::cell::Cell<u32>,
    }

    impl RerunLogger {
        /// Create a new RerunLogger and spawn the viewer.
        ///
        /// # Arguments
        /// * `app_name` - Name of the application shown in the viewer
        /// * `nodes` - Information about nodes in the simulation
        /// * `links` - Information about links between nodes
        pub fn new(
            app_name: &str,
            nodes: Vec<VisNodeInfo>,
            links: Vec<VisLinkInfo>,
        ) -> Result<Self, Box<dyn std::error::Error>> {
            // Try to find rerun executable in the following order:
            // 1. In a "rerun" folder relative to the current executable
            // 2. In a "rerun" folder relative to the current working directory
            // 3. Fall back to PATH lookup (default behavior)
            let rerun_exe_path = Self::find_rerun_executable();

            // Initialize rerun with spawn options
            let spawn_opts = rerun::SpawnOptions {
                executable_path: rerun_exe_path,
                ..Default::default()
            };
            let rec = rerun::RecordingStreamBuilder::new(app_name).spawn_opts(&spawn_opts)?;

            let mut logger = RerunLogger {
                rec,
                entity_to_node: HashMap::new(),
                radio_id_to_name: HashMap::new(),
                links: HashMap::new(),
                node_positions: HashMap::new(),
                highlighted_links: HashMap::new(),
                highlighted_nodes: HashMap::new(),
                disabled: AtomicBool::new(false),
                error_count: std::cell::Cell::new(0),
            };

            // Set up node mappings
            for node in &nodes {
                logger
                    .entity_to_node
                    .insert(node.firmware_entity_id, node.clone());
                logger
                    .entity_to_node
                    .insert(node.radio_entity_id, node.clone());
                logger
                    .radio_id_to_name
                    .insert(node.radio_entity_id, node.name.clone());
                logger
                    .node_positions
                    .insert(node.name.clone(), node.location.clone());
            }

            // Set up link mappings
            for link in &links {
                let key = format!("{}->{}", link.from, link.to);
                logger.links.insert(key, link.clone());
            }

            // Log initial node positions and links
            eprintln!("Rerun: Logging {} nodes and {} links", nodes.len(), links.len());
            logger.log_initial_state(&nodes, &links)?;

            Ok(logger)
        }

        /// Find the rerun executable in the following order:
        /// 1. In a "rerun" folder relative to the current executable
        /// 2. In a "rerun" folder relative to the current working directory
        /// 3. Return None to fall back to PATH lookup
        fn find_rerun_executable() -> Option<String> {
            #[cfg(target_os = "windows")]
            const RERUN_EXE: &str = "rerun.exe";
            #[cfg(not(target_os = "windows"))]
            const RERUN_EXE: &str = "rerun";

            // Try relative to current executable
            if let Ok(exe_path) = std::env::current_exe() {
                // Go up from target/release or target/debug to the workspace root
                let mut path = exe_path;
                for _ in 0..3 {
                    // Try up to 3 levels up (exe -> release/debug -> target -> workspace)
                    if let Some(parent) = path.parent() {
                        let rerun_path = parent.join("rerun").join(RERUN_EXE);
                        if rerun_path.exists() {
                            eprintln!("Rerun: Found executable at {:?}", rerun_path);
                            return Some(rerun_path.to_string_lossy().to_string());
                        }
                        path = parent.to_path_buf();
                    }
                }
            }

            // Try relative to current working directory
            if let Ok(cwd) = std::env::current_dir() {
                let rerun_path = cwd.join("rerun").join(RERUN_EXE);
                if rerun_path.exists() {
                    eprintln!("Rerun: Found executable at {:?}", rerun_path);
                    return Some(rerun_path.to_string_lossy().to_string());
                }
            }

            eprintln!("Rerun: No local executable found, falling back to PATH lookup");
            None
        }

        /// Log the initial state (node positions on map, link lines).
        /// 
        /// Uses a layered visualization approach:
        /// - Outer ring: Node type indicator (static, muted colors, larger radius)
        /// - Inner circle: State indicator (dynamic, bright colors for activity)
        fn log_initial_state(
            &self,
            nodes: &[VisNodeInfo],
            links: &[VisLinkInfo],
        ) -> Result<(), Box<dyn std::error::Error>> {
            // Log nodes by type as separate layers for better visual distinction
            // Each node type gets its own entity path with distinct styling
            
            // Collect nodes by type
            let mut repeaters: Vec<&VisNodeInfo> = Vec::new();
            let mut companions: Vec<&VisNodeInfo> = Vec::new();
            let mut room_servers: Vec<&VisNodeInfo> = Vec::new();
            let mut unknown: Vec<&VisNodeInfo> = Vec::new();

            for node in nodes {
                match node.node_type.as_str() {
                    "Repeater" => repeaters.push(node),
                    "Companion" => companions.push(node),
                    "RoomServer" => room_servers.push(node),
                    _ => unknown.push(node),
                }
            }

            // Log Repeaters - Blue outer ring (tower/infrastructure)
            if !repeaters.is_empty() {
                let positions: Vec<(f64, f64)> = repeaters
                    .iter()
                    .map(|n| (n.location.latitude, n.location.longitude))
                    .collect();
                let labels: Vec<String> = repeaters.iter().map(|n| n.name.clone()).collect();
                
                self.rec.log_static(
                    "map/nodes/repeaters",
                    &rerun::GeoPoints::from_lat_lon(positions)
                        .with_colors([[70, 130, 180, 200]]) // Steel blue, slightly transparent
                        .with_radii([rerun::Radius::new_ui_points(14.0)]), // Large outer ring
                )?;
            }

            // Log Companions - Green outer ring (mobile/client)
            if !companions.is_empty() {
                let positions: Vec<(f64, f64)> = companions
                    .iter()
                    .map(|n| (n.location.latitude, n.location.longitude))
                    .collect();
                
                self.rec.log_static(
                    "map/nodes/companions",
                    &rerun::GeoPoints::from_lat_lon(positions)
                        .with_colors([[60, 179, 113, 200]]) // Medium sea green, slightly transparent
                        .with_radii([rerun::Radius::new_ui_points(14.0)]),
                )?;
            }

            // Log Room Servers - Orange outer ring (server/hub)
            if !room_servers.is_empty() {
                let positions: Vec<(f64, f64)> = room_servers
                    .iter()
                    .map(|n| (n.location.latitude, n.location.longitude))
                    .collect();
                
                self.rec.log_static(
                    "map/nodes/room_servers",
                    &rerun::GeoPoints::from_lat_lon(positions)
                        .with_colors([[210, 105, 30, 200]]) // Chocolate orange, slightly transparent
                        .with_radii([rerun::Radius::new_ui_points(14.0)]),
                )?;
            }

            // Log Unknown types - Gray outer ring
            if !unknown.is_empty() {
                let positions: Vec<(f64, f64)> = unknown
                    .iter()
                    .map(|n| (n.location.latitude, n.location.longitude))
                    .collect();
                
                self.rec.log_static(
                    "map/nodes/unknown",
                    &rerun::GeoPoints::from_lat_lon(positions)
                        .with_colors([[128, 128, 128, 200]])
                        .with_radii([rerun::Radius::new_ui_points(14.0)]),
                )?;
            }

            // Log initial state indicator (inner circle) for all nodes - starts as dim/idle
            for node in nodes {
                self.rec.log_static(
                    format!("map/nodes/state/{}", sanitize_for_entity_path(&node.name)),
                    &rerun::GeoPoints::from_lat_lon([(node.location.latitude, node.location.longitude)])
                        .with_colors([[80, 80, 80, 255]]) // Dark gray = idle
                        .with_radii([rerun::Radius::new_ui_points(6.0)]), // Smaller inner circle
                )?;
            }

            // Log all links as GeoLineStrings (gray/dim when inactive)
            // Use log_static for initial state so links appear without timeline scrubbing
            self.log_all_links_static(links)?;

            Ok(())
        }

        /// Log all links as static (timeless) entities.
        /// Used for initial visualization - links will be visible at all times.
        fn log_all_links_static(
            &self,
            links: &[VisLinkInfo],
        ) -> Result<(), Box<dyn std::error::Error>> {
            // Build unique bidirectional links (avoid drawing same link twice)
            let mut drawn_links: std::collections::HashSet<(String, String)> =
                std::collections::HashSet::new();

            for link in links {
                // Create a canonical key for the link (sorted names)
                let (a, b) = if link.from < link.to {
                    (link.from.clone(), link.to.clone())
                } else {
                    (link.to.clone(), link.from.clone())
                };

                if drawn_links.contains(&(a.clone(), b.clone())) {
                    continue;
                }
                drawn_links.insert((a.clone(), b.clone()));

                let line_points: Vec<(f64, f64)> = vec![
                    (link.from_location.latitude, link.from_location.longitude),
                    (link.to_location.latitude, link.to_location.longitude),
                ];

                // Log as static geo line for each link (very thin, semi-transparent)
                let link_path = format!("map/links/{}_to_{}", sanitize_for_entity_path(&a), sanitize_for_entity_path(&b));
                self.rec.log_static(
                    link_path.as_str(),
                    &rerun::GeoLineStrings::from_lat_lon([line_points])
                        .with_colors([[80, 80, 80, 60]]) // Very dim, mostly transparent
                        .with_radii([rerun::Radius::new_ui_points(0.5)]),
                )?;
            }

            Ok(())
        }

        /// Log all links, optionally highlighting active ones.
        /// This version uses log() for time-varying data during simulation.
        fn log_all_links(
            &self,
            links: &[VisLinkInfo],
            active_link: Option<(&str, &str)>,
        ) -> Result<(), Box<dyn std::error::Error>> {
            // Build unique bidirectional links (avoid drawing same link twice)
            let mut drawn_links: std::collections::HashSet<(String, String)> =
                std::collections::HashSet::new();

            for link in links {
                // Create a canonical key for the link (sorted names)
                let (a, b) = if link.from < link.to {
                    (link.from.clone(), link.to.clone())
                } else {
                    (link.to.clone(), link.from.clone())
                };

                if drawn_links.contains(&(a.clone(), b.clone())) {
                    continue;
                }
                drawn_links.insert((a.clone(), b.clone()));

                // Determine if this link is active
                let is_active = active_link.map_or(false, |(from, to)| {
                    (link.from == from && link.to == to) || (link.from == to && link.to == from)
                });

                // Line color: bright yellow/orange when active, very dim when inactive
                let color: [u8; 4] = if is_active {
                    [255, 200, 0, 255] // Bright yellow for active link
                } else {
                    [80, 80, 80, 60] // Very dim, mostly transparent
                };

                // Line width: thicker when active
                let radius = if is_active {
                    rerun::Radius::new_ui_points(4.0)
                } else {
                    rerun::Radius::new_ui_points(0.5)
                };

                let line_points: Vec<(f64, f64)> = vec![
                    (link.from_location.latitude, link.from_location.longitude),
                    (link.to_location.latitude, link.to_location.longitude),
                ];

                // Log as individual geo line for each link
                let link_path = format!("map/links/{}_to_{}", sanitize_for_entity_path(&a), sanitize_for_entity_path(&b));
                self.rec.log(
                    link_path.as_str(),
                    &rerun::GeoLineStrings::from_lat_lon([line_points])
                        .with_colors([color])
                        .with_radii([radius]),
                )?;
            }

            Ok(())
        }

        /// Highlight a specific link during traffic.
        fn highlight_link(
            &mut self,
            from_name: &str,
            to_name: &str,
            current_time_us: u64,
        ) -> Result<(), Box<dyn std::error::Error>> {
            // Get positions for both nodes
            let from_pos = match self.node_positions.get(from_name) {
                Some(pos) => pos.clone(),
                None => return Ok(()),
            };
            let to_pos = match self.node_positions.get(to_name) {
                Some(pos) => pos.clone(),
                None => return Ok(()),
            };

            // Create canonical link name (sorted)
            let (a, b) = if from_name < to_name {
                (from_name.to_string(), to_name.to_string())
            } else {
                (to_name.to_string(), from_name.to_string())
            };

            // Store expiry time for this highlight (use sanitized key for path consistency)
            let canonical_key = format!("{}_to_{}", sanitize_for_entity_path(&a), sanitize_for_entity_path(&b));
            self.highlighted_links.insert(canonical_key.clone(), current_time_us + HIGHLIGHT_DURATION_US);

            // Log highlighted link
            let line_points: Vec<(f64, f64)> = vec![
                (from_pos.latitude, from_pos.longitude),
                (to_pos.latitude, to_pos.longitude),
            ];

            let link_path = format!("map/links/{}", canonical_key);
            self.rec.log(
                link_path.as_str(),
                &rerun::GeoLineStrings::from_lat_lon([line_points])
                    .with_colors([[255, 200, 0, 255]]) // Bright yellow
                    .with_radii([rerun::Radius::new_ui_points(4.0)]),
            )?;

            Ok(())
        }

        /// Reset a link to inactive state.
        fn reset_link(&self, from_name: &str, to_name: &str) -> Result<(), Box<dyn std::error::Error>> {
            // Get positions for both nodes
            let from_pos = match self.node_positions.get(from_name) {
                Some(pos) => pos,
                None => return Ok(()),
            };
            let to_pos = match self.node_positions.get(to_name) {
                Some(pos) => pos,
                None => return Ok(()),
            };

            // Create canonical link name (sorted)
            let (a, b) = if from_name < to_name {
                (from_name, to_name)
            } else {
                (to_name, from_name)
            };

            // Log reset link (back to gray)
            let line_points: Vec<(f64, f64)> = vec![
                (from_pos.latitude, from_pos.longitude),
                (to_pos.latitude, to_pos.longitude),
            ];

            let link_path = format!("map/links/{}_to_{}", sanitize_for_entity_path(a), sanitize_for_entity_path(b));
            self.rec.log(
                link_path.as_str(),
                &rerun::GeoLineStrings::from_lat_lon([line_points])
                    .with_colors([[80, 80, 80, 60]]) // Very dim, mostly transparent
                    .with_radii([rerun::Radius::new_ui_points(0.5)]),
            )?;

            Ok(())
        }

        /// Reset expired highlights (links and nodes that have been visible long enough).
        fn reset_expired_highlights(&mut self, current_time_us: u64) -> Result<(), Box<dyn std::error::Error>> {
            // Collect expired links
            let expired_links: Vec<String> = self.highlighted_links
                .iter()
                .filter(|(_, &expiry)| current_time_us >= expiry)
                .map(|(k, _)| k.clone())
                .collect();

            // Reset expired links
            for canonical_key in expired_links {
                self.highlighted_links.remove(&canonical_key);
                // Parse the canonical key to get node names (format: "a_to_b")
                if let Some(idx) = canonical_key.find("_to_") {
                    let from_name = &canonical_key[..idx];
                    let to_name = &canonical_key[idx + 4..];
                    let _ = self.reset_link(from_name, to_name);
                }
            }

            // Collect expired nodes
            let expired_nodes: Vec<String> = self.highlighted_nodes
                .iter()
                .filter(|(_, &expiry)| current_time_us >= expiry)
                .map(|(k, _)| k.clone())
                .collect();

            // Reset expired nodes to idle state
            for node_name in expired_nodes {
                self.highlighted_nodes.remove(&node_name);
                if let Some(pos) = self.node_positions.get(&node_name) {
                    self.rec.log(
                        format!("map/nodes/state/{}", sanitize_for_entity_path(&node_name)),
                        &rerun::GeoPoints::from_lat_lon([(pos.latitude, pos.longitude)])
                            .with_colors([[80, 80, 80, 255]]) // Dark gray = idle
                            .with_radii([rerun::Radius::new_ui_points(6.0)]),
                    )?;
                }
            }

            Ok(())
        }

        /// Check if logging is disabled (e.g., viewer disconnected).
        fn is_disabled(&self) -> bool {
            self.disabled.load(Ordering::Relaxed)
        }

        /// Handle a logging error - tracks consecutive errors and disables after threshold.
        fn handle_error(&self, _err: &dyn std::error::Error) {
            let count = self.error_count.get() + 1;
            self.error_count.set(count);
            
            // After 3 consecutive errors, assume viewer is disconnected
            if count >= 3 {
                if !self.disabled.swap(true, Ordering::Relaxed) {
                    eprintln!("\n⚠️  Rerun viewer disconnected - disabling visualization logging");
                }
            }
        }

        /// Reset error count on successful log.
        fn clear_errors(&self) {
            self.error_count.set(0);
        }

        /// Log a simulation event.
        pub fn log_event(&mut self, event: &Event) -> Result<(), Box<dyn std::error::Error>> {
            // Skip if logging is disabled (viewer disconnected)
            if self.is_disabled() {
                return Ok(());
            }

            let current_time_us = event.time.as_micros();
            
            // Set the time for this event
            self.rec.set_time("sim_time", std::time::Duration::from_micros(current_time_us));

            // Reset any highlights that have expired (ignore errors here)
            let _ = self.reset_expired_highlights(current_time_us);

            let result = match &event.payload {
                EventPayload::TransmitAir(tx_event) => {
                    self.log_transmit(event, tx_event)
                }
                EventPayload::RadioRxPacket(rx_event) => {
                    self.log_receive(event, rx_event)
                }
                EventPayload::MessageSend(msg_event) => {
                    self.log_message_send(event, msg_event)
                }
                EventPayload::MessageReceived(msg_event) => {
                    self.log_message_received(event, msg_event)
                }
                EventPayload::MessageAcknowledged(ack_event) => {
                    self.log_message_acked(event, ack_event)
                }
                _ => {
                    // Other events not visualized
                    Ok(())
                }
            };

            match &result {
                Ok(_) => self.clear_errors(),
                Err(e) => self.handle_error(e.as_ref()),
            }

            // Always return Ok to not disrupt simulation
            Ok(())
        }

        /// Log a transmission event.
        fn log_transmit(
            &mut self,
            event: &Event,
            tx_event: &mcsim_common::TransmitAirEvent,
        ) -> Result<(), Box<dyn std::error::Error>> {
            let current_time_us = event.time.as_micros();
            
            if let Some(node) = self.entity_to_node.get(&tx_event.radio_id.0) {
                let node_name = node.name.clone();
                let node_lat = node.location.latitude;
                let node_lon = node.location.longitude;
                
                // Track this node's highlight expiry
                self.highlighted_nodes.insert(node_name.clone(), current_time_us + HIGHLIGHT_DURATION_US);
                
                // Update the inner state circle to show TX (bright red)
                self.rec.log(
                    format!("map/nodes/state/{}", sanitize_for_entity_path(&node_name)),
                    &rerun::GeoPoints::from_lat_lon([(node_lat, node_lon)])
                        .with_colors([[255, 50, 50, 255]]) // Bright red for TX
                        .with_radii([rerun::Radius::new_ui_points(6.0)]),
                )?;

                // Decode packet for detailed logging
                let packet_decode = format_packet_decode(&tx_event.packet.payload);

                // Log transmission info as text with decoded packet details
                // Format: size | packet decode | radio params (for alignment)
                self.rec.log(
                    format!("radio/{}/tx_info", sanitize_for_entity_path(&node_name)),
                    &rerun::TextLog::new(format!(
                        "TX: {:3}B | {} | {}dBm",
                        tx_event.packet.payload.len(),
                        packet_decode,
                        tx_event.params.tx_power_dbm
                    )),
                )?;
            }

            Ok(())
        }

        /// Log a receive event.
        fn log_receive(
            &mut self,
            event: &Event,
            rx_event: &mcsim_common::RadioRxPacketEvent,
        ) -> Result<(), Box<dyn std::error::Error>> {
            let current_time_us = event.time.as_micros();
            
            // Find the transmitting node name from the source radio ID
            let tx_node_name = self.radio_id_to_name.get(&rx_event.source_radio_id.0).cloned();
            
            // Find which node received this
            for target in &event.targets {
                if let Some(node) = self.entity_to_node.get(&target.0) {
                    // Determine status and color based on reception outcome
                    let (status, color): (&str, [u8; 4]) = if rx_event.was_collided {
                        ("COLLISION", [255, 140, 0, 255]) // Dark orange for collision
                    } else if rx_event.was_weak_signal {
                        ("WEAK", [255, 80, 80, 255]) // Red for weak signal
                    } else {
                        ("OK", [50, 255, 50, 255]) // Bright green for successful RX
                    };

                    // Clone data we need before mutable borrow
                    let node_name = node.name.clone();
                    let node_lat = node.location.latitude;
                    let node_lon = node.location.longitude;

                    // Track this node's highlight expiry
                    self.highlighted_nodes.insert(node_name.clone(), current_time_us + HIGHLIGHT_DURATION_US);

                    // Update the inner state circle to show RX state
                    self.rec.log(
                        format!("map/nodes/state/{}", sanitize_for_entity_path(&node_name)),
                        &rerun::GeoPoints::from_lat_lon([(node_lat, node_lon)])
                            .with_colors([color])
                            .with_radii([rerun::Radius::new_ui_points(6.0)]),
                    )?;

                    // Decode packet for detailed logging
                    let packet_decode = format_packet_decode(&rx_event.packet.payload);

                    // Log reception info with decoded packet details
                    // Format: size | packet decode | radio params (for alignment)
                    self.rec.log(
                        format!("radio/{}/rx_info", sanitize_for_entity_path(&node_name)),
                        &rerun::TextLog::new(format!(
                            "RX: {:3}B | {} | SNR:{:5.1}dB RSSI:{:6.1}dBm [{}]",
                            rx_event.packet.payload.len(),
                            packet_decode,
                            rx_event.snr_db,
                            rx_event.rssi_dbm,
                            status
                        )),
                    )?;

                    // Highlight the link between transmitter and receiver (only for successful receptions)
                    if let Some(ref tx_name) = tx_node_name {
                        if !rx_event.was_collided && !rx_event.was_weak_signal {
                            let _ = self.highlight_link(tx_name, &node_name, current_time_us);
                        }
                    }

                    break; // Only log for first matching target
                }
            }

            Ok(())
        }

        /// Log a message send event.
        fn log_message_send(
            &self,
            event: &Event,
            msg_event: &mcsim_common::MessageSendEvent,
        ) -> Result<(), Box<dyn std::error::Error>> {
            if let Some(node) = self.entity_to_node.get(&event.source.0) {
                self.rec.log(
                    format!("messages/{}/sent", sanitize_for_entity_path(&node.name)),
                    &rerun::TextLog::new(format!(
                        "MSG SEND: to={:?}, ack={}, \"{}\"",
                        msg_event.destination,
                        msg_event.want_ack,
                        truncate_string(&msg_event.content, 50)
                    )),
                )?;
            }

            Ok(())
        }

        /// Log a message received event.
        fn log_message_received(
            &self,
            event: &Event,
            _msg_event: &mcsim_common::MessageReceivedEvent,
        ) -> Result<(), Box<dyn std::error::Error>> {
            for target in &event.targets {
                if let Some(node) = self.entity_to_node.get(&target.0) {
                    self.rec.log(
                        format!("messages/{}/received", sanitize_for_entity_path(&node.name)),
                        &rerun::TextLog::new("MSG RECEIVED"),
                    )?;
                    break;
                }
            }

            Ok(())
        }

        /// Log a message acknowledged event.
        fn log_message_acked(
            &self,
            event: &Event,
            ack_event: &mcsim_common::MessageAckedEvent,
        ) -> Result<(), Box<dyn std::error::Error>> {
            for target in &event.targets {
                if let Some(node) = self.entity_to_node.get(&target.0) {
                    self.rec.log(
                        format!("messages/{}/acked", sanitize_for_entity_path(&node.name)),
                        &rerun::TextLog::new(format!(
                            "MSG ACK: hops={}",
                            ack_event.hop_count
                        )),
                    )?;
                    break;
                }
            }

            Ok(())
        }

        /// Log statistics update.
        pub fn log_stats(
            &self,
            time: SimTime,
            _total_events: u64,
            packets_tx: u64,
            packets_rx: u64,
            collisions: u64,
        ) -> Result<(), Box<dyn std::error::Error>> {
            // Skip if logging is disabled
            if self.is_disabled() {
                return Ok(());
            }

            self.rec.set_time("sim_time", std::time::Duration::from_micros(time.as_micros()));

            // Log packet stats as a bar chart for easy comparison
            // The bars represent: [TX, RX, Collisions]
            let values: Vec<u64> = vec![packets_tx, packets_rx, collisions];
            let result = self.rec.log(
                "stats/packets",
                &rerun::BarChart::new(values),
            );

            // Check if failed
            if let Err(e) = result {
                self.handle_error(&e);
            } else {
                self.clear_errors();
            }

            Ok(())
        }

        /// Log metrics from a MetricsExport snapshot.
        ///
        /// This logs all metrics in the export to Rerun as time-series Scalars,
        /// organized by entity path:
        /// - `metrics/{metric_name}` - Aggregated totals
        /// - `metrics/{metric_name}/by_node/{node_name}` - Per-node breakdowns
        /// - `metrics/{metric_name}/by_node_type/{node_type}` - Per-type breakdowns
        ///
        /// # Arguments
        /// * `time` - Current simulation time
        /// * `export` - The metrics export containing metrics and their breakdowns
        pub fn log_metrics_snapshot(
            &self,
            time: SimTime,
            export: &crate::metric_spec::MetricsExport,
        ) -> Result<(), Box<dyn std::error::Error>> {
            // Skip if logging is disabled
            if self.is_disabled() {
                return Ok(());
            }

            self.rec.set_time("sim_time", std::time::Duration::from_micros(time.as_micros()));

            // Process each metric in the export
            for (metric_name, metric_value) in &export.metrics {
                // Create a sanitized entity path from the metric name
                // e.g., "mcsim.radio.tx_packets" -> "metrics/radio/tx_packets"
                let base_path = format!("metrics/{}", metric_name.replace('.', "/").trim_start_matches("mcsim/"));

                match metric_value {
                    MetricValue::Counter(counter) => {
                        self.log_counter_metric(&base_path, counter)?;
                    }
                    MetricValue::Gauge(gauge) => {
                        self.log_gauge_metric(&base_path, gauge)?;
                    }
                    MetricValue::Histogram(histogram) => {
                        self.log_histogram_metric(&base_path, histogram)?;
                    }
                }
            }

            Ok(())
        }

        /// Log a counter metric and its breakdowns to Rerun.
        fn log_counter_metric(
            &self,
            base_path: &str,
            counter: &CounterValue,
        ) -> Result<(), Box<dyn std::error::Error>> {
            // Log the total/aggregate value
            self.rec.log(
                format!("{}/total", base_path),
                &rerun::Scalars::new([counter.total as f64]),
            )?;

            // Log breakdowns
            for (label_key, label_values) in &counter.labels {
                for (label_value, nested_counter) in label_values {
                    let breakdown_path = format!("{}/by_{}/{}", base_path, label_key, sanitize_for_entity_path(label_value));
                    // Log nested counter total
                    self.rec.log(
                        format!("{}/total", breakdown_path),
                        &rerun::Scalars::new([nested_counter.total as f64]),
                    )?;
                    // Recursively log any nested breakdowns
                    if !nested_counter.labels.is_empty() {
                        self.log_counter_metric(&breakdown_path, nested_counter)?;
                    }
                }
            }

            Ok(())
        }

        /// Log a gauge metric and its breakdowns to Rerun.
        fn log_gauge_metric(
            &self,
            base_path: &str,
            gauge: &GaugeValue,
        ) -> Result<(), Box<dyn std::error::Error>> {
            // Log the total/aggregate value
            self.rec.log(
                format!("{}/total", base_path),
                &rerun::Scalars::new([gauge.total]),
            )?;

            // Log breakdowns
            for (label_key, label_values) in &gauge.labels {
                for (label_value, nested_gauge) in label_values {
                    let breakdown_path = format!("{}/by_{}/{}", base_path, label_key, sanitize_for_entity_path(label_value));
                    // Log nested gauge total
                    self.rec.log(
                        format!("{}/total", breakdown_path),
                        &rerun::Scalars::new([nested_gauge.total]),
                    )?;
                    // Recursively log any nested breakdowns
                    if !nested_gauge.labels.is_empty() {
                        self.log_gauge_metric(&breakdown_path, nested_gauge)?;
                    }
                }
            }

            Ok(())
        }

        /// Log a histogram metric and its breakdowns to Rerun.
        fn log_histogram_metric(
            &self,
            base_path: &str,
            histogram: &HistogramValue,
        ) -> Result<(), Box<dyn std::error::Error>> {
            // Log summary statistics for the histogram
            self.rec.log(
                format!("{}/count", base_path),
                &rerun::Scalars::new([histogram.total.count as f64]),
            )?;
            self.rec.log(
                format!("{}/mean", base_path),
                &rerun::Scalars::new([histogram.total.mean]),
            )?;
            self.rec.log(
                format!("{}/p50", base_path),
                &rerun::Scalars::new([histogram.total.p50]),
            )?;
            self.rec.log(
                format!("{}/p90", base_path),
                &rerun::Scalars::new([histogram.total.p90]),
            )?;
            self.rec.log(
                format!("{}/p99", base_path),
                &rerun::Scalars::new([histogram.total.p99]),
            )?;
            self.rec.log(
                format!("{}/min", base_path),
                &rerun::Scalars::new([histogram.total.min]),
            )?;
            self.rec.log(
                format!("{}/max", base_path),
                &rerun::Scalars::new([histogram.total.max]),
            )?;

            // Log breakdowns
            for (label_key, label_values) in &histogram.labels {
                for (label_value, nested_histogram) in label_values {
                    let breakdown_path = format!("{}/by_{}/{}", base_path, label_key, sanitize_for_entity_path(label_value));
                    // Recursively log nested histogram
                    self.log_histogram_metric(&breakdown_path, nested_histogram)?;
                }
            }

            Ok(())
        }

        /// Get a reference to the underlying RecordingStream.
        ///
        /// Useful for sharing the stream with other Rerun loggers like blueprint setup.
        pub fn recording_stream(&self) -> &rerun::RecordingStream {
            &self.rec
        }
    }

    /// Truncate a string to a maximum length, adding ellipsis if needed.
    fn truncate_string(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            format!("{}...", &s[..max_len - 3])
        }
    }
}

// ============================================================================
// Stub implementation (when rerun feature is disabled)
// ============================================================================

#[cfg(not(feature = "rerun"))]
mod stub_impl {
    use super::*;

    /// No-op stub for RerunLogger when the rerun feature is disabled.
    /// All methods do nothing but maintain the same API signature.
    pub struct RerunLogger;

    impl RerunLogger {
        /// Create a new RerunLogger (no-op stub).
        /// Always returns an error since rerun is not enabled.
        pub fn new(
            _app_name: &str,
            _nodes: Vec<VisNodeInfo>,
            _links: Vec<VisLinkInfo>,
        ) -> Result<Self, Box<dyn std::error::Error>> {
            Err("rerun feature is not enabled - build with --features rerun".into())
        }

        /// Log a simulation event (no-op).
        pub fn log_event(&mut self, _event: &Event) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }

        /// Log statistics update (no-op).
        pub fn log_stats(
            &self,
            _time: SimTime,
            _total_events: u64,
            _packets_tx: u64,
            _packets_rx: u64,
            _collisions: u64,
        ) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }

        /// Log metrics snapshot (no-op).
        pub fn log_metrics_snapshot(
            &self,
            _time: SimTime,
            _export: &crate::metric_spec::MetricsExport,
        ) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }
    }
}

// Re-export the appropriate implementation
#[cfg(feature = "rerun")]
pub use real_impl::RerunLogger;

#[cfg(not(feature = "rerun"))]
pub use stub_impl::RerunLogger;

// ============================================================================
// RerunMetricsLogger - Real implementation
// ============================================================================

#[cfg(feature = "rerun")]
mod metrics_real_impl {
    use mcsim_metrics::MetricLabels;

    /// Logs metrics to Rerun as time-series Scalars.
    ///
    /// This logger provides methods to log counter, gauge, and histogram metrics
    /// to Rerun visualization, organized by entity path hierarchy:
    /// - `/mcsim/nodes/{node_name}/metrics/{metric_name}` - Per-node metrics
    /// - `/mcsim/groups/{group_name}/metrics/{metric_name}` - Aggregated by group
    /// - `/mcsim/network/{network_name}/metrics/{metric_name}` - Network-wide
    pub struct RerunMetricsLogger {
        rec: rerun::RecordingStream,
    }

    impl RerunMetricsLogger {
        /// Create a new RerunMetricsLogger with the given RecordingStream.
        ///
        /// # Arguments
        /// * `rec` - The Rerun RecordingStream to log metrics to
        pub fn new(rec: rerun::RecordingStream) -> Self {
            Self { rec }
        }

        /// Log a counter metric.
        ///
        /// Entity path: `/mcsim/nodes/{node_name}/metrics/{metric_name}`
        ///
        /// # Arguments
        /// * `metric_name` - The name of the metric (dots are converted to slashes)
        /// * `labels` - The metric labels containing node information
        /// * `value` - The counter value
        /// * `time_us` - The simulation time in microseconds
        pub fn log_counter(
            &self,
            metric_name: &str,
            labels: &MetricLabels,
            value: u64,
            time_us: i64,
        ) {
            let entity_path = format!(
                "/mcsim/nodes/{}/metrics/{}",
                labels.node,
                metric_name.replace('.', "/")
            );

            self.rec
                .set_time("sim_time", std::time::Duration::from_micros(time_us as u64));
            self.rec
                .log(entity_path, &rerun::Scalars::new([value as f64]))
                .ok();
        }

        /// Log a gauge metric.
        ///
        /// Entity path: `/mcsim/nodes/{node_name}/metrics/{metric_name}`
        ///
        /// # Arguments
        /// * `metric_name` - The name of the metric (dots are converted to slashes)
        /// * `labels` - The metric labels containing node information
        /// * `value` - The gauge value
        /// * `time_us` - The simulation time in microseconds
        pub fn log_gauge(
            &self,
            metric_name: &str,
            labels: &MetricLabels,
            value: f64,
            time_us: i64,
        ) {
            let entity_path = format!(
                "/mcsim/nodes/{}/metrics/{}",
                labels.node,
                metric_name.replace('.', "/")
            );

            self.rec
                .set_time("sim_time", std::time::Duration::from_micros(time_us as u64));
            self.rec
                .log(entity_path, &rerun::Scalars::new([value]))
                .ok();
        }

        /// Log a histogram observation.
        ///
        /// Entity path: `/mcsim/nodes/{node_name}/metrics/{metric_name}`
        ///
        /// # Arguments
        /// * `metric_name` - The name of the metric (dots are converted to slashes)
        /// * `labels` - The metric labels containing node information
        /// * `value` - The observed value
        /// * `time_us` - The simulation time in microseconds
        pub fn log_histogram_observation(
            &self,
            metric_name: &str,
            labels: &MetricLabels,
            value: f64,
            time_us: i64,
        ) {
            let entity_path = format!(
                "/mcsim/nodes/{}/metrics/{}",
                labels.node,
                metric_name.replace('.', "/")
            );

            self.rec
                .set_time("sim_time", std::time::Duration::from_micros(time_us as u64));
            self.rec
                .log(entity_path, &rerun::Scalars::new([value]))
                .ok();
        }

        /// Log an aggregated metric for a group.
        ///
        /// Entity path: `/mcsim/groups/{group_name}/metrics/{metric_name}`
        ///
        /// # Arguments
        /// * `metric_name` - The name of the metric (dots are converted to slashes)
        /// * `group_name` - The name of the group
        /// * `value` - The aggregated value
        /// * `time_us` - The simulation time in microseconds
        pub fn log_group_metric(
            &self,
            metric_name: &str,
            group_name: &str,
            value: f64,
            time_us: i64,
        ) {
            let entity_path = format!(
                "/mcsim/groups/{}/metrics/{}",
                group_name,
                metric_name.replace('.', "/")
            );

            self.rec
                .set_time("sim_time", std::time::Duration::from_micros(time_us as u64));
            self.rec
                .log(entity_path, &rerun::Scalars::new([value]))
                .ok();
        }

        /// Log a network-wide aggregate metric.
        ///
        /// Entity path: `/mcsim/network/{network_name}/metrics/{metric_name}`
        ///
        /// # Arguments
        /// * `metric_name` - The name of the metric (dots are converted to slashes)
        /// * `network` - The network/simulation identifier
        /// * `value` - The aggregated value
        /// * `time_us` - The simulation time in microseconds
        pub fn log_network_metric(
            &self,
            metric_name: &str,
            network: &str,
            value: f64,
            time_us: i64,
        ) {
            let entity_path = format!(
                "/mcsim/network/{}/metrics/{}",
                network,
                metric_name.replace('.', "/")
            );

            self.rec
                .set_time("sim_time", std::time::Duration::from_micros(time_us as u64));
            self.rec
                .log(entity_path, &rerun::Scalars::new([value]))
                .ok();
        }

        /// Get a reference to the underlying RecordingStream.
        ///
        /// Useful for sharing the stream with other Rerun loggers.
        pub fn recording_stream(&self) -> &rerun::RecordingStream {
            &self.rec
        }
    }
}

// ============================================================================
// RerunMetricsLogger - Stub implementation
// ============================================================================

#[cfg(not(feature = "rerun"))]
mod metrics_stub_impl {
    use mcsim_metrics::MetricLabels;

    /// No-op stub for RerunMetricsLogger when the rerun feature is disabled.
    /// All methods do nothing but maintain the same API signature.
    pub struct RerunMetricsLogger;

    impl RerunMetricsLogger {
        /// Create a new RerunMetricsLogger (no-op stub).
        ///
        /// Note: This stub does not require a RecordingStream since rerun is disabled.
        /// It exists only for API compatibility.
        pub fn new_stub() -> Self {
            Self
        }

        /// Log a counter metric (no-op).
        pub fn log_counter(
            &self,
            _metric_name: &str,
            _labels: &MetricLabels,
            _value: u64,
            _time_us: i64,
        ) {
        }

        /// Log a gauge metric (no-op).
        pub fn log_gauge(
            &self,
            _metric_name: &str,
            _labels: &MetricLabels,
            _value: f64,
            _time_us: i64,
        ) {
        }

        /// Log a histogram observation (no-op).
        pub fn log_histogram_observation(
            &self,
            _metric_name: &str,
            _labels: &MetricLabels,
            _value: f64,
            _time_us: i64,
        ) {
        }

        /// Log an aggregated metric for a group (no-op).
        pub fn log_group_metric(
            &self,
            _metric_name: &str,
            _group_name: &str,
            _value: f64,
            _time_us: i64,
        ) {
        }

        /// Log a network-wide aggregate metric (no-op).
        pub fn log_network_metric(
            &self,
            _metric_name: &str,
            _network: &str,
            _value: f64,
            _time_us: i64,
        ) {
        }
    }
}

// Re-export RerunMetricsLogger
#[cfg(feature = "rerun")]
pub use metrics_real_impl::RerunMetricsLogger;

#[cfg(not(feature = "rerun"))]
pub use metrics_stub_impl::RerunMetricsLogger;
