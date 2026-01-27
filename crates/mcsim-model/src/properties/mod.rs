//! # MCSim Properties System
//!
//! This module provides a type-safe property registry for configuring simulation parameters.
//! Properties can apply to nodes, edges, or the simulation as a whole.
//!
//! ## Module Organization
//!
//! - [`value`] - Property value types and conversion traits
//! - [`types`] - Property type definitions and metadata
//! - [`definitions`] - All property constant definitions (easy to review in one place)
//! - [`registry`] - Property lookup and resolved/unresolved property sets
//! - [`agent`] - Agent configuration types
//!
//! ## Property Namespaces
//!
//! Properties are organized into namespaces using `/` as a separator:
//! - `radio/frequency_hz` - Radio frequency in Hz
//! - `radio/bandwidth_hz` - Radio bandwidth in Hz
//! - `repeater/tx_delay_min_ms` - Minimum TX delay for repeaters
//!
//! ## Type-Safe Property Access
//!
//! Properties are defined with compile-time type information, enabling type-safe access:
//!
//! ```ignore
//! use mcsim_model::properties::{RADIO_FREQUENCY_HZ, ResolvedProperties, NodeScope};
//!
//! let props: ResolvedProperties<NodeScope> = ResolvedProperties::new();
//!
//! // Returns u32 directly - no unwrapping needed
//! let freq: u32 = props.get(&RADIO_FREQUENCY_HZ);
//!
//! // For nullable properties, use get_opt
//! let altitude: Option<f64> = props.get_opt(&LOCATION_ALTITUDE_M);
//! ```
//!
//! ## Property Resolution
//!
//! Properties are resolved in the following order (later overrides earlier):
//! 1. Built-in code defaults
//! 2. Defaults from YAML files (in order loaded)
//! 3. Explicit values on nodes/edges (in order loaded)
//!
//! ## Example YAML
//!
//! ```yaml
//! defaults:
//!   radio:
//!     frequency_hz: 910525000
//!     bandwidth_hz: 62500
//!   repeater:
//!     tx_delay_min_ms: 100
//!
//! nodes:
//!   - name: "Node1"
//!     # Can override any property here
//! ```

pub mod agent;
pub mod definitions;
pub mod registry;
pub mod types;
pub mod value;

// Re-export commonly used types from value
pub use value::{FromPropertyValue, PropertyValue, ToPropertyValue};

// Re-export types and definitions
pub use types::{
    EdgeScope, NodeScope, Property, PropertyBaseType, PropertyDefault, PropertyDef, PropertyScope,
    PropertyType, ScopeMarker, SimulationScope,
};

// Re-export all property definitions for easy access
pub use definitions::{
    // Agent Direct Message
    AGENT_DIRECT_ENABLED,
    AGENT_DIRECT_STARTUP_S,
    AGENT_DIRECT_STARTUP_JITTER_S,
    AGENT_DIRECT_TARGETS,
    AGENT_DIRECT_INTERVAL_S,
    AGENT_DIRECT_INTERVAL_JITTER_S,
    AGENT_DIRECT_ACK_TIMEOUT_S,
    AGENT_DIRECT_SESSION_MESSAGE_COUNT,
    AGENT_DIRECT_SESSION_INTERVAL_S,
    AGENT_DIRECT_SESSION_INTERVAL_JITTER_S,
    AGENT_DIRECT_MESSAGE_COUNT,
    AGENT_DIRECT_SHUTDOWN_S,
    // Agent Channel Message
    AGENT_CHANNEL_ENABLED,
    AGENT_CHANNEL_STARTUP_S,
    AGENT_CHANNEL_STARTUP_JITTER_S,
    AGENT_CHANNEL_TARGETS,
    AGENT_CHANNEL_INTERVAL_S,
    AGENT_CHANNEL_INTERVAL_JITTER_S,
    AGENT_CHANNEL_SESSION_MESSAGE_COUNT,
    AGENT_CHANNEL_SESSION_INTERVAL_S,
    AGENT_CHANNEL_SESSION_INTERVAL_JITTER_S,
    AGENT_CHANNEL_MESSAGE_COUNT,
    AGENT_CHANNEL_SHUTDOWN_S,
    // CLI (Node scope)
    CLI_PASSWORD,
    CLI_COMMANDS,
    // Colocated Prediction (Simulation scope)
    COLOCATED_PATH_LOSS_DB,
    // Companion
    COMPANION_CHANNELS,
    COMPANION_CONTACTS,
    COMPANION_AUTO_CONTACTS_MAX,
    // Firmware (Node scope)
    FIRMWARE_TYPE,
    FIRMWARE_UART_PORT,
    FIRMWARE_STARTUP_TIME_S,
    FIRMWARE_STARTUP_JITTER_S,
    // Firmware Simulation (Simulation scope)
    FIRMWARE_SPIN_DETECTION_THRESHOLD,
    FIRMWARE_IDLE_LOOPS_BEFORE_YIELD,
    FIRMWARE_LOG_SPIN_DETECTION,
    FIRMWARE_LOG_LOOP_ITERATIONS,
    FIRMWARE_INITIAL_RTC_SECS,
    // FSPL Prediction (Simulation scope)
    FSPL_MIN_DISTANCE_M,
    // ITM Prediction Parameters (Simulation scope)
    ITM_MIN_DISTANCE_M,
    ITM_TERRAIN_SAMPLES,
    ITM_CLIMATE,
    ITM_POLARIZATION,
    ITM_GROUND_PERMITTIVITY,
    ITM_GROUND_CONDUCTIVITY,
    ITM_SURFACE_REFRACTIVITY,
    // Keys
    KEYS_PRIVATE_KEY,
    KEYS_PUBLIC_KEY,
    // Link (Edge)
    LINK_MEAN_SNR_DB_AT20DBM,
    LINK_RSSI_DBM,
    LINK_SNR_STD_DEV,
    // Link Quality Classification (Simulation scope)
    LINK_MARGIN_EXCELLENT_DB,
    LINK_MARGIN_GOOD_DB,
    LINK_MARGIN_MARGINAL_DB,
    // Location
    LOCATION_ALTITUDE_M,
    LOCATION_LATITUDE,
    LOCATION_LONGITUDE,
    // LoRa PHY (Simulation scope)
    LORA_PREAMBLE_SYMBOLS,
    // Messaging
    MESSAGING_DIRECT_ACK_TIMEOUT_PER_HOP_S,
    MESSAGING_DIRECT_ATTEMPTS,
    MESSAGING_FLOOD_ACK_TIMEOUT_S,
    MESSAGING_FLOOD_ATTEMPTS_AFTER_DIRECT,
    MESSAGING_FLOOD_ATTEMPTS_NO_PATH,
    // Metrics (Node scope)
    METRICS_GROUPS,
    // Metrics (Simulation scope)
    METRICS_WARMUP_S,
    // Predict-Link Parameters (Simulation scope)
    PREDICT_FREQUENCY_MHZ,
    PREDICT_TX_POWER_DBM,
    PREDICT_SPREADING_FACTOR,
    PREDICT_DEM_DIR,
    PREDICT_ELEVATION_CACHE_DIR,
    PREDICT_ELEVATION_SOURCE,
    PREDICT_ELEVATION_ZOOM_LEVEL,
    PREDICT_TERRAIN_SAMPLES,
    // Radio (Node scope)
    RADIO_BANDWIDTH_HZ,
    RADIO_CODING_RATE,
    RADIO_FREQUENCY_HZ,
    RADIO_SPREADING_FACTOR,
    RADIO_TX_POWER_DBM,
    // Radio Thresholds (Simulation scope)
    RADIO_CAPTURE_EFFECT_THRESHOLD_DB,
    RADIO_NOISE_FLOOR_DBM,
    RADIO_RX_TO_TX_TURNAROUND_US,
    RADIO_TX_TO_RX_TURNAROUND_US,
    // SNR Thresholds per Spreading Factor (Simulation scope)
    RADIO_SNR_THRESHOLD_SF7_DB,
    RADIO_SNR_THRESHOLD_SF8_DB,
    RADIO_SNR_THRESHOLD_SF9_DB,
    RADIO_SNR_THRESHOLD_SF10_DB,
    RADIO_SNR_THRESHOLD_SF11_DB,
    RADIO_SNR_THRESHOLD_SF12_DB,
    // Room Server
    ROOM_SERVER_ROOM_ID,
    // Runner (Simulation scope)
    RUNNER_WATCHDOG_TIMEOUT_S,
    RUNNER_PERIODIC_STATS_INTERVAL_S,
    // Packet Tracker (Simulation scope)
    PACKET_TRACKER_EVICTION_AGE_S,
    // Simulation
    SIMULATION_DURATION_S,
    SIMULATION_SEED,
    SIMULATION_UART_BASE_PORT,
};

// Re-export registry functions and types
pub use registry::{
    default_value, get_property_def, is_known_property, known_namespaces, properties_by_namespace,
    properties_by_scope, PropertySetError, ResolvedProperties, UnresolvedProperties,
    ALL_PROPERTIES,
};

// Re-export agent types
pub use agent::{AgentConfig, DirectMessageConfig, ChannelMessageConfig};
