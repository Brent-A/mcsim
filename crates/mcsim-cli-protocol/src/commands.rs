//! Commands that can be sent to the repeater/room server firmware.
//!
//! The CLI supports several categories of commands:
//! - Configuration get/set commands
//! - Action commands (reboot, advert, etc.)
//! - Stats/info commands
//! - Logging commands

use crate::codec::LineCodec;

/// Configuration keys that can be read/written via `get`/`set` commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigKey {
    /// Airtime factor (`af`)
    AirtimeFactor,
    /// Interference threshold (`int.thresh`)
    InterferenceThreshold,
    /// AGC reset interval in seconds (`agc.reset.interval`)
    AgcResetInterval,
    /// Multi-ACK mode (`multi.acks`)
    MultiAcks,
    /// Allow read-only access (`allow.read.only`)
    AllowReadOnly,
    /// Flood advertisement interval in hours (`flood.advert.interval`)
    FloodAdvertInterval,
    /// Local advertisement interval in minutes (`advert.interval`)
    AdvertInterval,
    /// Guest password (`guest.password`)
    GuestPassword,
    /// Private key - serial only (`prv.key`)
    PrivateKey,
    /// Node name (`name`)
    Name,
    /// Repeat/forwarding enabled (`repeat`)
    Repeat,
    /// Node latitude (`lat`)
    Latitude,
    /// Node longitude (`lon`)
    Longitude,
    /// Radio parameters: freq,bw,sf,cr (`radio`)
    Radio,
    /// RX delay base (`rxdelay`)
    RxDelay,
    /// TX delay factor (`txdelay`)
    TxDelay,
    /// Direct TX delay factor (`direct.txdelay`)
    DirectTxDelay,
    /// Flood max hops (`flood.max`)
    FloodMax,
    /// TX power in dBm (`tx`)
    TxPower,
    /// Frequency in MHz (`freq`)
    Frequency,
    /// Public key (`public.key`)
    PublicKey,
    /// Firmware role (`role`)
    Role,
    /// Bridge type (`bridge.type`)
    BridgeType,
    /// Bridge enabled (`bridge.enabled`)
    BridgeEnabled,
    /// Bridge delay in ms (`bridge.delay`)
    BridgeDelay,
    /// Bridge packet source (`bridge.source`)
    BridgeSource,
    /// Bridge baud rate (`bridge.baud`)
    BridgeBaud,
    /// Bridge channel - ESP-NOW only (`bridge.channel`)
    BridgeChannel,
    /// Bridge secret - ESP-NOW only (`bridge.secret`)
    BridgeSecret,
    /// ADC multiplier for battery voltage (`adc.multiplier`)
    AdcMultiplier,
}

impl ConfigKey {
    /// Get the config key string used in commands.
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfigKey::AirtimeFactor => "af",
            ConfigKey::InterferenceThreshold => "int.thresh",
            ConfigKey::AgcResetInterval => "agc.reset.interval",
            ConfigKey::MultiAcks => "multi.acks",
            ConfigKey::AllowReadOnly => "allow.read.only",
            ConfigKey::FloodAdvertInterval => "flood.advert.interval",
            ConfigKey::AdvertInterval => "advert.interval",
            ConfigKey::GuestPassword => "guest.password",
            ConfigKey::PrivateKey => "prv.key",
            ConfigKey::Name => "name",
            ConfigKey::Repeat => "repeat",
            ConfigKey::Latitude => "lat",
            ConfigKey::Longitude => "lon",
            ConfigKey::Radio => "radio",
            ConfigKey::RxDelay => "rxdelay",
            ConfigKey::TxDelay => "txdelay",
            ConfigKey::DirectTxDelay => "direct.txdelay",
            ConfigKey::FloodMax => "flood.max",
            ConfigKey::TxPower => "tx",
            ConfigKey::Frequency => "freq",
            ConfigKey::PublicKey => "public.key",
            ConfigKey::Role => "role",
            ConfigKey::BridgeType => "bridge.type",
            ConfigKey::BridgeEnabled => "bridge.enabled",
            ConfigKey::BridgeDelay => "bridge.delay",
            ConfigKey::BridgeSource => "bridge.source",
            ConfigKey::BridgeBaud => "bridge.baud",
            ConfigKey::BridgeChannel => "bridge.channel",
            ConfigKey::BridgeSecret => "bridge.secret",
            ConfigKey::AdcMultiplier => "adc.multiplier",
        }
    }

    /// Parse a config key from its string representation.
    pub fn from_str(s: &str) -> Option<ConfigKey> {
        match s {
            "af" => Some(ConfigKey::AirtimeFactor),
            "int.thresh" => Some(ConfigKey::InterferenceThreshold),
            "agc.reset.interval" => Some(ConfigKey::AgcResetInterval),
            "multi.acks" => Some(ConfigKey::MultiAcks),
            "allow.read.only" => Some(ConfigKey::AllowReadOnly),
            "flood.advert.interval" => Some(ConfigKey::FloodAdvertInterval),
            "advert.interval" => Some(ConfigKey::AdvertInterval),
            "guest.password" => Some(ConfigKey::GuestPassword),
            "prv.key" => Some(ConfigKey::PrivateKey),
            "name" => Some(ConfigKey::Name),
            "repeat" => Some(ConfigKey::Repeat),
            "lat" => Some(ConfigKey::Latitude),
            "lon" => Some(ConfigKey::Longitude),
            "radio" => Some(ConfigKey::Radio),
            "rxdelay" => Some(ConfigKey::RxDelay),
            "txdelay" => Some(ConfigKey::TxDelay),
            "direct.txdelay" => Some(ConfigKey::DirectTxDelay),
            "flood.max" => Some(ConfigKey::FloodMax),
            "tx" => Some(ConfigKey::TxPower),
            "freq" => Some(ConfigKey::Frequency),
            "public.key" => Some(ConfigKey::PublicKey),
            "role" => Some(ConfigKey::Role),
            "bridge.type" => Some(ConfigKey::BridgeType),
            "bridge.enabled" => Some(ConfigKey::BridgeEnabled),
            "bridge.delay" => Some(ConfigKey::BridgeDelay),
            "bridge.source" => Some(ConfigKey::BridgeSource),
            "bridge.baud" => Some(ConfigKey::BridgeBaud),
            "bridge.channel" => Some(ConfigKey::BridgeChannel),
            "bridge.secret" => Some(ConfigKey::BridgeSecret),
            "adc.multiplier" => Some(ConfigKey::AdcMultiplier),
            _ => None,
        }
    }
}

/// Commands that can be sent to the repeater/room server CLI.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    // ========== Action Commands ==========
    /// Reboot the device.
    Reboot,

    /// Send a self-advertisement.
    Advert,

    /// Sync the clock with the given timestamp.
    /// The timestamp should be greater than the current device time.
    ClockSync,

    /// Get the current clock time.
    Clock,

    /// Set the time to a specific epoch seconds value.
    /// Only works if the new time is greater than current time.
    SetTime {
        /// Unix timestamp in seconds.
        epoch_secs: u32,
    },

    /// Start OTA update mode.
    StartOta,

    /// Erase the filesystem (serial-only).
    Erase,

    // ========== Configuration Get Commands ==========
    /// Get a configuration value.
    GetConfig {
        /// The configuration key to get.
        key: ConfigKey,
    },

    /// Get a configuration value by raw string key.
    GetConfigRaw {
        /// The configuration key string.
        key: String,
    },

    // ========== Configuration Set Commands ==========
    /// Set a configuration value.
    SetConfig {
        /// The configuration key to set.
        key: ConfigKey,
        /// The value to set.
        value: String,
    },

    /// Set a configuration value by raw string key.
    SetConfigRaw {
        /// The configuration key string.
        key: String,
        /// The value to set.
        value: String,
    },

    /// Change the admin password.
    SetPassword {
        /// The new password.
        password: String,
    },

    // ========== Radio Commands ==========
    /// Apply temporary radio parameters for a limited time.
    TempRadio {
        /// Frequency in MHz.
        freq: f32,
        /// Bandwidth in kHz.
        bw: f32,
        /// Spreading factor (5-12).
        sf: u8,
        /// Coding rate (5-8).
        cr: u8,
        /// Timeout in minutes.
        timeout_mins: u32,
    },

    // ========== Stats Commands ==========
    /// Get neighbors list.
    Neighbors,

    /// Remove a neighbor by public key prefix.
    RemoveNeighbor {
        /// Hex-encoded public key prefix.
        pubkey_hex: String,
    },

    /// Clear all statistics.
    ClearStats,

    /// Get core mesh statistics (serial-only).
    StatsCore,

    /// Get radio statistics (serial-only).
    StatsRadio,

    /// Get packet statistics (serial-only).
    StatsPackets,

    // ========== Info Commands ==========
    /// Get firmware version.
    Version,

    /// Get board/manufacturer name.
    Board,

    // ========== Sensor Commands ==========
    /// Get a sensor setting value.
    SensorGet {
        /// The sensor key to get.
        key: String,
    },

    /// Set a sensor setting value.
    SensorSet {
        /// The sensor key to set.
        key: String,
        /// The value to set.
        value: String,
    },

    /// List all sensor settings.
    SensorList {
        /// Optional start offset for pagination.
        start: Option<u32>,
    },

    // ========== GPS Commands ==========
    /// Enable GPS.
    GpsOn,

    /// Disable GPS.
    GpsOff,

    /// Sync time from GPS.
    GpsSync,

    /// Set node location from GPS.
    GpsSetLoc,

    /// Get/set GPS advertisement policy.
    GpsAdvert {
        /// Optional policy to set: "none", "share", or "prefs".
        policy: Option<String>,
    },

    /// Get GPS status.
    GpsStatus,

    // ========== Logging Commands ==========
    /// Start packet logging.
    LogStart,

    /// Stop packet logging.
    LogStop,

    /// Erase the log file.
    LogErase,

    /// Dump the log file (serial-only).
    LogDump,

    // ========== Room Server Specific ==========
    /// Set permissions for a user (room server only).
    SetPerm {
        /// Hex-encoded public key prefix.
        pubkey_hex: String,
        /// Permission value.
        permissions: u8,
    },

    /// Get the access control list (room server only, serial-only).
    GetAcl,

    // ========== Raw Command ==========
    /// Send a raw command string.
    Raw {
        /// The raw command text.
        command: String,
    },
}

impl Command {
    /// Encode the command as a line to send to the firmware.
    /// Returns the bytes to send (including the `\r` terminator).
    pub fn encode(&self) -> Vec<u8> {
        let cmd_str = self.to_command_string();
        LineCodec::encode_command(&cmd_str)
    }

    /// Get the command string without the terminator.
    pub fn to_command_string(&self) -> String {
        match self {
            // Action commands
            Command::Reboot => "reboot".to_string(),
            Command::Advert => "advert".to_string(),
            Command::ClockSync => "clock sync".to_string(),
            Command::Clock => "clock".to_string(),
            Command::SetTime { epoch_secs } => format!("time {}", epoch_secs),
            Command::StartOta => "start ota".to_string(),
            Command::Erase => "erase".to_string(),

            // Get config
            Command::GetConfig { key } => format!("get {}", key.as_str()),
            Command::GetConfigRaw { key } => format!("get {}", key),

            // Set config
            Command::SetConfig { key, value } => format!("set {} {}", key.as_str(), value),
            Command::SetConfigRaw { key, value } => format!("set {} {}", key, value),
            Command::SetPassword { password } => format!("password {}", password),

            // Radio
            Command::TempRadio { freq, bw, sf, cr, timeout_mins } => {
                format!("tempradio {} {} {} {} {}", freq, bw, sf, cr, timeout_mins)
            }

            // Stats
            Command::Neighbors => "neighbors".to_string(),
            Command::RemoveNeighbor { pubkey_hex } => format!("neighbor.remove {}", pubkey_hex),
            Command::ClearStats => "clear stats".to_string(),
            Command::StatsCore => "stats-core".to_string(),
            Command::StatsRadio => "stats-radio".to_string(),
            Command::StatsPackets => "stats-packets".to_string(),

            // Info
            Command::Version => "ver".to_string(),
            Command::Board => "board".to_string(),

            // Sensors
            Command::SensorGet { key } => format!("sensor get {}", key),
            Command::SensorSet { key, value } => format!("sensor set {} {}", key, value),
            Command::SensorList { start } => {
                if let Some(s) = start {
                    format!("sensor list {}", s)
                } else {
                    "sensor list".to_string()
                }
            }

            // GPS
            Command::GpsOn => "gps on".to_string(),
            Command::GpsOff => "gps off".to_string(),
            Command::GpsSync => "gps sync".to_string(),
            Command::GpsSetLoc => "gps setloc".to_string(),
            Command::GpsAdvert { policy } => {
                if let Some(p) = policy {
                    format!("gps advert {}", p)
                } else {
                    "gps advert".to_string()
                }
            }
            Command::GpsStatus => "gps".to_string(),

            // Logging
            Command::LogStart => "log start".to_string(),
            Command::LogStop => "log stop".to_string(),
            Command::LogErase => "log erase".to_string(),
            Command::LogDump => "log".to_string(),

            // Room server specific
            Command::SetPerm { pubkey_hex, permissions } => {
                format!("setperm {} {}", pubkey_hex, permissions)
            }
            Command::GetAcl => "get acl".to_string(),

            // Raw
            Command::Raw { command } => command.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_reboot() {
        let cmd = Command::Reboot;
        assert_eq!(cmd.encode(), b"reboot\r");
    }

    #[test]
    fn test_encode_get_config() {
        let cmd = Command::GetConfig { key: ConfigKey::Name };
        assert_eq!(cmd.encode(), b"get name\r");
    }

    #[test]
    fn test_encode_set_config() {
        let cmd = Command::SetConfig {
            key: ConfigKey::Name,
            value: "MyNode".to_string(),
        };
        assert_eq!(cmd.encode(), b"set name MyNode\r");
    }

    #[test]
    fn test_encode_temp_radio() {
        let cmd = Command::TempRadio {
            freq: 915.0,
            bw: 250.0,
            sf: 10,
            cr: 5,
            timeout_mins: 60,
        };
        assert_eq!(cmd.encode(), b"tempradio 915 250 10 5 60\r");
    }

    #[test]
    fn test_encode_set_perm() {
        let cmd = Command::SetPerm {
            pubkey_hex: "AABBCCDD".to_string(),
            permissions: 3,
        };
        assert_eq!(cmd.encode(), b"setperm AABBCCDD 3\r");
    }
}
