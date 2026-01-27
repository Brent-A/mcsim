//! # mcsim-runner
//!
//! CLI runner for MCSim simulation.
//!
//! This is the main entry point for running MeshCore network simulations.

mod build_model;

// Use modules and types from the library crate
use mcsim_runner::metric_spec;
use mcsim_runner::metrics_export;
use mcsim_runner::realtime::RealTimeConfig;
#[cfg(feature = "rerun")]
use mcsim_runner::rerun_blueprint;
use mcsim_runner::rerun_logger::{RerunLogger, VisLinkInfo, VisNodeInfo};
use mcsim_runner::uart_server::SyncUartManager;
use mcsim_runner::watchdog::Watchdog;
use mcsim_runner::{EventLoop, ProgressInfo, RunnerError, SimulationStats, SimTime};

use clap::{Parser, Subcommand, ValueEnum};
use mcsim_common::entity_tracer::{EntityTracer, EntityTracerConfig};
use mcsim_model::{build_simulation, load_model};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

// ============================================================================
// Constants
// ============================================================================

/// Default watchdog timeout in seconds (matches RUNNER_WATCHDOG_TIMEOUT_S property)
pub const DEFAULT_WATCHDOG_TIMEOUT_S: u64 = 10;

// ============================================================================
// Duration Parsing
// ============================================================================

/// Parse a duration string with units into seconds.
/// 
/// Supported formats:
/// - Plain number: `60` (interpreted as seconds)
/// - With unit suffix: `60s`, `10m`, `2h`, `1d`
/// - Combined units: `1h30m`, `2d12h`, `1d2h30m45s`
/// 
/// Units:
/// - `s` = seconds
/// - `m` = minutes (60 seconds)
/// - `h` = hours (3600 seconds)
/// - `d` = days (86400 seconds)
fn parse_duration(s: &str) -> Result<f64, String> {
    let s = s.trim();
    
    // If it's just a number, treat as seconds
    if let Ok(secs) = s.parse::<f64>() {
        return Ok(secs);
    }
    
    let mut total_seconds: f64 = 0.0;
    let mut current_number = String::new();
    
    for c in s.chars() {
        if c.is_ascii_digit() || c == '.' {
            current_number.push(c);
        } else {
            if current_number.is_empty() {
                return Err(format!("Invalid duration format: unexpected '{}' in '{}'", c, s));
            }
            
            let value: f64 = current_number.parse()
                .map_err(|_| format!("Invalid number '{}' in duration '{}'", current_number, s))?;
            
            let multiplier = match c {
                's' => 1.0,
                'm' => 60.0,
                'h' => 3600.0,
                'd' => 86400.0,
                _ => return Err(format!("Unknown duration unit '{}' in '{}'. Use s, m, h, or d.", c, s)),
            };
            
            total_seconds += value * multiplier;
            current_number.clear();
        }
    }
    
    // If there's a trailing number without unit, treat as seconds
    if !current_number.is_empty() {
        let value: f64 = current_number.parse()
            .map_err(|_| format!("Invalid number '{}' in duration '{}'", current_number, s))?;
        total_seconds += value;
    }
    
    if total_seconds == 0.0 && !s.is_empty() {
        return Err(format!("Invalid duration format: '{}'", s));
    }
    
    Ok(total_seconds)
}

// ============================================================================
// CLI Configuration
// ============================================================================

/// Output format for metrics at end of simulation.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum MetricsOutputFormat {
    /// JSON format for programmatic consumption.
    Json,
    /// Prometheus text exposition format.
    Prometheus,
    /// CSV format with columns per metric and rows per path.
    Csv,
}

/// MCSim - MeshCore Network Simulator
#[derive(Parser, Debug)]
#[command(name = "mcsim")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Available commands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run a simulation from a YAML model file
    Run(RunnerConfig),
    /// List all available metrics with descriptions and labels
    Metrics,
    /// List all available properties with descriptions and defaults
    Properties,
    /// Predict link quality between two geographic coordinates using DEM and ITM
    PredictLink(PredictLinkConfig),
    /// Estimate true SNR distribution from observed measurements
    EstimateSnr(EstimateSnrConfig),
    /// Build a simulation model from mesh node JSON data
    BuildModel(BuildModelConfig),
    /// Generate Ed25519 keypairs with optional public key prefix matching
    Keygen(KeygenConfig),
}

/// Configuration for key generation
#[derive(Parser, Debug)]
pub struct KeygenConfig {
    /// Public key specification: "*" for random, "ab12*" for prefix matching,
    /// or 64 hex chars for exact key
    #[arg(long, default_value = "*")]
    pub pubkey: String,

    /// Random seed for deterministic generation (default: random from system)
    #[arg(short, long)]
    pub seed: Option<u64>,

    /// Maximum number of attempts for prefix matching (default: 1,000,000)
    #[arg(long)]
    pub max_attempts: Option<u32>,

    /// Output format: text or json (default: text)
    #[arg(long, default_value = "text")]
    pub format: String,
}

/// Configuration for building a model from mesh node data
#[derive(Parser, Debug)]
pub struct BuildModelConfig {
    /// Path to the input JSON file (e.g., sea_nodes_full.json)
    pub input: PathBuf,

    /// Path to the output YAML file (stdout if not specified)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Elevation data source: 'aws' (fetch tiles from AWS) or 'local_dem' (use local USGS files)
    #[arg(long, value_name = "SOURCE", default_value = "aws")]
    pub elevation_source: String,

    /// Cache directory for AWS terrain tiles (default: ./elevation_cache)
    #[arg(long, default_value = "./elevation_cache")]
    pub elevation_cache: PathBuf,

    /// Zoom level for AWS terrain tiles (1-14, default: 12)
    #[arg(long, default_value = "12")]
    pub zoom: u8,

    /// DEM data directory for local USGS files (default: ./dem_data)
    #[arg(long, default_value = "./dem_data")]
    pub dem_dir: PathBuf,

    /// Spreading factor (7-12, default: 7)
    #[arg(long, default_value = "7")]
    pub sf: u8,

    /// TX power in dBm (default: 20)
    #[arg(long, default_value = "20")]
    pub tx_power: i8,

    /// Minimum SNR threshold for including links (default: derived from SF)
    #[arg(long)]
    pub min_snr: Option<f64>,

    /// Maximum number of nodes to include
    #[arg(long)]
    pub max_nodes: Option<usize>,

    /// Include all node types (not just Repeaters)
    #[arg(long)]
    pub all_types: bool,

    /// Antenna height in meters (default: 2.0)
    #[arg(long, default_value = "2.0")]
    pub antenna_height: f64,

    /// Number of terrain samples for link prediction (default: 100)
    #[arg(long, default_value = "100")]
    pub terrain_samples: usize,

    /// Number of hex characters to retain from public key prefix (default: 2)
    #[arg(long, default_value = "2")]
    pub pubkey_prefix_len: usize,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,
}

// ============================================================================
// Predict Link Configuration (supports YAML files + CLI overrides)
// ============================================================================

/// Configuration for link prediction
/// 
/// Supports loading configuration from one or more YAML files (using the standard
/// model format with `simulation` section), with CLI arguments as final overrides.
/// YAML files are merged in order (later files override earlier).
/// 
/// ## Usage Examples
/// 
/// ```bash
/// # All CLI arguments (coordinates are required)
/// mcsim predict-link 47.6062 -122.3321 47.6097 -122.3331
/// 
/// # Config file with CLI override for spreading factor
/// mcsim predict-link --config radio.yaml 47.6062 -122.3321 47.6097 -122.3331 --sf 9
/// 
/// # Multiple config files (merged in order)
/// mcsim predict-link --config defaults.yaml --config site.yaml 47.6 -122.3 47.7 -122.4
/// ```
/// 
/// ## Example YAML (uses standard model format)
/// 
/// ```yaml
/// simulation:
///   predict:
///     radio:
///       frequency_mhz: 915.0
///       tx_power_dbm: 22
///       spreading_factor: 9
///     terrain:
///       dem_dir: "./dem_data"
///       samples: 150
/// ```
#[derive(Parser, Debug)]
#[command(allow_hyphen_values = true)]
pub struct PredictLinkConfig {
    /// Path(s) to YAML configuration file(s). Multiple files are merged in order (later overrides earlier).
    /// Uses the standard model format - only the `simulation` section is read.
    #[arg(short, long = "config", value_name = "FILE")]
    pub configs: Vec<PathBuf>,
    
    /// Latitude of the transmitter (degrees) - REQUIRED
    pub from_lat: f64,
    /// Longitude of the transmitter (degrees) - REQUIRED
    pub from_lon: f64,
    /// Latitude of the receiver (degrees) - REQUIRED
    pub to_lat: f64,
    /// Longitude of the receiver (degrees) - REQUIRED
    pub to_lon: f64,
    /// Height of the transmitter above ground (meters, default: 2.0)
    #[arg(long, default_value = "2.0")]
    pub from_height: f64,
    /// Height of the receiver above ground (meters, default: 2.0)
    #[arg(long, default_value = "2.0")]
    pub to_height: f64,
    /// Frequency in MHz (overrides config file)
    #[arg(long)]
    pub freq: Option<f64>,
    /// DEM data directory for local USGS tiles (overrides config file)
    #[arg(long)]
    pub dem_dir: Option<PathBuf>,
    /// TX power in dBm (overrides config file)
    #[arg(long)]
    pub tx_power: Option<i8>,
    /// Spreading factor (7-12) (overrides config file)
    #[arg(long)]
    pub sf: Option<u8>,
    /// Number of terrain samples (overrides config file)
    #[arg(long)]
    pub samples: Option<usize>,
    /// Elevation data source: 'aws' (fetch tiles from AWS) or 'local_dem' (use local USGS files)
    #[arg(long, value_name = "SOURCE")]
    pub elevation_source: Option<String>,
    /// Cache directory for AWS terrain tiles (default: ./elevation_cache)
    #[arg(long)]
    pub elevation_cache: Option<PathBuf>,
    /// Zoom level for AWS terrain tiles (1-14, default: 12)
    #[arg(long)]
    pub zoom: Option<u8>,
}

/// Resolved configuration with all required fields.
#[derive(Debug)]
pub struct ResolvedPredictLinkConfig {
    pub from_lat: f64,
    pub from_lon: f64,
    pub to_lat: f64,
    pub to_lon: f64,
    pub from_height: f64,
    pub to_height: f64,
    pub freq: f64,
    pub dem_dir: PathBuf,
    pub tx_power: i8,
    pub sf: u8,
    pub samples: usize,
    pub elevation_source: String,  // "aws" or "local_dem"
    pub elevation_cache: PathBuf,
    pub zoom: u8,
}

impl PredictLinkConfig {
    /// Load and merge YAML configs, then apply CLI overrides to produce final config.
    pub fn resolve(&self) -> Result<ResolvedPredictLinkConfig, RunnerError> {
        use mcsim_model::{
            load_models, ResolvedProperties, SimulationScope,
            PREDICT_FREQUENCY_MHZ, PREDICT_TX_POWER_DBM, PREDICT_SPREADING_FACTOR,
            PREDICT_DEM_DIR, PREDICT_TERRAIN_SAMPLES,
            PREDICT_ELEVATION_CACHE_DIR, PREDICT_ELEVATION_SOURCE, PREDICT_ELEVATION_ZOOM_LEVEL,
        };
        
        // Load simulation properties from YAML files (if any)
        let props: ResolvedProperties<SimulationScope> = if self.configs.is_empty() {
            ResolvedProperties::new()
        } else {
            let paths: Vec<&Path> = self.configs.iter().map(|p| p.as_path()).collect();
            let model = load_models(&paths).map_err(|e| {
                RunnerError::ConfigError(format!("Failed to load config: {}", e))
            })?;
            model.simulation_properties().clone()
        };
        
        // Get values from properties, with CLI overrides taking precedence
        let freq = self.freq.unwrap_or_else(|| props.get(&PREDICT_FREQUENCY_MHZ));
        let tx_power = self.tx_power.unwrap_or_else(|| props.get(&PREDICT_TX_POWER_DBM));
        let sf = self.sf.unwrap_or_else(|| props.get(&PREDICT_SPREADING_FACTOR));
        let dem_dir = self.dem_dir.clone().unwrap_or_else(|| PathBuf::from(props.get::<String>(&PREDICT_DEM_DIR)));
        let samples = self.samples.unwrap_or_else(|| props.get::<u32>(&PREDICT_TERRAIN_SAMPLES) as usize);
        let elevation_source = self.elevation_source.clone().unwrap_or_else(|| props.get::<String>(&PREDICT_ELEVATION_SOURCE));
        let elevation_cache = self.elevation_cache.clone().unwrap_or_else(|| PathBuf::from(props.get::<String>(&PREDICT_ELEVATION_CACHE_DIR)));
        let zoom = self.zoom.unwrap_or_else(|| props.get(&PREDICT_ELEVATION_ZOOM_LEVEL));
        
        Ok(ResolvedPredictLinkConfig {
            from_lat: self.from_lat,
            from_lon: self.from_lon,
            to_lat: self.to_lat,
            to_lon: self.to_lon,
            from_height: self.from_height,
            to_height: self.to_height,
            freq,
            dem_dir,
            tx_power,
            sf,
            samples,
            elevation_source,
            elevation_cache,
            zoom,
        })
    }
}

/// Configuration for SNR estimation from observed data
#[derive(Parser, Debug)]
#[command(allow_hyphen_values = true)]
pub struct EstimateSnrConfig {
    /// Observed SNR values in dB (comma-separated or space-separated)
    /// Example: -15.2,-18.1,-14.5 or "-15.2 -18.1 -14.5"
    #[arg(required = true, num_args = 1.., value_delimiter = ',')]
    pub observations: Vec<f64>,

    /// Spreading factor (7-12, default: 7)
    #[arg(long, default_value = "7")]
    pub sf: u8,

    /// Bandwidth in Hz (default: 125000)
    #[arg(long, default_value = "125000")]
    pub bandwidth: f64,

    /// Coding rate denominator (5-8, representing 4/5 to 4/8, default: 5)
    #[arg(long, default_value = "5")]
    pub coding_rate: u8,

    /// Custom SNR threshold override (dB). If not specified, uses the threshold
    /// derived from the spreading factor.
    #[arg(long)]
    pub threshold: Option<f64>,

    /// Output format: text or json (default: text)
    #[arg(long, default_value = "text")]
    pub format: String,
}

/// Configuration for running a simulation
#[derive(Parser, Debug)]
pub struct RunnerConfig {
    /// Path(s) to YAML model file(s). Multiple files are merged in order (later overrides earlier).
    #[arg(required = true)]
    pub models: Vec<PathBuf>,

    /// Simulation duration (omit for realtime mode).
    /// Accepts plain seconds or units: 60, 60s, 10m, 2h, 1d, 1h30m, 2d12h30m45s
    #[arg(short, long, value_parser = parse_duration)]
    pub duration: Option<f64>,

    /// Random seed (default: random)
    #[arg(short, long)]
    pub seed: Option<u64>,

    /// Output trace file path (JSON)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Base TCP port for UART connections (each node gets sequential ports)
    #[arg(short = 'p', long, default_value = "9000")]
    pub uart_base_port: u16,

    /// Enable rerun.io visualization (spawns viewer UI)
    /// Requires the 'rerun' feature to be enabled at compile time.
    #[arg(long)]
    pub rerun: bool,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Trace specific entities. Comma-separated list of entity names or "entity:ID" for IDs.
    /// Use "*" to trace all entities.
    /// Examples: --trace "Alice,Bob" or --trace "entity:1,entity:2" or --trace "*"
    #[arg(long)]
    pub trace: Option<String>,

    /// Output format for metrics at end of simulation.
    #[arg(long, value_enum)]
    pub metrics_output: Option<MetricsOutputFormat>,

    /// File path to write metrics (stdout if not specified).
    #[arg(long)]
    pub metrics_file: Option<PathBuf>,

    /// Metric specifications for export. Can be specified multiple times.
    /// Format: <pattern>[/<breakdown1>[/<breakdown2>...]]
    /// Examples:
    ///   --metric "mcsim.radio.*" (all radio metrics, totals only)
    ///   --metric "mcsim.radio.rx_packets/node" (by node)
    ///   --metric "mcsim.radio.*/payload_type/node" (by payload_type, then by node)
    ///   --metric "mcsim.radio.rx_packets/*" (by all available labels)
    /// Available breakdowns: node, node_type, group, route_type, payload_type, payload_hash
    /// Use /* to automatically break down by all labels present in the metric data.
    /// If not specified, defaults to all metrics with totals only.
    #[arg(long = "metric", value_name = "SPEC")]
    pub metric_specs: Vec<String>,

    /// Speed multiplier for realtime mode (default: 1.0 = real-time).
    /// Values > 1.0 run faster than real-time, < 1.0 run slower.
    /// Only applies when --duration is not specified (realtime mode).
    /// Examples: --speed 2.0 (2x speed), --speed 0.5 (half speed)
    #[arg(long, default_value = "1.0")]
    pub speed: f64,

    /// Maximum catch-up time in milliseconds before warning (default: 100).
    /// In realtime mode, if simulation falls behind by more than this,
    /// a warning is printed.
    #[arg(long, default_value = "100")]
    pub max_catchup_ms: u64,

    /// Break at a specific event number (for debugging slow events).
    /// When the event loop reaches this event, it will pause and print details.
    /// Use with the same --seed value to reproduce event sequences.
    #[arg(long)]
    pub break_at_event: Option<u64>,

    /// Watchdog timeout in seconds (default: 10).
    /// If an event takes longer than this, the watchdog will print details.
    #[arg(long, default_value_t = DEFAULT_WATCHDOG_TIMEOUT_S)]
    pub watchdog_timeout: u64,

    /// Metrics warmup period in seconds.
    /// Metrics recorded during this period are discarded to allow steady state.
    /// Accepts plain seconds or units: 60, 60s, 10m, 2h, etc.
    /// Overrides the metrics/warmup_s property from the model.
    #[arg(long, value_parser = parse_duration)]
    pub metrics_warmup: Option<f64>,
}

// ============================================================================
// Terminal UI Functions
// ============================================================================

/// Get port string for a node, including connection status indicator.
fn get_port_str(
    node_info: &mcsim_model::NodeInfo,
    uart_manager: Option<&SyncUartManager>,
) -> String {
    if let Some(uart_mgr) = uart_manager {
        let port = uart_mgr
            .node_infos()
            .iter()
            .find(|info| info.entity_id == node_info.firmware_entity_id)
            .map(|info| info.port)
            .unwrap_or(0);
        let connected = uart_mgr.is_client_connected(node_info.firmware_entity_id);
        if connected {
            format!("{}*", port)
        } else {
            format!("{}", port)
        }
    } else {
        String::from("-")
    }
}

/// Print the initial entity table at the start of the simulation.
/// This is a simple dump without cursor manipulation - allows logs to scroll naturally.
fn print_initial_table(event_loop: &EventLoop) {
    eprintln!();
    eprintln!(
        "┌{}┬{}┬{}┐",
        "─".repeat(20),
        "─".repeat(14),
        "─".repeat(8)
    );
    eprintln!(
        "│ {:^18} │ {:^12} │ {:^6} │",
        "Node", "Type", "Port"
    );
    eprintln!(
        "├{}┼{}┼{}┤",
        "─".repeat(20),
        "─".repeat(14),
        "─".repeat(8)
    );

    for node_info in event_loop.node_infos() {
        let port_str = get_port_str(node_info, event_loop.uart_manager());
        eprintln!(
            "│ {:18} │ {:12} │ {:>6} │",
            &node_info.name, &node_info.node_type, port_str
        );
    }

    eprintln!(
        "└{}┴{}┴{}┘",
        "─".repeat(20),
        "─".repeat(14),
        "─".repeat(8)
    );
    eprintln!();
    let _ = std::io::stderr().flush();
}

/// Print the final summary table with per-node statistics.
fn print_summary_table(event_loop: &EventLoop) {
    eprintln!();
    eprintln!(
        "┌{}┬{}┬{}┬{}┬{}┬{}┐",
        "─".repeat(18),
        "─".repeat(12),
        "─".repeat(8),
        "─".repeat(10),
        "─".repeat(10),
        "─".repeat(12)
    );
    eprintln!(
        "│ {:^16} │ {:^10} │ {:^6} │ {:^8} │ {:^8} │ {:^10} │",
        "Node", "Type", "Port", "TX", "RX", "Collisions"
    );
    eprintln!(
        "├{}┼{}┼{}┼{}┼{}┼{}┤",
        "─".repeat(18),
        "─".repeat(12),
        "─".repeat(8),
        "─".repeat(10),
        "─".repeat(10),
        "─".repeat(12)
    );

    for node_info in event_loop.node_infos() {
        let stats = event_loop
            .node_stats()
            .get(&node_info.radio_entity_id)
            .cloned()
            .unwrap_or_default();
        let port_str = get_port_str(node_info, event_loop.uart_manager());

        eprintln!(
            "│ {:16} │ {:10} │ {:>6} │ {:>8} │ {:>8} │ {:>10} │",
            &node_info.name, &node_info.node_type, port_str, stats.tx, stats.rx, stats.collisions
        );
    }

    eprintln!(
        "└{}┴{}┴{}┴{}┴{}┴{}┘",
        "─".repeat(18),
        "─".repeat(12),
        "─".repeat(8),
        "─".repeat(10),
        "─".repeat(10),
        "─".repeat(12)
    );
    let _ = std::io::stderr().flush();
}

// ============================================================================
// Main Entry Point
// ============================================================================

/// Run a simulation with the given configuration.
pub fn run_simulation(config: RunnerConfig) -> Result<SimulationStats, RunnerError> {
    // Parse metric specs early - these are used for both label filtering and export
    let metric_specs: Vec<metric_spec::MetricSpec> = if config.metric_specs.is_empty() {
        // Default: show all metrics with per-node breakdown
        vec![
            metric_spec::MetricSpec::parse("mcsim.radio.*/node").unwrap(),
            metric_spec::MetricSpec::parse("mcsim.dm.*").unwrap(),
            metric_spec::MetricSpec::parse("mcsim.flood.*").unwrap(),
            metric_spec::MetricSpec::parse("mcsim.path.*").unwrap(),
            metric_spec::MetricSpec::parse("mcsim.timing.*").unwrap(),
        ]
    } else {
        config
            .metric_specs
            .iter()
            .filter_map(|s| metric_spec::MetricSpec::parse(s).ok())
            .collect()
    };

    // Install metrics recorder if metrics export is requested or rerun is enabled
    let metrics_recorder = if config.metrics_output.is_some() || config.rerun {
        let recorder = Arc::new(metrics_export::InMemoryRecorder::new());
        // Set label filter based on metric specs - this ensures we only track
        // the labels that are requested in the specs, preventing unbounded memory growth
        recorder.set_label_filter(metric_specs.clone());
        if let Err(e) = metrics::set_global_recorder(recorder.clone()) {
            eprintln!("Warning: Failed to set metrics recorder: {}", e);
            None
        } else {
            // Describe all metrics from mcsim-metrics
            mcsim_metrics::describe_metrics();
            Some(recorder)
        }
    } else {
        None
    };

    // Create tokio runtime for async TCP handling
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");

    // Load and merge model(s)
    let model = if config.models.len() == 1 {
        load_model(&config.models[0])?
    } else {
        let paths: Vec<&Path> = config.models.iter().map(|p| p.as_path()).collect();
        mcsim_model::load_models(&paths)?
    };

    if config.verbose {
        eprintln!("Loaded model with {} nodes from {} file(s)", model.nodes().len(), config.models.len());
    }

    // Generate seed if not provided
    let seed = config.seed.unwrap_or_else(|| {
        use rand::Rng;
        rand::thread_rng().gen()
    });

    if config.verbose {
        eprintln!("Using seed: {}", seed);
    }

    // Build simulation
    let simulation = build_simulation(&model, seed)?;

    if config.verbose {
        eprintln!(
            "Built simulation with {} entities",
            simulation.entities.len()
        );
    }

    // Set up UART TCP servers for each node
    let mut uart_manager = SyncUartManager::new(config.uart_base_port, runtime.handle().clone());
    
    // First, reserve all explicitly assigned ports to prevent conflicts
    for node_info in &simulation.node_infos {
        if let Some(port) = node_info.uart_port {
            uart_manager.reserve_port(port);
        }
    }
    
    // Then register all nodes - explicit ports will be used, others assigned sequentially
    for node_info in &simulation.node_infos {
        uart_manager.register_node(
            node_info.firmware_entity_id,
            node_info.name.clone(),
            node_info.node_type.clone(),
            &node_info.public_key,
            node_info.uart_port,
        );
    }
    
    // Start the UART TCP listeners
    uart_manager.start()?;

    // Set up trace output
    let trace_output: Option<Box<dyn Write>> = if let Some(ref path) = config.output {
        Some(Box::new(std::fs::File::create(path)?))
    } else {
        None
    };

    // Set up entity tracer if requested
    let entity_tracer = if let Some(ref trace_spec) = config.trace {
        let tracer_config = EntityTracerConfig::from_spec(trace_spec);
        if config.verbose && tracer_config.is_enabled() {
            if tracer_config.traces_all() {
                eprintln!("Entity tracing enabled for all entities");
            } else {
                eprintln!("Entity tracing enabled for: {}", trace_spec);
            }
        }
        EntityTracer::new(tracer_config)
    } else {
        EntityTracer::disabled()
    };

    // Set up rerun visualization if enabled
    let rerun_logger = if config.rerun {
        if config.verbose {
            eprintln!("Initializing rerun.io visualization...");
        }
        
        // Build node info for visualization
        let vis_nodes: Vec<VisNodeInfo> = simulation.node_infos.iter().map(|n| {
            VisNodeInfo {
                name: n.name.clone(),
                node_type: n.node_type.clone(),
                firmware_entity_id: n.firmware_entity_id,
                radio_entity_id: n.radio_entity_id,
                location: n.location.clone(),
            }
        }).collect();
        
        // Build a name->location lookup for creating link info
        let node_locations: std::collections::HashMap<String, mcsim_common::GeoCoord> = 
            simulation.node_infos.iter()
                .map(|n| (n.name.clone(), n.location.clone()))
                .collect();
        
        // Build link info from model edges
        let vis_links: Vec<VisLinkInfo> = model.edges().iter().filter_map(|(_, edge)| {
            let from_loc = node_locations.get(&edge.from)?;
            let to_loc = node_locations.get(&edge.to)?;
            // Get mean_snr_db_at20dbm from edge properties
            let mean_snr: f64 = edge.properties()
                .get(&mcsim_model::LINK_MEAN_SNR_DB_AT20DBM);
            Some(VisLinkInfo {
                from: edge.from.clone(),
                to: edge.to.clone(),
                from_location: from_loc.clone(),
                to_location: to_loc.clone(),
                mean_snr_db_at20dbm: mean_snr,
            })
        }).collect();
        
        match RerunLogger::new("MCSim", vis_nodes, vis_links) {
            Ok(logger) => {
                eprintln!("✓ Rerun viewer spawned");
                
                // Set up metrics visualization blueprint
                #[cfg(feature = "rerun")]
                {
                    if let Err(e) = rerun_blueprint::initialize_metrics_visualization(logger.recording_stream()) {
                        eprintln!("⚠ Failed to set up metrics blueprint: {}", e);
                    } else if config.verbose {
                        eprintln!("✓ Metrics visualization blueprint configured");
                    }
                }
                
                Some(logger)
            }
            Err(e) => {
                eprintln!("⚠ Failed to initialize rerun: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Create event loop with UART manager and optional rerun logger
    let mut event_loop = EventLoop::new(
        simulation,
        seed,
        trace_output,
        Some(uart_manager),
        rerun_logger,
        entity_tracer,
    );

    // Configure packet tracker eviction from model properties
    let eviction_age: Option<f64> = model.simulation_properties().get(&mcsim_model::PACKET_TRACKER_EVICTION_AGE_S);
    if eviction_age.is_some() {
        event_loop.set_packet_eviction_age(eviction_age);
        if config.verbose {
            eprintln!("Packet tracker eviction age: {:.0}s", eviction_age.unwrap());
        }
    }

    // Configure metrics for Rerun visualization if both rerun and metrics are enabled
    if config.rerun {
        if let Some(ref recorder) = metrics_recorder {
            // Use the already-parsed metric specs for rerun visualization
            event_loop.set_metrics_for_rerun(recorder.clone(), metric_specs.clone());
            if config.verbose {
                eprintln!("✓ Metrics visualization enabled for Rerun");
            }
        }
    }

    // Determine metrics warmup time (CLI overrides model property)
    let warmup_secs: f64 = config.metrics_warmup.unwrap_or_else(|| {
        model.simulation_properties().get(&mcsim_model::METRICS_WARMUP_S)
    });
    let warmup_time = if warmup_secs > 0.0 {
        if config.verbose && config.metrics_output.is_some() {
            eprintln!("Metrics warmup period: {:.1}s (metrics will be cleared after warmup)", warmup_secs);
        }
        Some(SimTime::from_secs(warmup_secs))
    } else {
        None
    };

    // Track whether warmup has completed (for clearing metrics)
    let warmup_cleared = std::cell::Cell::new(false);

    // Run in either timed or realtime mode
    let stats = if let Some(duration_secs) = config.duration {
        // Timed mode: run for specified duration
        let duration = SimTime::from_secs(duration_secs);

        if config.verbose {
            eprintln!("Running simulation for {} seconds...", duration_secs);
        }

        eprintln!("⏱  Running timed simulation for {} seconds...", duration_secs);
        
        // Create watchdog for monitoring slow events
        let watchdog = Watchdog::new(std::time::Duration::from_secs(config.watchdog_timeout));
        watchdog.state().set_seed(seed);
        
        // Set up Ctrl+C handler for graceful shutdown
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();
        ctrlc::set_handler(move || {
            stop_flag_clone.store(true, Ordering::Relaxed);
        })
        .expect("Error setting Ctrl+C handler");
        
        // Progress callback for timed mode
        let print_progress = |_event_loop: &EventLoop, progress: ProgressInfo, is_final: bool| {
            // Check if warmup period has completed and clear metrics
            if let Some(warmup) = warmup_time {
                if !warmup_cleared.get() && progress.sim_time >= warmup {
                    if let Some(ref recorder) = metrics_recorder {
                        recorder.clear();
                        if config.verbose {
                            eprintln!("  [WARMUP] Cleared metrics at {:.1}s (warmup period: {:.1}s)", 
                                progress.sim_time.as_secs_f64(), warmup.as_secs_f64());
                        }
                    }
                    warmup_cleared.set(true);
                }
            }

            if is_final {
                // Final summary is handled by print_summary_table
                return;
            }
            
            let sim_secs = progress.sim_time.as_secs_f64();
            let eta_secs = progress.estimated_remaining.as_secs_f64();
            
            // Format ETA nicely
            let eta_str = if eta_secs > 60.0 {
                format!("{}m {:.0}s", (eta_secs / 60.0) as u64, eta_secs % 60.0)
            } else {
                format!("{:.1}s", eta_secs)
            };
            
            eprintln!(
                "  [{:5.1}%] sim: {:.1}s | events: {} | speed: {:.1}x | ETA: {}",
                progress.progress_percent,
                sim_secs,
                progress.events_processed,
                progress.time_multiplier,
                eta_str
            );
        };

        let result = event_loop.run_with_watchdog(
            duration, 
            Some(stop_flag),
            Some(&watchdog),
            config.break_at_event,
            print_progress
        )?;
        
        // Stop the watchdog thread
        watchdog.stop();
        
        // Print summary table for timed mode
        print_summary_table(&event_loop);
        
        result
    } else {
        // Realtime mode: run until Ctrl+C
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        // Set up Ctrl+C handler
        ctrlc::set_handler(move || {
            stop_flag_clone.store(true, Ordering::Relaxed);
        })
        .expect("Error setting Ctrl+C handler");

        // Configure real-time mode with speed multiplier and catch-up settings
        let periodic_stats_interval: Option<u64> = model.simulation_properties()
            .get(&mcsim_model::RUNNER_PERIODIC_STATS_INTERVAL_S);
        let realtime_config = RealTimeConfig::with_speed(config.speed)
            .with_max_catchup_ms(config.max_catchup_ms)
            .with_periodic_stats_interval(periodic_stats_interval);
        event_loop.set_realtime_config(realtime_config);

        if config.verbose {
            eprintln!("Starting realtime simulation at {}x speed...", config.speed);
        }

        let speed_str = if (config.speed - 1.0).abs() < 0.001 {
            String::from("real-time")
        } else {
            format!("{}x speed", config.speed)
        };
        eprintln!("\n🚀 Running in {} mode. Press Ctrl+C to stop.", speed_str);
        
        // Print the initial entity table once, then let logs scroll
        print_initial_table(&event_loop);

        // Run with periodic callback to check warmup
        let result = event_loop.run_realtime(stop_flag, |event_loop, _elapsed| {
            // Check if warmup period has completed and clear metrics
            if let Some(warmup) = warmup_time {
                if !warmup_cleared.get() && event_loop.current_time() >= warmup {
                    if let Some(ref recorder) = metrics_recorder {
                        recorder.clear();
                        if config.verbose {
                            eprintln!("  [WARMUP] Cleared metrics at {:.1}s (warmup period: {:.1}s)", 
                                event_loop.current_time().as_secs_f64(), warmup.as_secs_f64());
                        }
                    }
                    warmup_cleared.set(true);
                }
            }
        })?;

        eprintln!("\n⏹  Simulation stopped.");
        result
    };

    if config.verbose {
        eprintln!("Simulation complete!");
        eprintln!("  Total events: {}", stats.total_events);
        eprintln!("  Packets TX: {}", stats.packets_transmitted);
        eprintln!("  Packets RX: {}", stats.packets_received);
        eprintln!("  Collisions: {}", stats.packets_collided);
        eprintln!("  Messages sent: {}", stats.messages_sent);
        eprintln!("  Messages acked: {}", stats.messages_acked);
        eprintln!("  Wall time: {}ms", stats.wall_time_ms);
    }

    // Export metrics if requested
    if let Some(format) = config.metrics_output {
        if let Some(recorder) = metrics_recorder {
            let mut writer: Box<dyn Write> = match &config.metrics_file {
                Some(path) => Box::new(std::fs::File::create(path)?),
                None => Box::new(std::io::stdout()),
            };

            // Parse metric specs if provided
            let specs: Vec<metric_spec::MetricSpec> = config
                .metric_specs
                .iter()
                .map(|s| {
                    metric_spec::MetricSpec::parse(s)
                        .map_err(|e| RunnerError::ConfigError(format!("Invalid metric spec '{}': {}", s, e)))
                })
                .collect::<Result<Vec<_>, _>>()?;

            match format {
                MetricsOutputFormat::Json => {
                    if specs.is_empty() {
                        // Use legacy format when no specs provided
                        let snapshot = recorder.snapshot();
                        metrics_export::export_json(&snapshot, &mut writer)?;
                    } else {
                        // Use new spec-based format
                        let export = recorder.snapshot_with_specs(&specs);
                        metrics_export::export_json_with_specs(&export, &mut writer)?;
                    }
                }
                MetricsOutputFormat::Prometheus => {
                    // Prometheus format uses legacy snapshot
                    let snapshot = recorder.snapshot();
                    metrics_export::export_prometheus(&snapshot, &mut writer)?;
                }
                MetricsOutputFormat::Csv => {
                    // CSV format with hierarchical paths
                    let export = recorder.snapshot_with_specs(&specs);
                    metrics_export::export_csv_with_specs(&export, &mut writer)?;
                }
            }

            if config.verbose {
                if let Some(ref path) = config.metrics_file {
                    eprintln!("Metrics exported to: {}", path.display());
                }
            }
        }
    }

    Ok(stats)
}

// ============================================================================
// Link Prediction
// ============================================================================

/// Print a link prediction result in human-readable format.
fn print_link_prediction(pred: &mcsim_link::LinkPrediction) {
    println!("Link Prediction Results");
    println!("=======================");
    println!();
    println!("Path:");
    println!(
        "  From: ({:.6}, {:.6}) at {:.1}m AGL",
        pred.path.from_lat, pred.path.from_lon, pred.path.from_height
    );
    println!(
        "  To:   ({:.6}, {:.6}) at {:.1}m AGL",
        pred.path.to_lat, pred.path.to_lon, pred.path.to_height
    );
    println!("  Distance: {:.2} km", pred.path.distance_km);
    println!();
    println!("Terrain:");
    println!("  Samples: {}", pred.terrain.sample_count);
    println!("  Resolution: {:.1}m", pred.terrain.resolution_m);
    println!(
        "  Elevation: min={:.1}m, max={:.1}m, mean={:.1}m",
        pred.terrain.min_elevation, pred.terrain.max_elevation, pred.terrain.mean_elevation
    );
    println!("  Delta H (terrain irregularity): {:.1}m", pred.terrain.delta_h);
    println!();
    println!("Radio:");
    println!("  Frequency: {:.1} MHz", pred.radio.freq_mhz);
    println!("  TX Power: {} dBm", pred.radio.tx_power_dbm);
    println!("  Noise Floor: {:.1} dBm (assumed)", pred.radio.noise_floor_dbm);
    println!();
    println!("Path Loss ({}):", pred.prediction_method);
    println!("  Median:           {:.1} dB", pred.path_loss_db);
    if pred.itm_warnings != 0 {
        println!("  Warnings:         0x{:04x}", pred.itm_warnings);
    }
    println!();
    println!("Predicted SNR:");
    println!("  Mean:             {:.1} dB", pred.snr_db);
    println!(
        "  Std Dev:          {:.1} dB (estimated from terrain)",
        pred.snr_std_dev_db
    );
    println!();
    println!(
        "Link Assessment (SF{}, {:.1} dB threshold):",
        pred.radio.spreading_factor, pred.radio.snr_threshold_db
    );
    println!("  Link Margin:      {:.1} dB (from median)", pred.link_margin_db);
    println!("  Status:           {}", pred.status);
}

/// Predict link quality between two geographic coordinates using DEM and ITM.
fn predict_link(config: PredictLinkConfig) -> Result<(), RunnerError> {
    use mcsim_link::{
        load_dem, load_itm, load_aws_elevation,
        predict_link as do_predict, predict_link_with_elevation,
        LinkPredictionConfig,
    };

    // Resolve the config (merge YAML files + CLI overrides)
    let config = config.resolve()?;

    // Load ITM library
    eprintln!("Loading ITM library...");
    let itm = load_itm().map_err(|e| {
        RunnerError::ConfigError(format!("{}", e))
    })?;

    // Build prediction config
    let pred_config = LinkPredictionConfig {
        from_lat: config.from_lat,
        from_lon: config.from_lon,
        to_lat: config.to_lat,
        to_lon: config.to_lon,
        from_height: config.from_height,
        to_height: config.to_height,
        freq_mhz: config.freq,
        tx_power_dbm: config.tx_power,
        spreading_factor: config.sf,
        terrain_samples: config.samples,
    };

    eprintln!(
        "Sampling terrain from ({:.6}, {:.6}) to ({:.6}, {:.6})...",
        config.from_lat, config.from_lon, config.to_lat, config.to_lon
    );

    // Perform prediction using appropriate elevation source
    let prediction = match config.elevation_source.as_str() {
        "aws" => {
            // Use AWS terrain tiles (fetched on demand and cached)
            eprintln!(
                "Using AWS terrain tiles (cache: {}, zoom: {})...",
                config.elevation_cache.display(),
                config.zoom
            );
            let elevation = load_aws_elevation(&config.elevation_cache, config.zoom).map_err(|e| {
                RunnerError::ConfigError(format!("{}", e))
            })?;
            eprintln!();
            predict_link_with_elevation(&elevation, &itm, &pred_config).map_err(|e| {
                RunnerError::ConfigError(format!("{}", e))
            })?
        }
        "local_dem" => {
            // Use local DEM files
            eprintln!("Loading DEM data from {}...", config.dem_dir.display());
            let dem = load_dem(&config.dem_dir).map_err(|e| {
                RunnerError::ConfigError(format!("{}", e))
            })?;
            eprintln!("DEM loaded");
            eprintln!();
            do_predict(&dem, &itm, &pred_config).map_err(|e| {
                RunnerError::ConfigError(format!("{}", e))
            })?
        }
        other => {
            return Err(RunnerError::ConfigError(format!(
                "Unknown elevation source '{}'. Use 'aws' or 'local_dem'.",
                other
            )));
        }
    };

    // Print results
    print_link_prediction(&prediction);

    Ok(())
}

/// Estimate true SNR distribution from observed measurements.
fn estimate_snr_command(config: EstimateSnrConfig) -> Result<(), RunnerError> {
    use mcsim_link::{estimate_snr, estimate_snr_with_threshold, LoraModulationParams};

    if config.observations.is_empty() {
        return Err(RunnerError::ConfigError(
            "No SNR observations provided".to_string(),
        ));
    }

    let result = if let Some(threshold) = config.threshold {
        // Use custom threshold
        estimate_snr_with_threshold(config.observations.clone(), threshold)
    } else {
        // Use threshold derived from modulation parameters
        let params = LoraModulationParams {
            spreading_factor: config.sf,
            bandwidth_hz: config.bandwidth,
            coding_rate: config.coding_rate,
        };
        estimate_snr(config.observations.clone(), params)
    }
    .map_err(|e| RunnerError::ConfigError(format!("Estimation failed: {}", e)))?;

    if config.format == "json" {
        // JSON output
        let json = serde_json::json!({
            "mean_snr_db": result.mean_snr,
            "std_dev_db": result.std_dev,
            "threshold_db": result.threshold,
            "observation_count": result.observation_count,
            "sample_mean_db": result.sample_mean,
            "reception_probability": result.reception_probability(),
            "link_margin_db": result.link_margin(),
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        // Text output
        println!("SNR Estimation Results");
        println!("======================");
        println!();
        println!("Input:");
        println!("  Observations:     {} values", result.observation_count);
        println!("  Sample Mean:      {:.2} dB", result.sample_mean);
        println!("  Threshold:        {:.1} dB (SF{})", result.threshold, config.sf);
        println!();
        println!("Estimated Distribution:");
        println!("  Mean SNR:         {:.2} dB", result.mean_snr);
        println!("  Std Deviation:    {:.2} dB", result.std_dev);
        println!();
        println!("Link Quality:");
        println!("  Link Margin:      {:.2} dB", result.link_margin());
        println!(
            "  Reception Prob:   {:.1}%",
            result.reception_probability() * 100.0
        );

        // Interpretation
        let margin = result.link_margin();
        let status = if margin > 10.0 {
            "Excellent"
        } else if margin > 5.0 {
            "Good"
        } else if margin > 0.0 {
            "Marginal"
        } else {
            "UNRELIABLE"
        };
        println!("  Status:           {}", status);
    }

    Ok(())
}

/// Build a simulation model from mesh node JSON data.
fn build_model_command(config: BuildModelConfig) -> Result<(), RunnerError> {
    let model_config = build_model::BuildModelConfig {
        input_path: config.input,
        output_path: config.output,
        elevation_source: config.elevation_source,
        elevation_cache: config.elevation_cache,
        zoom: config.zoom,
        dem_dir: config.dem_dir,
        spreading_factor: config.sf,
        tx_power_dbm: config.tx_power,
        min_snr_threshold: config.min_snr,
        max_nodes: config.max_nodes,
        include_all_types: config.all_types,
        antenna_height: config.antenna_height,
        terrain_samples: config.terrain_samples,
        public_key_prefix_len: config.pubkey_prefix_len,
        verbose: config.verbose,
    };

    build_model::build_model(&model_config).map_err(|e| {
        RunnerError::ConfigError(format!("Build model failed: {}", e))
    })
}

/// Generate Ed25519 keypairs with optional prefix matching.
fn keygen_command(config: KeygenConfig) -> Result<(), RunnerError> {
    use mcsim_model::{KeySpec, KeyConfig, generate_keypair};
    use std::time::Instant;

    // Parse the public key spec
    let pubkey_spec = KeySpec::parse(&config.pubkey).map_err(|e| {
        RunnerError::ConfigError(format!("Invalid pubkey spec: {}", e))
    })?;

    let key_config = KeyConfig {
        private_key: KeySpec::default(), // Random
        public_key: pubkey_spec.clone(),
    };

    // Get the seed (either provided or random)
    let seed = config.seed.unwrap_or_else(|| rand::random());

    // Time the key generation
    let start = Instant::now();
    let result = generate_keypair(seed, &key_config, "keygen", config.max_attempts);
    let elapsed = start.elapsed();

    match result {
        Ok(keygen_result) => {
            let private_hex = hex::encode(&keygen_result.keypair.private_key);
            let public_hex = hex::encode(&keygen_result.keypair.public_key);
            let iterations = keygen_result.iterations;
            let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
            let us_per_iter = if iterations > 0 {
                (elapsed.as_micros() as f64) / (iterations as f64)
            } else {
                0.0
            };

            if config.format == "json" {
                let json = serde_json::json!({
                    "private_key": private_hex,
                    "public_key": public_hex,
                    "spec": config.pubkey,
                    "seed": seed,
                    "max_attempts": config.max_attempts,
                    "iterations": iterations,
                    "elapsed_ms": elapsed_ms,
                    "us_per_iteration": us_per_iter,
                });
                println!("{}", serde_json::to_string_pretty(&json)?);
            } else {
                println!("Ed25519 Keypair Generation");
                println!("==========================");
                println!();
                println!("Spec:        {}", config.pubkey);
                println!("Seed:        {}", seed);
                println!("Threads:     {}", rayon::current_num_threads());
                if let Some(max) = config.max_attempts {
                    println!("Max Attempts: {}", max);
                }
                println!();
                println!("Private Key: {}", private_hex);
                println!("Public Key:  {}", public_hex);
                println!();
                println!("Iterations:  {}", iterations);
                println!("Time:        {:.3} ms", elapsed_ms);
                if iterations > 1 {
                    println!("Per Iter:    {:.2} µs", us_per_iter);
                }
            }
        }
        Err(e) => {
            let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
            if config.format == "json" {
                let json: serde_json::Value = serde_json::json!({
                    "error": e.to_string(),
                    "spec": config.pubkey,
                    "seed": seed,
                    "max_attempts": config.max_attempts,
                    "elapsed_ms": elapsed_ms,
                });
                println!("{}", serde_json::to_string_pretty(&json)?);
            } else {
                eprintln!("Error: {}", e);
                eprintln!("Time: {:.3} ms", elapsed_ms);
            }
            return Err(RunnerError::ConfigError(e.to_string()));
        }
    }

    Ok(())
}

fn main() -> Result<(), RunnerError> {
    // Initialize tracing subscriber with RUST_LOG env filter
    // Default to "warn" level if RUST_LOG is not set
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn"));
    
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(filter)
        .init();

    let cli = Cli::parse();
    
    match cli.command {
        Commands::Run(config) => {
            let metrics_output = config.metrics_output;
            let stats = run_simulation(config)?;

            // Output stats as JSON to stdout only if not exporting metrics to stdout
            if metrics_output.is_none() {
                println!("{}", serde_json::to_string_pretty(&stats)?);
            }
        }
        Commands::Metrics => {
            print_metrics_info();
        }
        Commands::Properties => {
            print_properties_info();
        }
        Commands::PredictLink(config) => {
            predict_link(config)?;
        }
        Commands::EstimateSnr(config) => {
            estimate_snr_command(config)?;
        }
        Commands::BuildModel(config) => {
            build_model_command(config)?;
        }
        Commands::Keygen(config) => {
            keygen_command(config)?;
        }
    }

    Ok(())
}

/// Print information about all available metrics
fn print_metrics_info() {
    use mcsim_metrics::metric_defs;
    
    println!("MCSim Available Metrics");
    println!("=======================\n");
    
    println!("All metrics support the following labels:");
    println!("  - node: Individual node identifier");
    println!("  - node_type: Type of node (repeater, room_server, companion, sensor)");
    println!("  - groups: Custom grouping tags (comma-separated)");
    println!();
    
    // Group metrics by category based on prefix
    let categories = [
        ("Radio/PHY Layer", "mcsim.radio."),
        ("Packet/Network Layer", "mcsim.packet."),
        ("Direct Message Layer", "mcsim.dm."),
        ("Flood Propagation", "mcsim.flood."),
        ("Path Message Delivery", "mcsim.path."),
        ("Timing", "mcsim.timing."),
    ];
    
    for (category_name, prefix) in categories {
        println!("## {}\n", category_name);
        
        for metric in metric_defs::ALL {
            if metric.name.starts_with(prefix) {
                println!("  {}", metric.name);
                println!("    Type: {}", metric.kind);
                let unit_str = metric.unit_str();
                if !unit_str.is_empty() {
                    println!("    Unit: {}", unit_str);
                }
                if !metric.description.is_empty() {
                    println!("    Description: {}", metric.description);
                }
                if !metric.labels.is_empty() {
                    println!("    Extra labels: {}", metric.labels.join(", "));
                }
                println!();
            }
        }
    }
    
    println!("## Usage Examples\n");
    println!("  # Run simulation and export all radio metrics by node:");
    println!("  mcsim run model.yaml --duration 1m --metrics-output json --metric \"mcsim.radio.*/node\"\n");
    println!("  # Export message metrics with multiple breakdowns:");
    println!("  mcsim run model.yaml --duration 1h30m --metrics-output json --metric \"mcsim.dm.*/node/node_type\"\n");
    println!("  # Export specific metric totals only:");
    println!("  mcsim run model.yaml --duration 60s --metrics-output json --metric \"mcsim.radio.tx_packets\"\n");
}

/// Print information about all available properties
fn print_properties_info() {
    use mcsim_model::{properties_by_scope, PropertyScope};
    
    println!("MCSim Available Properties");
    println!("==========================\n");
    
    println!("Properties configure simulation behavior and can be set in YAML files.");
    println!("Properties are organized by scope (what they apply to) and namespace.\n");
    
    println!("## Property Resolution Order\n");
    println!("  1. Built-in code defaults (shown below)");
    println!("  2. Defaults from YAML files (in order loaded)");
    println!("  3. Explicit values on nodes/edges (in order loaded)\n");
    println!("When loading multiple YAML files, later files override earlier ones.\n");
    
    // Print by scope
    let scopes = [
        (PropertyScope::Node, "Node Properties", "Apply to individual nodes in the network"),
        (PropertyScope::Edge, "Edge Properties", "Apply to links between nodes"),
        (PropertyScope::Simulation, "Simulation Properties", "Apply to the entire simulation"),
    ];
    
    for (scope, scope_name, scope_desc) in scopes {
        println!("## {}\n", scope_name);
        println!("{}\n", scope_desc);
        
        // Get all namespaces for this scope
        let mut props_by_ns: std::collections::BTreeMap<String, Vec<_>> = std::collections::BTreeMap::new();
        for prop in properties_by_scope(scope) {
            let ns = prop.namespace().unwrap_or("(root)").to_string();
            props_by_ns.entry(ns).or_default().push(prop);
        }
        
        for (namespace, props) in props_by_ns {
            if namespace != "(root)" {
                println!("### {}/\n", namespace);
            }
            
            for prop in props {
                println!("  {}", prop.name);
                println!("    {}", prop.description);
                print!("    Default: {}", prop.default);
                if let Some(unit) = &prop.unit {
                    print!(" {}", unit);
                }
                println!();
                if !prop.aliases.is_empty() {
                    println!("    Aliases: {}", prop.aliases.join(", "));
                }
                println!();
            }
        }
    }
    
    println!("## YAML Examples\n");
    println!("### Setting defaults for all nodes:\n");
    println!("```yaml");
    println!("defaults:");
    println!("  radio:");
    println!("    frequency_hz: 910525000");
    println!("    bandwidth_hz: 62500");
    println!("    spreading_factor: 7");
    println!("  repeater:");
    println!("    tx_delay_min_ms: 100");
    println!("    tx_delay_max_ms: 500");
    println!("```\n");
    
    println!("### Overriding properties for a specific node:\n");
    println!("```yaml");
    println!("nodes:");
    println!("  - name: \"HighPowerRepeater\"");
    println!("    location:");
    println!("      lat: 47.6062");
    println!("      lon: -122.3321");
    println!("    radio_override:");
    println!("      tx_power_dbm: 30");
    println!("    firmware:");
    println!("      type: Repeater");
    println!("```\n");
    
    println!("### Defining edge properties:\n");
    println!("```yaml");
    println!("edges:");
    println!("  - from: \"Node1\"");
    println!("    to: \"Node2\"");
    println!("    mean_snr_db_at20dbm: 15.0");
    println!("    snr_std_dev: 2.0");
    println!("```\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runner_config_parse() {
        // Test that default values work
        let config = RunnerConfig {
            models: vec![PathBuf::from("test.yaml")],
            duration: Some(3600.0),
            seed: Some(12345),
            output: None,
            uart_base_port: 9000,
            rerun: false,
            verbose: false,
            trace: None,
            metrics_output: None,
            metrics_file: None,
            metric_specs: vec![],
            speed: 1.0,
            max_catchup_ms: 100,
            break_at_event: None,
            watchdog_timeout: DEFAULT_WATCHDOG_TIMEOUT_S,
            metrics_warmup: None,
        };
        assert_eq!(config.duration, Some(3600.0));
    }

    #[test]
    fn test_runner_config_realtime_mode() {
        // Test realtime mode (no duration)
        let config = RunnerConfig {
            models: vec![PathBuf::from("test.yaml")],
            duration: None,
            seed: Some(12345),
            output: None,
            uart_base_port: 9000,
            rerun: false,
            verbose: false,
            trace: None,
            metrics_output: None,
            metrics_file: None,
            metric_specs: vec![],
            speed: 1.0,
            max_catchup_ms: 100,
            break_at_event: None,
            watchdog_timeout: DEFAULT_WATCHDOG_TIMEOUT_S,
            metrics_warmup: None,
        };
        assert!(config.duration.is_none());
    }

    #[test]
    fn test_runner_config_speed_multiplier() {
        // Test speed multiplier option
        let config = RunnerConfig {
            models: vec![PathBuf::from("test.yaml")],
            duration: None,
            seed: Some(12345),
            output: None,
            uart_base_port: 9000,
            rerun: false,
            verbose: false,
            trace: None,
            metrics_output: None,
            metrics_file: None,
            metric_specs: vec![],
            speed: 2.0,
            max_catchup_ms: 200,
            break_at_event: None,
            watchdog_timeout: DEFAULT_WATCHDOG_TIMEOUT_S,
            metrics_warmup: None,
        };
        assert_eq!(config.speed, 2.0);
        assert_eq!(config.max_catchup_ms, 200);
    }

    #[test]
    fn test_metrics_output_format() {
        // Test metrics output format options
        let config = RunnerConfig {
            models: vec![PathBuf::from("test.yaml")],
            duration: Some(60.0),
            seed: Some(12345),
            output: None,
            uart_base_port: 9000,
            rerun: false,
            verbose: false,
            trace: None,
            metrics_output: Some(MetricsOutputFormat::Json),
            metrics_file: Some(PathBuf::from("metrics.json")),
            metric_specs: vec!["mcsim.radio.*/node".to_string()],
            speed: 1.0,
            max_catchup_ms: 100,
            break_at_event: None,
            watchdog_timeout: DEFAULT_WATCHDOG_TIMEOUT_S,
            metrics_warmup: None,
        };
        assert!(config.metrics_output.is_some());
        assert!(config.metrics_file.is_some());
    }

    #[test]
    fn test_runner_config_multiple_models() {
        // Test multiple model files
        let config = RunnerConfig {
            models: vec![
                PathBuf::from("base.yaml"),
                PathBuf::from("overlay.yaml"),
            ],
            duration: Some(60.0),
            seed: Some(12345),
            output: None,
            uart_base_port: 9000,
            rerun: false,
            verbose: false,
            trace: None,
            metrics_output: None,
            metrics_file: None,
            metric_specs: vec![],
            speed: 1.0,
            max_catchup_ms: 100,
            break_at_event: None,
            watchdog_timeout: DEFAULT_WATCHDOG_TIMEOUT_S,
            metrics_warmup: None,
        };
        assert_eq!(config.models.len(), 2);
    }

    #[test]
    fn test_parse_duration_plain_seconds() {
        assert_eq!(parse_duration("60").unwrap(), 60.0);
        assert_eq!(parse_duration("3600").unwrap(), 3600.0);
        assert_eq!(parse_duration("0.5").unwrap(), 0.5);
    }

    #[test]
    fn test_parse_duration_with_units() {
        assert_eq!(parse_duration("60s").unwrap(), 60.0);
        assert_eq!(parse_duration("10m").unwrap(), 600.0);
        assert_eq!(parse_duration("2h").unwrap(), 7200.0);
        assert_eq!(parse_duration("1d").unwrap(), 86400.0);
    }

    #[test]
    fn test_parse_duration_combined() {
        assert_eq!(parse_duration("1h30m").unwrap(), 5400.0);
        assert_eq!(parse_duration("2d12h").unwrap(), 216000.0);
        assert_eq!(parse_duration("1d2h30m45s").unwrap(), 95445.0);
        assert_eq!(parse_duration("24h10m20s").unwrap(), 87020.0);
        assert_eq!(parse_duration("3d1h").unwrap(), 262800.0);
    }

    #[test]
    fn test_parse_duration_fractional() {
        assert_eq!(parse_duration("1.5h").unwrap(), 5400.0);
        assert_eq!(parse_duration("0.5d").unwrap(), 43200.0);
    }

    #[test]
    fn test_parse_duration_errors() {
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("10x").is_err());
        assert!(parse_duration("h10").is_err());
    }
}
