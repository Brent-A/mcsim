//! # mcsim-runner library
//!
//! Library interface for MCSim simulation runner.
//!
//! This module re-exports the key types and functions needed for running
//! simulations programmatically and for integration testing.
//!
//! ## Parallel Node Stepping
//!
//! The runner supports parallel stepping of firmware nodes within a time slice.
//! When multiple firmware entities have events at the same simulation time,
//! they can be stepped concurrently using the async `step_begin()` / `step_wait()`
//! pattern from the DLL interface.
//!
//! Determinism is maintained by:
//! - Processing events in sorted order by entity ID
//! - Collecting results and generating new events sequentially
//! - Using deterministic RNG seeding per entity
//!
//! ## Real-Time Mode
//!
//! The runner supports real-time simulation mode where simulation time tracks
//! wall clock time. Features include:
//! - Speed multiplier for faster/slower than real-time simulation
//! - Catch-up logic when simulation falls behind wall clock
//! - Drift tracking and warnings

pub mod metric_spec;
pub mod metrics_export;
mod packet_tracker;
pub mod parallel_step;
pub mod realtime;
pub mod rerun_blueprint;
pub mod rerun_logger;
pub mod uart_server;
pub mod watchdog;

use mcsim_common::entity_tracer::EntityTracer;
use mcsim_common::{EntityId, Event, EventPayload, SimContext};
pub use mcsim_common::SimTime;
use mcsim_model::BuiltSimulation;
use packet_tracker::PacketTracker;
pub use parallel_step::{ParallelStepConfig, FirmwareStepOutput};
pub use realtime::{RealTimeConfig, RealTimePacer, RealTimePacerStats, PeriodicStats};
pub use rerun_logger::RerunLogger;
use serde::Serialize;
use std::collections::{BinaryHeap, HashMap};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
pub use uart_server::SyncUartManager;
pub use watchdog::{Watchdog, WatchdogState, CurrentEventInfo};

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during simulation.
#[derive(Debug, Error)]
pub enum RunnerError {
    /// Model error.
    #[error("Model error: {0}")]
    Model(#[from] mcsim_model::ModelError),

    /// Simulation error.
    #[error("Simulation error: {0}")]
    Simulation(#[from] mcsim_common::SimError),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Format a duration in a human-readable way with fixed width (e.g., "1h23m45s").
/// Always shows hours:minutes:seconds for consistent column alignment.
fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    
    format!("{:2}h{:02}m{:02}s", hours, minutes, seconds)
}

// ============================================================================
// Simulation Statistics
// ============================================================================

/// Per-node statistics.
#[derive(Debug, Clone, Default, Serialize)]
pub struct NodeStats {
    /// Packets transmitted by this node.
    pub tx: u64,
    /// Packets received by this node.
    pub rx: u64,
    /// Packets that collided when received by this node.
    pub collisions: u64,
}

/// Statistics collected during simulation.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SimulationStats {
    /// Total events processed.
    pub total_events: u64,
    /// Packets transmitted.
    pub packets_transmitted: u64,
    /// Packets received.
    pub packets_received: u64,
    /// Packets lost to collisions.
    pub packets_collided: u64,
    /// Messages sent.
    pub messages_sent: u64,
    /// Messages acknowledged.
    pub messages_acked: u64,
    /// Final simulation time.
    pub simulation_time_us: u64,
    /// Wall clock time in milliseconds.
    pub wall_time_ms: u64,
}

// ============================================================================
// Progress Reporting
// ============================================================================

/// Progress information passed to the progress callback during simulation.
#[derive(Debug, Clone)]
pub struct ProgressInfo {
    /// Current simulation time.
    pub sim_time: SimTime,
    /// Target simulation time (end time).
    pub target_time: SimTime,
    /// Elapsed wall clock time since start.
    pub wall_elapsed: Duration,
    /// Total events processed so far.
    pub events_processed: u64,
    /// Simulation time multiplier (how many times faster than real-time).
    pub time_multiplier: f64,
    /// Estimated time remaining based on current pace.
    pub estimated_remaining: Duration,
    /// Progress as a percentage (0.0 to 100.0).
    pub progress_percent: f64,
}

// ============================================================================
// Trace Recording
// ============================================================================

/// A trace entry for output.
#[derive(Debug, Clone, Serialize)]
pub struct TraceEntry {
    /// Origin node name.
    pub origin: String,
    /// Origin node ID.
    pub origin_id: String,
    /// Timestamp (ISO 8601).
    pub timestamp: String,
    /// Event type.
    #[serde(rename = "type")]
    pub entry_type: String,
    /// Direction (TX/RX).
    pub direction: String,
    /// Signal-to-noise ratio.
    #[serde(rename = "SNR")]
    pub snr: String,
    /// Received signal strength.
    #[serde(rename = "RSSI")]
    pub rssi: String,
    /// Raw packet payload (hex-encoded).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packet_hex: Option<String>,
    /// Decoded packet data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packet: Option<serde_json::Value>,
    /// Reception status: "ok", "collided", or "weak".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reception_status: Option<String>,
    /// Packet start time in seconds (for TX and RX events).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packet_start_time_s: Option<f64>,
    /// Packet end time in seconds (for TX and RX events).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packet_end_time_s: Option<f64>,
}

/// Trace recorder for outputting simulation events.
pub struct TraceRecorder {
    output: Option<Box<dyn Write>>,
    entries: Vec<TraceEntry>,
}

impl TraceRecorder {
    /// Create a new trace recorder.
    pub fn new(output: Option<Box<dyn Write>>) -> Self {
        TraceRecorder {
            output,
            entries: Vec::new(),
        }
    }

    /// Record an event.
    pub fn record(&mut self, entry: TraceEntry) {
        self.entries.push(entry);
    }

    /// Flush all entries to output.
    pub fn flush(&mut self) -> Result<(), RunnerError> {
        if let Some(ref mut output) = self.output {
            let json = serde_json::to_string_pretty(&self.entries)?;
            writeln!(output, "{}", json)?;
        }
        Ok(())
    }
}

// ============================================================================
// Event Loop
// ============================================================================

/// The main simulation event loop.
///
/// Supports both sequential and parallel event processing modes.
/// Parallel stepping can be enabled via [`ParallelStepConfig`] for improved
/// performance when stepping multiple firmware nodes at the same simulation time.
pub struct EventLoop {
    event_queue: BinaryHeap<Event>,
    simulation: BuiltSimulation,
    context: SimContext,
    trace: TraceRecorder,
    stats: SimulationStats,
    /// Per-node statistics, keyed by radio entity ID.
    node_stats: HashMap<u64, NodeStats>,
    /// Mapping from firmware entity ID to radio entity ID.
    firmware_to_radio: HashMap<u64, u64>,
    /// Mapping from radio entity ID to node name.
    radio_to_name: HashMap<u64, String>,
    /// Mapping from entity ID to (node_name, node_type) for metrics labels.
    entity_to_labels: HashMap<u64, (String, String)>,
    /// Set of firmware entity IDs (for identifying firmware events).
    #[allow(dead_code)]
    firmware_entity_ids: std::collections::HashSet<u64>,
    uart_manager: Option<SyncUartManager>,
    rerun_logger: Option<RerunLogger>,
    entity_tracer: EntityTracer,
    /// Packet tracker for delivery metrics.
    packet_tracker: PacketTracker,
    /// Maximum age of tracked packets before eviction (in microseconds).
    /// If None, packets are never evicted.
    packet_eviction_age_us: Option<u64>,
    /// Last simulation time when packet eviction was performed.
    last_eviction_time_us: u64,
    /// Configuration for parallel stepping.
    parallel_config: ParallelStepConfig,
    /// Configuration for real-time mode.
    realtime_config: RealTimeConfig,
    /// Optional metrics recorder for collecting metrics and logging to Rerun.
    metrics_recorder: Option<Arc<metrics_export::InMemoryRecorder>>,
    /// Metric specs for Rerun visualization.
    rerun_metric_specs: Vec<metric_spec::MetricSpec>,
}

impl EventLoop {
    /// Create a new event loop.
    pub fn new(
        simulation: BuiltSimulation,
        seed: u64,
        trace_output: Option<Box<dyn Write>>,
        uart_manager: Option<SyncUartManager>,
        rerun_logger: Option<RerunLogger>,
        entity_tracer: EntityTracer,
    ) -> Self {
        let mut event_queue = BinaryHeap::new();

        // Add initial events to queue
        for event in simulation.initial_events.iter().cloned() {
            event_queue.push(event);
        }

        // Initialize per-node stats and build entity ID mappings
        let mut node_stats = HashMap::new();
        let mut firmware_to_radio = HashMap::new();
        let mut radio_to_name = HashMap::new();
        let mut entity_to_labels = HashMap::new();
        let mut firmware_entity_ids = std::collections::HashSet::new();
        for node_info in &simulation.node_infos {
            node_stats.insert(node_info.radio_entity_id, NodeStats::default());
            firmware_to_radio.insert(node_info.firmware_entity_id, node_info.radio_entity_id);
            radio_to_name.insert(node_info.radio_entity_id, node_info.name.clone());
            // Map both firmware and radio entity IDs to labels for metrics
            let labels = (node_info.name.clone(), node_info.node_type.clone());
            entity_to_labels.insert(node_info.firmware_entity_id, labels.clone());
            entity_to_labels.insert(node_info.radio_entity_id, labels);
            firmware_entity_ids.insert(node_info.firmware_entity_id);
        }

        // Create packet tracker with total node count
        let total_nodes = simulation.node_infos.len();
        let packet_tracker = PacketTracker::new(total_nodes);

        EventLoop {
            event_queue,
            simulation,
            context: SimContext::with_tracer(seed, entity_tracer.clone()),
            trace: TraceRecorder::new(trace_output),
            stats: SimulationStats::default(),
            node_stats,
            firmware_to_radio,
            radio_to_name,
            entity_to_labels,
            firmware_entity_ids,
            uart_manager,
            rerun_logger,
            entity_tracer,
            packet_tracker,
            packet_eviction_age_us: None,
            last_eviction_time_us: 0,
            parallel_config: ParallelStepConfig::default(),
            realtime_config: RealTimeConfig::default(),
            metrics_recorder: None,
            rerun_metric_specs: Vec::new(),
        }
    }
    
    /// Enable or disable parallel stepping.
    pub fn set_parallel_stepping(&mut self, enabled: bool) {
        self.parallel_config.enabled = enabled;
    }
    
    /// Configure parallel stepping with custom settings.
    pub fn set_parallel_config(&mut self, config: ParallelStepConfig) {
        self.parallel_config = config;
    }
    
    /// Configure real-time mode settings.
    pub fn set_realtime_config(&mut self, config: RealTimeConfig) {
        self.realtime_config = config;
    }
    
    /// Get a reference to the real-time configuration.
    pub fn realtime_config(&self) -> &RealTimeConfig {
        &self.realtime_config
    }
    
    /// Configure packet tracker eviction.
    ///
    /// When set, packets older than the specified age will be periodically
    /// evicted from the tracker to limit memory usage. Metrics are emitted
    /// for each packet before removal.
    ///
    /// # Arguments
    /// * `age_secs` - Maximum packet age in seconds, or None to disable eviction
    pub fn set_packet_eviction_age(&mut self, age_secs: Option<f64>) {
        self.packet_eviction_age_us = age_secs.map(|s| (s * 1_000_000.0) as u64);
    }
    
    /// Perform packet eviction if configured and enough time has passed.
    ///
    /// Eviction is performed at most once per eviction age period to avoid
    /// excessive overhead.
    fn maybe_evict_packets(&mut self, current_time_us: u64) {
        if let Some(max_age_us) = self.packet_eviction_age_us {
            // Only evict once per eviction period
            if current_time_us >= self.last_eviction_time_us + max_age_us {
                self.packet_tracker.evict_old_packets(current_time_us, max_age_us);
                self.last_eviction_time_us = current_time_us;
            }
        }
    }
    
    /// Configure metrics recording and Rerun visualization.
    ///
    /// This enables periodic logging of metrics snapshots to Rerun.
    ///
    /// # Arguments
    /// * `recorder` - The metrics recorder to take snapshots from
    /// * `specs` - Metric specifications defining which metrics to log and their breakdowns
    pub fn set_metrics_for_rerun(
        &mut self,
        recorder: Arc<metrics_export::InMemoryRecorder>,
        specs: Vec<metric_spec::MetricSpec>,
    ) {
        self.metrics_recorder = Some(recorder);
        self.rerun_metric_specs = specs;
    }
    
    /// Check if an entity ID is a firmware entity.
    #[allow(dead_code)]
    fn is_firmware_entity(&self, entity_id: EntityId) -> bool {
        self.firmware_entity_ids.contains(&entity_id.0)
    }

    /// Dispatch an event to its target entities, recording per-entity step time metrics.
    fn dispatch_event_with_metrics(
        &mut self,
        event: &Event,
    ) -> Result<(), mcsim_common::SimError> {
        for target in &event.targets {
            if let Some(entity) = self.simulation.entities.get_mut(*target) {
                self.context.set_source(*target);
                
                // Time the step
                let step_start = std::time::Instant::now();
                entity.handle_event(event, &mut self.context)?;
                let step_elapsed = step_start.elapsed();
                
                // Record metric with labels if we have them
                if let Some((name, node_type)) = self.entity_to_labels.get(&target.0) {
                    let labels = [
                        ("node", name.clone()),
                        ("node_type", node_type.clone()),
                    ];
                    metrics::histogram!(
                        mcsim_metrics::metric_defs::SIMULATION_STEP_TIME.name,
                        &labels
                    ).record(step_elapsed.as_micros() as f64);
                }
            } else {
                eprintln!("ERROR: EntityNotFound {:?} when dispatching {:?}", target, event.payload);
                return Err(mcsim_common::SimError::EntityNotFound(*target));
            }
        }
        Ok(())
    }

    /// Run the simulation for the specified duration.
    pub fn run(&mut self, duration: SimTime) -> Result<SimulationStats, RunnerError> {
        self.run_with_progress(duration, None, |_, _, _| {})
    }

    /// Run the simulation with an optional stop flag and a progress callback.
    /// 
    /// The progress callback is invoked periodically (approximately every `progress_interval`)
    /// with information about the simulation progress.
    ///
    /// # Arguments
    /// * `duration` - Simulation duration to run for
    /// * `stop_flag` - Optional flag to signal early termination
    /// * `on_progress` - Callback invoked with progress information
    pub fn run_with_progress<F>(
        &mut self,
        duration: SimTime,
        stop_flag: Option<Arc<AtomicBool>>,
        mut on_progress: F,
    ) -> Result<SimulationStats, RunnerError>
    where
        F: FnMut(&Self, ProgressInfo, bool), // bool = is_final
    {
        let start_time = Instant::now();
        let end_time = duration;
        let progress_interval = Duration::from_secs(5);
        let mut last_progress = Instant::now();
        let mut last_progress_sim_time = SimTime::ZERO;
        let mut last_progress_events = 0u64;
        // Report every N events as a fallback if time-based reporting doesn't trigger
        let event_progress_interval = 100_000u64;

        // Add end-of-simulation event
        self.event_queue.push(Event {
            id: mcsim_common::EventId(u64::MAX),
            time: end_time,
            source: mcsim_common::EntityId::new(0),
            targets: vec![],
            payload: EventPayload::SimulationEnd,
        });

        // Main event loop
        while let Some(event) = self.event_queue.pop() {
            // Check for stop flag
            if let Some(ref flag) = stop_flag {
                if flag.load(Ordering::Relaxed) {
                    break;
                }
            }

            // Stop at simulation end
            if matches!(event.payload, EventPayload::SimulationEnd) {
                break;
            }

            // Advance simulation time
            self.context.set_time(event.time);

            // Handle SerialTx events: forward to TCP clients AND dispatch to entity targets
            if let EventPayload::SerialTx(serial_event) = &event.payload {
                // Forward to TCP clients (for UART bridge) only if self-targeted
                // This avoids double-sending when firmware sends to both self and agent
                if event.targets.contains(&event.source) {
                    if let Some(ref uart_mgr) = self.uart_manager {
                        uart_mgr.send_to_client(event.source.0, &serial_event.data);
                    }
                }
                // Continue to dispatch to entities (agents receive SerialTx from firmware)
            }

            // Dispatch event to target entities (with per-entity timing metrics)
            self.dispatch_event_with_metrics(&event)?;

            // Collect new events
            let new_events = self.context.take_pending_events();
            for new_event in new_events {
                self.event_queue.push(new_event);
            }

            // Update statistics
            self.stats.total_events += 1;
            self.update_stats(&event);

            // Record trace entry
            self.record_trace(&event);

            // Log to rerun visualization
            if let Some(ref mut rerun) = self.rerun_logger {
                let _ = rerun.log_event(&event);
            }

            // Periodically evict old packets to limit memory usage
            self.maybe_evict_packets(self.context.time().as_micros());

            // Report progress periodically (time-based or event-count-based)
            let events_since_last = self.stats.total_events - last_progress_events;
            let should_report = last_progress.elapsed() >= progress_interval 
                || events_since_last >= event_progress_interval;
            
            if should_report {
                let wall_elapsed = start_time.elapsed();
                let sim_time = self.context.time();
                let progress_percent = (sim_time.as_secs_f64() / end_time.as_secs_f64()) * 100.0;

                // Calculate time multiplier based on recent progress
                let sim_delta = sim_time.as_secs_f64() - last_progress_sim_time.as_secs_f64();
                let wall_delta = last_progress.elapsed().as_secs_f64();
                let time_multiplier = if wall_delta > 0.0 { sim_delta / wall_delta } else { 0.0 };

                // Estimate remaining time based on current pace
                let remaining_sim = end_time.as_secs_f64() - sim_time.as_secs_f64();
                let estimated_remaining = if time_multiplier > 0.0 {
                    Duration::from_secs_f64(remaining_sim / time_multiplier)
                } else {
                    Duration::from_secs(0)
                };

                let progress = ProgressInfo {
                    sim_time,
                    target_time: end_time,
                    wall_elapsed,
                    events_processed: self.stats.total_events,
                    time_multiplier,
                    estimated_remaining,
                    progress_percent,
                };
                on_progress(self, progress, false);

                // Log metrics to rerun periodically
                if let Some(ref rerun) = self.rerun_logger {
                    if let Some(ref recorder) = self.metrics_recorder {
                        let export = recorder.snapshot_with_specs(&self.rerun_metric_specs);
                        let _ = rerun.log_metrics_snapshot(sim_time, &export);
                    }
                }

                last_progress = Instant::now();
                last_progress_sim_time = sim_time;
                last_progress_events = self.stats.total_events;
            }
        }

        // Emit packet tracking summaries
        self.packet_tracker.emit_flood_summaries();

        // Finalize stats
        self.stats.simulation_time_us = self.context.time().as_micros();
        self.stats.wall_time_ms = start_time.elapsed().as_millis() as u64;

        // Final progress report
        let wall_elapsed = start_time.elapsed();
        let sim_time = self.context.time();
        let time_multiplier = sim_time.as_secs_f64() / wall_elapsed.as_secs_f64();
        let progress = ProgressInfo {
            sim_time,
            target_time: end_time,
            wall_elapsed,
            events_processed: self.stats.total_events,
            time_multiplier,
            estimated_remaining: Duration::ZERO,
            progress_percent: 100.0,
        };
        on_progress(self, progress, true);

        // Flush trace
        self.trace.flush()?;

        Ok(self.stats.clone())
    }

    /// Run the simulation with an optional stop flag for graceful shutdown.
    pub fn run_with_stop_flag(
        &mut self,
        duration: SimTime,
        stop_flag: Option<Arc<AtomicBool>>,
    ) -> Result<SimulationStats, RunnerError> {
        self.run_with_progress(duration, stop_flag, |_, _, _| {})
    }

    /// Run the simulation with watchdog monitoring and optional break point.
    /// 
    /// The watchdog monitors for slow events and prints detailed information
    /// when an event takes longer than the timeout.
    ///
    /// # Arguments
    /// * `duration` - Simulation duration to run for
    /// * `stop_flag` - Optional flag to signal early termination
    /// * `watchdog` - Optional watchdog for monitoring slow events
    /// * `break_at_event` - Optional event number to break at for debugging
    /// * `on_progress` - Callback invoked with progress information
    pub fn run_with_watchdog<F>(
        &mut self,
        duration: SimTime,
        stop_flag: Option<Arc<AtomicBool>>,
        watchdog: Option<&Watchdog>,
        break_at_event: Option<u64>,
        mut on_progress: F,
    ) -> Result<SimulationStats, RunnerError>
    where
        F: FnMut(&Self, ProgressInfo, bool), // bool = is_final
    {
        let start_time = Instant::now();
        let end_time = duration;
        let progress_interval = Duration::from_secs(5);
        let mut last_progress = Instant::now();
        let mut last_progress_sim_time = SimTime::ZERO;
        let mut last_progress_events = 0u64;
        let event_progress_interval = 100_000u64;

        // Register entity names with the watchdog for better logging
        if let Some(watchdog) = watchdog {
            let names = self.simulation.node_infos.iter().flat_map(|ni| {
                vec![
                    (ni.firmware_entity_id, ni.name.clone()),
                    (ni.radio_entity_id, format!("{}/radio", ni.name)),
                ]
            });
            watchdog.state().register_entity_names(names);
        }

        // Add end-of-simulation event
        self.event_queue.push(Event {
            id: mcsim_common::EventId(u64::MAX),
            time: end_time,
            source: mcsim_common::EntityId::new(0),
            targets: vec![],
            payload: EventPayload::SimulationEnd,
        });

        // Track event number for break point
        let mut event_number: u64 = 0;

        // Main event loop
        while let Some(event) = self.event_queue.pop() {
            event_number += 1;

            // Check for stop flag
            if let Some(ref flag) = stop_flag {
                if flag.load(Ordering::Relaxed) {
                    break;
                }
            }

            // Stop at simulation end
            if matches!(event.payload, EventPayload::SimulationEnd) {
                break;
            }

            // Check for break point
            if let Some(break_event) = break_at_event {
                if event_number == break_event {
                    let info = CurrentEventInfo::from_event(&event, event_number);
                    eprintln!();
                    eprintln!("┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                    eprintln!("┃ 🛑 BREAK POINT: Event #{}", event_number);
                    eprintln!("┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                    eprintln!("┃ Event ID:      {}", info.event_id);
                    eprintln!("┃ Event Type:    {}", info.event_type);
                    eprintln!("┃ Sim Time:      {:.3}s", info.sim_time.as_secs_f64());
                    eprintln!("┃ Source:        entity:{}", info.source_entity_id);
                    if !info.target_entity_ids.is_empty() {
                        let targets: Vec<String> = info.target_entity_ids.iter()
                            .map(|id| format!("entity:{}", id))
                            .collect();
                        eprintln!("┃ Targets:       {}", targets.join(", "));
                    }
                    if !info.details.is_empty() {
                        eprintln!("┃ Details:       {}", info.details);
                    }
                    eprintln!("┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                    eprintln!();
                    eprintln!("Press Ctrl+C to exit, or attach a debugger to trace into the event.");
                    eprintln!();
                    
                    // Wait indefinitely for user to attach debugger or exit
                    loop {
                        std::thread::sleep(Duration::from_secs(1));
                        if let Some(ref flag) = stop_flag {
                            if flag.load(Ordering::Relaxed) {
                                break;
                            }
                        }
                    }
                    break;
                }
            }

            // Update watchdog with current event info
            if let Some(watchdog) = watchdog {
                let event_info = CurrentEventInfo::from_event(&event, event_number);
                watchdog.state().set_current_event(Some(event_info));
            }

            // Advance simulation time
            self.context.set_time(event.time);

            // Handle SerialTx events: forward to TCP clients AND dispatch to entity targets
            if let EventPayload::SerialTx(serial_event) = &event.payload {
                // Forward to TCP clients (for UART bridge) only if self-targeted
                // This avoids double-sending when firmware sends to both self and agent
                if event.targets.contains(&event.source) {
                    if let Some(ref uart_mgr) = self.uart_manager {
                        let uart_start = std::time::Instant::now();
                        uart_mgr.send_to_client(event.source.0, &serial_event.data);
                        let uart_elapsed = uart_start.elapsed();
                        if uart_elapsed > Duration::from_millis(100) {
                            eprintln!("⚠ UART send_to_client took {:.3}s for entity {}", 
                                uart_elapsed.as_secs_f64(), event.source.0);
                        }
                    }
                }
            }

            // Dispatch event to target entities (with per-entity timing metrics)
            let dispatch_start = std::time::Instant::now();
            self.dispatch_event_with_metrics(&event)?;
            let dispatch_elapsed = dispatch_start.elapsed();
            if dispatch_elapsed > Duration::from_millis(100) {
                eprintln!("⚠ dispatch_event took {:.3}s for {:?} to {:?}", 
                    dispatch_elapsed.as_secs_f64(), event.payload, event.targets);
            }

            // Clear watchdog - event finished
            if let Some(watchdog) = watchdog {
                watchdog.state().set_current_event(None);
            }

            // Collect new events
            let new_events = self.context.take_pending_events();
            for new_event in new_events {
                self.event_queue.push(new_event);
            }

            // Update statistics
            self.stats.total_events += 1;
            self.update_stats(&event);

            // Record trace entry
            self.record_trace(&event);

            // Log to rerun visualization
            if let Some(ref mut rerun) = self.rerun_logger {
                let _ = rerun.log_event(&event);
            }

            // Periodically evict old packets to limit memory usage
            self.maybe_evict_packets(self.context.time().as_micros());

            // Report progress periodically
            let events_since_last = self.stats.total_events - last_progress_events;
            let should_report = last_progress.elapsed() >= progress_interval 
                || events_since_last >= event_progress_interval;
            
            if should_report {
                let wall_elapsed = start_time.elapsed();
                let sim_time = self.context.time();
                let progress_percent = (sim_time.as_secs_f64() / end_time.as_secs_f64()) * 100.0;

                let sim_delta = sim_time.as_secs_f64() - last_progress_sim_time.as_secs_f64();
                let wall_delta = last_progress.elapsed().as_secs_f64();
                let time_multiplier = if wall_delta > 0.0 { sim_delta / wall_delta } else { 0.0 };

                let remaining_sim = end_time.as_secs_f64() - sim_time.as_secs_f64();
                let estimated_remaining = if time_multiplier > 0.0 {
                    Duration::from_secs_f64(remaining_sim / time_multiplier)
                } else {
                    Duration::from_secs(0)
                };

                let progress = ProgressInfo {
                    sim_time,
                    target_time: end_time,
                    wall_elapsed,
                    events_processed: self.stats.total_events,
                    time_multiplier,
                    estimated_remaining,
                    progress_percent,
                };
                on_progress(self, progress, false);

                // Log metrics to rerun periodically
                if let Some(ref rerun) = self.rerun_logger {
                    if let Some(ref recorder) = self.metrics_recorder {
                        let export = recorder.snapshot_with_specs(&self.rerun_metric_specs);
                        let _ = rerun.log_metrics_snapshot(sim_time, &export);
                    }
                }

                last_progress = Instant::now();
                last_progress_sim_time = sim_time;
                last_progress_events = self.stats.total_events;
            }
        }

        // Emit packet tracking summaries
        self.packet_tracker.emit_flood_summaries();

        // Finalize stats
        self.stats.simulation_time_us = self.context.time().as_micros();
        self.stats.wall_time_ms = start_time.elapsed().as_millis() as u64;

        // Final progress report
        let wall_elapsed = start_time.elapsed();
        let sim_time = self.context.time();
        let time_multiplier = if wall_elapsed.as_secs_f64() > 0.0 {
            sim_time.as_secs_f64() / wall_elapsed.as_secs_f64()
        } else {
            0.0
        };
        let progress = ProgressInfo {
            sim_time,
            target_time: end_time,
            wall_elapsed,
            events_processed: self.stats.total_events,
            time_multiplier,
            estimated_remaining: Duration::ZERO,
            progress_percent: 100.0,
        };
        on_progress(self, progress, true);

        // Flush trace
        self.trace.flush()?;

        Ok(self.stats.clone())
    }

    /// Get the per-node statistics.
    pub fn node_stats(&self) -> &HashMap<u64, NodeStats> {
        &self.node_stats
    }

    /// Get the simulation node info.
    pub fn node_infos(&self) -> &[mcsim_model::NodeInfo] {
        &self.simulation.node_infos
    }

    /// Get current statistics.
    pub fn stats(&self) -> &SimulationStats {
        &self.stats
    }

    /// Get reference to the UART manager if present.
    pub fn uart_manager(&self) -> Option<&SyncUartManager> {
        self.uart_manager.as_ref()
    }

    /// Get current simulation time.
    pub fn current_time(&self) -> SimTime {
        self.context.time()
    }

    /// Check if entity tracing is enabled.
    pub fn is_tracing_enabled(&self) -> bool {
        self.entity_tracer.is_enabled()
    }

    /// Run the simulation in realtime mode, matching wall clock time.
    /// Returns stats when stop_flag is set to true.
    ///
    /// The `on_tick` callback is called approximately every second with:
    /// - A reference to the EventLoop (for accessing stats, node_infos, etc.)
    /// - The elapsed Duration since simulation start
    ///
    /// ## Real-Time Pacing
    ///
    /// The simulation uses the configured [`RealTimeConfig`] to control pacing:
    /// - `speed_multiplier`: Controls how fast simulation runs relative to wall clock
    /// - `max_catchup_ms`: Warning threshold for when simulation falls behind
    /// - `enabled`: When false, runs as fast as possible
    ///
    /// Use [`set_realtime_config()`](Self::set_realtime_config) to configure before calling this method.
    pub fn run_realtime<F>(
        &mut self,
        stop_flag: Arc<AtomicBool>,
        mut on_tick: F,
    ) -> Result<SimulationStats, RunnerError>
    where
        F: FnMut(&Self, Duration),
    {
        let start_wall = Instant::now();
        let start_sim = self.context.time();
        let mut pacer = RealTimePacer::new(self.realtime_config.clone(), start_sim);
        
        let mut last_tick = Instant::now();
        let tick_interval = Duration::from_secs(1);

        while !stop_flag.load(Ordering::Relaxed) {
            // Calculate target simulation time using the pacer (handles speed multiplier)
            let target_sim_time = pacer.target_sim_time();

            // Poll for incoming serial data from TCP clients and inject SerialRx events
            if let Some(ref uart_mgr) = self.uart_manager {
                for node_info in &self.simulation.node_infos {
                    if let Some(data) = uart_mgr.try_recv_from_client(node_info.firmware_entity_id) {
                        // Create a SerialRx event for this firmware entity
                        let current_sim = self.context.time();
                        let event = Event {
                            id: mcsim_common::EventId(self.context.next_event_id()),
                            time: current_sim,
                            source: mcsim_common::EntityId::new(node_info.firmware_entity_id),
                            targets: vec![mcsim_common::EntityId::new(node_info.firmware_entity_id)],
                            payload: EventPayload::SerialRx(mcsim_common::SerialRxEvent { data }),
                        };
                        self.event_queue.push(event);
                    }
                }
            }

            // Process all events up to target simulation time (catch-up logic)
            let mut events_this_tick = 0;
            while let Some(event) = self.event_queue.peek() {
                if event.time > target_sim_time || stop_flag.load(Ordering::Relaxed) {
                    break; // Event is in the future or stop requested
                }

                let event = self.event_queue.pop().unwrap();

                // Advance simulation time
                self.context.set_time(event.time);

                // Handle SerialTx events: forward to TCP clients AND dispatch to entity targets
                if let EventPayload::SerialTx(serial_event) = &event.payload {
                    // Forward to TCP clients (for UART bridge) only if self-targeted
                    // This avoids double-sending when firmware sends to both self and agent
                    if event.targets.contains(&event.source) {
                        if let Some(ref uart_mgr) = self.uart_manager {
                            uart_mgr.send_to_client(event.source.0, &serial_event.data);
                        }
                    }
                    // Continue to dispatch to entities (agents receive SerialTx from firmware)
                }

                // Dispatch event to target entities (with per-entity timing metrics)
                self.dispatch_event_with_metrics(&event)?;

                // Collect new events
                let new_events = self.context.take_pending_events();
                for new_event in new_events {
                    self.event_queue.push(new_event);
                }

                // Update statistics
                self.stats.total_events += 1;
                self.update_stats(&event);

                // Record trace entry
                self.record_trace(&event);

                // Log to rerun visualization
                if let Some(ref mut rerun) = self.rerun_logger {
                    let _ = rerun.log_event(&event);
                }

                // Periodically evict old packets to limit memory usage
                self.maybe_evict_packets(self.context.time().as_micros());

                events_this_tick += 1;

                // Check stop flag periodically during heavy event processing
                if events_this_tick % 1000 == 0 && stop_flag.load(Ordering::Relaxed) {
                    break;
                }
            }

            // Check for lag warning (simulation falling behind wall clock)
            if let Some(drift_ms) = pacer.check_lag_warning(self.context.time()) {
                eprintln!(
                    "⚠ Simulation lagging by {}ms (speed: {}x)",
                    drift_ms, self.realtime_config.speed_multiplier
                );
            }

            // Check for periodic stats output (memory, event rate, etc.)
            if let Some(stats) = pacer.check_periodic_stats(self.context.time(), self.stats.total_events) {
                eprintln!(
                    "📊 real={} | sim={} | ratio={:5.2}x | events={:>12} | ev/s real={:>8.0} sim={:>8.0} | mem={}",
                    format_duration(stats.wall_elapsed),
                    format_duration(Duration::from_secs_f64(stats.sim_time.as_secs_f64())),
                    stats.sim_to_realtime_ratio,
                    stats.total_events,
                    stats.event_rate_real,
                    stats.event_rate_sim,
                    stats.memory_human_readable(),
                );
            }

            // Call tick callback approximately every second
            if last_tick.elapsed() >= tick_interval {
                on_tick(self, start_wall.elapsed());

                // Log stats and metrics to rerun periodically
                if let Some(ref rerun) = self.rerun_logger {
                    let _ = rerun.log_stats(
                        self.context.time(),
                        self.stats.total_events,
                        self.stats.packets_transmitted,
                        self.stats.packets_received,
                        self.stats.packets_collided,
                    );
                    
                    // Log metrics snapshot if recorder is available
                    if let Some(ref recorder) = self.metrics_recorder {
                        let export = recorder.snapshot_with_specs(&self.rerun_metric_specs);
                        let _ = rerun.log_metrics_snapshot(self.context.time(), &export);
                    }
                }

                last_tick = Instant::now();
            }

            // Calculate sleep time to prevent busy-waiting
            // If we have a next event, sleep until it should be processed
            // Otherwise, sleep for minimum duration
            // Always cap at 10ms to ensure Ctrl-C responsiveness on all platforms
            let sleep_duration = if let Some(next_event) = self.event_queue.peek() {
                pacer
                    .sleep_until_event(next_event.time)
                    .unwrap_or(pacer.min_sleep_duration())
            } else {
                pacer.min_sleep_duration()
            }
            .min(Duration::from_millis(10));

            if sleep_duration > Duration::ZERO {
                std::thread::sleep(sleep_duration);
            }
        }

        // Log final pacing stats
        let pacer_stats = pacer.stats();
        if pacer_stats.total_lag_warnings > 0 {
            eprintln!(
                "📊 Real-time stats: {} lag warnings, max drift {}ms, speed {}x",
                pacer_stats.total_lag_warnings,
                pacer_stats.max_drift_seen_ms,
                pacer_stats.speed_multiplier
            );
        }

        // Finalize stats
        self.stats.wall_time_ms = start_wall.elapsed().as_millis() as u64;
        self.stats.simulation_time_us = self.context.time().as_micros();

        // Emit packet tracking summaries
        self.packet_tracker.emit_flood_summaries();

        // Flush trace
        self.trace.flush()?;

        Ok(self.stats.clone())
    }

    /// Update statistics based on event type.
    fn update_stats(&mut self, event: &Event) {
        match &event.payload {
            EventPayload::TransmitAir(tx) => {
                self.stats.packets_transmitted += 1;

                // Track per-node TX
                if let Some(stats) = self.node_stats.get_mut(&tx.radio_id.0) {
                    stats.tx += 1;
                }

                // Track packet for delivery metrics
                // Try to parse packet to determine if flood vs direct
                if let Ok(packet) = meshcore_packet::MeshCorePacket::decode(&tx.packet.payload) {
                    let origin_time = event.time.as_micros();

                    // For direct packets, try to extract destination from payload
                    let destination = if !packet.is_flood() {
                        // For direct messages, we'd need to extract destination from encrypted header
                        // For now, we'll just track them without specific destination
                        None
                    } else {
                        None
                    };

                    self.packet_tracker.track_send(&packet, destination, origin_time);
                }
            }
            EventPayload::RadioRxPacket(rx) => {
                // Track per-node RX/collision for each target (firmware entities)
                // Map firmware entity IDs to radio entity IDs
                for target in &event.targets {
                    // target is a firmware entity ID, map to radio entity ID
                    if let Some(&radio_id) = self.firmware_to_radio.get(&target.0) {
                        if let Some(stats) = self.node_stats.get_mut(&radio_id) {
                            if rx.was_collided {
                                stats.collisions += 1;
                            } else {
                                stats.rx += 1;
                            }
                        }

                        // Track packet reception for non-collided packets
                        if !rx.was_collided {
                            if let Ok(packet) = meshcore_packet::MeshCorePacket::decode(&rx.packet.payload) {
                                let receive_time = event.time.as_micros();

                                // Get receiver node name
                                if let Some(node_name) = self.radio_to_name.get(&radio_id) {
                                    self.packet_tracker.track_reception(
                                        &packet,
                                        node_name,
                                        receive_time,
                                    );
                                }
                            }
                        }
                    }
                }
                // Update global stats
                if rx.was_collided {
                    self.stats.packets_collided += 1;
                } else {
                    self.stats.packets_received += 1;
                }
            }
            EventPayload::MessageSend(_) => {
                self.stats.messages_sent += 1;
            }
            EventPayload::MessageAcknowledged(_) => {
                self.stats.messages_acked += 1;
            }
            _ => {}
        }
    }

    /// Extract packet data from a LoraPacket for trace output.
    ///
    /// # Arguments
    /// * `packet` - The LoRa packet to extract data from
    ///
    /// # Returns
    /// A tuple containing:
    /// * `Option<String>` - Hex-encoded raw packet payload (always Some for valid packets)
    /// * `Option<serde_json::Value>` - Decoded MeshCore packet as JSON (Some if decode succeeds)
    fn extract_packet_data(packet: &mcsim_common::LoraPacket) -> (Option<String>, Option<serde_json::Value>) {
        // Encode raw payload as hex
        let hex = hex::encode(&packet.payload);
        // Try to decode the packet
        let json = packet.decoded()
            .and_then(|p| serde_json::to_value(p).ok());
        (Some(hex), json)
    }

    /// Record a trace entry for an event.
    fn record_trace(&mut self, event: &Event) {
        // Convert simulation time to ISO 8601 timestamp
        // Using a base time of 2025-01-01T00:00:00Z for simulation start
        let sim_secs = event.time.as_secs_f64();
        let timestamp = format!(
            "2025-01-01T{:02}:{:02}:{:06.3}Z",
            (sim_secs / 3600.0) as u32 % 24,
            (sim_secs / 60.0) as u32 % 60,
            sim_secs % 60.0
        );

        // Extract packet data if available
        let (packet_hex, packet_json) = match &event.payload {
            EventPayload::TransmitAir(tx) => Self::extract_packet_data(&tx.packet),
            EventPayload::RadioRxPacket(rx) => Self::extract_packet_data(&rx.packet),
            _ => (None, None),
        };

        // Determine reception status for RX packets
        let reception_status = match &event.payload {
            EventPayload::RadioRxPacket(rx) => {
                if rx.was_collided {
                    Some("collided".to_string())
                } else if rx.was_weak_signal {
                    Some("weak".to_string())
                } else {
                    Some("ok".to_string())
                }
            },
            _ => None,
        };

        // Extract packet timing information for TX and RX events
        let (packet_start_time_s, packet_end_time_s) = match &event.payload {
            EventPayload::TransmitAir(tx) => {
                // Start time is the event time, end time is from the event
                (Some(event.time.as_secs_f64()), Some(tx.end_time.as_secs_f64()))
            },
            EventPayload::RadioRxPacket(rx) => {
                // Start and end times are from the reception event
                (Some(rx.start_time.as_secs_f64()), Some(rx.end_time.as_secs_f64()))
            },
            _ => (None, None),
        };

        let (entry_type, direction, snr, rssi) = match &event.payload {
            EventPayload::TransmitAir(tx) => (
                "PACKET".to_string(),
                "TX".to_string(),
                "N/A".to_string(),
                format!("{} dBm", tx.params.tx_power_dbm),
            ),
            EventPayload::RadioRxPacket(rx) => (
                "PACKET".to_string(),
                "RX".to_string(),
                format!("{:.1} dB", rx.snr_db),
                format!("{:.1} dBm", rx.rssi_dbm),
            ),
            EventPayload::MessageSend(msg) => (
                "MESSAGE".to_string(),
                "TX".to_string(),
                "N/A".to_string(),
                format!("dest={:?}", msg.destination),
            ),
            EventPayload::MessageReceived(_) => (
                "MESSAGE".to_string(),
                "RX".to_string(),
                "N/A".to_string(),
                "N/A".to_string(),
            ),
            EventPayload::Timer { timer_id } => (
                format!("TIMER({})", timer_id),
                "INT".to_string(),
                "N/A".to_string(),
                "N/A".to_string(),
            ),
            _ => return, // Don't record other event types
        };

        let entry = TraceEntry {
            origin: format!("Entity_{}", event.source.0),
            origin_id: format!("{}", event.source.0),
            timestamp,
            entry_type,
            direction,
            snr,
            rssi,
            packet_hex,
            packet: packet_json,
            reception_status,
            packet_start_time_s,
            packet_end_time_s,
        };

        self.trace.record(entry);
    }
}

/// Create a new event loop from a built simulation.
pub fn create_event_loop(
    simulation: BuiltSimulation,
    seed: u64,
) -> EventLoop {
    EventLoop::new(
        simulation,
        seed,
        None,
        None,
        None,
        EntityTracer::disabled(),
    )
}

// Re-export key types for convenience
pub use mcsim_model::{build_simulation, load_model, load_model_from_str, BuiltSimulation as SimulationBuild, ModelLoader};
