//! Property constant definitions.
//!
//! This module contains all property definitions organized by category.
//! Each property is a compile-time constant with type-safe access.
//!
//! ## Maintenance Guidelines
//!
//! When adding or modifying properties:
//!
//! 1. **Description is user-facing** - The description string (second parameter to
//!    `Property::new`) is displayed by `mcsim properties` command. Include ALL relevant
//!    help information in the description, not just in doc comments.
//!
//! 2. **Doc comments are for developers** - Use `///` comments for implementation notes,
//!    internal details, or additional context that helps developers but isn't needed by users.
//!
//! 3. **Description should be self-contained** - Users should understand the property fully
//!    from the description alone, including:
//!    - What the property controls
//!    - Valid values or formats (e.g., "7-12" for spreading factor)
//!    - Special values (e.g., "*" for random, "null" behavior)
//!    - Any important defaults or behaviors
//!
//! ## Property Categories
//!
//! ### Node Properties
//! - **Radio** - LoRa radio configuration (frequency, bandwidth, spreading factor, etc.)
//! - **Keys** - Cryptographic key specifications
//! - **Location** - Geographic coordinates
//! - **Firmware** - Firmware type and UART configuration
//! - **Repeater** - Repeater-specific settings
//! - **Companion** - Companion device settings
//! - **Room Server** - Room server configuration
//! - **Agent** - Agent behavior configuration
//! - **Metrics** - Metrics and grouping configuration
//! - **CLI** - Properties for nodes with CLIs (repeaters and room servers)
//!
//! ### Edge Properties
//! - **Link** - Radio link characteristics (SNR, RSSI)
//!
//! ### Simulation Properties
//! - **Simulation** - Global simulation settings (duration, seed, ports)

use super::types::{
    EdgeScope, NodeScope, Property, PropertyBaseType, PropertyDefault, PropertyType,
    SimulationScope,
};

// ============================================================================
// Radio Properties (Node scope)
// ============================================================================

/// Radio frequency in Hz.
pub const RADIO_FREQUENCY_HZ: Property<u32, NodeScope> = Property::new(
    "radio/frequency_hz",
    "Radio frequency in Hz",
    PropertyDefault::Integer(910_525_000),
)
.with_unit("Hz");

/// Radio bandwidth in Hz.
pub const RADIO_BANDWIDTH_HZ: Property<u32, NodeScope> = Property::new(
    "radio/bandwidth_hz",
    "Radio bandwidth in Hz",
    PropertyDefault::Integer(62_500),
)
.with_unit("Hz");

/// LoRa spreading factor (7-12).
pub const RADIO_SPREADING_FACTOR: Property<u8, NodeScope> = Property::new(
    "radio/spreading_factor",
    "LoRa spreading factor (7-12)",
    PropertyDefault::Integer(7),
);

/// LoRa coding rate denominator (5-8, representing 4/5 to 4/8).
pub const RADIO_CODING_RATE: Property<u8, NodeScope> = Property::new(
    "radio/coding_rate",
    "LoRa coding rate denominator (5-8, representing 4/5 to 4/8)",
    PropertyDefault::Integer(5),
);

/// Transmit power in dBm.
pub const RADIO_TX_POWER_DBM: Property<i8, NodeScope> = Property::new(
    "radio/tx_power_dbm",
    "Transmit power in dBm",
    PropertyDefault::Integer(20),
)
.with_unit("dBm");

// ============================================================================
// Keys Properties (Node scope)
// ============================================================================

/// Private key specification for node identity.
pub const KEYS_PRIVATE_KEY: Property<String, NodeScope> = Property::new(
    "keys/private_key",
    "Private key specification. Modes: '*' = random keypair, 'prefix*' = generate until public key starts with prefix, or 64 hex chars for exact key",
    PropertyDefault::String("*"),
);

/// Public key specification for node identity.
pub const KEYS_PUBLIC_KEY: Property<String, NodeScope> = Property::new(
    "keys/public_key",
    "Public key specification. Modes: '*' = random keypair, 'prefix*' = generate until public key starts with prefix, or 64 hex chars for exact key",
    PropertyDefault::String("*"),
);

// ============================================================================
// Location Properties (Node scope)
// ============================================================================

/// Latitude of the node in decimal degrees.
pub const LOCATION_LATITUDE: Property<f64, NodeScope> = Property::new(
    "location/latitude",
    "Latitude of the node in decimal degrees",
    PropertyDefault::Float(0.0),
)
.with_unit("degrees")
.with_aliases(&["location/lat"]);

/// Longitude of the node in decimal degrees.
pub const LOCATION_LONGITUDE: Property<f64, NodeScope> = Property::new(
    "location/longitude",
    "Longitude of the node in decimal degrees",
    PropertyDefault::Float(0.0),
)
.with_unit("degrees")
.with_aliases(&["location/lon"]);

/// Altitude of the node in meters (nullable).
pub const LOCATION_ALTITUDE_M: Property<Option<f64>, NodeScope> = Property::new(
    "location/altitude_m",
    "Altitude of the node in meters",
    PropertyDefault::Null,
)
.with_type(PropertyType::new(PropertyBaseType::Float).nullable())
.with_unit("m")
.with_aliases(&["location/alt"]);

// ============================================================================
// Firmware Properties (Node scope)
// ============================================================================

/// Firmware type of the node ("repeater", "companion", "roomserver").
pub const FIRMWARE_TYPE: Property<String, NodeScope> = Property::new(
    "firmware/type",
    "Firmware type of the node (\"repeater\", \"companion\", \"roomserver\")",
    PropertyDefault::String("Repeater"),
);

/// TCP port to expose the UART interface on (nullable).
pub const FIRMWARE_UART_PORT: Property<Option<u16>, NodeScope> = Property::new(
    "firmware/uart_port",
    "TCP port to expose the UART interface on",
    PropertyDefault::Null,
)
.with_type(PropertyType::new(PropertyBaseType::Integer).nullable())
.with_unit("port");

/// Startup delay time for the firmware in seconds.
/// The firmware will not process any events (radio, serial, timer) until this time has passed.
/// The actual startup time is: startup_time_s + random(-startup_time_jitter_s, +startup_time_jitter_s)
pub const FIRMWARE_STARTUP_TIME_S: Property<f64, NodeScope> = Property::new(
    "firmware/startup_time_s",
    "Startup delay time for the firmware in seconds. The firmware will not process any events until this time has passed. Use with startup_time_jitter_s for randomization",
    PropertyDefault::Float(0.0),
)
.with_unit("s");

/// Jitter for the firmware startup time in seconds.
/// The jitter is applied as a uniform random value in the range [-jitter, +jitter].
pub const FIRMWARE_STARTUP_JITTER_S: Property<f64, NodeScope> = Property::new(
    "firmware/startup_time_jitter_s",
    "Jitter for the firmware startup time in seconds. Applied as uniform random value in range [-jitter, +jitter]",
    PropertyDefault::Float(0.0),
)
.with_unit("s");

// ============================================================================
// Companion Properties (Node scope)
// ============================================================================

/// Channels to subscribe to at startup.
pub const COMPANION_CHANNELS: Property<Option<Vec<String>>, NodeScope> = Property::new(
    "companion/channels",
    "Channels to subscribe to at startup (list of channel names). If null, no channels are added at startup",
    PropertyDefault::Null,
)
.with_type(PropertyType::new(PropertyBaseType::String).array());

/// Contacts to add at startup.
pub const COMPANION_CONTACTS: Property<Option<Vec<String>>, NodeScope> = Property::new(
    "companion/contacts",
    "Contacts to add at startup (list of node names, resolved to public keys). If null, nodes must exchange advertisements",
    PropertyDefault::Null,
)
.with_type(PropertyType::new(PropertyBaseType::String).array());

// ============================================================================
// Messaging Properties (Node scope)
// ============================================================================

/// Timeout waiting for ACK when message sent via flood routing.
pub const MESSAGING_FLOOD_ACK_TIMEOUT_S: Property<f64, NodeScope> = Property::new(
    "messaging/flood_ack_timeout_s",
    "Timeout waiting for ACK when message sent via flood routing",
    PropertyDefault::Float(30.0),
)
.with_unit("s");

/// Timeout per hop when message sent via direct routing.
pub const MESSAGING_DIRECT_ACK_TIMEOUT_PER_HOP_S: Property<f64, NodeScope> = Property::new(
    "messaging/direct_ack_timeout_per_hop_s",
    "Timeout per hop when message sent via direct routing (total timeout = path_length × this value)",
    PropertyDefault::Float(5.0),
)
.with_unit("s");

/// Number of attempts using direct routing before clearing path and switching to flood.
pub const MESSAGING_DIRECT_ATTEMPTS: Property<u8, NodeScope> = Property::new(
    "messaging/direct_attempts",
    "Number of attempts using direct routing before clearing path and switching to flood (only applies when path is known)",
    PropertyDefault::Integer(3),
);

/// Number of flood attempts after direct attempts are exhausted.
pub const MESSAGING_FLOOD_ATTEMPTS_AFTER_DIRECT: Property<u8, NodeScope> = Property::new(
    "messaging/flood_attempts_after_direct",
    "Number of flood attempts after direct attempts exhausted (only applies when starting with a known path)",
    PropertyDefault::Integer(1),
);

/// Number of flood attempts when no path is known to the recipient.
pub const MESSAGING_FLOOD_ATTEMPTS_NO_PATH: Property<u8, NodeScope> = Property::new(
    "messaging/flood_attempts_no_path",
    "Number of flood attempts when no path is known to the recipient",
    PropertyDefault::Integer(3),
);

// ============================================================================
// Room Server Properties (Node scope)
// ============================================================================

/// Room ID for room server (16-byte hex string).
pub const ROOM_SERVER_ROOM_ID: Property<Option<String>, NodeScope> = Property::new(
    "room_server/room_id",
    "Room ID for room server (16-byte hex string)",
    PropertyDefault::Null,
);

// ============================================================================
// CLI Properties (Node scope)
// ============================================================================

/// Admin password for CLI access.
pub const CLI_PASSWORD: Property<Option<String>, NodeScope> = Property::new(
    "cli/password",
    "Admin password for CLI access. If set, the CLI agent sends this password at startup to authenticate",
    PropertyDefault::String("admin"),
)
.with_type(PropertyType::new(PropertyBaseType::String).nullable());

/// CLI commands to execute at startup.
pub const CLI_COMMANDS: Property<Vec<String>, NodeScope> = Property::new(
    "cli/commands",
    "CLI commands to execute at startup. List of raw command strings (e.g., ['set rxdelay 0', 'set name MyNode'])",
    PropertyDefault::Vec(&[]),
)
.with_type(PropertyType::new(PropertyBaseType::String).array());

// ============================================================================
// Agent Direct Message Properties (Node scope)
// ============================================================================

/// Enable sending direct messages.
pub const AGENT_DIRECT_ENABLED: Property<bool, NodeScope> = Property::new(
    "agent/direct/enabled",
    "Enable sending direct messages",
    PropertyDefault::Bool(false),
);

/// Wait time before starting direct messages.
pub const AGENT_DIRECT_STARTUP_S: Property<f64, NodeScope> = Property::new(
    "agent/direct/startup_s",
    "Wait time before starting direct messages",
    PropertyDefault::Float(0.0),
)
.with_unit("s");

/// Standard deviation in the randomness of the startup interval.
pub const AGENT_DIRECT_STARTUP_JITTER_S: Property<f64, NodeScope> = Property::new(
    "agent/direct/startup_jitter_s",
    "Standard deviation in the randomness of the startup interval",
    PropertyDefault::Float(0.0),
)
.with_unit("s");

/// Names of nodes to target direct messages to.
pub const AGENT_DIRECT_TARGETS: Property<Option<Vec<String>>, NodeScope> = Property::new(
    "agent/direct/targets",
    "Names of nodes to target direct messages to. Multiple targets rotate per message. If null, all other companions are targeted randomly",
    PropertyDefault::Null,
)
.with_type(PropertyType::new(PropertyBaseType::String).array().nullable())
.with_unit("node_name");

/// Interval after receiving an ack (or timeout) before sending the next message.
pub const AGENT_DIRECT_INTERVAL_S: Property<f64, NodeScope> = Property::new(
    "agent/direct/interval_s",
    "Interval after receiving an ack (or timeout) before sending the next message",
    PropertyDefault::Float(5.0),
)
.with_unit("s");

/// Standard deviation of the randomness in the message interval timer.
pub const AGENT_DIRECT_INTERVAL_JITTER_S: Property<f64, NodeScope> = Property::new(
    "agent/direct/interval_jitter_s",
    "Standard deviation of the randomness in the message interval timer",
    PropertyDefault::Float(0.0),
)
.with_unit("s");

/// Timeout waiting for an ACK before proceeding.
pub const AGENT_DIRECT_ACK_TIMEOUT_S: Property<f64, NodeScope> = Property::new(
    "agent/direct/ack_timeout_s",
    "Timeout waiting for an ACK before proceeding",
    PropertyDefault::Float(10.0),
)
.with_unit("s");

/// Pause after this many messages before waiting for another session.
pub const AGENT_DIRECT_SESSION_MESSAGE_COUNT: Property<Option<u32>, NodeScope> = Property::new(
    "agent/direct/session_message_count",
    "Pause after this many messages before waiting for session_interval_s. If null, messaging continues without session breaks",
    PropertyDefault::Null,
)
.with_type(PropertyType::new(PropertyBaseType::Integer).nullable());

/// Time to wait for the next session.
pub const AGENT_DIRECT_SESSION_INTERVAL_S: Property<f64, NodeScope> = Property::new(
    "agent/direct/session_interval_s",
    "Time to wait for the next session",
    PropertyDefault::Float(3600.0),
)
.with_unit("s");

/// Standard deviation of randomness in the session interval timer.
pub const AGENT_DIRECT_SESSION_INTERVAL_JITTER_S: Property<f64, NodeScope> = Property::new(
    "agent/direct/session_interval_jitter_s",
    "Standard deviation of randomness in the session interval timer",
    PropertyDefault::Float(0.0),
)
.with_unit("s");

/// Total count of messages before the agent stops sending.
pub const AGENT_DIRECT_MESSAGE_COUNT: Property<Option<u32>, NodeScope> = Property::new(
    "agent/direct/message_count",
    "Total count of messages before the agent stops sending. If null, sends indefinitely",
    PropertyDefault::Null,
)
.with_type(PropertyType::new(PropertyBaseType::Integer).nullable());

/// Time before the agent stops sending.
pub const AGENT_DIRECT_SHUTDOWN_S: Property<Option<f64>, NodeScope> = Property::new(
    "agent/direct/shutdown_s",
    "Time before the agent stops sending. If null, sends indefinitely",
    PropertyDefault::Null,
)
.with_type(PropertyType::new(PropertyBaseType::Float).nullable())
.with_unit("s");

// ============================================================================
// Agent Channel Message Properties (Node scope)
// ============================================================================

/// Enable sending channel messages.
pub const AGENT_CHANNEL_ENABLED: Property<bool, NodeScope> = Property::new(
    "agent/channel/enabled",
    "Enable sending channel messages",
    PropertyDefault::Bool(false),
);

/// Wait time before starting channel messages.
pub const AGENT_CHANNEL_STARTUP_S: Property<f64, NodeScope> = Property::new(
    "agent/channel/startup_s",
    "Wait time before starting channel messages",
    PropertyDefault::Float(0.0),
)
.with_unit("s");

/// Standard deviation in the randomness of the startup interval.
pub const AGENT_CHANNEL_STARTUP_JITTER_S: Property<f64, NodeScope> = Property::new(
    "agent/channel/startup_jitter_s",
    "Standard deviation in the randomness of the startup interval",
    PropertyDefault::Float(0.0),
)
.with_unit("s");

/// Names of channels to target channel messages to.
pub const AGENT_CHANNEL_TARGETS: Property<Vec<String>, NodeScope> = Property::new(
    "agent/channel/targets",
    "Names of channels to target channel messages to. Multiple channels rotate per message",
    PropertyDefault::Vec(&[PropertyDefault::String("Public")]),
)
.with_type(PropertyType::new(PropertyBaseType::String).array());

/// Interval after sending before sending the next message.
pub const AGENT_CHANNEL_INTERVAL_S: Property<f64, NodeScope> = Property::new(
    "agent/channel/interval_s",
    "Interval after sending before sending the next message",
    PropertyDefault::Float(5.0),
)
.with_unit("s");

/// Standard deviation of the randomness in the message interval timer.
pub const AGENT_CHANNEL_INTERVAL_JITTER_S: Property<f64, NodeScope> = Property::new(
    "agent/channel/interval_jitter_s",
    "Standard deviation of the randomness in the message interval timer",
    PropertyDefault::Float(0.0),
)
.with_unit("s");

/// Pause after this many messages before waiting for another session.
pub const AGENT_CHANNEL_SESSION_MESSAGE_COUNT: Property<Option<u32>, NodeScope> = Property::new(
    "agent/channel/session_message_count",
    "Pause after this many messages before waiting for session_interval_s. If null, messaging continues without session breaks",
    PropertyDefault::Null,
)
.with_type(PropertyType::new(PropertyBaseType::Integer).nullable());

/// Time to wait for the next session.
pub const AGENT_CHANNEL_SESSION_INTERVAL_S: Property<f64, NodeScope> = Property::new(
    "agent/channel/session_interval_s",
    "Time to wait for the next session",
    PropertyDefault::Float(3600.0),
)
.with_unit("s");

/// Standard deviation of randomness in the session interval timer.
pub const AGENT_CHANNEL_SESSION_INTERVAL_JITTER_S: Property<f64, NodeScope> = Property::new(
    "agent/channel/session_interval_jitter_s",
    "Standard deviation of randomness in the session interval timer",
    PropertyDefault::Float(0.0),
)
.with_unit("s");

/// Total count of messages before the agent stops sending.
pub const AGENT_CHANNEL_MESSAGE_COUNT: Property<Option<u32>, NodeScope> = Property::new(
    "agent/channel/message_count",
    "Total count of messages before the agent stops sending. If null, sends indefinitely",
    PropertyDefault::Null,
)
.with_type(PropertyType::new(PropertyBaseType::Integer).nullable());

/// Time before the agent stops sending.
pub const AGENT_CHANNEL_SHUTDOWN_S: Property<Option<f64>, NodeScope> = Property::new(
    "agent/channel/shutdown_s",
    "Time before the agent stops sending. If null, sends indefinitely",
    PropertyDefault::Null,
)
.with_type(PropertyType::new(PropertyBaseType::Float).nullable())
.with_unit("s");

// ============================================================================
// Metrics Properties (Node scope)
// ============================================================================

/// Groups this node belongs to for metrics aggregation.
pub const METRICS_GROUPS: Property<Vec<String>, NodeScope> = Property::new(
    "metrics/groups",
    "Groups this node belongs to for metrics aggregation",
    PropertyDefault::Vec(&[]),
)
.with_type(PropertyType::new(PropertyBaseType::String).array());

// ============================================================================
// Metrics Properties (Simulation scope)
// ============================================================================

/// Warmup period before metrics collection starts.
pub const METRICS_WARMUP_S: Property<f64, SimulationScope> = Property::new(
    "metrics/warmup_s",
    "Warmup period in seconds before metrics collection starts. Metrics recorded during this period are discarded to allow the simulation to reach steady state",
    PropertyDefault::Float(0.0),
)
.with_unit("s");

// ============================================================================
// Link Properties (Edge scope)
// ============================================================================

/// Mean signal-to-noise ratio at the receiver when transmitter is at 20 dBm.
pub const LINK_MEAN_SNR_DB_AT20DBM: Property<f64, EdgeScope> = Property::new(
    "link/mean_snr_db_at20dbm",
    "Mean signal-to-noise ratio at the receiver when transmitter is at 20 dBm",
    PropertyDefault::Float(0.0),
)
.with_unit("dB")
.with_aliases(&["mean_snr_db_at20dbm"]);

/// Standard deviation of SNR for Gaussian fading model.
pub const LINK_SNR_STD_DEV: Property<f64, EdgeScope> = Property::new(
    "link/snr_std_dev",
    "Standard deviation of SNR for Gaussian fading model",
    PropertyDefault::Float(1.8),
)
.with_unit("dB")
.with_aliases(&["snr_std_dev"]);

/// Received signal strength indicator.
pub const LINK_RSSI_DBM: Property<f64, EdgeScope> = Property::new(
    "link/rssi_dbm",
    "Received signal strength indicator",
    PropertyDefault::Float(-100.0),
)
.with_unit("dBm")
.with_aliases(&["rssi_dbm"]);

// ============================================================================
// Simulation Properties (Simulation scope)
// ============================================================================

/// Total simulation duration.
pub const SIMULATION_DURATION_S: Property<f64, SimulationScope> = Property::new(
    "simulation/duration_s",
    "Total simulation duration",
    PropertyDefault::Float(3600.0),
)
.with_unit("s");

/// Random seed for reproducible simulations.
pub const SIMULATION_SEED: Property<i64, SimulationScope> = Property::new(
    "simulation/seed",
    "Random seed for reproducible simulations",
    PropertyDefault::Integer(0),
);

/// Base TCP port for UART connections.
pub const SIMULATION_UART_BASE_PORT: Property<u16, SimulationScope> = Property::new(
    "simulation/uart_base_port",
    "Base TCP port for UART connections",
    PropertyDefault::Integer(9000),
)
.with_unit("port");

// ============================================================================
// Radio Thresholds (Simulation scope)
// ============================================================================

/// Capture effect threshold - minimum SNR difference for capture.
pub const RADIO_CAPTURE_EFFECT_THRESHOLD_DB: Property<f64, SimulationScope> = Property::new(
    "radio/capture_effect_threshold_db",
    "Minimum SNR difference required for capture effect (stronger signal captures the receiver)",
    PropertyDefault::Float(6.0),
)
.with_unit("dB");

/// LoRa noise floor at 250kHz bandwidth.
pub const RADIO_NOISE_FLOOR_DBM: Property<f64, SimulationScope> = Property::new(
    "radio/noise_floor_dbm",
    "LoRa noise floor at 250kHz bandwidth",
    PropertyDefault::Float(-120.0),
)
.with_unit("dBm");

/// RX to TX turnaround time.
pub const RADIO_RX_TO_TX_TURNAROUND_US: Property<u32, SimulationScope> = Property::new(
    "radio/rx_to_tx_turnaround_us",
    "RX to TX turnaround time (radio mode switch delay)",
    PropertyDefault::Integer(100),
)
.with_unit("µs");

/// TX to RX turnaround time.
pub const RADIO_TX_TO_RX_TURNAROUND_US: Property<u32, SimulationScope> = Property::new(
    "radio/tx_to_rx_turnaround_us",
    "TX to RX turnaround time (radio mode switch delay)",
    PropertyDefault::Integer(100),
)
.with_unit("µs");

// ============================================================================
// SNR Thresholds per Spreading Factor (Simulation scope)
// ============================================================================

/// SNR demodulation threshold for SF7.
pub const RADIO_SNR_THRESHOLD_SF7_DB: Property<f64, SimulationScope> = Property::new(
    "radio/snr_threshold_sf7_db",
    "SNR demodulation threshold for SF7",
    PropertyDefault::Float(-7.5),
)
.with_unit("dB");

/// SNR demodulation threshold for SF8.
pub const RADIO_SNR_THRESHOLD_SF8_DB: Property<f64, SimulationScope> = Property::new(
    "radio/snr_threshold_sf8_db",
    "SNR demodulation threshold for SF8",
    PropertyDefault::Float(-10.0),
)
.with_unit("dB");

/// SNR demodulation threshold for SF9.
pub const RADIO_SNR_THRESHOLD_SF9_DB: Property<f64, SimulationScope> = Property::new(
    "radio/snr_threshold_sf9_db",
    "SNR demodulation threshold for SF9",
    PropertyDefault::Float(-12.5),
)
.with_unit("dB");

/// SNR demodulation threshold for SF10.
pub const RADIO_SNR_THRESHOLD_SF10_DB: Property<f64, SimulationScope> = Property::new(
    "radio/snr_threshold_sf10_db",
    "SNR demodulation threshold for SF10",
    PropertyDefault::Float(-15.0),
)
.with_unit("dB");

/// SNR demodulation threshold for SF11.
pub const RADIO_SNR_THRESHOLD_SF11_DB: Property<f64, SimulationScope> = Property::new(
    "radio/snr_threshold_sf11_db",
    "SNR demodulation threshold for SF11",
    PropertyDefault::Float(-17.5),
)
.with_unit("dB");

/// SNR demodulation threshold for SF12.
pub const RADIO_SNR_THRESHOLD_SF12_DB: Property<f64, SimulationScope> = Property::new(
    "radio/snr_threshold_sf12_db",
    "SNR demodulation threshold for SF12",
    PropertyDefault::Float(-20.0),
)
.with_unit("dB");

// ============================================================================
// Link Quality Classification (Simulation scope)
// ============================================================================

/// Minimum margin for excellent link quality.
pub const LINK_MARGIN_EXCELLENT_DB: Property<f64, SimulationScope> = Property::new(
    "link/margin_excellent_db",
    "Minimum margin for excellent link quality",
    PropertyDefault::Float(10.0),
)
.with_unit("dB");

/// Minimum margin for good link quality.
pub const LINK_MARGIN_GOOD_DB: Property<f64, SimulationScope> = Property::new(
    "link/margin_good_db",
    "Minimum margin for good link quality",
    PropertyDefault::Float(5.0),
)
.with_unit("dB");

/// Minimum margin for marginal link quality.
pub const LINK_MARGIN_MARGINAL_DB: Property<f64, SimulationScope> = Property::new(
    "link/margin_marginal_db",
    "Minimum margin for marginal link quality",
    PropertyDefault::Float(0.0),
)
.with_unit("dB");

// ============================================================================
// Predict-Link Parameters (Simulation scope)
// ============================================================================

/// Radio frequency in MHz for link prediction.
pub const PREDICT_FREQUENCY_MHZ: Property<f64, SimulationScope> = Property::new(
    "predict/radio/frequency_mhz",
    "Radio frequency in MHz for link prediction",
    PropertyDefault::Float(910.525),
)
.with_unit("MHz");

/// TX power in dBm for link prediction.
pub const PREDICT_TX_POWER_DBM: Property<i8, SimulationScope> = Property::new(
    "predict/radio/tx_power_dbm",
    "TX power in dBm for link prediction",
    PropertyDefault::Integer(20),
)
.with_unit("dBm");

/// LoRa spreading factor (7-12) for link prediction.
pub const PREDICT_SPREADING_FACTOR: Property<u8, SimulationScope> = Property::new(
    "predict/radio/spreading_factor",
    "LoRa spreading factor (7-12) for link prediction",
    PropertyDefault::Integer(7),
);

/// Path to DEM data directory for terrain lookups (used for USGS tiles).
pub const PREDICT_DEM_DIR: Property<String, SimulationScope> = Property::new(
    "predict/terrain/dem_dir",
    "Path to DEM data directory for terrain lookups (used for USGS tiles)",
    PropertyDefault::String("./dem_data"),
);

/// Path to elevation tile cache directory for AWS terrain tiles.
pub const PREDICT_ELEVATION_CACHE_DIR: Property<String, SimulationScope> = Property::new(
    "predict/terrain/elevation_cache_dir",
    "Path to elevation tile cache directory for AWS terrain tiles",
    PropertyDefault::String("./elevation_cache"),
);

/// Zoom level for AWS elevation tiles.
pub const PREDICT_ELEVATION_ZOOM_LEVEL: Property<u8, SimulationScope> = Property::new(
    "predict/terrain/elevation_zoom_level",
    "Zoom level for AWS elevation tiles (1-14, higher = more detail). Zoom 12 provides ~38m resolution at equator",
    PropertyDefault::Integer(12),
);

/// Elevation data source for terrain lookups.
pub const PREDICT_ELEVATION_SOURCE: Property<String, SimulationScope> = Property::new(
    "predict/terrain/source",
    "Elevation data source: 'aws' (fetch 512x512 GeoTIFF tiles from AWS Open Data) or 'local_dem' (use local USGS DEM files from dem_dir)",
    PropertyDefault::String("aws"),
);

/// Number of terrain samples along the path for link prediction.
pub const PREDICT_TERRAIN_SAMPLES: Property<u32, SimulationScope> = Property::new(
    "predict/terrain/samples",
    "Number of terrain samples along the path for link prediction",
    PropertyDefault::Integer(100),
);

// ============================================================================
// ITM Prediction Parameters (Simulation scope)
// ============================================================================

/// Minimum path distance for ITM model.
pub const ITM_MIN_DISTANCE_M: Property<f64, SimulationScope> = Property::new(
    "predict/itm/min_distance_m",
    "Minimum path distance for ITM model",
    PropertyDefault::Float(1000.0),
)
.with_unit("m");

/// Number of elevation points in terrain profile.
pub const ITM_TERRAIN_SAMPLES: Property<u32, SimulationScope> = Property::new(
    "predict/itm/terrain_samples",
    "Number of elevation points in terrain profile",
    PropertyDefault::Integer(100),
);

/// Radio climate setting (continental_temperate, etc.).
pub const ITM_CLIMATE: Property<String, SimulationScope> = Property::new(
    "predict/itm/climate",
    "Radio climate setting (continental_temperate, etc.)",
    PropertyDefault::String("continental_temperate"),
);

/// Radio polarization (vertical, horizontal).
pub const ITM_POLARIZATION: Property<String, SimulationScope> = Property::new(
    "predict/itm/polarization",
    "Radio polarization (vertical, horizontal)",
    PropertyDefault::String("vertical"),
);

/// Ground relative permittivity (epsilon).
pub const ITM_GROUND_PERMITTIVITY: Property<f64, SimulationScope> = Property::new(
    "predict/itm/ground_permittivity",
    "Ground relative permittivity (epsilon)",
    PropertyDefault::Float(15.0),
);

/// Ground conductivity (S/m).
pub const ITM_GROUND_CONDUCTIVITY: Property<f64, SimulationScope> = Property::new(
    "predict/itm/ground_conductivity",
    "Ground conductivity (S/m)",
    PropertyDefault::Float(0.005),
)
.with_unit("S/m");

/// Surface refractivity (N-units).
pub const ITM_SURFACE_REFRACTIVITY: Property<f64, SimulationScope> = Property::new(
    "predict/itm/surface_refractivity",
    "Surface refractivity (N-units)",
    PropertyDefault::Float(301.0),
);

// ============================================================================
// FSPL Prediction (Simulation scope)
// ============================================================================

/// Minimum distance for free-space path loss model.
pub const FSPL_MIN_DISTANCE_M: Property<f64, SimulationScope> = Property::new(
    "predict/fspl/min_distance_m",
    "Minimum distance for free-space path loss model",
    PropertyDefault::Float(1.0),
)
.with_unit("m");

// ============================================================================
// Colocated Prediction (Simulation scope)
// ============================================================================

/// Fixed near-field path loss for colocated nodes.
pub const COLOCATED_PATH_LOSS_DB: Property<f64, SimulationScope> = Property::new(
    "predict/colocated/path_loss_db",
    "Fixed near-field path loss for colocated nodes",
    PropertyDefault::Float(20.0),
)
.with_unit("dB");

// ============================================================================
// LoRa PHY (Simulation scope)
// ============================================================================

/// LoRa preamble symbol count.
pub const LORA_PREAMBLE_SYMBOLS: Property<u32, SimulationScope> = Property::new(
    "lora/preamble_symbols",
    "LoRa preamble symbol count",
    PropertyDefault::Integer(8),
);

// ============================================================================
// Firmware Simulation (Simulation scope)
// ============================================================================

/// Poll count before detecting spin/yield.
pub const FIRMWARE_SPIN_DETECTION_THRESHOLD: Property<u32, SimulationScope> = Property::new(
    "firmware/spin_detection_threshold",
    "Poll count before detecting spin/yield",
    PropertyDefault::Integer(3),
);

/// Idle loop count before yield.
pub const FIRMWARE_IDLE_LOOPS_BEFORE_YIELD: Property<u32, SimulationScope> = Property::new(
    "firmware/idle_loops_before_yield",
    "Idle loop count before yield",
    PropertyDefault::Integer(2),
);

/// Enable debug logging for spin detection.
pub const FIRMWARE_LOG_SPIN_DETECTION: Property<bool, SimulationScope> = Property::new(
    "firmware/log_spin_detection",
    "Enable debug logging for spin detection",
    PropertyDefault::Bool(false),
);

/// Enable debug logging for loop iterations.
pub const FIRMWARE_LOG_LOOP_ITERATIONS: Property<bool, SimulationScope> = Property::new(
    "firmware/log_loop_iterations",
    "Enable debug logging for loop iterations",
    PropertyDefault::Bool(false),
);

/// Initial RTC Unix timestamp.
pub const FIRMWARE_INITIAL_RTC_SECS: Property<u64, SimulationScope> = Property::new(
    "firmware/initial_rtc_secs",
    "Initial RTC Unix timestamp",
    PropertyDefault::Integer(1700000000),
)
.with_unit("s");

// ============================================================================
// Runner (Simulation scope)
// ============================================================================

/// Watchdog stall detection timeout.
pub const RUNNER_WATCHDOG_TIMEOUT_S: Property<u64, SimulationScope> = Property::new(
    "runner/watchdog_timeout_s",
    "Watchdog stall detection timeout",
    PropertyDefault::Integer(10),
)
.with_unit("s");

/// Periodic stats output interval for realtime mode.
pub const RUNNER_PERIODIC_STATS_INTERVAL_S: Property<Option<u64>, SimulationScope> = Property::new(
    "runner/periodic_stats_interval_s",
    "Interval in seconds for periodic stats output in realtime mode (simulation time, event rate, memory usage). If null, periodic stats are disabled",
    PropertyDefault::Integer(10),
)
.with_type(PropertyType::new(PropertyBaseType::Integer).nullable())
.with_unit("s");

// ============================================================================
// Packet Tracker (Simulation scope)
// ============================================================================

/// Maximum age of tracked packets before eviction.
pub const PACKET_TRACKER_EVICTION_AGE_S: Property<Option<f64>, SimulationScope> = Property::new(
    "packet_tracker/eviction_age_s",
    "Maximum age of tracked packets before eviction. Packets older than this are summarized (metrics emitted) and removed to limit memory usage. If null, packets are never evicted. Recommended: 300-600s for long simulations",
    PropertyDefault::Float(10.0),
)
.with_type(PropertyType::new(PropertyBaseType::Float).nullable())
.with_unit("s");
