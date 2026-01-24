//! Watchdog thread for monitoring slow events in the simulation.
//!
//! The watchdog runs in a separate thread and monitors the main event loop.
//! If an event takes longer than the configured timeout, it prints detailed
//! information about the event to help diagnose hangs or very slow operations.

use mcsim_common::{Event, EventPayload, SimTime};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Information about the currently processing event.
#[derive(Debug, Clone)]
pub struct CurrentEventInfo {
    /// Event number (sequential count).
    pub event_number: u64,
    /// Event ID.
    pub event_id: u64,
    /// Simulation time of the event.
    pub sim_time: SimTime,
    /// Source entity ID.
    pub source_entity_id: u64,
    /// Target entity IDs.
    pub target_entity_ids: Vec<u64>,
    /// Event type name.
    pub event_type: String,
    /// Optional extra details about the event.
    pub details: String,
    /// When processing of this event started.
    pub started_at: Instant,
}

impl CurrentEventInfo {
    /// Create info from an Event.
    pub fn from_event(event: &Event, event_number: u64) -> Self {
        let (event_type, details) = describe_event_payload(&event.payload);
        CurrentEventInfo {
            event_number,
            event_id: event.id.0,
            sim_time: event.time,
            source_entity_id: event.source.0,
            target_entity_ids: event.targets.iter().map(|e| e.0).collect(),
            event_type,
            details,
            started_at: Instant::now(),
        }
    }
}

/// Describe an event payload for logging.
fn describe_event_payload(payload: &EventPayload) -> (String, String) {
    match payload {
        EventPayload::TransmitAir(e) => (
            "TransmitAir".to_string(),
            format!(
                "freq={}Hz, sf={}, pkt_len={}, end={:.3}s",
                e.params.frequency_hz, e.params.spreading_factor, e.packet.payload.len(), e.end_time.as_secs_f64()
            ),
        ),
        EventPayload::ReceiveAir(e) => (
            "ReceiveAir".to_string(),
            format!(
                "src_radio={}, mean_snr={:.1}dB, pkt_len={}",
                e.source_radio_id.0,
                e.mean_snr_db_at20dbm,
                e.packet.payload.len()
            ),
        ),
        EventPayload::RadioRxPacket(e) => (
            "RadioRxPacket".to_string(),
            format!("snr={:.1}dB, rssi={:.1}dBm, len={}, collided={}", e.snr_db, e.rssi_dbm, e.packet.payload.len(), e.was_collided),
        ),
        EventPayload::RadioStateChanged(e) => (
            "RadioStateChanged".to_string(),
            format!("{:?}", e.new_state),
        ),
        EventPayload::RadioTxRequest(e) => (
            "RadioTxRequest".to_string(),
            format!("pkt_len={}", e.packet.payload.len()),
        ),
        EventPayload::SerialRx(e) => (
            "SerialRx".to_string(),
            format!("len={}", e.data.len()),
        ),
        EventPayload::SerialTx(e) => (
            "SerialTx".to_string(),
            format!("len={}", e.data.len()),
        ),
        EventPayload::MessageSend(e) => (
            "MessageSend".to_string(),
            format!("to={:?}, content_len={}", e.destination, e.content.len()),
        ),
        EventPayload::MessageReceived(e) => (
            "MessageReceived".to_string(),
            format!("from={:?}", e.from),
        ),
        EventPayload::MessageAcknowledged(_) => (
            "MessageAcknowledged".to_string(),
            String::new(),
        ),
        EventPayload::Timer { timer_id } => (
            "Timer".to_string(),
            format!("timer_id={}", timer_id),
        ),
        EventPayload::SimulationEnd => (
            "SimulationEnd".to_string(),
            String::new(),
        ),
    }
}

/// Shared state between the main event loop and the watchdog thread.
pub struct WatchdogState {
    /// The currently processing event info.
    current_event: Mutex<Option<CurrentEventInfo>>,
    /// Flag to signal the watchdog thread to stop.
    stop_flag: AtomicBool,
    /// Entity name lookup (entity_id -> name).
    entity_names: Mutex<std::collections::HashMap<u64, String>>,
    /// Count of watchdog alerts fired.
    alert_count: AtomicU64,
    /// Simulation seed for reproducibility.
    seed: AtomicU64,
}

impl WatchdogState {
    /// Create a new watchdog state.
    pub fn new() -> Self {
        WatchdogState {
            current_event: Mutex::new(None),
            stop_flag: AtomicBool::new(false),
            entity_names: Mutex::new(std::collections::HashMap::new()),
            alert_count: AtomicU64::new(0),
            seed: AtomicU64::new(0),
        }
    }

    /// Set the simulation seed for display in alerts.
    pub fn set_seed(&self, seed: u64) {
        self.seed.store(seed, Ordering::Relaxed);
    }

    /// Get the simulation seed.
    pub fn get_seed(&self) -> u64 {
        self.seed.load(Ordering::Relaxed)
    }

    /// Register entity names for better logging.
    pub fn register_entity_names<I>(&self, names: I)
    where
        I: IntoIterator<Item = (u64, String)>,
    {
        let mut map = self.entity_names.lock().unwrap();
        for (id, name) in names {
            map.insert(id, name);
        }
    }

    /// Set the currently processing event.
    pub fn set_current_event(&self, info: Option<CurrentEventInfo>) {
        let mut current = self.current_event.lock().unwrap();
        *current = info;
    }

    /// Get the currently processing event info.
    pub fn get_current_event(&self) -> Option<CurrentEventInfo> {
        self.current_event.lock().unwrap().clone()
    }

    /// Signal the watchdog to stop.
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    /// Check if the watchdog should stop.
    pub fn should_stop(&self) -> bool {
        self.stop_flag.load(Ordering::Relaxed)
    }

    /// Get entity name by ID.
    pub fn entity_name(&self, id: u64) -> String {
        let map = self.entity_names.lock().unwrap();
        map.get(&id).cloned().unwrap_or_else(|| format!("entity:{}", id))
    }

    /// Increment and return the alert count.
    pub fn increment_alert_count(&self) -> u64 {
        self.alert_count.fetch_add(1, Ordering::Relaxed) + 1
    }
}

impl Default for WatchdogState {
    fn default() -> Self {
        Self::new()
    }
}

/// Watchdog thread handle.
pub struct Watchdog {
    state: Arc<WatchdogState>,
    thread_handle: Option<JoinHandle<()>>,
    timeout: Duration,
}

impl Watchdog {
    /// Create and start a new watchdog thread.
    pub fn new(timeout: Duration) -> Self {
        let state = Arc::new(WatchdogState::new());
        let watchdog_state = Arc::clone(&state);
        let check_interval = Duration::from_millis(500);

        let thread_handle = thread::spawn(move || {
            let mut last_alerted_event: Option<u64> = None;

            while !watchdog_state.should_stop() {
                thread::sleep(check_interval);

                if let Some(event_info) = watchdog_state.get_current_event() {
                    let elapsed = event_info.started_at.elapsed();

                    // Only alert once per event (don't spam)
                    if elapsed >= timeout && last_alerted_event != Some(event_info.event_number) {
                        last_alerted_event = Some(event_info.event_number);
                        let alert_num = watchdog_state.increment_alert_count();

                        eprintln!();
                        eprintln!("┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                        eprintln!("┃ ⚠️  WATCHDOG ALERT #{}: Event taking too long ({:.1}s)", alert_num, elapsed.as_secs_f64());
                        eprintln!("┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                        eprintln!("┃ Event Number:  {}", event_info.event_number);
                        eprintln!("┃ Event ID:      {}", event_info.event_id);
                        eprintln!("┃ Event Type:    {}", event_info.event_type);
                        eprintln!("┃ Sim Time:      {:.3}s", event_info.sim_time.as_secs_f64());
                        eprintln!("┃ Source:        {} (id={})", 
                            watchdog_state.entity_name(event_info.source_entity_id),
                            event_info.source_entity_id);
                        if !event_info.target_entity_ids.is_empty() {
                            let targets: Vec<String> = event_info.target_entity_ids.iter()
                                .map(|id| format!("{} (id={})", watchdog_state.entity_name(*id), id))
                                .collect();
                            eprintln!("┃ Targets:       {}", targets.join(", "));
                        }
                        if !event_info.details.is_empty() {
                            eprintln!("┃ Details:       {}", event_info.details);
                        }
                        eprintln!("┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                        eprintln!("┃ To debug this event, re-run with:");
                        eprintln!("┃   --seed {} --break-at-event {}", watchdog_state.get_seed(), event_info.event_number);
                        eprintln!("┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                        eprintln!();
                    }
                }
            }
        });

        Watchdog {
            state,
            thread_handle: Some(thread_handle),
            timeout,
        }
    }

    /// Get a reference to the watchdog state for the main loop to update.
    pub fn state(&self) -> &Arc<WatchdogState> {
        &self.state
    }

    /// Get the configured timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Stop the watchdog thread and wait for it to finish.
    pub fn stop(mut self) {
        self.state.stop();
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.state.stop();
        // Don't wait for thread in drop - it will terminate on its own
    }
}
