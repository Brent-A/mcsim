//! AWS Terrain Tiles fetcher with local caching.
//!
//! This module provides functionality to fetch 512x512 GeoTIFF elevation tiles
//! from the AWS Open Data S3 bucket and cache them locally.
//!
//! Source: https://s3.amazonaws.com/elevation-tiles-prod/geotiff/{z}/{x}/{y}.tif
//!
//! ## Tile Coordinate System
//!
//! Uses the OpenStreetMap Slippy Map tile naming convention:
//! - `z` is the zoom level (1-14, default 12)
//! - `x` is the column (0 to 2^z - 1, from west to east)
//! - `y` is the row (0 to 2^z - 1, from north to south)
//!
//! At zoom level 12:
//! - 4096 x 4096 tiles globally
//! - Each tile covers ~0.088° (about 9.8 km at equator)
//! - Resolution is approximately 38m per pixel
//!
//! ## Thread Safety
//!
//! The `AwsTileFetcher` supports concurrent access from multiple threads:
//! - Different tiles can be downloaded in parallel
//! - Multiple threads requesting the same tile will coordinate, with only one
//!   performing the download while others wait
//! - Cached tiles are served immediately without blocking

use crate::{DemError, DemTile, Result};
use crate::tile::TileBounds;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, RwLock};

/// Default maximum number of tiles to cache in memory.
/// Each tile is 512x512 pixels at ~4 bytes per pixel = ~1MB per tile.
/// 1024 tiles = ~1GB memory usage at maximum.
const DEFAULT_TILE_CACHE_SIZE: usize = 1024;

/// AWS S3 base URL for elevation tiles.
const AWS_TILE_BASE_URL: &str = "https://s3.amazonaws.com/elevation-tiles-prod/geotiff";

/// Minimum valid zoom level.
pub const MIN_ZOOM: u8 = 1;

/// Maximum valid zoom level for AWS elevation tiles.
pub const MAX_ZOOM: u8 = 14;

/// Default zoom level (good balance of detail and tile count).
pub const DEFAULT_ZOOM: u8 = 12;

/// OSM-style tile coordinates (z, x, y).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileCoord {
    /// Zoom level (1-14).
    pub z: u8,
    /// X coordinate (column, 0 at 180°W, increases eastward).
    pub x: u32,
    /// Y coordinate (row, 0 at ~85.05°N, increases southward).
    pub y: u32,
}

impl TileCoord {
    /// Create a new tile coordinate.
    ///
    /// # Panics
    /// Panics if coordinates are out of range for the zoom level.
    pub fn new(z: u8, x: u32, y: u32) -> Self {
        let max_coord = 1u32 << z;
        assert!(x < max_coord, "x={} out of range for zoom {}", x, z);
        assert!(y < max_coord, "y={} out of range for zoom {}", y, z);
        Self { z, x, y }
    }

    /// Convert latitude/longitude to tile coordinates.
    ///
    /// Uses the OpenStreetMap Slippy Map tiling formula:
    /// - x = floor((lon + 180) / 360 * 2^z)
    /// - y = floor((1 - ln(tan(lat) + sec(lat)) / π) / 2 * 2^z)
    ///
    /// # Arguments
    /// * `lat` - Latitude in degrees (-85.05 to 85.05)
    /// * `lon` - Longitude in degrees (-180 to 180)
    /// * `z` - Zoom level (1-14)
    ///
    /// # Returns
    /// Tile coordinates, or error if inputs are invalid.
    pub fn from_lat_lon(lat: f64, lon: f64, z: u8) -> Result<Self> {
        if z < MIN_ZOOM || z > MAX_ZOOM {
            return Err(DemError::InvalidZoomLevel(z));
        }

        // Clamp latitude to valid range for Web Mercator
        // The exact limit is ±85.0511287798° (arctan(sinh(π)))
        let lat_clamped = lat.clamp(-85.0511, 85.0511);

        let n = (1u32 << z) as f64;

        // OSM Slippy Map formula for x
        let x = ((lon + 180.0) / 360.0 * n).floor() as u32;

        // OSM Slippy Map formula for y
        let lat_rad = lat_clamped.to_radians();
        let y = ((1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / PI) / 2.0 * n).floor() as u32;

        // Clamp to valid range (handles edge cases at exactly ±180°)
        let max_coord = (1u32 << z) - 1;
        let x = x.min(max_coord);
        let y = y.min(max_coord);

        Ok(Self { z, x, y })
    }

    /// Get the bounding box for this tile.
    ///
    /// Returns (min_lat, max_lat, min_lon, max_lon).
    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        let n = (1u32 << self.z) as f64;

        // Longitude bounds
        let min_lon = self.x as f64 / n * 360.0 - 180.0;
        let max_lon = (self.x + 1) as f64 / n * 360.0 - 180.0;

        // Latitude bounds (inverse of Slippy Map formula)
        let max_lat = (PI * (1.0 - 2.0 * self.y as f64 / n)).sinh().atan().to_degrees();
        let min_lat = (PI * (1.0 - 2.0 * (self.y + 1) as f64 / n)).sinh().atan().to_degrees();

        (min_lat, max_lat, min_lon, max_lon)
    }

    /// Get the cache file path for this tile.
    pub fn cache_path(&self, cache_dir: &Path) -> PathBuf {
        cache_dir
            .join(self.z.to_string())
            .join(self.x.to_string())
            .join(format!("{}.tif", self.y))
    }

    /// Get the AWS S3 URL for this tile.
    pub fn aws_url(&self) -> String {
        format!("{}/{}/{}/{}.tif", AWS_TILE_BASE_URL, self.z, self.x, self.y)
    }
}

/// Callback for tile download progress.
pub type DownloadCallback = Box<dyn Fn(&str) + Send + Sync>;

/// Status of a tile download in progress.
#[derive(Clone)]
enum DownloadStatus {
    /// Download is in progress.
    InProgress,
    /// Download completed successfully.
    Complete,
    /// Download failed with an error message.
    Failed(String),
}

/// Tracks in-flight downloads to prevent duplicate requests.
struct DownloadTracker {
    /// Map of tile coordinates to their download status.
    /// Only contains entries for tiles currently being downloaded.
    in_flight: HashMap<TileCoord, DownloadStatus>,
}

impl DownloadTracker {
    fn new() -> Self {
        Self {
            in_flight: HashMap::new(),
        }
    }
}

/// LRU cache for loaded tiles in memory.
struct TileCache {
    /// Loaded tiles indexed by coordinate.
    tiles: HashMap<TileCoord, DemTile>,
    /// Access order for LRU eviction (most recently used at the back).
    access_order: Vec<TileCoord>,
    /// Maximum number of tiles to cache.
    max_size: usize,
}

impl TileCache {
    fn new(max_size: usize) -> Self {
        Self {
            tiles: HashMap::new(),
            access_order: Vec::new(),
            max_size,
        }
    }

    fn get(&mut self, coord: &TileCoord) -> Option<&DemTile> {
        if self.tiles.contains_key(coord) {
            // Move to back (most recently used)
            if let Some(pos) = self.access_order.iter().position(|k| k == coord) {
                self.access_order.remove(pos);
                self.access_order.push(*coord);
            }
            self.tiles.get(coord)
        } else {
            None
        }
    }

    fn insert(&mut self, coord: TileCoord, tile: DemTile) {
        // If already present, just update access order
        if self.tiles.contains_key(&coord) {
            if let Some(pos) = self.access_order.iter().position(|k| k == &coord) {
                self.access_order.remove(pos);
                self.access_order.push(coord);
            }
            return;
        }

        // Evict oldest tiles if at capacity
        while self.tiles.len() >= self.max_size && !self.access_order.is_empty() {
            let oldest = self.access_order.remove(0);
            self.tiles.remove(&oldest);
        }

        // Insert new tile
        self.tiles.insert(coord, tile);
        self.access_order.push(coord);
    }
}

/// Download statistics for the fetcher.
#[derive(Debug, Clone, Copy, Default)]
pub struct DownloadStats {
    /// Number of tiles downloaded this session.
    pub tiles_downloaded: usize,
    /// Total bytes downloaded this session.
    pub bytes_downloaded: u64,
}

/// AWS elevation tile fetcher with local caching.
///
/// This fetcher supports concurrent access from multiple threads:
/// - Different tiles can be downloaded in parallel
/// - Multiple threads requesting the same tile will coordinate
/// - Cached tiles are served immediately without blocking
/// - Loaded tiles are cached in memory for fast repeated queries
pub struct AwsTileFetcher {
    /// Cache directory for downloaded tiles.
    cache_dir: PathBuf,
    /// Default zoom level.
    zoom: u8,
    /// HTTP client for downloading tiles.
    client: reqwest::blocking::Client,
    /// Tracks which tiles are currently being downloaded.
    download_tracker: Mutex<DownloadTracker>,
    /// Condition variable for waiting on downloads.
    download_complete: Condvar,
    /// In-memory cache of loaded tiles (thread-safe).
    tile_cache: RwLock<TileCache>,
    /// Number of tiles downloaded this session (atomic for thread safety).
    tiles_downloaded: AtomicUsize,
    /// Total bytes downloaded this session (atomic for thread safety).
    bytes_downloaded: AtomicU64,
}

impl std::fmt::Debug for AwsTileFetcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwsTileFetcher")
            .field("cache_dir", &self.cache_dir)
            .field("zoom", &self.zoom)
            .finish()
    }
}

impl AwsTileFetcher {
    /// Create a new fetcher with the default zoom level.
    pub fn new<P: AsRef<Path>>(cache_dir: P) -> Result<Self> {
        Self::with_zoom(cache_dir, DEFAULT_ZOOM)
    }

    /// Create a new fetcher with a specified zoom level.
    pub fn with_zoom<P: AsRef<Path>>(cache_dir: P, zoom: u8) -> Result<Self> {
        if zoom < MIN_ZOOM || zoom > MAX_ZOOM {
            return Err(DemError::InvalidZoomLevel(zoom));
        }

        let cache_dir = cache_dir.as_ref().to_path_buf();

        // Create cache directory if it doesn't exist
        fs::create_dir_all(&cache_dir)?;

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;

        Ok(Self {
            cache_dir,
            zoom,
            client,
            download_tracker: Mutex::new(DownloadTracker::new()),
            download_complete: Condvar::new(),
            tile_cache: RwLock::new(TileCache::new(DEFAULT_TILE_CACHE_SIZE)),
            tiles_downloaded: AtomicUsize::new(0),
            bytes_downloaded: AtomicU64::new(0),
        })
    }

    /// Get the zoom level.
    pub fn zoom(&self) -> u8 {
        self.zoom
    }

    /// Get download statistics for this session.
    pub fn download_stats(&self) -> DownloadStats {
        DownloadStats {
            tiles_downloaded: self.tiles_downloaded.load(Ordering::Relaxed),
            bytes_downloaded: self.bytes_downloaded.load(Ordering::Relaxed),
        }
    }

    /// Reset download statistics.
    pub fn reset_download_stats(&self) {
        self.tiles_downloaded.store(0, Ordering::Relaxed);
        self.bytes_downloaded.store(0, Ordering::Relaxed);
    }

    /// Set the zoom level.
    pub fn set_zoom(&mut self, zoom: u8) -> Result<()> {
        if zoom < MIN_ZOOM || zoom > MAX_ZOOM {
            return Err(DemError::InvalidZoomLevel(zoom));
        }
        self.zoom = zoom;
        Ok(())
    }

    /// Get the cache directory.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Get tile coordinates for a lat/lon.
    pub fn tile_for_coord(&self, lat: f64, lon: f64) -> Result<TileCoord> {
        TileCoord::from_lat_lon(lat, lon, self.zoom)
    }

    /// Check if a tile is cached locally.
    pub fn is_cached(&self, coord: &TileCoord) -> bool {
        coord.cache_path(&self.cache_dir).exists()
    }

    /// Get the cache path for a coordinate.
    pub fn cache_path_for_coord(&self, lat: f64, lon: f64) -> Result<PathBuf> {
        let coord = self.tile_for_coord(lat, lon)?;
        Ok(coord.cache_path(&self.cache_dir))
    }

    /// Fetch a tile, using cache if available.
    ///
    /// Returns the path to the local tile file.
    pub fn fetch_tile(&self, coord: &TileCoord) -> Result<PathBuf> {
        self.fetch_tile_with_callback(coord, None)
    }

    /// Fetch a tile with an optional progress callback.
    ///
    /// This method is thread-safe. If multiple threads request the same tile:
    /// - The first thread will perform the download
    /// - Other threads will wait for the download to complete
    /// - Different tiles can be downloaded concurrently
    ///
    /// The callback is called with status messages for user feedback during downloads.
    pub fn fetch_tile_with_callback(
        &self,
        coord: &TileCoord,
        callback: Option<&DownloadCallback>,
    ) -> Result<PathBuf> {
        let cache_path = coord.cache_path(&self.cache_dir);

        // Fast path: check cache first (no locking needed for file existence check)
        if cache_path.exists() {
            return Ok(cache_path);
        }

        // Check if another thread is already downloading this tile
        loop {
            let mut tracker = self.download_tracker.lock().unwrap();
            
            match tracker.in_flight.get(coord) {
                Some(DownloadStatus::InProgress) => {
                    // Another thread is downloading this tile, wait for it
                    tracker = self.download_complete.wait(tracker).unwrap();
                    // After waking, check again (loop continues)
                    continue;
                }
                Some(DownloadStatus::Complete) => {
                    // Download completed by another thread
                    tracker.in_flight.remove(coord);
                    return Ok(cache_path);
                }
                Some(DownloadStatus::Failed(err)) => {
                    // Previous download failed, try again
                    let err_msg = err.clone();
                    tracker.in_flight.remove(coord);
                    return Err(DemError::TileDownloadFailed {
                        z: coord.z,
                        x: coord.x,
                        y: coord.y,
                        reason: err_msg,
                    });
                }
                None => {
                    // Check cache again (might have been downloaded while we waited)
                    if cache_path.exists() {
                        return Ok(cache_path);
                    }
                    // Mark as in-progress and proceed to download
                    tracker.in_flight.insert(*coord, DownloadStatus::InProgress);
                    break;
                }
            }
        }

        // We are responsible for downloading this tile
        // Note: Other threads can now download different tiles concurrently
        let result = self.download_tile_internal(coord, &cache_path, callback);

        // Update status and notify waiting threads
        {
            let mut tracker = self.download_tracker.lock().unwrap();
            match &result {
                Ok(_) => {
                    tracker.in_flight.insert(*coord, DownloadStatus::Complete);
                }
                Err(e) => {
                    tracker.in_flight.insert(*coord, DownloadStatus::Failed(e.to_string()));
                }
            }
        }
        // Notify all waiting threads
        self.download_complete.notify_all();

        result
    }

    /// Internal method to perform the actual tile download.
    fn download_tile_internal(
        &self,
        coord: &TileCoord,
        cache_path: &Path,
        _callback: Option<&DownloadCallback>,
    ) -> Result<PathBuf> {
        // Create parent directories
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Download from AWS
        let url = coord.aws_url();

        let response = self.client.get(&url).send()?;

        if !response.status().is_success() {
            return Err(DemError::TileDownloadFailed {
                z: coord.z,
                x: coord.x,
                y: coord.y,
                reason: format!("HTTP {}", response.status()),
            });
        }

        let bytes = response.bytes()?;
        
        // Update download statistics
        self.tiles_downloaded.fetch_add(1, Ordering::Relaxed);
        self.bytes_downloaded.fetch_add(bytes.len() as u64, Ordering::Relaxed);

        // Write to cache file
        let mut file = fs::File::create(cache_path)?;
        file.write_all(&bytes)?;

        Ok(cache_path.to_path_buf())
    }

    /// Get elevation at a coordinate, fetching the tile if needed.
    pub fn get_elevation(&self, lat: f64, lon: f64) -> Result<f32> {
        self.get_elevation_with_callback(lat, lon, None)
    }

    /// Get elevation at a coordinate with an optional progress callback.
    ///
    /// This method uses an in-memory cache to avoid repeatedly loading tiles
    /// from disk. The cache uses LRU eviction when full.
    pub fn get_elevation_with_callback(
        &self,
        lat: f64,
        lon: f64,
        callback: Option<&DownloadCallback>,
    ) -> Result<f32> {
        let coord = self.tile_for_coord(lat, lon)?;
        
        // Fast path: check if tile is already in memory cache
        {
            let mut cache = self.tile_cache.write().unwrap();
            if let Some(tile) = cache.get(&coord) {
                return tile.get_elevation(lat, lon);
            }
        }
        
        // Tile not in memory - ensure it's downloaded/cached on disk
        let tile_path = self.fetch_tile_with_callback(&coord, callback)?;
        
        // Get the tile bounds from the coordinate
        let (min_lat, max_lat, min_lon, max_lon) = coord.bounds();
        let bounds = TileBounds {
            min_lat,
            max_lat,
            min_lon,
            max_lon,
        };
        
        // Load tile from disk
        let tile = DemTile::from_file_with_bounds(&tile_path, bounds)?;
        let elevation = tile.get_elevation(lat, lon)?;
        
        // Cache the loaded tile in memory
        {
            let mut cache = self.tile_cache.write().unwrap();
            cache.insert(coord, tile);
        }
        
        Ok(elevation)
    }

    /// Prefetch tiles for a bounding box.
    ///
    /// Returns the number of tiles fetched (not including already cached).
    pub fn prefetch_region(
        &self,
        min_lat: f64,
        max_lat: f64,
        min_lon: f64,
        max_lon: f64,
        callback: Option<&DownloadCallback>,
    ) -> Result<usize> {
        // Get corner tiles
        let tl = self.tile_for_coord(max_lat, min_lon)?;
        let br = self.tile_for_coord(min_lat, max_lon)?;

        let mut fetched = 0;
        let total = ((br.x - tl.x + 1) * (br.y - tl.y + 1)) as usize;

        if let Some(cb) = callback {
            cb(&format!("Prefetching {} tiles for region...", total));
        }

        for x in tl.x..=br.x {
            for y in tl.y..=br.y {
                let coord = TileCoord::new(self.zoom, x, y);
                if !self.is_cached(&coord) {
                    self.fetch_tile_with_callback(&coord, callback)?;
                    fetched += 1;
                }
            }
        }

        if let Some(cb) = callback {
            cb(&format!(
                "Prefetch complete: {} new tiles downloaded",
                fetched
            ));
        }

        Ok(fetched)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_coord_from_lat_lon() {
        // Seattle: 47.6062, -122.3321
        // Verify the coordinate calculation using OSM formula
        let coord = TileCoord::from_lat_lon(47.6062, -122.3321, 12).unwrap();
        assert_eq!(coord.z, 12);
        // Verify tile contains the point
        let (min_lat, max_lat, min_lon, max_lon) = coord.bounds();
        assert!(47.6062 >= min_lat && 47.6062 <= max_lat);
        assert!(-122.3321 >= min_lon && -122.3321 <= max_lon);
    }

    #[test]
    fn test_tile_coord_equator() {
        // Equator at prime meridian
        let coord = TileCoord::from_lat_lon(0.0, 0.0, 12).unwrap();
        assert_eq!(coord.z, 12);
        // At zoom 12, x=2048 is the tile just east of the prime meridian
        assert_eq!(coord.x, 2048);
        // y=2048 is at the equator
        assert_eq!(coord.y, 2048);
    }

    #[test]
    fn test_tile_coord_bounds() {
        // Get the tile for Seattle and verify it contains Seattle
        let coord = TileCoord::from_lat_lon(47.6062, -122.3321, 12).unwrap();
        let (min_lat, max_lat, min_lon, max_lon) = coord.bounds();

        // Check that Seattle is within the bounds
        assert!(
            47.6062 >= min_lat && 47.6062 <= max_lat,
            "Seattle lat {} not in [{}, {}]",
            47.6062,
            min_lat,
            max_lat
        );
        assert!(
            -122.3321 >= min_lon && -122.3321 <= max_lon,
            "Seattle lon {} not in [{}, {}]",
            -122.3321,
            min_lon,
            max_lon
        );
    }

    #[test]
    fn test_tile_coord_roundtrip() {
        // Test that a coordinate maps to a tile that contains it
        let test_points = [
            (47.6062, -122.3321), // Seattle
            (40.7128, -74.0060),  // New York
            (51.5074, -0.1278),   // London
            (-33.8688, 151.2093), // Sydney
            (0.0, 0.0),           // Null Island
        ];

        for (lat, lon) in test_points {
            let coord = TileCoord::from_lat_lon(lat, lon, 12).unwrap();
            let (min_lat, max_lat, min_lon, max_lon) = coord.bounds();

            assert!(
                lat >= min_lat && lat <= max_lat,
                "lat {} not in [{}, {}] for tile {:?}",
                lat,
                min_lat,
                max_lat,
                coord
            );
            assert!(
                lon >= min_lon && lon <= max_lon,
                "lon {} not in [{}, {}] for tile {:?}",
                lon,
                min_lon,
                max_lon,
                coord
            );
        }
    }

    #[test]
    fn test_tile_url() {
        let coord = TileCoord::new(12, 655, 1407);
        assert_eq!(
            coord.aws_url(),
            "https://s3.amazonaws.com/elevation-tiles-prod/geotiff/12/655/1407.tif"
        );
    }

    #[test]
    fn test_cache_path() {
        let coord = TileCoord::new(12, 655, 1407);
        let path = coord.cache_path(Path::new("./elevation_cache"));
        assert_eq!(
            path,
            PathBuf::from("./elevation_cache/12/655/1407.tif")
        );
    }

    #[test]
    fn test_invalid_zoom() {
        assert!(TileCoord::from_lat_lon(0.0, 0.0, 0).is_err());
        assert!(TileCoord::from_lat_lon(0.0, 0.0, 15).is_err());
    }
}
