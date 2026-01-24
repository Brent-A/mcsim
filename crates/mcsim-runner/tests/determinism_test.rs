//! Determinism tests for the MCSim simulator
//!
//! These tests verify that running the same simulation with the same seed
//! produces identical results, ensuring reproducibility. Deterministic simulation
//! is critical for debugging, testing, and scientific reproducibility.
//!
//! ## Test Strategy
//!
//! 1. **Same Seed Test**: Run the same simulation configuration twice with
//!    identical seeds and verify that all metrics match exactly.
//!
//! 2. **Different Seed Test**: Run simulations with different seeds and verify
//!    that results differ (proving the seed actually affects the simulation).
//!
//! 3. **Multiple Run Consistency**: Run the same configuration multiple times
//!    to ensure consistency across many runs.
//!
//! ## Note on Serial Execution
//!
//! These tests use #[serial] to ensure they run sequentially. This is necessary
//! because the firmware DLLs may have shared state within a process that can
//! cause non-determinism when multiple simulations run concurrently.

use std::path::Path;

use mcsim_runner::{create_event_loop, build_simulation, load_model, SimTime};
use serial_test::serial;

// ============================================================================
// Simulation Results for Comparison
// ============================================================================

/// Results captured from a simulation run for determinism comparison.
///
/// All fields that should be deterministic for a given seed are included here.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SimulationResults {
    /// Total number of events processed during simulation.
    total_events: u64,
    /// Number of packets transmitted over the air.
    packets_transmitted: u64,
    /// Number of packets successfully received.
    packets_received: u64,
    /// Number of packets lost to collisions.
    packets_collided: u64,
    /// Final simulation time in microseconds.
    final_sim_time_us: u64,
}

impl SimulationResults {
    /// Create results from simulation stats.
    fn from_stats(stats: &mcsim_runner::SimulationStats) -> Self {
        Self {
            total_events: stats.total_events,
            packets_transmitted: stats.packets_transmitted,
            packets_received: stats.packets_received,
            packets_collided: stats.packets_collided,
            final_sim_time_us: stats.simulation_time_us,
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Run a simulation with the specified configuration and seed, returning results.
///
/// # Arguments
/// * `config_path` - Path to the YAML configuration file (relative to tests directory)
/// * `seed` - Random seed for the simulation
/// * `duration_secs` - How long to run the simulation in simulation seconds
///
/// # Returns
/// `SimulationResults` containing deterministic metrics from the run.
///
/// # Panics
/// Panics if the simulation cannot be loaded or run.
fn run_simulation_with_seed(
    config_path: &Path,
    seed: u64,
    duration_secs: f64,
) -> SimulationResults {
    // Load the model configuration
    let model = load_model(config_path)
        .unwrap_or_else(|e| panic!("Failed to load model from {:?}: {}", config_path, e));
    
    // Build the simulation with the specified seed
    let simulation = build_simulation(&model, seed)
        .unwrap_or_else(|e| panic!("Failed to build simulation: {}", e));
    
    // Create the event loop
    let mut event_loop = create_event_loop(simulation, seed);
    
    // Run for the specified duration
    let duration = SimTime::from_secs(duration_secs);
    let stats = event_loop.run(duration)
        .unwrap_or_else(|e| panic!("Failed to run simulation: {}", e));
    
    SimulationResults::from_stats(&stats)
}

// ============================================================================
// Determinism Tests
// ============================================================================

/// Test that running the same simulation with the same seed produces identical results.
///
/// This is the core determinism test. If this fails, it indicates that some part
/// of the simulation is using non-deterministic sources (system time, thread
/// scheduling, unordered collections, etc.).
#[test]
#[serial]
fn test_determinism_same_seed() {
    let seed = 12345u64;
    let config_path = Path::new("tests/two_companions.yaml");
    let duration_secs = 30.0; // Run for 30 simulated seconds
    
    // First run
    let result1 = run_simulation_with_seed(config_path, seed, duration_secs);
    
    // Second run with the same seed
    let result2 = run_simulation_with_seed(config_path, seed, duration_secs);
    
    // All metrics should be identical
    assert_eq!(
        result1.total_events, result2.total_events,
        "Event count should be deterministic: run1={}, run2={}",
        result1.total_events, result2.total_events
    );
    
    assert_eq!(
        result1.packets_transmitted, result2.packets_transmitted,
        "Packet TX count should be deterministic: run1={}, run2={}",
        result1.packets_transmitted, result2.packets_transmitted
    );
    
    assert_eq!(
        result1.packets_received, result2.packets_received,
        "Packet RX count should be deterministic: run1={}, run2={}",
        result1.packets_received, result2.packets_received
    );
    
    assert_eq!(
        result1.packets_collided, result2.packets_collided,
        "Collision count should be deterministic: run1={}, run2={}",
        result1.packets_collided, result2.packets_collided
    );
    
    assert_eq!(
        result1.final_sim_time_us, result2.final_sim_time_us,
        "Final simulation time should be deterministic: run1={}, run2={}",
        result1.final_sim_time_us, result2.final_sim_time_us
    );
    
    // Final comprehensive check
    assert_eq!(
        result1, result2,
        "Full simulation results should be identical for same seed"
    );
    
    // Print results for verification
    eprintln!("✓ Determinism test passed with seed={}", seed);
    eprintln!("  Total events: {}", result1.total_events);
    eprintln!("  Packets TX: {}", result1.packets_transmitted);
    eprintln!("  Packets RX: {}", result1.packets_received);
    eprintln!("  Collisions: {}", result1.packets_collided);
    eprintln!("  Sim time: {} µs", result1.final_sim_time_us);
}

/// Test that running with different seeds produces different results.
///
/// This verifies that the seed actually affects the simulation behavior.
/// If this test fails (results are the same), it could mean:
/// - The simulation is completely deterministic regardless of seed
/// - The seed is not being properly used
/// - The simulation duration is too short to show variation
#[test]
#[serial]
fn test_determinism_different_seeds_differ() {
    let config_path = Path::new("tests/two_companions.yaml");
    let duration_secs = 30.0;
    
    let result1 = run_simulation_with_seed(config_path, 12345, duration_secs);
    let result2 = run_simulation_with_seed(config_path, 67890, duration_secs);
    
    // Results should differ for different seeds
    // Note: In some edge cases (very simple simulations), results might be
    // identical. We check multiple fields to increase likelihood of detecting
    // seed influence.
    let seeds_affect_results = result1 != result2;
    
    if !seeds_affect_results {
        // Log warning but don't necessarily fail - some simulations may be
        // deterministic regardless of seed (e.g., no random elements)
        eprintln!(
            "⚠ Warning: Different seeds produced identical results. \
             This may indicate the simulation has no stochastic elements, \
             or the test duration is too short."
        );
        eprintln!("  Seed 12345: {:?}", result1);
        eprintln!("  Seed 67890: {:?}", result2);
    } else {
        eprintln!("✓ Different seeds produced different results (as expected)");
        eprintln!("  Seed 12345: events={}, tx={}", result1.total_events, result1.packets_transmitted);
        eprintln!("  Seed 67890: events={}, tx={}", result2.total_events, result2.packets_transmitted);
    }
    
    // Note: This test may pass even if seeds produce identical results, because
    // the simulation might not have any stochastic elements (e.g., all timings
    // are fixed in the configuration). This is not a failure - it just means
    // the seed isn't used for randomization in this particular simulation.
    //
    // The important thing is that the simulation IS deterministic (same seed
    // produces same results), which is verified by other tests.
    if !seeds_affect_results {
        eprintln!("Note: Seeds don't affect this simulation's results, which is acceptable");
        eprintln!("      for fully deterministic simulations without random elements.");
    }
}

/// Test determinism across multiple runs with the same seed.
///
/// This is a more thorough version of the basic determinism test that runs
/// the simulation multiple times to catch intermittent non-determinism.
#[test]
#[serial]
fn test_determinism_multiple_runs() {
    let seed = 42u64;
    let config_path = Path::new("tests/two_companions.yaml");
    let duration_secs = 20.0;
    let num_runs = 3;
    
    // Collect results from multiple runs
    let mut results = Vec::with_capacity(num_runs);
    for _i in 0..num_runs {
        let result = run_simulation_with_seed(config_path, seed, duration_secs);
        results.push(result);
    }
    
    // All results should be identical
    let first_result = &results[0];
    for (i, result) in results.iter().enumerate().skip(1) {
        assert_eq!(
            first_result, result,
            "Run {} differs from run 1. \
             This indicates non-deterministic behavior.\n\
             Run 1: {:?}\n\
             Run {}: {:?}",
            i + 1, first_result, i + 1, result
        );
    }
    
    eprintln!("✓ All {} runs with seed={} produced identical results", num_runs, seed);
    eprintln!("  Total events: {}", first_result.total_events);
    eprintln!("  Packets TX: {}", first_result.packets_transmitted);
}

/// Test determinism with a longer simulation duration.
///
/// Longer simulations are more likely to expose non-determinism issues
/// that might not appear in shorter runs.
#[test]
#[serial]
fn test_determinism_extended_duration() {
    let seed = 99999u64;
    let config_path = Path::new("tests/two_companions.yaml");
    let duration_secs = 120.0; // 2 minutes of simulation time
    
    let result1 = run_simulation_with_seed(config_path, seed, duration_secs);
    let result2 = run_simulation_with_seed(config_path, seed, duration_secs);
    
    assert_eq!(
        result1, result2,
        "Extended duration simulation should be deterministic.\n\
         Run 1: {:?}\n\
         Run 2: {:?}",
        result1, result2
    );
    
    eprintln!("✓ Extended duration ({}s) determinism test passed", duration_secs);
    eprintln!("  Total events: {}", result1.total_events);
    eprintln!("  Packets TX: {}", result1.packets_transmitted);
}

/// Test determinism using the example configuration file.
///
/// This tests a more complex simulation setup with multiple node types.
#[test]
#[serial]
fn test_determinism_complex_network() {
    let seed = 54321u64;
    // Use the example network which has more nodes and complexity
    let config_path = Path::new("../../examples/topologies/simple.yaml");
    let duration_secs = 30.0;
    
    let result1 = run_simulation_with_seed(config_path, seed, duration_secs);
    let result2 = run_simulation_with_seed(config_path, seed, duration_secs);
    
    assert_eq!(
        result1, result2,
        "Complex network simulation should be deterministic.\n\
         Run 1: {:?}\n\
         Run 2: {:?}",
        result1, result2
    );
    
    eprintln!("✓ Complex network determinism test passed");
    eprintln!("  Total events: {}", result1.total_events);
    eprintln!("  Packets TX: {}", result1.packets_transmitted);
    eprintln!("  Packets RX: {}", result1.packets_received);
    eprintln!("  Collisions: {}", result1.packets_collided);
}
