//! DEM tile manager for handling multiple tiles with lazy loading.

use crate::{DemError, DemTile, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Tile key based on the northwest corner of the tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TileKey {
    /// Latitude of the northwest corner (always positive for north, negative for south).
    lat: i32,
    /// Longitude of the northwest corner (always positive for east, negative for west).
    lon: i32,
}

impl TileKey {
    /// Create a tile key for a given coordinate.
    fn from_coord(lat: f64, lon: f64) -> Self {
        // The tile's key is the northwest corner
        // For example, coordinate (47.5, -122.5) is in tile n48w123
        let lat_key = if lat >= 0.0 {
            lat.ceil() as i32
        } else {
            lat.floor() as i32
        };
        let lon_key = if lon >= 0.0 {
            lon.floor() as i32
        } else {
            lon.floor() as i32
        };

        TileKey {
            lat: lat_key,
            lon: lon_key,
        }
    }

    /// Create a tile key from a tile's bounds.
    #[allow(dead_code)]
    fn from_bounds(bounds: &crate::tile::TileBounds) -> Self {
        TileKey {
            lat: bounds.max_lat.round() as i32,
            lon: bounds.min_lon.round() as i32,
        }
    }

    /// Create a tile key from a USGS filename like "USGS_13_n48w123_*.tif".
    fn from_filename(filename: &str) -> Option<Self> {
        // Find the n##w### pattern
        let mut chars = filename.chars().peekable();
        let mut lat_str = String::new();
        let mut lon_str = String::new();
        let mut is_north;
        let mut is_west;

        while let Some(c) = chars.next() {
            if c == 'n' || c == 's' {
                is_north = c == 'n';
                // Read latitude digits
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() {
                        lat_str.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                // Expect 'w' or 'e'
                if let Some(dir) = chars.next() {
                    is_west = dir == 'w';
                    // Read longitude digits
                    while let Some(&d) = chars.peek() {
                        if d.is_ascii_digit() {
                            lon_str.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }

                    if !lat_str.is_empty() && !lon_str.is_empty() {
                        let lat: i32 = lat_str.parse().ok()?;
                        let lon: i32 = lon_str.parse().ok()?;

                        return Some(TileKey {
                            lat: if is_north { lat } else { -lat },
                            lon: if is_west { -(lon as i32) } else { lon },
                        });
                    }
                }
            }
        }

        None
    }
}

/// Manager for multiple DEM tiles with lazy loading.
///
/// The `DemManager` indexes available tiles by scanning a directory, but only
/// loads tile data into memory when needed. This provides fast startup time
/// while still allowing elevation queries across multiple tiles.
///
/// This type is thread-safe and can be shared across threads.
///
/// # Example
///
/// ```no_run
/// use mcsim_dem::DemManager;
///
/// let mut manager = DemManager::new();
/// manager.add_directory("dem_data")?;  // Fast - just indexes files
///
/// // Query elevation - loads only the needed tile
/// let elevation = manager.get_elevation(47.6062, -122.3321)?; // Seattle
/// println!("Seattle elevation: {} meters", elevation);
/// # Ok::<(), mcsim_dem::DemError>(())
/// ```
#[derive(Debug)]
pub struct DemManager {
    /// Available tile files indexed by their northwest corner.
    tile_paths: HashMap<TileKey, PathBuf>,
    /// Cache of loaded tiles (thread-safe for concurrent access).
    cache: RwLock<TileCache>,
    /// Maximum number of tiles to keep in cache.
    max_cache_size: usize,
}

/// LRU cache for loaded tiles.
#[derive(Debug)]
struct TileCache {
    /// Loaded tiles indexed by key.
    tiles: HashMap<TileKey, DemTile>,
    /// Access order for LRU eviction (most recently used at the back).
    access_order: Vec<TileKey>,
}

impl TileCache {
    fn new() -> Self {
        Self { 
            tiles: HashMap::new(),
            access_order: Vec::new(),
        }
    }

    fn get(&self, key: &TileKey) -> Option<&DemTile> {
        self.tiles.get(key)
    }
    
    /// Mark a key as recently used (move to back of access order).
    fn touch(&mut self, key: &TileKey) {
        if let Some(pos) = self.access_order.iter().position(|k| k == key) {
            self.access_order.remove(pos);
            self.access_order.push(*key);
        }
    }

    fn insert(&mut self, key: TileKey, tile: DemTile, max_size: usize) {
        // If already present, just update access order
        if self.tiles.contains_key(&key) {
            self.touch(&key);
            return;
        }
        
        // Evict oldest tiles if at capacity
        while self.tiles.len() >= max_size && !self.access_order.is_empty() {
            let oldest = self.access_order.remove(0);
            self.tiles.remove(&oldest);
        }
        
        // Insert new tile
        self.tiles.insert(key, tile);
        self.access_order.push(key);
    }
    
    fn len(&self) -> usize {
        self.tiles.len()
    }
    
    fn clear(&mut self) {
        self.tiles.clear();
        self.access_order.clear();
    }
}

impl Default for DemManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Default maximum number of tiles to cache.
const DEFAULT_MAX_CACHE_SIZE: usize = 32;

impl DemManager {
    /// Create a new empty DEM manager with default cache size.
    pub fn new() -> Self {
        Self::with_cache_size(DEFAULT_MAX_CACHE_SIZE)
    }
    
    /// Create a new empty DEM manager with a specified cache size.
    pub fn with_cache_size(max_cache_size: usize) -> Self {
        Self {
            tile_paths: HashMap::new(),
            cache: RwLock::new(TileCache::new()),
            max_cache_size,
        }
    }

    /// Add all GeoTIFF files from a directory to the index.
    ///
    /// This is fast because it only scans filenames without loading tile data.
    /// Files must have a `.tif` extension and follow the USGS naming convention.
    ///
    /// Returns the number of tiles indexed.
    pub fn add_directory<P: AsRef<Path>>(&mut self, dir: P) -> Result<usize> {
        let dir = dir.as_ref();
        let mut count = 0;

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "tif") {
                if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                    if let Some(key) = TileKey::from_filename(filename) {
                        self.tile_paths.insert(key, path);
                        count += 1;
                    }
                }
            }
        }

        Ok(count)
    }

    /// Add a single GeoTIFF file to the index.
    ///
    /// This is fast because it only parses the filename without loading tile data.
    pub fn add_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let path = path.as_ref();
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| DemError::InvalidFilename(path.display().to_string()))?;

        let key = TileKey::from_filename(filename)
            .ok_or_else(|| DemError::InvalidFilename(filename.to_string()))?;

        self.tile_paths.insert(key, path.to_path_buf());
        Ok(())
    }

    /// Load all GeoTIFF files from a directory (compatibility method).
    ///
    /// This now uses lazy loading - tiles are indexed but not loaded until needed.
    pub fn load_directory<P: AsRef<Path>>(&mut self, dir: P) -> Result<usize> {
        self.add_directory(dir)
    }

    /// Load a single GeoTIFF file (compatibility method).
    ///
    /// This now uses lazy loading - tile is indexed but not loaded until needed.
    pub fn load_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        self.add_file(path)
    }

    /// Ensure a tile is loaded and return a reference to it.
    fn ensure_tile_loaded(&self, key: TileKey) -> Result<()> {
        // Check if already loaded (read lock)
        {
            let cache = self.cache.read().map_err(|_| DemError::CacheLockPoisoned)?;
            if cache.get(&key).is_some() {
                return Ok(());
            }
        }

        // Get the path for this tile
        let path = self
            .tile_paths
            .get(&key)
            .ok_or(DemError::NoTileFound {
                lat: key.lat as f64,
                lon: key.lon as f64,
            })?
            .clone();

        // Load the tile
        let tile = DemTile::from_file(&path)?;
        
        // Acquire write lock and insert
        let mut cache = self.cache.write().map_err(|_| DemError::CacheLockPoisoned)?;
        cache.insert(key, tile, self.max_cache_size);

        Ok(())
    }

    /// Get the elevation at a geographic coordinate.
    ///
    /// Loads the required tile on demand if not already cached.
    ///
    /// # Arguments
    /// * `lat` - Latitude in decimal degrees (positive = north)
    /// * `lon` - Longitude in decimal degrees (negative = west)
    ///
    /// # Returns
    /// Elevation in meters, or an error if no tile covers the coordinate.
    pub fn get_elevation(&self, lat: f64, lon: f64) -> Result<f32> {
        let key = TileKey::from_coord(lat, lon);
        self.ensure_tile_loaded(key)?;

        let cache = self.cache.read().map_err(|_| DemError::CacheLockPoisoned)?;
        let tile = cache.get(&key).ok_or(DemError::NoTileFound { lat, lon })?;
        tile.get_elevation(lat, lon)
    }

    /// Get the elevation at a geographic coordinate using nearest-neighbor sampling.
    ///
    /// Loads the required tile on demand if not already cached.
    pub fn get_elevation_nearest(&self, lat: f64, lon: f64) -> Result<f32> {
        let key = TileKey::from_coord(lat, lon);
        self.ensure_tile_loaded(key)?;

        let cache = self.cache.read().map_err(|_| DemError::CacheLockPoisoned)?;
        let tile = cache.get(&key).ok_or(DemError::NoTileFound { lat, lon })?;
        tile.get_elevation_nearest(lat, lon)
    }

    /// Check if a tile is available (indexed) for the given coordinate.
    ///
    /// Note: This does not check if the tile is currently loaded in memory.
    pub fn has_tile(&self, lat: f64, lon: f64) -> bool {
        let key = TileKey::from_coord(lat, lon);
        self.tile_paths.contains_key(&key)
    }

    /// Check if a tile is currently loaded in memory.
    pub fn is_tile_loaded(&self, lat: f64, lon: f64) -> bool {
        let key = TileKey::from_coord(lat, lon);
        self.cache.read().map(|c| c.get(&key).is_some()).unwrap_or(false)
    }

    /// Get the number of indexed tiles (available but not necessarily loaded).
    pub fn tile_count(&self) -> usize {
        self.tile_paths.len()
    }

    /// Get the number of currently loaded tiles in memory.
    pub fn loaded_tile_count(&self) -> usize {
        self.cache.read().map(|c| c.len()).unwrap_or(0)
    }

    /// Preload a specific tile into memory.
    ///
    /// Useful when you know you'll need a tile and want to control when loading happens.
    pub fn preload_tile(&self, lat: f64, lon: f64) -> Result<()> {
        let key = TileKey::from_coord(lat, lon);
        self.ensure_tile_loaded(key)
    }

    /// Clear all loaded tiles from memory.
    ///
    /// The tiles remain indexed and can be reloaded on demand.
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
    }

    /// Get the bounding box that covers all indexed tiles.
    ///
    /// This works without loading any tiles since it uses the indexed keys.
    pub fn total_bounds(&self) -> Option<crate::tile::TileBounds> {
        let mut keys = self.tile_paths.keys();
        let first = keys.next()?;

        let mut min_lat = first.lat - 1;
        let mut max_lat = first.lat;
        let mut min_lon = first.lon;
        let mut max_lon = first.lon + 1;

        for key in keys {
            min_lat = min_lat.min(key.lat - 1);
            max_lat = max_lat.max(key.lat);
            min_lon = min_lon.min(key.lon);
            max_lon = max_lon.max(key.lon + 1);
        }

        Some(crate::tile::TileBounds {
            min_lat: min_lat as f64,
            max_lat: max_lat as f64,
            min_lon: min_lon as f64,
            max_lon: max_lon as f64,
        })
    }

    /// Sample elevations along a line between two points.
    ///
    /// Returns a vector of (distance, elevation) pairs, where distance is
    /// measured from the start point in meters.
    ///
    /// Note: This may load multiple tiles if the line crosses tile boundaries.
    ///
    /// # Arguments
    /// * `start_lat`, `start_lon` - Starting point
    /// * `end_lat`, `end_lon` - Ending point
    /// * `num_samples` - Number of samples along the line
    pub fn sample_line(
        &self,
        start_lat: f64,
        start_lon: f64,
        end_lat: f64,
        end_lon: f64,
        num_samples: usize,
    ) -> Vec<(f64, Result<f32>)> {
        let mut results = Vec::with_capacity(num_samples);

        // Calculate total distance using haversine formula
        let total_distance = haversine_distance(start_lat, start_lon, end_lat, end_lon);

        for i in 0..num_samples {
            let t = i as f64 / (num_samples - 1) as f64;
            let lat = start_lat + t * (end_lat - start_lat);
            let lon = start_lon + t * (end_lon - start_lon);
            let distance = t * total_distance;

            let elevation = self.get_elevation(lat, lon);
            results.push((distance, elevation));
        }

        results
    }
}

/// Calculate the distance between two points using the haversine formula.
///
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_key_from_coord() {
        // Seattle: 47.6062, -122.3321 should be in tile n48w123
        let key = TileKey::from_coord(47.6062, -122.3321);
        assert_eq!(key.lat, 48);
        assert_eq!(key.lon, -123);

        // At the tile corner
        let key = TileKey::from_coord(48.0, -123.0);
        assert_eq!(key.lat, 48);
        assert_eq!(key.lon, -123);

        // Just south of the corner
        let key = TileKey::from_coord(47.9999, -122.0001);
        assert_eq!(key.lat, 48);
        assert_eq!(key.lon, -123);
    }

    #[test]
    fn test_tile_key_from_filename() {
        let key = TileKey::from_filename("USGS_13_n48w123_20240327.tif").unwrap();
        assert_eq!(key.lat, 48);
        assert_eq!(key.lon, -123);

        let key = TileKey::from_filename("USGS_13_n47w122_20250813.tif").unwrap();
        assert_eq!(key.lat, 47);
        assert_eq!(key.lon, -122);

        // Invalid filename
        assert!(TileKey::from_filename("invalid.tif").is_none());
    }

    #[test]
    fn test_haversine_distance() {
        // Seattle to Portland is approximately 233 km
        let dist = haversine_distance(47.6062, -122.3321, 45.5152, -122.6784);
        assert!((dist - 233_000.0).abs() < 5_000.0); // Within 5 km
    }
}
