//! Protocol error types.

use thiserror::Error;

/// Errors that can occur when working with the companion protocol.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    /// Frame is too short to be valid.
    #[error("frame too short: expected at least {expected} bytes, got {actual}")]
    FrameTooShort {
        /// Expected minimum length.
        expected: usize,
        /// Actual length received.
        actual: usize,
    },

    /// Frame is too long.
    #[error("frame too long: maximum {max} bytes, got {actual}")]
    FrameTooLong {
        /// Maximum allowed length.
        max: usize,
        /// Actual length received.
        actual: usize,
    },

    /// Unknown command code.
    #[error("unknown command code: 0x{0:02X}")]
    UnknownCommand(u8),

    /// Unknown response code.
    #[error("unknown response code: 0x{0:02X}")]
    UnknownResponse(u8),

    /// Unknown error code from firmware.
    #[error("unknown error code: {0}")]
    UnknownErrorCode(u8),

    /// Invalid data in frame.
    #[error("invalid frame data: {0}")]
    InvalidData(String),

    /// Firmware returned an error.
    #[error("firmware error: {0}")]
    FirmwareError(FirmwareErrorCode),

    /// Feature is disabled on the firmware.
    #[error("feature disabled on firmware")]
    FeatureDisabled,

    /// UTF-8 decoding error.
    #[error("invalid UTF-8 in string field")]
    InvalidUtf8,
}

/// Error codes returned by the firmware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirmwareErrorCode {
    /// Command not supported.
    UnsupportedCommand,
    /// Contact or item not found.
    NotFound,
    /// Table (contacts, packets, etc.) is full.
    TableFull,
    /// Bad state for this operation.
    BadState,
    /// File I/O error.
    FileIoError,
    /// Illegal argument.
    IllegalArg,
    /// Unknown error code.
    Unknown(u8),
}

impl std::fmt::Display for FirmwareErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FirmwareErrorCode::UnsupportedCommand => write!(f, "unsupported command"),
            FirmwareErrorCode::NotFound => write!(f, "not found"),
            FirmwareErrorCode::TableFull => write!(f, "table full"),
            FirmwareErrorCode::BadState => write!(f, "bad state"),
            FirmwareErrorCode::FileIoError => write!(f, "file I/O error"),
            FirmwareErrorCode::IllegalArg => write!(f, "illegal argument"),
            FirmwareErrorCode::Unknown(code) => write!(f, "unknown error (0x{:02X})", code),
        }
    }
}

impl From<u8> for FirmwareErrorCode {
    fn from(code: u8) -> Self {
        use crate::constants::*;
        match code {
            ERR_CODE_UNSUPPORTED_CMD => FirmwareErrorCode::UnsupportedCommand,
            ERR_CODE_NOT_FOUND => FirmwareErrorCode::NotFound,
            ERR_CODE_TABLE_FULL => FirmwareErrorCode::TableFull,
            ERR_CODE_BAD_STATE => FirmwareErrorCode::BadState,
            ERR_CODE_FILE_IO_ERROR => FirmwareErrorCode::FileIoError,
            ERR_CODE_ILLEGAL_ARG => FirmwareErrorCode::IllegalArg,
            _ => FirmwareErrorCode::Unknown(code),
        }
    }
}

impl From<FirmwareErrorCode> for u8 {
    fn from(code: FirmwareErrorCode) -> Self {
        use crate::constants::*;
        match code {
            FirmwareErrorCode::UnsupportedCommand => ERR_CODE_UNSUPPORTED_CMD,
            FirmwareErrorCode::NotFound => ERR_CODE_NOT_FOUND,
            FirmwareErrorCode::TableFull => ERR_CODE_TABLE_FULL,
            FirmwareErrorCode::BadState => ERR_CODE_BAD_STATE,
            FirmwareErrorCode::FileIoError => ERR_CODE_FILE_IO_ERROR,
            FirmwareErrorCode::IllegalArg => ERR_CODE_ILLEGAL_ARG,
            FirmwareErrorCode::Unknown(code) => code,
        }
    }
}
