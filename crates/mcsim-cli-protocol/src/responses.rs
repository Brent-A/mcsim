//! Response parsing for CLI protocol.
//!
//! Responses from the firmware are prefixed with `  -> ` and can be:
//! - Simple status: `OK`, `Error`, etc.
//! - Values: `> <value>` for get commands
//! - Multi-line output: stats, logs, etc.

use crate::error::{CliError, CliResult};

/// Parsed response from the CLI.
#[derive(Debug, Clone, PartialEq)]
pub enum Response {
    /// Simple OK response.
    Ok,

    /// OK with additional message.
    OkMessage(String),

    /// A value response (from `get` commands).
    /// The `> ` prefix is stripped.
    Value(String),

    /// An error response.
    Error(String),

    /// Clock time response.
    ClockTime {
        /// Hours (0-23).
        hour: u8,
        /// Minutes (0-59).
        minute: u8,
        /// Day of month (1-31).
        day: u8,
        /// Month (1-12).
        month: u8,
        /// Year (e.g., 2024).
        year: u16,
    },

    /// Firmware version response.
    Version {
        /// Version string (e.g., "v1.11.0").
        version: String,
        /// Build date string.
        build_date: String,
    },

    /// Unknown/unrecognized response.
    Unknown(String),
}

impl Response {
    /// Parse a response line.
    ///
    /// The input should be the response text without the `  -> ` prefix.
    pub fn parse(text: &str) -> CliResult<Response> {
        let text = text.trim();

        // Check for simple OK
        if text == "OK" {
            return Ok(Response::Ok);
        }

        // Check for OK with message
        if text.starts_with("OK - ") {
            return Ok(Response::OkMessage(text[5..].to_string()));
        }

        // Check for value response (from get commands)
        if text.starts_with("> ") {
            return Ok(Response::Value(text[2..].to_string()));
        }

        // Check for error responses
        if text.starts_with("ERR:") || text.starts_with("Error") || text.starts_with("error") {
            return Ok(Response::Error(text.to_string()));
        }

        // Check for clock response: "HH:MM - D/M/YYYY UTC"
        if let Some(response) = Self::try_parse_clock(text) {
            return Ok(response);
        }

        // Check for version response: "vX.Y.Z (Build: ...)"
        if let Some(response) = Self::try_parse_version(text) {
            return Ok(response);
        }

        // Unknown response
        Ok(Response::Unknown(text.to_string()))
    }

    /// Try to parse a clock time response.
    fn try_parse_clock(text: &str) -> Option<Response> {
        // Format: "HH:MM - D/M/YYYY UTC" or "HH:MM:SS - D/M/YYYY U"
        let parts: Vec<&str> = text.split(" - ").collect();
        if parts.len() != 2 {
            return None;
        }

        let time_parts: Vec<&str> = parts[0].split(':').collect();
        if time_parts.len() < 2 {
            return None;
        }

        let hour: u8 = time_parts[0].parse().ok()?;
        let minute: u8 = time_parts[1].parse().ok()?;

        // Date part: "D/M/YYYY UTC" or "D/M/YYYY U"
        let date_str = parts[1].trim_end_matches(" UTC").trim_end_matches(" U");
        let date_parts: Vec<&str> = date_str.split('/').collect();
        if date_parts.len() != 3 {
            return None;
        }

        let day: u8 = date_parts[0].parse().ok()?;
        let month: u8 = date_parts[1].parse().ok()?;
        let year: u16 = date_parts[2].parse().ok()?;

        Some(Response::ClockTime { hour, minute, day, month, year })
    }

    /// Try to parse a version response.
    fn try_parse_version(text: &str) -> Option<Response> {
        // Format: "vX.Y.Z (Build: DATE)"
        if !text.starts_with('v') {
            return None;
        }

        if let Some(build_start) = text.find("(Build: ") {
            let version = text[..build_start].trim().to_string();
            let build_date = text[build_start + 8..].trim_end_matches(')').to_string();
            return Some(Response::Version { version, build_date });
        }

        None
    }

    /// Check if this is an OK response (either plain OK or OK with message).
    pub fn is_ok(&self) -> bool {
        matches!(self, Response::Ok | Response::OkMessage(_))
    }

    /// Check if this is an error response.
    pub fn is_error(&self) -> bool {
        matches!(self, Response::Error(_))
    }

    /// Get the value if this is a Value response.
    pub fn as_value(&self) -> Option<&str> {
        match self {
            Response::Value(v) => Some(v),
            _ => None,
        }
    }

    /// Get the error message if this is an Error response.
    pub fn as_error(&self) -> Option<&str> {
        match self {
            Response::Error(e) => Some(e),
            _ => None,
        }
    }
}

/// Radio parameters parsed from `get radio` response.
#[derive(Debug, Clone, PartialEq)]
pub struct RadioParams {
    /// Frequency in MHz.
    pub freq: f32,
    /// Bandwidth in kHz.
    pub bw: f32,
    /// Spreading factor (5-12).
    pub sf: u8,
    /// Coding rate (5-8).
    pub cr: u8,
}

impl RadioParams {
    /// Parse radio parameters from a value string.
    ///
    /// Format: "freq,bw,sf,cr" (e.g., "915.0,250.0,10,5")
    pub fn parse(value: &str) -> CliResult<RadioParams> {
        let parts: Vec<&str> = value.split(',').collect();
        if parts.len() != 4 {
            return Err(CliError::ParseError(format!(
                "expected 4 parts, got {}: {}",
                parts.len(),
                value
            )));
        }

        let freq: f32 = parts[0]
            .parse()
            .map_err(|_| CliError::ParseError(format!("invalid freq: {}", parts[0])))?;
        let bw: f32 = parts[1]
            .parse()
            .map_err(|_| CliError::ParseError(format!("invalid bw: {}", parts[1])))?;
        let sf: u8 = parts[2]
            .parse()
            .map_err(|_| CliError::ParseError(format!("invalid sf: {}", parts[2])))?;
        let cr: u8 = parts[3]
            .parse()
            .map_err(|_| CliError::ParseError(format!("invalid cr: {}", parts[3])))?;

        Ok(RadioParams { freq, bw, sf, cr })
    }
}

/// Neighbor information parsed from `neighbors` response.
#[derive(Debug, Clone, PartialEq)]
pub struct NeighborInfo {
    /// Public key prefix (hex string).
    pub pubkey_prefix: String,
    /// Name (if known).
    pub name: Option<String>,
    /// Last heard timestamp or "seconds ago".
    pub last_heard: String,
    /// Signal-to-noise ratio.
    pub snr: Option<f32>,
}

/// GPS status parsed from `gps` response.
#[derive(Debug, Clone, PartialEq)]
pub struct GpsStatus {
    /// Whether GPS is enabled (EN pin on).
    pub enabled: bool,
    /// Whether GPS is active (setting).
    pub active: bool,
    /// Whether GPS has a fix.
    pub has_fix: bool,
    /// Number of satellites.
    pub satellites: u8,
}

impl GpsStatus {
    /// Parse GPS status from response text.
    ///
    /// Format: "on, active, fix, N sats" or "off"
    pub fn parse(text: &str) -> CliResult<GpsStatus> {
        let text = text.trim();
        
        if text == "off" {
            return Ok(GpsStatus {
                enabled: false,
                active: false,
                has_fix: false,
                satellites: 0,
            });
        }

        // Parse "on, active/deactivated, fix/no fix, N sats"
        let parts: Vec<&str> = text.split(", ").collect();
        if parts.is_empty() || parts[0] != "on" {
            return Err(CliError::ParseError(format!("unexpected GPS status: {}", text)));
        }

        let active = parts.get(1).map(|&s| s == "active").unwrap_or(false);
        let has_fix = parts.get(2).map(|&s| s == "fix").unwrap_or(false);
        let satellites = parts
            .get(3)
            .and_then(|s| s.trim_end_matches(" sats").parse().ok())
            .unwrap_or(0);

        Ok(GpsStatus {
            enabled: true,
            active,
            has_fix,
            satellites,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ok() {
        let response = Response::parse("OK").unwrap();
        assert_eq!(response, Response::Ok);
        assert!(response.is_ok());
    }

    #[test]
    fn test_parse_ok_message() {
        let response = Response::parse("OK - repeat is now ON").unwrap();
        assert_eq!(response, Response::OkMessage("repeat is now ON".to_string()));
        assert!(response.is_ok());
    }

    #[test]
    fn test_parse_value() {
        let response = Response::parse("> my_node_name").unwrap();
        assert_eq!(response, Response::Value("my_node_name".to_string()));
        assert_eq!(response.as_value(), Some("my_node_name"));
    }

    #[test]
    fn test_parse_error() {
        let response = Response::parse("ERR: clock cannot go backwards").unwrap();
        assert!(response.is_error());
    }

    #[test]
    fn test_parse_clock() {
        let response = Response::parse("12:30 - 15/6/2024 UTC").unwrap();
        assert_eq!(
            response,
            Response::ClockTime {
                hour: 12,
                minute: 30,
                day: 15,
                month: 6,
                year: 2024,
            }
        );
    }

    #[test]
    fn test_parse_version() {
        let response = Response::parse("v1.11.0 (Build: 30 Nov 2025)").unwrap();
        assert_eq!(
            response,
            Response::Version {
                version: "v1.11.0".to_string(),
                build_date: "30 Nov 2025".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_radio_params() {
        let params = RadioParams::parse("915.0,250.0,10,5").unwrap();
        assert_eq!(params.freq, 915.0);
        assert_eq!(params.bw, 250.0);
        assert_eq!(params.sf, 10);
        assert_eq!(params.cr, 5);
    }

    #[test]
    fn test_parse_gps_status_off() {
        let status = GpsStatus::parse("off").unwrap();
        assert!(!status.enabled);
    }

    #[test]
    fn test_parse_gps_status_on() {
        let status = GpsStatus::parse("on, active, fix, 8 sats").unwrap();
        assert!(status.enabled);
        assert!(status.active);
        assert!(status.has_fix);
        assert_eq!(status.satellites, 8);
    }
}
