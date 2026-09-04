//! CLI Agent for repeater and room server firmware.
//!
//! This agent communicates with MeshCore repeater and room server firmware
//! using the line-based CLI protocol. Unlike the companion firmware which uses
//! a binary protocol, repeaters and room servers expose a text CLI.
//!
//! The CLI agent applies configuration at node startup by:
//! 1. Optionally authenticating with a password
//! 2. Executing a list of CLI commands (e.g., "set rxdelay 0")

use mcsim_cli_protocol::{Command, LineCodec, Response};
use mcsim_common::{
    entity_tracer::TraceEvent, Entity, EntityId, Event, EventPayload, NodeId, SerialRxEvent,
    SimContext, SimError, SimTime,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, trace, warn};

// ============================================================================
// Configuration Types
// ============================================================================

/// Configuration for the CLI agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliAgentConfig {
    /// Agent name (usually the node name).
    pub name: String,
    /// Admin password to authenticate with (if required).
    pub password: Option<String>,
    /// List of CLI commands to execute at startup.
    pub commands: Vec<String>,
    /// CLI commands to send at scheduled absolute sim times, for probing runtime state
    /// (e.g. `clients`, `reach <hash>`, `get flood.suppress`) after the network has populated.
    /// Each entry is `"@<seconds> <command>"` (parsed at construction).
    pub scheduled: Vec<String>,
}

impl Default for CliAgentConfig {
    fn default() -> Self {
        CliAgentConfig {
            name: "CliAgent".to_string(),
            password: None,
            commands: Vec::new(),
            scheduled: Vec::new(),
        }
    }
}

impl CliAgentConfig {
    /// Check if this CLI agent has any configuration to apply.
    pub fn has_configuration(&self) -> bool {
        self.password.is_some() || !self.commands.is_empty() || !self.scheduled.is_empty()
    }
}

// ============================================================================
// Protocol State
// ============================================================================

/// State of the CLI agent protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliProtocolState {
    /// Waiting for initial timer to start.
    Uninitialized,
    /// Sending password authentication.
    Authenticating,
    /// Waiting for authentication response.
    AwaitingAuthResponse,
    /// Sending configuration commands.
    SendingCommands,
    /// Waiting for command response.
    AwaitingCommandResponse,
    /// All configuration complete.
    Complete,
}

// ============================================================================
// Timer IDs
// ============================================================================

const TIMER_STARTUP: u64 = 0;
const TIMER_NEXT_COMMAND: u64 = 1;

/// Base timer id for scheduled (runtime-probe) commands. Each scheduled probe gets
/// `TIMER_SCHEDULED_BASE + index`, so the timer id alone identifies which probe fired.
/// Chosen well above the startup-command timer ids (0/1) to avoid collision.
const TIMER_SCHEDULED_BASE: u64 = 0x100;

/// Delay between commands to allow firmware processing (milliseconds).
const COMMAND_DELAY_MS: u64 = 50;

// ============================================================================
// CLI Agent Entity
// ============================================================================

/// CLI agent entity for configuring repeaters and room servers.
///
/// This agent uses the text-based CLI protocol to configure firmware
/// at startup. It sends password authentication (if configured) and
/// then executes a list of CLI commands.
pub struct CliAgent {
    id: EntityId,
    config: CliAgentConfig,
    attached_node: NodeId,
    attached_firmware: EntityId,

    // Protocol state
    codec: LineCodec,
    state: CliProtocolState,
    
    // Command execution state
    command_index: usize,

    // Scheduled runtime probes (cli/scheduled): parsed (at_s, command), sorted by time.
    scheduled_commands: Vec<(f64, String)>,
    // The most recent scheduled probe awaiting a reply (for attributed trace logging).
    last_probe: Option<String>,

    // Statistics
    commands_sent: u32,
    commands_succeeded: u32,
    commands_failed: u32,
}

impl CliAgent {
    /// Create a new CLI agent.
    pub fn new(
        id: EntityId,
        config: CliAgentConfig,
        attached_node: NodeId,
        attached_firmware: EntityId,
    ) -> Self {
        // Parse scheduled probes ("@<seconds> <command>") once, sorted by time.
        let scheduled_commands = parse_scheduled(&config.scheduled);

        CliAgent {
            id,
            config,
            attached_node,
            attached_firmware,
            codec: LineCodec::new(),
            state: CliProtocolState::Uninitialized,
            command_index: 0,
            scheduled_commands,
            last_probe: None,
            commands_sent: 0,
            commands_succeeded: 0,
            commands_failed: 0,
        }
    }

    /// Get the configuration.
    pub fn config(&self) -> &CliAgentConfig {
        &self.config
    }

    /// Get the current protocol state.
    pub fn protocol_state(&self) -> CliProtocolState {
        self.state
    }

    /// Get the attached node ID.
    pub fn attached_node(&self) -> NodeId {
        self.attached_node
    }

    /// Get the attached firmware entity ID.
    pub fn attached_firmware(&self) -> EntityId {
        self.attached_firmware
    }

    /// Get the number of commands sent.
    pub fn commands_sent(&self) -> u32 {
        self.commands_sent
    }

    /// Get the number of commands that succeeded.
    pub fn commands_succeeded(&self) -> u32 {
        self.commands_succeeded
    }

    /// Get the number of commands that failed.
    pub fn commands_failed(&self) -> u32 {
        self.commands_failed
    }

    // ========================================================================
    // Protocol Helpers
    // ========================================================================

    /// Send a CLI command to the firmware via SerialRx event.
    fn send_command(&mut self, ctx: &mut SimContext, cmd: &Command) {
        let frame = cmd.encode();
        let cmd_str = cmd.to_command_string();
        
        trace!(
            "CliAgent[{}]: Sending command '{}' ({} bytes)",
            self.config.name,
            cmd_str,
            frame.len()
        );

        // Log command being sent for --trace
        ctx.tracer().log(TraceEvent::custom(
            Some(&self.config.name),
            self.id,
            ctx.time(),
            format!("CLI send: {}", cmd_str),
        ));
        
        // Track echo for the codec
        self.codec.set_last_command(&cmd_str);
        
        ctx.post_immediate(
            vec![self.attached_firmware],
            EventPayload::SerialRx(SerialRxEvent { data: frame }),
        );
        
        self.commands_sent += 1;
    }

    /// Send a raw command string to the firmware.
    fn send_raw_command(&mut self, ctx: &mut SimContext, command: &str) {
        let cmd = Command::Raw { command: command.to_string() };
        self.send_command(ctx, &cmd);
    }

    /// Start the configuration process.
    fn start_configuration(&mut self, ctx: &mut SimContext) {
        debug!(
            "CliAgent[{}]: Starting configuration (password={}, commands={})",
            self.config.name,
            self.config.password.is_some(),
            self.config.commands.len()
        );

        ctx.tracer().log(TraceEvent::custom(
            Some(&self.config.name),
            self.id,
            ctx.time(),
            format!(
                "Starting CLI configuration (password={}, {} commands)",
                self.config.password.is_some(),
                self.config.commands.len()
            ),
        ));

        // If we have a password, authenticate first
        if let Some(ref password) = self.config.password {
            self.state = CliProtocolState::Authenticating;
            let cmd = Command::SetPassword { password: password.clone() };
            self.send_command(ctx, &cmd);
            self.state = CliProtocolState::AwaitingAuthResponse;
        } else {
            // No password, skip to commands
            self.start_sending_commands(ctx);
        }

        // Schedule runtime probes (cli/scheduled) at absolute sim times. These are independent
        // of the startup command sequence: each fires once at its target time and the reply is
        // captured via the normal SerialTx path and traced as "CLI probe complete: ...".
        for (i, (at_s, _cmd)) in self.scheduled_commands.iter().enumerate() {
            let now = ctx.time();
            let target = SimTime::from_secs(*at_s);
            if target > now {
                ctx.post_event(
                    target - now,
                    vec![self.id],
                    EventPayload::Timer { timer_id: TIMER_SCHEDULED_BASE + i as u64 },
                );
            } else {
                warn!(
                    "CliAgent[{}]: scheduled probe @{}s is in the past (now={:?}); skipping",
                    self.config.name, at_s, now
                );
            }
        }
    }

    /// Start sending configuration commands.
    fn start_sending_commands(&mut self, ctx: &mut SimContext) {
        if self.config.commands.is_empty() {
            debug!("CliAgent[{}]: No commands to send, configuration complete", self.config.name);
            self.state = CliProtocolState::Complete;
            return;
        }

        self.command_index = 0;
        self.state = CliProtocolState::SendingCommands;
        self.send_next_command(ctx);
    }

    /// Send the next command in the list.
    fn send_next_command(&mut self, ctx: &mut SimContext) {
        if self.command_index >= self.config.commands.len() {
            // All commands sent
            debug!(
                "CliAgent[{}]: All {} commands sent ({} succeeded, {} failed)",
                self.config.name,
                self.config.commands.len(),
                self.commands_succeeded,
                self.commands_failed
            );

            ctx.tracer().log(TraceEvent::custom(
                Some(&self.config.name),
                self.id,
                ctx.time(),
                format!(
                    "CLI configuration complete ({} commands, {} succeeded, {} failed)",
                    self.config.commands.len(),
                    self.commands_succeeded,
                    self.commands_failed
                ),
            ));

            self.state = CliProtocolState::Complete;
            return;
        }

        let command = self.config.commands[self.command_index].clone();
        debug!(
            "CliAgent[{}]: Sending command {}/{}: '{}'",
            self.config.name,
            self.command_index + 1,
            self.config.commands.len(),
            command
        );

        self.send_raw_command(ctx, &command);
        self.state = CliProtocolState::AwaitingCommandResponse;
    }

    /// Handle a response from the firmware.
    fn handle_response(&mut self, response: Response, ctx: &mut SimContext) {
        match self.state {
            CliProtocolState::AwaitingAuthResponse => {
                if response.is_ok() {
                    debug!("CliAgent[{}]: Authentication successful", self.config.name);
                    ctx.tracer().log(TraceEvent::custom(
                        Some(&self.config.name),
                        self.id,
                        ctx.time(),
                        "CLI complete: password authentication OK".to_string(),
                    ));
                    self.commands_succeeded += 1;
                } else if response.is_error() {
                    warn!("CliAgent[{}]: Authentication failed: {:?}", self.config.name, response);
                    ctx.tracer().log(TraceEvent::custom(
                        Some(&self.config.name),
                        self.id,
                        ctx.time(),
                        format!("CLI error: password authentication failed: {:?}", response),
                    ));
                    self.commands_failed += 1;
                } else {
                    // Unknown response (e.g., "password now: xxx") - treat as success
                    debug!("CliAgent[{}]: Authentication response: {:?}", self.config.name, response);
                    ctx.tracer().log(TraceEvent::custom(
                        Some(&self.config.name),
                        self.id,
                        ctx.time(),
                        format!("CLI complete: password -> {:?}", response),
                    ));
                    self.commands_succeeded += 1;
                }
                // Proceed to commands regardless of auth result
                self.schedule_next_command(ctx);
            }
            CliProtocolState::AwaitingCommandResponse => {
                // Get the command that was just executed (index was not yet incremented)
                let current_cmd = self.config.commands.get(self.command_index)
                    .map(|s| s.as_str())
                    .unwrap_or("<unknown>");
                
                if response.is_ok() {
                    trace!("CliAgent[{}]: Command OK", self.config.name);
                    ctx.tracer().log(TraceEvent::custom(
                        Some(&self.config.name),
                        self.id,
                        ctx.time(),
                        format!("CLI complete: {} -> OK", current_cmd),
                    ));
                    self.commands_succeeded += 1;
                } else if response.is_error() {
                    warn!(
                        "CliAgent[{}]: Command failed: {:?}",
                        self.config.name, response
                    );
                    ctx.tracer().log(TraceEvent::custom(
                        Some(&self.config.name),
                        self.id,
                        ctx.time(),
                        format!("CLI error: {} -> {:?}", current_cmd, response),
                    ));
                    self.commands_failed += 1;
                } else {
                    // Value or other response - check if it's an implicit error
                    let is_implicit_error = matches!(&response, Response::Unknown(s) 
                        if s.to_lowercase().contains("unknown") 
                        || s.to_lowercase().contains("invalid")
                        || s.to_lowercase().contains("failed"));
                    
                    if is_implicit_error {
                        warn!(
                            "CliAgent[{}]: Command failed (implicit): {:?}",
                            self.config.name, response
                        );
                        ctx.tracer().log(TraceEvent::custom(
                            Some(&self.config.name),
                            self.id,
                            ctx.time(),
                            format!("CLI error: {} -> {:?}", current_cmd, response),
                        ));
                        self.commands_failed += 1;
                    } else {
                        // Value or other response - treat as success
                        trace!("CliAgent[{}]: Command response: {:?}", self.config.name, response);
                        ctx.tracer().log(TraceEvent::custom(
                            Some(&self.config.name),
                            self.id,
                            ctx.time(),
                            format!("CLI complete: {} -> {:?}", current_cmd, response),
                        ));
                        self.commands_succeeded += 1;
                    }
                }
                
                self.command_index += 1;
                self.schedule_next_command(ctx);
            }
            _ => {
                // Not in a startup-command exchange. If this is the reply to a scheduled
                // runtime probe, attribute it; otherwise just trace it.
                if let Some(cmd) = self.last_probe.take() {
                    ctx.tracer().log(TraceEvent::custom(
                        Some(&self.config.name),
                        self.id,
                        ctx.time(),
                        format!("CLI probe complete: {} -> {:?}", cmd, response),
                    ));
                } else {
                    trace!("CliAgent[{}]: Unexpected response in state {:?}: {:?}",
                        self.config.name, self.state, response);
                }
            }
        }
    }

    /// Schedule the next command with a small delay.
    fn schedule_next_command(&mut self, ctx: &mut SimContext) {
        self.state = CliProtocolState::SendingCommands;
        let delay = SimTime::from_millis(COMMAND_DELAY_MS);
        ctx.post_event(delay, vec![self.id], EventPayload::Timer { timer_id: TIMER_NEXT_COMMAND });
    }
}

impl Entity for CliAgent {
    fn entity_id(&self) -> EntityId {
        self.id
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut SimContext) -> Result<(), SimError> {
        match &event.payload {
            EventPayload::Timer { timer_id } => {
                match *timer_id {
                    TIMER_STARTUP => {
                        // Start configuration on startup timer
                        if self.state == CliProtocolState::Uninitialized {
                            self.start_configuration(ctx);
                        }
                    }
                    TIMER_NEXT_COMMAND => {
                        // Send the next command
                        if self.state == CliProtocolState::SendingCommands {
                            self.send_next_command(ctx);
                        }
                    }
                    id if id >= TIMER_SCHEDULED_BASE => {
                        // A scheduled runtime probe fired: send its command. Replies are
                        // captured by the SerialTx path; we stash the command so the response
                        // can be traced as "CLI probe complete: <cmd> -> <reply>".
                        let idx = (id - TIMER_SCHEDULED_BASE) as usize;
                        // Clone out of scheduled_commands so we don't hold an immutable borrow
                        // of self across the mutable send_raw_command call below.
                        let probe = self.scheduled_commands.get(idx).cloned();
                        if let Some((at_s, cmd)) = probe {
                            ctx.tracer().log(TraceEvent::custom(
                                Some(&self.config.name),
                                self.id,
                                ctx.time(),
                                format!("CLI probe send @{}s: {}", at_s, cmd),
                            ));
                            self.last_probe = Some(cmd.clone());
                            self.send_raw_command(ctx, &cmd);
                        }
                    }
                    _ => {}
                }
            }
            EventPayload::SerialTx(serial_event) => {
                // Feed received serial data into the codec
                self.codec.push(&serial_event.data);

                // Try to decode responses
                while let Some(response_text) = self.codec.decode_response() {
                    match Response::parse(&response_text) {
                        Ok(response) => {
                            trace!("CliAgent[{}]: Response: {:?}", self.config.name, response);
                            self.handle_response(response, ctx);
                        }
                        Err(e) => {
                            trace!("CliAgent[{}]: Failed to parse response '{}': {:?}",
                                self.config.name, response_text, e);
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

/// Parse `cli/scheduled` entries of the form `"@<seconds> <command>"` into `(secs, command)`,
/// sorted ascending by time. Malformed entries are warned and skipped. A bare `@<secs>` with no
/// command, a missing `@`, or non-numeric seconds all count as malformed.
fn parse_scheduled(entries: &[String]) -> Vec<(f64, String)> {
    let mut out = Vec::new();
    for e in entries {
        let s = e.trim();
        let body = s.strip_prefix('@').map(|b| b.trim_start()).unwrap_or(s);
        let (secs_str, cmd) = match body.split_once(char::is_whitespace) {
            Some(p) => p,
            None => {
                warn!("CliAgent: malformed cli/scheduled entry '{e}' (expected '@<secs> <command>'); skipping");
                continue;
            }
        };
        let at_s: f64 = match secs_str.parse() {
            Ok(v) => v,
            Err(_) => {
                warn!("CliAgent: cli/scheduled entry '{e}' has non-numeric seconds '{secs_str}'; skipping");
                continue;
            }
        };
        let cmd = cmd.trim();
        if cmd.is_empty() {
            warn!("CliAgent: cli/scheduled entry '{e}' has empty command; skipping");
            continue;
        }
        out.push((at_s, cmd.to_string()));
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Create a new CLI agent.
pub fn create_cli_agent(
    id: EntityId,
    config: CliAgentConfig,
    attached_node: NodeId,
    attached_firmware: EntityId,
) -> CliAgent {
    CliAgent::new(id, config, attached_node, attached_firmware)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_agent_config_default() {
        let config = CliAgentConfig::default();
        assert!(!config.has_configuration());
        assert!(config.password.is_none());
        assert!(config.commands.is_empty());
    }

    #[test]
    fn test_cli_agent_config_with_password() {
        let config = CliAgentConfig {
            name: "Test".to_string(),
            password: Some("secret".to_string()),
            commands: Vec::new(),
            scheduled: Vec::new(),
        };
        assert!(config.has_configuration());
    }

    #[test]
    fn test_cli_agent_config_with_commands() {
        let config = CliAgentConfig {
            name: "Test".to_string(),
            password: None,
            commands: vec!["set rxdelay 0".to_string()],
            scheduled: Vec::new(),
        };
        assert!(config.has_configuration());
    }

    #[test]
    fn test_parse_scheduled() {
        let entries = vec![
            "@250 clients".to_string(),
            "  @30.5 neighbors ".to_string(),   // leading space + trailing space trimmed
            "@120 reach 01020304".to_string(),   // command with argument preserved
            "@bogus nope".to_string(),           // non-numeric seconds -> skipped
            "@500".to_string(),                  // no command -> skipped
        ];
        let parsed = parse_scheduled(&entries);
        // Sorted ascending by time; 2 malformed entries dropped.
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], (30.5, "neighbors".to_string()));
        assert_eq!(parsed[1], (120.0, "reach 01020304".to_string()));
        assert_eq!(parsed[2], (250.0, "clients".to_string()));
    }

    #[test]
    fn test_cli_agent_config_with_scheduled_only() {
        // A node with only scheduled probes (no password/commands) must still count as
        // having configuration so the model builder allocates a CliAgent for it.
        let config = CliAgentConfig {
            name: "Test".to_string(),
            password: None,
            commands: Vec::new(),
            scheduled: vec!["@250 clients".to_string()],
        };
        assert!(config.has_configuration());
    }
}
