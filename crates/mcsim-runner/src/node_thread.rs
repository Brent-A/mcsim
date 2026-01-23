//! Per-node threading infrastructure for MCSim.
//!
//! This module provides the foundation for running each simulation node on its own thread,
//! with local event processing and channel-based communication with a central coordinator.
//!
//! ## Architecture
//!
//! Each node runs firmware, radio, and agent components on a single thread. Intra-node
//! events are processed locally without touching the global event queue. Only air events
//! (TransmitAir/ReceiveAir) and time synchronization require global coordination.
//!
//! ## Key Types
//!
//! - [`NodeThread`]: Owns firmware, radio, and agent for a single node
//! - [`NodeCommand`]: Commands sent from coordinator to node threads
//! - [`NodeReport`]: Reports sent from node threads back to coordinator
//! - [`Coordinator`]: Manages all node threads and global event routing
//!
//! ## Feature Flag
//!
//! This module is gated behind the `per_node_threading` feature flag to allow
//! incremental migration from the existing [`EventLoop`](crate::EventLoop).

use crossbeam_channel::{Receiver, Sender};
use mcsim_common::{
    EntityId, LoraPacket, RadioParams, ReceiveAirEvent, SimTime, TransmitAirEvent,
};
use std::collections::BinaryHeap;
use std::thread::{self, JoinHandle};

// ============================================================================
// Node Commands (Coordinator → Node)
// ============================================================================

/// Commands sent from the coordinator to a node thread.
#[derive(Debug)]
pub enum NodeCommand {
    /// Advance local time to the given value.
    /// 
    /// The node should process all local events up to this time and then report
    /// back with [`NodeReport::TimeReached`].
    AdvanceTime {
        /// Target simulation time to advance to.
        until: SimTime,
    },

    /// A radio packet is being received (from Graph entity via coordinator).
    /// 
    /// This is injected into the node's radio, which may generate a
    /// `RadioRxPacket` local event for the firmware.
    ReceiveAir(ReceiveAirEvent),

    /// Stop the node thread gracefully.
    /// 
    /// The node should clean up and report [`NodeReport::Shutdown`] before exiting.
    Shutdown,
}

// ============================================================================
// Node Reports (Node → Coordinator)
// ============================================================================

/// Trace event for debugging and logging.
/// 
/// These are batched and sent with [`NodeReport::TimeReached`] when tracing is enabled.
#[derive(Debug, Clone)]
pub struct TraceEvent {
    /// Simulation time when the event occurred.
    pub time: SimTime,
    /// Description of the event.
    pub description: String,
}

/// Reports sent from a node thread to the coordinator.
#[derive(Debug)]
pub enum NodeReport {
    /// Node has processed all events up to the given time.
    TimeReached {
        /// The simulation time that was reached.
        time: SimTime,
        /// The next time when this node has pending work, if any.
        /// 
        /// This allows the coordinator to efficiently schedule the next
        /// global time advancement without polling.
        next_wake_time: Option<SimTime>,
        /// Trace events since last report (empty if tracing disabled for this node).
        trace_events: Vec<TraceEvent>,
    },

    /// Radio is transmitting a packet.
    /// 
    /// The coordinator should route this through the Graph entity to determine
    /// which other nodes receive it.
    TransmitAir(TransmitAirEvent),

    /// Node has shut down successfully.
    Shutdown,

    /// Error occurred in the node thread.
    Error(String),
}

// ============================================================================
// Local Event Queue
// ============================================================================

/// Events that are processed locally within a node.
/// 
/// These events never leave the node thread - they are generated and consumed
/// locally, avoiding the overhead of the global event queue.
#[derive(Debug, Clone)]
pub struct LocalEvent {
    /// When this event should be processed.
    pub time: SimTime,
    /// The event payload.
    pub payload: LocalEventPayload,
}

/// Payload for local events within a node.
#[derive(Debug, Clone)]
pub enum LocalEventPayload {
    /// Timer expired (firmware wake, radio turnaround, etc.).
    Timer {
        /// Timer identifier.
        timer_id: u64,
    },

    /// Radio has packet ready for firmware.
    RadioRxPacket {
        /// The received packet.
        packet: LoraPacket,
        /// Source radio entity ID.
        source_radio_id: EntityId,
        /// Signal-to-noise ratio in dB.
        snr_db: f64,
        /// Received signal strength in dBm.
        rssi_dbm: f64,
        /// Whether the packet was damaged by collision.
        was_collided: bool,
    },

    /// Radio state changed (TX complete, RX ready, etc.).
    RadioStateChanged {
        /// The new radio state.
        state: mcsim_common::RadioState,
    },

    /// Serial data from agent to firmware.
    SerialRxFromAgent {
        /// The serial data.
        data: Vec<u8>,
    },

    /// Firmware started transmitting (schedules air event).
    FirmwareTxStarted {
        /// The packet being transmitted.
        packet: LoraPacket,
        /// Radio parameters for the transmission.
        params: RadioParams,
        /// When transmission will end.
        end_time: SimTime,
    },
}

impl PartialEq for LocalEvent {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time
    }
}

impl Eq for LocalEvent {}

impl PartialOrd for LocalEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LocalEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse ordering for min-heap (earliest time first)
        other.time.cmp(&self.time)
    }
}

// ============================================================================
// Node Thread Configuration
// ============================================================================

/// Configuration for a node thread.
#[derive(Debug, Clone)]
pub struct NodeThreadConfig {
    /// Human-readable name for this node.
    pub name: String,
    /// Index of this node in the coordinator's node list.
    pub node_index: usize,
    /// Entity ID of this node's firmware component.
    pub firmware_entity_id: EntityId,
    /// Entity ID of this node's radio component.
    pub radio_entity_id: EntityId,
    /// TCP port for UART bridge (if enabled).
    pub uart_port: Option<u16>,
    /// Whether tracing is enabled for this node.
    pub tracing_enabled: bool,
}

// ============================================================================
// Node Thread
// ============================================================================

/// A node thread that owns firmware, radio, and agent for a single simulation node.
/// 
/// This struct is the central piece of the per-node threading architecture.
/// Each node runs on its own thread, processing local events and communicating
/// with the coordinator via channels.
pub struct NodeThread {
    /// Configuration for this node.
    config: NodeThreadConfig,
    /// Local event queue (min-heap by time).
    local_queue: BinaryHeap<LocalEvent>,
    /// Current simulation time as observed by this node.
    current_time: SimTime,
    /// Trace events accumulated since last report.
    trace_events: Vec<TraceEvent>,
}

impl NodeThread {
    /// Create a new node thread with the given configuration.
    pub fn new(config: NodeThreadConfig) -> Self {
        Self {
            config,
            local_queue: BinaryHeap::new(),
            current_time: SimTime::ZERO,
            trace_events: Vec::new(),
        }
    }

    /// Get the node name.
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Get the current simulation time for this node.
    pub fn current_time(&self) -> SimTime {
        self.current_time
    }

    /// Record a trace event if tracing is enabled.
    #[inline]
    pub fn trace(&mut self, f: impl FnOnce() -> TraceEvent) {
        if self.config.tracing_enabled {
            self.trace_events.push(f());
        }
    }

    /// Get the next wake time (earliest event in local queue).
    pub fn next_wake_time(&self) -> Option<SimTime> {
        self.local_queue.peek().map(|e| e.time)
    }

    /// Process a command from the coordinator.
    /// 
    /// Returns a report to send back, or None if the thread should exit.
    pub fn handle_command(&mut self, cmd: NodeCommand, report_tx: &Sender<(usize, NodeReport)>) -> bool {
        match cmd {
            NodeCommand::AdvanceTime { until } => {
                // Process local events up to target time
                while let Some(event) = self.local_queue.peek() {
                    if event.time > until {
                        break;
                    }
                    let event = self.local_queue.pop().unwrap();
                    self.process_local_event(event, report_tx);
                }

                self.current_time = until;

                // Drain trace events and report completion
                let trace_events = std::mem::take(&mut self.trace_events);
                let report = NodeReport::TimeReached {
                    time: self.current_time,
                    next_wake_time: self.next_wake_time(),
                    trace_events,
                };
                let _ = report_tx.send((self.config.node_index, report));
                true
            }

            NodeCommand::ReceiveAir(rx_event) => {
                // Queue the reception for processing
                // In the full implementation, this would interact with the radio
                // to determine collision, SNR, etc.
                let local_event = LocalEvent {
                    time: rx_event.end_time,
                    payload: LocalEventPayload::RadioRxPacket {
                        packet: rx_event.packet,
                        source_radio_id: rx_event.source_radio_id,
                        snr_db: rx_event.mean_snr_db_at20dbm, // Simplified for now
                        rssi_dbm: rx_event.rssi_dbm,
                        was_collided: false, // Would be determined by radio
                    },
                };
                self.local_queue.push(local_event);
                true
            }

            NodeCommand::Shutdown => {
                let _ = report_tx.send((self.config.node_index, NodeReport::Shutdown));
                false
            }
        }
    }

    /// Process a local event.
    fn process_local_event(&mut self, event: LocalEvent, _report_tx: &Sender<(usize, NodeReport)>) {
        self.trace(|| TraceEvent {
            time: event.time,
            description: format!("Processing local event: {:?}", event.payload),
        });

        match event.payload {
            LocalEventPayload::Timer { timer_id: _ } => {
                // In full implementation: route to firmware or radio based on timer_id
            }
            LocalEventPayload::RadioRxPacket { .. } => {
                // In full implementation: inject into firmware
            }
            LocalEventPayload::RadioStateChanged { .. } => {
                // In full implementation: notify firmware of state change
            }
            LocalEventPayload::SerialRxFromAgent { .. } => {
                // In full implementation: inject serial data into firmware
            }
            LocalEventPayload::FirmwareTxStarted { .. } => {
                // In full implementation: handle radio transmission start
            }
        }
    }
}

// ============================================================================
// Node Thread Handle
// ============================================================================

/// Handle to a running node thread.
/// 
/// This is held by the coordinator to send commands and join the thread on shutdown.
pub struct NodeThreadHandle {
    /// Send commands to the node thread.
    cmd_tx: Sender<NodeCommand>,
    /// Node name for debugging.
    name: String,
    /// Thread join handle.
    thread: JoinHandle<()>,
}

impl NodeThreadHandle {
    /// Send a command to the node thread.
    pub fn send(&self, cmd: NodeCommand) -> Result<(), crossbeam_channel::SendError<NodeCommand>> {
        self.cmd_tx.send(cmd)
    }

    /// Get the node name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Check if the node thread has finished (potentially due to panic).
    pub fn is_finished(&self) -> bool {
        self.thread.is_finished()
    }

    /// Join the node thread, blocking until it exits.
    pub fn join(self) -> Result<(), Box<dyn std::any::Any + Send + 'static>> {
        self.thread.join()
    }
}

/// Spawn a node thread with the given configuration.
/// 
/// Returns a handle for the coordinator to communicate with the thread,
/// and the thread begins listening for commands immediately.
pub fn spawn_node_thread(
    config: NodeThreadConfig,
    report_tx: Sender<(usize, NodeReport)>,
) -> NodeThreadHandle {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let name = config.name.clone();

    let thread = thread::Builder::new()
        .name(format!("node-{}", config.name))
        .spawn(move || {
            node_thread_main(config, cmd_rx, report_tx);
        })
        .expect("Failed to spawn node thread");

    NodeThreadHandle { cmd_tx, name, thread }
}

/// Main function for a node thread.
/// 
/// Blocks waiting for commands from the coordinator and processes them
/// until receiving a [`NodeCommand::Shutdown`].
fn node_thread_main(
    config: NodeThreadConfig,
    cmd_rx: Receiver<NodeCommand>,
    report_tx: Sender<(usize, NodeReport)>,
) {
    let mut node = NodeThread::new(config);

    loop {
        // Block waiting for a command from the coordinator
        match cmd_rx.recv() {
            Ok(cmd) => {
                if !node.handle_command(cmd, &report_tx) {
                    break; // Shutdown requested
                }
            }
            Err(_) => {
                // Channel closed - coordinator has dropped us
                break;
            }
        }
    }
}

// ============================================================================
// Coordinator
// ============================================================================

/// Global event for the coordinator's event queue.
/// 
/// These are events that require global coordination, primarily air transmissions.
#[derive(Debug)]
pub struct GlobalEvent {
    /// When this event occurs.
    pub time: SimTime,
    /// The event payload.
    pub payload: GlobalEventPayload,
}

/// Payload for global events.
#[derive(Debug)]
pub enum GlobalEventPayload {
    /// A transmission will complete at this time.
    TransmissionEnd {
        /// The original transmit event.
        tx_event: TransmitAirEvent,
    },
}

impl PartialEq for GlobalEvent {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time
    }
}

impl Eq for GlobalEvent {}

impl PartialOrd for GlobalEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GlobalEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse ordering for min-heap (earliest time first)
        other.time.cmp(&self.time)
    }
}

/// The coordinator that manages all node threads and global event routing.
/// 
/// The coordinator is responsible for:
/// - Time synchronization across all nodes
/// - Routing air transmissions through the Graph entity
/// - Collecting reports from nodes
/// - Handling simulation-level control (start, stop, etc.)
pub struct Coordinator {
    /// Handles to all node threads.
    nodes: Vec<NodeThreadHandle>,
    /// Receive reports from all nodes (single receiver, multiple senders).
    report_rx: Receiver<(usize, NodeReport)>,
    /// Sender for reports (cloned to each node thread).
    report_tx: Sender<(usize, NodeReport)>,
    /// Global event queue (transmission completions, etc.).
    event_queue: BinaryHeap<GlobalEvent>,
    /// Current simulation time.
    current_time: SimTime,
    /// Tracked next wake time per node.
    node_wake_times: Vec<Option<SimTime>>,
}

impl Coordinator {
    /// Create a new coordinator with no nodes.
    pub fn new() -> Self {
        let (report_tx, report_rx) = crossbeam_channel::unbounded();
        Self {
            nodes: Vec::new(),
            report_rx,
            report_tx,
            event_queue: BinaryHeap::new(),
            current_time: SimTime::ZERO,
            node_wake_times: Vec::new(),
        }
    }

    /// Get the current simulation time.
    pub fn current_time(&self) -> SimTime {
        self.current_time
    }

    /// Get the number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Spawn and add a node thread.
    pub fn add_node(&mut self, config: NodeThreadConfig) {
        let node_index = self.nodes.len();
        let config = NodeThreadConfig {
            node_index,
            ..config
        };
        let handle = spawn_node_thread(config, self.report_tx.clone());
        self.nodes.push(handle);
        self.node_wake_times.push(None);
    }

    /// Calculate the next global time to advance to.
    /// 
    /// This is the minimum of:
    /// - The earliest node wake time
    /// - The earliest global event time
    fn calculate_next_time(&self) -> Option<SimTime> {
        let min_node_wake = self.node_wake_times.iter()
            .filter_map(|t| *t)
            .min();

        let min_global_event = self.event_queue.peek().map(|e| e.time);

        match (min_node_wake, min_global_event) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    /// Advance all nodes to the given time.
    /// 
    /// Sends `AdvanceTime` commands to all nodes and waits for all
    /// `TimeReached` reports.
    pub fn advance_to(&mut self, target_time: SimTime) -> Result<(), String> {
        // Send AdvanceTime to all nodes
        for node in &self.nodes {
            if let Err(e) = node.send(NodeCommand::AdvanceTime { until: target_time }) {
                return Err(format!("Failed to send AdvanceTime to {}: {}", node.name(), e));
            }
        }

        // Wait for all nodes to report TimeReached
        let mut pending = self.nodes.len();
        while pending > 0 {
            match self.report_rx.recv() {
                Ok((node_index, report)) => {
                    match report {
                        NodeReport::TimeReached { time: _, next_wake_time, trace_events: _ } => {
                            self.node_wake_times[node_index] = next_wake_time;
                            pending -= 1;
                        }
                        NodeReport::TransmitAir(tx_event) => {
                            // Queue the transmission end event
                            self.event_queue.push(GlobalEvent {
                                time: tx_event.end_time,
                                payload: GlobalEventPayload::TransmissionEnd { tx_event },
                            });
                        }
                        NodeReport::Error(msg) => {
                            return Err(format!("Node {} error: {}", node_index, msg));
                        }
                        NodeReport::Shutdown => {
                            return Err(format!("Node {} shut down unexpectedly", node_index));
                        }
                    }
                }
                Err(_) => {
                    return Err("Report channel closed unexpectedly".to_string());
                }
            }
        }

        self.current_time = target_time;
        Ok(())
    }

    /// Run the simulation for the specified duration.
    /// 
    /// This is a basic implementation that advances time step by step.
    /// A full implementation would integrate with the Graph entity for
    /// routing transmissions.
    pub fn run(&mut self, duration: SimTime) -> Result<(), String> {
        let end_time = duration;

        while self.current_time < end_time {
            // Calculate next time to advance to
            let next_time = self.calculate_next_time()
                .map(|t| t.min(end_time))
                .unwrap_or(end_time);

            if next_time <= self.current_time {
                // No progress possible - simulation is stuck
                break;
            }

            // Process any global events at the current time
            while let Some(event) = self.event_queue.peek() {
                if event.time > next_time {
                    break;
                }
                let _event = self.event_queue.pop().unwrap();
                // In full implementation: route TransmitAir through Graph
                // and send ReceiveAir to affected nodes
            }

            // Advance all nodes
            self.advance_to(next_time)?;
        }

        Ok(())
    }

    /// Shut down all node threads gracefully.
    pub fn shutdown(self) -> Result<(), String> {
        // Send shutdown to all nodes
        for node in &self.nodes {
            let _ = node.send(NodeCommand::Shutdown);
        }

        // Wait for all shutdown reports
        let mut pending = self.nodes.len();
        while pending > 0 {
            match self.report_rx.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok((_, NodeReport::Shutdown)) => {
                    pending -= 1;
                }
                Ok((_, _)) => {
                    // Ignore other reports during shutdown
                }
                Err(_) => {
                    // Timeout or channel closed
                    break;
                }
            }
        }

        // Join all threads
        let mut errors = Vec::new();
        for node in self.nodes {
            let name = node.name.clone();
            if let Err(e) = node.join() {
                errors.push(format!("Node {} panicked: {:?}", name, e));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

impl Default for Coordinator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_command_advance_time() {
        let (report_tx, report_rx) = crossbeam_channel::unbounded();
        let config = NodeThreadConfig {
            name: "test_node".to_string(),
            node_index: 0,
            firmware_entity_id: EntityId::new(1),
            radio_entity_id: EntityId::new(2),
            uart_port: None,
            tracing_enabled: false,
        };

        let mut node = NodeThread::new(config);
        let target_time = SimTime::from_millis(100);

        let running = node.handle_command(
            NodeCommand::AdvanceTime { until: target_time },
            &report_tx,
        );

        assert!(running);
        assert_eq!(node.current_time(), target_time);

        // Check the report
        let (idx, report) = report_rx.recv().unwrap();
        assert_eq!(idx, 0);
        match report {
            NodeReport::TimeReached { time, .. } => {
                assert_eq!(time, target_time);
            }
            _ => panic!("Expected TimeReached report"),
        }
    }

    #[test]
    fn test_node_command_shutdown() {
        let (report_tx, report_rx) = crossbeam_channel::unbounded();
        let config = NodeThreadConfig {
            name: "test_node".to_string(),
            node_index: 0,
            firmware_entity_id: EntityId::new(1),
            radio_entity_id: EntityId::new(2),
            uart_port: None,
            tracing_enabled: false,
        };

        let mut node = NodeThread::new(config);

        let running = node.handle_command(NodeCommand::Shutdown, &report_tx);

        assert!(!running);

        // Check the report
        let (_, report) = report_rx.recv().unwrap();
        match report {
            NodeReport::Shutdown => {}
            _ => panic!("Expected Shutdown report"),
        }
    }

    #[test]
    fn test_coordinator_basic() {
        let coordinator = Coordinator::new();
        assert_eq!(coordinator.node_count(), 0);
        assert_eq!(coordinator.current_time(), SimTime::ZERO);
    }

    #[test]
    fn test_coordinator_add_node() {
        let mut coordinator = Coordinator::new();

        let config = NodeThreadConfig {
            name: "node1".to_string(),
            node_index: 0, // Will be overwritten
            firmware_entity_id: EntityId::new(1),
            radio_entity_id: EntityId::new(2),
            uart_port: None,
            tracing_enabled: false,
        };

        coordinator.add_node(config);
        assert_eq!(coordinator.node_count(), 1);

        // Clean shutdown
        coordinator.shutdown().expect("Shutdown should succeed");
    }

    #[test]
    fn test_coordinator_advance_time() {
        let mut coordinator = Coordinator::new();

        let config = NodeThreadConfig {
            name: "node1".to_string(),
            node_index: 0,
            firmware_entity_id: EntityId::new(1),
            radio_entity_id: EntityId::new(2),
            uart_port: None,
            tracing_enabled: false,
        };

        coordinator.add_node(config);

        // Advance time
        let target = SimTime::from_millis(100);
        coordinator.advance_to(target).expect("Advance should succeed");
        assert_eq!(coordinator.current_time(), target);

        // Clean shutdown
        coordinator.shutdown().expect("Shutdown should succeed");
    }

    #[test]
    fn test_local_event_ordering() {
        let mut heap = BinaryHeap::new();

        heap.push(LocalEvent {
            time: SimTime::from_millis(100),
            payload: LocalEventPayload::Timer { timer_id: 1 },
        });
        heap.push(LocalEvent {
            time: SimTime::from_millis(50),
            payload: LocalEventPayload::Timer { timer_id: 2 },
        });
        heap.push(LocalEvent {
            time: SimTime::from_millis(150),
            payload: LocalEventPayload::Timer { timer_id: 3 },
        });

        // Should pop in time order (earliest first)
        assert_eq!(heap.pop().unwrap().time, SimTime::from_millis(50));
        assert_eq!(heap.pop().unwrap().time, SimTime::from_millis(100));
        assert_eq!(heap.pop().unwrap().time, SimTime::from_millis(150));
    }

    #[test]
    fn test_trace_disabled_no_allocation() {
        let config = NodeThreadConfig {
            name: "test".to_string(),
            node_index: 0,
            firmware_entity_id: EntityId::new(1),
            radio_entity_id: EntityId::new(2),
            uart_port: None,
            tracing_enabled: false,
        };

        let mut node = NodeThread::new(config);

        // This closure should never be called when tracing is disabled
        node.trace(|| panic!("Closure should not be called when tracing is disabled"));

        // Trace events should be empty
        assert!(node.trace_events.is_empty());
    }

    #[test]
    fn test_trace_enabled() {
        let config = NodeThreadConfig {
            name: "test".to_string(),
            node_index: 0,
            firmware_entity_id: EntityId::new(1),
            radio_entity_id: EntityId::new(2),
            uart_port: None,
            tracing_enabled: true,
        };

        let mut node = NodeThread::new(config);

        node.trace(|| TraceEvent {
            time: SimTime::from_millis(100),
            description: "Test event".to_string(),
        });

        assert_eq!(node.trace_events.len(), 1);
        assert_eq!(node.trace_events[0].description, "Test event");
    }
}
