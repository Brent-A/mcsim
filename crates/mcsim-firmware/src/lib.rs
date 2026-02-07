//! # mcsim-firmware
//!
//! MeshCore firmware simulation for MCSim.
//!
//! This crate wraps the MeshCore C++ firmware compiled as DLLs, providing a Rust interface
//! that integrates with the simulation framework.
//!
//! ## Usage
//!
//! ```no_run
//! use mcsim_firmware::{RepeaterFirmware, RepeaterConfig, FirmwareConfig};
//! use mcsim_common::{EntityId, NodeId, SimTime};
//!
//! let config = RepeaterConfig {
//!     base: FirmwareConfig::default(),
//!     ..Default::default()
//! };
//! let firmware = RepeaterFirmware::new(
//!     EntityId::new(1),
//!     config,
//!     EntityId::new(0), // attached radio
//!     "MyNode".to_string(),
//! )?;
//! # Ok::<(), mcsim_firmware::FirmwareError>(())
//! ```

pub mod dll;
pub mod tracer;

use dll::{DllError, FirmwareDll, FirmwareType, NodeConfig, OwnedFirmwareNode};
pub use dll::{YieldReason, FirmwareSimulationParams};
use mcsim_common::{
    entity_tracer::FirmwareYieldReason,
    Entity, EntityId, Event, EventPayload, FirmwareLogEvent, NodeId, SimContext, SimError, SimTime,
};
use meshcore_packet::EncryptionKey;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert DLL YieldReason to tracer FirmwareYieldReason.
fn to_trace_yield_reason(reason: YieldReason) -> FirmwareYieldReason {
    match reason {
        YieldReason::Idle => FirmwareYieldReason::Idle,
        YieldReason::RadioTxStart => FirmwareYieldReason::RadioTxStart,
        YieldReason::RadioTxComplete => FirmwareYieldReason::RadioTxComplete,
        YieldReason::Reboot => FirmwareYieldReason::Reboot,
        YieldReason::PowerOff => FirmwareYieldReason::PowerOff,
        YieldReason::Error => FirmwareYieldReason::Error,
    }
}

/// Describe an event payload for tracing.
fn describe_event(payload: &EventPayload) -> String {
    match payload {
        EventPayload::RadioRxPacket(e) => {
            format!("RadioRxPacket(len={}, snr={:.1}, rssi={:.1})", 
                e.packet.payload.len(), e.snr_db, e.rssi_dbm)
        }
        EventPayload::RadioStateChanged(e) => {
            format!("RadioStateChanged({:?})", e.new_state)
        }
        EventPayload::SerialRx(e) => {
            format!("SerialRx(len={})", e.data.len())
        }
        EventPayload::Timer { timer_id } => {
            format!("Timer(id={})", timer_id)
        }
        _ => format!("{:?}", std::mem::discriminant(payload)),
    }
}

fn emit_firmware_log_events(ctx: &mut SimContext, log_str: &str) {
    if log_str.is_empty() {
        return;
    }
    for line in log_str.lines() {
        if line.is_empty() {
            continue;
        }
        ctx.post_immediate(
            Vec::new(),
            EventPayload::FirmwareLog(FirmwareLogEvent {
                line: line.to_string(),
            }),
        );
    }
}

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur when working with firmware entities.
#[derive(Debug, Error)]
pub enum FirmwareError {
    /// DLL loading or operation failed.
    #[error("DLL error: {0}")]
    Dll(#[from] DllError),

    /// Firmware node creation failed.
    #[error("Failed to create firmware node")]
    CreateFailed,
}

// ============================================================================
// Common Configuration
// ============================================================================

/// Common firmware configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareConfig {
    /// Node identifier (public key).
    pub node_id: NodeId,
    /// Public key bytes.
    pub public_key: [u8; 32],
    /// Private key bytes.
    pub private_key: [u8; 32],
    /// Encryption key (optional).
    pub encryption_key: Option<EncryptionKey>,
    /// RNG seed for deterministic simulation.
    pub rng_seed: u32,
}

impl Default for FirmwareConfig {
    fn default() -> Self {
        FirmwareConfig {
            node_id: NodeId::from_bytes([0u8; 32]),
            public_key: [0u8; 32],
            private_key: [0u8; 32],
            encryption_key: None,
            rng_seed: 12345, // Default seed - should be overridden per node
        }
    }
}

// ============================================================================
// Firmware Entity Trait
// ============================================================================

/// Result from an async firmware step.
#[derive(Debug)]
pub struct FirmwareStepResult {
    /// The yield reason from the DLL.
    pub reason: YieldReason,
    /// Wake time in milliseconds.
    pub wake_millis: u64,
    /// Radio TX data if transmitting.
    pub radio_tx_data: Option<(Vec<u8>, u32)>,
    /// Serial TX data if any.
    pub serial_tx_data: Option<Vec<u8>>,
    /// Log output from firmware.
    pub log_output: String,
    /// Error message if any.
    pub error_message: Option<String>,
}

/// Trait for firmware entities.
pub trait FirmwareEntity: Entity {
    /// Get the node ID.
    fn node_id(&self) -> NodeId;

    /// Get the public key.
    fn public_key(&self) -> &[u8; 32];

    /// Get the attached radio entity ID.
    fn attached_radio(&self) -> EntityId;
    
    /// Begin an async firmware step (non-blocking).
    /// Call `step_wait()` afterward to get the result.
    fn step_begin(&mut self, event: &Event);
    
    /// Wait for the async firmware step to complete and return results.
    fn step_wait(&mut self) -> FirmwareStepResult;
    
    /// Check if this entity is ready for parallel stepping.
    fn supports_parallel_step(&self) -> bool {
        true
    }
}

// ============================================================================
// Repeater Firmware
// ============================================================================

/// Repeater firmware configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepeaterConfig {
    /// Base firmware configuration.
    pub base: FirmwareConfig,
}

impl Default for RepeaterConfig {
    fn default() -> Self {
        RepeaterConfig {
            base: FirmwareConfig::default(),
        }
    }
}

/// Repeater firmware entity backed by the C++ DLL.
pub struct RepeaterFirmware {
    id: EntityId,
    name: String,
    config: RepeaterConfig,
    attached_radio: EntityId,
    attached_cli_agent: Option<EntityId>,

    // DLL state - persistent node that survives across events
    node: OwnedFirmwareNode,
    // Current simulation time in milliseconds
    current_millis: u64,
    // Initial RTC time (Unix timestamp) - added to sim time for RTC clock
    initial_rtc: u32,
    // Pending TX packet from last step
    pending_tx: Option<(Vec<u8>, u32)>,
    // Whether we're waiting for TX completion
    awaiting_tx_complete: bool,
    // Next wake time
    wake_millis: u64,
    // Startup time in microseconds - events before this are dropped
    startup_time_us: u64,
}

impl RepeaterFirmware {
    /// Create a new repeater firmware entity.
    pub fn new(
        id: EntityId,
        config: RepeaterConfig,
        attached_radio: EntityId,
        name: String,
    ) -> Result<Self, FirmwareError> {
        Self::with_sim_params(id, config, attached_radio, name, &FirmwareSimulationParams::default())
    }

    /// Create a new repeater firmware entity with simulation parameters.
    pub fn with_sim_params(
        id: EntityId,
        config: RepeaterConfig,
        attached_radio: EntityId,
        name: String,
        sim_params: &FirmwareSimulationParams,
    ) -> Result<Self, FirmwareError> {
        let dll = Arc::new(FirmwareDll::load(FirmwareType::Repeater)?);

        // The firmware expects an "expanded" 64-byte private key which is the SHA512 hash
        // of the 32-byte seed, with the first 32 bytes clamped for Ed25519.
        use sha2::{Sha512, Digest};
        let mut hasher = Sha512::new();
        hasher.update(&config.base.private_key);
        let hash_result = hasher.finalize();
        let mut prv_key_64 = [0u8; 64];
        prv_key_64.copy_from_slice(&hash_result);
        // Apply Ed25519 clamping to the scalar (first 32 bytes)
        prv_key_64[0] &= 248;
        prv_key_64[31] &= 63;
        prv_key_64[31] |= 64;

        // Use initial RTC time from simulation parameters
        let initial_rtc: u32 = sim_params.initial_rtc_secs as u32;

        let node_config = NodeConfig::default()
            .with_keys(&config.base.public_key, &prv_key_64)
            .with_initial_time(0, initial_rtc)
            .with_rng_seed(config.base.rng_seed)
            .with_name(&name)
            .with_spin_detection(
                sim_params.spin_detection_threshold,
                sim_params.idle_loops_before_yield,
            )
            .with_spin_logging(
                sim_params.log_spin_detection,
                sim_params.log_loop_iterations,
            );

        // Create the persistent node
        let node = OwnedFirmwareNode::new(dll, &node_config)
            .map_err(|e| FirmwareError::Dll(e))?;

        Ok(RepeaterFirmware {
            id,
            name,
            config,
            attached_radio,
            attached_cli_agent: None,
            node,
            current_millis: 0,
            initial_rtc,
            pending_tx: None,
            awaiting_tx_complete: false,
            wake_millis: 0,
            startup_time_us: sim_params.startup_time_us,
        })
    }

    /// Get the node name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the configuration.
    pub fn config(&self) -> &RepeaterConfig {
        &self.config
    }

    /// Get the attached CLI agent.
    pub fn attached_cli_agent(&self) -> Option<EntityId> {
        self.attached_cli_agent
    }

    /// Set the attached CLI agent.
    pub fn set_attached_cli_agent(&mut self, cli_agent: EntityId) {
        self.attached_cli_agent = Some(cli_agent);
    }
}

impl Entity for RepeaterFirmware {
    fn entity_id(&self) -> EntityId {
        self.id
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut SimContext) -> Result<(), SimError> {
        // Update current time
        self.current_millis = event.time.as_micros() / 1000;
        // Clone the tracer to avoid borrow conflict with ctx
        let tracer = ctx.tracer().clone();

        // Check if we're still in startup delay period
        let event_time_us = event.time.as_micros();
        if event_time_us < self.startup_time_us {
            // Drop all events before startup time (radio, serial, timer)
            tracer.log_event_received(Some(&self.name), self.id, event.time, event);
            log::trace!(
                "[{}] Dropping event during startup delay: {:?} (event_time={}us < startup={}us)",
                self.name, event.payload, event_time_us, self.startup_time_us
            );
            return Ok(());
        }

        // Log event received
        tracer.log_event_received(Some(&self.name), self.id, event.time, event);

        // Log step begin with trigger event description
        let trigger_desc = describe_event(&event.payload);
        tracer.log_firmware_step_begin(Some(&self.name), self.id, event.time, &trigger_desc);

        match &event.payload {
            EventPayload::RadioRxPacket(rx_event) => {
                if !rx_event.was_collided && !rx_event.was_weak_signal {
                    // Inject the packet into the firmware (only if successfully received)
                    self.node.inject_radio_rx(
                        &rx_event.packet.payload,
                        rx_event.rssi_dbm as f32,
                        rx_event.snr_db as f32,
                    );
                }
            }
            EventPayload::RadioStateChanged(state_event) => {
                // Notify DLL of state change (for spin detection)
                self.node.notify_state_change(state_event.state_version);
                
                // Radio state changed - TX complete transitions back to Receiving
                if state_event.new_state == mcsim_common::RadioState::Receiving && self.awaiting_tx_complete {
                    self.node.notify_tx_complete();
                    self.awaiting_tx_complete = false;
                    self.pending_tx = None;
                }
            }
            EventPayload::SerialRx(serial_event) => {
                // Inject serial data from external source (e.g., TCP client)
                self.node.inject_serial_rx(&serial_event.data);
            }
            EventPayload::Timer { timer_id: _ } => {
                // Wake timer or periodic timer - just step below
            }
            _ => return Ok(()),
        }

        // Step the firmware
        let rtc_secs = self.initial_rtc + (self.current_millis / 1000) as u32;
        let result = self.node.step(self.current_millis, rtc_secs);

        self.wake_millis = result.wake_millis;

        // Log yield with details
        let mut yield_details = vec![
            ("wake_ms".to_string(), format!("{}", result.wake_millis)),
            ("current_ms".to_string(), format!("{}", self.current_millis)),
        ];
        if let Some(msg) = result.error_message() {
            yield_details.push(("error".to_string(), msg.to_string()));
        }
        tracer.log_firmware_step_yield(
            Some(&self.name),
            self.id,
            event.time,
            to_trace_yield_reason(result.reason),
            yield_details,
        );

        match result.reason {
            YieldReason::RadioTxStart => {
                // Firmware wants to transmit
                let tx_data = result.radio_tx().to_vec();
                let airtime_ms = result.radio_tx_airtime_ms;
                self.pending_tx = Some((tx_data.clone(), airtime_ms));
                self.awaiting_tx_complete = true;

                // Log TX request
                tracer.log_firmware_tx_request(
                    Some(&self.name),
                    self.id,
                    event.time,
                    tx_data.len(),
                    airtime_ms,
                );

                // Send packet to radio as LoraPacket (eagerly decoded for metrics)
                ctx.post_immediate(
                    vec![self.attached_radio],
                    EventPayload::RadioTxRequest(mcsim_common::RadioTxRequestEvent {
                        packet: mcsim_common::LoraPacket::new(tx_data),
                    }),
                );
            }
            YieldReason::Idle => {
                // Schedule wake timer if needed
                if result.wake_millis > self.current_millis {
                    let delay_us = (result.wake_millis - self.current_millis) * 1000;
                    let delay_ms = delay_us / 1000;
                    tracer.log_timer_scheduled(Some(&self.name), self.id, event.time, 1, delay_ms);
                    ctx.post_event(
                        SimTime::from_micros(delay_us),
                        vec![self.id],
                        EventPayload::Timer { timer_id: 1 },
                    );
                }
            }
            YieldReason::RadioTxComplete => {
                // TX completed internally
                self.awaiting_tx_complete = false;
                self.pending_tx = None;
            }
            YieldReason::Error => {
                if let Some(msg) = result.error_message() {
                    eprintln!("Firmware error: {}", msg);
                }
            }
            _ => {}
        }

        // Emit serial TX data if any
        let serial_tx = result.serial_tx();
        if !serial_tx.is_empty() {
            tracer.log_firmware_serial_tx(Some(&self.name), self.id, event.time, serial_tx);
            
            // Send to self first (for UART/TCP bridge)
            ctx.post_immediate(
                vec![self.id],
                EventPayload::SerialTx(mcsim_common::SerialTxEvent {
                    data: serial_tx.to_vec(),
                }),
            );
            
            // Also forward to attached CLI agent if present
            if let Some(cli_agent_id) = self.attached_cli_agent {
                ctx.post_immediate(
                    vec![cli_agent_id],
                    EventPayload::SerialTx(mcsim_common::SerialTxEvent {
                        data: serial_tx.to_vec(),
                    }),
                );
            }
        }

        // Log firmware output
        let log_str = result.log_output();
        tracer.log_firmware_output(Some(&self.name), self.id, event.time, &log_str);
        emit_firmware_log_events(ctx, &log_str);

        Ok(())
    }
}

impl FirmwareEntity for RepeaterFirmware {
    fn node_id(&self) -> NodeId {
        self.config.base.node_id
    }

    fn public_key(&self) -> &[u8; 32] {
        &self.config.base.public_key
    }

    fn attached_radio(&self) -> EntityId {
        self.attached_radio
    }
    
    fn step_begin(&mut self, event: &Event) {
        // Update current time
        self.current_millis = event.time.as_micros() / 1000;
        
        // Process event payload to inject data into DLL
        match &event.payload {
            EventPayload::RadioRxPacket(rx_event) => {
                if !rx_event.was_collided {
                    self.node.inject_radio_rx(
                        &rx_event.packet.payload,
                        rx_event.rssi_dbm as f32,
                        rx_event.snr_db as f32,
                    );
                }
            }
            EventPayload::RadioStateChanged(state_event) => {
                self.node.notify_state_change(state_event.state_version);
                if state_event.new_state == mcsim_common::RadioState::Receiving && self.awaiting_tx_complete {
                    self.node.notify_tx_complete();
                    self.awaiting_tx_complete = false;
                    self.pending_tx = None;
                }
            }
            EventPayload::SerialRx(serial_event) => {
                self.node.inject_serial_rx(&serial_event.data);
            }
            EventPayload::Timer { timer_id: _ } => {
                // Wake timer - just step below
            }
            _ => {}
        }
        
        // Begin async step
        let rtc_secs = self.initial_rtc + (self.current_millis / 1000) as u32;
        self.node.step_begin(self.current_millis, rtc_secs);
    }
    
    fn step_wait(&mut self) -> FirmwareStepResult {
        let result = self.node.step_wait();
        self.wake_millis = result.wake_millis;
        
        // Determine TX data
        let radio_tx_data = if result.reason == YieldReason::RadioTxStart {
            let tx_data = result.radio_tx().to_vec();
            let airtime_ms = result.radio_tx_airtime_ms;
            self.pending_tx = Some((tx_data.clone(), airtime_ms));
            self.awaiting_tx_complete = true;
            Some((tx_data, airtime_ms))
        } else {
            if result.reason == YieldReason::RadioTxComplete {
                self.awaiting_tx_complete = false;
                self.pending_tx = None;
            }
            None
        };
        
        // Get serial TX data
        let serial_tx = result.serial_tx();
        let serial_tx_data = if serial_tx.is_empty() {
            None
        } else {
            Some(serial_tx.to_vec())
        };
        
        FirmwareStepResult {
            reason: result.reason,
            wake_millis: result.wake_millis,
            radio_tx_data,
            serial_tx_data,
            log_output: result.log_output(),
            error_message: result.error_message(),
        }
    }
}

// ============================================================================
// Companion Firmware
// ============================================================================

/// Companion firmware configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionConfig {
    /// Base firmware configuration.
    pub base: FirmwareConfig,
}

impl Default for CompanionConfig {
    fn default() -> Self {
        CompanionConfig {
            base: FirmwareConfig::default(),
        }
    }
}

/// Companion firmware entity backed by the C++ DLL.
pub struct CompanionFirmware {
    id: EntityId,
    name: String,
    config: CompanionConfig,
    attached_radio: EntityId,
    attached_agent: Option<EntityId>,

    // DLL state - persistent node that survives across events
    node: OwnedFirmwareNode,
    current_millis: u64,
    // Initial RTC time (Unix timestamp) - added to sim time for RTC clock
    initial_rtc: u32,
    pending_tx: Option<(Vec<u8>, u32)>,
    awaiting_tx_complete: bool,
    wake_millis: u64,
    // Startup time in microseconds - events before this are dropped
    startup_time_us: u64,
}

impl CompanionFirmware {
    /// Create a new companion firmware entity.
    pub fn new(
        id: EntityId,
        config: CompanionConfig,
        attached_radio: EntityId,
        attached_agent: Option<EntityId>,
        name: String,
    ) -> Result<Self, FirmwareError> {
        Self::with_sim_params(id, config, attached_radio, attached_agent, name, &FirmwareSimulationParams::default())
    }

    /// Create a new companion firmware entity with simulation parameters.
    pub fn with_sim_params(
        id: EntityId,
        config: CompanionConfig,
        attached_radio: EntityId,
        attached_agent: Option<EntityId>,
        name: String,
        sim_params: &FirmwareSimulationParams,
    ) -> Result<Self, FirmwareError> {
        let dll = Arc::new(FirmwareDll::load(FirmwareType::Companion)?);

        // The firmware expects an "expanded" 64-byte private key which is the SHA512 hash
        // of the 32-byte seed, with the first 32 bytes clamped for Ed25519.
        // We compute this expansion here to match what ed25519_create_keypair does.
        use sha2::{Sha512, Digest};
        let mut hasher = Sha512::new();
        hasher.update(&config.base.private_key);
        let hash_result = hasher.finalize();
        let mut prv_key_64 = [0u8; 64];
        prv_key_64.copy_from_slice(&hash_result);
        // Apply Ed25519 clamping to the scalar (first 32 bytes)
        prv_key_64[0] &= 248;
        prv_key_64[31] &= 63;
        prv_key_64[31] |= 64;

        // Use initial RTC time from simulation parameters
        let initial_rtc: u32 = sim_params.initial_rtc_secs as u32;

        let node_config = NodeConfig::default()
            .with_keys(&config.base.public_key, &prv_key_64)
            .with_initial_time(0, initial_rtc)
            .with_rng_seed(config.base.rng_seed)
            .with_name(&name)
            .with_spin_detection(
                sim_params.spin_detection_threshold,
                sim_params.idle_loops_before_yield,
            )
            .with_spin_logging(
                sim_params.log_spin_detection,
                sim_params.log_loop_iterations,
            );

        // Create the persistent node
        let node = OwnedFirmwareNode::new(dll, &node_config)
            .map_err(|e| FirmwareError::Dll(e))?;

        Ok(CompanionFirmware {
            id,
            name,
            config,
            attached_radio,
            attached_agent,
            node,
            current_millis: 0,
            initial_rtc,
            pending_tx: None,
            awaiting_tx_complete: false,
            wake_millis: 0,
            startup_time_us: sim_params.startup_time_us,
        })
    }

    /// Get the node name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the configuration.
    pub fn config(&self) -> &CompanionConfig {
        &self.config
    }

    /// Get the attached agent entity ID.
    pub fn attached_agent(&self) -> Option<EntityId> {
        self.attached_agent
    }

    /// Set the attached agent entity ID.
    pub fn set_attached_agent(&mut self, agent: EntityId) {
        self.attached_agent = Some(agent);
    }
}

impl Entity for CompanionFirmware {
    fn entity_id(&self) -> EntityId {
        self.id
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut SimContext) -> Result<(), SimError> {
        self.current_millis = event.time.as_micros() / 1000;
        // Clone the tracer to avoid borrow conflict with ctx
        let tracer = ctx.tracer().clone();

        // Check if we're still in startup delay period
        let event_time_us = event.time.as_micros();
        if event_time_us < self.startup_time_us {
            // Drop all events before startup time (radio, serial, timer)
            tracer.log_event_received(Some(&self.name), self.id, event.time, event);
            log::trace!(
                "[{}] Dropping event during startup delay: {:?} (event_time={}us < startup={}us)",
                self.name, event.payload, event_time_us, self.startup_time_us
            );
            return Ok(());
        }

        // Log event received
        tracer.log_event_received(Some(&self.name), self.id, event.time, event);

        // Log step begin with trigger event description
        let trigger_desc = describe_event(&event.payload);
        tracer.log_firmware_step_begin(Some(&self.name), self.id, event.time, &trigger_desc);

        match &event.payload {
            EventPayload::RadioRxPacket(rx_event) => {
                if !rx_event.was_collided {
                    self.node.inject_radio_rx(
                        &rx_event.packet.payload,
                        rx_event.rssi_dbm as f32,
                        rx_event.snr_db as f32,
                    );
                    // Also forward to attached agent
                    if let Some(agent_id) = self.attached_agent {
                        ctx.post_immediate(
                            vec![agent_id],
                            EventPayload::RadioRxPacket(rx_event.clone()),
                        );
                    }
                }
            }
            EventPayload::RadioStateChanged(state_event) => {
                // Notify DLL of state change (for spin detection)
                self.node.notify_state_change(state_event.state_version);
                
                if state_event.new_state == mcsim_common::RadioState::Receiving && self.awaiting_tx_complete {
                    self.node.notify_tx_complete();
                    self.awaiting_tx_complete = false;
                    self.pending_tx = None;
                }
            }
            EventPayload::SerialRx(serial_event) => {
                // Inject serial data from external source (e.g., TCP client)
                self.node.inject_serial_rx(&serial_event.data);
            }
            EventPayload::Timer { timer_id: _ } => {
                // Wake timer or periodic timer - just step below
            }
            _ => return Ok(()),
        }

        // Step the firmware
        let rtc_secs = self.initial_rtc + (self.current_millis / 1000) as u32;
        let result = self.node.step(self.current_millis, rtc_secs);

        self.wake_millis = result.wake_millis;

        // Log yield with details
        let mut yield_details = vec![
            ("wake_ms".to_string(), format!("{}", result.wake_millis)),
            ("current_ms".to_string(), format!("{}", self.current_millis)),
        ];
        if let Some(msg) = result.error_message() {
            yield_details.push(("error".to_string(), msg.to_string()));
        }
        tracer.log_firmware_step_yield(
            Some(&self.name),
            self.id,
            event.time,
            to_trace_yield_reason(result.reason),
            yield_details,
        );

        match result.reason {
            YieldReason::RadioTxStart => {
                let tx_data = result.radio_tx().to_vec();
                let airtime_ms = result.radio_tx_airtime_ms;
                self.pending_tx = Some((tx_data.clone(), airtime_ms));
                self.awaiting_tx_complete = true;

                // Log TX request
                tracer.log_firmware_tx_request(
                    Some(&self.name),
                    self.id,
                    event.time,
                    tx_data.len(),
                    airtime_ms,
                );

                ctx.post_immediate(
                    vec![self.attached_radio],
                    EventPayload::RadioTxRequest(mcsim_common::RadioTxRequestEvent {
                        packet: mcsim_common::LoraPacket::new(tx_data),
                    }),
                );
            }
            YieldReason::Idle => {
                if result.wake_millis > self.current_millis {
                    let delay_us = (result.wake_millis - self.current_millis) * 1000;
                    let delay_ms = delay_us / 1000;
                    tracer.log_timer_scheduled(Some(&self.name), self.id, event.time, 1, delay_ms);
                    ctx.post_event(
                        SimTime::from_micros(delay_us),
                        vec![self.id],
                        EventPayload::Timer { timer_id: 1 },
                    );
                }
            }
            YieldReason::RadioTxComplete => {
                self.awaiting_tx_complete = false;
                self.pending_tx = None;
            }
            YieldReason::Error => {
                if let Some(msg) = result.error_message() {
                    eprintln!("Firmware error: {}", msg);
                }
            }
            _ => {}
        }

        // Emit serial TX data if any
        let serial_tx = result.serial_tx();
        if !serial_tx.is_empty() {
            tracer.log_firmware_serial_tx(Some(&self.name), self.id, event.time, serial_tx);
            
            // Send to self first (for UART/TCP bridge)
            ctx.post_immediate(
                vec![self.id],
                EventPayload::SerialTx(mcsim_common::SerialTxEvent {
                    data: serial_tx.to_vec(),
                }),
            );
            
            // Also forward to attached agent if present
            if let Some(agent_id) = self.attached_agent {
                ctx.post_immediate(
                    vec![agent_id],
                    EventPayload::SerialTx(mcsim_common::SerialTxEvent {
                        data: serial_tx.to_vec(),
                    }),
                );
            }
        }

        // Log firmware output
        let log_str = result.log_output();
        tracer.log_firmware_output(Some(&self.name), self.id, event.time, &log_str);
        emit_firmware_log_events(ctx, &log_str);

        Ok(())
    }
}

impl FirmwareEntity for CompanionFirmware {
    fn node_id(&self) -> NodeId {
        self.config.base.node_id
    }

    fn public_key(&self) -> &[u8; 32] {
        &self.config.base.public_key
    }

    fn attached_radio(&self) -> EntityId {
        self.attached_radio
    }
    
    fn step_begin(&mut self, event: &Event) {
        // Update current time
        self.current_millis = event.time.as_micros() / 1000;
        
        // Process event payload to inject data into DLL
        match &event.payload {
            EventPayload::RadioRxPacket(rx_event) => {
                if !rx_event.was_collided {
                    self.node.inject_radio_rx(
                        &rx_event.packet.payload,
                        rx_event.rssi_dbm as f32,
                        rx_event.snr_db as f32,
                    );
                }
            }
            EventPayload::RadioStateChanged(state_event) => {
                self.node.notify_state_change(state_event.state_version);
                if state_event.new_state == mcsim_common::RadioState::Receiving && self.awaiting_tx_complete {
                    self.node.notify_tx_complete();
                    self.awaiting_tx_complete = false;
                    self.pending_tx = None;
                }
            }
            EventPayload::SerialRx(serial_event) => {
                self.node.inject_serial_rx(&serial_event.data);
            }
            EventPayload::Timer { timer_id: _ } => {
                // Wake timer - just step below
            }
            _ => {}
        }
        
        // Begin async step
        let rtc_secs = self.initial_rtc + (self.current_millis / 1000) as u32;
        self.node.step_begin(self.current_millis, rtc_secs);
    }
    
    fn step_wait(&mut self) -> FirmwareStepResult {
        let result = self.node.step_wait();
        self.wake_millis = result.wake_millis;
        
        // Determine TX data
        let radio_tx_data = if result.reason == YieldReason::RadioTxStart {
            let tx_data = result.radio_tx().to_vec();
            let airtime_ms = result.radio_tx_airtime_ms;
            self.pending_tx = Some((tx_data.clone(), airtime_ms));
            self.awaiting_tx_complete = true;
            Some((tx_data, airtime_ms))
        } else {
            if result.reason == YieldReason::RadioTxComplete {
                self.awaiting_tx_complete = false;
                self.pending_tx = None;
            }
            None
        };
        
        // Get serial TX data
        let serial_tx = result.serial_tx();
        let serial_tx_data = if serial_tx.is_empty() {
            None
        } else {
            Some(serial_tx.to_vec())
        };
        
        FirmwareStepResult {
            reason: result.reason,
            wake_millis: result.wake_millis,
            radio_tx_data,
            serial_tx_data,
            log_output: result.log_output(),
            error_message: result.error_message(),
        }
    }
}

// ============================================================================
// Room Server Firmware
// ============================================================================

/// Room server firmware configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomServerConfig {
    /// Base firmware configuration.
    pub base: FirmwareConfig,
    /// Room identifier.
    pub room_id: [u8; 16],
}

impl Default for RoomServerConfig {
    fn default() -> Self {
        RoomServerConfig {
            base: FirmwareConfig::default(),
            room_id: [0u8; 16],
        }
    }
}

/// Room server firmware entity backed by the C++ DLL.
pub struct RoomServerFirmware {
    id: EntityId,
    name: String,
    config: RoomServerConfig,
    attached_radio: EntityId,
    attached_cli_agent: Option<EntityId>,

    // DLL state - persistent node that survives across events
    node: OwnedFirmwareNode,
    current_millis: u64,
    // Initial RTC time (Unix timestamp) - added to sim time for RTC clock
    initial_rtc: u32,
    pending_tx: Option<(Vec<u8>, u32)>,
    awaiting_tx_complete: bool,
    wake_millis: u64,
    // Startup time in microseconds - events before this are dropped
    startup_time_us: u64,
}

impl RoomServerFirmware {
    /// Create a new room server firmware entity.
    pub fn new(
        id: EntityId,
        config: RoomServerConfig,
        attached_radio: EntityId,
        name: String,
    ) -> Result<Self, FirmwareError> {
        Self::with_sim_params(id, config, attached_radio, name, &FirmwareSimulationParams::default())
    }

    /// Create a new room server firmware entity with simulation parameters.
    pub fn with_sim_params(
        id: EntityId,
        config: RoomServerConfig,
        attached_radio: EntityId,
        name: String,
        sim_params: &FirmwareSimulationParams,
    ) -> Result<Self, FirmwareError> {
        let dll = Arc::new(FirmwareDll::load(FirmwareType::RoomServer)?);

        // The firmware expects an "expanded" 64-byte private key which is the SHA512 hash
        // of the 32-byte seed, with the first 32 bytes clamped for Ed25519.
        use sha2::{Sha512, Digest};
        let mut hasher = Sha512::new();
        hasher.update(&config.base.private_key);
        let hash_result = hasher.finalize();
        let mut prv_key_64 = [0u8; 64];
        prv_key_64.copy_from_slice(&hash_result);
        // Apply Ed25519 clamping to the scalar (first 32 bytes)
        prv_key_64[0] &= 248;
        prv_key_64[31] &= 63;
        prv_key_64[31] |= 64;

        // Use initial RTC time from simulation parameters
        let initial_rtc: u32 = sim_params.initial_rtc_secs as u32;

        let node_config = NodeConfig::default()
            .with_keys(&config.base.public_key, &prv_key_64)
            .with_initial_time(0, initial_rtc)
            .with_rng_seed(config.base.rng_seed)
            .with_name(&name)
            .with_spin_detection(
                sim_params.spin_detection_threshold,
                sim_params.idle_loops_before_yield,
            )
            .with_spin_logging(
                sim_params.log_spin_detection,
                sim_params.log_loop_iterations,
            );

        // Create the persistent node
        let node = OwnedFirmwareNode::new(dll, &node_config)
            .map_err(|e| FirmwareError::Dll(e))?;

        Ok(RoomServerFirmware {
            id,
            name,
            config,
            attached_radio,
            attached_cli_agent: None,
            node,
            current_millis: 0,
            initial_rtc,
            pending_tx: None,
            awaiting_tx_complete: false,
            wake_millis: 0,
            startup_time_us: sim_params.startup_time_us,
        })
    }

    /// Get the node name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the configuration.
    pub fn config(&self) -> &RoomServerConfig {
        &self.config
    }

    /// Get the attached CLI agent.
    pub fn attached_cli_agent(&self) -> Option<EntityId> {
        self.attached_cli_agent
    }

    /// Set the attached CLI agent.
    pub fn set_attached_cli_agent(&mut self, cli_agent: EntityId) {
        self.attached_cli_agent = Some(cli_agent);
    }
}

impl Entity for RoomServerFirmware {
    fn entity_id(&self) -> EntityId {
        self.id
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut SimContext) -> Result<(), SimError> {
        self.current_millis = event.time.as_micros() / 1000;
        // Clone the tracer to avoid borrow conflict with ctx
        let tracer = ctx.tracer().clone();

        // Check if we're still in startup delay period
        let event_time_us = event.time.as_micros();
        if event_time_us < self.startup_time_us {
            // Drop all events before startup time (radio, serial, timer)
            tracer.log_event_received(Some(&self.name), self.id, event.time, event);
            log::trace!(
                "[{}] Dropping event during startup delay: {:?} (event_time={}us < startup={}us)",
                self.name, event.payload, event_time_us, self.startup_time_us
            );
            return Ok(());
        }

        // Log event received
        tracer.log_event_received(Some(&self.name), self.id, event.time, event);

        // Log step begin with trigger event description
        let trigger_desc = describe_event(&event.payload);
        tracer.log_firmware_step_begin(Some(&self.name), self.id, event.time, &trigger_desc);

        match &event.payload {
            EventPayload::RadioRxPacket(rx_event) => {
                if !rx_event.was_collided {
                    self.node.inject_radio_rx(
                        &rx_event.packet.payload,
                        rx_event.rssi_dbm as f32,
                        rx_event.snr_db as f32,
                    );
                }
            }
            EventPayload::RadioStateChanged(state_event) => {
                // Notify DLL of state change (for spin detection)
                self.node.notify_state_change(state_event.state_version);
                
                if state_event.new_state == mcsim_common::RadioState::Receiving && self.awaiting_tx_complete {
                    self.node.notify_tx_complete();
                    self.awaiting_tx_complete = false;
                    self.pending_tx = None;
                }
            }
            EventPayload::SerialRx(serial_event) => {
                // Inject serial data from external source (e.g., TCP client)
                self.node.inject_serial_rx(&serial_event.data);
            }
            EventPayload::Timer { timer_id: _ } => {
                // Wake timer or periodic timer - just step below
            }
            _ => return Ok(()),
        }

        // Step the firmware
        let rtc_secs = self.initial_rtc + (self.current_millis / 1000) as u32;
        let result = self.node.step(self.current_millis, rtc_secs);

        self.wake_millis = result.wake_millis;

        // Log yield with details
        let mut yield_details = vec![
            ("wake_ms".to_string(), format!("{}", result.wake_millis)),
            ("current_ms".to_string(), format!("{}", self.current_millis)),
        ];
        if let Some(msg) = result.error_message() {
            yield_details.push(("error".to_string(), msg.to_string()));
        }
        tracer.log_firmware_step_yield(
            Some(&self.name),
            self.id,
            event.time,
            to_trace_yield_reason(result.reason),
            yield_details,
        );

        match result.reason {
            YieldReason::RadioTxStart => {
                let tx_data = result.radio_tx().to_vec();
                let airtime_ms = result.radio_tx_airtime_ms;
                self.pending_tx = Some((tx_data.clone(), airtime_ms));
                self.awaiting_tx_complete = true;

                // Log TX request
                tracer.log_firmware_tx_request(
                    Some(&self.name),
                    self.id,
                    event.time,
                    tx_data.len(),
                    airtime_ms,
                );

                ctx.post_immediate(
                    vec![self.attached_radio],
                    EventPayload::RadioTxRequest(mcsim_common::RadioTxRequestEvent {
                        packet: mcsim_common::LoraPacket::new(tx_data),
                    }),
                );
            }
            YieldReason::Idle => {
                if result.wake_millis > self.current_millis {
                    let delay_us = (result.wake_millis - self.current_millis) * 1000;
                    let delay_ms = delay_us / 1000;
                    tracer.log_timer_scheduled(Some(&self.name), self.id, event.time, 1, delay_ms);
                    ctx.post_event(
                        SimTime::from_micros(delay_us),
                        vec![self.id],
                        EventPayload::Timer { timer_id: 1 },
                    );
                }
            }
            YieldReason::RadioTxComplete => {
                self.awaiting_tx_complete = false;
                self.pending_tx = None;
            }
            YieldReason::Error => {
                if let Some(msg) = result.error_message() {
                    eprintln!("Firmware error: {}", msg);
                }
            }
            _ => {}
        }

        // Emit serial TX data if any
        let serial_tx = result.serial_tx();
        if !serial_tx.is_empty() {
            tracer.log_firmware_serial_tx(Some(&self.name), self.id, event.time, serial_tx);
            
            // Send to self first (for UART/TCP bridge)
            ctx.post_immediate(
                vec![self.id],
                EventPayload::SerialTx(mcsim_common::SerialTxEvent {
                    data: serial_tx.to_vec(),
                }),
            );
            
            // Also forward to attached CLI agent if present
            if let Some(cli_agent_id) = self.attached_cli_agent {
                ctx.post_immediate(
                    vec![cli_agent_id],
                    EventPayload::SerialTx(mcsim_common::SerialTxEvent {
                        data: serial_tx.to_vec(),
                    }),
                );
            }
        }

        // Log firmware output
        let log_str = result.log_output();
        tracer.log_firmware_output(Some(&self.name), self.id, event.time, &log_str);
        emit_firmware_log_events(ctx, &log_str);

        Ok(())
    }
}

impl FirmwareEntity for RoomServerFirmware {
    fn node_id(&self) -> NodeId {
        self.config.base.node_id
    }

    fn public_key(&self) -> &[u8; 32] {
        &self.config.base.public_key
    }

    fn attached_radio(&self) -> EntityId {
        self.attached_radio
    }
    
    fn step_begin(&mut self, event: &Event) {
        // Update current time
        self.current_millis = event.time.as_micros() / 1000;
        
        // Process event payload to inject data into DLL
        match &event.payload {
            EventPayload::RadioRxPacket(rx_event) => {
                if !rx_event.was_collided {
                    self.node.inject_radio_rx(
                        &rx_event.packet.payload,
                        rx_event.rssi_dbm as f32,
                        rx_event.snr_db as f32,
                    );
                }
            }
            EventPayload::RadioStateChanged(state_event) => {
                self.node.notify_state_change(state_event.state_version);
                if state_event.new_state == mcsim_common::RadioState::Receiving && self.awaiting_tx_complete {
                    self.node.notify_tx_complete();
                    self.awaiting_tx_complete = false;
                    self.pending_tx = None;
                }
            }
            EventPayload::SerialRx(serial_event) => {
                self.node.inject_serial_rx(&serial_event.data);
            }
            EventPayload::Timer { timer_id: _ } => {
                // Wake timer - just step below
            }
            _ => {}
        }
        
        // Begin async step
        let rtc_secs = self.initial_rtc + (self.current_millis / 1000) as u32;
        self.node.step_begin(self.current_millis, rtc_secs);
    }
    
    fn step_wait(&mut self) -> FirmwareStepResult {
        let result = self.node.step_wait();
        self.wake_millis = result.wake_millis;
        
        // Determine TX data
        let radio_tx_data = if result.reason == YieldReason::RadioTxStart {
            let tx_data = result.radio_tx().to_vec();
            let airtime_ms = result.radio_tx_airtime_ms;
            self.pending_tx = Some((tx_data.clone(), airtime_ms));
            self.awaiting_tx_complete = true;
            Some((tx_data, airtime_ms))
        } else {
            if result.reason == YieldReason::RadioTxComplete {
                self.awaiting_tx_complete = false;
                self.pending_tx = None;
            }
            None
        };
        
        // Get serial TX data
        let serial_tx = result.serial_tx();
        let serial_tx_data = if serial_tx.is_empty() {
            None
        } else {
            Some(serial_tx.to_vec())
        };
        
        FirmwareStepResult {
            reason: result.reason,
            wake_millis: result.wake_millis,
            radio_tx_data,
            serial_tx_data,
            log_output: result.log_output(),
            error_message: result.error_message(),
        }
    }
}

// ============================================================================
// Factory Functions
// ============================================================================

/// Create a new repeater firmware entity.
pub fn create_repeater(
    id: EntityId,
    config: RepeaterConfig,
    attached_radio: EntityId,
    name: String,
) -> Result<RepeaterFirmware, FirmwareError> {
    RepeaterFirmware::new(id, config, attached_radio, name)
}

/// Create a new repeater firmware entity with simulation parameters.
pub fn create_repeater_with_params(
    id: EntityId,
    config: RepeaterConfig,
    attached_radio: EntityId,
    name: String,
    sim_params: &FirmwareSimulationParams,
) -> Result<RepeaterFirmware, FirmwareError> {
    RepeaterFirmware::with_sim_params(id, config, attached_radio, name, sim_params)
}

/// Create a new companion firmware entity.
pub fn create_companion(
    id: EntityId,
    config: CompanionConfig,
    attached_radio: EntityId,
    name: String,
) -> Result<CompanionFirmware, FirmwareError> {
    CompanionFirmware::new(id, config, attached_radio, None, name)
}

/// Create a new companion firmware entity with simulation parameters.
pub fn create_companion_with_params(
    id: EntityId,
    config: CompanionConfig,
    attached_radio: EntityId,
    name: String,
    sim_params: &FirmwareSimulationParams,
) -> Result<CompanionFirmware, FirmwareError> {
    CompanionFirmware::with_sim_params(id, config, attached_radio, None, name, sim_params)
}

/// Create a new room server firmware entity.
pub fn create_room_server(
    id: EntityId,
    config: RoomServerConfig,
    attached_radio: EntityId,
    name: String,
) -> Result<RoomServerFirmware, FirmwareError> {
    RoomServerFirmware::new(id, config, attached_radio, name)
}

/// Create a new room server firmware entity with simulation parameters.
pub fn create_room_server_with_params(
    id: EntityId,
    config: RoomServerConfig,
    attached_radio: EntityId,
    name: String,
    sim_params: &FirmwareSimulationParams,
) -> Result<RoomServerFirmware, FirmwareError> {
    RoomServerFirmware::with_sim_params(id, config, attached_radio, name, sim_params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repeater_config_default() {
        let config = RepeaterConfig::default();
        // Config just wraps base FirmwareConfig - timing params handled by DLL
        assert_eq!(config.base.rng_seed, 12345);
    }

    #[test]
    fn test_companion_config_default() {
        let config = CompanionConfig::default();
        // Config just wraps base FirmwareConfig - timing params handled by DLL
        assert_eq!(config.base.rng_seed, 12345);
    }

    #[test]
    fn test_room_server_config_default() {
        let config = RoomServerConfig::default();
        assert_eq!(config.room_id, [0u8; 16]);
    }
}
