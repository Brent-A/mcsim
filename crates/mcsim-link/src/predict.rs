//! Link prediction using DEM and ITM.

use mcsim_dem::{AwsTileFetcher, DemManager, DownloadCallback, DownloadStats};
use mcsim_itm::{Climate, Itm, Polarization, TerrainProfile};
use mcsim_model::properties::{
    ResolvedProperties, SimulationScope,
    // Radio properties
    RADIO_NOISE_FLOOR_DBM,
    RADIO_SNR_THRESHOLD_SF7_DB, RADIO_SNR_THRESHOLD_SF8_DB, RADIO_SNR_THRESHOLD_SF9_DB,
    RADIO_SNR_THRESHOLD_SF10_DB, RADIO_SNR_THRESHOLD_SF11_DB, RADIO_SNR_THRESHOLD_SF12_DB,
    // Link quality classification
    LINK_MARGIN_EXCELLENT_DB, LINK_MARGIN_GOOD_DB, LINK_MARGIN_MARGINAL_DB,
    // ITM parameters
    ITM_MIN_DISTANCE_M as ITM_MIN_DISTANCE_M_PROP, ITM_TERRAIN_SAMPLES, ITM_CLIMATE,
    ITM_POLARIZATION, ITM_GROUND_PERMITTIVITY, ITM_GROUND_CONDUCTIVITY, ITM_SURFACE_REFRACTIVITY,
    // FSPL parameters
    FSPL_MIN_DISTANCE_M as FSPL_MIN_DISTANCE_M_PROP,
    // Colocated parameters
    COLOCATED_PATH_LOSS_DB as COLOCATED_PATH_LOSS_DB_PROP,
    // Elevation tile parameters
    PREDICT_ELEVATION_CACHE_DIR, PREDICT_ELEVATION_ZOOM_LEVEL,
};
use std::path::Path;
use thiserror::Error;

/// Minimum path distance in meters required for ITM calculations.
/// Below this threshold, free-space path loss is used instead.
///
/// **Deprecated**: Use [`LinkPredictionParams::itm_min_distance_m`] instead.
pub const ITM_MIN_DISTANCE_M: f64 = 1000.0;

/// Minimum path distance in meters for free-space path loss to be valid.
/// Below this threshold, we assume near-field/direct coupling with minimal loss.
///
/// **Deprecated**: Use [`LinkPredictionParams::fspl_min_distance_m`] instead.
pub const FSPL_MIN_DISTANCE_M: f64 = 1.0;

/// Path loss in dB to use when nodes are co-located (distance ≈ 0).
/// This represents near-field coupling with some isolation.
///
/// **Deprecated**: Use [`LinkPredictionParams::colocated_path_loss_db`] instead.
pub const COLOCATED_PATH_LOSS_DB: f64 = 20.0;

// ============================================================================
// Link Prediction Parameters
// ============================================================================

/// Configurable parameters for link prediction.
///
/// This struct encapsulates all configurable parameters used in link prediction,
/// including radio parameters, link quality thresholds, and ITM model settings.
///
/// # Example
///
/// ```ignore
/// use mcsim_link::LinkPredictionParams;
/// use mcsim_model::properties::{ResolvedProperties, SimulationScope};
///
/// // Use defaults
/// let params = LinkPredictionParams::default();
///
/// // Or load from properties
/// let sim_props: ResolvedProperties<SimulationScope> = ResolvedProperties::new();
/// let params = LinkPredictionParams::from_properties(&sim_props);
/// ```
#[derive(Debug, Clone)]
pub struct LinkPredictionParams {
    // Radio parameters
    /// Noise floor in dBm.
    pub noise_floor_dbm: f64,
    /// SNR thresholds for SF7-SF12 (indexed 0-5).
    pub snr_thresholds: [f64; 6],

    // Link quality classification
    /// Minimum margin for excellent link quality (dB).
    pub margin_excellent_db: f64,
    /// Minimum margin for good link quality (dB).
    pub margin_good_db: f64,
    /// Minimum margin for marginal link quality (dB).
    pub margin_marginal_db: f64,

    // ITM parameters
    /// Minimum path distance for ITM model (meters).
    pub itm_min_distance_m: f64,
    /// Number of elevation points in terrain profile.
    pub itm_terrain_samples: u32,
    /// Radio climate setting.
    pub itm_climate: String,
    /// Radio polarization.
    pub itm_polarization: String,
    /// Ground relative permittivity (epsilon).
    pub itm_ground_permittivity: f64,
    /// Ground conductivity (S/m).
    pub itm_ground_conductivity: f64,
    /// Surface refractivity (N-units).
    pub itm_surface_refractivity: f64,

    // FSPL parameters
    /// Minimum distance for free-space path loss model (meters).
    pub fspl_min_distance_m: f64,

    // Colocated parameters
    /// Fixed near-field path loss for colocated nodes (dB).
    pub colocated_path_loss_db: f64,
}

impl Default for LinkPredictionParams {
    fn default() -> Self {
        Self {
            // Radio parameters
            noise_floor_dbm: -120.0,
            snr_thresholds: [-7.5, -10.0, -12.5, -15.0, -17.5, -20.0],

            // Link quality classification
            margin_excellent_db: 10.0,
            margin_good_db: 5.0,
            margin_marginal_db: 0.0,

            // ITM parameters
            itm_min_distance_m: 1000.0,
            itm_terrain_samples: 100,
            itm_climate: "continental_temperate".to_string(),
            itm_polarization: "vertical".to_string(),
            itm_ground_permittivity: 15.0,
            itm_ground_conductivity: 0.005,
            itm_surface_refractivity: 301.0,

            // FSPL parameters
            fspl_min_distance_m: 1.0,

            // Colocated parameters
            colocated_path_loss_db: 20.0,
        }
    }
}

impl LinkPredictionParams {
    /// Create a new `LinkPredictionParams` from simulation properties.
    ///
    /// This method extracts all relevant parameters from the resolved simulation
    /// properties, allowing configuration via YAML files.
    pub fn from_properties(props: &ResolvedProperties<SimulationScope>) -> Self {
        Self {
            // Radio parameters
            noise_floor_dbm: props.get(&RADIO_NOISE_FLOOR_DBM),
            snr_thresholds: [
                props.get(&RADIO_SNR_THRESHOLD_SF7_DB),
                props.get(&RADIO_SNR_THRESHOLD_SF8_DB),
                props.get(&RADIO_SNR_THRESHOLD_SF9_DB),
                props.get(&RADIO_SNR_THRESHOLD_SF10_DB),
                props.get(&RADIO_SNR_THRESHOLD_SF11_DB),
                props.get(&RADIO_SNR_THRESHOLD_SF12_DB),
            ],

            // Link quality classification
            margin_excellent_db: props.get(&LINK_MARGIN_EXCELLENT_DB),
            margin_good_db: props.get(&LINK_MARGIN_GOOD_DB),
            margin_marginal_db: props.get(&LINK_MARGIN_MARGINAL_DB),

            // ITM parameters
            itm_min_distance_m: props.get(&ITM_MIN_DISTANCE_M_PROP),
            itm_terrain_samples: props.get(&ITM_TERRAIN_SAMPLES),
            itm_climate: props.get(&ITM_CLIMATE),
            itm_polarization: props.get(&ITM_POLARIZATION),
            itm_ground_permittivity: props.get(&ITM_GROUND_PERMITTIVITY),
            itm_ground_conductivity: props.get(&ITM_GROUND_CONDUCTIVITY),
            itm_surface_refractivity: props.get(&ITM_SURFACE_REFRACTIVITY),

            // FSPL parameters
            fspl_min_distance_m: props.get(&FSPL_MIN_DISTANCE_M_PROP),

            // Colocated parameters
            colocated_path_loss_db: props.get(&COLOCATED_PATH_LOSS_DB_PROP),
        }
    }

    /// Get the SNR threshold for a given spreading factor.
    ///
    /// # Arguments
    ///
    /// * `spreading_factor` - LoRa spreading factor (7-12)
    ///
    /// # Returns
    ///
    /// The SNR threshold in dB for the given spreading factor.
    /// Returns the SF7 threshold for unknown spreading factors.
    pub fn snr_threshold_for_sf(&self, spreading_factor: u8) -> f64 {
        match spreading_factor {
            7 => self.snr_thresholds[0],
            8 => self.snr_thresholds[1],
            9 => self.snr_thresholds[2],
            10 => self.snr_thresholds[3],
            11 => self.snr_thresholds[4],
            12 => self.snr_thresholds[5],
            _ => self.snr_thresholds[0], // Default to SF7
        }
    }

    /// Classify link quality based on margin.
    ///
    /// # Arguments
    ///
    /// * `link_margin` - Link margin in dB
    ///
    /// # Returns
    ///
    /// The [`LinkStatus`] classification for the given margin.
    pub fn classify_link(&self, link_margin: f64) -> LinkStatus {
        if link_margin > self.margin_excellent_db {
            LinkStatus::Excellent
        } else if link_margin > self.margin_good_db {
            LinkStatus::Good
        } else if link_margin > self.margin_marginal_db {
            LinkStatus::Marginal
        } else {
            LinkStatus::Unreliable
        }
    }

    /// Parse the ITM climate setting from the string configuration.
    pub fn parse_climate(&self) -> Climate {
        match self.itm_climate.to_lowercase().as_str() {
            "equatorial" => Climate::Equatorial,
            "continental_subtropical" => Climate::ContinentalSubtropical,
            "maritime_subtropical" => Climate::MaritimeSubtropical,
            "desert" => Climate::Desert,
            "continental_temperate" => Climate::ContinentalTemperate,
            "maritime_temperate_overland" => Climate::MaritimeTemperateOverLand,
            "maritime_temperate_oversea" => Climate::MaritimeTemperateOverSea,
            _ => Climate::ContinentalTemperate, // Default
        }
    }

    /// Parse the ITM polarization setting from the string configuration.
    pub fn parse_polarization(&self) -> Polarization {
        match self.itm_polarization.to_lowercase().as_str() {
            "horizontal" => Polarization::Horizontal,
            "vertical" => Polarization::Vertical,
            _ => Polarization::Vertical, // Default
        }
    }
}

/// Errors that can occur during link prediction.
#[derive(Debug, Error)]
pub enum LinkPredictionError {
    #[error("DEM error: {0}")]
    DemError(String),

    #[error("ITM error: {0}")]
    ItmError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Configuration for link prediction.
#[derive(Debug, Clone)]
pub struct LinkPredictionConfig {
    /// Latitude of the transmitter (degrees).
    pub from_lat: f64,
    /// Longitude of the transmitter (degrees).
    pub from_lon: f64,
    /// Latitude of the receiver (degrees).
    pub to_lat: f64,
    /// Longitude of the receiver (degrees).
    pub to_lon: f64,
    /// Height of the transmitter above ground (meters).
    pub from_height: f64,
    /// Height of the receiver above ground (meters).
    pub to_height: f64,
    /// Frequency in MHz.
    pub freq_mhz: f64,
    /// TX power in dBm.
    pub tx_power_dbm: i8,
    /// LoRa spreading factor (7-12).
    pub spreading_factor: u8,
    /// Number of terrain samples along the path.
    pub terrain_samples: usize,
}

impl Default for LinkPredictionConfig {
    fn default() -> Self {
        Self {
            from_lat: 0.0,
            from_lon: 0.0,
            to_lat: 0.0,
            to_lon: 0.0,
            from_height: 2.0,
            to_height: 2.0,
            freq_mhz: 910.525, // LoRa default
            tx_power_dbm: 20,
            spreading_factor: 7,
            terrain_samples: 100,
        }
    }
}

/// Information about the path between transmitter and receiver.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PathInfo {
    /// Latitude of the transmitter (degrees).
    pub from_lat: f64,
    /// Longitude of the transmitter (degrees).
    pub from_lon: f64,
    /// Latitude of the receiver (degrees).
    pub to_lat: f64,
    /// Longitude of the receiver (degrees).
    pub to_lon: f64,
    /// Height of transmitter above ground (meters).
    pub from_height: f64,
    /// Height of receiver above ground (meters).
    pub to_height: f64,
    /// Path distance in kilometers.
    pub distance_km: f64,
}

/// Information about the terrain along the path.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TerrainInfo {
    /// Number of elevation samples.
    pub sample_count: usize,
    /// Resolution between samples (meters).
    pub resolution_m: f64,
    /// Minimum elevation along path (meters).
    pub min_elevation: f64,
    /// Maximum elevation along path (meters).
    pub max_elevation: f64,
    /// Mean elevation along path (meters).
    pub mean_elevation: f64,
    /// Terrain irregularity (delta H) in meters.
    pub delta_h: f64,
}

/// Radio parameters used in the prediction.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RadioParams {
    /// Frequency in MHz.
    pub freq_mhz: f64,
    /// TX power in dBm.
    pub tx_power_dbm: i8,
    /// Noise floor in dBm.
    pub noise_floor_dbm: f64,
    /// LoRa spreading factor.
    pub spreading_factor: u8,
    /// SNR threshold for the spreading factor (dB).
    pub snr_threshold_db: f64,
}

/// Link quality status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LinkStatus {
    /// Excellent link with >10 dB margin.
    Excellent,
    /// Good link with >5 dB margin.
    Good,
    /// Marginal link with 0-5 dB margin.
    Marginal,
    /// Unreliable link with negative margin.
    Unreliable,
}

impl LinkStatus {
    /// Returns a human-readable description of the status.
    pub fn description(&self) -> &'static str {
        match self {
            LinkStatus::Excellent => "Excellent (>10 dB margin)",
            LinkStatus::Good => "Good (>5 dB margin)",
            LinkStatus::Marginal => "Marginal (0-5 dB margin)",
            LinkStatus::Unreliable => "UNRELIABLE (negative margin)",
        }
    }
}

impl std::fmt::Display for LinkStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

/// The method used for path loss prediction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PredictionMethod {
    /// Irregular Terrain Model (ITM) - used for paths >= 1km.
    Itm,
    /// Free-space path loss - used for short paths < 1km where ITM is not valid.
    FreeSpace,
    /// Co-located nodes - used when distance is effectively zero.
    Colocated,
}

impl std::fmt::Display for PredictionMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PredictionMethod::Itm => write!(f, "ITM"),
            PredictionMethod::FreeSpace => write!(f, "Free-Space"),
            PredictionMethod::Colocated => write!(f, "Co-located"),
        }
    }
}

/// Result of a link prediction.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LinkPrediction {
    /// Path information.
    pub path: PathInfo,
    /// Terrain information.
    pub terrain: TerrainInfo,
    /// Radio parameters used.
    pub radio: RadioParams,
    /// Median path loss in dB (from ITM or free-space).
    pub path_loss_db: f64,
    /// The method used for prediction.
    pub prediction_method: PredictionMethod,
    /// ITM warning flags (0 if none, or if free-space was used).
    pub itm_warnings: u32,
    /// Predicted mean SNR in dB.
    pub snr_db: f64,
    /// Estimated standard deviation of SNR (dB).
    pub snr_std_dev_db: f64,
    /// Link margin in dB (SNR - threshold).
    pub link_margin_db: f64,
    /// Overall link status.
    pub status: LinkStatus,
}

impl LinkPrediction {
    /// Returns true if the link is considered viable (positive margin).
    pub fn is_viable(&self) -> bool {
        self.link_margin_db > 0.0
    }
}

/// Predict link quality between two geographic coordinates.
///
/// This function uses DEM data to extract terrain elevation profile along the path,
/// then applies the Irregular Terrain Model (ITM) to estimate path loss.
///
/// This is a convenience wrapper that uses default prediction parameters.
/// For customizable parameters, use [`predict_link_with_params`].
///
/// # Arguments
///
/// * `dem` - A configured DEM manager with loaded elevation data.
/// * `itm` - A configured ITM instance.
/// * `config` - Link prediction configuration.
///
/// # Returns
///
/// A `LinkPrediction` containing all computed link parameters.
pub fn predict_link(
    dem: &DemManager,
    itm: &Itm,
    config: &LinkPredictionConfig,
) -> Result<LinkPrediction, LinkPredictionError> {
    predict_link_with_params(dem, itm, config, &LinkPredictionParams::default())
}

/// Predict link quality between two geographic coordinates with custom parameters.
///
/// This function uses DEM data to extract terrain elevation profile along the path,
/// then applies the Irregular Terrain Model (ITM) to estimate path loss.
///
/// # Arguments
///
/// * `dem` - A configured DEM manager with loaded elevation data.
/// * `itm` - A configured ITM instance.
/// * `config` - Link prediction configuration.
/// * `params` - Link prediction parameters (thresholds, ITM settings, etc.).
///
/// # Returns
///
/// A `LinkPrediction` containing all computed link parameters.
///
/// # Example
///
/// ```ignore
/// use mcsim_link::{predict_link_with_params, LinkPredictionConfig, LinkPredictionParams};
/// use mcsim_model::properties::{ResolvedProperties, SimulationScope};
///
/// let sim_props: ResolvedProperties<SimulationScope> = ResolvedProperties::new();
/// let params = LinkPredictionParams::from_properties(&sim_props);
///
/// let result = predict_link_with_params(&dem, &itm, &config, &params)?;
/// ```
pub fn predict_link_with_params(
    dem: &DemManager,
    itm: &Itm,
    config: &LinkPredictionConfig,
    params: &LinkPredictionParams,
) -> Result<LinkPrediction, LinkPredictionError> {
    // Validate configuration
    if config.terrain_samples < 2 {
        return Err(LinkPredictionError::ConfigError(
            "Need at least 2 terrain samples".to_string(),
        ));
    }

    // Sample terrain along the path
    let samples = dem.sample_line(
        config.from_lat,
        config.from_lon,
        config.to_lat,
        config.to_lon,
        config.terrain_samples,
    );

    // Extract elevations, checking for errors
    let mut elevations: Vec<f64> = Vec::with_capacity(samples.len());
    for (distance, result) in &samples {
        match result {
            Ok(elev) => elevations.push(*elev as f64),
            Err(e) => {
                return Err(LinkPredictionError::DemError(format!(
                    "Failed to get elevation at distance {:.1}m: {}",
                    distance, e
                )));
            }
        }
    }

    if elevations.len() < 2 {
        return Err(LinkPredictionError::ConfigError(
            "Insufficient elevation samples".to_string(),
        ));
    }

    // Calculate path distance from samples
    let path_distance_m = samples.last().map(|(d, _)| *d).unwrap_or(0.0);
    let path_distance_km = path_distance_m / 1000.0;

    // Calculate resolution from samples
    let resolution_m = if elevations.len() > 1 {
        path_distance_m / (elevations.len() - 1) as f64
    } else {
        0.0
    };

    // Validate resolution is reasonable (only needed for ITM)
    let resolution_valid = resolution_m.is_finite() && resolution_m > 0.0;

    // Validate all elevations are finite
    for (i, elev) in elevations.iter().enumerate() {
        if !elev.is_finite() {
            return Err(LinkPredictionError::ConfigError(format!(
                "Invalid elevation at sample {}: {}",
                i, elev
            )));
        }
    }

    // Calculate terrain statistics
    let min_elev = elevations.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_elev = elevations.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mean_elev = elevations.iter().sum::<f64>() / elevations.len() as f64;
    let delta_h = max_elev - min_elev;

    // Determine prediction method and calculate path loss
    let (path_loss_db, itm_warnings, prediction_method) = if path_distance_m < params.fspl_min_distance_m {
        // Use fixed path loss for co-located nodes where distance ≈ 0
        // Free-space model returns -infinity at zero distance
        (params.colocated_path_loss_db, 0, PredictionMethod::Colocated)
    } else if path_distance_m >= params.itm_min_distance_m && resolution_valid {
        // Use ITM for paths >= configured min distance with valid terrain resolution
        let profile = TerrainProfile::from_elevations(resolution_m, &elevations);
        let pfl = profile.to_pfl();

        // Get terrain parameters from params
        let epsilon = params.itm_ground_permittivity;
        let sigma = params.itm_ground_conductivity;
        let n_0 = params.itm_surface_refractivity;
        let climate = params.parse_climate();
        let polarization = params.parse_polarization();

        // Get the median (50/50/50) result - this is the most likely path loss
        let median_result = itm
            .p2p_tls(
                config.from_height,
                config.to_height,
                &pfl,
                climate,
                n_0,
                config.freq_mhz,
                polarization,
                epsilon,
                sigma,
                0, // mdvar: single message mode
                50.0,
                50.0,
                50.0,
            )
            .map_err(|e| LinkPredictionError::ItmError(format!("ITM calculation failed: {}", e)))?;

        (median_result.loss_db, median_result.warnings.bits() as u32, PredictionMethod::Itm)
    } else {
        // Use free-space path loss for short distances where ITM is not valid
        let fspl = itm
            .free_space_loss(path_distance_m, config.freq_mhz)
            .map_err(|e| LinkPredictionError::ItmError(format!("Free-space calculation failed: {}", e)))?;
        
        (fspl, 0, PredictionMethod::FreeSpace)
    };

    // Estimate SNR variability based on terrain irregularity and prediction method
    let std_dev_loss = match prediction_method {
        PredictionMethod::Colocated | PredictionMethod::FreeSpace => {
            // Co-located and free-space are more predictable, use lower variability
            2.0
        }
        PredictionMethod::Itm => {
            if delta_h < 10.0 {
                2.0 // Flat terrain - lower variability
            } else if delta_h < 50.0 {
                3.5 // Moderate terrain
            } else if delta_h < 100.0 {
                5.0 // Hilly terrain
            } else {
                6.5 // Mountainous terrain - higher variability
            }
        }
    };

    // Calculate SNR using configurable noise floor
    // SNR = TX Power - Path Loss - Noise Floor
    let noise_floor_dbm = params.noise_floor_dbm;
    let median_snr = config.tx_power_dbm as f64 - path_loss_db - noise_floor_dbm;

    // LoRa link budget assessment - sensitivity varies by SF
    let snr_threshold = params.snr_threshold_for_sf(config.spreading_factor);

    let link_margin = median_snr - snr_threshold;

    // Classify link using configurable thresholds
    let status = params.classify_link(link_margin);

    Ok(LinkPrediction {
        path: PathInfo {
            from_lat: config.from_lat,
            from_lon: config.from_lon,
            to_lat: config.to_lat,
            to_lon: config.to_lon,
            from_height: config.from_height,
            to_height: config.to_height,
            distance_km: path_distance_km,
        },
        terrain: TerrainInfo {
            sample_count: elevations.len(),
            resolution_m,
            min_elevation: min_elev,
            max_elevation: max_elev,
            mean_elevation: mean_elev,
            delta_h,
        },
        radio: RadioParams {
            freq_mhz: config.freq_mhz,
            tx_power_dbm: config.tx_power_dbm,
            noise_floor_dbm,
            spreading_factor: config.spreading_factor,
            snr_threshold_db: snr_threshold,
        },
        path_loss_db,
        prediction_method,
        itm_warnings,
        snr_db: median_snr,
        snr_std_dev_db: std_dev_loss,
        link_margin_db: link_margin,
        status,
    })
}

// ============================================================================
// Elevation Source Abstraction
// ============================================================================

/// Source of elevation data for terrain profiles.
///
/// This enum abstracts over different sources of elevation data:
/// - Local USGS DEM tiles
/// - AWS terrain tiles (fetched on demand and cached locally)
pub enum ElevationSource {
    /// Local USGS DEM tiles loaded via `DemManager`.
    LocalDem(DemManager),
    /// AWS terrain tiles fetched on demand via `AwsTileFetcher`.
    AwsTiles {
        fetcher: AwsTileFetcher,
        callback: Option<DownloadCallback>,
    },
}

impl std::fmt::Debug for ElevationSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElevationSource::LocalDem(dem) => f.debug_tuple("LocalDem").field(dem).finish(),
            ElevationSource::AwsTiles { fetcher, .. } => {
                f.debug_struct("AwsTiles")
                    .field("fetcher", fetcher)
                    .field("callback", &"<callback>")
                    .finish()
            }
        }
    }
}

impl ElevationSource {
    /// Create an elevation source from local USGS DEM tiles.
    pub fn from_local_dem(dem: DemManager) -> Self {
        ElevationSource::LocalDem(dem)
    }

    /// Create an elevation source using AWS terrain tiles.
    ///
    /// # Arguments
    /// * `cache_dir` - Directory to cache downloaded tiles
    /// * `zoom` - Zoom level (1-14, default 12)
    /// * `callback` - Optional callback for download progress feedback
    pub fn from_aws_tiles<P: AsRef<Path>>(
        cache_dir: P,
        zoom: u8,
        callback: Option<DownloadCallback>,
    ) -> Result<Self, LinkPredictionError> {
        let fetcher = AwsTileFetcher::with_zoom(cache_dir, zoom).map_err(|e| {
            LinkPredictionError::DemError(format!("Failed to create AWS tile fetcher: {}", e))
        })?;
        Ok(ElevationSource::AwsTiles { fetcher, callback })
    }

    /// Create an elevation source from simulation properties.
    ///
    /// Uses AWS tiles with settings from `PREDICT_ELEVATION_CACHE_DIR` and
    /// `PREDICT_ELEVATION_ZOOM_LEVEL` properties.
    pub fn from_properties(
        props: &ResolvedProperties<SimulationScope>,
        callback: Option<DownloadCallback>,
    ) -> Result<Self, LinkPredictionError> {
        let cache_dir: String = props.get(&PREDICT_ELEVATION_CACHE_DIR);
        let zoom: u8 = props.get(&PREDICT_ELEVATION_ZOOM_LEVEL);
        Self::from_aws_tiles(cache_dir, zoom, callback)
    }

    /// Get download statistics (for AWS tiles source only).
    /// Returns None for local DEM sources.
    pub fn download_stats(&self) -> Option<DownloadStats> {
        match self {
            ElevationSource::LocalDem(_) => None,
            ElevationSource::AwsTiles { fetcher, .. } => Some(fetcher.download_stats()),
        }
    }

    /// Get elevation at a coordinate.
    pub fn get_elevation(&self, lat: f64, lon: f64) -> Result<f32, LinkPredictionError> {
        match self {
            ElevationSource::LocalDem(dem) => dem.get_elevation(lat, lon).map_err(|e| {
                LinkPredictionError::DemError(format!("Failed to get elevation: {}", e))
            }),
            ElevationSource::AwsTiles { fetcher, callback } => {
                fetcher
                    .get_elevation_with_callback(lat, lon, callback.as_ref())
                    .map_err(|e| {
                        LinkPredictionError::DemError(format!("Failed to get elevation: {}", e))
                    })
            }
        }
    }

    /// Sample elevations along a line between two points.
    ///
    /// Returns a vector of (distance, elevation) pairs, where distance is
    /// measured from the start point in meters.
    pub fn sample_line(
        &self,
        start_lat: f64,
        start_lon: f64,
        end_lat: f64,
        end_lon: f64,
        num_samples: usize,
    ) -> Vec<(f64, Result<f32, LinkPredictionError>)> {
        match self {
            ElevationSource::LocalDem(dem) => {
                dem.sample_line(start_lat, start_lon, end_lat, end_lon, num_samples)
                    .into_iter()
                    .map(|(d, r)| {
                        (
                            d,
                            r.map_err(|e| {
                                LinkPredictionError::DemError(format!(
                                    "Failed to get elevation: {}",
                                    e
                                ))
                            }),
                        )
                    })
                    .collect()
            }
            ElevationSource::AwsTiles { fetcher, callback } => {
                // Calculate total distance using haversine formula
                let total_distance = haversine_distance(start_lat, start_lon, end_lat, end_lon);

                let mut results = Vec::with_capacity(num_samples);
                for i in 0..num_samples {
                    let t = i as f64 / (num_samples - 1) as f64;
                    let lat = start_lat + t * (end_lat - start_lat);
                    let lon = start_lon + t * (end_lon - start_lon);
                    let distance = t * total_distance;

                    let elevation = fetcher
                        .get_elevation_with_callback(lat, lon, callback.as_ref())
                        .map_err(|e| {
                            LinkPredictionError::DemError(format!(
                                "Failed to get elevation: {}",
                                e
                            ))
                        });
                    results.push((distance, elevation));
                }
                results
            }
        }
    }
}

/// Calculate the distance between two points using the haversine formula.
/// Returns the distance in meters.
fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;

    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let delta_lat = (lat2 - lat1).to_radians();
    let delta_lon = (lon2 - lon1).to_radians();

    let a = (delta_lat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();

    EARTH_RADIUS_M * c
}

/// Load AWS elevation tiles for link prediction.
///
/// # Arguments
///
/// * `cache_dir` - Path to the cache directory for downloaded tiles.
/// * `zoom` - Zoom level (1-14, default 12).
///
/// # Returns
///
/// An `ElevationSource` configured to fetch from AWS.
pub fn load_aws_elevation<P: AsRef<Path>>(
    cache_dir: P,
    zoom: u8,
) -> Result<ElevationSource, LinkPredictionError> {
    // Create a callback that prints to stderr
    let callback: DownloadCallback = Box::new(|msg: &str| {
        eprintln!("{}", msg);
    });
    ElevationSource::from_aws_tiles(cache_dir, zoom, Some(callback))
}

/// Load AWS elevation tiles with a custom progress callback.
///
/// # Arguments
///
/// * `cache_dir` - Path to the cache directory for downloaded tiles.
/// * `zoom` - Zoom level (1-14, default 12).
/// * `callback` - Callback for download progress feedback.
///
/// # Returns
///
/// An `ElevationSource` configured to fetch from AWS.
pub fn load_aws_elevation_with_callback<P: AsRef<Path>>(
    cache_dir: P,
    zoom: u8,
    callback: Option<DownloadCallback>,
) -> Result<ElevationSource, LinkPredictionError> {
    ElevationSource::from_aws_tiles(cache_dir, zoom, callback)
}

// ============================================================================
// Generic Link Prediction with ElevationSource
// ============================================================================

/// Predict link quality using any elevation source.
///
/// This is the recommended entry point for link prediction. It works with both
/// local DEM files and AWS terrain tiles.
///
/// # Arguments
///
/// * `elevation` - The elevation data source.
/// * `itm` - A configured ITM instance.
/// * `config` - Link prediction configuration.
///
/// # Returns
///
/// A `LinkPrediction` containing all computed link parameters.
pub fn predict_link_with_elevation(
    elevation: &ElevationSource,
    itm: &Itm,
    config: &LinkPredictionConfig,
) -> Result<LinkPrediction, LinkPredictionError> {
    predict_link_with_elevation_and_params(elevation, itm, config, &LinkPredictionParams::default())
}

/// Predict link quality using any elevation source with custom parameters.
///
/// # Arguments
///
/// * `elevation` - The elevation data source.
/// * `itm` - A configured ITM instance.
/// * `config` - Link prediction configuration.
/// * `params` - Link prediction parameters (thresholds, ITM settings, etc.).
///
/// # Returns
///
/// A `LinkPrediction` containing all computed link parameters.
pub fn predict_link_with_elevation_and_params(
    elevation: &ElevationSource,
    itm: &Itm,
    config: &LinkPredictionConfig,
    params: &LinkPredictionParams,
) -> Result<LinkPrediction, LinkPredictionError> {
    // Validate configuration
    if config.terrain_samples < 2 {
        return Err(LinkPredictionError::ConfigError(
            "Need at least 2 terrain samples".to_string(),
        ));
    }

    // Sample terrain along the path
    let samples = elevation.sample_line(
        config.from_lat,
        config.from_lon,
        config.to_lat,
        config.to_lon,
        config.terrain_samples,
    );

    // Extract elevations, checking for errors
    let mut elevations: Vec<f64> = Vec::with_capacity(samples.len());
    for (distance, result) in &samples {
        match result {
            Ok(elev) => elevations.push(*elev as f64),
            Err(e) => {
                return Err(LinkPredictionError::DemError(format!(
                    "Failed to get elevation at distance {:.1}m: {}",
                    distance, e
                )));
            }
        }
    }

    if elevations.len() < 2 {
        return Err(LinkPredictionError::ConfigError(
            "Insufficient elevation samples".to_string(),
        ));
    }

    // Calculate path distance from samples
    let path_distance_m = samples.last().map(|(d, _)| *d).unwrap_or(0.0);
    let path_distance_km = path_distance_m / 1000.0;

    // Calculate resolution from samples
    let resolution_m = if elevations.len() > 1 {
        path_distance_m / (elevations.len() - 1) as f64
    } else {
        0.0
    };

    // Validate resolution is reasonable (only needed for ITM)
    let resolution_valid = resolution_m.is_finite() && resolution_m > 0.0;

    // Validate all elevations are finite
    for (i, elev) in elevations.iter().enumerate() {
        if !elev.is_finite() {
            return Err(LinkPredictionError::ConfigError(format!(
                "Invalid elevation at sample {}: {}",
                i, elev
            )));
        }
    }

    // Calculate terrain statistics
    let min_elev = elevations.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_elev = elevations.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mean_elev = elevations.iter().sum::<f64>() / elevations.len() as f64;
    let delta_h = max_elev - min_elev;

    // Determine prediction method and calculate path loss
    let (path_loss_db, itm_warnings, prediction_method) =
        if path_distance_m < params.fspl_min_distance_m {
            // Use fixed path loss for co-located nodes where distance ≈ 0
            (params.colocated_path_loss_db, 0, PredictionMethod::Colocated)
        } else if path_distance_m >= params.itm_min_distance_m && resolution_valid {
            // Use ITM for paths >= configured min distance with valid terrain resolution
            let profile = TerrainProfile::from_elevations(resolution_m, &elevations);
            let pfl = profile.to_pfl();

            // Get terrain parameters from params
            let epsilon = params.itm_ground_permittivity;
            let sigma = params.itm_ground_conductivity;
            let n_0 = params.itm_surface_refractivity;
            let climate = params.parse_climate();
            let polarization = params.parse_polarization();

            // Get the median (50/50/50) result - this is the most likely path loss
            let median_result = itm
                .p2p_tls(
                    config.from_height,
                    config.to_height,
                    &pfl,
                    climate,
                    n_0,
                    config.freq_mhz,
                    polarization,
                    epsilon,
                    sigma,
                    0, // mdvar: single message mode
                    50.0,
                    50.0,
                    50.0,
                )
                .map_err(|e| {
                    LinkPredictionError::ItmError(format!("ITM calculation failed: {}", e))
                })?;

            (
                median_result.loss_db,
                median_result.warnings.bits() as u32,
                PredictionMethod::Itm,
            )
        } else {
            // Use free-space path loss for short distances where ITM is not valid
            let fspl = itm.free_space_loss(path_distance_m, config.freq_mhz).map_err(|e| {
                LinkPredictionError::ItmError(format!("Free-space calculation failed: {}", e))
            })?;

            (fspl, 0, PredictionMethod::FreeSpace)
        };

    // Estimate SNR variability based on terrain irregularity and prediction method
    let std_dev_loss = match prediction_method {
        PredictionMethod::Colocated | PredictionMethod::FreeSpace => {
            // Co-located and free-space are more predictable, use lower variability
            2.0
        }
        PredictionMethod::Itm => {
            if delta_h < 10.0 {
                2.0 // Flat terrain - lower variability
            } else if delta_h < 50.0 {
                3.5 // Moderate terrain
            } else if delta_h < 100.0 {
                5.0 // Hilly terrain
            } else {
                6.5 // Mountainous terrain - higher variability
            }
        }
    };

    // Calculate SNR using configurable noise floor
    let noise_floor_dbm = params.noise_floor_dbm;
    let median_snr = config.tx_power_dbm as f64 - path_loss_db - noise_floor_dbm;

    // LoRa link budget assessment - sensitivity varies by SF
    let snr_threshold = params.snr_threshold_for_sf(config.spreading_factor);
    let link_margin = median_snr - snr_threshold;

    // Classify link using configurable thresholds
    let status = params.classify_link(link_margin);

    Ok(LinkPrediction {
        path: PathInfo {
            from_lat: config.from_lat,
            from_lon: config.from_lon,
            to_lat: config.to_lat,
            to_lon: config.to_lon,
            from_height: config.from_height,
            to_height: config.to_height,
            distance_km: path_distance_km,
        },
        terrain: TerrainInfo {
            sample_count: elevations.len(),
            resolution_m,
            min_elevation: min_elev,
            max_elevation: max_elev,
            mean_elevation: mean_elev,
            delta_h,
        },
        radio: RadioParams {
            freq_mhz: config.freq_mhz,
            tx_power_dbm: config.tx_power_dbm,
            noise_floor_dbm,
            spreading_factor: config.spreading_factor,
            snr_threshold_db: snr_threshold,
        },
        path_loss_db,
        prediction_method,
        itm_warnings,
        snr_db: median_snr,
        snr_std_dev_db: std_dev_loss,
        link_margin_db: link_margin,
        status,
    })
}

// ============================================================================
// Legacy DEM-based Functions (for backward compatibility)
// ============================================================================

/// Load DEM data from a directory.
///
/// # Arguments
///
/// * `dem_dir` - Path to the directory containing DEM tiles.
///
/// # Returns
///
/// A configured `DemManager` with all tiles loaded.
pub fn load_dem<P: AsRef<Path>>(dem_dir: P) -> Result<DemManager, LinkPredictionError> {
    let mut dem = DemManager::new();
    let tile_count = dem.add_directory(dem_dir.as_ref()).map_err(|e| {
        LinkPredictionError::DemError(format!("Failed to load DEM data: {}", e))
    })?;

    if tile_count == 0 {
        return Err(LinkPredictionError::DemError(format!(
            "No DEM tiles found in {}",
            dem_dir.as_ref().display()
        )));
    }

    Ok(dem)
}

/// Load the ITM library.
///
/// # Returns
///
/// A configured `Itm` instance ready for calculations.
pub fn load_itm() -> Result<Itm, LinkPredictionError> {
    Itm::new().map_err(|e| LinkPredictionError::ItmError(format!("Failed to load ITM library: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_link_status_description() {
        assert_eq!(LinkStatus::Excellent.description(), "Excellent (>10 dB margin)");
        assert_eq!(LinkStatus::Good.description(), "Good (>5 dB margin)");
        assert_eq!(LinkStatus::Marginal.description(), "Marginal (0-5 dB margin)");
        assert_eq!(LinkStatus::Unreliable.description(), "UNRELIABLE (negative margin)");
    }

    #[test]
    fn test_default_config() {
        let config = LinkPredictionConfig::default();
        assert_eq!(config.from_height, 2.0);
        assert_eq!(config.to_height, 2.0);
        assert_eq!(config.freq_mhz, 910.525);
        assert_eq!(config.tx_power_dbm, 20);
        assert_eq!(config.spreading_factor, 7);
        assert_eq!(config.terrain_samples, 100);
    }

    #[test]
    fn test_link_prediction_params_default() {
        let params = LinkPredictionParams::default();

        // Radio parameters
        assert_eq!(params.noise_floor_dbm, -120.0);
        assert_eq!(params.snr_thresholds, [-7.5, -10.0, -12.5, -15.0, -17.5, -20.0]);

        // Link quality classification
        assert_eq!(params.margin_excellent_db, 10.0);
        assert_eq!(params.margin_good_db, 5.0);
        assert_eq!(params.margin_marginal_db, 0.0);

        // ITM parameters
        assert_eq!(params.itm_min_distance_m, 1000.0);
        assert_eq!(params.itm_terrain_samples, 100);
        assert_eq!(params.itm_climate, "continental_temperate");
        assert_eq!(params.itm_polarization, "vertical");
        assert_eq!(params.itm_ground_permittivity, 15.0);
        assert_eq!(params.itm_ground_conductivity, 0.005);
        assert_eq!(params.itm_surface_refractivity, 301.0);

        // FSPL parameters
        assert_eq!(params.fspl_min_distance_m, 1.0);

        // Colocated parameters
        assert_eq!(params.colocated_path_loss_db, 20.0);
    }

    #[test]
    fn test_link_prediction_params_snr_threshold() {
        let params = LinkPredictionParams::default();

        assert_eq!(params.snr_threshold_for_sf(7), -7.5);
        assert_eq!(params.snr_threshold_for_sf(8), -10.0);
        assert_eq!(params.snr_threshold_for_sf(9), -12.5);
        assert_eq!(params.snr_threshold_for_sf(10), -15.0);
        assert_eq!(params.snr_threshold_for_sf(11), -17.5);
        assert_eq!(params.snr_threshold_for_sf(12), -20.0);
        // Unknown SF defaults to SF7
        assert_eq!(params.snr_threshold_for_sf(6), -7.5);
        assert_eq!(params.snr_threshold_for_sf(13), -7.5);
    }

    #[test]
    fn test_link_prediction_params_classify_link() {
        let params = LinkPredictionParams::default();

        assert_eq!(params.classify_link(15.0), LinkStatus::Excellent);
        assert_eq!(params.classify_link(10.1), LinkStatus::Excellent);
        assert_eq!(params.classify_link(10.0), LinkStatus::Good);
        assert_eq!(params.classify_link(7.0), LinkStatus::Good);
        assert_eq!(params.classify_link(5.0), LinkStatus::Marginal);
        assert_eq!(params.classify_link(2.0), LinkStatus::Marginal);
        assert_eq!(params.classify_link(0.0), LinkStatus::Unreliable);
        assert_eq!(params.classify_link(-5.0), LinkStatus::Unreliable);
    }

    #[test]
    fn test_link_prediction_params_parse_climate() {
        let mut params = LinkPredictionParams::default();

        params.itm_climate = "equatorial".to_string();
        assert_eq!(params.parse_climate(), Climate::Equatorial);

        params.itm_climate = "continental_subtropical".to_string();
        assert_eq!(params.parse_climate(), Climate::ContinentalSubtropical);

        params.itm_climate = "maritime_subtropical".to_string();
        assert_eq!(params.parse_climate(), Climate::MaritimeSubtropical);

        params.itm_climate = "desert".to_string();
        assert_eq!(params.parse_climate(), Climate::Desert);

        params.itm_climate = "continental_temperate".to_string();
        assert_eq!(params.parse_climate(), Climate::ContinentalTemperate);

        params.itm_climate = "maritime_temperate_overland".to_string();
        assert_eq!(params.parse_climate(), Climate::MaritimeTemperateOverLand);

        params.itm_climate = "maritime_temperate_oversea".to_string();
        assert_eq!(params.parse_climate(), Climate::MaritimeTemperateOverSea);

        // Unknown defaults to continental temperate
        params.itm_climate = "unknown".to_string();
        assert_eq!(params.parse_climate(), Climate::ContinentalTemperate);
    }

    #[test]
    fn test_link_prediction_params_parse_polarization() {
        let mut params = LinkPredictionParams::default();

        params.itm_polarization = "horizontal".to_string();
        assert_eq!(params.parse_polarization(), Polarization::Horizontal);

        params.itm_polarization = "vertical".to_string();
        assert_eq!(params.parse_polarization(), Polarization::Vertical);

        // Unknown defaults to vertical
        params.itm_polarization = "unknown".to_string();
        assert_eq!(params.parse_polarization(), Polarization::Vertical);
    }

    #[test]
    fn test_link_prediction_params_from_properties() {
        let props: ResolvedProperties<SimulationScope> = ResolvedProperties::new();
        let params = LinkPredictionParams::from_properties(&props);

        // Verify values match property defaults
        assert_eq!(params.noise_floor_dbm, -120.0);
        assert_eq!(params.snr_thresholds[0], -7.5);  // SF7
        assert_eq!(params.snr_thresholds[5], -20.0); // SF12
        assert_eq!(params.margin_excellent_db, 10.0);
        assert_eq!(params.itm_min_distance_m, 1000.0);
        assert_eq!(params.fspl_min_distance_m, 1.0);
        assert_eq!(params.colocated_path_loss_db, 20.0);
    }
}
