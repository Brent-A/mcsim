//! Common types used in the protocol.

use crate::constants::*;

/// A 32-byte public key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PublicKey(pub [u8; PUB_KEY_SIZE]);

impl PublicKey {
    /// Create a new public key from bytes.
    pub fn new(bytes: [u8; PUB_KEY_SIZE]) -> Self {
        PublicKey(bytes)
    }

    /// Create from a slice. Returns None if slice is wrong length.
    pub fn from_slice(slice: &[u8]) -> Option<Self> {
        if slice.len() == PUB_KEY_SIZE {
            let mut bytes = [0u8; PUB_KEY_SIZE];
            bytes.copy_from_slice(slice);
            Some(PublicKey(bytes))
        } else {
            None
        }
    }

    /// Get the 6-byte prefix used in many protocol messages.
    pub fn prefix(&self) -> [u8; PUB_KEY_PREFIX_SIZE] {
        let mut prefix = [0u8; PUB_KEY_PREFIX_SIZE];
        prefix.copy_from_slice(&self.0[..PUB_KEY_PREFIX_SIZE]);
        prefix
    }

    /// Get the underlying bytes.
    pub fn as_bytes(&self) -> &[u8; PUB_KEY_SIZE] {
        &self.0
    }

    /// Get the bytes as a hex string.
    pub fn to_hex(&self) -> String {
        hex_encode(&self.0)
    }
}

impl Default for PublicKey {
    fn default() -> Self {
        PublicKey([0u8; PUB_KEY_SIZE])
    }
}

impl AsRef<[u8]> for PublicKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// A 6-byte public key prefix (used in many message types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PublicKeyPrefix(pub [u8; PUB_KEY_PREFIX_SIZE]);

impl PublicKeyPrefix {
    /// Create a new prefix from bytes.
    pub fn new(bytes: [u8; PUB_KEY_PREFIX_SIZE]) -> Self {
        PublicKeyPrefix(bytes)
    }

    /// Create from a slice. Returns None if slice is wrong length.
    pub fn from_slice(slice: &[u8]) -> Option<Self> {
        if slice.len() >= PUB_KEY_PREFIX_SIZE {
            let mut bytes = [0u8; PUB_KEY_PREFIX_SIZE];
            bytes.copy_from_slice(&slice[..PUB_KEY_PREFIX_SIZE]);
            Some(PublicKeyPrefix(bytes))
        } else {
            None
        }
    }

    /// Get the underlying bytes.
    pub fn as_bytes(&self) -> &[u8; PUB_KEY_PREFIX_SIZE] {
        &self.0
    }

    /// Get the bytes as a hex string.
    pub fn to_hex(&self) -> String {
        hex_encode(&self.0)
    }
}

impl Default for PublicKeyPrefix {
    fn default() -> Self {
        PublicKeyPrefix([0u8; PUB_KEY_PREFIX_SIZE])
    }
}

impl From<&PublicKey> for PublicKeyPrefix {
    fn from(key: &PublicKey) -> Self {
        let mut prefix = [0u8; PUB_KEY_PREFIX_SIZE];
        prefix.copy_from_slice(&key.0[..PUB_KEY_PREFIX_SIZE]);
        PublicKeyPrefix(prefix)
    }
}

/// Contact information stored on the device.
#[derive(Debug, Clone)]
pub struct ContactInfo {
    /// Contact's public key.
    pub public_key: PublicKey,
    /// Contact type (Chat, Repeater, RoomServer).
    pub contact_type: u8,
    /// Contact flags.
    pub flags: u8,
    /// Outbound path length (-1 if unknown/flood).
    pub out_path_len: i8,
    /// Outbound path data.
    pub out_path: [u8; MAX_PATH_SIZE],
    /// Contact name (up to 31 chars + null).
    pub name: String,
    /// Timestamp of last advertisement.
    pub last_advert_timestamp: u32,
    /// GPS latitude (microdegrees).
    pub gps_lat: i32,
    /// GPS longitude (microdegrees).
    pub gps_lon: i32,
    /// Last modification timestamp.
    pub lastmod: u32,
}

impl Default for ContactInfo {
    fn default() -> Self {
        ContactInfo {
            public_key: PublicKey::default(),
            contact_type: ADV_TYPE_CHAT,
            flags: 0,
            out_path_len: -1,
            out_path: [0u8; MAX_PATH_SIZE],
            name: String::new(),
            last_advert_timestamp: 0,
            gps_lat: 0,
            gps_lon: 0,
            lastmod: 0,
        }
    }
}

impl ContactInfo {
    /// Get latitude as a floating point value.
    pub fn latitude(&self) -> f64 {
        self.gps_lat as f64 / 1_000_000.0
    }

    /// Get longitude as a floating point value.
    pub fn longitude(&self) -> f64 {
        self.gps_lon as f64 / 1_000_000.0
    }

    /// Check if the contact has a known direct path.
    pub fn has_direct_path(&self) -> bool {
        self.out_path_len >= 0
    }
}

/// Channel details.
#[derive(Debug, Clone)]
pub struct ChannelInfo {
    /// Channel index (0-based).
    pub index: u8,
    /// Channel name (up to 31 chars).
    pub name: String,
    /// Channel secret key (16 bytes for 128-bit).
    pub secret: [u8; 16],
}

impl Default for ChannelInfo {
    fn default() -> Self {
        ChannelInfo {
            index: 0,
            name: String::new(),
            secret: [0u8; 16],
        }
    }
}

/// Self/node information returned by CMD_APP_START.
#[derive(Debug, Clone)]
pub struct SelfInfo {
    /// Node advertisement type.
    pub advert_type: u8,
    /// Current TX power in dBm.
    pub tx_power_dbm: u8,
    /// Maximum TX power supported.
    pub max_tx_power_dbm: u8,
    /// Node's public key.
    pub public_key: PublicKey,
    /// GPS latitude (microdegrees).
    pub gps_lat: i32,
    /// GPS longitude (microdegrees).
    pub gps_lon: i32,
    /// Multi-ACK count.
    pub multi_acks: u8,
    /// Advertisement location policy.
    pub advert_loc_policy: u8,
    /// Telemetry modes packed byte.
    pub telemetry_modes: u8,
    /// Manual add contacts flag.
    pub manual_add_contacts: u8,
    /// Radio frequency in kHz.
    pub freq_khz: u32,
    /// Radio bandwidth in Hz.
    pub bandwidth_hz: u32,
    /// Spreading factor.
    pub spreading_factor: u8,
    /// Coding rate.
    pub coding_rate: u8,
    /// Node name.
    pub node_name: String,
}

impl Default for SelfInfo {
    fn default() -> Self {
        SelfInfo {
            advert_type: ADV_TYPE_CHAT,
            tx_power_dbm: 0,
            max_tx_power_dbm: 0,
            public_key: PublicKey::default(),
            gps_lat: 0,
            gps_lon: 0,
            multi_acks: 0,
            advert_loc_policy: 0,
            telemetry_modes: 0,
            manual_add_contacts: 0,
            freq_khz: 0,
            bandwidth_hz: 0,
            spreading_factor: 0,
            coding_rate: 0,
            node_name: String::new(),
        }
    }
}

impl SelfInfo {
    /// Get latitude as floating point.
    pub fn latitude(&self) -> f64 {
        self.gps_lat as f64 / 1_000_000.0
    }

    /// Get longitude as floating point.
    pub fn longitude(&self) -> f64 {
        self.gps_lon as f64 / 1_000_000.0
    }

    /// Get frequency in MHz.
    pub fn frequency_mhz(&self) -> f64 {
        self.freq_khz as f64 / 1000.0
    }

    /// Get bandwidth in kHz.
    pub fn bandwidth_khz(&self) -> f64 {
        self.bandwidth_hz as f64 / 1000.0
    }
}

/// Device information returned by CMD_DEVICE_QUERY.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Firmware version code.
    pub firmware_version_code: u8,
    /// Maximum contacts / 2.
    pub max_contacts_half: u8,
    /// Maximum group channels.
    pub max_group_channels: u8,
    /// BLE PIN code.
    pub ble_pin: u32,
    /// Firmware build date.
    pub build_date: String,
    /// Manufacturer name.
    pub manufacturer: String,
    /// Firmware version string.
    pub firmware_version: String,
}

impl Default for DeviceInfo {
    fn default() -> Self {
        DeviceInfo {
            firmware_version_code: 0,
            max_contacts_half: 0,
            max_group_channels: 0,
            ble_pin: 0,
            build_date: String::new(),
            manufacturer: String::new(),
            firmware_version: String::new(),
        }
    }
}

impl DeviceInfo {
    /// Get the maximum number of contacts supported.
    pub fn max_contacts(&self) -> usize {
        (self.max_contacts_half as usize) * 2
    }
}

/// Radio parameters.
#[derive(Debug, Clone, Copy)]
pub struct RadioParams {
    /// Frequency in kHz.
    pub freq_khz: u32,
    /// Bandwidth in Hz.
    pub bandwidth_hz: u32,
    /// Spreading factor (5-12).
    pub spreading_factor: u8,
    /// Coding rate (5-8).
    pub coding_rate: u8,
}

impl Default for RadioParams {
    fn default() -> Self {
        RadioParams {
            freq_khz: 910_525, // 910.525 MHz
            bandwidth_hz: 62_500, // 62.5 kHz
            spreading_factor: 7,
            coding_rate: 5,
        }
    }
}

/// Tuning parameters.
#[derive(Debug, Clone, Copy)]
pub struct TuningParams {
    /// RX delay base (scaled by 1000).
    pub rx_delay_base: u32,
    /// Airtime factor (scaled by 1000).
    pub airtime_factor: u32,
}

impl TuningParams {
    /// Get RX delay base as float.
    pub fn rx_delay_base_f32(&self) -> f32 {
        self.rx_delay_base as f32 / 1000.0
    }

    /// Get airtime factor as float.
    pub fn airtime_factor_f32(&self) -> f32 {
        self.airtime_factor as f32 / 1000.0
    }
}

/// Battery and storage information.
#[derive(Debug, Clone, Copy)]
pub struct BatteryAndStorage {
    /// Battery voltage in millivolts.
    pub battery_millivolts: u16,
    /// Storage used in KB.
    pub storage_used_kb: u32,
    /// Total storage in KB.
    pub storage_total_kb: u32,
}

impl BatteryAndStorage {
    /// Get battery voltage in volts.
    pub fn battery_volts(&self) -> f32 {
        self.battery_millivolts as f32 / 1000.0
    }

    /// Get storage used percentage.
    pub fn storage_used_percent(&self) -> f32 {
        if self.storage_total_kb > 0 {
            (self.storage_used_kb as f32 / self.storage_total_kb as f32) * 100.0
        } else {
            0.0
        }
    }
}

/// Message type for text messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextType {
    /// Plain text message.
    Plain,
    /// CLI/command data.
    CliData,
    /// Signed plain text.
    SignedPlain,
    /// Unknown type.
    Unknown(u8),
}

impl From<u8> for TextType {
    fn from(value: u8) -> Self {
        match value {
            TXT_TYPE_PLAIN => TextType::Plain,
            TXT_TYPE_CLI_DATA => TextType::CliData,
            TXT_TYPE_SIGNED_PLAIN => TextType::SignedPlain,
            _ => TextType::Unknown(value),
        }
    }
}

impl From<TextType> for u8 {
    fn from(value: TextType) -> Self {
        match value {
            TextType::Plain => TXT_TYPE_PLAIN,
            TextType::CliData => TXT_TYPE_CLI_DATA,
            TextType::SignedPlain => TXT_TYPE_SIGNED_PLAIN,
            TextType::Unknown(v) => v,
        }
    }
}

/// Core statistics.
#[derive(Debug, Clone, Copy)]
pub struct CoreStats {
    /// Battery voltage in millivolts.
    pub battery_mv: u16,
    /// Uptime in seconds.
    pub uptime_secs: u32,
    /// Error flags.
    pub error_flags: u16,
    /// Outbound queue length.
    pub queue_len: u8,
}

/// Radio statistics.
#[derive(Debug, Clone, Copy)]
pub struct RadioStats {
    /// Noise floor in dBm.
    pub noise_floor: i16,
    /// Last RSSI.
    pub last_rssi: i8,
    /// Last SNR (scaled by 4).
    pub last_snr_x4: i8,
    /// Total TX air time in seconds.
    pub tx_air_secs: u32,
    /// Total RX air time in seconds.
    pub rx_air_secs: u32,
}

impl RadioStats {
    /// Get the last SNR as a float.
    pub fn last_snr(&self) -> f32 {
        self.last_snr_x4 as f32 / 4.0
    }
}

/// Packet statistics.
#[derive(Debug, Clone, Copy)]
pub struct PacketStats {
    /// Total packets received (at radio level).
    pub recv: u32,
    /// Total packets sent (at radio level).
    pub sent: u32,
    /// Flood packets sent.
    pub sent_flood: u32,
    /// Direct packets sent.
    pub sent_direct: u32,
    /// Flood packets received.
    pub recv_flood: u32,
    /// Direct packets received.
    pub recv_direct: u32,
}

/// A received text message from a contact.
#[derive(Debug, Clone)]
pub struct ReceivedContactMessage {
    /// Sender's public key prefix.
    pub sender_prefix: PublicKeyPrefix,
    /// Path length (0xFF = flood).
    pub path_len: u8,
    /// Message type.
    pub text_type: TextType,
    /// Sender's timestamp.
    pub timestamp: u32,
    /// SNR (scaled by 4, only in v3+).
    pub snr_x4: Option<i8>,
    /// Extra data (for signed messages).
    pub extra: Vec<u8>,
    /// Message text.
    pub text: String,
}

impl ReceivedContactMessage {
    /// Get the SNR as a float (if available).
    pub fn snr(&self) -> Option<f32> {
        self.snr_x4.map(|s| s as f32 / 4.0)
    }

    /// Check if this was a flood message.
    pub fn is_flood(&self) -> bool {
        self.path_len == 0xFF
    }
}

/// A received text message from a channel.
#[derive(Debug, Clone)]
pub struct ReceivedChannelMessage {
    /// Channel index.
    pub channel_idx: u8,
    /// Path length (0xFF = flood).
    pub path_len: u8,
    /// Message type.
    pub text_type: TextType,
    /// Sender's timestamp.
    pub timestamp: u32,
    /// SNR (scaled by 4, only in v3+).
    pub snr_x4: Option<i8>,
    /// Message text.
    pub text: String,
}

impl ReceivedChannelMessage {
    /// Get the SNR as a float (if available).
    pub fn snr(&self) -> Option<f32> {
        self.snr_x4.map(|s| s as f32 / 4.0)
    }

    /// Check if this was a flood message.
    pub fn is_flood(&self) -> bool {
        self.path_len == 0xFF
    }
}

/// Helper to encode bytes as hex.
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
