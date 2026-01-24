//! Error types for the CLI protocol.

use thiserror::Error;

/// Errors that can occur when working with the CLI protocol.
#[derive(Debug, Error)]
pub enum CliError {
    /// Failed to parse a response.
    #[error("failed to parse response: {0}")]
    ParseError(String),

    /// Invalid command format.
    #[error("invalid command: {0}")]
    InvalidCommand(String),

    /// The firmware returned an error.
    #[error("firmware error: {0}")]
    FirmwareError(String),

    /// Timeout waiting for response.
    #[error("timeout waiting for response")]
    Timeout,

    /// Buffer overflow (command or response too long).
    #[error("buffer overflow: max {max} bytes, got {actual}")]
    BufferOverflow { max: usize, actual: usize },
}

/// Result type alias for CLI operations.
pub type CliResult<T> = Result<T, CliError>;
