//! # mcsim-agents
//!
//! Operator agent simulation for MCSim.
//!
//! This crate provides:
//!
//! - [`Agent`] - A unified agent that communicates with MeshCore companion
//!   firmware using the UART protocol. Agents can send direct messages and/or channel
//!   messages based on their configuration.
//!
//! - [`CliAgent`] - An agent that communicates with MeshCore repeater and room server
//!   firmware using the text-based CLI protocol. This agent applies configuration
//!   at node startup (password, CLI commands).

pub mod cli_agent;

pub use cli_agent::{CliAgent, CliAgentConfig, CliProtocolState, create_cli_agent};

use mcsim_common::{
    entity_tracer::TraceEvent, Entity, EntityId, Event, EventPayload, NodeId, SerialRxEvent,
    SimContext, SimError, SimTime,
};
use mcsim_companion_protocol::{
    Command, ChannelInfo, ContactInfo, Message, ProtocolSession, PushNotification, PublicKey,
    PublicKeyPrefix, ReceivedChannelMessage, ReceivedContactMessage, Response, TextType,
    ADV_TYPE_CHAT, ADV_TYPE_REPEATER, ADV_TYPE_ROOM_SERVER, MAX_PATH_SIZE,
};
use meshcore_packet::CorridorTriple;
use mcsim_metrics::{metric_defs, MetricLabels};
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use rand_distr::{Distribution, Normal};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, trace, warn};

// ============================================================================
// Configuration Types
// ============================================================================

/// Configuration for direct message sending behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectMessageConfig {
    /// Whether direct messaging is enabled.
    pub enabled: bool,
    /// Wait time before starting direct messages.
    pub startup_s: f64,
    /// Standard deviation in the randomness of the startup interval.
    pub startup_jitter_s: f64,
    /// Node IDs to target direct messages to.
    /// If empty, will be populated with all other companions.
    pub targets: Vec<NodeId>,
    /// Interval after receiving an ack (or timeout) before sending the next message.
    pub interval_s: f64,
    /// Standard deviation of the randomness in the message interval timer.
    pub interval_jitter_s: f64,
    /// Timeout waiting for an ACK before proceeding.
    pub ack_timeout_s: f64,
    /// Pause after this many messages before waiting for another session.
    /// If None, messaging continues indefinitely.
    pub session_message_count: Option<u32>,
    /// Time to wait for the next session.
    pub session_interval_s: f64,
    /// Standard deviation of randomness in the session interval timer.
    pub session_interval_jitter_s: f64,
    /// Count of messages before the agent stops sending.
    /// If None, the agent sends indefinitely.
    pub message_count: Option<u32>,
    /// Time before the agent stops sending.
    /// If None, the agent sends indefinitely.
    pub shutdown_s: Option<f64>,
    /// Number of uncounted path-warmup DMs to send before the main counted session.
    /// Each warmup DM triggers path discovery (FLOOD → PATH+ACK → Alice learns DIRECT
    /// route) but is not counted in delivery metrics. 0 = disabled.
    pub path_warmup_count: u32,
}

impl Default for DirectMessageConfig {
    fn default() -> Self {
        DirectMessageConfig {
            enabled: false,
            startup_s: 0.0,
            startup_jitter_s: 0.0,
            targets: Vec::new(),
            interval_s: 5.0,
            interval_jitter_s: 0.0,
            ack_timeout_s: 10.0,
            session_message_count: None,
            session_interval_s: 3600.0,
            session_interval_jitter_s: 0.0,
            message_count: None,
            shutdown_s: None,
            path_warmup_count: 0,
        }
    }
}

/// Channel configuration with name and secret.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelTarget {
    /// Channel name (e.g., "#test", "Public").
    pub name: String,
    /// Channel secret (16 bytes). If not provided, derived from name.
    pub secret: Option<[u8; 16]>,
}

/// Well-known key for the "Public" channel (embedded in MeshCore firmware).
/// This is the hex representation of base64 "izOH6cXN6mrJ5e26oRXNcg==".
const PUBLIC_CHANNEL_KEY: [u8; 16] = [
    0x8b, 0x33, 0x87, 0xe9, 0xc5, 0xcd, 0xea, 0x6a,
    0xc9, 0xe5, 0xed, 0xba, 0xa1, 0x15, 0xcd, 0x72,
];

impl ChannelTarget {
    /// Create a channel config with just a name (secret will be derived).
    pub fn from_name(name: String) -> Self {
        ChannelTarget { name, secret: None }
    }

    /// Get the secret, using the appropriate derivation method:
    /// - If secret is explicitly set, use it
    /// - If name is "Public", use the well-known firmware key
    /// - If name starts with "#", derive from SHA256(name)[0..16]
    /// - Otherwise, derive from SHA256(name)[0..16]
    pub fn get_secret(&self) -> [u8; 16] {
        use sha2::{Sha256, Digest};
        
        if let Some(secret) = self.secret {
            secret
        } else if self.name == "Public" {
            // Use well-known key embedded in MeshCore firmware
            PUBLIC_CHANNEL_KEY
        } else {
            // Derive from name (for "#channel" names and others)
            // MeshCore derives channel secrets as SHA256(name)[0..16]
            let mut hasher = Sha256::new();
            hasher.update(self.name.as_bytes());
            let hash = hasher.finalize();
            let mut secret = [0u8; 16];
            secret.copy_from_slice(&hash[..16]);
            secret
        }
    }
}

impl From<String> for ChannelTarget {
    fn from(name: String) -> Self {
        ChannelTarget::from_name(name)
    }
}

/// A contact to add to the firmware's contact list.
/// This enables encrypted DM communication with the contact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactTarget {
    /// Contact name.
    pub name: String,
    /// Contact's public key (32 bytes).
    pub public_key: NodeId,
    /// Contact type (ADV_TYPE_CHAT, ADV_TYPE_REPEATER, ADV_TYPE_ROOM_SERVER).
    pub contact_type: u8,
}

impl ContactTarget {
    /// Create a new contact target with a specified type.
    pub fn new(name: String, public_key: NodeId, contact_type: u8) -> Self {
        ContactTarget { name, public_key, contact_type }
    }
    
    /// Create a new chat contact target.
    pub fn chat(name: String, public_key: NodeId) -> Self {
        ContactTarget { name, public_key, contact_type: ADV_TYPE_CHAT }
    }
    
    /// Create a new repeater contact target.
    pub fn repeater(name: String, public_key: NodeId) -> Self {
        ContactTarget { name, public_key, contact_type: ADV_TYPE_REPEATER }
    }
    
    /// Create a new room server contact target.
    pub fn room_server(name: String, public_key: NodeId) -> Self {
        ContactTarget { name, public_key, contact_type: ADV_TYPE_ROOM_SERVER }
    }
}

/// Configuration for channel message sending behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMessageConfig {
    /// Whether channel messaging is enabled.
    pub enabled: bool,
    /// Wait time before starting channel messages.
    pub startup_s: f64,
    /// Standard deviation in the randomness of the startup interval.
    pub startup_jitter_s: f64,
    /// Channels to target channel messages to (will be subscribed AND messaged to).
    pub targets: Vec<ChannelTarget>,
    /// Channels to subscribe to but NOT send messages to.
    pub subscribe_only: Vec<ChannelTarget>,
    /// Interval after sending before sending the next message.
    pub interval_s: f64,
    /// Standard deviation of the randomness in the message interval timer.
    pub interval_jitter_s: f64,
    /// Pause after this many messages before waiting for another session.
    /// If None, messaging continues indefinitely.
    pub session_message_count: Option<u32>,
    /// Time to wait for the next session.
    pub session_interval_s: f64,
    /// Standard deviation of randomness in the session interval timer.
    pub session_interval_jitter_s: f64,
    /// Count of messages before the agent stops sending.
    /// If None, the agent sends indefinitely.
    pub message_count: Option<u32>,
    /// Time before the agent stops sending.
    /// If None, the agent sends indefinitely.
    pub shutdown_s: Option<f64>,
    /// Geo-corridor triples for scoped flood delivery.
    /// If non-empty, the agent uses CMD_SEND_CHANNEL_TXT_MSG_CORRIDOR.
    pub corridor: Vec<CorridorTriple>,
}

impl Default for ChannelMessageConfig {
    fn default() -> Self {
        ChannelMessageConfig {
            enabled: false,
            startup_s: 0.0,
            startup_jitter_s: 0.0,
            targets: vec![ChannelTarget::from_name("Public".to_string())],
            subscribe_only: Vec::new(),
            interval_s: 5.0,
            interval_jitter_s: 0.0,
            session_message_count: None,
            session_interval_s: 3600.0,
            session_interval_jitter_s: 0.0,
            message_count: None,
            shutdown_s: None,
            corridor: Vec::new(),
        }
    }
}

/// Configuration for TRACE path sending behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceConfig {
    /// Whether TRACE sending is enabled.
    pub enabled: bool,
    /// Wait time before starting TRACE sends.
    pub startup_s: f64,
    /// Standard deviation in the randomness of the startup interval.
    pub startup_jitter_s: f64,
    /// Resolved path bytes (sequence of per-hop hash prefixes).
    /// The destination is the final hash in the path.
    pub path: Vec<u8>,
    /// Interval after receiving a TraceData response (or timeout) before sending the next trace.
    pub interval_s: f64,
    /// Standard deviation of the randomness in the interval timer.
    pub interval_jitter_s: f64,
    /// Timeout waiting for a TraceData push before proceeding.
    pub response_timeout_s: f64,
    /// Count of traces before the agent stops sending.
    /// If None, the agent sends indefinitely.
    pub message_count: Option<u32>,
    /// Time before the agent stops sending.
    /// If None, the agent sends indefinitely.
    pub shutdown_s: Option<f64>,
    /// TRACE tag (arbitrary identifier, echoed in TraceData).
    pub tag: u32,
    /// TRACE auth code (arbitrary identifier, echoed in TraceData).
    pub auth: u32,
    /// TRACE flags. Lower 2 bits select path hash size:
    ///   0 = 1 byte/hash, 1 = 2 bytes/hash, 2 = 4 bytes/hash, 3 = 8 bytes/hash.
    pub flags: u8,
}

impl Default for TraceConfig {
    fn default() -> Self {
        TraceConfig {
            enabled: false,
            startup_s: 0.0,
            startup_jitter_s: 0.0,
            path: Vec::new(),
            interval_s: 5.0,
            interval_jitter_s: 0.0,
            response_timeout_s: 10.0,
            message_count: None,
            shutdown_s: None,
            tag: 0,
            auth: 0,
            flags: 0,
        }
    }
}

/// Unified agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent name (usually the node name).
    pub name: String,
    /// Direct message configuration.
    pub direct: DirectMessageConfig,
    /// Channel message configuration.
    pub channel: ChannelMessageConfig,
    /// TRACE path configuration.
    pub trace: TraceConfig,
    /// Contacts to add to firmware's contact list at startup.
    /// Required for DM communication - firmware needs contacts to decrypt/ACK messages.
    pub contacts: Vec<ContactTarget>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        AgentConfig {
            name: "Agent".to_string(),
            direct: DirectMessageConfig::default(),
            channel: ChannelMessageConfig::default(),
            trace: TraceConfig::default(),
            contacts: Vec::new(),
        }
    }
}

impl AgentConfig {
    /// Check if this agent has any messaging behavior enabled.
    pub fn is_enabled(&self) -> bool {
        self.direct.enabled || self.channel.enabled || self.trace.enabled
    }
}

// ============================================================================
// Protocol State
// ============================================================================

/// Protocol initialization state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolState {
    /// Not yet initialized - need to send DeviceQuery.
    Uninitialized,
    /// DeviceQuery sent, waiting for DeviceInfo response.
    AwaitingDeviceInfo,
    /// DeviceInfo received, need to send AppStart.
    DeviceInfoReceived,
    /// AppStart sent, waiting for SelfInfo response.
    AwaitingAppStart,
    /// Setting up channels, waiting for OK responses.
    SettingUpChannels,
    /// Setting up contacts, waiting for OK responses.
    SettingUpContacts,
    /// Querying contacts to verify setup (debug).
    QueryingContacts,
    /// Fully initialized and ready for messaging.
    Ready,
}

// ============================================================================
// Message State Machines
// ============================================================================

/// State for the direct message sending state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectMessageState {
    /// Waiting for startup timer.
    WaitingStartup,
    /// Path warmup: sent a probe DM, waiting for ACK or timeout.
    /// `remaining` is the number of warmup DMs left including this one.
    WaitingWarmupAck { remaining: u32 },
    /// Path warmup: waiting (session_interval_s) before sending the next probe DM.
    WaitingWarmupInterval { remaining: u32 },
    /// Ready to send (idle).
    Idle,
    /// Waiting for interval timer after ack/timeout.
    WaitingInterval,
    /// Message sent, waiting for ack.
    WaitingAck { expected_ack: u32 },
    /// Session complete, waiting for next session.
    WaitingSession,
    /// Permanently shut down (message_count or shutdown_s reached).
    Shutdown,
    /// Disabled.
    Disabled,
}

/// State for the channel message sending state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelMessageState {
    /// Waiting for startup timer.
    WaitingStartup,
    /// Ready to send (idle).
    Idle,
    /// Waiting for interval timer.
    WaitingInterval,
    /// Session complete, waiting for next session.
    WaitingSession,
    /// Permanently shut down (message_count or shutdown_s reached).
    Shutdown,
    /// Disabled.
    Disabled,
}

/// State for the TRACE path sending state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TraceState {
    /// Waiting for startup timer.
    WaitingStartup,
    /// Ready to send (idle).
    Idle,
    /// TRACE sent, waiting for TraceData push response.
    WaitingResponse,
    /// Waiting for interval timer after response/timeout.
    WaitingInterval,
    /// Permanently shut down (message_count or shutdown_s reached).
    Shutdown,
    /// Disabled.
    Disabled,
}

// ============================================================================
// Timer IDs
// ============================================================================

const TIMER_PROTOCOL_INIT: u64 = 0;
const TIMER_DIRECT_STARTUP: u64 = 1;
const TIMER_DIRECT_INTERVAL: u64 = 2;
const TIMER_DIRECT_ACK_TIMEOUT: u64 = 3;
const TIMER_DIRECT_SESSION: u64 = 4;
const TIMER_CHANNEL_STARTUP: u64 = 5;
const TIMER_CHANNEL_INTERVAL: u64 = 6;
const TIMER_CHANNEL_SESSION: u64 = 7;
const TIMER_DIRECT_SHUTDOWN: u64 = 8;
const TIMER_CHANNEL_SHUTDOWN: u64 = 9;
/// Separate timer for warmup DM ACK timeouts (avoids stale-timer interference with counted DMs).
const TIMER_DIRECT_WARMUP_TIMEOUT: u64 = 10;
const TIMER_TRACE_STARTUP: u64 = 11;
const TIMER_TRACE_INTERVAL: u64 = 12;
const TIMER_TRACE_RESPONSE_TIMEOUT: u64 = 13;
const TIMER_TRACE_SHUTDOWN: u64 = 14;

// ============================================================================
// Agent Entity
// ============================================================================

/// Unified agent entity that can send direct and/or channel messages.
pub struct Agent {
    id: EntityId,
    config: AgentConfig,
    attached_node: NodeId,
    attached_firmware: EntityId,

    // Protocol state
    protocol_session: ProtocolSession,
    protocol_state: ProtocolState,
    
    // Channel setup state
    channels_setup: usize,
    
    // Contact setup state
    contacts_setup: usize,
    
    // Direct message state
    direct_state: DirectMessageState,
    direct_target_idx: usize,
    direct_session_count: u32,
    
    // Channel message state
    channel_state: ChannelMessageState,
    channel_target_idx: usize,
    channel_session_count: u32,

    // TRACE state
    trace_state: TraceState,

    // Message counters
    message_seq: u32,
    direct_messages_sent: u32,
    channel_messages_sent: u32,
    trace_messages_sent: u32,
    messages_received: u32,

    // ACK hash tracking for metrics correctness
    // warmup_ack_hashes: hashes of warmup probe DMs (learned from Response::Sent in WaitingWarmupAck)
    // counted_ack_hashes: hashes of counted DMs that have already been acked (dedup FLOOD retries)
    warmup_ack_hashes: Vec<u32>,
    counted_ack_hashes: std::collections::HashSet<u32>,
    
    // Metrics labels for this agent
    metrics_labels: MetricLabels,
}

impl Agent {
    /// Create a new agent.
    pub fn new(
        id: EntityId,
        config: AgentConfig,
        attached_node: NodeId,
        attached_firmware: EntityId,
    ) -> Self {
        let metrics_labels = MetricLabels::new(
            config.name.clone(),
            "agent",
        );
        
        let direct_state = if config.direct.enabled {
            DirectMessageState::WaitingStartup
        } else {
            DirectMessageState::Disabled
        };
        
        let channel_state = if config.channel.enabled {
            ChannelMessageState::WaitingStartup
        } else {
            ChannelMessageState::Disabled
        };

        let trace_state = if config.trace.enabled {
            TraceState::WaitingStartup
        } else {
            TraceState::Disabled
        };

        Agent {
            id,
            config,
            attached_node,
            attached_firmware,
            protocol_session: ProtocolSession::new(),
            protocol_state: ProtocolState::Uninitialized,
            channels_setup: 0,
            contacts_setup: 0,
            direct_state,
            direct_target_idx: 0,
            direct_session_count: 0,
            channel_state,
            channel_target_idx: 0,
            channel_session_count: 0,
            trace_state,
            message_seq: 0,
            direct_messages_sent: 0,
            channel_messages_sent: 0,
            trace_messages_sent: 0,
            messages_received: 0,
            warmup_ack_hashes: Vec::new(),
            counted_ack_hashes: std::collections::HashSet::new(),
            metrics_labels,
        }
    }

    /// Get the configuration.
    pub fn config(&self) -> &AgentConfig {
        &self.config
    }
    
    /// Get the total direct messages sent.
    pub fn direct_messages_sent(&self) -> u32 {
        self.direct_messages_sent
    }
    
    /// Get the total channel messages sent.
    pub fn channel_messages_sent(&self) -> u32 {
        self.channel_messages_sent
    }

    /// Get the total TRACE packets sent.
    pub fn trace_messages_sent(&self) -> u32 {
        self.trace_messages_sent
    }

    /// Get the total messages received.
    pub fn messages_received(&self) -> u32 {
        self.messages_received
    }

    /// Get the current protocol state.
    pub fn protocol_state(&self) -> ProtocolState {
        self.protocol_state
    }

    /// Get the attached node ID.
    pub fn attached_node(&self) -> NodeId {
        self.attached_node
    }

    /// Get the attached firmware entity ID.
    pub fn attached_firmware(&self) -> EntityId {
        self.attached_firmware
    }

    // ========================================================================
    // Protocol Helpers
    // ========================================================================

    /// Send a framed command to the firmware via SerialRx event.
    fn send_command(&self, ctx: &mut SimContext, cmd: &Command) {
        let session = ProtocolSession::new();
        let frame = session.encode_command(cmd);
        trace!("Agent[{}]: Sending command {:?} ({} bytes)", self.config.name, cmd.code(), frame.len());
        ctx.post_immediate(
            vec![self.attached_firmware],
            EventPayload::SerialRx(SerialRxEvent { data: frame }),
        );
    }

    /// Initialize the protocol by sending DeviceQuery.
    fn start_initialization(&mut self, ctx: &mut SimContext) {
        debug!("Agent[{}]: Starting protocol initialization", self.config.name);
        self.send_command(ctx, &Command::DeviceQuery { app_version: 8 });
        self.protocol_state = ProtocolState::AwaitingDeviceInfo;
    }

    /// Continue initialization after DeviceInfo.
    fn send_app_start(&mut self, ctx: &mut SimContext) {
        debug!("Agent[{}]: Sending AppStart", self.config.name);
        self.send_command(
            ctx,
            &Command::AppStart {
                reserved: [0u8; 7],
                app_name: format!("MCSim-{}", self.config.name),
            },
        );
        self.protocol_state = ProtocolState::AwaitingAppStart;
    }

    /// Set up channels after receiving SelfInfo.
    fn setup_channels(&mut self, ctx: &mut SimContext) {
        // Total channels to set up = targets + subscribe_only
        let total_channels = self.config.channel.targets.len() + self.config.channel.subscribe_only.len();
        if total_channels == 0 {
            // No channels to set up, move to contact setup
            debug!("Agent[{}]: No channels to configure, setting up contacts", self.config.name);
            self.setup_contacts(ctx);
            return;
        }

        // Start setting up channels
        self.channels_setup = 0;
        self.protocol_state = ProtocolState::SettingUpChannels;
        self.send_next_channel_setup(ctx);
    }

    /// Get the channel config for a given setup index.
    /// Returns (channel_config, channel_idx) where channel_idx is the firmware slot.
    fn get_channel_for_setup(&self, setup_idx: usize) -> Option<&ChannelTarget> {
        let target_count = self.config.channel.targets.len();
        if setup_idx < target_count {
            Some(&self.config.channel.targets[setup_idx])
        } else {
            let subscribe_idx = setup_idx - target_count;
            self.config.channel.subscribe_only.get(subscribe_idx)
        }
    }

    /// Send the next SetChannel command.
    fn send_next_channel_setup(&mut self, ctx: &mut SimContext) {
        let total_channels = self.config.channel.targets.len() + self.config.channel.subscribe_only.len();
        if self.channels_setup >= total_channels {
            // All channels set up, move to contact setup
            debug!("Agent[{}]: All {} channels configured, setting up contacts", 
                self.config.name, total_channels);
            self.setup_contacts(ctx);
            return;
        }

        let channel_config = self.get_channel_for_setup(self.channels_setup).unwrap();
        let channel_idx = self.channels_setup as u8;
        let secret = channel_config.get_secret();

        debug!("Agent[{}]: Setting up channel {} at index {}", 
            self.config.name, channel_config.name, channel_idx);
        
        self.send_command(
            ctx,
            &Command::SetChannel {
                channel: ChannelInfo {
                    index: channel_idx,
                    name: channel_config.name.clone(),
                    secret,
                },
            },
        );
    }

    /// Set up contacts after channel setup.
    fn setup_contacts(&mut self, ctx: &mut SimContext) {
        if self.config.contacts.is_empty() {
            // No contacts to set up, go directly to Ready
            debug!("Agent[{}]: No contacts to configure, going to Ready", self.config.name);
            self.protocol_state = ProtocolState::Ready;
            self.on_ready(ctx);
            return;
        }

        // Start setting up contacts
        debug!("Agent[{}]: Setting up {} contacts", self.config.name, self.config.contacts.len());
        self.contacts_setup = 0;
        self.protocol_state = ProtocolState::SettingUpContacts;
        self.send_next_contact_setup(ctx);
    }

    /// Send the next AddUpdateContact command.
    fn send_next_contact_setup(&mut self, ctx: &mut SimContext) {
        if self.contacts_setup >= self.config.contacts.len() {
            // All contacts set up, query them back to verify
            ctx.tracer().log(TraceEvent::custom(
                Some(&self.config.name),
                self.id,
                ctx.time(),
                format!("All {} contacts configured, querying to verify", self.config.contacts.len()),
            ));
            self.query_contacts(ctx);
            return;
        }

        let contact = &self.config.contacts[self.contacts_setup];
        ctx.tracer().log(TraceEvent::custom(
            Some(&self.config.name),
            self.id,
            ctx.time(),
            format!("Adding contact {} (pubkey prefix: {:02x}{:02x})",
                contact.name, contact.public_key.0[0], contact.public_key.0[1]),
        ));
        
        // Create ContactInfo for the firmware
        let contact_info = ContactInfo {
            public_key: PublicKey::new(contact.public_key.0),
            contact_type: contact.contact_type,
            flags: 0,
            out_path_len: -1,  // Unknown path, will use flood routing
            out_path: [0u8; MAX_PATH_SIZE],
            name: contact.name.clone(),
            last_advert_timestamp: 0,
            gps_lat: 0,
            gps_lon: 0,
            lastmod: 0,
        };
        
        self.send_command(ctx, &Command::AddUpdateContact { contact: contact_info });
    }

    /// Query contacts from firmware to verify they were stored correctly.
    fn query_contacts(&mut self, ctx: &mut SimContext) {
        debug!("Agent[{}]: Querying contacts from firmware", self.config.name);
        self.protocol_state = ProtocolState::QueryingContacts;
        self.send_command(ctx, &Command::GetContacts { since: None });
    }

    /// Called when protocol is ready - start messaging state machines.
    fn on_ready(&mut self, ctx: &mut SimContext) {
        // Start direct message state machine
        if self.direct_state == DirectMessageState::WaitingStartup {
            let delay = self.jittered_delay(
                ctx.rng(),
                self.config.direct.startup_s,
                self.config.direct.startup_jitter_s,
            );
            ctx.post_event(delay, vec![self.id], EventPayload::Timer { timer_id: TIMER_DIRECT_STARTUP });
            
            // Schedule shutdown timer if configured
            if let Some(shutdown_s) = self.config.direct.shutdown_s {
                let shutdown_delay = SimTime::from_secs(shutdown_s);
                ctx.post_event(shutdown_delay, vec![self.id], EventPayload::Timer { timer_id: TIMER_DIRECT_SHUTDOWN });
            }
        }
        
        // Start channel message state machine
        if self.channel_state == ChannelMessageState::WaitingStartup {
            let delay = self.jittered_delay(
                ctx.rng(),
                self.config.channel.startup_s,
                self.config.channel.startup_jitter_s,
            );
            ctx.post_event(delay, vec![self.id], EventPayload::Timer { timer_id: TIMER_CHANNEL_STARTUP });

            // Schedule shutdown timer if configured
            if let Some(shutdown_s) = self.config.channel.shutdown_s {
                let shutdown_delay = SimTime::from_secs(shutdown_s);
                ctx.post_event(shutdown_delay, vec![self.id], EventPayload::Timer { timer_id: TIMER_CHANNEL_SHUTDOWN });
            }
        }

        // Start TRACE state machine
        if self.trace_state == TraceState::WaitingStartup {
            let delay = self.jittered_delay(
                ctx.rng(),
                self.config.trace.startup_s,
                self.config.trace.startup_jitter_s,
            );
            ctx.post_event(delay, vec![self.id], EventPayload::Timer { timer_id: TIMER_TRACE_STARTUP });

            // Schedule shutdown timer if configured
            if let Some(shutdown_s) = self.config.trace.shutdown_s {
                let shutdown_delay = SimTime::from_secs(shutdown_s);
                ctx.post_event(shutdown_delay, vec![self.id], EventPayload::Timer { timer_id: TIMER_TRACE_SHUTDOWN });
            }
        }
    }

    // ========================================================================
    // Timing Helpers
    // ========================================================================

    /// Calculate a delay with optional jitter (using normal distribution).
    fn jittered_delay(&self, rng: &mut ChaCha8Rng, base_s: f64, jitter_s: f64) -> SimTime {
        let delay = if jitter_s > 0.0 {
            let normal = Normal::new(base_s, jitter_s).unwrap();
            normal.sample(rng).max(0.0)
        } else {
            base_s
        };
        SimTime::from_secs(delay)
    }

    // ========================================================================
    // Direct Message State Machine
    // ========================================================================

    /// Send the next direct message.
    fn send_next_direct_message(&mut self, ctx: &mut SimContext) {
        // Check if we've hit the message count limit
        if let Some(limit) = self.config.direct.message_count {
            if self.direct_messages_sent >= limit {
                debug!("Agent[{}]: Direct message count limit reached ({})", self.config.name, limit);
                self.direct_state = DirectMessageState::Shutdown;
                return;
            }
        }
        
        if self.config.direct.targets.is_empty() {
            warn!("Agent[{}]: No direct message targets configured", self.config.name);
            self.direct_state = DirectMessageState::Disabled;
            return;
        }

        // Get next target in rotation
        let target = &self.config.direct.targets[self.direct_target_idx];
        self.direct_target_idx = (self.direct_target_idx + 1) % self.config.direct.targets.len();

        // Generate message content
        self.message_seq += 1;
        let content = format!("DM {} from {}", self.direct_messages_sent + 1, self.config.name);
        let timestamp = ctx.time().as_secs_f64() as u32;
        let recipient = PublicKeyPrefix::new(target.public_key_hash());

        ctx.tracer().log(TraceEvent::custom(
            Some(&self.config.name),
            self.id,
            ctx.time(),
            format!("Sending DM to recipient_prefix={:?} (target pubkey: {:?})",
                recipient.as_bytes(), &target.0[..8]),
        ));

        debug!(
            "Agent[{}]: Sending DM to {:?}: {}",
            self.config.name,
            recipient.to_hex(),
            content
        );

        // Record metric
        mcsim_metrics::metrics::counter!(
            metric_defs::MESSAGE_SENT.name,
            &self.metrics_labels.to_labels()
        ).increment(1);

        self.send_command(
            ctx,
            &Command::SendTextMessage {
                text_type: TextType::Plain,
                attempt: 0,
                timestamp,
                recipient_prefix: recipient,
                text: content,
            },
        );

        self.direct_messages_sent += 1;
        self.direct_session_count += 1;
        
        // Transition to waiting for ack, start timeout timer
        self.direct_state = DirectMessageState::WaitingAck { expected_ack: 0 };
        let timeout = SimTime::from_secs(self.config.direct.ack_timeout_s);
        ctx.post_event(timeout, vec![self.id], EventPayload::Timer { timer_id: TIMER_DIRECT_ACK_TIMEOUT });
    }

    /// Handle ACK received for direct message.
    /// Advance the state machine in response to a SendConfirmed ACK.
    ///
    /// The `ack_hash` is compared against the expected hash to guard against
    /// late warmup ACKs arriving in `WaitingAck` state (which would otherwise
    /// prematurely advance the counted-DM state machine).
    fn handle_direct_ack(&mut self, ack_hash: u32, ctx: &mut SimContext) {
        match self.direct_state {
            DirectMessageState::WaitingWarmupAck { remaining } => {
                // Only advance if this ACK is actually for a warmup DM.
                if self.warmup_ack_hashes.contains(&ack_hash) {
                    self.advance_warmup_or_start_session(ctx, remaining);
                }
            }
            DirectMessageState::WaitingAck { expected_ack } => {
                // Only advance if ACK matches the currently-expected counted DM.
                // A hash of 0 means Response::Sent hasn't arrived yet — ignore.
                if expected_ack != 0 && expected_ack == ack_hash {
                    self.schedule_next_direct_or_session(ctx);
                }
            }
            _ => {
                // Late/stale ACK in a state we've moved past — ignore.
            }
        }
    }

    /// Handle ACK timeout for direct message.
    fn handle_direct_timeout(&mut self, ctx: &mut SimContext) {
        match self.direct_state {
            DirectMessageState::WaitingWarmupAck { remaining } => {
                debug!("Agent[{}]: Warmup DM ACK timeout ({} remaining)", self.config.name, remaining);
                self.advance_warmup_or_start_session(ctx, remaining);
            }
            DirectMessageState::WaitingAck { .. } => {
                debug!("Agent[{}]: Direct message ACK timeout", self.config.name);
                self.schedule_next_direct_or_session(ctx);
            }
            _ => {}
        }
    }

    /// After a warmup DM ACK or timeout: send next warmup DM or start real session.
    fn advance_warmup_or_start_session(&mut self, ctx: &mut SimContext, remaining: u32) {
        if remaining > 1 {
            // More warmup DMs to send — wait session_interval_s then send next probe
            self.direct_state = DirectMessageState::WaitingWarmupInterval { remaining: remaining - 1 };
            let delay = self.jittered_delay(
                ctx.rng(),
                self.config.direct.session_interval_s,
                self.config.direct.session_interval_jitter_s,
            );
            ctx.post_event(delay, vec![self.id], EventPayload::Timer { timer_id: TIMER_DIRECT_SESSION });
        } else {
            // Warmup complete — start real counted session
            debug!("Agent[{}]: Path warmup complete, starting counted session", self.config.name);
            self.direct_state = DirectMessageState::Idle;
            self.send_next_direct_message(ctx);
        }
    }

    /// Send a single path-warmup (probe) DM to the first configured target.
    /// Does NOT increment `direct_messages_sent` or record delivery metrics.
    fn send_warmup_dm(&mut self, ctx: &mut SimContext, remaining: u32) {
        if self.config.direct.targets.is_empty() {
            warn!("Agent[{}]: No direct message targets for warmup, skipping", self.config.name);
            self.direct_state = DirectMessageState::Idle;
            self.send_next_direct_message(ctx);
            return;
        }

        // Use the first target (don't advance the rotation index)
        let target = &self.config.direct.targets[0];
        let timestamp = ctx.time().as_secs_f64() as u32;
        let recipient = PublicKeyPrefix::new(target.public_key_hash());

        ctx.tracer().log(TraceEvent::custom(
            Some(&self.config.name),
            self.id,
            ctx.time(),
            format!(
                "Sending warmup DM ({}/{}) to recipient_prefix={:?}",
                self.config.direct.path_warmup_count - remaining + 1,
                self.config.direct.path_warmup_count,
                recipient.as_bytes(),
            ),
        ));

        debug!(
            "Agent[{}]: Sending warmup DM {}/{} to {:?}",
            self.config.name,
            self.config.direct.path_warmup_count - remaining + 1,
            self.config.direct.path_warmup_count,
            recipient.to_hex(),
        );

        self.send_command(
            ctx,
            &Command::SendTextMessage {
                text_type: TextType::Plain,
                attempt: 0,
                timestamp,
                recipient_prefix: recipient,
                text: format!(
                    "Warmup {}/{} from {}",
                    self.config.direct.path_warmup_count - remaining + 1,
                    self.config.direct.path_warmup_count,
                    self.config.name,
                ),
            },
        );

        // Transition to waiting for warmup ACK; use a SEPARATE timer ID so that when the
        // warmup times out and the counted session starts, a stale warmup timer cannot
        // accidentally trigger the counted-DM timeout handler.
        self.direct_state = DirectMessageState::WaitingWarmupAck { remaining };
        let timeout = SimTime::from_secs(self.config.direct.ack_timeout_s);
        ctx.post_event(timeout, vec![self.id], EventPayload::Timer { timer_id: TIMER_DIRECT_WARMUP_TIMEOUT });
    }

    /// Schedule next direct message or session break.
    fn schedule_next_direct_or_session(&mut self, ctx: &mut SimContext) {
        // Check if session is complete
        if let Some(limit) = self.config.direct.session_message_count {
            if self.direct_session_count >= limit {
                // Session complete, schedule next session
                self.direct_session_count = 0;
                self.direct_state = DirectMessageState::WaitingSession;
                let delay = self.jittered_delay(
                    ctx.rng(),
                    self.config.direct.session_interval_s,
                    self.config.direct.session_interval_jitter_s,
                );
                ctx.post_event(delay, vec![self.id], EventPayload::Timer { timer_id: TIMER_DIRECT_SESSION });
                return;
            }
        }

        // Schedule next message
        self.direct_state = DirectMessageState::WaitingInterval;
        let delay = self.jittered_delay(
            ctx.rng(),
            self.config.direct.interval_s,
            self.config.direct.interval_jitter_s,
        );
        ctx.post_event(delay, vec![self.id], EventPayload::Timer { timer_id: TIMER_DIRECT_INTERVAL });
    }

    // ========================================================================
    // Channel Message State Machine
    // ========================================================================

    /// Send the next channel message.
    fn send_next_channel_message(&mut self, ctx: &mut SimContext) {
        // Check if we've hit the message count limit
        if let Some(limit) = self.config.channel.message_count {
            if self.channel_messages_sent >= limit {
                debug!("Agent[{}]: Channel message count limit reached ({})", self.config.name, limit);
                self.channel_state = ChannelMessageState::Shutdown;
                return;
            }
        }
        
        if self.config.channel.targets.is_empty() {
            warn!("Agent[{}]: No channel targets configured", self.config.name);
            self.channel_state = ChannelMessageState::Disabled;
            return;
        }

        // Get next channel in rotation
        let channel_idx = self.channel_target_idx as u8;
        self.channel_target_idx = (self.channel_target_idx + 1) % self.config.channel.targets.len();

        // Generate message content
        self.message_seq += 1;
        let content = format!("CH {} from {}", self.channel_messages_sent + 1, self.config.name);
        let timestamp = ctx.time().as_secs_f64() as u32;

        debug!(
            "Agent[{}]: Sending to channel {}: {}",
            self.config.name,
            channel_idx,
            content
        );

        // Record metric
        mcsim_metrics::metrics::counter!(
            metric_defs::MESSAGE_SENT.name,
            &self.metrics_labels.to_labels()
        ).increment(1);

        self.send_command(
            ctx,
            &if self.config.channel.corridor.is_empty() {
                Command::SendChannelTextMessage {
                    text_type: TextType::Plain,
                    channel_idx,
                    timestamp,
                    text: content,
                }
            } else {
                Command::SendChannelTextMessageWithCorridor {
                    text_type: TextType::Plain,
                    channel_idx,
                    timestamp,
                    text: content,
                    encoded_triples: self.config.channel.corridor.iter()
                        .map(|t| t.encode())
                        .collect(),
                }
            },
        );

        self.channel_messages_sent += 1;
        self.channel_session_count += 1;
        
        self.schedule_next_channel_or_session(ctx);
    }

    /// Schedule next channel message or session break.
    fn schedule_next_channel_or_session(&mut self, ctx: &mut SimContext) {
        // Check if session is complete
        if let Some(limit) = self.config.channel.session_message_count {
            if self.channel_session_count >= limit {
                // Session complete, schedule next session
                self.channel_session_count = 0;
                self.channel_state = ChannelMessageState::WaitingSession;
                let delay = self.jittered_delay(
                    ctx.rng(),
                    self.config.channel.session_interval_s,
                    self.config.channel.session_interval_jitter_s,
                );
                ctx.post_event(delay, vec![self.id], EventPayload::Timer { timer_id: TIMER_CHANNEL_SESSION });
                return;
            }
        }

        // Schedule next message
        self.channel_state = ChannelMessageState::WaitingInterval;
        let delay = self.jittered_delay(
            ctx.rng(),
            self.config.channel.interval_s,
            self.config.channel.interval_jitter_s,
        );
        ctx.post_event(delay, vec![self.id], EventPayload::Timer { timer_id: TIMER_CHANNEL_INTERVAL });
    }

    // ========================================================================
    // TRACE State Machine
    // ========================================================================

    /// Send the next TRACE path packet.
    fn send_next_trace(&mut self, ctx: &mut SimContext) {
        // Check if we've hit the message count limit
        if let Some(limit) = self.config.trace.message_count {
            if self.trace_messages_sent >= limit {
                debug!("Agent[{}]: TRACE message count limit reached ({})", self.config.name, limit);
                self.trace_state = TraceState::Shutdown;
                return;
            }
        }

        if self.config.trace.path.is_empty() {
            warn!("Agent[{}]: No TRACE path configured", self.config.name);
            self.trace_state = TraceState::Disabled;
            return;
        }

        // Increment tag/auth per trace so responses can be matched.
        // The base values come from config; we add the sent-counter for uniqueness.
        let tag = self.config.trace.tag.wrapping_add(self.trace_messages_sent);
        let auth = self.config.trace.auth.wrapping_add(self.trace_messages_sent);

        ctx.tracer().log(TraceEvent::custom(
            Some(&self.config.name),
            self.id,
            ctx.time(),
            format!(
                "Sending TRACE tag={:08x} auth={:08x} flags={} path_len={} path={:02x?}",
                tag, auth, self.config.trace.flags,
                self.config.trace.path.len(),
                self.config.trace.path
            ),
        ));

        debug!(
            "Agent[{}]: Sending TRACE tag={:08x} auth={:08x} path_len={}",
            self.config.name, tag, auth, self.config.trace.path.len()
        );

        self.send_command(
            ctx,
            &Command::SendTracePath {
                tag,
                auth,
                flags: self.config.trace.flags,
                path: self.config.trace.path.clone(),
            },
        );

        self.trace_messages_sent += 1;
        self.trace_state = TraceState::WaitingResponse;

        let timeout = SimTime::from_secs(self.config.trace.response_timeout_s);
        ctx.post_event(timeout, vec![self.id], EventPayload::Timer { timer_id: TIMER_TRACE_RESPONSE_TIMEOUT });
    }

    /// Handle a TraceData push notification.
    fn handle_trace_data(
        &mut self,
        tag: u32,
        auth_code: u32,
        flags: u8,
        path_hashes: &[u8],
        path_snrs: &[u8],
        final_snr_x4: i8,
        ctx: &mut SimContext,
    ) {
        // Decode SNRs (scaled by 4) for each hop.
        let snrs_db: Vec<f32> = path_snrs.iter()
            .map(|s| *s as f32 / 4.0)
            .collect();
        let final_snr_db = final_snr_x4 as f32 / 4.0;

        ctx.tracer().log(TraceEvent::custom(
            Some(&self.config.name),
            self.id,
            ctx.time(),
            format!(
                "TraceData received tag={:08x} auth={:08x} flags={} path_len={} path_hashes={:02x?} per_hop_snrs={:.1?} final_snr={:.1}",
                tag, auth_code, flags, path_hashes.len(), path_hashes, snrs_db, final_snr_db
            ),
        ));

        // Only advance our own sender state machine for responses matching the
        // currently outstanding trace. We sent with tag/auth derived from
        // config + (trace_messages_sent - 1).
        let expected_tag = self.config.trace.tag.wrapping_add(self.trace_messages_sent.saturating_sub(1));
        let expected_auth = self.config.trace.auth.wrapping_add(self.trace_messages_sent.saturating_sub(1));

        if tag == expected_tag && auth_code == expected_auth {
            debug!(
                "Agent[{}]: TraceData matches outstanding trace, advancing state machine",
                self.config.name
            );
            if self.trace_state == TraceState::WaitingResponse {
                self.schedule_next_trace(ctx);
            }
        } else {
            trace!(
                "Agent[{}]: TraceData tag/auth mismatch (got {:08x}/{:08x}, expected {:08x}/{:08x})",
                self.config.name, tag, auth_code, expected_tag, expected_auth
            );
        }
    }

    /// Handle TRACE response timeout.
    fn handle_trace_timeout(&mut self, ctx: &mut SimContext) {
        if self.trace_state == TraceState::WaitingResponse {
            debug!("Agent[{}]: TRACE response timeout", self.config.name);
            ctx.tracer().log(TraceEvent::custom(
                Some(&self.config.name),
                self.id,
                ctx.time(),
                format!(
                    "TRACE response timeout after {} sent",
                    self.trace_messages_sent
                ),
            ));
            self.schedule_next_trace(ctx);
        }
    }

    /// Schedule the next TRACE send.
    fn schedule_next_trace(&mut self, ctx: &mut SimContext) {
        self.trace_state = TraceState::WaitingInterval;
        let delay = self.jittered_delay(
            ctx.rng(),
            self.config.trace.interval_s,
            self.config.trace.interval_jitter_s,
        );
        ctx.post_event(delay, vec![self.id], EventPayload::Timer { timer_id: TIMER_TRACE_INTERVAL });
    }

    // ========================================================================
    // Message Handling
    // ========================================================================

    /// Handle a received protocol message.
    fn handle_message(&mut self, msg: Message, ctx: &mut SimContext) {
        match msg {
            Message::Response(resp) => self.handle_response(resp, ctx),
            Message::Push(push) => self.handle_push(push, ctx),
        }
    }

    /// Handle a protocol response.
    fn handle_response(&mut self, resp: Response, ctx: &mut SimContext) {
        match resp {
            Response::DeviceInfo(_info) => {
                debug!("Agent[{}]: Received DeviceInfo, sending AppStart", self.config.name);
                self.protocol_state = ProtocolState::DeviceInfoReceived;
                self.send_app_start(ctx);
            }
            Response::SelfInfo(info) => {
                debug!(
                    "Agent[{}]: Received SelfInfo, pubkey={:?}",
                    self.config.name,
                    info.public_key.to_hex()
                );
                // After SelfInfo, set up any channels before going Ready
                self.setup_channels(ctx);
            }
            Response::Ok => {
                // OK response - if we're setting up channels or contacts, move to next one
                if self.protocol_state == ProtocolState::SettingUpChannels {
                    self.channels_setup += 1;
                    self.send_next_channel_setup(ctx);
                } else if self.protocol_state == ProtocolState::SettingUpContacts {
                    self.contacts_setup += 1;
                    self.send_next_contact_setup(ctx);
                } else {
                    trace!("Agent[{}]: OK response", self.config.name);
                }
            }
            Response::Sent { expected_ack, .. } => {
                debug!(
                    "Agent[{}]: Message sent, ack_hash=0x{:08x}",
                    self.config.name, expected_ack
                );
                match self.direct_state {
                    DirectMessageState::WaitingAck { .. } => {
                        // Update expected ack for counted DMs
                        self.direct_state = DirectMessageState::WaitingAck { expected_ack };
                    }
                    DirectMessageState::WaitingWarmupAck { .. } => {
                        // Track warmup DM hash so late ACKs can be filtered out of metrics
                        self.warmup_ack_hashes.push(expected_ack);
                    }
                    _ => {}
                }
            }
            Response::ContactMessageV2(msg) | Response::ContactMessageV3(msg) => {
                ctx.tracer().log(TraceEvent::custom(
                    Some(&self.config.name),
                    self.id,
                    ctx.time(),
                    format!("Received ContactMessage from {:?}", msg.sender_prefix.to_hex()),
                ));
                self.handle_contact_message(msg, ctx);
            }
            Response::ChannelMessageV2(msg) | Response::ChannelMessageV3(msg) => {
                self.handle_channel_message(msg, ctx);
            }
            Response::Error(code) => {
                warn!("Agent[{}]: Error response: {:?}", self.config.name, code);
                // If we're setting up channels or contacts and get an error, try the next one anyway
                if self.protocol_state == ProtocolState::SettingUpChannels {
                    self.channels_setup += 1;
                    self.send_next_channel_setup(ctx);
                } else if self.protocol_state == ProtocolState::SettingUpContacts {
                    self.contacts_setup += 1;
                    self.send_next_contact_setup(ctx);
                }
            }
            Response::ContactsStart { total_count } => {
                if self.protocol_state == ProtocolState::QueryingContacts {
                    info!("Agent[{}]: Firmware reports {} contacts in GetContacts response", self.config.name, total_count);
                }
            }
            Response::Contact(contact) => {
                if self.protocol_state == ProtocolState::QueryingContacts {
                    trace!("Agent[{}]: Contact in firmware: '{}' type={} path_len={}",
                        self.config.name, contact.name, contact.contact_type, contact.out_path_len);
                }
            }
            Response::EndOfContacts { most_recent_lastmod } => {
                if self.protocol_state == ProtocolState::QueryingContacts {
                    info!("Agent[{}]: EndOfContacts received (lastmod={}), going to Ready", 
                        self.config.name, most_recent_lastmod);
                    self.protocol_state = ProtocolState::Ready;
                    self.on_ready(ctx);
                } else {
                    warn!("Agent[{}]: Received EndOfContacts in unexpected state {:?}",
                        self.config.name, self.protocol_state);
                }
            }
            _ => {
                trace!("Agent[{}]: Unhandled response: {:?}", self.config.name, resp);
            }
        }
    }

    /// Handle a received contact (DM) message.
    fn handle_contact_message(&mut self, msg: ReceivedContactMessage, ctx: &mut SimContext) {
        self.messages_received += 1;

        debug!(
            "Agent[{}]: Received DM from {:?}: {}",
            self.config.name,
            msg.sender_prefix.to_hex(),
            msg.text
        );

        // Skip delivery metrics for warmup probe DMs (text starts with "Warmup ").
        // Warmup DMs are path-discovery probes sent before the counted session; they
        // should not inflate mcsim.dm.delivered or mcsim.dm.delivery_latency.
        if !msg.text.starts_with("Warmup ") {
            mcsim_metrics::metrics::counter!(
                metric_defs::MESSAGE_DELIVERED.name,
                &self.metrics_labels.to_labels()
            ).increment(1);

            // Record delivery latency based on message timestamp.
            // The timestamp is the send time in seconds (truncated to u32).
            let receive_time_secs = ctx.time().as_secs_f64();
            let send_time_secs = msg.timestamp as f64;
            let latency_ms = (receive_time_secs - send_time_secs) * 1000.0;

            mcsim_metrics::metrics::histogram!(
                metric_defs::MESSAGE_DELIVERY_LATENCY.name,
                &self.metrics_labels.to_labels()
            ).record(latency_ms);
        }
    }

    /// Handle a received channel message.
    fn handle_channel_message(&mut self, msg: ReceivedChannelMessage, _ctx: &mut SimContext) {
        self.messages_received += 1;

        debug!(
            "Agent[{}]: Received channel {} message: {}",
            self.config.name,
            msg.channel_idx,
            msg.text
        );

        // Record metric for message received
        mcsim_metrics::metrics::counter!(
            metric_defs::MESSAGE_DELIVERED.name,
            &self.metrics_labels.to_labels()
        ).increment(1);
    }

    /// Handle a push notification.
    fn handle_push(&mut self, push: PushNotification, ctx: &mut SimContext) {
        match push {
            PushNotification::SendConfirmed { ack_hash, trip_time_ms } => {
                debug!(
                    "Agent[{}]: ACK confirmed, hash=0x{:08x}, rtt={}ms",
                    self.config.name, ack_hash, trip_time_ms
                );

                // Record metrics only for counted DMs (not warmup probes).
                // - warmup_ack_hashes: set of hashes for warmup probe DMs (tracked via Response::Sent)
                // - counted_ack_hashes: deduplicates FLOOD's multiple ACKs for the same counted DM
                let is_warmup = self.warmup_ack_hashes.contains(&ack_hash);
                if !is_warmup {
                    let is_new = self.counted_ack_hashes.insert(ack_hash);
                    if is_new {
                        // First ACK for this counted DM — record metrics
                        mcsim_metrics::metrics::counter!(
                            metric_defs::MESSAGE_ACKED.name,
                            &self.metrics_labels.to_labels()
                        ).increment(1);

                        mcsim_metrics::metrics::histogram!(
                            metric_defs::MESSAGE_ACK_LATENCY.name,
                            &self.metrics_labels.to_labels()
                        ).record(trip_time_ms as f64);
                    }
                }

                // Advance state machine only if this ACK is for the currently-expected DM.
                self.handle_direct_ack(ack_hash, ctx);
            }
            PushNotification::MessageWaiting => {
                // A message is waiting - send SyncNextMessage to retrieve it
                ctx.tracer().log(TraceEvent::custom(
                    Some(&self.config.name),
                    self.id,
                    ctx.time(),
                    "Message waiting, requesting sync",
                ));
                self.send_command(ctx, &Command::SyncNextMessage);
            }
            PushNotification::Advert { public_key } => {
                debug!(
                    "Agent[{}]: Advert received from {:?}",
                    self.config.name,
                    public_key.to_hex()
                );
            }
            PushNotification::TraceData {
                path_len: _,
                flags,
                tag,
                auth_code,
                path_hashes,
                path_snrs,
                final_snr_x4,
            } => {
                self.handle_trace_data(
                    tag,
                    auth_code,
                    flags,
                    &path_hashes,
                    &path_snrs,
                    final_snr_x4,
                    ctx,
                );
            }
            _ => {
                trace!("Agent[{}]: Unhandled push: {:?}", self.config.name, push);
            }
        }
    }
}

impl Entity for Agent {
    fn entity_id(&self) -> EntityId {
        self.id
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut SimContext) -> Result<(), SimError> {
        match &event.payload {
            EventPayload::Timer { timer_id } => {
                match *timer_id {
                    TIMER_PROTOCOL_INIT => {
                        // Initialize protocol on startup timer
                        if self.protocol_state == ProtocolState::Uninitialized {
                            self.start_initialization(ctx);
                        }
                    }
                    TIMER_DIRECT_STARTUP => {
                        // Direct messaging startup complete
                        if self.protocol_state == ProtocolState::Ready {
                            let warmup = self.config.direct.path_warmup_count;
                            if warmup > 0 {
                                self.send_warmup_dm(ctx, warmup);
                            } else {
                                self.direct_state = DirectMessageState::Idle;
                                self.send_next_direct_message(ctx);
                            }
                        }
                    }
                    TIMER_DIRECT_INTERVAL => {
                        // Time to send next direct message
                        if self.protocol_state == ProtocolState::Ready 
                            && self.direct_state == DirectMessageState::WaitingInterval 
                        {
                            self.direct_state = DirectMessageState::Idle;
                            self.send_next_direct_message(ctx);
                        }
                    }
                    TIMER_DIRECT_ACK_TIMEOUT => {
                        // ACK timeout for counted direct messages — guard against stale timers.
                        if matches!(self.direct_state, DirectMessageState::WaitingAck { .. }) {
                            self.handle_direct_timeout(ctx);
                        }
                    }
                    TIMER_DIRECT_WARMUP_TIMEOUT => {
                        // ACK timeout for warmup probe DMs — guard against stale timers.
                        if matches!(self.direct_state, DirectMessageState::WaitingWarmupAck { .. }) {
                            self.handle_direct_timeout(ctx);
                        }
                    }
                    TIMER_DIRECT_SESSION => {
                        // Direct message session break complete — also used for warmup intervals
                        if self.protocol_state == ProtocolState::Ready {
                            match self.direct_state {
                                DirectMessageState::WaitingWarmupInterval { remaining } => {
                                    self.send_warmup_dm(ctx, remaining);
                                }
                                DirectMessageState::WaitingSession => {
                                    self.direct_state = DirectMessageState::Idle;
                                    self.send_next_direct_message(ctx);
                                }
                                _ => {}
                            }
                        }
                    }
                    TIMER_CHANNEL_STARTUP => {
                        // Channel messaging startup complete
                        if self.protocol_state == ProtocolState::Ready {
                            self.channel_state = ChannelMessageState::Idle;
                            self.send_next_channel_message(ctx);
                        }
                    }
                    TIMER_CHANNEL_INTERVAL => {
                        // Time to send next channel message
                        if self.protocol_state == ProtocolState::Ready 
                            && self.channel_state == ChannelMessageState::WaitingInterval 
                        {
                            self.channel_state = ChannelMessageState::Idle;
                            self.send_next_channel_message(ctx);
                        }
                    }
                    TIMER_CHANNEL_SESSION => {
                        // Channel message session break complete
                        if self.protocol_state == ProtocolState::Ready 
                            && self.channel_state == ChannelMessageState::WaitingSession 
                        {
                            self.channel_state = ChannelMessageState::Idle;
                            self.send_next_channel_message(ctx);
                        }
                    }
                    TIMER_DIRECT_SHUTDOWN => {
                        // Direct message shutdown timer fired
                        if self.direct_state != DirectMessageState::Disabled 
                            && self.direct_state != DirectMessageState::Shutdown 
                        {
                            debug!("Agent[{}]: Direct message shutdown timer reached", self.config.name);
                            self.direct_state = DirectMessageState::Shutdown;
                        }
                    }
                    TIMER_CHANNEL_SHUTDOWN => {
                        // Channel message shutdown timer fired
                        if self.channel_state != ChannelMessageState::Disabled 
                            && self.channel_state != ChannelMessageState::Shutdown 
                        {
                            debug!("Agent[{}]: Channel message shutdown timer reached", self.config.name);
                            self.channel_state = ChannelMessageState::Shutdown;
                        }
                    }
                    TIMER_TRACE_STARTUP => {
                        // TRACE startup complete
                        if self.protocol_state == ProtocolState::Ready {
                            self.trace_state = TraceState::Idle;
                            self.send_next_trace(ctx);
                        }
                    }
                    TIMER_TRACE_INTERVAL => {
                        // Time to send next TRACE
                        if self.protocol_state == ProtocolState::Ready
                            && self.trace_state == TraceState::WaitingInterval
                        {
                            self.trace_state = TraceState::Idle;
                            self.send_next_trace(ctx);
                        }
                    }
                    TIMER_TRACE_RESPONSE_TIMEOUT => {
                        // Response timeout for TRACE
                        if self.trace_state == TraceState::WaitingResponse {
                            self.handle_trace_timeout(ctx);
                        }
                    }
                    TIMER_TRACE_SHUTDOWN => {
                        // TRACE shutdown timer fired
                        if self.trace_state != TraceState::Disabled
                            && self.trace_state != TraceState::Shutdown
                        {
                            debug!("Agent[{}]: TRACE shutdown timer reached", self.config.name);
                            self.trace_state = TraceState::Shutdown;
                        }
                    }
                    _ => {}
                }
            }
            EventPayload::SerialTx(serial_event) => {
                // Feed received serial data into the protocol decoder
                self.protocol_session.feed(&serial_event.data);

                // Process all complete messages
                loop {
                    match self.protocol_session.try_decode() {
                        Ok(Some(msg)) => {
                            match &msg {
                                Message::Response(r) => ctx.tracer().log(TraceEvent::custom(
                                    Some(&self.config.name),
                                    self.id,
                                    ctx.time(),
                                    format!("Response: {:?}", std::mem::discriminant(r)),
                                )),
                                Message::Push(p) => ctx.tracer().log(TraceEvent::custom(
                                    Some(&self.config.name),
                                    self.id,
                                    ctx.time(),
                                    format!("Push: {:?}", p),
                                )),
                            }
                            self.handle_message(msg, ctx);
                        }
                        Ok(None) => {
                            // No more complete frames, need more data
                            break;
                        }
                        Err(e) => {
                            // Protocol decode error - log and reset session
                            error!(
                                "Agent[{}]: Protocol decode error: {}. Resetting session. \
                                 State={:?}, data_len={}, first_bytes={:02x?}",
                                self.config.name,
                                e,
                                self.protocol_state,
                                serial_event.data.len(),
                                &serial_event.data[..serial_event.data.len().min(32)]
                            );
                            ctx.tracer().log(TraceEvent::custom(
                                Some(&self.config.name),
                                self.id,
                                ctx.time(),
                                format!("PROTOCOL ERROR: {}", e),
                            ));
                            // Reset the protocol session to recover
                            self.protocol_session.reset();
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

// ============================================================================
// Factory Functions
// ============================================================================

/// Create a new agent.
pub fn create_agent(
    id: EntityId,
    config: AgentConfig,
    attached_node: NodeId,
    attached_firmware: EntityId,
) -> Agent {
    Agent::new(id, config, attached_node, attached_firmware)
}

// ============================================================================
// Message Generator (Utility)
// ============================================================================

/// Random message content generator.
pub struct MessageGenerator {
    rng: ChaCha8Rng,
    word_list: Vec<String>,
}

impl MessageGenerator {
    /// Create a new message generator with the given seed.
    pub fn new(seed: u64) -> Self {
        use rand::SeedableRng;

        // Default word list
        let word_list = vec![
            "hello", "world", "test", "message", "mesh", "network", "radio", "signal", "node",
            "packet", "data", "send", "receive", "link", "route", "hop", "relay", "beacon",
            "status", "check", "copy", "roger", "over", "out",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        MessageGenerator {
            rng: ChaCha8Rng::seed_from_u64(seed),
            word_list,
        }
    }

    /// Generate a random message with the specified word count range.
    pub fn generate(&mut self, min_words: usize, max_words: usize) -> String {
        let word_count = self.rng.gen_range(min_words..=max_words);
        let mut words = Vec::with_capacity(word_count);

        for _ in 0..word_count {
            let idx = self.rng.gen_range(0..self.word_list.len());
            words.push(self.word_list[idx].clone());
        }

        words.join(" ")
    }

    /// Set a custom word list.
    pub fn set_word_list(&mut self, words: Vec<String>) {
        self.word_list = words;
    }
}

/// Create a new message generator.
pub fn create_message_generator(seed: u64) -> MessageGenerator {
    MessageGenerator::new(seed)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_generator() {
        let mut gen = MessageGenerator::new(12345);
        let msg = gen.generate(3, 5);
        let word_count = msg.split_whitespace().count();
        assert!(word_count >= 3 && word_count <= 5);
    }

    #[test]
    fn test_message_generator_deterministic() {
        let mut gen1 = MessageGenerator::new(12345);
        let mut gen2 = MessageGenerator::new(12345);

        let msg1 = gen1.generate(5, 10);
        let msg2 = gen2.generate(5, 10);

        assert_eq!(msg1, msg2);
    }

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig::default();
        assert!(!config.is_enabled());
        assert!(!config.direct.enabled);
        assert!(!config.channel.enabled);
    }

    #[test]
    fn test_channel_target_secret_derivation() {
        let target = ChannelTarget::from_name("Public".to_string());
        let secret = target.get_secret();
        // Secret should be consistent
        let secret2 = target.get_secret();
        assert_eq!(secret, secret2);
        
        // Public channel should use the well-known firmware key
        assert_eq!(secret, PUBLIC_CHANNEL_KEY);
        
        // Hash channel should derive from name
        let hash_target = ChannelTarget::from_name("#test".to_string());
        let hash_secret = hash_target.get_secret();
        // Should be deterministic
        assert_eq!(hash_secret, hash_target.get_secret());
        // Should be different from Public
        assert_ne!(hash_secret, PUBLIC_CHANNEL_KEY);
    }
}
