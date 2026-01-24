//! Example: Query elevation from DEM files.
//!
//! Usage: cargo run --example query_elevation -- <lat> <lon> [dem_dir]

use mcsim_dem::DemManager;
use std::env;
use std::time::Instant;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <lat> <lon> [dem_dir]", args[0]);
        eprintln!("Example: {} 47.6062 -122.3321 ./dem_data", args[0]);
        std::process::exit(1);
    }

    let lat: f64 = args[1].parse().expect("Invalid latitude");
    let lon: f64 = args[2].parse().expect("Invalid longitude");
    let dem_dir = args.get(3).map(|s| s.as_str()).unwrap_or("dem_data");

    println!("Indexing DEM tiles from {}...", dem_dir);
    let start = Instant::now();

    let mut manager = DemManager::new();
    let count = manager.add_directory(dem_dir).expect("Failed to index DEM directory");
    
    println!("Indexed {} tiles in {:.3}s", count, start.elapsed().as_secs_f64());

    if let Some(bounds) = manager.total_bounds() {
        println!(
            "Coverage: lat {:.2}° to {:.2}°, lon {:.2}° to {:.2}°",
            bounds.min_lat, bounds.max_lat, bounds.min_lon, bounds.max_lon
        );
    }

    println!("\nQuerying elevation at ({}, {})...", lat, lon);
    let query_start = Instant::now();
    
    match manager.get_elevation(lat, lon) {
        Ok(elevation) => {
            println!("Elevation: {:.2} meters (loaded in {:.2}s)", elevation, query_start.elapsed().as_secs_f64());
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }

    // Second query should be fast (tile already loaded)
    let query_start = Instant::now();
    if let Ok(nearest) = manager.get_elevation_nearest(lat, lon) {
        println!("Elevation (nearest): {:.2} meters (cached: {:.6}s)", nearest, query_start.elapsed().as_secs_f64());
    }
}
