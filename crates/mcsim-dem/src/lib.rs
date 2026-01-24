//! # mcsim-dem
//!
//! Digital Elevation Model (DEM) reader for USGS GeoTIFF files and AWS terrain tiles.
//!
//! This crate provides functionality to read elevation data from:
//! - USGS 3DEP (3D Elevation Program) GeoTIFF tiles (local files)
//! - AWS Open Data terrain tiles (fetched on demand and cached locally)
//!
//! ## Overview
//!
//! ### USGS 3DEP Tiles
//!
//! USGS 3DEP data provides elevation information at various resolutions:
//! - 1/3 arc-second (~10 meters) - Most common for the contiguous US
//! - 1 arc-second (~30 meters)
//! - 2 arc-second (~60 meters) - Alaska
//!
//! The tiles are organized by 1x1 degree cells, named like `USGS_13_n48w123_*.tif`
//! where `n48w123` means the tile covers latitude 48°N to 49°N and longitude 123°W to 122°W.
//!
//! ### AWS Terrain Tiles
//!
//! AWS provides global 512x512 GeoTIFF tiles at various zoom levels:
//! - Zoom 12 (~38m resolution at equator) is recommended
//! - Tiles are fetched from: `https://s3.amazonaws.com/elevation-tiles-prod/geotiff/{z}/{x}/{y}.tif`
//! - Uses OpenStreetMap Slippy Map tiling convention
//!
//! ## Examples
//!
//! ### Using Local USGS Tiles
//!
//! ```no_run
//! use mcsim_dem::{DemTile, DemManager};
//!
//! // Use the manager for multiple tiles with lazy loading
//! let mut manager = DemManager::new();
//! manager.add_directory("dem_data")?;  // Fast - just indexes files
//!
//! // Query elevation - loads the needed tile on demand
//! let elevation = manager.get_elevation(48.5, -122.5)?;
//! println!("Elevation: {} meters", elevation);
//!
//! // Or load a single tile directly
//! let tile = DemTile::from_file("dem_data/USGS_13_n48w123_20240327.tif")?;
//! let elevation = tile.get_elevation(47.5, -122.5)?;
//! # Ok::<(), mcsim_dem::DemError>(())
//! ```
//!
//! ### Using AWS Terrain Tiles
//!
//! ```no_run
//! use mcsim_dem::{AwsTileFetcher, TileCoord, DownloadCallback};
//!
//! // Create a fetcher with local cache
//! let fetcher = AwsTileFetcher::new("./elevation_cache")?;
//!
//! // Get elevation (fetches tile if not cached)
//! let elevation = fetcher.get_elevation(47.6062, -122.3321)?;
//! println!("Seattle elevation: {} meters", elevation);
//!
//! // With progress callback for user feedback
//! let callback: DownloadCallback = Box::new(|msg: &str| println!("{}", msg));
//! let elevation = fetcher.get_elevation_with_callback(
//!     47.6062, -122.3321,
//!     Some(&callback)
//! )?;
//! # Ok::<(), mcsim_dem::DemError>(())
//! ```

mod aws_tiles;
mod error;
mod manager;
mod tile;

pub use aws_tiles::{AwsTileFetcher, DownloadCallback, DownloadStats, TileCoord, DEFAULT_ZOOM, MAX_ZOOM, MIN_ZOOM};
pub use error::DemError;
pub use manager::DemManager;
pub use tile::DemTile;

/// Result type for DEM operations.
pub type Result<T> = std::result::Result<T, DemError>;
