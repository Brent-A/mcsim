//! # mcsim-model
//!
//! YAML model loading and simulation building for MCSim.
//!
//! This crate provides:
//! - YAML schema types for simulation models
//! - Model loading from files and strings
//! - Simulation building from loaded models
//! - Property registry for configuring simulation parameters
//!
//! ## Dynamic Property System
//!
//! Nodes and edges use a dynamic property system instead of explicit typed fields.
//! Properties are stored in a flat namespace (e.g., `radio/frequency_hz`) and can be:
//! - Defined as built-in defaults in the property registry
//! - Overridden in the `defaults` section of YAML files
//! - Overridden on individual nodes/edges
//!
//! Properties are resolved in order: built-in → defaults → explicit values.

pub mod keys;
pub mod properties;
pub use keys::{generate_keypair, generate_keypair_with_spec, GeneratedKeypair, KeyConfig, KeygenResult, KeySpec, DEFAULT_MAX_KEY_GENERATION_ATTEMPTS};
pub use properties::{
    default_value, get_property_def, properties_by_scope, PropertyDef,
    PropertyScope, PropertyValue, ResolvedProperties,
    UnresolvedProperties, NodeScope, EdgeScope, SimulationScope, ScopeMarker,
    PropertyType, PropertyBaseType, Property, FromPropertyValue,
    // Property constants
    RADIO_FREQUENCY_HZ, RADIO_BANDWIDTH_HZ, RADIO_SPREADING_FACTOR, RADIO_CODING_RATE, RADIO_TX_POWER_DBM,
    COMPANION_CHANNELS, COMPANION_CONTACTS, COMPANION_AUTO_CONTACTS_MAX,
    // Agent properties
    AGENT_DIRECT_ENABLED, AGENT_DIRECT_STARTUP_S, AGENT_DIRECT_STARTUP_JITTER_S, AGENT_DIRECT_TARGETS,
    AGENT_DIRECT_INTERVAL_S, AGENT_DIRECT_INTERVAL_JITTER_S, AGENT_DIRECT_ACK_TIMEOUT_S,
    AGENT_DIRECT_SESSION_MESSAGE_COUNT, AGENT_DIRECT_SESSION_INTERVAL_S, AGENT_DIRECT_SESSION_INTERVAL_JITTER_S,
    AGENT_DIRECT_MESSAGE_COUNT, AGENT_DIRECT_SHUTDOWN_S,
    AGENT_CHANNEL_ENABLED, AGENT_CHANNEL_STARTUP_S, AGENT_CHANNEL_STARTUP_JITTER_S, AGENT_CHANNEL_TARGETS,
    AGENT_CHANNEL_INTERVAL_S, AGENT_CHANNEL_INTERVAL_JITTER_S,
    AGENT_CHANNEL_SESSION_MESSAGE_COUNT, AGENT_CHANNEL_SESSION_INTERVAL_S, AGENT_CHANNEL_SESSION_INTERVAL_JITTER_S,
    AGENT_CHANNEL_MESSAGE_COUNT, AGENT_CHANNEL_SHUTDOWN_S,
    // CLI properties
    CLI_PASSWORD, CLI_COMMANDS,
    // Agent config types
    AgentConfig, DirectMessageConfig, ChannelMessageConfig,
    LINK_MEAN_SNR_DB_AT20DBM, LINK_SNR_STD_DEV, LINK_RSSI_DBM,
    LOCATION_LATITUDE, LOCATION_LONGITUDE, LOCATION_ALTITUDE_M,
    SIMULATION_DURATION_S, SIMULATION_SEED, SIMULATION_UART_BASE_PORT,
    FIRMWARE_TYPE, FIRMWARE_UART_PORT, FIRMWARE_STARTUP_TIME_S, FIRMWARE_STARTUP_JITTER_S,
    KEYS_PRIVATE_KEY, KEYS_PUBLIC_KEY,
    METRICS_GROUPS, METRICS_WARMUP_S, ROOM_SERVER_ROOM_ID,
    // Firmware simulation properties
    FIRMWARE_SPIN_DETECTION_THRESHOLD, FIRMWARE_IDLE_LOOPS_BEFORE_YIELD,
    FIRMWARE_LOG_SPIN_DETECTION, FIRMWARE_LOG_LOOP_ITERATIONS, FIRMWARE_INITIAL_RTC_SECS,
    // Predict-link properties
    PREDICT_FREQUENCY_MHZ, PREDICT_TX_POWER_DBM, PREDICT_SPREADING_FACTOR,
    PREDICT_DEM_DIR, PREDICT_ELEVATION_CACHE_DIR, PREDICT_ELEVATION_SOURCE, PREDICT_ELEVATION_ZOOM_LEVEL, PREDICT_TERRAIN_SAMPLES,
    // Packet tracker properties
    PACKET_TRACKER_EVICTION_AGE_S,
    // Runner properties
    RUNNER_WATCHDOG_TIMEOUT_S, RUNNER_PERIODIC_STATS_INTERVAL_S,
};

use mcsim_common::{EntityId, EntityRegistry, Event, EventPayload, GeoCoord, NodeId, SimTime};
use mcsim_lora::{LinkModel, RadioParams};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use thiserror::Error;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during model operations.
#[derive(Debug, Error)]
pub enum ModelError {
    /// YAML parsing error.
    #[error("YAML parse error: {0}")]
    YamlError(#[from] serde_yaml::Error),

    /// Node not found.
    #[error("Node not found: {0}")]
    NodeNotFound(String),

    /// Invalid configuration.
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Duplicate node name.
    #[error("Duplicate node name: {0}")]
    DuplicateNode(String),

    /// Invalid edge definition.
    #[error("Invalid edge: {from} -> {to}")]
    InvalidEdge {
        /// Source node name.
        from: String,
        /// Destination node name.
        to: String,
    },

    /// IO error.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Firmware error.
    #[error("Firmware error: {0}")]
    FirmwareError(#[from] mcsim_firmware::FirmwareError),

    /// Key generation error.
    #[error("Key generation failed for node '{node}': could not find public key with prefix '{prefix}' after {attempts} attempts")]
    KeyGenerationFailed {
        /// Node name.
        node: String,
        /// Requested prefix.
        prefix: String,
        /// Number of attempts.
        attempts: u32,
    },

    /// Invalid key specification.
    #[error("Invalid key specification: {0}")]
    InvalidKeySpec(String),
}


// ============================================================================
// Public Model API (Resolved Properties)
// ============================================================================

/// A loaded simulation model with resolved properties.
/// 
/// This is the public API for accessing model data. All nodes and edges
/// have their properties fully resolved (built-in defaults → model defaults → explicit values).
#[derive(Debug, Clone)]
pub struct Model {
    /// Resolved nodes.
    nodes: BTreeMap<String, Node>,
    /// Resolved edges.
    edges: BTreeMap<(String, String), Edge>,
    /// Simulation properties.
    simulation: ResolvedProperties<SimulationScope>,
}

impl Model {
    /// Get the nodes in this model.
    pub fn nodes(&self) -> &BTreeMap<String, Node> {
        &self.nodes
    }

    /// Get the edges in this model.
    pub fn edges(&self) -> &BTreeMap<(String, String), Edge> {
        &self.edges
    }

    /// Get the simulation-wide properties.
    pub fn simulation_properties(&self) -> &ResolvedProperties<SimulationScope> {
        &self.simulation
    }

    /// Find a node by name.
    pub fn find_node(&self, name: &str) -> Option<&Node> {
        self.nodes.get(name)
    }
}

/// A node in the simulation model with resolved properties.
#[derive(Debug, Clone)]
pub struct Node {
    /// Node name (unique identifier).
    pub name: String,    
    /// All resolved properties for this node.
    properties: ResolvedProperties<NodeScope>,
}

impl Node {
    /// Get the resolved properties for this node.
    pub fn properties(&self) -> &ResolvedProperties<NodeScope> {
        &self.properties
    }
}

/// An edge (link) in the simulation model with resolved properties.
#[derive(Debug, Clone)]
pub struct Edge {
    /// Source node name.
    pub from: String,
    /// Destination node name.
    pub to: String,
    /// All resolved properties for this edge.
    properties: ResolvedProperties<EdgeScope>,
}

impl Edge {
    /// Get the resolved properties for this edge.
    pub fn properties(&self) -> &ResolvedProperties<EdgeScope> {
        &self.properties
    }
}

// ============================================================================
// YAML Schema Types (Internal)
// ============================================================================

/// Defaults section in YAML.
/// 
/// Properties must be nested under their scope:
/// ```yaml
/// defaults:
///   node:
///     radio:
///       frequency_hz: 910525000
///   edge:
///     link:
///       mean_snr_db_at20dbm: 10.0
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulationDefaultsYaml {
    /// Node defaults
    #[serde(default)]
    node: UnresolvedProperties<NodeScope>,
    /// Edge defaults
    #[serde(default)]
    edge: UnresolvedProperties<EdgeScope>,
}

/// Root simulation model structure (YAML schema).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulationModelYaml {
    /// Default property values (keyed by namespace, then by property name).
    /// These override built-in defaults for all nodes/edges.
    #[serde(default)]
    defaults: SimulationDefaultsYaml,
    /// Node definitions.
    #[serde(default)]
    nodes: Vec<NodeConfigYaml>,
    /// Edge (link) definitions.
    #[serde(default)]
    edges: Vec<EdgeConfigYaml>,
    /// Simulation-wide properties (not tied to nodes/edges).
    #[serde(default)]
    simulation: Option<UnresolvedProperties<SimulationScope>>,
}

/// Node configuration with dynamic properties (YAML schema, internal).
/// Node configuration with dynamic properties (YAML schema, internal).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeConfigYaml {
    /// Node name (must be unique).
    name: String,
    
    /// When true, this node will be removed during model merging.
    #[serde(default)]
    remove: bool,
    
    /// All properties including location, firmware, keys, groups, etc.
    /// Standard properties are deserialized into this map via flatten.
    #[serde(flatten)]
    properties: UnresolvedProperties<NodeScope>,
}

/// Edge (link) configuration with dynamic properties.
///
/// ## Example YAML
///
/// ```yaml
/// edges:
///   - from: "Node1"
///     to: "Node2"
///     link:
///       mean_snr_db_at20dbm: 15.0
///       snr_std_dev: 2.0
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EdgeConfigYaml {
    /// Source node name.
    from: String,
    /// Destination node name.
    to: String,
    /// When true, this edge will be removed during model merging.
    #[serde(default)]
    remove: bool,
    /// Dynamic properties organized by namespace.
    #[serde(flatten)]
    properties: UnresolvedProperties<EdgeScope>,
}


// ============================================================================
// Model Loading
// ============================================================================

/// Load a simulation model from a file.
pub fn load_model(path: &Path) -> Result<Model, ModelError> {
    load_models(&[path])
}

/// Parse a simulation model from a YAML string.
pub fn load_model_from_str(yaml_str: &str) -> Result<Model, ModelError> {
    load_models_from_str(&[yaml_str])
}

/// Load and merge multiple simulation models from files.
/// 
/// Later files override earlier ones:
/// - Defaults from later files override defaults from earlier files
/// - Nodes with the same name from later files override nodes from earlier files
/// - Edges are accumulated (not deduplicated)
/// 
/// Validation (checking node/edge references) is performed only on the final merged model.
pub fn load_models(paths: &[&Path]) -> Result<Model, ModelError> {
    if paths.is_empty() {
        return Err(ModelError::InvalidConfig("No model files provided".to_string()));
    }

    let yaml_strings: Result<Vec<String>, std::io::Error> = paths
        .iter()
        .map(|path| std::fs::read_to_string(path))
        .collect();
    let yaml_strings = yaml_strings?;
    let yaml_strs: Vec<&str> = yaml_strings.iter().map(|s| s.as_str()).collect();
    
    load_models_from_str(&yaml_strs)
}

/// Load and merge multiple simulation models from YAML strings.
/// 
/// Later strings override earlier ones:
/// - Defaults from later strings override defaults from earlier strings
/// - Nodes with the same name from later strings override nodes from earlier strings
/// - Edges are accumulated (not deduplicated)
/// 
/// Validation (checking node/edge references) is performed only on the final merged model.
pub fn load_models_from_str(yaml_strs: &[&str]) -> Result<Model, ModelError> {
    if yaml_strs.is_empty() {
        return Err(ModelError::InvalidConfig("No model strings provided".to_string()));
    }

    let mut node_defaults: ResolvedProperties<NodeScope> = ResolvedProperties::new();
    let mut edge_defaults: ResolvedProperties<EdgeScope> = ResolvedProperties::new();

    let mut yamls = Vec::new();
    for yaml_str in yaml_strs {
        let yaml: SimulationModelYaml = serde_yaml::from_str(yaml_str)?;
        
        node_defaults.apply_unresolved(&yaml.defaults.node);
        edge_defaults.apply_unresolved(&yaml.defaults.edge);

        yamls.push(yaml);
    }

    let mut nodes: BTreeMap<String, Node> = BTreeMap::new();
    let mut edges = BTreeMap::new();
    let mut simulation: ResolvedProperties<SimulationScope> = ResolvedProperties::new();

    for yaml in yamls {
        // Merge nodes
        for node in yaml.nodes {
            if node.remove {
                if !nodes.contains_key(&node.name) {
                    return Err(ModelError::NodeNotFound(node.name.clone()));
                }
                nodes.remove(&node.name);

                // When we remove a node, we also remove any connected edges
                edges.retain(|(from, to), _| from != &node.name && to != &node.name);
            } else if let Some(existing) = nodes.get_mut(&node.name) {
                // Node already exists - merge properties from the overlay
                existing.properties.apply_unresolved(&node.properties);
            } else {
                let mut properties = node_defaults.clone();
                properties.apply_unresolved(&node.properties);
                let node = Node {
                    name: node.name.clone(),
                    properties,
                };
                nodes.insert(node.name.clone(), node);
            }
        }

        // Merge edges
        for edge in yaml.edges {
            let key = (edge.from.clone(), edge.to.clone());
            if edge.remove {
                if !edges.contains_key(&key) {
                    return Err(ModelError::InvalidEdge {
                        from: edge.from.clone(),
                        to: edge.to.clone(),
                    });
                }
                edges.remove(&key);
            } else {
                let mut properties = edge_defaults.clone();
                properties.apply_unresolved(&edge.properties);
                let edge = Edge {
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                    properties,
                };
                edges.insert(key, edge);
            }
        }

        // Validate edges refer to existing nodes
        for (from, to) in edges.keys() {
            if !nodes.contains_key(from) {
                return Err(ModelError::InvalidEdge {
                    from: from.clone(),
                    to: to.clone(),
                });
            }
            if !nodes.contains_key(to) {
                return Err(ModelError::InvalidEdge {
                    from: from.clone(),
                    to: to.clone(),
                });
            }
        }

        // Merge simulation properties
        if let Some(sim_props) = yaml.simulation {
            simulation.apply_unresolved(&sim_props);
        }
    }

    Ok(Model {
        nodes,
        edges,
        simulation
    })
}


// ============================================================================
// Model Building
// ============================================================================

/// Information about a node for display purposes.
#[derive(Debug, Clone)]
pub struct NodeInfo {
    /// Node name from the model.
    pub name: String,
    /// Node type (Repeater, Companion, RoomServer).
    pub node_type: String,
    /// Entity ID of the firmware entity.
    pub firmware_entity_id: u64,
    /// Entity ID of the radio entity.
    pub radio_entity_id: u64,
    /// Entity ID of the agent entity (for Companions).
    pub agent_entity_id: Option<u64>,
    /// Entity ID of the CLI agent entity (for Repeaters/RoomServers with CLI config).
    pub cli_agent_entity_id: Option<u64>,
    /// Geographic location of the node.
    pub location: GeoCoord,
    /// Public key (32 bytes).
    pub public_key: [u8; 32],
    /// Specific TCP port for UART connection (if specified in config).
    pub uart_port: Option<u16>,
}

/// Result of building a simulation from a model.
pub struct BuiltSimulation {
    /// Entity registry with all entities.
    pub entities: EntityRegistry,
    /// Link model.
    pub link_model: LinkModel,
    /// Initial events to seed the simulation.
    pub initial_events: Vec<Event>,
    /// Information about each node for display.
    pub node_infos: Vec<NodeInfo>,
}

/// Build a simulation from a model.
pub fn build_simulation(model: &Model, seed: u64) -> Result<BuiltSimulation, ModelError> {
    use mcsim_firmware::{
        RepeaterFirmware, RepeaterConfig, CompanionFirmware, CompanionConfig, 
        RoomServerFirmware, RoomServerConfig, FirmwareConfig,
        FirmwareSimulationParams,
    };
    use mcsim_lora::Radio;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    let mut entities = EntityRegistry::new();
    let mut link_model = LinkModel::new();
    let mut initial_events = Vec::new();
    let mut event_id_counter: u64 = 0;

    // RNG for generating node keys
    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    // Read firmware simulation parameters from model simulation properties
    let sim_props = model.simulation_properties();
    let firmware_sim_params = FirmwareSimulationParams {
        spin_detection_threshold: sim_props.get(&FIRMWARE_SPIN_DETECTION_THRESHOLD),
        idle_loops_before_yield: sim_props.get(&FIRMWARE_IDLE_LOOPS_BEFORE_YIELD),
        log_spin_detection: sim_props.get(&FIRMWARE_LOG_SPIN_DETECTION),
        log_loop_iterations: sim_props.get(&FIRMWARE_LOG_LOOP_ITERATIONS),
        initial_rtc_secs: sim_props.get(&FIRMWARE_INITIAL_RTC_SECS),
        startup_time_us: 0, // Default; overridden per-node based on node properties
    };

    // Maps for entity ID allocation and name lookup
    let mut next_entity_id: u64 = 0;
    
    // Create Graph entity first (entity ID 0)
    let graph_id = EntityId::new(next_entity_id);
    next_entity_id += 1;
    
    // Use BTreeMap instead of HashMap to ensure deterministic iteration order.
    // This is critical for simulation reproducibility - HashMap iteration order
    // is non-deterministic and can cause different results between runs.
    let mut node_name_to_radio_id: std::collections::BTreeMap<String, EntityId> = std::collections::BTreeMap::new();
    let mut node_name_to_firmware_id: std::collections::BTreeMap<String, EntityId> = std::collections::BTreeMap::new();
    let mut node_name_to_node_id: std::collections::BTreeMap<String, NodeId> = std::collections::BTreeMap::new();
    let mut node_name_to_agent_id: std::collections::BTreeMap<String, EntityId> = std::collections::BTreeMap::new();
    let mut node_name_to_cli_agent_id: std::collections::BTreeMap<String, EntityId> = std::collections::BTreeMap::new();
    let mut node_name_to_firmware_type: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    // Map from node name to computed firmware startup time (with jitter applied)
    let mut node_name_to_firmware_startup_time: std::collections::BTreeMap<String, SimTime> = std::collections::BTreeMap::new();
    
    // Collect node information for display
    let mut node_infos: Vec<NodeInfo> = Vec::new();

    // Pre-pass: allocate entity IDs for radio, firmware, and agent (if present)
    for (_, node) in model.nodes() {
        let radio_id = EntityId::new(next_entity_id);
        next_entity_id += 1;
        let firmware_id = EntityId::new(next_entity_id);
        next_entity_id += 1;

        node_name_to_radio_id.insert(node.name.clone(), radio_id);
        node_name_to_firmware_id.insert(node.name.clone(), firmware_id);

        // Companions always get an agent (for protocol/channel setup even if messaging disabled)
        let firmware_type: String = node.properties().get(&FIRMWARE_TYPE);
        node_name_to_firmware_type.insert(node.name.clone(), firmware_type.to_lowercase());
        let is_companion = firmware_type.to_lowercase() == "companion";
        
        if is_companion {
            let agent_id = EntityId::new(next_entity_id);
            next_entity_id += 1;
            node_name_to_agent_id.insert(node.name.clone(), agent_id);
        }
        
        // Repeaters and room servers get a CLI agent if they have CLI configuration
        let is_repeater = firmware_type.to_lowercase() == "repeater";
        let is_room_server = firmware_type.to_lowercase() == "room_server" || firmware_type.to_lowercase() == "roomserver";
        if is_repeater || is_room_server {
            let cli_password: Option<String> = node.properties().get(&CLI_PASSWORD);
            let cli_commands: Vec<String> = node.properties().get(&CLI_COMMANDS);
            if cli_password.is_some() || !cli_commands.is_empty() {
                let cli_agent_id = EntityId::new(next_entity_id);
                next_entity_id += 1;
                node_name_to_cli_agent_id.insert(node.name.clone(), cli_agent_id);
            }
        }
    }

    // First pass: create entities for each node
    for (_, node) in model.nodes() {
        // Get pre-allocated entity IDs
        let radio_id = *node_name_to_radio_id.get(&node.name).unwrap();
        let firmware_id = *node_name_to_firmware_id.get(&node.name).unwrap();
        let agent_id = node_name_to_agent_id.get(&node.name).copied();

        // Properties are already resolved
        let resolved = node.properties();

        let key_config = KeyConfig {
            private_key: KeySpec::parse(&resolved.get(&properties::KEYS_PRIVATE_KEY))?,
            public_key: KeySpec::parse(&resolved.get(&properties::KEYS_PUBLIC_KEY))?,
        };

        // Generate node identity (public/private key pair) based on key config
        let generated = generate_keypair_with_spec(&mut rng, &key_config, &node.name)?;
        let public_key = generated.public_key;
        let private_key = generated.private_key;
        let node_id = NodeId::from_bytes(public_key);
        
        log::debug!(
            "Node '{}': public_key={}", 
            node.name, 
            hex::encode(&public_key[..8])
        );

        // Store mappings
        node_name_to_radio_id.insert(node.name.clone(), radio_id);
        node_name_to_firmware_id.insert(node.name.clone(), firmware_id);
        node_name_to_node_id.insert(node.name.clone(), node_id);

        // Get radio params from resolved properties
        let radio_params = RadioParams {
            frequency_hz: resolved.get(&RADIO_FREQUENCY_HZ),
            bandwidth_hz: resolved.get(&RADIO_BANDWIDTH_HZ),
            spreading_factor: resolved.get(&RADIO_SPREADING_FACTOR),
            coding_rate: resolved.get(&RADIO_CODING_RATE),
            tx_power_dbm: resolved.get(&RADIO_TX_POWER_DBM),
        };

        // Create radio entity with config
        let position = GeoCoord {
            latitude: resolved.get(&properties::LOCATION_LATITUDE),
            longitude: resolved.get(&properties::LOCATION_LONGITUDE),
            altitude_m: resolved.get(&properties::LOCATION_ALTITUDE_M),
        };
        let radio_config = mcsim_lora::RadioConfig {
            params: radio_params,
            rx_to_tx_turnaround: SimTime::from_micros(100),
            tx_to_rx_turnaround: SimTime::from_micros(100),
            graph_entity: graph_id,
        };
        
        // Get firmware type
        let firmware_type: String = resolved.get(&properties::FIRMWARE_TYPE);

        let groups: Vec<String> = resolved.get(&properties::METRICS_GROUPS);
        
        // Create metric labels for this node
        let metric_labels = mcsim_metrics::MetricLabels::new(
            node.name.clone(),
            &firmware_type,
        ).with_groups(groups);
        
        let radio = Radio::new(radio_id, radio_config, position.clone(), firmware_id, metric_labels);
        entities.register(Box::new(radio));

        // Generate a unique RNG seed for this node
        let node_rng_seed: u32 = rng.gen();

        // Get UART port if specified
        let uart_port = resolved.get(&properties::FIRMWARE_UART_PORT);

        // Compute firmware startup time with jitter
        let startup_time_s: f64 = resolved.get(&properties::FIRMWARE_STARTUP_TIME_S);
        let startup_jitter_s: f64 = resolved.get(&properties::FIRMWARE_STARTUP_JITTER_S);
        let jitter: f64 = if startup_jitter_s > 0.0 {
            rng.gen_range(-startup_jitter_s..=startup_jitter_s)
        } else {
            0.0
        };
        let firmware_startup_time = SimTime::from_secs((startup_time_s + jitter).max(0.0));
        node_name_to_firmware_startup_time.insert(node.name.clone(), firmware_startup_time);

        // Create node-specific firmware simulation params with startup time
        let node_firmware_sim_params = mcsim_firmware::FirmwareSimulationParams {
            startup_time_us: firmware_startup_time.as_micros(),
            ..firmware_sim_params.clone()
        };

        // Create firmware entity based on type
        match firmware_type.to_lowercase().as_str() {
            "repeater" => {
                let fw_config = RepeaterConfig {
                    base: FirmwareConfig {
                        node_id,
                        public_key,
                        private_key,
                        encryption_key: None,
                        rng_seed: node_rng_seed,
                    },
                };
                let mut firmware = RepeaterFirmware::with_sim_params(firmware_id, fw_config, radio_id, node.name.clone(), &node_firmware_sim_params)?;
                
                // Attach CLI agent if one was allocated for this node
                if let Some(cli_agent_id) = node_name_to_cli_agent_id.get(&node.name) {
                    firmware.set_attached_cli_agent(*cli_agent_id);
                }
                
                entities.register(Box::new(firmware));
                
                // Add initial timer event to start the firmware (after startup delay)
                initial_events.push(Event {
                    id: mcsim_common::EventId(event_id_counter),
                    time: firmware_startup_time + SimTime::from_millis(10),
                    source: firmware_id,
                    targets: vec![firmware_id],
                    payload: EventPayload::Timer { timer_id: 0 },
                });
                event_id_counter += 1;

                // Record node info
                let cli_agent_id = node_name_to_cli_agent_id.get(&node.name).copied();
                node_infos.push(NodeInfo {
                    name: node.name.clone(),
                    node_type: "Repeater".to_string(),
                    firmware_entity_id: firmware_id.0,
                    radio_entity_id: radio_id.0,
                    agent_entity_id: None,
                    cli_agent_entity_id: cli_agent_id.map(|id| id.0),
                    location: position.clone(),
                    public_key,
                    uart_port,
                });
            }
            "companion" => {
                let fw_config = CompanionConfig {
                    base: FirmwareConfig {
                        node_id,
                        public_key,
                        private_key,
                        encryption_key: None,
                        rng_seed: node_rng_seed,
                    },
                };
                let firmware = CompanionFirmware::with_sim_params(firmware_id, fw_config, radio_id, agent_id, node.name.clone(), &node_firmware_sim_params)?;
                entities.register(Box::new(firmware));
                
                // Add initial timer event (after startup delay)
                initial_events.push(Event {
                    id: mcsim_common::EventId(event_id_counter),
                    time: firmware_startup_time + SimTime::from_millis(10),
                    source: firmware_id,
                    targets: vec![firmware_id],
                    payload: EventPayload::Timer { timer_id: 0 },
                });
                event_id_counter += 1;
                
                node_infos.push(NodeInfo {
                    name: node.name.clone(),
                    node_type: "Companion".to_string(),
                    firmware_entity_id: firmware_id.0,
                    radio_entity_id: radio_id.0,
                    agent_entity_id: agent_id.map(|id| id.0),
                    cli_agent_entity_id: None,
                    location: position.clone(),
                    public_key,
                    uart_port,
                });
            }
            "room_server" | "roomserver" => {
                // Get room ID from properties (defaults to zeros if not specified)
                let room_id_opt: Option<String> = resolved.get(&ROOM_SERVER_ROOM_ID);
                let room_id = match room_id_opt {
                    Some(hex_str) => {
                        let bytes = hex::decode(&hex_str).unwrap_or_else(|_| vec![0u8; 16]);
                        let mut arr = [0u8; 16];
                        let len = bytes.len().min(16);
                        arr[..len].copy_from_slice(&bytes[..len]);
                        arr
                    }
                    None => [0u8; 16],
                };
                
                let fw_config = RoomServerConfig {
                    base: FirmwareConfig {
                        node_id,
                        public_key,
                        private_key,
                        encryption_key: None,
                        rng_seed: node_rng_seed,
                    },
                    room_id,
                };
                
                let mut firmware = RoomServerFirmware::with_sim_params(firmware_id, fw_config, radio_id, node.name.clone(), &node_firmware_sim_params)?;
                
                // Attach CLI agent if one was allocated for this node
                if let Some(cli_agent_id) = node_name_to_cli_agent_id.get(&node.name) {
                    firmware.set_attached_cli_agent(*cli_agent_id);
                }
                
                entities.register(Box::new(firmware));
                
                // Add initial timer event (after startup delay)
                initial_events.push(Event {
                    id: mcsim_common::EventId(event_id_counter),
                    time: firmware_startup_time + SimTime::from_millis(10),
                    source: firmware_id,
                    targets: vec![firmware_id],
                    payload: EventPayload::Timer { timer_id: 0 },
                });
                event_id_counter += 1;
                
                let cli_agent_id = node_name_to_cli_agent_id.get(&node.name).copied();
                node_infos.push(NodeInfo {
                    name: node.name.clone(),
                    node_type: "RoomServer".to_string(),
                    firmware_entity_id: firmware_id.0,
                    radio_entity_id: radio_id.0,
                    agent_entity_id: None,
                    cli_agent_entity_id: cli_agent_id.map(|id| id.0),
                    location: position.clone(),
                    public_key,
                    uart_port,
                });
            }
            _ => {
                return Err(ModelError::InvalidConfig(
                    format!("Unknown firmware_type '{}' for node '{}'", firmware_type, node.name)
                ));
            }
        }
    }

    // Second pass: create agent entities and initial events
    for (_, node_config) in &model.nodes {
        // Skip if no agent was allocated for this node
        let agent_id = match node_name_to_agent_id.get(&node_config.name) {
            Some(id) => *id,
            None => continue,
        };
        
        let props = node_config.properties();
        let direct_enabled: bool = props.get(&AGENT_DIRECT_ENABLED);
        let channel_enabled: bool = props.get(&AGENT_CHANNEL_ENABLED);
        
        let firmware_id = *node_name_to_firmware_id.get(&node_config.name).unwrap();
        let node_id = *node_name_to_node_id.get(&node_config.name).unwrap();

        let auto_contacts_max: u32 = props.get(&COMPANION_AUTO_CONTACTS_MAX);

        // Helper to build contact list when companion/contacts is null.
        // We want as many repeaters/other nodes as possible (up to firmware capacity),
        // but keep deterministic ordering: companions first, then others.
        // Limit: auto_contacts_max to avoid exceeding firmware capacity.
        let build_auto_contact_list = |exclude_self: &str| -> Vec<String> {
            let mut companions: Vec<String> = Vec::new();
            let mut others: Vec<String> = Vec::new();

            for (name, fw_type) in &node_name_to_firmware_type {
                if name == exclude_self {
                    continue;
                }
                if fw_type == "companion" {
                    companions.push(name.clone());
                } else {
                    others.push(name.clone());
                }
            }

            companions.sort();
            others.sort();

            let mut result = companions;
            result.extend(others);
            result.truncate(auto_contacts_max as usize);
            result
        };

        // Build contacts list from companion/contacts FIRST
        // These are node names that will be resolved to their public keys
        // Note: Skip adding self as a contact (node shouldn't add itself)
        // 
        // Helper to create contact with correct type based on firmware type
        let make_contact = |name: &str, node_id: NodeId| -> mcsim_agents::ContactTarget {
            match node_name_to_firmware_type.get(name).map(|s| s.as_str()) {
                Some("repeater") => mcsim_agents::ContactTarget::repeater(name.to_string(), node_id),
                Some("room_server") | Some("roomserver") => mcsim_agents::ContactTarget::room_server(name.to_string(), node_id),
                _ => mcsim_agents::ContactTarget::chat(name.to_string(), node_id),
            }
        };
        
        let companion_contact_names: Option<Vec<String>> = props.get(&COMPANION_CONTACTS);
        
        // Determine the resolved contact names (for use in DM target filtering)
        let resolved_contact_names: Vec<String> = match &companion_contact_names {
            Some(names) => names.iter()
                .filter(|name| *name != &node_config.name) // Skip self
                .filter(|name| node_name_to_node_id.contains_key(*name)) // Only include valid nodes
                .cloned()
                .collect(),
            None => {
                // If null, auto-populate contacts up to capacity (companions first, then others).
                build_auto_contact_list(&node_config.name)
            }
        };
        
        let contacts: Vec<mcsim_agents::ContactTarget> = resolved_contact_names.iter()
            .filter_map(|name| {
                node_name_to_node_id.get(name).map(|node_id| make_contact(name, *node_id))
            })
            .collect();

        // Build direct message config
        // If agent/direct/targets is specified, use those nodes
        // If agent/direct/targets is NULL, derive from contact list (companions only)
        let direct_targets: Option<Vec<String>> = props.get(&AGENT_DIRECT_TARGETS);
        let direct_target_ids: Vec<NodeId> = match direct_targets {
            Some(names) => names.iter()
                .filter_map(|name| node_name_to_node_id.get(name).copied())
                .collect(),
            None => {
                // If null, derive DM targets from resolved contact list (companions only).
                // This ensures we only DM nodes that are in our contact list.
                resolved_contact_names.iter()
                    .filter(|name| {
                        // Only include companion nodes as DM targets
                        node_name_to_firmware_type.get(*name).map(|t| t == "companion").unwrap_or(false)
                    })
                    .filter_map(|name| node_name_to_node_id.get(name).copied())
                    .collect()
            }
        };

        let direct_config = mcsim_agents::DirectMessageConfig {
            enabled: direct_enabled,
            startup_s: props.get(&AGENT_DIRECT_STARTUP_S),
            startup_jitter_s: props.get(&AGENT_DIRECT_STARTUP_JITTER_S),
            targets: direct_target_ids,
            interval_s: props.get(&AGENT_DIRECT_INTERVAL_S),
            interval_jitter_s: props.get(&AGENT_DIRECT_INTERVAL_JITTER_S),
            ack_timeout_s: props.get(&AGENT_DIRECT_ACK_TIMEOUT_S),
            session_message_count: props.get(&AGENT_DIRECT_SESSION_MESSAGE_COUNT),
            session_interval_s: props.get(&AGENT_DIRECT_SESSION_INTERVAL_S),
            session_interval_jitter_s: props.get(&AGENT_DIRECT_SESSION_INTERVAL_JITTER_S),
            message_count: props.get(&AGENT_DIRECT_MESSAGE_COUNT),
            shutdown_s: props.get(&AGENT_DIRECT_SHUTDOWN_S),
        };

        // Build channel message config
        // agent/channel/targets - channels to send messages TO (also subscribed)
        // companion/channels - channels to subscribe to but NOT send to
        let agent_channel_names: Vec<String> = props.get(&AGENT_CHANNEL_TARGETS);
        let companion_channel_names: Option<Vec<String>> = props.get(&COMPANION_CHANNELS);
        
        // Agent channel targets (send + subscribe)
        // Default to "Public" if empty
        let channel_targets: Vec<mcsim_agents::ChannelTarget> = if agent_channel_names.is_empty() {
            vec![mcsim_agents::ChannelTarget::from_name("Public".to_string())]
        } else {
            agent_channel_names.iter()
                .map(|name| mcsim_agents::ChannelTarget::from_name(name.clone()))
                .collect()
        };
        
        // Companion channels (subscribe only) - exclude any that are already in targets
        let target_set: std::collections::HashSet<&String> = agent_channel_names.iter().collect();
        let subscribe_only: Vec<mcsim_agents::ChannelTarget> = companion_channel_names
            .unwrap_or_default()
            .into_iter()
            .filter(|name| !target_set.contains(name))
            .map(|name| mcsim_agents::ChannelTarget::from_name(name))
            .collect();

        let channel_config = mcsim_agents::ChannelMessageConfig {
            enabled: channel_enabled,
            startup_s: props.get(&AGENT_CHANNEL_STARTUP_S),
            startup_jitter_s: props.get(&AGENT_CHANNEL_STARTUP_JITTER_S),
            targets: channel_targets,
            subscribe_only,
            interval_s: props.get(&AGENT_CHANNEL_INTERVAL_S),
            interval_jitter_s: props.get(&AGENT_CHANNEL_INTERVAL_JITTER_S),
            session_message_count: props.get(&AGENT_CHANNEL_SESSION_MESSAGE_COUNT),
            session_interval_s: props.get(&AGENT_CHANNEL_SESSION_INTERVAL_S),
            session_interval_jitter_s: props.get(&AGENT_CHANNEL_SESSION_INTERVAL_JITTER_S),
            message_count: props.get(&AGENT_CHANNEL_MESSAGE_COUNT),
            shutdown_s: props.get(&AGENT_CHANNEL_SHUTDOWN_S),
        };

        let agent_config = mcsim_agents::AgentConfig {
            name: node_config.name.clone(),
            direct: direct_config,
            channel: channel_config,
            contacts,
        };

        let agent = mcsim_agents::Agent::new(agent_id, agent_config, node_id, firmware_id);
        entities.register(Box::new(agent));

        // Get the firmware startup time for this node's agent startup
        let firmware_startup_time = *node_name_to_firmware_startup_time.get(&node_config.name).unwrap();
        
        // Agent starts 1 second after firmware startup
        initial_events.push(Event {
            id: mcsim_common::EventId(event_id_counter),
            time: firmware_startup_time + SimTime::from_secs(1.0),
            source: agent_id,
            targets: vec![agent_id],
            payload: EventPayload::Timer { timer_id: 0 },
        });
        event_id_counter += 1;
    }

    // Third pass: create CLI agents for repeaters and room servers with CLI configuration
    for (_, node_config) in &model.nodes {
        // Skip if no CLI agent was allocated for this node
        let cli_agent_id = match node_name_to_cli_agent_id.get(&node_config.name) {
            Some(id) => *id,
            None => continue,
        };

        let props = node_config.properties();
        let firmware_id = *node_name_to_firmware_id.get(&node_config.name).unwrap();
        let node_id = *node_name_to_node_id.get(&node_config.name).unwrap();

        // Build CLI agent config
        let cli_password: Option<String> = props.get(&CLI_PASSWORD);
        let cli_commands: Vec<String> = props.get(&CLI_COMMANDS);

        let cli_agent_config = mcsim_agents::CliAgentConfig {
            name: node_config.name.clone(),
            password: cli_password,
            commands: cli_commands,
        };

        log::debug!(
            "Creating CliAgent for '{}' with {} commands",
            node_config.name,
            cli_agent_config.commands.len()
        );

        let cli_agent = mcsim_agents::CliAgent::new(cli_agent_id, cli_agent_config, node_id, firmware_id);
        entities.register(Box::new(cli_agent));

        // Get the firmware startup time for this node's CLI agent startup
        let firmware_startup_time = *node_name_to_firmware_startup_time.get(&node_config.name).unwrap();

        // Add initial timer event to start the CLI agent after firmware has started
        // CLI agent starts 2 seconds after firmware startup (1s buffer after firmware init)
        initial_events.push(Event {
            id: mcsim_common::EventId(event_id_counter),
            time: firmware_startup_time + SimTime::from_secs(2.0),
            source: cli_agent_id,
            targets: vec![cli_agent_id],
            payload: EventPayload::Timer { timer_id: 0 },
        });
        event_id_counter += 1;
    }

    // Fourth pass: populate link model from edges
    for (_,edge) in &model.edges {
        let from_radio = node_name_to_radio_id.get(&edge.from)
            .ok_or_else(|| ModelError::NodeNotFound(edge.from.clone()))?;
        let to_radio = node_name_to_radio_id.get(&edge.to)
            .ok_or_else(|| ModelError::NodeNotFound(edge.to.clone()))?;

        // Use the already-resolved edge properties
        let edge_props = edge.properties();

        let mean_snr: f64 = edge_props.get(&LINK_MEAN_SNR_DB_AT20DBM);
        let snr_std_dev: f64 = edge_props.get(&LINK_SNR_STD_DEV);
        let rssi_dbm: f64 = edge_props.get(&LINK_RSSI_DBM);

        link_model.add_link(*from_radio, *to_radio, mean_snr, snr_std_dev, rssi_dbm);
    }

    // Create and register the Graph entity with the populated link model
    let graph = mcsim_lora::Graph::new(graph_id, link_model.clone());
    entities.register(Box::new(graph));

    Ok(BuiltSimulation {
        entities,
        link_model,
        initial_events,
        node_infos,
    })
}

/// Model loader utility.
pub struct ModelLoader;

impl ModelLoader {
    /// Load a model from a file.
    pub fn load_from_file(path: &Path) -> Result<Model, ModelError> {
        load_model(path)
    }

    /// Load a model from a string.
    pub fn load_from_str(yaml: &str) -> Result<Model, ModelError> {
        load_model_from_str(yaml)
    }

    /// Build a simulation from a model.
    pub fn build_simulation(
        model: &Model,
        seed: u64,
    ) -> Result<BuiltSimulation, ModelError> {
        build_simulation(model, seed)
    }
}

