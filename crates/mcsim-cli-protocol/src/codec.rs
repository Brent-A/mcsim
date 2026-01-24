//! Line-based codec for CLI communication.
//!
//! The CLI protocol uses simple line-based text communication where commands
//! are terminated with carriage return (`\r`) and responses are prefixed with
//! `  -> ` followed by the response text.

use bytes::BytesMut;

/// Maximum command/response line length.
pub const MAX_LINE_LENGTH: usize = 160;

/// Response prefix used by the firmware.
pub const RESPONSE_PREFIX: &str = "  -> ";

/// A codec for reading and writing CLI lines.
///
/// This handles the line-based nature of the CLI protocol:
/// - Accumulates received bytes until a complete line is found
/// - Detects response lines (prefixed with `  -> `)
/// - Handles echo characters that need to be filtered
#[derive(Debug, Default)]
pub struct LineCodec {
    /// Buffer for accumulating incoming data.
    buffer: BytesMut,
    /// Whether we're currently receiving echo characters.
    in_echo: bool,
    /// The last command sent (for echo filtering).
    last_command: Option<String>,
    /// Position in the last command for echo matching.
    echo_pos: usize,
}

impl LineCodec {
    /// Create a new line codec.
    pub fn new() -> Self {
        LineCodec {
            buffer: BytesMut::with_capacity(MAX_LINE_LENGTH * 2),
            in_echo: false,
            last_command: None,
            echo_pos: 0,
        }
    }

    /// Set the last command sent (used for echo filtering).
    pub fn set_last_command(&mut self, cmd: &str) {
        self.last_command = Some(cmd.to_string());
        self.echo_pos = 0;
        self.in_echo = true;
    }

    /// Clear the echo tracking state.
    pub fn clear_echo(&mut self) {
        self.last_command = None;
        self.echo_pos = 0;
        self.in_echo = false;
    }

    /// Add received data to the buffer.
    pub fn push(&mut self, data: &[u8]) {
        for &byte in data {
            // Check if this byte is part of the echo
            if self.in_echo {
                if let Some(ref cmd) = self.last_command {
                    let cmd_bytes = cmd.as_bytes();
                    if self.echo_pos < cmd_bytes.len() && byte == cmd_bytes[self.echo_pos] {
                        self.echo_pos += 1;
                        continue; // Skip echo character
                    }
                    // Check for the final \r echo
                    if self.echo_pos == cmd_bytes.len() && byte == b'\r' {
                        self.echo_pos += 1;
                        continue;
                    }
                    // Check for the \n after the echoed \r
                    if self.echo_pos == cmd_bytes.len() + 1 && byte == b'\n' {
                        self.in_echo = false;
                        continue;
                    }
                }
                // Not an echo character, stop echo mode
                self.in_echo = false;
            }
            
            self.buffer.extend_from_slice(&[byte]);
        }
    }

    /// Add received data without echo filtering.
    pub fn push_raw(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Try to decode a complete response line from the buffer.
    ///
    /// Returns `Some(response_text)` if a complete response line is available
    /// (a line starting with `  -> `), or `None` if more data is needed.
    pub fn decode_response(&mut self) -> Option<String> {
        // Look for a complete line ending with \r or \n
        let mut line_end = None;
        let mut response_start = None;
        
        for (i, window) in self.buffer.windows(RESPONSE_PREFIX.len()).enumerate() {
            if window == RESPONSE_PREFIX.as_bytes() {
                response_start = Some(i);
                break;
            }
        }
        
        if let Some(start) = response_start {
            // Find the end of this response line
            for i in start..self.buffer.len() {
                if self.buffer[i] == b'\r' || self.buffer[i] == b'\n' {
                    line_end = Some(i);
                    break;
                }
            }
            
            if let Some(end) = line_end {
                // Extract the response (without the prefix and newline)
                let response_data = &self.buffer[start + RESPONSE_PREFIX.len()..end];
                let response = String::from_utf8_lossy(response_data).to_string();
                
                // Remove everything up to and including the newline
                let mut consume_end = end;
                while consume_end < self.buffer.len() 
                    && (self.buffer[consume_end] == b'\r' || self.buffer[consume_end] == b'\n') 
                {
                    consume_end += 1;
                }
                let _ = self.buffer.split_to(consume_end);
                
                return Some(response);
            }
        }
        
        None
    }

    /// Try to decode any complete line from the buffer.
    ///
    /// This is useful for reading multi-line output like stats or logs.
    pub fn decode_line(&mut self) -> Option<String> {
        // Find the end of a line
        let mut line_end = None;
        
        for (i, &byte) in self.buffer.iter().enumerate() {
            if byte == b'\r' || byte == b'\n' {
                line_end = Some(i);
                break;
            }
        }
        
        if let Some(end) = line_end {
            // Extract the line
            let line_data = self.buffer.split_to(end);
            let line = String::from_utf8_lossy(&line_data).to_string();
            
            // Skip the newline character(s)
            while !self.buffer.is_empty() 
                && (self.buffer[0] == b'\r' || self.buffer[0] == b'\n') 
            {
                let _ = self.buffer.split_to(1);
            }
            
            // Skip empty lines
            if line.is_empty() {
                return self.decode_line();
            }
            
            return Some(line);
        }
        
        None
    }

    /// Encode a command for transmission.
    ///
    /// Appends the carriage return terminator.
    pub fn encode_command(cmd: &str) -> Vec<u8> {
        let mut buf = Vec::with_capacity(cmd.len() + 1);
        buf.extend_from_slice(cmd.as_bytes());
        buf.push(b'\r');
        buf
    }

    /// Get the number of buffered bytes.
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.clear_echo();
    }

    /// Get the current buffer contents as a string (for debugging).
    pub fn buffer_as_str(&self) -> String {
        String::from_utf8_lossy(&self.buffer).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_command() {
        let cmd = "get name";
        let encoded = LineCodec::encode_command(cmd);
        assert_eq!(encoded, b"get name\r");
    }

    #[test]
    fn test_decode_response() {
        let mut codec = LineCodec::new();
        codec.push_raw(b"  -> OK\r\n");
        
        let response = codec.decode_response();
        assert_eq!(response, Some("OK".to_string()));
    }

    #[test]
    fn test_decode_response_with_value() {
        let mut codec = LineCodec::new();
        codec.push_raw(b"  -> > my_node_name\r\n");
        
        let response = codec.decode_response();
        assert_eq!(response, Some("> my_node_name".to_string()));
    }

    #[test]
    fn test_partial_response() {
        let mut codec = LineCodec::new();
        codec.push_raw(b"  -> ");
        
        assert!(codec.decode_response().is_none());
        
        codec.push_raw(b"OK\r\n");
        
        let response = codec.decode_response();
        assert_eq!(response, Some("OK".to_string()));
    }

    #[test]
    fn test_decode_line() {
        let mut codec = LineCodec::new();
        codec.push_raw(b"line1\r\nline2\r\n");
        
        assert_eq!(codec.decode_line(), Some("line1".to_string()));
        assert_eq!(codec.decode_line(), Some("line2".to_string()));
        assert!(codec.decode_line().is_none());
    }
}
