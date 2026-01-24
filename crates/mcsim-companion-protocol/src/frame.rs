//! Frame encoding/decoding utilities.
//!
//! The companion protocol uses a simple framing format where each frame is
//! prefixed with a 2-byte length (little-endian) followed by the frame data.
//!
//! ```text
//! +--------+--------+-------------------+
//! | len_lo | len_hi | data[0..len]      |
//! +--------+--------+-------------------+
//! ```

use bytes::{Buf, BufMut, BytesMut};

/// Maximum frame size supported.
pub const MAX_FRAMED_SIZE: usize = 1024;

/// A codec for reading and writing framed messages.
///
/// The framing format is:
/// - 2 bytes: frame length (little-endian)
/// - N bytes: frame data
#[derive(Debug, Default)]
pub struct FrameCodec {
    /// Buffer for accumulating incoming data.
    buffer: BytesMut,
}

impl FrameCodec {
    /// Create a new frame codec.
    pub fn new() -> Self {
        FrameCodec {
            buffer: BytesMut::with_capacity(MAX_FRAMED_SIZE),
        }
    }

    /// Add received data to the buffer.
    pub fn push(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Try to decode a complete frame from the buffer.
    /// Format: '>' + len_lo + len_hi + data[0..len]
    ///
    /// Returns `Some(frame_data)` if a complete frame is available,
    /// or `None` if more data is needed.
    pub fn decode(&mut self) -> Option<Vec<u8>> {
        // Scan for '>' header byte, discarding any preceding garbage
        while !self.buffer.is_empty() && self.buffer[0] != b'>' {
            self.buffer.advance(1);
        }
        
        // Need at least 3 bytes: header + 2 bytes length
        if self.buffer.len() < 3 {
            return None;
        }

        // Read the length (little-endian), after the header byte
        let len = u16::from_le_bytes([self.buffer[1], self.buffer[2]]) as usize;

        // Check if we have the complete frame (header + len bytes + data)
        if self.buffer.len() < 3 + len {
            return None;
        }

        // Extract the frame
        self.buffer.advance(3); // Skip header and length
        let frame = self.buffer.split_to(len).to_vec();

        Some(frame)
    }

    /// Encode a frame with header and length prefix for host→device transmission.
    /// Format: '<' + len_lo + len_hi + data[0..len]
    pub fn encode(data: &[u8]) -> Vec<u8> {
        let len = data.len() as u16;
        let mut buf = Vec::with_capacity(3 + data.len());
        buf.push(b'<');  // Header byte for host→device frames
        buf.put_u16_le(len);
        buf.extend_from_slice(data);
        buf
    }

    /// Get the number of buffered bytes.
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

/// A simple synchronous interface for sending commands and receiving responses.
///
/// This can be used with any byte stream (serial port, TCP socket, etc.).
pub struct ProtocolSession {
    codec: FrameCodec,
}

impl Default for ProtocolSession {
    fn default() -> Self {
        Self::new()
    }
}

impl ProtocolSession {
    /// Create a new protocol session.
    pub fn new() -> Self {
        ProtocolSession {
            codec: FrameCodec::new(),
        }
    }

    /// Encode a command for transmission.
    pub fn encode_command(&self, command: &crate::Command) -> Vec<u8> {
        FrameCodec::encode(&command.encode())
    }

    /// Feed received data into the decoder.
    pub fn feed(&mut self, data: &[u8]) {
        self.codec.push(data);
    }

    /// Try to decode the next message.
    ///
    /// Returns `Ok(Some(message))` if a complete message was decoded,
    /// `Ok(None)` if more data is needed, or `Err` if decoding failed.
    pub fn try_decode(&mut self) -> Result<Option<crate::Message>, crate::ProtocolError> {
        match self.codec.decode() {
            Some(frame) => {
                let message = crate::Message::decode(&frame)?;
                Ok(Some(message))
            }
            None => Ok(None),
        }
    }

    /// Reset the session state.
    pub fn reset(&mut self) {
        self.codec.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to encode a frame as if from device (with '>' header) for testing decode
    fn encode_as_device(data: &[u8]) -> Vec<u8> {
        let len = data.len() as u16;
        let mut buf = Vec::with_capacity(3 + data.len());
        buf.push(b'>');  // Header byte for device→host frames
        buf.put_u16_le(len);
        buf.extend_from_slice(data);
        buf
    }

    #[test]
    fn test_frame_codec_encode_decode() {
        let mut codec = FrameCodec::new();

        // Encode a frame (host→device uses '<')
        let data = b"Hello, World!";
        let encoded = FrameCodec::encode(data);

        // Should be header (1 byte) + length (2 bytes) + data
        assert_eq!(encoded.len(), 3 + data.len());
        assert_eq!(encoded[0], b'<'); // Header for host→device
        assert_eq!(encoded[1], data.len() as u8);
        assert_eq!(encoded[2], 0); // High byte of length

        // To test decode, we need device→host format (with '>')
        let device_encoded = encode_as_device(data);
        codec.push(&device_encoded);
        let decoded = codec.decode().expect("should decode frame");
        assert_eq!(&decoded, data);
    }

    #[test]
    fn test_frame_codec_partial() {
        let mut codec = FrameCodec::new();

        let data = b"Test data";
        let encoded = encode_as_device(data);

        // Feed partial data (need at least 4 bytes for header + len + some data)
        codec.push(&encoded[..4]);
        assert!(codec.decode().is_none());

        // Feed the rest
        codec.push(&encoded[4..]);
        let decoded = codec.decode().expect("should decode frame");
        assert_eq!(&decoded, data);
    }

    #[test]
    fn test_frame_codec_multiple() {
        let mut codec = FrameCodec::new();

        let data1 = b"First";
        let data2 = b"Second";
        let encoded1 = encode_as_device(data1);
        let encoded2 = encode_as_device(data2);

        // Feed both frames at once
        codec.push(&encoded1);
        codec.push(&encoded2);

        // Should decode both
        let decoded1 = codec.decode().expect("should decode first frame");
        assert_eq!(&decoded1, data1);

        let decoded2 = codec.decode().expect("should decode second frame");
        assert_eq!(&decoded2, data2);

        // No more frames
        assert!(codec.decode().is_none());
    }
}
