//! Entity-level trace logging for simulation debugging.
//!
//! This module provides a generalized tracing framework for all simulation entities.
//! It allows detailed tracing of entity behavior including event handling, state changes,
//! and custom trace points.
//!
//! # Usage
//!
//! Entities can be traced by name (for named entities like nodes) or by entity ID.
//! The tracer supports both automatic tracing via wrapper and manual trace logging.
//!
//! ```rust,ignore
//! use mcsim_common::entity_tracer::{EntityTracer, EntityTracerConfig, TraceEvent};
//!
//! // Create a tracer config targeting specific entities
//! let config = EntityTracerConfig::from_spec("Alice,Bob,entity:42");
//! let tracer = EntityTracer::new(config);
//!
//! // Check if an entity should be traced
//! if tracer.should_trace_name("Alice") {
//!     tracer.log(TraceEvent::custom("Alice", 1, sim_time, "State changed"));
//! }
//! ```

use crate::{EntityId, Event, EventPayload, SimTime};
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

// ============================================================================
// Trace Event Types
// ============================================================================

/// Categories of trace events for filtering and display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TraceCategory {
    /// Event received by entity.
    EventReceived,
    /// Event emitted by entity.
    EventEmitted,
    /// State change within entity.
    StateChange,
    /// Entity-specific operation (step, yield, etc.).
    Operation,
    /// Timer scheduled or fired.
    Timer,
    /// Custom/debug trace point.
    Custom,
}

impl fmt::Display for TraceCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TraceCategory::EventReceived => write!(f, "EVENT_RX"),
            TraceCategory::EventEmitted => write!(f, "EVENT_TX"),
            TraceCategory::StateChange => write!(f, "STATE"),
            TraceCategory::Operation => write!(f, "OP"),
            TraceCategory::Timer => write!(f, "TIMER"),
            TraceCategory::Custom => write!(f, "TRACE"),
        }
    }
}

/// A trace event record.
#[derive(Debug, Clone)]
pub struct TraceEvent {
    /// Name of the entity (if available).
    pub entity_name: Option<String>,
    /// Entity ID.
    pub entity_id: EntityId,
    /// Simulation time when the event occurred.
    pub sim_time: SimTime,
    /// Category of the trace event.
    pub category: TraceCategory,
    /// Human-readable description of the event.
    pub description: String,
    /// Optional additional details as key-value pairs.
    pub details: Vec<(String, String)>,
}

impl TraceEvent {
    /// Create a new trace event for an event being received.
    pub fn event_received(
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        event: &Event,
    ) -> Self {
        let (desc, details) = describe_event_payload(&event.payload);
        TraceEvent {
            entity_name: entity_name.map(|s| s.to_string()),
            entity_id,
            sim_time,
            category: TraceCategory::EventReceived,
            description: desc,
            details,
        }
    }

    /// Create a new trace event for an event being emitted.
    pub fn event_emitted(
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        event: &Event,
    ) -> Self {
        let (desc, details) = describe_event_payload(&event.payload);
        let mut all_details = details;
        all_details.push(("targets".to_string(), format!("{:?}", event.targets)));
        all_details.push(("delay_us".to_string(), format!("{}", event.time.as_micros() - sim_time.as_micros())));
        TraceEvent {
            entity_name: entity_name.map(|s| s.to_string()),
            entity_id,
            sim_time,
            category: TraceCategory::EventEmitted,
            description: desc,
            details: all_details,
        }
    }

    /// Create a state change trace event.
    pub fn state_change(
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        description: impl Into<String>,
    ) -> Self {
        TraceEvent {
            entity_name: entity_name.map(|s| s.to_string()),
            entity_id,
            sim_time,
            category: TraceCategory::StateChange,
            description: description.into(),
            details: Vec::new(),
        }
    }

    /// Create a state change trace event with details.
    pub fn state_change_with_details(
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        description: impl Into<String>,
        details: Vec<(String, String)>,
    ) -> Self {
        TraceEvent {
            entity_name: entity_name.map(|s| s.to_string()),
            entity_id,
            sim_time,
            category: TraceCategory::StateChange,
            description: description.into(),
            details,
        }
    }

    /// Create an operation trace event.
    pub fn operation(
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        description: impl Into<String>,
    ) -> Self {
        TraceEvent {
            entity_name: entity_name.map(|s| s.to_string()),
            entity_id,
            sim_time,
            category: TraceCategory::Operation,
            description: description.into(),
            details: Vec::new(),
        }
    }

    /// Create an operation trace event with details.
    pub fn operation_with_details(
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        description: impl Into<String>,
        details: Vec<(String, String)>,
    ) -> Self {
        TraceEvent {
            entity_name: entity_name.map(|s| s.to_string()),
            entity_id,
            sim_time,
            category: TraceCategory::Operation,
            description: description.into(),
            details,
        }
    }

    /// Create a timer trace event.
    pub fn timer(
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        description: impl Into<String>,
    ) -> Self {
        TraceEvent {
            entity_name: entity_name.map(|s| s.to_string()),
            entity_id,
            sim_time,
            category: TraceCategory::Timer,
            description: description.into(),
            details: Vec::new(),
        }
    }

    /// Create a custom trace event.
    pub fn custom(
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        description: impl Into<String>,
    ) -> Self {
        TraceEvent {
            entity_name: entity_name.map(|s| s.to_string()),
            entity_id,
            sim_time,
            category: TraceCategory::Custom,
            description: description.into(),
            details: Vec::new(),
        }
    }

    /// Add a detail to this event.
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.push((key.into(), value.into()));
        self
    }
}

/// Describe an event payload for tracing.
fn describe_event_payload(payload: &EventPayload) -> (String, Vec<(String, String)>) {
    match payload {
        EventPayload::TransmitAir(e) => {
            let details = vec![
                ("radio_id".to_string(), format!("{:?}", e.radio_id)),
                ("packet_len".to_string(), format!("{}", e.packet.payload.len())),
                ("end_time_us".to_string(), format!("{}", e.end_time.as_micros())),
            ];
            ("TransmitAir".to_string(), details)
        }
        EventPayload::ReceiveAir(e) => {
            let details = vec![
                ("source_radio".to_string(), format!("{:?}", e.source_radio_id)),
                ("packet_len".to_string(), format!("{}", e.packet.payload.len())),
                ("mean_snr_db".to_string(), format!("{:.1}", e.mean_snr_db_at20dbm)),
                ("snr_std_dev".to_string(), format!("{:.1}", e.snr_std_dev)),
                ("rssi_dbm".to_string(), format!("{:.1}", e.rssi_dbm)),
            ];
            ("ReceiveAir".to_string(), details)
        }
        EventPayload::RadioRxPacket(e) => {
            let details = vec![
                ("packet_len".to_string(), format!("{}", e.packet.payload.len())),
                ("snr_db".to_string(), format!("{:.1}", e.snr_db)),
                ("rssi_dbm".to_string(), format!("{:.1}", e.rssi_dbm)),
                ("collided".to_string(), format!("{}", e.was_collided)),
            ];
            ("RadioRxPacket".to_string(), details)
        }
        EventPayload::RadioStateChanged(e) => {
            let details = vec![
                ("new_state".to_string(), format!("{:?}", e.new_state)),
                ("state_version".to_string(), format!("{}", e.state_version)),
            ];
            ("RadioStateChanged".to_string(), details)
        }
        EventPayload::RadioTxRequest(e) => {
            let details = vec![
                ("packet_len".to_string(), format!("{}", e.packet.payload.len())),
            ];
            ("RadioTxRequest".to_string(), details)
        }
        EventPayload::SerialRx(e) => {
            let details = vec![
                ("data_len".to_string(), format!("{}", e.data.len())),
            ];
            ("SerialRx".to_string(), details)
        }
        EventPayload::SerialTx(e) => {
            let details = vec![
                ("data_len".to_string(), format!("{}", e.data.len())),
            ];
            ("SerialTx".to_string(), details)
        }
        EventPayload::FirmwareLog(e) => {
            let details = vec![
                ("line_len".to_string(), format!("{}", e.line.len())),
            ];
            ("FirmwareLog".to_string(), details)
        }
        EventPayload::MessageSend(e) => {
            let details = vec![
                ("destination".to_string(), format!("{:?}", e.destination)),
                ("content_len".to_string(), format!("{}", e.content.len())),
                ("want_ack".to_string(), format!("{}", e.want_ack)),
            ];
            ("MessageSend".to_string(), details)
        }
        EventPayload::MessageReceived(e) => {
            let details = vec![
                ("from".to_string(), format!("{}", e.from)),
            ];
            ("MessageReceived".to_string(), details)
        }
        EventPayload::MessageAcknowledged(e) => {
            let details = vec![
                ("hop_count".to_string(), format!("{}", e.hop_count)),
            ];
            ("MessageAcknowledged".to_string(), details)
        }
        EventPayload::Timer { timer_id } => {
            let details = vec![
                ("timer_id".to_string(), format!("{}", timer_id)),
            ];
            ("Timer".to_string(), details)
        }
        EventPayload::SimulationEnd => {
            ("SimulationEnd".to_string(), Vec::new())
        }
    }
}

/// Sanitize a string for display, replacing non-printable characters with escape sequences.
/// This is useful for logging strings that may contain binary data or control characters.
fn sanitize_for_display(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '\\' {
            // Escape backslashes to avoid ambiguity with escape sequences
            result.push_str("\\\\");
        } else if c.is_ascii_graphic() || c == ' ' {
            result.push(c);
        } else if c == '\t' {
            result.push_str("\\t");
        } else if c == '\n' {
            result.push_str("\\n");
        } else if c == '\r' {
            result.push_str("\\r");
        } else if c == '\0' {
            result.push_str("\\0");
        } else if c.is_ascii_control() {
            // ASCII control characters (0x00-0x1F, 0x7F)
            result.push_str(&format!("\\x{:02x}", c as u32));
        } else if c == '\u{FFFD}' {
            // Unicode replacement character (from invalid UTF-8)
            result.push_str("\\u{FFFD}");
        } else if !c.is_ascii() {
            // Non-ASCII: show as unicode escape if not printable
            if c.is_alphanumeric() || c.is_whitespace() {
                result.push(c);
            } else {
                result.push_str(&format!("\\u{{{:04x}}}", c as u32));
            }
        } else {
            result.push(c);
        }
    }
    result
}

// ============================================================================
// Tracer Configuration
// ============================================================================

/// Configuration for entity tracing.
#[derive(Debug, Clone)]
pub struct EntityTracerConfig {
    /// Entity names to trace. If empty and ids is empty, no tracing is done.
    pub traced_names: HashSet<String>,
    /// Entity IDs to trace (for entities without names or for precise targeting).
    pub traced_ids: HashSet<u64>,
    /// Categories to trace. If empty, all categories are traced.
    pub traced_categories: HashSet<TraceCategory>,
}

impl EntityTracerConfig {
    /// Create a new empty config (no tracing).
    pub fn none() -> Self {
        EntityTracerConfig {
            traced_names: HashSet::new(),
            traced_ids: HashSet::new(),
            traced_categories: HashSet::new(),
        }
    }

    /// Create a tracer config from a specification string.
    ///
    /// The spec format is a comma-separated list of:
    /// - Entity names (e.g., "Alice", "Bob")
    /// - Entity IDs prefixed with "entity:" (e.g., "entity:42")
    /// - Special values: "*" traces all entities
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Trace specific nodes by name
    /// let config = EntityTracerConfig::from_spec("Alice,Bob");
    ///
    /// // Trace by entity ID
    /// let config = EntityTracerConfig::from_spec("entity:1,entity:2");
    ///
    /// // Mix of names and IDs
    /// let config = EntityTracerConfig::from_spec("Alice,entity:42,Bob");
    /// ```
    pub fn from_spec(spec: &str) -> Self {
        if spec.is_empty() {
            return Self::none();
        }

        let mut traced_names = HashSet::new();
        let mut traced_ids = HashSet::new();

        for part in spec.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            if part == "*" {
                // Special case: trace all - we'll handle this specially
                traced_names.insert("*".to_string());
            } else if let Some(id_str) = part.strip_prefix("entity:") {
                if let Ok(id) = id_str.parse::<u64>() {
                    traced_ids.insert(id);
                }
            } else {
                traced_names.insert(part.to_string());
            }
        }

        EntityTracerConfig {
            traced_names,
            traced_ids,
            traced_categories: HashSet::new(), // Trace all categories by default
        }
    }

    /// Check if tracing is enabled at all.
    pub fn is_enabled(&self) -> bool {
        !self.traced_names.is_empty() || !self.traced_ids.is_empty()
    }

    /// Check if all entities should be traced.
    pub fn traces_all(&self) -> bool {
        self.traced_names.contains("*")
    }

    /// Check if a specific entity name should be traced.
    pub fn should_trace_name(&self, name: &str) -> bool {
        if !self.is_enabled() {
            return false;
        }
        self.traces_all() || self.traced_names.contains(name)
    }

    /// Check if a specific entity ID should be traced.
    pub fn should_trace_id(&self, id: EntityId) -> bool {
        if !self.is_enabled() {
            return false;
        }
        self.traces_all() || self.traced_ids.contains(&id.0)
    }

    /// Check if an entity should be traced (by name or ID).
    pub fn should_trace(&self, name: Option<&str>, id: EntityId) -> bool {
        if !self.is_enabled() {
            return false;
        }
        if self.traces_all() {
            return true;
        }
        if let Some(n) = name {
            if self.traced_names.contains(n) {
                return true;
            }
        }
        self.traced_ids.contains(&id.0)
    }

    /// Check if a category should be traced.
    pub fn should_trace_category(&self, category: TraceCategory) -> bool {
        self.traced_categories.is_empty() || self.traced_categories.contains(&category)
    }

    /// Add a category filter.
    pub fn with_category(mut self, category: TraceCategory) -> Self {
        self.traced_categories.insert(category);
        self
    }
}

impl Default for EntityTracerConfig {
    fn default() -> Self {
        Self::none()
    }
}

// ============================================================================
// Entity Tracer
// ============================================================================

/// Entity tracer that logs detailed entity behavior.
///
/// This is a shared tracer that can be cloned and used across multiple entities.
#[derive(Clone)]
pub struct EntityTracer {
    config: Arc<EntityTracerConfig>,
}

impl EntityTracer {
    /// Create a new entity tracer with the given configuration.
    pub fn new(config: EntityTracerConfig) -> Self {
        EntityTracer {
            config: Arc::new(config),
        }
    }

    /// Create a tracer that does no tracing.
    pub fn disabled() -> Self {
        EntityTracer::new(EntityTracerConfig::none())
    }

    /// Check if tracing is enabled at all.
    pub fn is_enabled(&self) -> bool {
        self.config.is_enabled()
    }

    /// Check if a specific entity should be traced (by name).
    pub fn should_trace_name(&self, name: &str) -> bool {
        self.config.should_trace_name(name)
    }

    /// Check if a specific entity should be traced (by ID).
    pub fn should_trace_id(&self, id: EntityId) -> bool {
        self.config.should_trace_id(id)
    }

    /// Check if an entity should be traced (by name or ID).
    pub fn should_trace(&self, name: Option<&str>, id: EntityId) -> bool {
        self.config.should_trace(name, id)
    }

    /// Get the tracer configuration.
    pub fn config(&self) -> &EntityTracerConfig {
        &self.config
    }

    /// Log a trace event.
    pub fn log(&self, event: TraceEvent) {
        // Check if we should trace this entity
        if !self.config.should_trace(event.entity_name.as_deref(), event.entity_id) {
            return;
        }

        // Check if we should trace this category
        if !self.config.should_trace_category(event.category) {
            return;
        }

        // Format and output the trace
        self.output_trace(&event);
    }

    /// Log that an entity is handling an event.
    pub fn log_event_received(
        &self,
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        event: &Event,
    ) {
        if !self.config.should_trace(entity_name, entity_id) {
            return;
        }
        self.log(TraceEvent::event_received(entity_name, entity_id, sim_time, event));
    }

    /// Log that an entity is emitting an event.
    pub fn log_event_emitted(
        &self,
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        event: &Event,
    ) {
        if !self.config.should_trace(entity_name, entity_id) {
            return;
        }
        self.log(TraceEvent::event_emitted(entity_name, entity_id, sim_time, event));
    }

    /// Log a state change.
    pub fn log_state_change(
        &self,
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        description: impl Into<String>,
    ) {
        if !self.config.should_trace(entity_name, entity_id) {
            return;
        }
        self.log(TraceEvent::state_change(entity_name, entity_id, sim_time, description));
    }

    /// Log an operation.
    pub fn log_operation(
        &self,
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        description: impl Into<String>,
    ) {
        if !self.config.should_trace(entity_name, entity_id) {
            return;
        }
        self.log(TraceEvent::operation(entity_name, entity_id, sim_time, description));
    }

    /// Log an operation with details.
    pub fn log_operation_with_details(
        &self,
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        description: impl Into<String>,
        details: Vec<(String, String)>,
    ) {
        if !self.config.should_trace(entity_name, entity_id) {
            return;
        }
        self.log(TraceEvent::operation_with_details(
            entity_name,
            entity_id,
            sim_time,
            description,
            details,
        ));
    }

    /// Format and output a trace event.
    fn output_trace(&self, event: &TraceEvent) {
        let time_ms = event.sim_time.as_micros() as f64 / 1000.0;
        
        // Format entity identifier
        let entity_str = if let Some(ref name) = event.entity_name {
            format!("{} (entity={})", name, event.entity_id.0)
        } else {
            format!("entity={}", event.entity_id.0)
        };

        // Format details
        let details_str = if event.details.is_empty() {
            String::new()
        } else {
            let detail_parts: Vec<String> = event
                .details
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            format!(" [{}]", detail_parts.join(", "))
        };

        eprintln!(
            "[TRACE] {} @ {:.3}ms: {} {}{}",
            entity_str,
            time_ms,
            event.category,
            event.description,
            details_str
        );
    }
}

impl Default for EntityTracer {
    fn default() -> Self {
        Self::disabled()
    }
}

// ============================================================================
// Firmware-Specific Trace Types (for backward compatibility)
// ============================================================================

/// Yield reason for firmware tracing.
/// This mirrors the DLL YieldReason but provides a trace-friendly representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirmwareYieldReason {
    /// Firmware is idle, waiting for events.
    Idle,
    /// Radio started transmitting.
    RadioTxStart,
    /// Radio completed transmission.
    RadioTxComplete,
    /// Firmware requested reboot.
    Reboot,
    /// Firmware requested power off.
    PowerOff,
    /// An error occurred.
    Error,
}

impl fmt::Display for FirmwareYieldReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FirmwareYieldReason::Idle => write!(f, "IDLE"),
            FirmwareYieldReason::RadioTxStart => write!(f, "RADIO_TX_START"),
            FirmwareYieldReason::RadioTxComplete => write!(f, "RADIO_TX_COMPLETE"),
            FirmwareYieldReason::Reboot => write!(f, "REBOOT"),
            FirmwareYieldReason::PowerOff => write!(f, "POWER_OFF"),
            FirmwareYieldReason::Error => write!(f, "ERROR"),
        }
    }
}

impl EntityTracer {
    /// Log a firmware step beginning.
    pub fn log_firmware_step_begin(
        &self,
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        trigger: &str,
    ) {
        if !self.config.should_trace(entity_name, entity_id) {
            return;
        }
        self.log(TraceEvent::operation(
            entity_name,
            entity_id,
            sim_time,
            format!("STEP_BEGIN <- {}", trigger),
        ));
    }

    /// Log a firmware step yield.
    pub fn log_firmware_step_yield(
        &self,
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        reason: FirmwareYieldReason,
        details: Vec<(String, String)>,
    ) {
        if !self.config.should_trace(entity_name, entity_id) {
            return;
        }
        self.log(TraceEvent::operation_with_details(
            entity_name,
            entity_id,
            sim_time,
            format!("YIELD {}", reason),
            details,
        ));
    }

    /// Log a firmware TX request.
    pub fn log_firmware_tx_request(
        &self,
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        packet_len: usize,
        airtime_ms: u32,
    ) {
        if !self.config.should_trace(entity_name, entity_id) {
            return;
        }
        self.log(TraceEvent::operation_with_details(
            entity_name,
            entity_id,
            sim_time,
            "TX_REQUEST",
            vec![
                ("packet_len".to_string(), format!("{}", packet_len)),
                ("airtime_ms".to_string(), format!("{}", airtime_ms)),
            ],
        ));
    }

    /// Log firmware serial output.
    pub fn log_firmware_serial_tx(
        &self,
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        data: &[u8],
    ) {
        if !self.config.should_trace(entity_name, entity_id) {
            return;
        }

        // Try to display as UTF-8 if possible, otherwise show hex
        let display = if let Ok(s) = std::str::from_utf8(data) {
            let trimmed = s.trim();
            if trimmed.len() <= 80 {
                format!("\"{}\"", trimmed.replace('\n', "\\n").replace('\r', "\\r"))
            } else {
                format!("\"{}...\" ({} bytes)", &trimmed[..40], data.len())
            }
        } else {
            format!("{} bytes: {:02x?}", data.len(), &data[..data.len().min(16)])
        };

        self.log(TraceEvent::operation_with_details(
            entity_name,
            entity_id,
            sim_time,
            "SERIAL_TX",
            vec![("data".to_string(), display)],
        ));
    }

    /// Log firmware log output.
    pub fn log_firmware_output(
        &self,
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        log_output: &str,
    ) {
        if !self.config.should_trace(entity_name, entity_id) {
            return;
        }
        if log_output.is_empty() {
            return;
        }

        for line in log_output.lines() {
            // Sanitize the line: replace non-printable characters with escape sequences
            let sanitized = sanitize_for_display(line);
            self.log(TraceEvent::custom(
                entity_name,
                entity_id,
                sim_time,
                format!("LOG: {}", sanitized),
            ));
        }
    }

    /// Log a scheduled timer.
    pub fn log_timer_scheduled(
        &self,
        entity_name: Option<&str>,
        entity_id: EntityId,
        sim_time: SimTime,
        timer_id: u64,
        delay_ms: u64,
    ) {
        if !self.config.should_trace(entity_name, entity_id) {
            return;
        }
        self.log(TraceEvent::timer(
            entity_name,
            entity_id,
            sim_time,
            format!("SCHEDULED timer_id={} delay={}ms", timer_id, delay_ms),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_spec_empty() {
        let config = EntityTracerConfig::from_spec("");
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_config_from_spec_names() {
        let config = EntityTracerConfig::from_spec("Alice,Bob");
        assert!(config.is_enabled());
        assert!(config.should_trace_name("Alice"));
        assert!(config.should_trace_name("Bob"));
        assert!(!config.should_trace_name("Charlie"));
    }

    #[test]
    fn test_config_from_spec_ids() {
        let config = EntityTracerConfig::from_spec("entity:1,entity:2");
        assert!(config.is_enabled());
        assert!(config.should_trace_id(EntityId::new(1)));
        assert!(config.should_trace_id(EntityId::new(2)));
        assert!(!config.should_trace_id(EntityId::new(3)));
    }

    #[test]
    fn test_config_from_spec_mixed() {
        let config = EntityTracerConfig::from_spec("Alice,entity:42,Bob");
        assert!(config.is_enabled());
        assert!(config.should_trace_name("Alice"));
        assert!(config.should_trace_name("Bob"));
        assert!(config.should_trace_id(EntityId::new(42)));
        assert!(!config.should_trace_name("Charlie"));
        assert!(!config.should_trace_id(EntityId::new(1)));
    }

    #[test]
    fn test_config_from_spec_all() {
        let config = EntityTracerConfig::from_spec("*");
        assert!(config.is_enabled());
        assert!(config.traces_all());
        assert!(config.should_trace_name("AnyName"));
        assert!(config.should_trace_id(EntityId::new(999)));
    }

    #[test]
    fn test_tracer_should_trace() {
        let config = EntityTracerConfig::from_spec("Alice,entity:42");
        let tracer = EntityTracer::new(config);
        
        assert!(tracer.should_trace(Some("Alice"), EntityId::new(1)));
        assert!(tracer.should_trace(Some("Bob"), EntityId::new(42)));
        assert!(tracer.should_trace(None, EntityId::new(42)));
        assert!(!tracer.should_trace(Some("Bob"), EntityId::new(1)));
    }
}
