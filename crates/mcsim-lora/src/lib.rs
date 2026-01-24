//! # mcsim-lora
//!
//! LoRa radio simulation for MCSim.
//!
//! This crate provides:
//! - Radio parameter configuration ([`RadioParams`])
//! - LoRa packet representation ([`LoraPacket`])
//! - Radio entity simulation ([`Radio`])
//! - Link model for signal propagation ([`LinkModel`])
//! - Collision detection ([`check_collision`])
//! - PHY calculations ([`calculate_time_on_air`], [`calculate_snr_sensitivity`])
//! - Configurable PHY parameters ([`LoraPhyConfig`])

use mcsim_common::{
    Entity, EntityId, Event, EventPayload, GeoCoord, SimContext, SimError,
    SimTime,
};
use mcsim_metrics::{metric_defs, metrics, MetricLabels};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// Re-export common types
pub use mcsim_common::LoraPacket;
pub use mcsim_common::RadioParams;

// ============================================================================
// Radio Parameters Extension
// ============================================================================

/// Extension trait for RadioParams to add LoRa-specific calculations.
pub trait RadioParamsExt {
    /// Create default MeshCore radio parameters.
    fn default_meshcore() -> mcsim_common::RadioParams;
    
    /// Calculate time on air for a given payload length.
    fn time_on_air(&self, payload_len: usize) -> SimTime;
}

impl RadioParamsExt for mcsim_common::RadioParams {
    fn default_meshcore() -> mcsim_common::RadioParams {
        mcsim_common::RadioParams {
            frequency_hz: 910_525_000,
            bandwidth_hz: 62_500,
            spreading_factor: 7,
            coding_rate: 5,
            tx_power_dbm: 20,
        }
    }

    fn time_on_air(&self, payload_len: usize) -> SimTime {
        calculate_time_on_air(self, payload_len)
    }
}

/// Create default MeshCore radio parameters.
pub fn default_radio_params() -> RadioParams {
    RadioParams {
        frequency_hz: 910_525_000,
        bandwidth_hz: 62_500,
        spreading_factor: 7,
        coding_rate: 5,
        tx_power_dbm: 20,
    }
}

// ============================================================================
// PHY Calculations
// ============================================================================

/// Configuration for LoRa PHY calculations.
///
/// This struct holds configurable parameters that were previously hardcoded.
/// The default values match the property defaults in `mcsim_model::properties`:
/// - `lora/preamble_symbols` (default: 8)
/// - `radio/snr_threshold_sf7_db` through `radio/snr_threshold_sf12_db`
///
/// Callers with access to simulation properties can construct this struct
/// by reading those properties and passing values to [`LoraPhyConfig::new()`].
///
/// # Example
///
/// ```ignore
/// use mcsim_lora::LoraPhyConfig;
///
/// // Use defaults (matches property defaults)
/// let config = LoraPhyConfig::default();
///
/// // Or construct from properties (in mcsim-model or mcsim-runner):
/// let config = LoraPhyConfig::new(
///     props.get(&LORA_PREAMBLE_SYMBOLS),
///     [
///         props.get(&RADIO_SNR_THRESHOLD_SF7_DB),
///         props.get(&RADIO_SNR_THRESHOLD_SF8_DB),
///         props.get(&RADIO_SNR_THRESHOLD_SF9_DB),
///         props.get(&RADIO_SNR_THRESHOLD_SF10_DB),
///         props.get(&RADIO_SNR_THRESHOLD_SF11_DB),
///         props.get(&RADIO_SNR_THRESHOLD_SF12_DB),
///     ],
/// );
/// ```
#[derive(Debug, Clone)]
pub struct LoraPhyConfig {
    /// Number of preamble symbols.
    /// Default: 8 (matches `lora/preamble_symbols` property default).
    pub preamble_symbols: u32,
    /// SNR thresholds for spreading factors 7-12, indexed as [SF-7].
    /// Default values: [-7.5, -10.0, -12.5, -15.0, -17.5, -20.0]
    /// (matches `radio/snr_threshold_sf*_db` property defaults).
    pub snr_thresholds: [f64; 6],
}

impl LoraPhyConfig {
    /// Default preamble symbol count (matches `lora/preamble_symbols` property default).
    pub const DEFAULT_PREAMBLE_SYMBOLS: u32 = 8;
    
    /// Default SNR thresholds for SF 7-12 (matches `radio/snr_threshold_sf*_db` property defaults).
    pub const DEFAULT_SNR_THRESHOLDS: [f64; 6] = [-7.5, -10.0, -12.5, -15.0, -17.5, -20.0];

    /// Create a new LoraPhyConfig with explicit values.
    ///
    /// Use this to pass values loaded from simulation properties.
    pub fn new(preamble_symbols: u32, snr_thresholds: [f64; 6]) -> Self {
        Self {
            preamble_symbols,
            snr_thresholds,
        }
    }

    /// Get the SNR threshold for a given spreading factor.
    ///
    /// Returns the threshold for SF 7-12. For invalid spreading factors,
    /// returns the SF8 threshold as a reasonable default.
    pub fn snr_threshold(&self, spreading_factor: u8) -> f64 {
        match spreading_factor {
            7..=12 => self.snr_thresholds[(spreading_factor - 7) as usize],
            _ => self.snr_thresholds[1], // SF8 default for invalid SF
        }
    }
}

impl Default for LoraPhyConfig {
    fn default() -> Self {
        Self {
            preamble_symbols: Self::DEFAULT_PREAMBLE_SYMBOLS,
            snr_thresholds: Self::DEFAULT_SNR_THRESHOLDS,
        }
    }
}

/// Calculate the time on air for a LoRa packet.
///
/// Uses the LoRa time on air formula based on spreading factor,
/// bandwidth, and coding rate.
///
/// This function uses the default preamble symbols (8). For configurable
/// preamble, use [`calculate_time_on_air_with_config()`].
pub fn calculate_time_on_air(params: &RadioParams, payload_len: usize) -> SimTime {
    calculate_time_on_air_with_config(params, payload_len, &LoraPhyConfig::default())
}

/// Calculate the time on air for a LoRa packet with configurable PHY parameters.
///
/// Uses the LoRa time on air formula based on spreading factor,
/// bandwidth, coding rate, and preamble symbols from the config.
pub fn calculate_time_on_air_with_config(
    params: &RadioParams,
    payload_len: usize,
    config: &LoraPhyConfig,
) -> SimTime {
    // Simplified LoRa time on air calculation
    let sf = params.spreading_factor as f64;
    let bw = params.bandwidth_hz as f64;
    let cr = params.coding_rate as f64;

    // Symbol time in seconds
    let t_sym = (2.0_f64.powf(sf)) / bw;

    // Preamble symbols + 4.25 (sync word and start frame delimiter)
    let n_preamble = config.preamble_symbols as f64 + 4.25;

    // Payload symbols (simplified calculation)
    let pl = payload_len as f64;
    let payload_symbols = 8.0 + ((8.0 * pl - 4.0 * sf + 28.0).max(0.0) / (4.0 * sf)).ceil() * cr;

    let total_symbols = n_preamble + payload_symbols;
    let time_seconds = total_symbols * t_sym;

    SimTime::from_secs(time_seconds)
}

/// Calculate the SNR sensitivity threshold for a spreading factor.
///
/// This function uses the default SNR thresholds. For configurable
/// thresholds, use [`calculate_snr_sensitivity_with_config()`] or
/// [`LoraPhyConfig::snr_threshold()`].
pub fn calculate_snr_sensitivity(spreading_factor: u8) -> f64 {
    LoraPhyConfig::default().snr_threshold(spreading_factor)
}

/// Calculate the SNR sensitivity threshold for a spreading factor with config.
///
/// This allows using property-based SNR thresholds instead of defaults.
pub fn calculate_snr_sensitivity_with_config(spreading_factor: u8, config: &LoraPhyConfig) -> f64 {
    config.snr_threshold(spreading_factor)
}

/// Minimum SNR separation required for capture effect (in dB).
/// If one signal is at least this much stronger than another, it survives.
pub const CAPTURE_EFFECT_THRESHOLD_DB: f64 = 6.0;

/// Sample a value from a Gaussian (normal) distribution.
/// Uses the Box-Muller transform for deterministic simulation.
pub fn sample_gaussian<R: Rng>(rng: &mut R, mean: f64, std_dev: f64) -> f64 {
    // Box-Muller transform
    let u1: f64 = rng.gen();
    let u2: f64 = rng.gen();
    
    // Avoid log(0)
    let u1 = if u1 == 0.0 { f64::MIN_POSITIVE } else { u1 };
    
    let z0 = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    mean + std_dev * z0
}

// ============================================================================
// Collision Detection
// ============================================================================

/// Context for collision detection.
#[derive(Debug, Clone)]
pub struct CollisionContext {
    /// Start time of transmission.
    pub start_time: SimTime,
    /// End time of transmission.
    pub end_time: SimTime,
    /// Frequency in Hz.
    pub frequency_hz: u32,
    /// Unique packet ID.
    pub packet_id: u64,
}

/// Result of collision check.
#[derive(Debug, Clone, PartialEq)]
pub enum CollisionResult {
    /// No collision detected.
    NoCollision,
    /// Incoming packet is destroyed.
    Destroyed,
    /// Both packets are destroyed.
    BothDestroyed(u64),
    /// Capture effect: incoming survives, other is destroyed.
    CaptureEffect(u64),
}

/// Check for collision between an incoming transmission and existing ones.
pub fn check_collision(
    incoming: &CollisionContext,
    existing: &[CollisionContext],
) -> CollisionResult {
    for other in existing {
        // Check frequency overlap (simplified: exact match required)
        if incoming.frequency_hz != other.frequency_hz {
            continue;
        }

        // Check time overlap
        let overlap = incoming.start_time < other.end_time && incoming.end_time > other.start_time;

        if overlap {
            // Simplified collision model: both destroyed
            return CollisionResult::BothDestroyed(other.packet_id);
        }
    }

    CollisionResult::NoCollision
}

// ============================================================================
// Link Model
// ============================================================================

/// Parameters for a radio link between two nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkParams {
    /// Mean signal-to-noise ratio in dB (at 20 dBm TX power).
    pub mean_snr_db_at20dbm: f64,
    /// Standard deviation of SNR in dB (for Gaussian variation).
    pub snr_std_dev: f64,
    /// Received signal strength in dBm.
    pub rssi_dbm: f64,
}

/// Model of radio links between nodes.
/// 
/// Uses BTreeMap for deterministic iteration order, which is critical
/// for simulation reproducibility.
#[derive(Clone)]
pub struct LinkModel {
    edges: BTreeMap<(EntityId, EntityId), LinkParams>,
}

impl LinkModel {
    /// Create a new empty link model.
    pub fn new() -> Self {
        LinkModel {
            edges: BTreeMap::new(),
        }
    }

    /// Add a directed link between two radios.
    pub fn add_link(&mut self, from: EntityId, to: EntityId, mean_snr_db_at20dbm: f64, snr_std_dev: f64, rssi_dbm: f64) {
        self.edges.insert(
            (from, to),
            LinkParams { mean_snr_db_at20dbm, snr_std_dev, rssi_dbm },
        );
    }

    /// Add a link using LinkParams.
    pub fn add_edge(&mut self, from: EntityId, to: EntityId, params: LinkParams) {
        self.edges.insert((from, to), params);
    }

    /// Get link parameters between two radios.
    pub fn get_link(&self, from: EntityId, to: EntityId) -> Option<&LinkParams> {
        self.edges.get(&(from, to))
    }

    /// Get all radios that can receive from a given transmitter.
    /// 
    /// Returns receivers in deterministic order (sorted by EntityId) since
    /// BTreeMap iteration is ordered. This is critical for simulation
    /// determinism since the order of ReceiveAir events affects collision
    /// detection outcomes.
    pub fn get_receivers(&self, from: EntityId) -> impl Iterator<Item = (EntityId, &LinkParams)> {
        self.edges
            .iter()
            .filter(move |((f, _), _)| *f == from)
            .map(|((_, to), params)| (*to, params))
    }
}

impl Default for LinkModel {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Radio Entity
// ============================================================================

/// Timer ID constants for the Radio entity.
/// We encode timer type in the timer_id field.
const TIMER_TX_TURNAROUND_COMPLETE: u64 = 1;
const TIMER_RX_TURNAROUND_COMPLETE: u64 = 2;
const TIMER_RX_COMPLETE_BASE: u64 = 0x1000; // reception_id is added to this

/// State of an active reception.
#[derive(Debug, Clone)]
struct ActiveReception {
    /// The packet being received.
    packet: LoraPacket,
    /// Source radio entity ID.
    source_radio_id: EntityId,
    /// Start time of reception.
    start_time: SimTime,
    /// End time of reception.
    end_time: SimTime,
    /// Signal-to-noise ratio in dB.
    snr_db: f64,
    /// Received signal strength in dBm.
    rssi_dbm: f64,
    /// Whether this packet was damaged by collision.
    collided: bool,
    /// Unique ID for this reception (for timer tracking).
    reception_id: u64,
}

/// Radio configuration including turnaround times.
#[derive(Debug, Clone)]
pub struct RadioConfig {
    /// LoRa radio parameters.
    pub params: RadioParams,
    /// Time to switch from RX to TX mode.
    pub rx_to_tx_turnaround: SimTime,
    /// Time to switch from TX to RX mode.
    pub tx_to_rx_turnaround: SimTime,
    /// Entity ID of the Graph entity (for routing transmissions).
    pub graph_entity: EntityId,
}

impl Default for RadioConfig {
    fn default() -> Self {
        RadioConfig {
            params: RadioParams {
                frequency_hz: 910_525_000,
                bandwidth_hz: 62_500,
                spreading_factor: 7,
                coding_rate: 5,
                tx_power_dbm: 20,
            },
            rx_to_tx_turnaround: SimTime::from_micros(100),
            tx_to_rx_turnaround: SimTime::from_micros(100),
            graph_entity: EntityId::new(0),
        }
    }
}

/// Internal state of the radio state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InternalRadioState {
    /// Ready to receive packets.
    Receiving,
    /// Switching from RX to TX.
    TxTurnaround,
    /// Actively transmitting.
    Transmitting,
}

/// Radio entity for LoRa simulation.
///
/// The Radio Entity lives in the coordinator and is responsible for:
/// 1. Transmission initiation - Sends TransmitAir events to the Graph Entity
/// 2. Reception handling - Receives ReceiveAir events from the Graph Entity
/// 3. Collision detection - Tracks all active receptions and determines collisions
/// 4. Reception decisions - Based on SNR, RSSI, and collision state
/// 5. State synchronization - Notifies Firmware Entity when state changes
pub struct Radio {
    id: EntityId,
    config: RadioConfig,
    position: GeoCoord,
    attached_firmware: EntityId,

    // Internal state machine
    state: InternalRadioState,
    /// State version - incremented on any state change visible to firmware.
    state_version: u32,

    // Transmission state
    /// Pending TX request while in turnaround.
    pending_tx: Option<LoraPacket>,
    /// Current transmission (when in Transmitting state).
    current_tx: Option<(LoraPacket, SimTime)>, // packet, end_time
    
    // Reception state
    /// Active receptions (packets currently being received).
    active_receptions: Vec<ActiveReception>,
    /// Counter for unique reception IDs.
    next_reception_id: u64,

    // Metrics
    /// Labels for emitting metrics.
    metric_labels: MetricLabels,
    /// Time of last state change (for turnaround time tracking).
    last_state_change_time: SimTime,
}

impl Radio {
    /// Create a new radio entity.
    pub fn new(
        id: EntityId,
        config: RadioConfig,
        position: GeoCoord,
        attached_firmware: EntityId,
        metric_labels: MetricLabels,
    ) -> Self {
        Radio {
            id,
            config,
            position,
            attached_firmware,
            state: InternalRadioState::Receiving,
            state_version: 0,
            pending_tx: None,
            current_tx: None,
            active_receptions: Vec::new(),
            next_reception_id: 0,
            metric_labels,
            last_state_change_time: SimTime::ZERO,
        }
    }

    /// Create a new radio entity with basic parameters (legacy API).
    pub fn with_params(
        id: EntityId,
        params: RadioParams,
        position: GeoCoord,
        attached_firmware: EntityId,
    ) -> Self {
        // Create default metric labels for legacy API
        let metric_labels = MetricLabels::new(
            format!("radio_{}", id.0),
            "radio",
        );
        Self::new(
            id,
            RadioConfig {
                params,
                ..Default::default()
            },
            position,
            attached_firmware,
            metric_labels,
        )
    }

    /// Get the metric labels for this radio.
    pub fn metric_labels(&self) -> &MetricLabels {
        &self.metric_labels
    }

    /// Get the radio parameters.
    pub fn params(&self) -> &RadioParams {
        &self.config.params
    }

    /// Get the radio configuration.
    pub fn config(&self) -> &RadioConfig {
        &self.config
    }

    /// Get the radio position.
    pub fn position(&self) -> GeoCoord {
        self.position
    }

    /// Get the attached firmware entity ID.
    pub fn attached_firmware(&self) -> EntityId {
        self.attached_firmware
    }

    /// Check if the radio is currently transmitting.
    pub fn is_transmitting(&self) -> bool {
        matches!(self.state, InternalRadioState::Transmitting | InternalRadioState::TxTurnaround)
    }

    /// Get the current state version.
    pub fn state_version(&self) -> u32 {
        self.state_version
    }

    /// Set the graph entity ID for routing transmissions.
    pub fn set_graph_entity(&mut self, graph_entity: EntityId) {
        self.config.graph_entity = graph_entity;
    }

    /// Notify firmware of state change.
    fn notify_state_change(&mut self, ctx: &mut SimContext, new_state: mcsim_common::RadioState) {
        self.state_version += 1;
        ctx.post_immediate(
            vec![self.attached_firmware],
            EventPayload::RadioStateChanged(mcsim_common::RadioStateChangedEvent {
                new_state,
                state_version: self.state_version,
            }),
        );
    }

    /// Check for collisions among active receptions with capture effect.
    /// 
    /// When two packets collide in time, we check for capture effect:
    /// - If one signal is at least 6 dB stronger than the other, it survives (capture effect)
    /// - Otherwise, both packets are destroyed
    fn check_collisions_with_capture(&mut self) {
        // Mark collided packets based on time overlap and capture effect
        for i in 0..self.active_receptions.len() {
            for j in (i + 1)..self.active_receptions.len() {
                let a = &self.active_receptions[i];
                let b = &self.active_receptions[j];
                
                // Check time overlap
                if a.start_time < b.end_time && a.end_time > b.start_time {
                    // Packets overlap - check for capture effect
                    let snr_diff = a.snr_db - b.snr_db;
                    
                    if snr_diff >= CAPTURE_EFFECT_THRESHOLD_DB {
                        // Packet a is significantly stronger - it survives, b is destroyed
                        self.active_receptions[j].collided = true;
                    } else if snr_diff <= -CAPTURE_EFFECT_THRESHOLD_DB {
                        // Packet b is significantly stronger - it survives, a is destroyed
                        self.active_receptions[i].collided = true;
                    } else {
                        // Neither is significantly stronger - both are destroyed
                        self.active_receptions[i].collided = true;
                        self.active_receptions[j].collided = true;
                    }
                }
            }
        }
    }

    /// Handle TX request from firmware.
    fn handle_tx_request(&mut self, packet: LoraPacket, ctx: &mut SimContext) {
        match self.state {
            InternalRadioState::Receiving => {
                // Start TX turnaround
                self.state = InternalRadioState::TxTurnaround;
                self.pending_tx = Some(packet);
                
                // Schedule turnaround completion
                ctx.post_event(
                    self.config.rx_to_tx_turnaround,
                    vec![self.id],
                    EventPayload::Timer { 
                        timer_id: TIMER_TX_TURNAROUND_COMPLETE
                    },
                );
            }
            InternalRadioState::TxTurnaround => {
                // Queue the TX request
                self.pending_tx = Some(packet);
            }
            InternalRadioState::Transmitting => {
                // Cannot TX while already transmitting - drop or queue
                // For now, we replace any pending TX
                self.pending_tx = Some(packet);
            }
        }
    }

    /// Start actual transmission after turnaround.
    fn start_transmission(&mut self, ctx: &mut SimContext) {
        if let Some(packet) = self.pending_tx.take() {
            // Calculate airtime
            let airtime = calculate_time_on_air(&self.config.params, packet.payload.len());
            let airtime_us = airtime.as_micros() as u64;
            let end_time = ctx.time() + airtime;
            let packet_size = packet.payload.len();
            
            // Build labels with packet breakdown
            // The recorder will filter to only the labels requested in metric specs
            let mut labels = self.metric_labels.to_labels();
            labels.push(("payload_type", packet.payload_type_label().to_string()));
            labels.push(("route_type", packet.route_type_label().to_string()));
            labels.push(("payload_hash", packet.payload_hash_label()));
            
            // Record TX metrics
            metrics::counter!(metric_defs::RADIO_TX_PACKETS.name, &labels).increment(1);
            metrics::counter!(metric_defs::RADIO_TX_AIRTIME.name, &labels).increment(airtime_us);
            metrics::histogram!(metric_defs::RADIO_TX_PACKET_SIZE.name, &labels).record(packet_size as f64);
            
            // Record turnaround time from last state change (use base labels without packet type)
            let base_labels = self.metric_labels.to_labels();
            if self.last_state_change_time != SimTime::ZERO {
                let turnaround_us = (ctx.time() - self.last_state_change_time).as_micros() as f64;
                metrics::histogram!(metric_defs::RADIO_TURNAROUND_TIME.name, &base_labels).record(turnaround_us);
            }
            self.last_state_change_time = ctx.time();
            
            // Update state
            self.state = InternalRadioState::Transmitting;
            self.current_tx = Some((packet.clone(), end_time));
            
            // Notify firmware of TX started
            self.notify_state_change(ctx, mcsim_common::RadioState::Transmitting);
            
            // Send TransmitAir to Graph entity
            ctx.post_immediate(
                vec![self.config.graph_entity],
                EventPayload::TransmitAir(mcsim_common::TransmitAirEvent {
                    radio_id: self.id,
                    packet,
                    params: self.config.params.clone(),
                    end_time,
                }),
            );
            
            // Schedule TX completion + RX turnaround
            let total_delay = airtime + self.config.tx_to_rx_turnaround;
            ctx.post_event(
                total_delay,
                vec![self.id],
                EventPayload::Timer {
                    timer_id: TIMER_RX_TURNAROUND_COMPLETE
                },
            );
        }
    }

    /// Handle reception of a packet (from Graph entity via ReceiveAir).
    fn handle_receive_air(&mut self, rx_event: &mcsim_common::ReceiveAirEvent, ctx: &mut SimContext) {
        // Can only receive when in Receiving state
        if self.state != InternalRadioState::Receiving {
            // Radio is not in receive mode - packet is lost
            return;
        }

        let reception_id = self.next_reception_id;
        self.next_reception_id += 1;

        // Sample the actual SNR from Gaussian distribution based on mean and std dev
        let snr_db = sample_gaussian(
            ctx.rng(),
            rx_event.mean_snr_db_at20dbm,
            rx_event.snr_std_dev,
        );

        let reception = ActiveReception {
            packet: rx_event.packet.clone(),
            source_radio_id: rx_event.source_radio_id,
            start_time: ctx.time(),
            end_time: rx_event.end_time,
            snr_db,
            rssi_dbm: rx_event.rssi_dbm,
            collided: false,
            reception_id,
        };

        self.active_receptions.push(reception);

        // Track active receptions gauge
        let labels = self.metric_labels.to_labels();
        metrics::gauge!(metric_defs::RADIO_ACTIVE_RECEPTIONS.name, &labels).increment(1.0);

        // Check for collisions with existing receptions (including capture effect)
        self.check_collisions_with_capture();

        // Schedule reception completion
        let delay = rx_event.end_time - ctx.time();
        ctx.post_event(
            delay,
            vec![self.id],
            EventPayload::Timer {
                timer_id: TIMER_RX_COMPLETE_BASE + reception_id
            },
        );
    }

    /// Handle reception completion timer.
    fn handle_rx_complete(&mut self, reception_id: u64, ctx: &mut SimContext) {
        // Find and remove the completed reception
        let idx = self.active_receptions.iter()
            .position(|r| r.reception_id == reception_id);

        if let Some(idx) = idx {
            let reception = self.active_receptions.remove(idx);
            
            // Base labels without packet type (for active_receptions gauge)
            let base_labels = self.metric_labels.to_labels();
            metrics::gauge!(metric_defs::RADIO_ACTIVE_RECEPTIONS.name, &base_labels).decrement(1.0);

            // Build labels with packet breakdown
            // The recorder will filter to only the labels requested in metric specs
            let mut labels = self.metric_labels.to_labels();
            labels.push(("payload_type", reception.packet.payload_type_label().to_string()));
            labels.push(("route_type", reception.packet.route_type_label().to_string()));
            labels.push(("payload_hash", reception.packet.payload_hash_label()));

            // Final collision check
            let survived = !reception.collided;
            
            // Check SNR threshold
            let snr_threshold = calculate_snr_sensitivity(self.config.params.spreading_factor);
            let snr_ok = reception.snr_db >= snr_threshold;

            // Calculate airtime for metrics
            let airtime = reception.end_time - reception.start_time;
            let airtime_us = airtime.as_micros() as u64;
            let packet_size = reception.packet.payload.len();

            // Always notify firmware/logger of reception outcome
            ctx.post_immediate(
                vec![self.attached_firmware],
                EventPayload::RadioRxPacket(mcsim_common::RadioRxPacketEvent {
                    packet: reception.packet,
                    source_radio_id: reception.source_radio_id,
                    snr_db: reception.snr_db,
                    rssi_dbm: reception.rssi_dbm,
                    was_collided: !survived,
                    was_weak_signal: !snr_ok,
                }),
            );

            if survived && snr_ok {
                // Record RX success metrics
                metrics::counter!(metric_defs::RADIO_RX_PACKETS.name, &labels).increment(1);
                metrics::counter!(metric_defs::RADIO_RX_AIRTIME.name, &labels).increment(airtime_us);
                metrics::histogram!(metric_defs::RADIO_RX_PACKET_SIZE.name, &labels).record(packet_size as f64);
                metrics::histogram!(metric_defs::RADIO_RX_SNR.name, &labels).record(reception.snr_db);
                metrics::histogram!(metric_defs::RADIO_RX_RSSI.name, &labels).record(reception.rssi_dbm);
            } else if !survived {
                // Packet lost due to collision
                metrics::counter!(metric_defs::RADIO_RX_COLLIDED.name, &labels).increment(1);
            } else {
                // Packet lost due to weak signal (SNR below threshold)
                metrics::counter!(metric_defs::RADIO_RX_WEAK.name, &labels).increment(1);
            }
        }
    }
}

/// Create a new radio entity (legacy API).
pub fn create_radio(
    id: EntityId,
    params: RadioParams,
    position: GeoCoord,
    attached_firmware: EntityId,
) -> Radio {
    Radio::with_params(id, params, position, attached_firmware)
}

impl Entity for Radio {
    fn entity_id(&self) -> EntityId {
        self.id
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut SimContext) -> Result<(), SimError> {
        match &event.payload {
            EventPayload::RadioTxRequest(tx_request) => {
                // Firmware requests transmission
                self.handle_tx_request(tx_request.packet.clone(), ctx);
            }
            EventPayload::ReceiveAir(rx_air_event) => {
                // Handle incoming transmission routed from Graph entity
                self.handle_receive_air(rx_air_event, ctx);
            }
            EventPayload::Timer { timer_id } => {
                // Handle internal timers
                if *timer_id == TIMER_TX_TURNAROUND_COMPLETE {
                    // TX turnaround complete - start actual transmission
                    self.start_transmission(ctx);
                } else if *timer_id == TIMER_RX_TURNAROUND_COMPLETE {
                    // RX turnaround complete - back to receiving
                    self.state = InternalRadioState::Receiving;
                    self.current_tx = None;
                    self.notify_state_change(ctx, mcsim_common::RadioState::Receiving);
                    
                    // Check if there's a pending TX request
                    if self.pending_tx.is_some() {
                        self.state = InternalRadioState::TxTurnaround;
                        ctx.post_event(
                            self.config.rx_to_tx_turnaround,
                            vec![self.id],
                            EventPayload::Timer { 
                                timer_id: TIMER_TX_TURNAROUND_COMPLETE
                            },
                        );
                    }
                } else if *timer_id >= TIMER_RX_COMPLETE_BASE {
                    // RX complete timer - extract reception_id
                    let reception_id = *timer_id - TIMER_RX_COMPLETE_BASE;
                    self.handle_rx_complete(reception_id, ctx);
                }
            }
            _ => {
                // Ignore other events
            }
        }
        Ok(())
    }
}

/// Trait for entities that have radio capabilities.
pub trait RadioEntity: Entity {
    /// Get the radio parameters.
    fn radio_params(&self) -> &RadioParams;

    /// Get the radio position.
    fn position(&self) -> GeoCoord;
}

impl RadioEntity for Radio {
    fn radio_params(&self) -> &RadioParams {
        &self.config.params
    }

    fn position(&self) -> GeoCoord {
        self.position
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a new link model.
pub fn create_link_model() -> LinkModel {
    LinkModel::new()
}

// ============================================================================
// Graph Entity - Routes transmissions between radios
// ============================================================================

/// The Graph entity routes radio transmissions to receivers.
/// 
/// It receives TransmitAir events from Radio entities and routes them
/// to appropriate receivers based on the LinkModel, sending ReceiveAir
/// events with SNR/RSSI from the link parameters.
pub struct Graph {
    id: EntityId,
    link_model: LinkModel,
}

impl Graph {
    /// Create a new Graph entity with the given link model.
    pub fn new(id: EntityId, link_model: LinkModel) -> Self {
        Graph { id, link_model }
    }

    /// Get a mutable reference to the link model for runtime modification.
    pub fn link_model_mut(&mut self) -> &mut LinkModel {
        &mut self.link_model
    }

    /// Get an immutable reference to the link model.
    pub fn link_model(&self) -> &LinkModel {
        &self.link_model
    }
}

impl Entity for Graph {
    fn entity_id(&self) -> EntityId {
        self.id
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut SimContext) -> Result<(), SimError> {
        match &event.payload {
            EventPayload::TransmitAir(tx_event) => {
                // Route to all receivers in range
                for (receiver_id, link_params) in self.link_model.get_receivers(tx_event.radio_id) {
                    ctx.post_immediate(
                        vec![receiver_id],
                        EventPayload::ReceiveAir(mcsim_common::ReceiveAirEvent {
                            source_radio_id: tx_event.radio_id,
                            packet: tx_event.packet.clone(),
                            params: tx_event.params.clone(),
                            end_time: tx_event.end_time,
                            mean_snr_db_at20dbm: link_params.mean_snr_db_at20dbm,
                            snr_std_dev: link_params.snr_std_dev,
                            rssi_dbm: link_params.rssi_dbm,
                        }),
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_on_air() {
        let params = RadioParams::default_meshcore();
        let toa = calculate_time_on_air(&params, 50);
        // Should be in the range of hundreds of milliseconds for SF11
        assert!(toa.as_millis() > 100);
        assert!(toa.as_millis() < 5000);
    }

    #[test]
    fn test_snr_sensitivity_thresholds() {
        // Verify SF sensitivity thresholds are reasonable
        let sf11_sens = calculate_snr_sensitivity(11);
        assert!(sf11_sens < -10.0 && sf11_sens > -25.0, "SF11 sensitivity should be around -17.5 dB");
        
        // Higher SF should have lower (more sensitive) threshold
        let sf7_sens = calculate_snr_sensitivity(7);
        let sf12_sens = calculate_snr_sensitivity(12);
        assert!(sf7_sens > sf12_sens, "SF12 should be more sensitive than SF7");
    }

    #[test]
    fn test_gaussian_sampling() {
        // Test that Gaussian sampling produces values around the mean
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;
        
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let mean = 10.0;
        let std_dev = 1.8;
        
        // Sample many values and check they're roughly centered around the mean
        let samples: Vec<f64> = (0..1000).map(|_| sample_gaussian(&mut rng, mean, std_dev)).collect();
        let sample_mean: f64 = samples.iter().sum::<f64>() / samples.len() as f64;
        
        // Mean should be within 0.2 of expected value with 1000 samples
        assert!((sample_mean - mean).abs() < 0.2, "Sample mean {} should be close to {}", sample_mean, mean);
    }

    #[test]
    fn test_collision_no_overlap() {
        let incoming = CollisionContext {
            start_time: SimTime::from_millis(1000),
            end_time: SimTime::from_millis(1500),
            frequency_hz: 906_000_000,
            packet_id: 1,
        };
        let existing = vec![CollisionContext {
            start_time: SimTime::from_millis(0),
            end_time: SimTime::from_millis(500),
            frequency_hz: 906_000_000,
            packet_id: 0,
        }];

        assert_eq!(check_collision(&incoming, &existing), CollisionResult::NoCollision);
    }

    #[test]
    fn test_collision_overlap() {
        let incoming = CollisionContext {
            start_time: SimTime::from_millis(400),
            end_time: SimTime::from_millis(900),
            frequency_hz: 906_000_000,
            packet_id: 1,
        };
        let existing = vec![CollisionContext {
            start_time: SimTime::from_millis(0),
            end_time: SimTime::from_millis(500),
            frequency_hz: 906_000_000,
            packet_id: 0,
        }];

        assert_eq!(check_collision(&incoming, &existing), CollisionResult::BothDestroyed(0));
    }
}
