//! Error types for meshcore-packet.

use thiserror::Error;

/// Errors that can occur during packet operations.
#[derive(Debug, Error)]
pub enum PacketError {
    /// Invalid packet format.
    #[error("Invalid packet format: {0}")]
    InvalidFormat(String),

    /// Decode error at a specific offset.
    #[error("Decode error at offset {offset}: {message}")]
    DecodeError {
        /// Byte offset where the error occurred.
        offset: usize,
        /// Description of the error.
        message: String,
    },

    /// Encryption error.
    #[error("Encryption error: {0}")]
    EncryptionError(String),

    /// Invalid signature.
    #[error("Invalid signature")]
    InvalidSignature,

    /// Packet too large.
    #[error("Packet too large: {size} bytes (max {max})")]
    TooLarge {
        /// Actual size.
        size: usize,
        /// Maximum allowed size.
        max: usize,
    },

    /// Invalid packet type.
    #[error("Invalid packet type: {0}")]
    InvalidPacketType(u8),

    /// Missing required field.
    #[error("Missing required field: {0}")]
    MissingField(String),

    /// Invalid UTF-8 string.
    #[error("Invalid UTF-8 string: {0}")]
    InvalidUtf8(String),

    /// Checksum mismatch.
    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch {
        /// Expected checksum.
        expected: String,
        /// Actual checksum.
        actual: String,
    },
}

impl PacketError {
    /// Create a decode error at a specific offset.
    pub fn decode_at(offset: usize, message: impl Into<String>) -> Self {
        PacketError::DecodeError {
            offset,
            message: message.into(),
        }
    }

    /// Create an invalid format error.
    pub fn invalid_format(message: impl Into<String>) -> Self {
        PacketError::InvalidFormat(message.into())
    }

    /// Create an encryption error.
    pub fn encryption(message: impl Into<String>) -> Self {
        PacketError::EncryptionError(message.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = PacketError::decode_at(10, "unexpected byte");
        assert!(err.to_string().contains("offset 10"));

        let err = PacketError::invalid_format("missing header");
        assert!(err.to_string().contains("missing header"));
    }
}
