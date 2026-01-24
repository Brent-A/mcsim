//! Integration tests for mcsim-dem with real data files.
//!
//! These tests require the DEM files in dem_data/ to be present.

use mcsim_dem::{DemManager, DemTile};
use std::path::Path;
use std::time::Instant;

const DEM_DATA_DIR: &str = "../../dem_data";

fn dem_data_available() -> bool {
    Path::new(DEM_DATA_DIR).exists()
}

#[test]
fn test_load_single_tile() {
    if !dem_data_available() {
        eprintln!("Skipping test: dem_data directory not found");
        return;
    }

    let tile_path = format!("{}/USGS_13_n48w123_20240327.tif", DEM_DATA_DIR);
    if !Path::new(&tile_path).exists() {
        eprintln!("Skipping test: {} not found", tile_path);
        return;
    }

    let tile = DemTile::from_file(&tile_path).expect("Failed to load tile");

    // Check dimensions - 1/3 arc-second tiles are typically 10812 x 10812
    let (width, height) = tile.dimensions();
    println!("Tile dimensions: {}x{}", width, height);
    assert!(width > 3000, "Tile should have reasonable width");
    assert!(height > 3000, "Tile should have reasonable height");

    // Check bounds - this tile should cover n47-48, w123-122
    let bounds = tile.bounds();
    println!(
        "Tile bounds: lat {:.2} to {:.2}, lon {:.2} to {:.2}",
        bounds.min_lat, bounds.max_lat, bounds.min_lon, bounds.max_lon
    );

    // The bounds should be approximately 1 degree in each direction
    let lat_range = bounds.max_lat - bounds.min_lat;
    let lon_range = bounds.max_lon - bounds.min_lon;
    assert!(
        (lat_range - 1.0).abs() < 0.01,
        "Lat range should be ~1 degree"
    );
    assert!(
        (lon_range - 1.0).abs() < 0.01,
        "Lon range should be ~1 degree"
    );

    // Check resolution
    let (lon_res, _lat_res) = tile.resolution();
    println!(
        "Resolution: {:.6} deg/pixel ({:.2} m/pixel)",
        lon_res,
        lon_res * 111320.0 * bounds.min_lat.to_radians().cos()
    );
    let (lon_m, lat_m) = tile.resolution_meters();
    println!("Resolution in meters: {:.2} x {:.2}", lon_m, lat_m);

    // For 1/3 arc-second data, resolution should be ~10m
    assert!(lat_m < 15.0 && lat_m > 5.0, "Resolution should be ~10m");
}

#[test]
fn test_get_elevation_seattle() {
    if !dem_data_available() {
        eprintln!("Skipping test: dem_data directory not found");
        return;
    }

    let mut manager = DemManager::new();
    let start = Instant::now();
    let count = manager
        .add_directory(DEM_DATA_DIR)
        .expect("Failed to index directory");
    println!("Indexed {} tiles in {:?}", count, start.elapsed());

    if count == 0 {
        eprintln!("Skipping test: no tiles indexed");
        return;
    }

    // Seattle coordinates (downtown)
    let seattle_lat = 47.6062;
    let seattle_lon = -122.3321;

    if !manager.has_tile(seattle_lat, seattle_lon) {
        eprintln!("Skipping test: no tile for Seattle");
        return;
    }

    let start = Instant::now();
    let elevation = manager
        .get_elevation(seattle_lat, seattle_lon)
        .expect("Failed to get elevation");

    println!("Seattle elevation: {} meters (loaded in {:?})", elevation, start.elapsed());

    // Seattle is near sea level but has some hills
    // Downtown is roughly 0-100m elevation
    assert!(
        elevation >= -10.0 && elevation <= 200.0,
        "Seattle elevation should be reasonable: {}",
        elevation
    );
}

#[test]
fn test_get_elevation_mt_rainier_area() {
    if !dem_data_available() {
        eprintln!("Skipping test: dem_data directory not found");
        return;
    }

    let mut manager = DemManager::new();
    let count = manager
        .add_directory(DEM_DATA_DIR)
        .expect("Failed to index directory");

    if count == 0 {
        eprintln!("Skipping test: no tiles indexed");
        return;
    }

    // Check a point that should be in one of our tiles
    // The tiles we have: n47w122, n47w123, n47w124, n48w123, n48w124, n49w122, n49w123, n49w124

    // Try Olympic Peninsula area (should be in n48w124 or n48w123)
    let olympic_lat = 47.8;
    let olympic_lon = -123.5;

    if manager.has_tile(olympic_lat, olympic_lon) {
        let elevation = manager
            .get_elevation(olympic_lat, olympic_lon)
            .expect("Failed to get elevation");
        println!(
            "Olympic Peninsula ({}, {}) elevation: {} meters",
            olympic_lat, olympic_lon, elevation
        );
    }

    // Print coverage info
    if let Some(bounds) = manager.total_bounds() {
        println!(
            "Total coverage: lat {:.2} to {:.2}, lon {:.2} to {:.2}",
            bounds.min_lat, bounds.max_lat, bounds.min_lon, bounds.max_lon
        );
    }
}

#[test]
fn test_sample_line() {
    if !dem_data_available() {
        eprintln!("Skipping test: dem_data directory not found");
        return;
    }

    let mut manager = DemManager::new();
    let count = manager
        .add_directory(DEM_DATA_DIR)
        .expect("Failed to index directory");

    if count == 0 {
        eprintln!("Skipping test: no tiles indexed");
        return;
    }

    // Sample a line within a tile
    let start_lat = 47.5;
    let start_lon = -122.5;
    let end_lat = 47.6;
    let end_lon = -122.4;

    if !manager.has_tile(start_lat, start_lon) || !manager.has_tile(end_lat, end_lon) {
        eprintln!("Skipping test: no tiles for sample line");
        return;
    }

    let samples = manager.sample_line(start_lat, start_lon, end_lat, end_lon, 10);

    println!("Elevation profile:");
    for (distance, elevation) in &samples {
        match elevation {
            Ok(e) => println!("  {:.0}m: {:.1}m", distance, e),
            Err(e) => println!("  {:.0}m: error - {}", distance, e),
        }
    }

    assert_eq!(samples.len(), 10);
    // At least some samples should succeed
    let successful = samples.iter().filter(|(_, e)| e.is_ok()).count();
    assert!(successful > 0, "At least some samples should succeed");
}
