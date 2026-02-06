//! # mcsim-common
//!
//! Common types and traits for the MCSim simulation framework.
//!
//! This crate provides core simulation primitives including:
//! - Time representation ([`SimTime`])
//! - Geographic coordinates ([`GeoCoord`])
//! - Entity identification ([`EntityId`], [`NodeId`])
//! - Event system ([`Event`], [`EventPayload`])
//! - Simulation context ([`SimContext`])
//! - Entity traits ([`Entity`])
//! - Entity tracing ([`entity_tracer`])

pub mod entity_tracer;

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

// Re-export meshcore-packet types
pub use meshcore_packet::{Destination, MeshCorePacket, PublicKeyHash};

// ============================================================================
// Error Types
// ============================================================================

/// Simulation errors.
#[derive(Debug, Error)]
pub enum SimError {
    /// Entity not found.
    #[error("Entity not found: {0:?}")]
    EntityNotFound(EntityId),

    /// Invalid event target.
    #[error("Invalid event target: {0:?}")]
    InvalidTarget(EntityId),

    /// Simulation time overflow.
    #[error("Simulation time overflow")]
    TimeOverflow,

    /// Event handler error.
    #[error("Event handler error in entity {entity:?}: {message}")]
    HandlerError {
        /// Entity that had the error.
        entity: EntityId,
        /// Error message.
        message: String,
    },
}

// ============================================================================
// Time Types
// ============================================================================

/// Simulation time in microseconds since simulation start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct SimTime(u64);

impl SimTime {
    /// Zero time.
    pub const ZERO: SimTime = SimTime(0);

    /// Create from microseconds.
    pub fn from_micros(us: u64) -> Self {
        SimTime(us)
    }

    /// Create from milliseconds.
    pub fn from_millis(ms: u64) -> Self {
        SimTime(ms * 1000)
    }

    /// Create from seconds (float).
    pub fn from_secs(s: f64) -> Self {
        SimTime((s * 1_000_000.0) as u64)
    }

    /// Get as microseconds.
    pub fn as_micros(&self) -> u64 {
        self.0
    }

    /// Get as milliseconds.
    pub fn as_millis(&self) -> u64 {
        self.0 / 1000
    }

    /// Get as seconds (float).
    pub fn as_secs_f64(&self) -> f64 {
        self.0 as f64 / 1_000_000.0
    }

    /// Add duration to time.
    pub fn add(&self, duration: SimTime) -> Option<SimTime> {
        self.0.checked_add(duration.0).map(SimTime)
    }

    /// Subtract duration from time.
    pub fn sub(&self, duration: SimTime) -> Option<SimTime> {
        self.0.checked_sub(duration.0).map(SimTime)
    }
}

impl std::ops::Add for SimTime {
    type Output = SimTime;

    fn add(self, rhs: Self) -> Self::Output {
        SimTime(self.0 + rhs.0)
    }
}

impl std::ops::Sub for SimTime {
    type Output = SimTime;

    fn sub(self, rhs: Self) -> Self::Output {
        SimTime(self.0.saturating_sub(rhs.0))
    }
}

// ============================================================================
// Geographic Types
// ============================================================================

/// Geographic coordinate.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GeoCoord {
    /// Latitude in degrees.
    pub latitude: f64,
    /// Longitude in degrees.
    pub longitude: f64,
    /// Altitude in meters (optional).
    pub altitude_m: Option<f64>,
}

impl GeoCoord {
    /// Create a new coordinate.
    pub fn new(latitude: f64, longitude: f64) -> Self {
        GeoCoord {
            latitude,
            longitude,
            altitude_m: None,
        }
    }

    /// Create a new coordinate with altitude.
    pub fn with_altitude(latitude: f64, longitude: f64, altitude_m: f64) -> Self {
        GeoCoord {
            latitude,
            longitude,
            altitude_m: Some(altitude_m),
        }
    }

    /// Calculate distance to another coordinate in meters.
    /// Uses the Haversine formula.
    pub fn distance_to(&self, other: &GeoCoord) -> f64 {
        const EARTH_RADIUS_M: f64 = 6_371_000.0;

        let lat1 = self.latitude.to_radians();
        let lat2 = other.latitude.to_radians();
        let dlat = (other.latitude - self.latitude).to_radians();
        let dlon = (other.longitude - self.longitude).to_radians();

        let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().asin();

        EARTH_RADIUS_M * c
    }
}

// ============================================================================
// Entity Types
// ============================================================================

/// Unique identifier for an entity in the simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EntityId(pub u64);

impl EntityId {
    /// Create a new entity ID.
    pub fn new(id: u64) -> Self {
        EntityId(id)
    }
}

/// Node identifier (public key hash).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub [u8; 32]);

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Display first 6 bytes as hex (public key hash)
        for byte in &self.0[..6] {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

impl NodeId {
    /// Create a new node ID from bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        NodeId(bytes)
    }

    /// Get the public key hash (first 6 bytes).
    pub fn public_key_hash(&self) -> PublicKeyHash {
        let mut hash = [0u8; 6];
        hash.copy_from_slice(&self.0[..6]);
        hash
    }
}

// ============================================================================
// Event Types
// ============================================================================

/// Unique identifier for an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(pub u64);

/// A simulation event.
#[derive(Debug, Clone)]
pub struct Event {
    /// Unique event ID.
    pub id: EventId,
    /// Time when the event occurs.
    pub time: SimTime,
    /// Entity that created the event.
    pub source: EntityId,
    /// Target entities for the event.
    pub targets: Vec<EntityId>,
    /// Event payload.
    pub payload: EventPayload,
}

impl PartialEq for Event {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Event {}

impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Event {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse ordering for min-heap (earliest time first)
        other.time.cmp(&self.time).then_with(|| other.id.0.cmp(&self.id.0))
    }
}

/// LoRa packet wrapper with optional pre-decoded MeshCore packet.
///
/// The decoded packet is computed lazily on first access and cached for reuse.
/// This allows metrics and logging to access packet-type information without
/// repeated decode operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoraPacket {
    /// Raw payload bytes.
    pub payload: Vec<u8>,
    /// Pre-decoded MeshCore packet (computed lazily, skipped in serialization).
    #[serde(skip)]
    decoded: Option<meshcore_packet::MeshCorePacket>,
}

impl LoraPacket {
    /// Create a new LoRa packet from raw bytes.
    ///
    /// The MeshCore packet is decoded eagerly for efficient access.
    pub fn new(payload: Vec<u8>) -> Self {
        let decoded = meshcore_packet::MeshCorePacket::decode(&payload).ok();
        Self { payload, decoded }
    }

    /// Create a LoRa packet from raw bytes without decoding.
    ///
    /// Use this when you know the packet won't need to be decoded (e.g., random test data).
    pub fn from_bytes(payload: Vec<u8>) -> Self {
        Self {
            payload,
            decoded: None,
        }
    }

    /// Get the pre-decoded MeshCore packet, if decode succeeded.
    pub fn decoded(&self) -> Option<&meshcore_packet::MeshCorePacket> {
        self.decoded.as_ref()
    }

    /// Ensure the packet is decoded, decoding if necessary.
    ///
    /// Returns the decoded packet, or None if decoding fails.
    pub fn ensure_decoded(&mut self) -> Option<&meshcore_packet::MeshCorePacket> {
        if self.decoded.is_none() {
            self.decoded = meshcore_packet::MeshCorePacket::decode(&self.payload).ok();
        }
        self.decoded.as_ref()
    }

    /// Get the payload type label for metrics.
    ///
    /// Returns "unknown" if the packet could not be decoded.
    pub fn payload_type_label(&self) -> &'static str {
        self.decoded
            .as_ref()
            .map(|p| p.payload_type().as_label())
            .unwrap_or("unknown")
    }

    /// Get the route type label for metrics.
    ///
    /// Returns "unknown" if the packet could not be decoded.
    pub fn route_type_label(&self) -> &'static str {
        self.decoded
            .as_ref()
            .map(|p| p.route_type().as_label())
            .unwrap_or("unknown")
    }

    /// Check if this is a flood-routed packet.
    ///
    /// Returns false if the packet could not be decoded.
    pub fn is_flood(&self) -> bool {
        self.decoded.as_ref().map(|p| p.is_flood()).unwrap_or(false)
    }

    /// Check if this is a direct-routed packet.
    ///
    /// Returns false if the packet could not be decoded.
    pub fn is_direct(&self) -> bool {
        self.decoded.as_ref().map(|p| p.is_direct()).unwrap_or(false)
    }

    /// Get the payload hash label for metrics.
    ///
    /// Returns a 16-character uppercase hex string identifying this packet's payload.
    /// Returns "unknown" if the packet could not be decoded.
    pub fn payload_hash_label(&self) -> String {
        self.decoded
            .as_ref()
            .map(|p| p.payload_hash_label().as_label())
            .unwrap_or_else(|| "unknown".to_string())
    }
}

/// LoRa radio parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioParams {
    /// Frequency in Hz.
    pub frequency_hz: u32,
    /// Bandwidth in Hz.
    pub bandwidth_hz: u32,
    /// Spreading factor (7-12).
    pub spreading_factor: u8,
    /// Coding rate (5-8, representing 4/5 to 4/8).
    pub coding_rate: u8,
    /// Transmit power in dBm.
    pub tx_power_dbm: i8,
}

/// Transmit air event - broadcast when a radio begins transmission.
/// Directed to a Graph entity which routes to appropriate receivers.
#[derive(Debug, Clone)]
pub struct TransmitAirEvent {
    /// Radio that is transmitting.
    pub radio_id: EntityId,
    /// The packet being transmitted.
    pub packet: LoraPacket,
    /// Radio parameters for this transmission.
    pub params: RadioParams,
    /// When transmission will end.
    pub end_time: SimTime,
}

/// Receive air event - sent from Graph entity to receiving radios.
/// Contains link model parameters for SNR sampling.
#[derive(Debug, Clone)]
pub struct ReceiveAirEvent {
    /// Radio that transmitted.
    pub source_radio_id: EntityId,
    /// The packet being received.
    pub packet: LoraPacket,
    /// Radio parameters for this transmission.
    pub params: RadioParams,
    /// When transmission will end.
    pub end_time: SimTime,
    /// Mean signal-to-noise ratio in dB at 20 dBm TX power (from link model).
    pub mean_snr_db_at20dbm: f64,
    /// Standard deviation of SNR in dB for Gaussian variation.
    pub snr_std_dev: f64,
    /// Received signal strength in dBm (from link model).
    pub rssi_dbm: f64,
}

/// States visible to firmware via RadioStateChangedEvent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RadioState {
    /// Ready to receive packets, can start TX.
    Receiving,
    /// Actively transmitting, cannot receive.
    Transmitting,
}

/// Radio has a packet for firmware (includes all reception outcomes).
/// Radio → Firmware event.
#[derive(Debug, Clone)]
pub struct RadioRxPacketEvent {
    /// The received packet.
    pub packet: LoraPacket,
    /// Radio entity ID of the sender.
    pub source_radio_id: EntityId,
    /// Signal-to-noise ratio in dB.
    pub snr_db: f64,
    /// Received signal strength in dBm.
    pub rssi_dbm: f64,
    /// Whether the packet was damaged by collision.
    pub was_collided: bool,
    /// Whether the packet was too weak (SNR below threshold).
    pub was_weak_signal: bool,
    /// When the packet transmission started.
    pub start_time: SimTime,
    /// When the packet transmission ended (reception complete).
    pub end_time: SimTime,
}

/// Radio state machine changed - covers all state transitions.
/// Radio → Firmware event.
/// 
/// Handles all state transitions including:
/// - TX started (state → TRANSMITTING)
/// - TX complete (state → RX_TURNAROUND or RECEIVING)  
/// - Turnaround complete (state → RECEIVING or TRANSMITTING)
#[derive(Debug, Clone)]
pub struct RadioStateChangedEvent {
    /// The new radio state.
    pub new_state: RadioState,
    /// Incremented on each state change for ordering.
    pub state_version: u32,
}

/// Firmware requests radio to transmit a packet.
/// Firmware → Radio event.
#[derive(Debug, Clone)]
pub struct RadioTxRequestEvent {
    /// The packet to transmit.
    pub packet: LoraPacket,
}

/// Message send event data.
#[derive(Debug, Clone)]
pub struct MessageSendEvent {
    /// Message destination.
    pub destination: Destination,
    /// Message content.
    pub content: String,
    /// Whether to request an acknowledgment.
    pub want_ack: bool,
}

/// Message received event data.
#[derive(Debug, Clone)]
pub struct MessageReceivedEvent {
    /// The received packet.
    pub packet: MeshCorePacket,
    /// Sender node ID.
    pub from: NodeId,
}

/// Message acknowledged event data.
#[derive(Debug, Clone)]
pub struct MessageAckedEvent {
    /// Hash of the original packet.
    pub original_hash: [u8; 6],
    /// Number of hops.
    pub hop_count: u8,
}

/// Event payload variants.
#[derive(Debug, Clone)]
pub enum EventPayload {
    // =========== Radio Layer Events ===========
    /// A radio started transmitting (directed to Graph entity).
    TransmitAir(TransmitAirEvent),
    /// A packet is being received (from Graph entity to receiver).
    ReceiveAir(ReceiveAirEvent),

    // =========== Radio → Firmware Events ===========
    /// Radio has a packet for firmware.
    RadioRxPacket(RadioRxPacketEvent),
    /// Radio state changed (TX start, TX complete, RX ready, etc.).
    RadioStateChanged(RadioStateChangedEvent),

    // =========== Firmware → Radio Events ===========
    /// Firmware requests transmission.
    RadioTxRequest(RadioTxRequestEvent),

    // =========== Serial/UART Events ===========
    /// Serial data received from external source (e.g., TCP client).
    SerialRx(SerialRxEvent),
    /// Serial data to be sent to external source (e.g., TCP client).
    SerialTx(SerialTxEvent),

    // =========== Agent Layer Events ===========
    /// Request to send a message.
    MessageSend(MessageSendEvent),
    /// A message was received.
    MessageReceived(MessageReceivedEvent),
    /// A message was acknowledged.
    MessageAcknowledged(MessageAckedEvent),

    // =========== Scheduling ===========
    /// A delayed callback.
    Timer {
        /// User-defined timer ID.
        timer_id: u64,
    },

    // =========== Simulation Control ===========
    /// End the simulation.
    SimulationEnd,
}

/// Serial data received from external source.
#[derive(Debug, Clone)]
pub struct SerialRxEvent {
    /// The received data.
    pub data: Vec<u8>,
}

/// Serial data to be sent to external source.
#[derive(Debug, Clone)]
pub struct SerialTxEvent {
    /// The data to send.
    pub data: Vec<u8>,
}

// ============================================================================
// Simulation Context
// ============================================================================

use crate::entity_tracer::EntityTracer;

/// Context passed to entities during event handling.
pub struct SimContext {
    time: SimTime,
    rng: ChaCha8Rng,
    pending_events: Vec<Event>,
    next_event_id: u64,
    source_entity: EntityId,
    tracer: EntityTracer,
}

impl SimContext {
    /// Create a new simulation context.
    pub fn new(seed: u64) -> Self {
        SimContext {
            time: SimTime::ZERO,
            rng: ChaCha8Rng::seed_from_u64(seed),
            pending_events: Vec::new(),
            next_event_id: 0,
            source_entity: EntityId(0),
            tracer: EntityTracer::disabled(),
        }
    }

    /// Create a new simulation context with a tracer.
    pub fn with_tracer(seed: u64, tracer: EntityTracer) -> Self {
        SimContext {
            time: SimTime::ZERO,
            rng: ChaCha8Rng::seed_from_u64(seed),
            pending_events: Vec::new(),
            next_event_id: 0,
            source_entity: EntityId(0),
            tracer,
        }
    }

    /// Get the current simulation time.
    pub fn time(&self) -> SimTime {
        self.time
    }

    /// Get mutable access to the random number generator.
    pub fn rng(&mut self) -> &mut ChaCha8Rng {
        &mut self.rng
    }

    /// Set the current time (used by event loop).
    pub fn set_time(&mut self, time: SimTime) {
        self.time = time;
    }

    /// Set the source entity (used by event loop).
    pub fn set_source(&mut self, entity: EntityId) {
        self.source_entity = entity;
    }

    /// Get the entity tracer.
    pub fn tracer(&self) -> &EntityTracer {
        &self.tracer
    }

    /// Post an event to occur after a delay.
    pub fn post_event(&mut self, delay: SimTime, targets: Vec<EntityId>, payload: EventPayload) {
        let event = Event {
            id: EventId(self.next_event_id),
            time: self.time + delay,
            source: self.source_entity,
            targets,
            payload,
        };
        self.next_event_id += 1;
        self.pending_events.push(event);
    }

    /// Post an event to occur immediately (at current time).
    pub fn post_immediate(&mut self, targets: Vec<EntityId>, payload: EventPayload) {
        self.post_event(SimTime::ZERO, targets, payload);
    }

    /// Take all pending events (used by event loop).
    pub fn take_pending_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.pending_events)
    }

    /// Get the next event ID (used by event loop for external event creation).
    pub fn next_event_id(&mut self) -> u64 {
        let id = self.next_event_id;
        self.next_event_id += 1;
        id
    }
}

// ============================================================================
// Entity Trait
// ============================================================================

/// Base trait for all simulation entities.
pub trait Entity: Send {
    /// Get the entity's unique ID.
    fn entity_id(&self) -> EntityId;

    /// Handle an event.
    fn handle_event(&mut self, event: &Event, ctx: &mut SimContext) -> Result<(), SimError>;
}

// ============================================================================
// Entity Registry
// ============================================================================

/// Registry for managing simulation entities.
pub struct EntityRegistry {
    entities: HashMap<EntityId, Box<dyn Entity>>,
}

impl EntityRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        EntityRegistry {
            entities: HashMap::new(),
        }
    }

    /// Register an entity.
    pub fn register(&mut self, entity: Box<dyn Entity>) {
        let id = entity.entity_id();
        self.entities.insert(id, entity);
    }

    /// Get an entity by ID.
    pub fn get(&self, id: EntityId) -> Option<&dyn Entity> {
        self.entities.get(&id).map(|e| e.as_ref())
    }

    /// Get a mutable reference to an entity by ID.
    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut Box<dyn Entity>> {
        self.entities.get_mut(&id)
    }

    /// Dispatch an event to its target entities.
    pub fn dispatch_event(&mut self, event: &Event, ctx: &mut SimContext) -> Result<(), SimError> {
        for target in &event.targets {
            if let Some(entity) = self.entities.get_mut(target) {
                ctx.set_source(*target);
                entity.handle_event(event, ctx)?;
            } else {
                eprintln!("ERROR: EntityNotFound {:?} when dispatching {:?}", target, event.payload);
                return Err(SimError::EntityNotFound(*target));
            }
        }
        Ok(())
    }

    /// Get all entity IDs.
    pub fn entity_ids(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.entities.keys().copied()
    }

    /// Get the number of registered entities.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }
}

impl Default for EntityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sim_time_conversions() {
        let time = SimTime::from_secs(1.5);
        assert_eq!(time.as_millis(), 1500);
        assert_eq!(time.as_micros(), 1_500_000);
        assert!((time.as_secs_f64() - 1.5).abs() < 0.0001);
    }

    #[test]
    fn test_sim_time_arithmetic() {
        let t1 = SimTime::from_millis(100);
        let t2 = SimTime::from_millis(50);
        assert_eq!((t1 + t2).as_millis(), 150);
        assert_eq!((t1 - t2).as_millis(), 50);
    }

    #[test]
    fn test_geo_coord_distance() {
        let sf = GeoCoord::new(37.7749, -122.4194);
        let la = GeoCoord::new(34.0522, -118.2437);
        let distance = sf.distance_to(&la);
        // SF to LA is approximately 559 km
        assert!(distance > 550_000.0 && distance < 570_000.0);
    }
}
