//! Build simulation model from mesh node JSON data.
//!
//! This module provides functionality to create a simulation YAML file from
//! observed mesh network data (like sea_nodes_full.json), using link prediction
//! and SNR estimation to generate realistic link characteristics.

use mcsim_link::{
    estimate_snr_with_threshold, load_dem, load_itm,
    load_aws_elevation, predict_link_with_elevation,
    ElevationSource, LinkPredictionConfig, LoraModulationParams, PredictionMethod,
};
use mcsim_itm::Itm;
use rayon::prelude::*;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use thiserror::Error;

// Thread-local ITM instance for parallel link evaluation.
// The ITM library functions are thread-safe (no global state in the C++ code),
// but we use thread-local instances to avoid any issues with concurrent
// libloading::Library::get() calls on a shared Library instance.
thread_local! {
    static THREAD_ITM: RefCell<Option<Itm>> = const { RefCell::new(None) };
}

/// Initialize the thread-local ITM instance if not already done.
fn with_thread_itm<F, R>(f: F) -> Result<R, BuildModelError>
where
    F: FnOnce(&Itm) -> Result<R, BuildModelError>,
{
    THREAD_ITM.with(|cell| {
        let mut borrow = cell.borrow_mut();
        if borrow.is_none() {
            *borrow = Some(load_itm().map_err(|e| BuildModelError::ItmError(e.to_string()))?);
        }
        f(borrow.as_ref().unwrap())
    })
}

/// Errors that can occur during model building.
#[derive(Debug, Error)]
pub enum BuildModelError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("DEM error: {0}")]
    DemError(String),

    #[error("ITM error: {0}")]
    ItmError(String),

    #[error("No nodes with location data found")]
    NoValidNodes,

    #[allow(dead_code)]
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

// ============================================================================
// JSON Schema Types (matching the mesh node API response)
// ============================================================================

/// Root structure of the mesh nodes JSON file.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct MeshNodesData {
    pub fetched_at: Option<String>,
    pub region: Option<String>,
    pub nodes: Vec<MeshNode>,
}

/// A mesh node from the JSON data.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct MeshNode {
    pub name: String,
    pub public_key: String,
    pub mode: String,
    pub location: Option<Location>,
    pub flags: Option<u32>,
    /// When this node was last seen on the network.
    pub last_seen: Option<String>,
    pub adverts_count: Option<u32>,
    pub recent_adverts: Option<Vec<Advert>>,
}

/// Location of a mesh node.
/// Supports both formats: {lat, lon} and {latitude, longitude}
/// Also handles null values.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Location {
    /// Short format: lat/lon (may have nulls)
    Short { lat: Option<f64>, lon: Option<f64> },
    /// Long format: latitude/longitude (may have nulls)
    Long { latitude: Option<f64>, longitude: Option<f64> },
}

impl Location {
    /// Get latitude if available
    pub fn lat(&self) -> Option<f64> {
        match self {
            Location::Short { lat, .. } => *lat,
            Location::Long { latitude, .. } => *latitude,
        }
    }

    /// Get longitude if available
    pub fn lon(&self) -> Option<f64> {
        match self {
            Location::Short { lon, .. } => *lon,
            Location::Long { longitude, .. } => *longitude,
        }
    }

    /// Check if location has valid coordinates
    pub fn is_valid(&self) -> bool {
        match (self.lat(), self.lon()) {
            (Some(lat), Some(lon)) => {
                // Reject null/zero coordinates (0,0 is in the ocean off Africa)
                !(lat.abs() < 0.0001 && lon.abs() < 0.0001)
            }
            _ => false,
        }
    }
}

/// An advertisement heard by a node.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Advert {
    pub origin: String,
    pub origin_id: String,
    pub path: Vec<String>,
    pub snr: i32,
    pub rssi: Option<i32>,
    pub decoded_payload: Option<DecodedPayload>,
}

/// Decoded payload of an advertisement.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct DecodedPayload {
    pub public_key: String,
    pub name: Option<String>,
}

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for building a model.
#[derive(Debug, Clone)]
pub struct BuildModelConfig {
    /// Path to the input JSON file.
    pub input_path: std::path::PathBuf,
    /// Path to the output YAML file (None for stdout).
    pub output_path: Option<std::path::PathBuf>,
    /// Elevation data source: "aws" or "local_dem".
    pub elevation_source: String,
    /// Cache directory for AWS terrain tiles.
    pub elevation_cache: std::path::PathBuf,
    /// Zoom level for AWS terrain tiles (1-14).
    pub zoom: u8,
    /// Path to DEM data directory (for local_dem source).
    pub dem_dir: std::path::PathBuf,
    /// Spreading factor (7-12).
    pub spreading_factor: u8,
    /// TX power in dBm.
    pub tx_power_dbm: i8,
    /// Minimum SNR threshold for including a link (based on SF).
    /// If None, uses the SF-derived threshold.
    pub min_snr_threshold: Option<f64>,
    /// Maximum number of nodes to include (None for all).
    pub max_nodes: Option<usize>,
    /// Whether to include all node types or just repeaters.
    pub include_all_types: bool,
    /// Antenna height in meters (above ground).
    pub antenna_height: f64,
    /// Number of terrain samples for link prediction.
    pub terrain_samples: usize,
    /// Number of hex characters to retain from public key prefix.
    pub public_key_prefix_len: usize,
    /// Verbose output.
    pub verbose: bool,
}

impl Default for BuildModelConfig {
    fn default() -> Self {
        Self {
            input_path: std::path::PathBuf::from("nodes.json"),
            output_path: None,
            elevation_source: "aws".to_string(),
            elevation_cache: std::path::PathBuf::from("./elevation_cache"),
            zoom: 12,
            dem_dir: std::path::PathBuf::from("./dem_data"),
            spreading_factor: 7,
            tx_power_dbm: 20,
            min_snr_threshold: None,
            max_nodes: None,
            include_all_types: false,
            antenna_height: 2.0,
            terrain_samples: 100,
            public_key_prefix_len: 2,
            verbose: false,
        }
    }
}

// ============================================================================
// Internal Types
// ============================================================================

/// Processed node ready for simulation.
#[derive(Debug, Clone)]
struct ProcessedNode {
    name: String,
    public_key: String,
    mode: String,
    lat: f64,
    lon: f64,
}

/// Zero-hop observation data.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ZeroHopData {
    /// Node that heard the advert (receiver).
    receiver_id: String,
    /// Node that sent the advert (transmitter).
    transmitter_id: String,
    /// SNR values observed.
    snr_values: Vec<f64>,
}

/// Link data to be written to YAML.
#[derive(Debug, Clone)]
struct LinkData {
    from: String,
    to: String,
    mean_snr_db: f64,
    snr_std_dev: f64,
    source: LinkSource,
    /// Predicted SNR (if different from estimated).
    predicted_snr_db: Option<f64>,
    /// Distance between nodes in kilometers (from prediction).
    distance_km: f64,
    /// Terrain irregularity (delta H) in meters.
    terrain_delta_h_m: f64,
    /// The method used for path loss prediction.
    prediction_method: Option<PredictionMethod>,
}

/// Source of link estimation.
#[derive(Debug, Clone, Copy, PartialEq)]
enum LinkSource {
    /// From ITM terrain prediction.
    Prediction,
    /// From observed zero-hop SNR values.
    Estimation,
}

// ============================================================================
// Model Building
// ============================================================================

/// Build a simulation model from mesh node JSON data.
pub fn build_model(config: &BuildModelConfig) -> Result<(), BuildModelError> {
    // Load and parse the JSON file
    let json_content = std::fs::read_to_string(&config.input_path)?;
    let mesh_data: MeshNodesData = serde_json::from_str(&json_content)?;

    if config.verbose {
        eprintln!("Loaded {} nodes from {}", mesh_data.nodes.len(), config.input_path.display());
    }

    // Filter and process nodes
    let processed_nodes = process_nodes(&mesh_data.nodes, config)?;
    
    if processed_nodes.is_empty() {
        return Err(BuildModelError::NoValidNodes);
    }

    if config.verbose {
        eprintln!("Processing {} nodes with location data", processed_nodes.len());
    }

    // Build lookup maps
    let node_by_id: HashMap<String, &ProcessedNode> = processed_nodes
        .iter()
        .map(|n| (n.public_key.clone(), n))
        .collect();

    // Extract zero-hop observations
    let zero_hop_data = extract_zero_hop_data(&mesh_data.nodes, &node_by_id);
    
    if config.verbose {
        eprintln!("Found {} zero-hop link observations", zero_hop_data.len());
    }

    // Load elevation source based on configuration
    let elevation: Arc<ElevationSource> = match config.elevation_source.as_str() {
        "aws" => {
            eprintln!(
                "Using AWS terrain tiles (cache: {}, zoom: {})...",
                config.elevation_cache.display(),
                config.zoom
            );
            let src = load_aws_elevation(&config.elevation_cache, config.zoom)
                .map_err(|e| BuildModelError::DemError(e.to_string()))?;
            Arc::new(src)
        }
        "local_dem" => {
            eprintln!("Loading DEM data from {}...", config.dem_dir.display());
            let dem = load_dem(&config.dem_dir)
                .map_err(|e| BuildModelError::DemError(e.to_string()))?;
            Arc::new(ElevationSource::from_local_dem(dem))
        }
        other => {
            return Err(BuildModelError::ConfigError(format!(
                "Unknown elevation source '{}'. Use 'aws' or 'local_dem'.",
                other
            )));
        }
    };
    
    eprintln!("Loading ITM library...");
    // Verify that the ITM library can be loaded (this will fail fast if the DLL is missing).
    // Each thread will load its own instance via thread_local for true parallelism.
    let _itm = load_itm().map_err(|e| BuildModelError::ItmError(e.to_string()))?;
    drop(_itm); // We'll use thread-local instances instead

    // Get SNR threshold for the spreading factor
    let snr_threshold = config.min_snr_threshold.unwrap_or_else(|| {
        LoraModulationParams::with_sf(config.spreading_factor).sensitivity_threshold_snr()
    });

    if config.verbose {
        eprintln!("Using SNR threshold: {:.1} dB (SF{})", snr_threshold, config.spreading_factor);
    }

    // Generate links between all node pairs using parallel processing
    let node_count = processed_nodes.len();
    let total_pairs = node_count * (node_count - 1);
    
    // Build list of all node pairs to evaluate
    let mut pairs: Vec<(usize, usize)> = Vec::with_capacity(total_pairs / 2);
    for i in 0..node_count {
        for j in (i + 1)..node_count {
            pairs.push((i, j));
        }
    }

    eprintln!("Evaluating {} potential links using {} threads...", total_pairs, rayon::current_num_threads());

    // Atomic counters for progress tracking
    let checked = Arc::new(AtomicUsize::new(0));
    let viable_count = Arc::new(AtomicUsize::new(0));
    let last_report = Arc::new(AtomicUsize::new(0));
    let start_time = std::time::Instant::now();
    let last_report_time = Arc::new(std::sync::atomic::AtomicU64::new(0));
    
    // Calculate reporting interval (report roughly 100 times during processing, but at least every 10 seconds)
    let report_interval = std::cmp::max(100, total_pairs / 100);

    // Process pairs in parallel using thread-local ITM instances.
    // The ITM library is thread-safe (no global state), but we use thread-local instances
    // to avoid any issues with concurrent libloading::Library::get() calls.
    let links: Vec<LinkData> = pairs
        .par_iter()
        .flat_map(|(i, j)| {
            let from_node = &processed_nodes[*i];
            let to_node = &processed_nodes[*j];
            
            let mut result = Vec::with_capacity(2);
            
            // Check for zero-hop data for this link
            let zero_hop_key = format!("{}:{}", from_node.public_key, to_node.public_key);
            let reverse_key = format!("{}:{}", to_node.public_key, from_node.public_key);

            // Use thread-local ITM instance for parallel access
            let eval_result = with_thread_itm(|itm| {
                let mut links = Vec::with_capacity(2);
                
                // Forward direction: from -> to
                if let Ok(Some(link)) = evaluate_link(
                    from_node,
                    to_node,
                    &elevation,
                    itm,
                    &zero_hop_data,
                    &zero_hop_key,
                    config,
                    snr_threshold,
                ) {
                    links.push(link);
                }

                // Reverse direction: to -> from
                if let Ok(Some(link)) = evaluate_link(
                    to_node,
                    from_node,
                    &elevation,
                    itm,
                    &zero_hop_data,
                    &reverse_key,
                    config,
                    snr_threshold,
                ) {
                    links.push(link);
                }
                
                Ok(links)
            });

            if let Ok(links) = eval_result {
                viable_count.fetch_add(links.len(), Ordering::Relaxed);
                result = links;
            }

            // Update progress counter
            let current = checked.fetch_add(2, Ordering::Relaxed) + 2;
            
            // Report progress periodically (by count) or at least every 10 seconds
            let last_count = last_report.load(Ordering::Relaxed);
            let elapsed_secs = start_time.elapsed().as_secs();
            let last_time = last_report_time.load(Ordering::Relaxed);
            
            let should_report = current >= last_count + report_interval || elapsed_secs >= last_time + 10;
            
            if should_report {
                // Try to be the one to report (avoid duplicate reports)
                if last_report.compare_exchange(last_count, current, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
                    last_report_time.store(elapsed_secs, Ordering::Relaxed);
                    let viable = viable_count.load(Ordering::Relaxed);
                    let elapsed_f = start_time.elapsed().as_secs_f64();
                    let links_per_sec = if elapsed_f > 0.0 { current as f64 / elapsed_f } else { 0.0 };
                    
                    // Get download stats if available
                    let download_info = if let Some(stats) = elevation.download_stats() {
                        if stats.tiles_downloaded > 0 {
                            format!(" | {} tiles, {:.1} MB downloaded", 
                                stats.tiles_downloaded,
                                stats.bytes_downloaded as f64 / 1_048_576.0)
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    };
                    
                    eprint!("\r  [{:>6}/{:>6}] {:.1}% complete, {} viable, {:.0} links/s{}    ", 
                        current, total_pairs, 
                        (current as f64 / total_pairs as f64) * 100.0,
                        viable,
                        links_per_sec,
                        download_info);
                    let _ = std::io::stderr().flush();
                }
            }
            
            result
        })
        .collect();

    // Final progress report (clear line and show final stats)
    let final_checked = checked.load(Ordering::Relaxed);
    let final_viable = viable_count.load(Ordering::Relaxed);
    let elapsed = start_time.elapsed();
    let final_links_per_sec = if elapsed.as_secs_f64() > 0.0 { final_checked as f64 / elapsed.as_secs_f64() } else { 0.0 };
    
    // Get final download stats
    let download_info = if let Some(stats) = elevation.download_stats() {
        if stats.tiles_downloaded > 0 {
            format!(" | {} tiles, {:.1} MB downloaded", 
                stats.tiles_downloaded,
                stats.bytes_downloaded as f64 / 1_048_576.0)
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    
    eprintln!("\r  [{:>6}/{:>6}] Done in {:.1}s! {} viable links, {:.0} links/s avg{}               ", 
        final_checked, total_pairs, elapsed.as_secs_f64(), final_viable, final_links_per_sec, download_info);

    // Generate YAML output
    generate_yaml(&processed_nodes, &links, config)?;

    Ok(())
}

/// Process raw nodes into filtered, processed nodes.
/// 
/// When multiple nodes have the same name, only the most recently seen node
/// is kept (based on the `last_seen` timestamp).
fn process_nodes(nodes: &[MeshNode], config: &BuildModelConfig) -> Result<Vec<ProcessedNode>, BuildModelError> {
    // First, collect all valid nodes with their timestamps
    let mut candidates: Vec<(&MeshNode, Option<&str>)> = nodes
        .iter()
        .filter(|n| {
            // Must have valid location
            match &n.location {
                Some(loc) if loc.is_valid() => {}
                _ => return false,
            }
            // Filter by mode if not including all types
            if !config.include_all_types && n.mode != "Repeater" {
                return false;
            }
            true
        })
        .map(|n| (n, n.last_seen.as_deref()))
        .collect();

    // Deduplicate by name, keeping the node with the most recent last_seen timestamp.
    // If multiple nodes have the same name, prefer the one with a later timestamp.
    // Nodes without timestamps are considered older than nodes with timestamps.
    let mut by_name: HashMap<&str, (&MeshNode, Option<&str>)> = HashMap::new();
    let mut duplicates_removed = 0usize;
    
    for (node, last_seen) in candidates.drain(..) {
        let name = node.name.as_str();
        match by_name.get(name) {
            Some((_, existing_ts)) => {
                // Compare timestamps - keep the newer one
                let should_replace = match (last_seen, existing_ts) {
                    // Both have timestamps: compare lexicographically (ISO 8601 sorts correctly)
                    (Some(new_ts), Some(old_ts)) => new_ts > *old_ts,
                    // New has timestamp, old doesn't: new is considered newer
                    (Some(_), None) => true,
                    // New has no timestamp: old is considered newer (or equal)
                    (None, _) => false,
                };
                if should_replace {
                    by_name.insert(name, (node, last_seen));
                }
                duplicates_removed += 1;
            }
            None => {
                by_name.insert(name, (node, last_seen));
            }
        }
    }
    
    if duplicates_removed > 0 && config.verbose {
        eprintln!("Removed {} duplicate nodes (kept most recent by last_seen)", duplicates_removed);
    }

    // Convert to ProcessedNode
    let mut processed: Vec<ProcessedNode> = by_name
        .into_values()
        .filter_map(|(n, _)| {
            let loc = n.location.as_ref()?;
            let lat = loc.lat()?;
            let lon = loc.lon()?;
            Some(ProcessedNode {
                name: n.name.clone(),
                public_key: n.public_key.clone(),
                mode: n.mode.clone(),
                lat,
                lon,
            })
        })
        .collect();

    // Sort by name for deterministic output
    processed.sort_by(|a, b| a.name.cmp(&b.name));

    // Limit number of nodes if specified
    if let Some(max) = config.max_nodes {
        processed.truncate(max);
    }

    Ok(processed)
}

/// Extract zero-hop observation data from adverts.
/// 
/// A zero-hop observation is when a node directly hears another node's advert
/// (path is empty string "").
fn extract_zero_hop_data(
    nodes: &[MeshNode],
    node_by_id: &HashMap<String, &ProcessedNode>,
) -> HashMap<String, ZeroHopData> {
    let mut zero_hop_map: HashMap<String, ZeroHopData> = HashMap::new();

    for node in nodes {
        // Skip nodes not in our processed set
        if !node_by_id.contains_key(&node.public_key) {
            continue;
        }

        if let Some(adverts) = &node.recent_adverts {
            for advert in adverts {
                // Check if this is a zero-hop advert (path is [""] or empty)
                let is_zero_hop = advert.path.len() == 1 && advert.path[0].is_empty();
                
                if !is_zero_hop {
                    continue;
                }

                // The transmitter is the node whose recent_adverts we're looking at
                let transmitter_id = &node.public_key;
                
                // The receiver is the origin_id (who heard and reported the advert)
                let receiver_id = &advert.origin_id;

                // Skip if receiver is not in our node set
                if !node_by_id.contains_key(receiver_id) {
                    continue;
                }

                // The key format is "transmitter:receiver" (direction: TX -> RX)
                let key = format!("{}:{}", transmitter_id, receiver_id);

                let entry = zero_hop_map.entry(key.clone()).or_insert_with(|| ZeroHopData {
                    receiver_id: receiver_id.clone(),
                    transmitter_id: transmitter_id.clone(),
                    snr_values: Vec::new(),
                });

                entry.snr_values.push(advert.snr as f64);
            }
        }
    }

    zero_hop_map
}

/// Evaluate a potential link between two nodes.
fn evaluate_link(
    from_node: &ProcessedNode,
    to_node: &ProcessedNode,
    elevation: &ElevationSource,
    itm: &mcsim_itm::Itm,
    zero_hop_data: &HashMap<String, ZeroHopData>,
    zero_hop_key: &str,
    config: &BuildModelConfig,
    snr_threshold: f64,
) -> Result<Option<LinkData>, BuildModelError> {
    // First, do link prediction using ITM
    let pred_config = LinkPredictionConfig {
        from_lat: from_node.lat,
        from_lon: from_node.lon,
        to_lat: to_node.lat,
        to_lon: to_node.lon,
        from_height: config.antenna_height,
        to_height: config.antenna_height,
        freq_mhz: 910.525,
        tx_power_dbm: config.tx_power_dbm,
        spreading_factor: config.spreading_factor,
        terrain_samples: config.terrain_samples,
    };

    let prediction = match predict_link_with_elevation(elevation, itm, &pred_config) {
        Ok(p) => p,
        Err(e) => {
            if config.verbose {
                eprintln!("Warning: Link prediction failed for {} -> {}: {}", 
                    from_node.name, to_node.name, e);
            }
            return Ok(None);
        }
    };

    // Check if we have zero-hop data for this link
    if let Some(zdata) = zero_hop_data.get(zero_hop_key) {
        if !zdata.snr_values.is_empty() {
            // Use estimation from observed data
            match estimate_snr_with_threshold(zdata.snr_values.clone(), snr_threshold) {
                Ok(est_result) => {
                    // Check if the estimated link is viable
                    if est_result.mean_snr < snr_threshold {
                        return Ok(None);
                    }

                    return Ok(Some(LinkData {
                        from: from_node.name.clone(),
                        to: to_node.name.clone(),
                        mean_snr_db: est_result.mean_snr,
                        snr_std_dev: est_result.std_dev,
                        source: LinkSource::Estimation,
                        predicted_snr_db: Some(prediction.snr_db),
                        distance_km: prediction.path.distance_km,
                        terrain_delta_h_m: prediction.terrain.delta_h,
                        prediction_method: Some(prediction.prediction_method),
                    }));
                }
                Err(e) => {
                    if config.verbose {
                        eprintln!("Warning: SNR estimation failed for {} -> {}: {}", 
                            from_node.name, to_node.name, e);
                    }
                    // Fall through to use prediction
                }
            }
        }
    }

    // No zero-hop data or estimation failed, use prediction
    if prediction.snr_db < snr_threshold {
        return Ok(None);
    }

    Ok(Some(LinkData {
        from: from_node.name.clone(),
        to: to_node.name.clone(),
        mean_snr_db: prediction.snr_db,
        snr_std_dev: prediction.snr_std_dev_db,
        source: LinkSource::Prediction,
        predicted_snr_db: None,
        distance_km: prediction.path.distance_km,
        terrain_delta_h_m: prediction.terrain.delta_h,
        prediction_method: Some(prediction.prediction_method),
    }))
}

/// Escape a string for YAML output.
/// 
/// This properly escapes special characters to produce valid YAML strings.
fn escape_yaml_string(s: &str) -> String {
    // Check if the string needs quoting
    let needs_quoting = s.is_empty()
        || s.contains(['"', '\'', '\n', '\r', '\t', ':', '#', '[', ']', '{', '}', ',', '&', '*', '!', '|', '>', '%', '@', '`'])
        || s.starts_with([' ', '-', '?'])
        || s.ends_with(' ')
        || s.chars().any(|c| c.is_control() || !c.is_ascii());
    
    if !needs_quoting {
        return s.to_string();
    }
    
    // Use double quotes and escape special characters
    let mut result = String::with_capacity(s.len() + 2);
    result.push('"');
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => {
                // Escape control characters as \xNN
                result.push_str(&format!("\\x{:02X}", c as u32));
            }
            _ => result.push(c),
        }
    }
    result.push('"');
    result
}

/// Generate the YAML output.
fn generate_yaml(
    nodes: &[ProcessedNode],
    links: &[LinkData],
    config: &BuildModelConfig,
) -> Result<(), BuildModelError> {
    let mut output: Box<dyn Write> = if let Some(ref path) = config.output_path {
        Box::new(std::fs::File::create(path)?)
    } else {
        Box::new(std::io::stdout())
    };

    // Write header comment
    writeln!(output, "# MeshCore Network Simulation Model")?;
    writeln!(output, "# Generated from: {}", config.input_path.display())?;
    writeln!(output, "# Generated at: {}", chrono::Utc::now().to_rfc3339())?;
    writeln!(output, "# Nodes: {}, Links: {}", nodes.len(), links.len())?;
    writeln!(output)?;

    // Write nodes
    writeln!(output, "nodes:")?;
    for node in nodes {
        // Generate the public key spec using configurable prefix length
        let prefix_len = config.public_key_prefix_len.min(node.public_key.len());
        let pub_key_prefix = &node.public_key[..prefix_len];
        
        writeln!(output, "  - name: {}", escape_yaml_string(&node.name))?;
        writeln!(output, "    location:")?;
        writeln!(output, "      lat: {}", node.lat)?;
        writeln!(output, "      lon: {}", node.lon)?;
        writeln!(output, "    keys:")?;
        writeln!(output, "      private_key: \"*\"")?;
        writeln!(output, "      public_key: \"{}*\"", pub_key_prefix)?;
        writeln!(output, "    firmware:")?;
        
        match node.mode.as_str() {
            "Repeater" => {
                writeln!(output, "      type: Repeater")?;
            }
            "Companion" => {
                writeln!(output, "      type: Companion")?;
            }
            "Room" | "Room Server" => {
                writeln!(output, "      type: RoomServer")?;
            }
            _ => {
                writeln!(output, "      type: Repeater")?;
            }
        }
        writeln!(output)?;
    }

    // Write edges
    writeln!(output, "edges:")?;
    for link in links {
        // Write a comment about the source with distance and terrain info
        match link.source {
            LinkSource::Estimation => {
                writeln!(output, "  # Estimated from zero-hop observations")?;
                writeln!(output, "  # Distance: {:.2} km, Terrain ΔH: {:.1}m",
                    link.distance_km, link.terrain_delta_h_m)?;
                if let Some(pred) = link.predicted_snr_db {
                    let method_str = link.prediction_method
                        .map(|m| format!(" ({})", m))
                        .unwrap_or_default();
                    writeln!(output, "  # Predicted SNR{}: {:.1} dB", method_str, pred)?;
                }
            }
            LinkSource::Prediction => {
                let method_str = link.prediction_method
                    .map(|m| format!("{} terrain prediction", m))
                    .unwrap_or_else(|| "Terrain prediction".to_string());
                writeln!(output, "  # {}", method_str)?;
                writeln!(output, "  # Distance: {:.2} km, Terrain ΔH: {:.1}m",
                    link.distance_km, link.terrain_delta_h_m)?;
            }
        }
        
        writeln!(output, "  - from: {}", escape_yaml_string(&link.from))?;
        writeln!(output, "    to: {}", escape_yaml_string(&link.to))?;
        writeln!(output, "    mean_snr_db_at20dbm: {:.1}", link.mean_snr_db)?;
        writeln!(output, "    snr_std_dev: {:.1}", link.snr_std_dev)?;
        
        // If this link was estimated, include the predicted value as a comment
        if link.source == LinkSource::Estimation {
            if let Some(pred) = link.predicted_snr_db {
                writeln!(output, "    # predicted_snr_db: {:.1}", pred)?;
            }
        }
        writeln!(output)?;
    }

    if let Some(ref path) = config.output_path {
        eprintln!("Wrote simulation model to: {}", path.display());
    }

    Ok(())
}
