# Per-Node Threading Architecture

This document describes the plan to refactor the MCSim threading architecture to use per-node threads with internal event loops, reducing overhead by eliminating unnecessary cross-thread event traffic.

## Problem Statement

### Current Architecture

The current architecture routes all events through a central event loop:

```
┌──────────────────────────────────────────────────────────────────────────┐
│                        Main Event Loop (Runner)                           │
│                                                                          │
│   ┌─────────────────────────────────────────────────────────────────┐    │
│   │                     Global Event Queue                           │    │
│   │  • Firmware → Radio events                                      │    │
│   │  • Radio → Firmware events                                      │    │
│   │  • Agent → Firmware events                                      │    │
│   │  • Firmware → Agent events                                      │    │
│   │  • Radio → Graph → Radio events (air propagation)               │    │
│   │  • Timer events                                                 │    │
│   │  • UART events (TCP ↔ Firmware)                                 │    │
│   └─────────────────────────────────────────────────────────────────┘    │
│                              │                                           │
│                              ▼                                           │
│   ┌─────────────────────────────────────────────────────────────────┐    │
│   │              Entity Dispatch (Sequential)                        │    │
│   │  • handle_event() on each target entity                         │    │
│   │  • Firmware entities step their DLL thread                       │    │
│   └─────────────────────────────────────────────────────────────────┘    │
│                                                                          │
└──────────────────────────────────────────────────────────────────────────┘
```

**Problems:**

1. **Excessive Event Traffic**: Events between co-located entities (firmware ↔ radio, firmware ↔ agent) flow through the global queue unnecessarily.

2. **Non-Local UART Handling**: TCP/UART events must flow through the main event loop even though they are non-deterministic and node-local.

3. **Two-Thread Model Per Node**: Firmware runs on a separate DLL thread that gets stepped, while entities are dispatched on the main thread. This adds synchronization overhead.

4. **Sequential Dispatch**: All entity event handling happens sequentially on the main thread, even for events that only affect a single node.

### Event Traffic Analysis

For a typical node with firmware, radio, and agent:

| Event Type | Current Path | Node-Local? | Global Visibility? |
|------------|--------------|-------------|-------------------|
| RadioTxRequest | Firmware → Queue → Radio | Yes | No |
| RadioRxPacket | Radio → Queue → Firmware | Yes | No |
| RadioStateChanged | Radio → Queue → Firmware | Yes | No |
| SerialRx (from Agent) | Agent → Queue → Firmware | Yes | No |
| SerialTx (to Agent) | Firmware → Queue → Agent | Yes | No |
| SerialRx (from TCP) | TCP → Queue → Firmware | Yes | No |
| SerialTx (to TCP) | Firmware → Queue → TCP | Yes | No |
| Timer | Queue → Firmware/Radio/Agent | Yes | No |
| TransmitAir | Radio → Queue → Graph | No | **Yes** |
| ReceiveAir | Graph → Queue → Radio | No | **Yes** |

**Only TransmitAir and ReceiveAir events need global coordination** because they involve the propagation graph routing transmissions to receivers.

## Proposed Architecture

### Design Goals

1. **One Thread Per Node**: Each node runs all its components (firmware, radio, agent) on a single thread.

2. **Local Event Processing**: Intra-node events are processed locally without touching the global event queue.

3. **Minimal Global Coordination**: Only radio transmission events and time synchronization require global coordination.

4. **Direct TCP Handling**: UART/TCP connections are handled directly by node threads.

5. **Synchronous Firmware Stepping**: Firmware DLL execution happens synchronously within the node thread, eliminating the separate stepper thread.

### High-Level Architecture

```
┌────────────────────────────────────────────────────────────────────────────┐
│                     Main Coordinator Thread                                 │
│                                                                            │
│   ┌─────────────────────────────────────────────────────────────────┐      │
│   │                  Global Event Queue                              │      │
│   │  • TransmitAir events (radio transmissions)                     │      │
│   │  • Time advancement notifications                                │      │
│   │  • Simulation control (end, pause)                              │      │
│   └─────────────────────────────────────────────────────────────────┘      │
│                                                                            │
│   ┌─────────────────────────────────────────────────────────────────┐      │
│   │                  Graph Entity                                    │      │
│   │  • Routes TransmitAir → ReceiveAir based on link model          │      │
│   │  • Computes SNR/RSSI for each receiver                          │      │
│   └─────────────────────────────────────────────────────────────────┘      │
│                                                                            │
│   ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐         │
│   │  Node Channel A  │  │  Node Channel B  │  │  Node Channel C  │         │
│   │  (mpsc sender)   │  │  (mpsc sender)   │  │  (mpsc sender)   │  ...    │
│   └────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘         │
│            │                     │                     │                   │
└────────────┼─────────────────────┼─────────────────────┼───────────────────┘
             │                     │                     │
             ▼                     ▼                     ▼
┌────────────────────┐  ┌────────────────────┐  ┌────────────────────┐
│   Node A Thread    │  │   Node B Thread    │  │   Node C Thread    │
│                    │  │                    │  │                    │
│  ┌──────────────┐  │  │  ┌──────────────┐  │  │  ┌──────────────┐  │
│  │ Local Event  │  │  │  │ Local Event  │  │  │  │ Local Event  │  │
│  │    Queue     │  │  │  │    Queue     │  │  │  │    Queue     │  │
│  └──────┬───────┘  │  │  └──────┬───────┘  │  │  └──────┬───────┘  │
│         │          │  │         │          │  │         │          │
│  ┌──────▼───────┐  │  │  ┌──────▼───────┐  │  │  ┌──────▼───────┐  │
│  │   Firmware   │  │  │  │   Firmware   │  │  │  │   Firmware   │  │
│  │   (sync)     │  │  │  │   (sync)     │  │  │  │   (sync)     │  │
│  └──────┬───────┘  │  │  └──────┬───────┘  │  │  └──────┬───────┘  │
│         │          │  │         │          │  │         │          │
│  ┌──────▼───────┐  │  │  ┌──────▼───────┐  │  │  ┌──────▼───────┐  │
│  │    Radio     │  │  │  │    Radio     │  │  │  │    Radio     │  │
│  │   (local)    │  │  │  │   (local)    │  │  │  │   (local)    │  │
│  └──────┬───────┘  │  │  └──────┬───────┘  │  │  └──────┬───────┘  │
│         │          │  │         │          │  │         │          │
│  ┌──────▼───────┐  │  │  ┌──────▼───────┐  │  │  ┌──────▼───────┐  │
│  │    Agent     │  │  │  │    Agent     │  │  │  │    Agent     │  │
│  │  (optional)  │  │  │  │  (optional)  │  │  │  │  (optional)  │  │
│  └──────────────┘  │  │  └──────────────┘  │  │  └──────────────┘  │
│                    │  │                    │  │                    │
│  ┌──────────────┐  │  │  ┌──────────────┐  │  │  ┌──────────────┐  │
│  │  TCP Handler │  │  │  │  TCP Handler │  │  │  │  TCP Handler │  │
│  │   (async)    │  │  │  │   (async)    │  │  │  │   (async)    │  │
│  └──────────────┘  │  │  └──────────────┘  │  │  └──────────────┘  │
└────────────────────┘  └────────────────────┘  └────────────────────┘
```

### Communication Model

#### Coordinator → Node Communication

The coordinator sends commands to node threads via typed channels:

```rust
/// Commands from the coordinator to a node thread.
enum NodeCommand {
    /// Advance local time to the given value.
    /// Node should process all local events up to this time.
    AdvanceTime(SimTime),
    
    /// A radio packet is being received (from Graph entity).
    ReceiveAir(ReceiveAirEvent),
    
    /// Stop the node thread.
    Shutdown,
}
```

#### Node → Coordinator Communication

Nodes report events and status back to the coordinator. All data flows through channels to avoid mutex contention:

```rust
/// Reports from a node thread to the coordinator.
enum NodeReport {
    /// Node has processed all events up to the given time.
    /// Reports the next wake time (when the node has pending work).
    TimeReached {
        reached_time: SimTime,
        next_wake_time: Option<SimTime>,
        /// Trace events since last report (empty if tracing disabled for this node).
        trace_events: Vec<TraceEvent>,
    },
    
    /// Radio is transmitting a packet.
    TransmitAir(TransmitAirEvent),
    
    /// Node has shut down.
    Shutdown,
    
    /// Error occurred.
    Error(String),
}
```

### Metrics and Tracing Strategy

**Metrics**: Use the `metrics` crate directly from node threads. The crate uses lock-free atomics internally, making it more efficient than channeling batched metrics.

**Tracing**: Use batched trace events sent via channel, with per-node enable/disable controlled by the existing `--trace` flag:

```rust
/// Thread-local trace buffer that avoids allocation when tracing is disabled.
struct NodeTraceBuffer {
    enabled: bool,
    events: Vec<TraceEvent>,
}

impl NodeTraceBuffer {
    fn new(node_name: &str, trace_filter: &TraceFilter) -> Self {
        Self {
            enabled: trace_filter.is_enabled(node_name),
            events: Vec::new(),
        }
    }
    
    /// Record a trace event. No-op if tracing disabled - no allocation or formatting.
    #[inline]
    fn trace(&mut self, event: impl FnOnce() -> TraceEvent) {
        if self.enabled {
            self.events.push(event());
        }
    }
    
    /// Drain accumulated events. Returns empty vec if tracing disabled.
    fn drain(&mut self) -> Vec<TraceEvent> {
        std::mem::take(&mut self.events)
    }
}
```

Usage in node thread:

```rust
// Closure is never called if tracing disabled - zero overhead
local_tracer.trace(|| TraceEvent::FirmwareStep {
    time: current_time,
    trigger: format!("{:?}", event),  // Formatting only happens if enabled
    yield_reason: result.reason,
});
```
```

### Node Thread Internal Loop

Each node thread blocks waiting for input from either the coordinator or TCP connections. We use `crossbeam_channel::select!` to efficiently wait on multiple event sources without busy-waiting.

#### Unified Event Source

```rust
use crossbeam_channel::{self as channel, Receiver, Sender, select};

/// Events that can wake a node thread.
/// This unifies coordinator commands and external I/O into a single enum.
enum NodeWakeEvent {
    /// Command from the coordinator.
    Command(NodeCommand),
    /// Data received from TCP/UART connection.
    TcpData(Vec<u8>),
}
```

#### Blocking Select Loop

```rust
fn node_thread_main(
    config: NodeThreadConfig,
    cmd_rx: Receiver<NodeCommand>,
    report_tx: Sender<NodeReport>,
    uart_rx: Option<Receiver<Vec<u8>>>,  // TCP data receiver (from async task)
    uart_tx: Option<Sender<Vec<u8>>>,    // TCP data sender (to async task)
    trace_filter: &TraceFilter,           // Determines which nodes have tracing enabled
) {
    // Create local components
    let mut firmware = create_firmware(&config);
    let mut radio = create_radio(&config);
    let mut agent = config.agent.map(|a| create_agent(&a));
    
    // Local event queue (sorted by time)
    let mut local_queue: BinaryHeap<LocalEvent> = BinaryHeap::new();
    
    // Thread-local trace buffer (zero overhead when tracing disabled for this node)
    let mut local_tracer = NodeTraceBuffer::new(&config.name, trace_filter);
    
    // Current simulation time as observed by this node
    let mut current_time = SimTime::ZERO;
    
    loop {
        // Block until we receive either a coordinator command or TCP data.
        // The thread sleeps here until one of the channels has data.
        let wake_event = match &uart_rx {
            Some(tcp_rx) => {
                // Block on both coordinator commands AND TCP data
                select! {
                    recv(cmd_rx) -> cmd => {
                        match cmd {
                            Ok(c) => NodeWakeEvent::Command(c),
                            Err(_) => break, // Coordinator disconnected
                        }
                    }
                    recv(tcp_rx) -> data => {
                        match data {
                            Ok(d) => NodeWakeEvent::TcpData(d),
                            Err(_) => continue, // TCP disconnected, keep waiting for commands
                        }
                    }
                }
            }
            None => {
                // No TCP connection - just block on coordinator commands
                match cmd_rx.recv() {
                    Ok(cmd) => NodeWakeEvent::Command(cmd),
                    Err(_) => break, // Coordinator disconnected
                }
            }
        };
        
        // Process the wake event
        match wake_event {
            NodeWakeEvent::Command(NodeCommand::AdvanceTime(target_time)) => {
                // Process local events up to target_time
                while let Some(event) = local_queue.peek() {
                    if event.time > target_time {
                        break;
                    }
                    let event = local_queue.pop().unwrap();
                    process_local_event(&mut firmware, &mut radio, &mut agent, event, &mut local_queue);
                }
                
                current_time = target_time;
                
                // Step firmware at the new time
                step_firmware_sync(&mut firmware, current_time, &mut local_queue, &uart_tx);
                
                // Drain trace events (empty vec if tracing disabled - no allocation)
                let trace_events = local_tracer.drain();
                
                // Report completion with next wake time and traces
                let next_wake = local_queue.peek().map(|e| e.time);
                report_tx.send(NodeReport::TimeReached {
                    reached_time: current_time,
                    next_wake_time: next_wake,
                    trace_events,
                }).unwrap();
            }
            
            NodeWakeEvent::Command(NodeCommand::ReceiveAir(rx_event)) => {
                // Inject into local radio, which may generate RadioRxPacket
                radio.handle_receive_air(&rx_event, &mut local_queue);
            }
            
            NodeWakeEvent::Command(NodeCommand::Shutdown) => {
                report_tx.send(NodeReport::Shutdown).unwrap();
                break;
            }
            
            NodeWakeEvent::TcpData(data) => {
                // TCP data arrived - inject directly into firmware (non-deterministic)
                firmware.inject_serial_rx(&data);
                // Immediately step firmware to process the input
                step_firmware_immediate(&mut firmware, current_time, &uart_tx);
            }
        }
    }
}
```

### Local Event Processing

Events within a node are processed directly without going through the global queue:

```rust
/// Events that are processed locally within a node.
enum LocalEvent {
    /// Timer expired.
    Timer { timer_id: u64, time: SimTime },
    
    /// Radio has packet ready for firmware.
    RadioRxPacket { packet: LoraPacket, snr_db: f64, rssi_dbm: f64, time: SimTime },
    
    /// Radio state changed.
    RadioStateChanged { state: RadioState, time: SimTime },
    
    /// Serial data from agent to firmware.
    SerialRxFromAgent { data: Vec<u8>, time: SimTime },
    
    /// Firmware started transmitting (schedules air event).
    FirmwareTxStarted { packet: LoraPacket, time: SimTime },
}

fn process_local_event(
    firmware: &mut Firmware,
    radio: &mut Radio,
    agent: &mut Option<Agent>,
    event: LocalEvent,
    queue: &mut BinaryHeap<LocalEvent>,
) {
    match event {
        LocalEvent::Timer { timer_id, time } => {
            // Route to appropriate component based on timer_id ranges
            // ...
        }
        
        LocalEvent::RadioRxPacket { packet, snr_db, rssi_dbm, time } => {
            firmware.inject_radio_rx(&packet.payload, rssi_dbm as f32, snr_db as f32);
            // Agent also receives if present
            if let Some(agent) = agent {
                agent.handle_radio_rx(&packet, snr_db, rssi_dbm, time, queue);
            }
        }
        
        LocalEvent::RadioStateChanged { state, time } => {
            firmware.notify_state_change(state);
        }
        
        // ... other events
    }
}
```

### Synchronous Firmware Stepping

Instead of the current async step model with a separate thread, firmware is stepped synchronously:

```rust
fn step_firmware_sync(
    firmware: &mut Firmware,
    current_time: SimTime,
    local_queue: &mut BinaryHeap<LocalEvent>,
    uart_tx: &Option<Sender<Vec<u8>>>,
) {
    let millis = current_time.as_micros() / 1000;
    let rtc_secs = compute_rtc_secs(current_time);
    
    // Step the DLL synchronously (blocks until yield)
    let result = firmware.node.step_sync(millis, rtc_secs);
    
    // Process outputs
    match result.reason {
        YieldReason::Idle => {
            // Schedule wake timer
            if result.wake_millis > millis {
                let wake_time = SimTime::from_micros(result.wake_millis * 1000);
                local_queue.push(LocalEvent::Timer {
                    timer_id: FIRMWARE_WAKE_TIMER,
                    time: wake_time,
                });
            }
        }
        
        YieldReason::RadioTxStart => {
            // Firmware wants to transmit - notify radio
            if let Some((data, time_on_air)) = result.radio_tx_data {
                radio.handle_tx_request(data, time_on_air, local_queue);
            }
        }
        
        // ... other yield reasons
    }
    
    // Handle serial output
    if let Some(serial_data) = result.serial_tx_data {
        // Send to agent (if present)
        if let Some(agent) = agent {
            agent.handle_serial_rx(&serial_data, current_time, local_queue);
        }
        // Send to TCP (non-blocking send to async task)
        if let Some(tx) = uart_tx {
            let _ = tx.try_send(serial_data);
        }
    }
}

/// Step firmware immediately in response to TCP data (non-deterministic).
/// This does not generate timed events - only handles serial I/O.
fn step_firmware_immediate(
    firmware: &mut Firmware,
    current_time: SimTime,
    uart_tx: &Option<Sender<Vec<u8>>>,
) {
    let millis = current_time.as_micros() / 1000;
    let rtc_secs = compute_rtc_secs(current_time);
    
    let result = firmware.node.step_sync(millis, rtc_secs);
    
    // Only handle serial output - don't schedule timed events
    if let Some(serial_data) = result.serial_tx_data {
        if let Some(tx) = uart_tx {
            let _ = tx.try_send(serial_data);
        }
    }
    
    // Radio TX requests from TCP-triggered stepping are buffered
    // and will be processed on the next time advancement
    if result.radio_tx_data.is_some() {
        firmware.pending_tx_from_tcp = result.radio_tx_data;
    }
}
```

### Coordinator Time Advancement

The coordinator manages global time synchronization:

```rust
impl Coordinator {
    fn run(&mut self, duration: SimTime) {
        while self.current_time < duration {
            // 1. Collect next wake times from all nodes
            let next_global_time = self.calculate_next_global_time();
            
            // 2. Check for any transmissions that complete before next_global_time
            let next_event_time = self.event_queue
                .peek()
                .map(|e| e.time)
                .unwrap_or(SimTime::MAX);
            
            let advance_to = next_global_time.min(next_event_time);
            
            // 3. Process global events at advance_to time
            while let Some(event) = self.event_queue.peek() {
                if event.time > advance_to {
                    break;
                }
                let event = self.event_queue.pop().unwrap();
                self.process_global_event(event);
            }
            
            // 4. Advance all nodes to the new time (parallel)
            self.advance_nodes_to(advance_to);
            
            // 5. Collect reports (TransmitAir events, new wake times)
            self.collect_node_reports();
            
            self.current_time = advance_to;
        }
    }
    
    fn advance_nodes_to(&mut self, target_time: SimTime) {
        // Send AdvanceTime to all nodes in parallel
        for node_cmd in &self.node_cmd_channels {
            node_cmd.send(NodeCommand::AdvanceTime(target_time)).unwrap();
        }
        
        // Wait for all nodes to report TimeReached
        let mut pending = self.node_count;
        while pending > 0 {
            match self.report_rx.recv() {
                Ok(NodeReport::TimeReached { reached_time, next_wake_time }) => {
                    // Update wake time tracking
                    pending -= 1;
                }
                // ... handle other reports
            }
        }
    }
}
```

### TCP/UART Handling

TCP connections use a bridge pattern: an async task handles the actual TCP I/O and communicates with the node thread via channels. This allows the node thread to block efficiently on both coordinator commands and TCP data.

#### Architecture

```text
┌─────────────────────────────────────────────────────────────────────┐
│                        Node Thread                                   │
│                                                                     │
│   ┌───────────────────────────────────────────────────────────┐     │
│   │  crossbeam::select! blocking on:                          │     │
│   │    • cmd_rx (coordinator commands)                        │     │
│   │    • uart_rx (TCP data from async bridge)                 │     │
│   └───────────────────────────────────────────────────────────┘     │
│              │                              ▲                       │
│              │ uart_tx                      │ uart_rx               │
│              ▼                              │                       │
└──────────────┼──────────────────────────────┼───────────────────────┘
               │                              │
               │  crossbeam channels          │
               │  (thread-safe, blocking)     │
               ▼                              │
┌──────────────────────────────────────────────────────────────────────┐
│                     TCP Bridge Task (async)                          │
│                                                                     │
│   tokio::select! on:                                                │
│     • TcpStream read → send to uart_rx channel                      │
│     • uart_tx channel recv → write to TcpStream                     │
│                                                                     │
└──────────────────────────────────────────────────────────────────────┘
```

#### TCP Bridge Implementation

```rust
/// Runs as a tokio task, bridging TCP socket to/from the node thread.
async fn tcp_bridge_task(
    mut tcp_stream: TcpStream,
    uart_tx_to_node: crossbeam_channel::Sender<Vec<u8>>,  // Data TO node thread
    uart_rx_from_node: tokio::sync::mpsc::Receiver<Vec<u8>>,  // Data FROM node thread
) {
    let (tcp_read, mut tcp_write) = tcp_stream.split();
    let mut tcp_read = BufReader::new(tcp_read);
    let mut read_buf = vec![0u8; 4096];
    
    // Convert crossbeam sender to async-compatible
    // (crossbeam send is non-blocking, so this is fine)
    
    loop {
        tokio::select! {
            // TCP data arrived - forward to node thread
            result = tcp_read.read(&mut read_buf) => {
                match result {
                    Ok(0) => break, // Connection closed
                    Ok(n) => {
                        let data = read_buf[..n].to_vec();
                        // Send to node thread (wakes the blocking select!)
                        if uart_tx_to_node.send(data).is_err() {
                            break; // Node thread gone
                        }
                    }
                    Err(_) => break,
                }
            }
            
            // Data from node thread - send to TCP
            Some(data) = uart_rx_from_node.recv() => {
                if tcp_write.write_all(&data).await.is_err() {
                    break;
                }
            }
        }
    }
}
```

#### Channel Setup

```rust
fn create_node_with_uart(config: NodeThreadConfig, tcp_listener_port: u16) 
    -> (NodeThreadHandle, JoinHandle<()>) 
{
    // Channels for coordinator ↔ node communication
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let (report_tx, report_rx) = crossbeam_channel::unbounded();
    
    // Channels for TCP ↔ node communication
    // uart_rx: TCP bridge sends data TO node thread (crossbeam for blocking recv)
    // uart_tx: Node thread sends data TO TCP bridge (tokio mpsc for async recv)
    let (uart_data_tx, uart_data_rx) = crossbeam_channel::unbounded();
    let (uart_out_tx, uart_out_rx) = tokio::sync::mpsc::channel(256);
    
    // Spawn the node thread
    let node_thread = std::thread::spawn(move || {
        node_thread_main(
            config,
            cmd_rx,
            report_tx,
            Some(uart_data_rx),  // Receives TCP data
            Some(uart_out_tx),   // Sends data to TCP
        );
    });
    
    // Spawn TCP listener task (accepts connections, spawns bridge tasks)
    let tcp_task = tokio::spawn(async move {
        let listener = TcpListener::bind(("0.0.0.0", tcp_listener_port)).await.unwrap();
        while let Ok((stream, _)) = listener.accept().await {
            // For simplicity, only handle one connection at a time
            let uart_tx = uart_data_tx.clone();
            let uart_rx = uart_out_rx; // Move receiver to bridge
            tcp_bridge_task(stream, uart_tx, uart_rx).await;
        }
    });
    
    (NodeThreadHandle { cmd_tx, thread: node_thread }, tcp_task)
}
```

## Implementation Plan

### Phase 1: Node Thread Infrastructure

**Goal**: Create the per-node thread infrastructure without changing event routing.

**Tasks**:

1. Create `NodeThread` struct that owns firmware, radio, and agent for a node
2. Implement `NodeCommand` and `NodeReport` channel types
3. Create `Coordinator` that manages node threads
4. Wire up basic time advancement (AdvanceTime / TimeReached)

**Files**:
- Create `crates/mcsim-runner/src/node_thread.rs`
- Modify `crates/mcsim-runner/src/lib.rs` - Add NodeThread-based EventLoop variant

**Validation**: Run existing tests, verify same results

### Phase 2: Local Event Queue

**Goal**: Move intra-node events to local queues.

**Tasks**:

1. Implement `LocalEvent` enum and local event queue per node
2. Route firmware ↔ radio events locally
3. Route firmware ↔ agent events locally  
4. Keep TransmitAir/ReceiveAir going through coordinator

**Files**:
- Modify `crates/mcsim-runner/src/node_thread.rs`
- May need to modify radio/firmware entity interfaces

**Validation**: Run existing tests, verify same results (determinism preserved)

### Phase 3: Synchronous Firmware Stepping

**Goal**: Eliminate the separate firmware stepper thread.

**Tasks**:

1. Add `step_sync()` method to firmware DLL interface
2. Modify `NodeThread` to step firmware synchronously
3. Remove async stepping infrastructure

**Files**:
- Modify `simulator/common/include/sim_node_base.h`
- Modify `crates/mcsim-firmware/src/dll.rs`
- Modify `crates/mcsim-firmware/src/lib.rs`

**Validation**: Run existing tests, measure performance improvement

### Phase 4: Direct TCP Handling

**Goal**: Handle TCP/UART connections directly in node threads.

**Tasks**:

1. Move `UartHandle` into `NodeThread`
2. Check for TCP data in node thread loop
3. Handle TCP-triggered firmware steps (non-deterministic)
4. Remove TCP event flow through coordinator

**Files**:
- Modify `crates/mcsim-runner/src/node_thread.rs`
- Modify `crates/mcsim-runner/src/uart_server.rs`
- Remove UART handling from main event loop

**Validation**: Test with actual TCP connections

### Phase 5: Optimization and Cleanup

**Goal**: Optimize the new architecture and clean up legacy code.

**Tasks**:

1. Implement parallel node advancement (send AdvanceTime to all, wait for all)
2. Add wake time coalescing
3. Remove legacy sequential event dispatch code
4. Update documentation

**Files**:
- Modify `crates/mcsim-runner/src/lib.rs`
- Update `docs/TIMING_AND_DETERMINISM.md`

**Validation**: Performance benchmarks, verify determinism

## API Changes

### New Types

```rust
// crates/mcsim-runner/src/node_thread.rs

use crossbeam_channel::{Receiver, Sender};

/// Configuration for a node thread.
pub struct NodeThreadConfig {
    pub name: String,
    pub node_id: NodeId,
    pub firmware_config: FirmwareConfig,
    pub radio_config: RadioConfig,
    pub agent_config: Option<AgentConfig>,
    pub uart_port: Option<u16>,  // TCP port for UART bridge
}

/// Handle to a running node thread.
pub struct NodeThreadHandle {
    /// Send commands to the node thread.
    cmd_tx: Sender<NodeCommand>,
    /// Node name for debugging.
    name: String,
    /// Thread join handle.
    thread: JoinHandle<()>,
}

/// The coordinator that manages all node threads.
pub struct Coordinator {
    /// Handles to all node threads.
    nodes: Vec<NodeThreadHandle>,
    /// Receive reports from all nodes (single receiver, multiple senders).
    report_rx: Receiver<(usize, NodeReport)>,  // (node_index, report)
    /// Graph entity for routing transmissions.
    graph: Graph,
    /// Global event queue (TransmitAir events, transmission end times).
    event_queue: BinaryHeap<GlobalEvent>,
    /// Current simulation time.
    current_time: SimTime,
    /// Tracked next wake time per node.
    node_wake_times: Vec<Option<SimTime>>,
}
```

### New Dependencies

Add to `Cargo.toml`:

```toml
[dependencies]
crossbeam-channel = "0.5"  # For multi-producer select! blocking
```

### Modified Interfaces

The `Entity` trait and current entity implementations remain largely unchanged, but their `handle_event` methods will be called from node threads rather than the main event loop.

## Determinism Considerations

### Preserved Determinism

The new architecture preserves determinism for:
- **Radio propagation**: TransmitAir/ReceiveAir still go through the coordinator
- **Time advancement**: All nodes advance to the same global time
- **Event ordering**: Local events are processed in time order
- **RNG**: Each node has its own seeded RNG

### Non-Deterministic Paths

TCP/UART data is explicitly non-deterministic:
- Data arrives based on wall clock time
- Firmware steps triggered by TCP data happen outside the time model
- Radio transmissions from TCP-triggered steps are queued and included in the next time sync

This matches the current behavior where TCP connections break determinism.

## Performance Expectations

### Reduced Overhead

- **Event Queue Operations**: O(log n) per event → mostly eliminated for local events
- **Cross-Thread Synchronization**: Reduced from every event to only air events + time sync
- **Memory**: Slightly higher (local queues per node), but better cache locality

### Parallelism

- Nodes can process local events in parallel
- Only air events and time advancement require synchronization
- Expected speedup: 2-4x for simulations with many nodes and few collisions

## Migration Strategy

The new architecture can coexist with the old:

1. Add feature flag `per_node_threading`
2. Implement new `Coordinator`-based event loop
3. Run both in tests to verify identical results
4. Deprecate old `EventLoop` once validated
5. Remove legacy code

## Design Decisions

1. **Agent Placement**: Should agents always be on the same thread as their firmware, or could they optionally run on a separate thread for complex agents?

   **Decision**: Always on the same thread for performance reasons. Complex agents can be optimized later if needed.

2. **Metrics Collection**: How should metrics be aggregated from node threads?

   **Decision**: Use the `metrics` crate directly from node threads. The crate uses lock-free atomics internally (`AtomicU64::fetch_add` for counters), which is more efficient than channeling batched metrics data. No custom batching needed.

3. **Tracing**: How should the entity tracer work with per-node threads?

   **Decision**: Thread-local trace buffers with per-node enable/disable via the existing `--trace` flag. When tracing is disabled for a node, zero allocation/formatting occurs (the closure passed to `trace()` is never called). When enabled, trace events are batched and sent via channel when the node blocks.

   ```rust
   // Zero overhead when tracing disabled - closure never called
   local_tracer.trace(|| TraceEvent::FirmwareStep {
       time: current_time,
       trigger: format!("{:?}", event),
       yield_reason: result.reason,
   });
   ```

4. **Error Handling**: If a node thread panics, how should the coordinator respond?

   **Decision**: Propagate panic to coordinator. The simulation is invalid if any node fails, and we want to surface errors quickly for debugging. The coordinator should catch the panic, log diagnostic information, and terminate gracefully.

   ```rust
   impl NodeThreadHandle {
       /// Check if the node thread has panicked.
       fn check_health(&self) -> Result<(), NodePanic> {
           if self.thread.is_finished() {
               // Thread exited unexpectedly - likely panicked
               Err(NodePanic { node_name: self.name.clone() })
           } else {
               Ok(())
           }
       }
   }
   ```

## Testing Strategy

The refactor must maintain compatibility with all existing tests. `cargo test` currently passes and must continue to pass after each phase of the refactor.

### Existing Tests

| Test File | Description | Expected Changes |
|-----------|-------------|------------------|
| `crates/mcsim-runner/tests/determinism_test.rs` | Verifies same seed produces identical results | **Critical** - Must pass unchanged. This is the primary validation that the refactor preserves behavior. |
| `crates/mcsim-runner/tests/integration_test.rs` | Model loading, LoRa PHY, packet encoding, event ordering | **No changes** - Tests components below the EventLoop level. |
| `crates/mcsim-runner/tests/metrics_test.rs` | Metrics collection across various scenarios | **No changes** - Uses subprocess runner, metrics crate remains the same. |
| `crates/mcsim-dem/tests/integration.rs` | DEM/elevation data loading | **No changes** - Unrelated to event loop. |
| Unit tests in `mcsim-lora`, `mcsim-agents`, `mcsim-common` | Component-level tests | **No changes** - Test individual components, not the event loop. |

### Tests Requiring Modification

**`crates/mcsim-runner/src/lib.rs`** - If there are any unit tests that directly construct `EventLoop`, they may need updates to work with `Coordinator`:

```rust
// Before: Direct EventLoop construction
let mut event_loop = create_event_loop(simulation, seed);
event_loop.run(duration)?;

// After: Same API, but internally uses Coordinator
// The create_event_loop function should remain compatible
let mut event_loop = create_event_loop(simulation, seed);
event_loop.run(duration)?;
```

**Goal**: Keep the public API (`create_event_loop`, `EventLoop::run`) compatible so existing tests work unchanged.

### New Tests to Add

#### 1. Node Thread Unit Tests

**File**: `crates/mcsim-runner/src/node_thread.rs` (inline `#[cfg(test)]` module)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_node_thread_processes_advance_time() {
        // Create a minimal node thread with mock firmware
        // Send AdvanceTime command
        // Verify TimeReached response with correct time
    }
    
    #[test]
    fn test_node_thread_handles_receive_air() {
        // Create node thread
        // Send ReceiveAir command
        // Verify radio processes the packet
    }
    
    #[test]
    fn test_node_thread_blocks_on_empty_channels() {
        // Verify thread blocks (doesn't busy-wait) when no commands
        // Can test by checking CPU usage or timing
    }
    
    #[test]
    fn test_local_tracer_disabled_no_allocation() {
        // Create NodeTraceBuffer with enabled=false
        // Call trace() with closure that would panic if called
        // Verify closure is never invoked
    }
}
```

#### 2. Coordinator Unit Tests

**File**: `crates/mcsim-runner/src/coordinator.rs` (or inline in `lib.rs`)

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_coordinator_advances_time_to_all_nodes() {
        // Create coordinator with multiple mock node handles
        // Call advance_nodes_to()
        // Verify all nodes receive AdvanceTime command
    }
    
    #[test]
    fn test_coordinator_collects_reports_from_all_nodes() {
        // Verify coordinator waits for all TimeReached reports
    }
    
    #[test]
    fn test_coordinator_routes_transmit_air_through_graph() {
        // Node A transmits
        // Verify Graph receives TransmitAir
        // Verify Node B receives ReceiveAir
    }
    
    #[test]
    fn test_coordinator_detects_node_panic() {
        // Create node that panics
        // Verify coordinator detects and reports error
    }
}
```

#### 3. Determinism Comparison Test

**File**: `crates/mcsim-runner/tests/determinism_test.rs` (add new test)

```rust
/// Test that the new Coordinator produces identical results to the old EventLoop.
/// This test should be run during the migration and can be removed after.
#[test]
#[serial]
#[cfg(feature = "per_node_threading")]
fn test_coordinator_matches_event_loop() {
    let seed = 12345u64;
    let config_path = Path::new("tests/two_companions.yaml");
    let duration_secs = 30.0;
    
    // Run with old EventLoop
    let result_old = run_simulation_with_event_loop(config_path, seed, duration_secs);
    
    // Run with new Coordinator
    let result_new = run_simulation_with_coordinator(config_path, seed, duration_secs);
    
    assert_eq!(result_old, result_new, 
        "Coordinator must produce identical results to EventLoop");
}
```

#### 4. TCP/UART Integration Test

**File**: `crates/mcsim-runner/tests/uart_test.rs` (new file)

```rust
//! Tests for TCP/UART handling in per-node threading mode.

#[test]
fn test_tcp_data_wakes_node_thread() {
    // Start simulation with TCP enabled for a node
    // Connect TCP client
    // Send data
    // Verify firmware receives and responds
}

#[test]
fn test_tcp_data_does_not_affect_determinism_when_disconnected() {
    // Run simulation without TCP connections
    // Verify determinism is preserved
}
```

#### 5. Performance Benchmark Test

**File**: `crates/mcsim-runner/benches/threading_benchmark.rs` (using criterion)

```rust
use criterion::{criterion_group, criterion_main, Criterion};

fn benchmark_event_loop_vs_coordinator(c: &mut Criterion) {
    let mut group = c.benchmark_group("threading");
    
    group.bench_function("event_loop_10_nodes", |b| {
        b.iter(|| run_simulation_event_loop(10, 10.0))
    });
    
    group.bench_function("coordinator_10_nodes", |b| {
        b.iter(|| run_simulation_coordinator(10, 10.0))
    });
    
    group.bench_function("coordinator_100_nodes", |b| {
        b.iter(|| run_simulation_coordinator(100, 10.0))
    });
    
    group.finish();
}

criterion_group!(benches, benchmark_event_loop_vs_coordinator);
criterion_main!(benches);
```

### Test Execution Strategy

**During each phase of the refactor:**

1. Run `cargo test` - all existing tests must pass
2. Run the determinism test multiple times to catch intermittent issues
3. Run with `--release` to catch optimization-related issues

**Specific commands:**

```bash
# Full test suite
cargo test

# Determinism tests with extra runs
cargo test -p mcsim-runner determinism -- --test-threads=1

# Run integration tests with logging to debug issues
RUST_LOG=debug cargo test -p mcsim-runner integration

# Run benchmarks (after Phase 5)
cargo bench -p mcsim-runner
```

### Acceptance Criteria

Before merging each phase:

1. ✅ `cargo test` passes (all existing tests)
2. ✅ Determinism test passes 10 consecutive runs
3. ✅ No new warnings from `cargo clippy`
4. ✅ Performance benchmark shows no regression (Phases 1-4) or improvement (Phase 5)
