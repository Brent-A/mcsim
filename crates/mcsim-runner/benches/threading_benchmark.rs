//! Performance benchmarks for per-node threading architecture.
//!
//! This benchmark compares the performance of the per-node threading architecture
//! with different node counts to verify that parallel execution provides benefits.
//!
//! ## Running the benchmarks
//!
//! ```bash
//! cargo bench --features per_node_threading -p mcsim-runner
//! ```
//!
//! ## Benchmarks included
//!
//! - `coordinator_advance_N_nodes` - Time to advance N nodes to a target time
//! - `coordinator_run_N_nodes` - Time to run a simulation with N nodes
//! - `coalesce_wake_times_N` - Time to coalesce N wake times

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

#[cfg(feature = "per_node_threading")]
use mcsim_common::{EntityId, SimTime};

#[cfg(feature = "per_node_threading")]
use mcsim_runner::node_thread::{
    coalesce_wake_times, CoalesceConfig, Coordinator, NodeThreadConfig, DEFAULT_COALESCE_THRESHOLD_US,
};

/// Benchmark coordinator creation and node addition.
#[cfg(feature = "per_node_threading")]
fn bench_coordinator_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("coordinator_creation");
    
    for node_count in [2, 4, 8, 16].iter() {
        group.throughput(Throughput::Elements(*node_count as u64));
        
        group.bench_with_input(
            BenchmarkId::new("create_and_add_nodes", node_count),
            node_count,
            |b, &count| {
                b.iter(|| {
                    let mut coordinator = Coordinator::new();
                    for i in 0..count {
                        let config = NodeThreadConfig {
                            name: format!("bench_node_{}", i),
                            node_index: i,
                            firmware_entity_id: EntityId::new((i + 1) as u64),
                            radio_entity_id: EntityId::new((i + 100) as u64),
                            uart_port: None,
                            tracing_enabled: false,
                        };
                        coordinator.add_node(config);
                    }
                    // Must shutdown to clean up threads
                    coordinator.shutdown().expect("Shutdown should succeed");
                    black_box(count)
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark parallel time advancement with different node counts.
#[cfg(feature = "per_node_threading")]
fn bench_parallel_advancement(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_advancement");
    group.sample_size(20); // Reduce sample size due to thread overhead
    
    for node_count in [2, 4, 8, 16].iter() {
        group.throughput(Throughput::Elements(*node_count as u64));
        
        group.bench_with_input(
            BenchmarkId::new("advance_to", node_count),
            node_count,
            |b, &count| {
                // Create coordinator with nodes once
                let mut coordinator = Coordinator::new();
                for i in 0..count {
                    let config = NodeThreadConfig {
                        name: format!("bench_node_{}", i),
                        node_index: i,
                        firmware_entity_id: EntityId::new((i + 1) as u64),
                        radio_entity_id: EntityId::new((i + 100) as u64),
                        uart_port: None,
                        tracing_enabled: false,
                    };
                    coordinator.add_node(config);
                }
                
                let mut target_time = SimTime::from_millis(100);
                
                b.iter(|| {
                    coordinator.advance_to(target_time).expect("Advance should succeed");
                    target_time = target_time + SimTime::from_millis(100);
                    black_box(target_time)
                });
                
                coordinator.shutdown().expect("Shutdown should succeed");
            },
        );
    }
    
    group.finish();
}

/// Benchmark wake time coalescing with different input sizes.
#[cfg(feature = "per_node_threading")]
fn bench_coalesce_wake_times(c: &mut Criterion) {
    let mut group = c.benchmark_group("coalesce_wake_times");
    
    for count in [4, 16, 64, 256].iter() {
        group.throughput(Throughput::Elements(*count as u64));
        
        // Scenario 1: All wake times far apart (no coalescing)
        group.bench_with_input(
            BenchmarkId::new("no_coalescing", count),
            count,
            |b, &count| {
                let wake_times: Vec<Option<SimTime>> = (0..count)
                    .map(|i| Some(SimTime::from_millis(i as u64 * 100)))
                    .collect();
                
                b.iter(|| {
                    coalesce_wake_times(black_box(&wake_times), DEFAULT_COALESCE_THRESHOLD_US)
                });
            },
        );
        
        // Scenario 2: All wake times close together (maximum coalescing)
        group.bench_with_input(
            BenchmarkId::new("all_coalesced", count),
            count,
            |b, &count| {
                let base_time = SimTime::from_millis(1000);
                let wake_times: Vec<Option<SimTime>> = (0..count)
                    .map(|i| Some(base_time + SimTime::from_micros(i as u64 * 100))) // 100us apart
                    .collect();
                
                b.iter(|| {
                    coalesce_wake_times(black_box(&wake_times), DEFAULT_COALESCE_THRESHOLD_US)
                });
            },
        );
        
        // Scenario 3: Mixed (some None, some far, some close)
        group.bench_with_input(
            BenchmarkId::new("mixed", count),
            count,
            |b, &count| {
                let wake_times: Vec<Option<SimTime>> = (0..count)
                    .map(|i| {
                        if i % 4 == 0 {
                            None
                        } else if i % 4 == 1 {
                            Some(SimTime::from_millis(i as u64 * 100))
                        } else {
                            Some(SimTime::from_millis(1000) + SimTime::from_micros(i as u64 * 100))
                        }
                    })
                    .collect();
                
                b.iter(|| {
                    coalesce_wake_times(black_box(&wake_times), DEFAULT_COALESCE_THRESHOLD_US)
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark coordinator run with different node counts.
#[cfg(feature = "per_node_threading")]
fn bench_coordinator_run(c: &mut Criterion) {
    let mut group = c.benchmark_group("coordinator_run");
    group.sample_size(10); // Reduce sample size due to longer runtime
    
    for node_count in [2, 4, 8].iter() {
        group.throughput(Throughput::Elements(*node_count as u64));
        
        group.bench_with_input(
            BenchmarkId::new("run_100ms", node_count),
            node_count,
            |b, &count| {
                b.iter_with_setup(
                    || {
                        // Setup: create coordinator with nodes
                        let mut coordinator = Coordinator::new();
                        for i in 0..count {
                            let config = NodeThreadConfig {
                                name: format!("bench_node_{}", i),
                                node_index: i,
                                firmware_entity_id: EntityId::new((i + 1) as u64),
                                radio_entity_id: EntityId::new((i + 100) as u64),
                                uart_port: None,
                                tracing_enabled: false,
                            };
                            coordinator.add_node(config);
                        }
                        coordinator
                    },
                    |mut coordinator| {
                        // Benchmark: run simulation
                        coordinator.run(SimTime::from_millis(100)).expect("Run should succeed");
                        coordinator.shutdown().expect("Shutdown should succeed");
                        black_box(())
                    },
                );
            },
        );
    }
    
    group.finish();
}

/// Benchmark coalescing config impact.
#[cfg(feature = "per_node_threading")]
fn bench_coalescing_impact(c: &mut Criterion) {
    let mut group = c.benchmark_group("coalescing_impact");
    group.sample_size(10);
    
    let node_count = 8;
    
    // With coalescing enabled (default)
    group.bench_function("with_coalescing", |b| {
        b.iter_with_setup(
            || {
                let mut coordinator = Coordinator::with_coalesce_config(CoalesceConfig {
                    enabled: true,
                    threshold_us: DEFAULT_COALESCE_THRESHOLD_US,
                });
                for i in 0..node_count {
                    let config = NodeThreadConfig {
                        name: format!("bench_node_{}", i),
                        node_index: i,
                        firmware_entity_id: EntityId::new((i + 1) as u64),
                        radio_entity_id: EntityId::new((i + 100) as u64),
                        uart_port: None,
                        tracing_enabled: false,
                    };
                    coordinator.add_node(config);
                }
                coordinator
            },
            |mut coordinator| {
                coordinator.run(SimTime::from_millis(100)).expect("Run should succeed");
                let stats = coordinator.stats().clone();
                coordinator.shutdown().expect("Shutdown should succeed");
                black_box(stats)
            },
        );
    });
    
    // With coalescing disabled
    group.bench_function("without_coalescing", |b| {
        b.iter_with_setup(
            || {
                let mut coordinator = Coordinator::with_coalesce_config(CoalesceConfig {
                    enabled: false,
                    threshold_us: 0,
                });
                for i in 0..node_count {
                    let config = NodeThreadConfig {
                        name: format!("bench_node_{}", i),
                        node_index: i,
                        firmware_entity_id: EntityId::new((i + 1) as u64),
                        radio_entity_id: EntityId::new((i + 100) as u64),
                        uart_port: None,
                        tracing_enabled: false,
                    };
                    coordinator.add_node(config);
                }
                coordinator
            },
            |mut coordinator| {
                coordinator.run(SimTime::from_millis(100)).expect("Run should succeed");
                let stats = coordinator.stats().clone();
                coordinator.shutdown().expect("Shutdown should succeed");
                black_box(stats)
            },
        );
    });
    
    group.finish();
}

// Dummy benchmarks when feature is not enabled
#[cfg(not(feature = "per_node_threading"))]
fn bench_coordinator_creation(_c: &mut Criterion) {
    // Feature not enabled, skip benchmark
}

#[cfg(not(feature = "per_node_threading"))]
fn bench_parallel_advancement(_c: &mut Criterion) {
    // Feature not enabled, skip benchmark
}

#[cfg(not(feature = "per_node_threading"))]
fn bench_coalesce_wake_times(_c: &mut Criterion) {
    // Feature not enabled, skip benchmark
}

#[cfg(not(feature = "per_node_threading"))]
fn bench_coordinator_run(_c: &mut Criterion) {
    // Feature not enabled, skip benchmark
}

#[cfg(not(feature = "per_node_threading"))]
fn bench_coalescing_impact(_c: &mut Criterion) {
    // Feature not enabled, skip benchmark
}

criterion_group!(
    benches,
    bench_coordinator_creation,
    bench_parallel_advancement,
    bench_coalesce_wake_times,
    bench_coordinator_run,
    bench_coalescing_impact,
);

criterion_main!(benches);
