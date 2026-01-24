# Radio Emulation Architecture

This document describes how MCSim emulates LoRa radio hardware for the simulated MeshCore firmware, including packet collision detection and time-synchronized execution across multiple nodes.

## Overview

MCSim provides a radio emulation layer that allows the MeshCore firmware to run unmodified while believing it is communicating with real radio hardware. The emulation achieves this through three key mechanisms:

1. **Hardware-like Radio Interface** - A C++ `SimRadio` class that implements the `mesh::Radio` interface
2. **Collision Detection** - Physical layer simulation of overlapping LoRa packets
3. **Time Lockstep Execution** - Synchronized advancement of simulation time across all nodes

## 1. Radio Emulation Layers

The radio emulation is split into two distinct layers that communicate across the Rust/C++ boundary:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Coordinator (Rust - mcsim-runner)                        │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │                         Radio Entity                                    ││
│  │  • Exists outside the firmware DLL                                      ││
│  │  • Sends TransmitAir events to Graph Entity                             ││
│  │  • Receives ReceiveAir events from Graph Entity                         ││
│  │  • Handles collision detection for incoming packets                     ││
│  │  • Decides whether a packet can be decoded (SNR, collisions)            ││
│  │  • Sends state updates to Firmware Entity via events                    ││
│  └─────────────────────────────────────────────────────────────────────────┘│
│                                    │                                        │
│                            Events (Rust)                                    │
│                                    ▼                                        │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │                       Firmware Entity                                   ││
│  │  • Receives events from Radio Entity                                    ││
│  │  • Translates events to C API calls (sim_inject_*, sim_notify_*)        ││
│  │  • Steps the DLL and interprets yield results                           ││
│  │  • Sends PacketSend events back to Radio Entity                         ││
│  └─────────────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                          C API (FFI boundary)
                     sim_inject_radio_rx()
                     sim_notify_tx_complete()
                     sim_notify_state_change()
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Firmware DLL (C++ - meshcore_*.dll)                      │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │                       Radio Simulation (SimRadio)                       ││
│  │  • Exists inside the firmware DLL                                       ││
│  │  • Services mesh::Radio API calls from firmware                         ││
│  │  • Maintains radio state machine (RX/TX/Transition)                     ││
│  │  • Yields simulation thread when transmitting or spinning               ││
│  │  • Queues injected packets for firmware to receive                      ││
│  └─────────────────────────────────────────────────────────────────────────┘│
│                                    │                                        │
│                           mesh::Radio API                                   │
│                                    ▼                                        │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │                         MeshCore Firmware                               ││
│  │  • Unmodified MeshCore code (Dispatcher, Mesh, etc.)                    ││
│  │  • Calls radio.startSendRaw(), radio.recvRaw(), etc.                    ││
│  └─────────────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────────────┘
```

### Event Flow Between Entities

The Radio Entity, Firmware Entity, and Graph Entity are separate event-driven entities in the simulation. They communicate through a minimal set of events:

```
┌──────────────┐                        ┌──────────────────┐       ┌─────────────────┐
│ Radio Entity │                        │ Firmware Entity  │       │  Graph Entity   │
│   (Node A)   │                        │    (Node A)      │       │                 │
└──────┬───────┘                        └────────┬─────────┘       └────────┬────────┘
       │                                         │                          │
       │◄─────── RadioTxRequest ─────────────────┤                          │
       │         {packet}                        │                          │
       │                                         │                          │
       ├──────── RadioStateChanged(Transmitting)►│                          │
       │                                         │                          │
       ├═══════════════════ TransmitAir ═════════════════════════════════════►
       │                    {packet, params, end_time}                      │
       │                                         │                          │
       │                                         │      Graph routes to     │
       │                                         │      receivers with      │
       │                                         │      SNR/RSSI from       │
       │                                         │      link model          │
       │◄═══════════════════ ReceiveAir ═════════════════════════════════════┤
       │         {packet, params, snr, rssi, end_time}                      │
       │                                         │                          │
       │  ... (collision detection, SNR) ...     │                          │
       │                                         │                          │
       ├──────── RadioRxPacket ─────────────────►│                          │
       │         {packet, snr, rssi, collided}   │                          │
       │                                         │                          │
```

### Radio Events

The events are separated into inter-radio communication and radio-to-firmware communication:

**Radio → Graph Entity Events** (Radio Entity → Graph Entity):

```rust
/// Radio parameters for LoRa transmission.
pub struct RadioParams {
    /// Frequency in Hz.
    pub frequency_hz: u32,
    /// Bandwidth in Hz.
    pub bandwidth_hz: u32,
    /// Spreading factor (7-12).
    pub spreading_factor: u8,
    /// Coding rate (5-8, representing 4/5 to 4/8).
    pub coding_rate: u8,
    /// Transmit power in dBm.
    pub tx_power_dbm: i8,
}

/// A radio started transmitting - sent to Graph entity.
/// Graph entity routes this to appropriate receivers as ReceiveAirEvents.
pub struct TransmitAirEvent {
    pub radio_id: EntityId,
    pub packet: LoraPacket,
    pub params: RadioParams,
    pub end_time: SimTime,
}
```

**Graph Entity → Radio Events** (Graph Entity → Receiving Radio Entities):

```rust
/// Packet being received - sent from Graph entity to receiving radios.
/// Contains SNR/RSSI based on link model edge information.
pub struct ReceiveAirEvent {
    pub source_radio_id: EntityId,
    pub packet: LoraPacket,
    pub params: RadioParams,
    pub end_time: SimTime,
    pub snr_db: f64,
    pub rssi_dbm: f64,
}
```

Note: The Graph entity receives `TransmitAirEvent`s and converts them to `ReceiveAirEvent`s
for each receiver based on the link model. This allows centralized control over:
1. Which receivers are in range
2. SNR/RSSI calculation based on link parameters
3. Collision detection across the network

**Radio → Firmware Events** (Radio Entity → Firmware Entity):

```rust
/// Radio has a packet for firmware (after collision/SNR filtering)
pub struct RadioRxPacketEvent {
    pub packet: LoraPacket,
    pub snr_db: f64,
    pub rssi_dbm: f64,
    pub was_collided: bool,
}

/// Radio state machine changed - covers all state transitions
pub struct RadioStateChangedEvent {
    pub new_state: RadioState,
    pub state_version: u32,
}
```

The `RadioStateChangedEvent` handles all state transitions including:
- TX started (state → TRANSMITTING)
- TX complete (state → RX_TURNAROUND or RECEIVING)  
- Turnaround complete (state → RECEIVING or TRANSMITTING)

The firmware determines what happened by examining `new_state`. No separate `RadioTxStartedEvent` or `RadioTxCompleteEvent` is needed.

**Firmware → Radio Events** (Firmware Entity → Radio Entity):

```rust
/// Firmware requests radio to transmit a packet
pub struct RadioTxRequestEvent {
    pub packet: LoraPacket,
}
```

### Updated EventPayload Enum

```rust
pub enum EventPayload {
    // =========== Radio Layer Events ===========
    /// A radio started transmitting (directed to Graph entity)
    TransmitAir(TransmitAirEvent),
    /// A packet is being received (from Graph entity to receiver)
    ReceiveAir(ReceiveAirEvent),
    
    // =========== Radio → Firmware Events ===========
    /// Radio has a packet for firmware
    RadioRxPacket(RadioRxPacketEvent),
    /// Radio state changed (TX start, TX complete, RX ready, etc.)
    RadioStateChanged(RadioStateChangedEvent),
    
    // =========== Firmware → Radio Events ===========
    /// Firmware requests transmission
    RadioTxRequest(RadioTxRequestEvent),
    
    // =========== Agent Layer Events ===========
    /// Request to send a message
    MessageSend(MessageSendEvent),
    /// A message was received
    MessageReceived(MessageReceivedEvent),
    /// A message was acknowledged
    MessageAcknowledged(MessageAckedEvent),

    // =========== Scheduling ===========
    Timer { timer_id: u64 },
    
    // =========== Simulation Control ===========
    SimulationEnd,
}
```

### Event Flow Examples

**Transmission Flow:**

```
1. Firmware calls radio.startSendRaw(data, len)
   └─► SimRadio yields with SIM_YIELD_RADIO_TX_START
   
2. Firmware Entity receives yield, creates RadioTxRequest event
   └─► Posts RadioTxRequest to attached Radio Entity
   
3. Radio Entity receives RadioTxRequest:
   a. Calculates airtime (end_time) and turnaround time
   b. Posts RadioStateChanged(Transmitting) to Firmware Entity
   c. Posts TransmitAir to Graph Entity
   d. Schedules internal timer for end_time + turnaround
   
4. Graph Entity receives TransmitAir:
   a. Looks up all receivers from link model
   b. For each receiver, creates ReceiveAir with SNR/RSSI from link edge
   c. Posts ReceiveAir events to receiving Radio Entities
   
5. Firmware Entity receives RadioStateChanged(Transmitting):
   └─► Calls sim_step() to resume firmware (knows TX is in progress)
   
6. Radio Entity timer fires (at end_time + turnaround):
   └─► Posts RadioStateChanged(Receiving) to Firmware Entity
   (Turnaround time handled internally, firmware just sees back to Receiving)
   
7. Firmware Entity receives RadioStateChanged(Receiving):
   └─► Calls sim_notify_state_change() on DLL, wakes polling firmware
```

**Reception Flow:**

```
1. Radio Entity A posts TransmitAir to Graph Entity
   └─► Graph Entity routes to receivers as ReceiveAir events
   
2. Radio Entity B receives ReceiveAir:
   a. Adds to active_transmissions list (with SNR/RSSI from event)
   b. Schedules internal RxCompleteTimer for end_time
   c. Checks for collisions with other active TXs
   
3. Radio Entity B timer fires (at end_time):
   a. Removes from active_transmissions
   b. Final collision check (late collisions)
   c. Checks if packet survived (no collision, SNR ok, queue space)
   d. If survived: Posts RadioRxPacket to Firmware Entity B
   
4. Firmware Entity B receives RadioRxPacket:
   a. Calls sim_inject_radio_rx() on DLL
   b. Increments state_version (packet arrival is a state change)
   └─► Wakes any spinning firmware waiting for packets
```

### Radio Entity (Rust)

The Radio Entity lives in the coordinator and is responsible for:

1. **Transmission initiation** - Sends `TransmitAir` events to the Graph Entity when transmitting
2. **Reception handling** - Receives `ReceiveAir` events from the Graph Entity with SNR/RSSI
3. **Collision detection** - Tracks all active receptions and determines if overlapping packets collide
4. **Reception decisions** - Based on SNR, RSSI, and collision state, decides if a packet can be decoded
5. **State synchronization** - Notifies the Radio Simulation when state changes occur (TX complete, packet arrived)

### Radio Simulation (C++)

The Radio Simulation (`SimRadio`) lives inside the firmware DLL and is responsible for:

1. **API implementation** - Implements the `mesh::Radio` interface that firmware code calls
2. **State mirroring** - Receives state updates from Radio Entity via `sim_notify_state_change()` and reports current state to firmware APIs
3. **Yield control** - Yields the simulation thread when TX starts or when polling APIs detect spinning
4. **Packet queuing** - Buffers injected RX packets until firmware calls `recvRaw()`, drops packets when FIFO is full

### Radio State Machine

The Radio Entity maintains an internal state machine. Only two states are visible to firmware:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Firmware-Visible States                          │
│  ┌──────────────────┐                    ┌──────────────────┐       │
│  │                  │   RadioTxRequest   │                  │       │
│  │    RECEIVING     │ ─────────────────► │   TRANSMITTING   │       │
│  │                  │                    │                  │       │
│  │                  │ ◄───────────────── │                  │       │
│  └──────────────────┘   TX complete      └──────────────────┘       │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│                 Radio Entity Internal States                        │
│  (invisible to firmware - handled via timing delays)                │
│                                                                     │
│  TX_TURNAROUND: Delay between RadioTxRequest and actual TX start    │
│  RX_TURNAROUND: Delay between TX end and RECEIVING state            │
│                                                                     │
│  These delays affect:                                               │
│  - When TransmitAir is sent to Graph entity                         │
│  - When RadioStateChanged(RECEIVING) is sent to firmware            │
│  - Packets arriving during turnaround are queued, not lost          │
└─────────────────────────────────────────────────────────────────────┘
```

```rust
/// States visible to firmware via RadioStateChangedEvent
pub enum RadioState {
    Receiving,     // Ready to receive packets, can start TX
    Transmitting,  // Actively transmitting, cannot receive
}
```

The turnaround times are modeled internally by the Radio Entity:
- **TX_TURNAROUND**: Radio Entity waits before sending `TransmitAir` and `RadioStateChanged(Transmitting)`
- **RX_TURNAROUND**: Radio Entity waits after TX ends before sending `RadioStateChanged(Receiving)`

This keeps the firmware interface simple while still accurately modeling radio timing behavior.

### Turnaround Time Configuration

Real radios have a non-zero switching time between RX and TX modes. This affects:
- When a node can start transmitting after receiving
- When a node can receive after finishing a transmission
- Potential for missing packets during transitions


```rust
// Radio Entity configuration (Rust)
pub struct RadioParams {
    // ... existing fields ...
    
    /// Time to switch from RX to TX mode
    pub rx_to_tx_turnaround: SimTime,
    /// Time to switch from TX to RX mode  
    pub tx_to_rx_turnaround: SimTime,
}

impl Default for RadioParams {
    fn default() -> Self {
        Self {
            // ... existing defaults ...
            rx_to_tx_turnaround: SimTime::from_micros(100),  // ~100μs typical
            tx_to_rx_turnaround: SimTime::from_micros(100),
        }
    }
}
```

The receive queue depth is configured in the **Radio Simulation** (C++ side), since it manages the actual FIFO buffer:

```cpp
// Radio Simulation configuration (C++)
static constexpr size_t RX_QUEUE_DEPTH = 4;  // Typical radio FIFO depth
```

**Receive Queue Behavior:**

Real radio chips (like SX126x used by many LoRa devices) have limited FIFO buffer space. If packets arrive faster than the firmware processes them, the buffer overflows and packets are lost.

| Scenario | Behavior |
| --- | --- |
| Queue has space | Packet queued, firmware receives on `recvRaw()` |
| Queue full | Packet dropped (simulates FIFO overflow) |

This is important for testing scenarios where:

- A node is overwhelmed with traffic (e.g., many nodes transmitting simultaneously)
- Firmware processing is slow relative to packet arrival rate
- Stress testing mesh behavior under high load

The **Radio Simulation** (not Radio Entity) tracks the queue depth and drops packets when full. This is because only the Radio Simulation knows whether the firmware has consumed packets via `recvRaw()` - the Radio Entity has no visibility into firmware-side queue state after delivering a packet. Drops can optionally be reported back to the coordinator for trace analysis.

The Radio Entity schedules turnaround completion events and only marks the radio as ready for the next operation after the turnaround completes. During turnaround:
- Over-the-air packets arriving are lost (radio cannot receive during transition)
- TX requests from firmware are queued until turnaround completes

## 2. The SimRadio Class

### The SimRadio Class

The simulated firmware sees a radio interface that fully implements the `mesh::Radio` abstract class from MeshCore. This allows the firmware code to run without modification.

```cpp
// From simulator/common/include/sim_radio.h

class SimRadio : public mesh::Radio {
public:
    // Configuration
    void configure(float freq, float bw, uint8_t sf, uint8_t cr, uint8_t tx_power);
    
    // mesh::Radio interface
    void begin() override;
    int recvRaw(uint8_t* bytes, int sz) override;
    uint32_t getEstAirtimeFor(int len_bytes) override;
    float packetScore(float snr, int packet_len) override;
    bool startSendRaw(const uint8_t* bytes, int len) override;
    bool isSendComplete() override;
    void onSendFinished() override;
    bool isInRecvMode() const override;
    bool isReceiving() override;
    float getLastRSSI() const override;
    float getLastSNR() const override;
    int getNoiseFloor() const override;
    
    // Statistics interface
    uint32_t getPacketsRecv() const;
    uint32_t getPacketsSent() const;
    uint32_t getTotalTxAirtime() const;
    uint32_t getTotalRxAirtime() const;
};
```

### Polling APIs and Yield Behavior

Several radio APIs are typically called in a polling loop by firmware:

- `isSendComplete()` - Check if transmission finished
- `isInRecvMode()` - Check if radio is in receive mode  
- `isReceiving()` - Check if radio is currently receiving a packet

**The Problem:** If firmware loops on these APIs waiting for state to change, the simulation would deadlock. The state can only change when the coordinator advances time and delivers events (e.g., `TransmitComplete`, `PacketReceived`), but the coordinator is waiting for the node to yield.

**The Solution:** These polling APIs must trigger a yield when they detect a tight polling loop, allowing the coordinator to advance time.

#### Radio State Change Events

Rather than tracking each API separately, we use a unified "radio state version" that increments whenever the radio state changes:

- TX starts or completes
- RX packet arrives
- Reception begins or ends

```cpp
class SimRadio {
    // Radio state version - incremented on any state change
    uint32_t state_version_ = 0;
    
    // Last observed state version for spin detection  
    uint32_t last_polled_version_ = 0;
    int poll_count_ = 0;
    
    void onStateChanged() {
        state_version_++;
    }
    
    // Called by any polling API (isSendComplete, isReceiving, etc.)
    void checkForSpin() {
        if (state_version_ != last_polled_version_) {
            // State changed since last poll - reset counter and yield
            last_polled_version_ = state_version_;
            poll_count_ = 0;
            sim_yield_idle();  // Give coordinator a chance to process
            return;
        }
        
        // State unchanged
        poll_count_++;
        
        if (poll_count_ >= 3) {
            // Firmware is spinning on unchanged state - yield until
            // either radio state changes or time advances
            poll_count_ = 0;
            sim_yield_idle();
        }
    }
};

bool SimRadio::isSendComplete() {
    checkForSpin();
    return !tx_in_progress_;
}

bool SimRadio::isReceiving() {
    checkForSpin();
    return has_active_reception_;
}

bool SimRadio::isInRecvMode() const {
    checkForSpin();
    return !tx_in_progress_ && !tx_pending_;
}
```

**Key Design Decisions:**

1. **Unified state tracking** - A single `state_version_` counter increments on any radio state change, simplifying the logic across all polling APIs.

2. **Yield on state change** - When the state has changed since the last poll, yield immediately to let the coordinator process the transition.

3. **Yield after 3 polls with no change** - If firmware polls the same unchanged state 3+ times, yield to let time advance (which may trigger state changes).

4. **Yield IDLE** - The firmware may choose to do other work. We don't force blocking until a specific condition is met.

### Received Packet Structure

```cpp
// Internal RX queue entry
struct RxPacket {
    uint8_t data[256];  // Raw LoRa payload
    size_t len;         // Payload length
    float rssi;         // Received signal strength (dBm)
    float snr;          // Signal-to-noise ratio (dB)
};
```

## 3. Collision Detection and Packet Reception

### LoRa Packet Events

The simulation uses the event types defined earlier in this document. For reference, the key events for packet handling are:

- **`TransmitAirEvent`** (Radio → Graph): Sent when a radio begins transmission. Directed to the Graph entity which routes to receivers.
- **`ReceiveAirEvent`** (Graph → Radio): Sent from Graph entity to receiving radios with SNR/RSSI from link model.
- **`RadioRxPacketEvent`** (Radio → Firmware): Delivered to firmware after collision/SNR filtering determines the packet survived.
- **`RadioTxRequestEvent`** (Firmware → Radio): Sent when firmware requests a transmission.

### The LoraPacket Structure

```rust
pub struct LoraPacket {
    pub payload: Vec<u8>,   // Raw bytes (MeshCore frame)
}
```

Note: Radio parameters (frequency, bandwidth, spreading factor, coding rate, tx power) are
included in the `TransmitAirEvent` and `ReceiveAirEvent` via the `RadioParams` struct,
rather than in the packet itself.

### Collision Detection Algorithm

The `mcsim-lora` crate implements collision detection for overlapping transmissions:

```rust
pub struct CollisionContext {
    pub start_time: SimTime,
    pub end_time: SimTime,
    pub frequency_hz: u32,
    pub packet_id: u64,
}

pub enum CollisionResult {
    NoCollision,              // Packet received cleanly
    Destroyed,                // Incoming packet destroyed
    BothDestroyed(u64),       // Mutual destruction
    CaptureEffect(u64),       // Stronger signal captures (future)
}
```

**Collision Rules:**
1. Packets on different frequencies never collide
2. Packets that don't overlap in time don't collide
3. Overlapping packets on the same frequency are both destroyed
4. Future: Capture effect allows stronger signal to survive if 6+ dB stronger

### Link Model

The link model determines signal propagation between radios:

```rust
pub struct LinkParams {
    pub snr_db: f64,      // Signal-to-noise ratio at receiver
    pub rssi_dbm: f64,    // Received signal strength
}

pub struct LinkModel {
    edges: HashMap<(EntityId, EntityId), LinkParams>,
}
```

For each transmitter, the link model provides:
- Which receivers can hear the transmission
- What SNR and RSSI each receiver experiences
- Whether the signal is strong enough to decode (based on spreading factor sensitivity)

### SNR Sensitivity Thresholds

```rust
// Approximate SNR thresholds for successful LoRa demodulation
fn calculate_snr_sensitivity(spreading_factor: u8) -> f64 {
    match spreading_factor {
        7  => -7.5,
        8  => -10.0,
        9  => -12.5,
        10 => -15.0,
        11 => -17.5,   // MeshCore default
        12 => -20.0,
        _  => -10.0,
    }
}
```
