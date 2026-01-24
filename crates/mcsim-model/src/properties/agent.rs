//! Agent configuration types extracted from properties.
//!
//! This module provides:
//! - [`DirectMessageConfig`] - Configuration for direct message sending behavior
//! - [`ChannelMessageConfig`] - Configuration for channel message sending behavior
//! - [`AgentConfig`] - Unified agent configuration extracted from resolved properties

use super::definitions::*;
use super::registry::ResolvedProperties;
use super::types::NodeScope;

// ============================================================================
// Direct Message Configuration
// ============================================================================

/// Configuration for direct message sending behavior.
#[derive(Debug, Clone)]
pub struct DirectMessageConfig {
    /// Whether direct messaging is enabled.
    pub enabled: bool,
    /// Wait time before starting direct messages.
    pub startup_s: f64,
    /// Standard deviation in the randomness of the startup interval.
    pub startup_jitter_s: f64,
    /// Names of nodes to target direct messages to.
    /// If None, all other companions will be targeted randomly.
    pub targets: Option<Vec<String>>,
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
}

impl Default for DirectMessageConfig {
    fn default() -> Self {
        DirectMessageConfig {
            enabled: false,
            startup_s: 0.0,
            startup_jitter_s: 0.0,
            targets: None,
            interval_s: 5.0,
            interval_jitter_s: 0.0,
            ack_timeout_s: 10.0,
            session_message_count: None,
            session_interval_s: 3600.0,
            session_interval_jitter_s: 0.0,
            message_count: None,
            shutdown_s: None,
        }
    }
}

// ============================================================================
// Channel Message Configuration
// ============================================================================

/// Configuration for channel message sending behavior.
#[derive(Debug, Clone)]
pub struct ChannelMessageConfig {
    /// Whether channel messaging is enabled.
    pub enabled: bool,
    /// Wait time before starting channel messages.
    pub startup_s: f64,
    /// Standard deviation in the randomness of the startup interval.
    pub startup_jitter_s: f64,
    /// Names of channels to target channel messages to.
    pub targets: Vec<String>,
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
}

impl Default for ChannelMessageConfig {
    fn default() -> Self {
        ChannelMessageConfig {
            enabled: false,
            startup_s: 0.0,
            startup_jitter_s: 0.0,
            targets: vec!["Public".to_string()],
            interval_s: 5.0,
            interval_jitter_s: 0.0,
            session_message_count: None,
            session_interval_s: 3600.0,
            session_interval_jitter_s: 0.0,
            message_count: None,
            shutdown_s: None,
        }
    }
}

// ============================================================================
// Unified Agent Configuration
// ============================================================================

/// Unified agent configuration extracted from resolved properties.
/// An agent is enabled if either direct or channel messaging is enabled.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Direct message configuration.
    pub direct: DirectMessageConfig,
    /// Channel message configuration.
    pub channel: ChannelMessageConfig,
}

impl AgentConfig {
    /// Create an AgentConfig from resolved properties.
    ///
    /// Returns `None` if neither direct nor channel messaging is enabled.
    pub fn create(props: &ResolvedProperties<NodeScope>) -> Option<Self> {
        let direct_enabled: bool = props.get(&AGENT_DIRECT_ENABLED);
        let channel_enabled: bool = props.get(&AGENT_CHANNEL_ENABLED);

        // No agent if neither is enabled
        if !direct_enabled && !channel_enabled {
            return None;
        }

        let direct = DirectMessageConfig {
            enabled: direct_enabled,
            startup_s: props.get(&AGENT_DIRECT_STARTUP_S),
            startup_jitter_s: props.get(&AGENT_DIRECT_STARTUP_JITTER_S),
            targets: props.get(&AGENT_DIRECT_TARGETS),
            interval_s: props.get(&AGENT_DIRECT_INTERVAL_S),
            interval_jitter_s: props.get(&AGENT_DIRECT_INTERVAL_JITTER_S),
            ack_timeout_s: props.get(&AGENT_DIRECT_ACK_TIMEOUT_S),
            session_message_count: props.get(&AGENT_DIRECT_SESSION_MESSAGE_COUNT),
            session_interval_s: props.get(&AGENT_DIRECT_SESSION_INTERVAL_S),
            session_interval_jitter_s: props.get(&AGENT_DIRECT_SESSION_INTERVAL_JITTER_S),
            message_count: props.get(&AGENT_DIRECT_MESSAGE_COUNT),
            shutdown_s: props.get(&AGENT_DIRECT_SHUTDOWN_S),
        };

        let channel = ChannelMessageConfig {
            enabled: channel_enabled,
            startup_s: props.get(&AGENT_CHANNEL_STARTUP_S),
            startup_jitter_s: props.get(&AGENT_CHANNEL_STARTUP_JITTER_S),
            targets: props.get(&AGENT_CHANNEL_TARGETS),
            interval_s: props.get(&AGENT_CHANNEL_INTERVAL_S),
            interval_jitter_s: props.get(&AGENT_CHANNEL_INTERVAL_JITTER_S),
            session_message_count: props.get(&AGENT_CHANNEL_SESSION_MESSAGE_COUNT),
            session_interval_s: props.get(&AGENT_CHANNEL_SESSION_INTERVAL_S),
            session_interval_jitter_s: props.get(&AGENT_CHANNEL_SESSION_INTERVAL_JITTER_S),
            message_count: props.get(&AGENT_CHANNEL_MESSAGE_COUNT),
            shutdown_s: props.get(&AGENT_CHANNEL_SHUTDOWN_S),
        };

        Some(AgentConfig { direct, channel })
    }

    /// Check if this agent has any messaging behavior enabled.
    pub fn is_enabled(&self) -> bool {
        self.direct.enabled || self.channel.enabled
    }

    /// Check if direct messaging is enabled.
    pub fn direct_enabled(&self) -> bool {
        self.direct.enabled
    }

    /// Check if channel messaging is enabled.
    pub fn channel_enabled(&self) -> bool {
        self.channel.enabled
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_none_when_disabled() {
        let props: ResolvedProperties<NodeScope> = ResolvedProperties::new();
        assert!(AgentConfig::create(&props).is_none());
    }

    #[test]
    fn test_agent_config_direct_enabled() {
        let mut props: ResolvedProperties<NodeScope> = ResolvedProperties::new();
        props.set(&AGENT_DIRECT_ENABLED, true).unwrap();
        props.set(&AGENT_DIRECT_INTERVAL_S, 10.0).unwrap();

        let config = AgentConfig::create(&props).expect("should create agent config");
        assert!(config.direct_enabled());
        assert!(!config.channel_enabled());
        assert_eq!(config.direct.interval_s, 10.0);
    }

    #[test]
    fn test_agent_config_channel_enabled() {
        let mut props: ResolvedProperties<NodeScope> = ResolvedProperties::new();
        props.set(&AGENT_CHANNEL_ENABLED, true).unwrap();
        props.set(&AGENT_CHANNEL_TARGETS, vec!["TestChannel".to_string()]).unwrap();

        let config = AgentConfig::create(&props).expect("should create agent config");
        assert!(!config.direct_enabled());
        assert!(config.channel_enabled());
        assert_eq!(config.channel.targets, vec!["TestChannel".to_string()]);
    }

    #[test]
    fn test_agent_config_both_enabled() {
        let mut props: ResolvedProperties<NodeScope> = ResolvedProperties::new();
        props.set(&AGENT_DIRECT_ENABLED, true).unwrap();
        props.set(&AGENT_CHANNEL_ENABLED, true).unwrap();

        let config = AgentConfig::create(&props).expect("should create agent config");
        assert!(config.direct_enabled());
        assert!(config.channel_enabled());
        assert!(config.is_enabled());
    }
}
