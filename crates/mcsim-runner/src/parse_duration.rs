//! Duration parsing utilities for CLI arguments.
//!
//! Supports parsing durations in various formats:
//! - Plain seconds: "60", "3600", "0.5"
//! - With units: "60s", "10m", "2h", "1d"
//! - Combined: "1h30m", "2d12h", "1d2h30m45s"

/// Parse a duration string into seconds.
///
/// Supports formats:
/// - Plain number: treated as seconds (e.g., "60" = 60 seconds)
/// - With suffix: s (seconds), m (minutes), h (hours), d (days)
/// - Combined: "1h30m" = 5400 seconds
///
/// # Examples
/// ```
/// use mcsim_runner::parse_duration::parse_duration;
///
/// assert_eq!(parse_duration("60").unwrap(), 60.0);
/// assert_eq!(parse_duration("10m").unwrap(), 600.0);
/// assert_eq!(parse_duration("1h30m").unwrap(), 5400.0);
/// ```
pub fn parse_duration(s: &str) -> Result<f64, String> {
    let s = s.trim();
    
    // If it's just a number, treat as seconds
    if let Ok(secs) = s.parse::<f64>() {
        return Ok(secs);
    }
    
    let mut total_seconds: f64 = 0.0;
    let mut current_number = String::new();
    
    for c in s.chars() {
        if c.is_ascii_digit() || c == '.' {
            current_number.push(c);
        } else {
            if current_number.is_empty() {
                return Err(format!("Invalid duration format: unexpected '{}' in '{}'", c, s));
            }
            
            let value: f64 = current_number.parse()
                .map_err(|_| format!("Invalid number '{}' in duration '{}'", current_number, s))?;
            
            let multiplier = match c {
                's' => 1.0,
                'm' => 60.0,
                'h' => 3600.0,
                'd' => 86400.0,
                _ => return Err(format!("Unknown duration unit '{}' in '{}'. Use s, m, h, or d.", c, s)),
            };
            
            total_seconds += value * multiplier;
            current_number.clear();
        }
    }
    
    // If there's a trailing number without unit, treat as seconds
    if !current_number.is_empty() {
        let value: f64 = current_number.parse()
            .map_err(|_| format!("Invalid number '{}' in duration '{}'", current_number, s))?;
        total_seconds += value;
    }
    
    if total_seconds == 0.0 && !s.is_empty() {
        return Err(format!("Invalid duration format: '{}'", s));
    }
    
    Ok(total_seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_plain_seconds() {
        assert_eq!(parse_duration("60").unwrap(), 60.0);
        assert_eq!(parse_duration("3600").unwrap(), 3600.0);
        assert_eq!(parse_duration("0.5").unwrap(), 0.5);
    }

    #[test]
    fn test_parse_duration_with_units() {
        assert_eq!(parse_duration("60s").unwrap(), 60.0);
        assert_eq!(parse_duration("10m").unwrap(), 600.0);
        assert_eq!(parse_duration("2h").unwrap(), 7200.0);
        assert_eq!(parse_duration("1d").unwrap(), 86400.0);
    }

    #[test]
    fn test_parse_duration_combined() {
        assert_eq!(parse_duration("1h30m").unwrap(), 5400.0);
        assert_eq!(parse_duration("2d12h").unwrap(), 216000.0);
        assert_eq!(parse_duration("1d2h30m45s").unwrap(), 95445.0);
        assert_eq!(parse_duration("24h10m20s").unwrap(), 87020.0);
        assert_eq!(parse_duration("3d1h").unwrap(), 262800.0);
    }

    #[test]
    fn test_parse_duration_fractional() {
        assert_eq!(parse_duration("1.5h").unwrap(), 5400.0);
        assert_eq!(parse_duration("0.5d").unwrap(), 43200.0);
    }

    #[test]
    fn test_parse_duration_errors() {
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("10x").is_err());
        assert!(parse_duration("h10").is_err());
    }
}
