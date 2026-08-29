//! # mcsim-agents
//!
//! Operator agent simulation for MCSim.
//!
//! This crate provides two types of agents for communicating with MeshCore firmware:
//!
//! - [`CompanionAgent`] - Communicates with MeshCore companion firmware using the
//!   binary UART protocol. Sends direct messages and/or channel messages.
//!
//! - [`CliAgent`] - Communicates with MeshCore repeater and room server firmware
//!   using the text-based CLI protocol. Applies configuration at node startup
//!   (password, CLI commands).
//!
//! ## Common Utilities
//!
//! - [`MessageGenerator`] - Random message content generator for testing.

pub mod cli_agent;
pub mod companion_agent;

// Re-export CLI agent types
pub use cli_agent::{CliAgent, CliAgentConfig, CliProtocolState, create_cli_agent};

// Re-export companion agent types
pub use companion_agent::{
    CompanionAgent, CompanionAgentConfig, CompanionProtocolState,
    ChannelMessageConfig, ChannelTarget, ContactTarget, DirectMessageConfig,
    create_companion_agent,
};

// Legacy type aliases for backward compatibility
#[deprecated(since = "0.2.0", note = "Use CompanionAgent instead")]
pub type Agent = CompanionAgent;
#[deprecated(since = "0.2.0", note = "Use CompanionAgentConfig instead")]
pub type AgentConfig = CompanionAgentConfig;
#[deprecated(since = "0.2.0", note = "Use CompanionProtocolState instead")]
pub type ProtocolState = CompanionProtocolState;
#[deprecated(since = "0.2.0", note = "Use create_companion_agent instead")]
pub fn create_agent(
    id: mcsim_common::EntityId,
    config: CompanionAgentConfig,
    attached_node: mcsim_common::NodeId,
    attached_firmware: mcsim_common::EntityId,
) -> CompanionAgent {
    create_companion_agent(id, config, attached_node, attached_firmware)
}

use rand::Rng;
use rand_chacha::ChaCha8Rng;

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
}
