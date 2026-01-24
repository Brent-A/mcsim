//! Error types for the DEM crate.

use thiserror::Error;

/// Errors that can occur when working with DEM data.
#[derive(Debug, Error)]
pub enum DemError {
    /// I/O error reading a file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// TIFF decoding error.
    #[error("TIFF decode error: {0}")]
    TiffDecode(#[from] tiff::TiffError),

    /// Invalid GeoTIFF - missing required tags.
    #[error("Invalid GeoTIFF: {0}")]
    InvalidGeoTiff(String),

    /// Coordinate is outside the bounds of the tile.
    #[error("Coordinate ({lat}, {lon}) is outside tile bounds ({min_lat}-{max_lat}, {min_lon}-{max_lon})")]
    OutOfBounds {
        /// Requested latitude.
        lat: f64,
        /// Requested longitude.
        lon: f64,
        /// Tile minimum latitude.
        min_lat: f64,
        /// Tile maximum latitude.
        max_lat: f64,
        /// Tile minimum longitude.
        min_lon: f64,
        /// Tile maximum longitude.
        max_lon: f64,
    },

    /// No tile found for the given coordinate.
    #[error("No tile found for coordinate ({lat}, {lon})")]
    NoTileFound {
        /// Requested latitude.
        lat: f64,
        /// Requested longitude.
        lon: f64,
    },

    /// Invalid tile filename - cannot parse coordinates.
    #[error("Invalid tile filename: {0}")]
    InvalidFilename(String),

    /// Cache lock was poisoned (a thread panicked while holding the lock).
    #[error("Tile cache lock was poisoned")]
    CacheLockPoisoned,

    /// Unsupported data type in the TIFF file.
    #[error("Unsupported TIFF data type: {0}")]
    UnsupportedDataType(String),

    /// No data value encountered.
    #[error("No elevation data at coordinate ({lat}, {lon})")]
    NoData {
        /// Requested latitude.
        lat: f64,
        /// Requested longitude.
        lon: f64,
    },

    /// HTTP request error when fetching tiles.
    #[error("HTTP request error: {0}")]
    HttpRequest(#[from] reqwest::Error),

    /// Failed to download tile from remote server.
    #[error("Failed to download tile z={z} x={x} y={y}: {reason}")]
    TileDownloadFailed {
        /// Zoom level.
        z: u8,
        /// X tile coordinate.
        x: u32,
        /// Y tile coordinate.
        y: u32,
        /// Reason for failure.
        reason: String,
    },

    /// Invalid zoom level.
    #[error("Invalid zoom level {0} (must be 1-14)")]
    InvalidZoomLevel(u8),
}
