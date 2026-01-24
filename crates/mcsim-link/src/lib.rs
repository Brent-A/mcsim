//! # mcsim-link
//!
//! Link estimation and prediction using Digital Elevation Models (DEM) and
//! the Irregular Terrain Model (ITM).
//!
//! This crate provides tools for predicting radio link quality between
//! geographic coordinates, taking into account terrain effects.
//!
//! ## Features
//!
//! - **Link Prediction**: Predict link quality using terrain data and ITM propagation model
//! - **SNR Estimation**: Estimate true SNR distribution from observed (truncated) measurements
//! - **Property-Based Configuration**: Load parameters from simulation properties

mod estimate;
mod predict;

pub use estimate::{
    estimate_snr, estimate_snr_with_config, estimate_snr_with_threshold,
    LoraModulationParams, LoraPhyConfig, SnrEstimationError, SnrEstimationResult,
};
pub use predict::{
    // Legacy DEM-based functions
    load_dem, load_itm, predict_link, predict_link_with_params,
    // New elevation source abstraction
    ElevationSource, load_aws_elevation, load_aws_elevation_with_callback,
    predict_link_with_elevation, predict_link_with_elevation_and_params,
    // Types
    LinkPrediction, LinkPredictionConfig, LinkPredictionError, LinkPredictionParams,
    LinkStatus, PathInfo, PredictionMethod, RadioParams, TerrainInfo,
    ITM_MIN_DISTANCE_M, FSPL_MIN_DISTANCE_M, COLOCATED_PATH_LOSS_DB,
};

// Re-export download stats from mcsim-dem
pub use mcsim_dem::DownloadStats;
