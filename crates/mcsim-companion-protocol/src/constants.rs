//! Protocol constants
//!
//! These constants define the command codes, response codes, and other
//! protocol-specific values used in the MeshCore companion UART protocol.

// ============================================================================
// Command Codes (host → firmware)
// ============================================================================

/// Initial handshake command - starts the app connection.
pub const CMD_APP_START: u8 = 1;
/// Send a text message to a contact.
pub const CMD_SEND_TXT_MSG: u8 = 2;
/// Send a text message to a channel.
pub const CMD_SEND_CHANNEL_TXT_MSG: u8 = 3;
/// Get the list of contacts (with optional 'since' filter).
pub const CMD_GET_CONTACTS: u8 = 4;
/// Get the current device time.
pub const CMD_GET_DEVICE_TIME: u8 = 5;
/// Set the device time.
pub const CMD_SET_DEVICE_TIME: u8 = 6;
/// Send a self-advertisement packet.
pub const CMD_SEND_SELF_ADVERT: u8 = 7;
/// Set the advertisement name.
pub const CMD_SET_ADVERT_NAME: u8 = 8;
/// Add or update a contact.
pub const CMD_ADD_UPDATE_CONTACT: u8 = 9;
/// Sync the next message from the offline queue.
pub const CMD_SYNC_NEXT_MESSAGE: u8 = 10;
/// Set radio parameters (frequency, bandwidth, SF, CR).
pub const CMD_SET_RADIO_PARAMS: u8 = 11;
/// Set radio TX power.
pub const CMD_SET_RADIO_TX_POWER: u8 = 12;
/// Reset the path to a contact (force re-discovery).
pub const CMD_RESET_PATH: u8 = 13;
/// Set advertisement latitude/longitude.
pub const CMD_SET_ADVERT_LATLON: u8 = 14;
/// Remove a contact.
pub const CMD_REMOVE_CONTACT: u8 = 15;
/// Share a contact via zero-hop broadcast.
pub const CMD_SHARE_CONTACT: u8 = 16;
/// Export a contact as a packet blob.
pub const CMD_EXPORT_CONTACT: u8 = 17;
/// Import a contact from a packet blob.
pub const CMD_IMPORT_CONTACT: u8 = 18;
/// Reboot the device.
pub const CMD_REBOOT: u8 = 19;
/// Get battery voltage and storage info.
pub const CMD_GET_BATT_AND_STORAGE: u8 = 20;
/// Set tuning parameters (RX delay, airtime factor).
pub const CMD_SET_TUNING_PARAMS: u8 = 21;
/// Query device information.
pub const CMD_DEVICE_QUERY: u8 = 22;
/// Export the private key.
pub const CMD_EXPORT_PRIVATE_KEY: u8 = 23;
/// Import a private key.
pub const CMD_IMPORT_PRIVATE_KEY: u8 = 24;
/// Send raw data packet.
pub const CMD_SEND_RAW_DATA: u8 = 25;
/// Send login request to a server.
pub const CMD_SEND_LOGIN: u8 = 26;
/// Send status request to a server.
pub const CMD_SEND_STATUS_REQ: u8 = 27;
/// Check if there's an active connection to a contact.
pub const CMD_HAS_CONNECTION: u8 = 28;
/// Logout/disconnect from a server.
pub const CMD_LOGOUT: u8 = 29;
/// Get a contact by public key.
pub const CMD_GET_CONTACT_BY_KEY: u8 = 30;
/// Get channel information.
pub const CMD_GET_CHANNEL: u8 = 31;
/// Set channel information.
pub const CMD_SET_CHANNEL: u8 = 32;
/// Start signing operation.
pub const CMD_SIGN_START: u8 = 33;
/// Provide data for signing.
pub const CMD_SIGN_DATA: u8 = 34;
/// Finish signing and get signature.
pub const CMD_SIGN_FINISH: u8 = 35;
/// Send trace path packet.
pub const CMD_SEND_TRACE_PATH: u8 = 36;
/// Set device PIN code.
pub const CMD_SET_DEVICE_PIN: u8 = 37;
/// Set other parameters (telemetry modes, etc.).
pub const CMD_SET_OTHER_PARAMS: u8 = 38;
/// Send telemetry request.
pub const CMD_SEND_TELEMETRY_REQ: u8 = 39;
/// Get custom variables.
pub const CMD_GET_CUSTOM_VARS: u8 = 40;
/// Set a custom variable.
pub const CMD_SET_CUSTOM_VAR: u8 = 41;
/// Get advertisement path for a contact.
pub const CMD_GET_ADVERT_PATH: u8 = 42;
/// Get tuning parameters.
pub const CMD_GET_TUNING_PARAMS: u8 = 43;
// NOTE: CMD range 44..49 reserved for WiFi operations
/// Send binary request.
pub const CMD_SEND_BINARY_REQ: u8 = 50;
/// Factory reset the device.
pub const CMD_FACTORY_RESET: u8 = 51;
/// Send path discovery request.
pub const CMD_SEND_PATH_DISCOVERY_REQ: u8 = 52;
/// Set flood scope (v8+).
pub const CMD_SET_FLOOD_SCOPE: u8 = 54;
/// Send control data (v8+).
pub const CMD_SEND_CONTROL_DATA: u8 = 55;
/// Get statistics (v8+).
pub const CMD_GET_STATS: u8 = 56;

// ============================================================================
// Stats Sub-types (for CMD_GET_STATS)
// ============================================================================

/// Core statistics (battery, uptime, queue length).
pub const STATS_TYPE_CORE: u8 = 0;
/// Radio statistics (noise floor, RSSI, air time).
pub const STATS_TYPE_RADIO: u8 = 1;
/// Packet statistics (counts of sent/received).
pub const STATS_TYPE_PACKETS: u8 = 2;

// ============================================================================
// Response Codes (firmware → host)
// ============================================================================

/// Generic OK response.
pub const RESP_CODE_OK: u8 = 0;
/// Generic error response (followed by error code).
pub const RESP_CODE_ERR: u8 = 1;
/// Start of contacts list.
pub const RESP_CODE_CONTACTS_START: u8 = 2;
/// A single contact entry.
pub const RESP_CODE_CONTACT: u8 = 3;
/// End of contacts list.
pub const RESP_CODE_END_OF_CONTACTS: u8 = 4;
/// Self info response (reply to CMD_APP_START).
pub const RESP_CODE_SELF_INFO: u8 = 5;
/// Message sent response (reply to CMD_SEND_TXT_MSG).
pub const RESP_CODE_SENT: u8 = 6;
/// Contact message received (legacy, ver < 3).
pub const RESP_CODE_CONTACT_MSG_RECV: u8 = 7;
/// Channel message received (legacy, ver < 3).
pub const RESP_CODE_CHANNEL_MSG_RECV: u8 = 8;
/// Current time response.
pub const RESP_CODE_CURR_TIME: u8 = 9;
/// No more messages in queue.
pub const RESP_CODE_NO_MORE_MESSAGES: u8 = 10;
/// Exported contact data.
pub const RESP_CODE_EXPORT_CONTACT: u8 = 11;
/// Battery and storage info.
pub const RESP_CODE_BATT_AND_STORAGE: u8 = 12;
/// Device info response.
pub const RESP_CODE_DEVICE_INFO: u8 = 13;
/// Private key export response.
pub const RESP_CODE_PRIVATE_KEY: u8 = 14;
/// Feature disabled response.
pub const RESP_CODE_DISABLED: u8 = 15;
/// Contact message received (ver >= 3).
pub const RESP_CODE_CONTACT_MSG_RECV_V3: u8 = 16;
/// Channel message received (ver >= 3).
pub const RESP_CODE_CHANNEL_MSG_RECV_V3: u8 = 17;
/// Channel info response.
pub const RESP_CODE_CHANNEL_INFO: u8 = 18;
/// Signing started response.
pub const RESP_CODE_SIGN_START: u8 = 19;
/// Signature response.
pub const RESP_CODE_SIGNATURE: u8 = 20;
/// Custom variables response.
pub const RESP_CODE_CUSTOM_VARS: u8 = 21;
/// Advertisement path response.
pub const RESP_CODE_ADVERT_PATH: u8 = 22;
/// Tuning parameters response.
pub const RESP_CODE_TUNING_PARAMS: u8 = 23;
/// Statistics response (v8+).
pub const RESP_CODE_STATS: u8 = 24;

// ============================================================================
// Push Codes (unsolicited firmware → host)
// ============================================================================

/// Advertisement received.
pub const PUSH_CODE_ADVERT: u8 = 0x80;
/// Path to a contact was updated.
pub const PUSH_CODE_PATH_UPDATED: u8 = 0x81;
/// Message send confirmed (ACK received).
pub const PUSH_CODE_SEND_CONFIRMED: u8 = 0x82;
/// Message waiting in queue.
pub const PUSH_CODE_MSG_WAITING: u8 = 0x83;
/// Raw data received.
pub const PUSH_CODE_RAW_DATA: u8 = 0x84;
/// Login to server succeeded.
pub const PUSH_CODE_LOGIN_SUCCESS: u8 = 0x85;
/// Login to server failed.
pub const PUSH_CODE_LOGIN_FAIL: u8 = 0x86;
/// Status response from server.
pub const PUSH_CODE_STATUS_RESPONSE: u8 = 0x87;
/// Raw RX data log (for debugging).
pub const PUSH_CODE_LOG_RX_DATA: u8 = 0x88;
/// Trace data received.
pub const PUSH_CODE_TRACE_DATA: u8 = 0x89;
/// New advertisement (when auto-add disabled).
pub const PUSH_CODE_NEW_ADVERT: u8 = 0x8A;
/// Telemetry response received.
pub const PUSH_CODE_TELEMETRY_RESPONSE: u8 = 0x8B;
/// Binary response received.
pub const PUSH_CODE_BINARY_RESPONSE: u8 = 0x8C;
/// Path discovery response received.
pub const PUSH_CODE_PATH_DISCOVERY_RESPONSE: u8 = 0x8D;
/// Control data received (v8+).
pub const PUSH_CODE_CONTROL_DATA: u8 = 0x8E;

// ============================================================================
// Error Codes
// ============================================================================

/// Unsupported command.
pub const ERR_CODE_UNSUPPORTED_CMD: u8 = 1;
/// Contact/item not found.
pub const ERR_CODE_NOT_FOUND: u8 = 2;
/// Table (contacts, packets, etc.) is full.
pub const ERR_CODE_TABLE_FULL: u8 = 3;
/// Bad state for this operation.
pub const ERR_CODE_BAD_STATE: u8 = 4;
/// File I/O error.
pub const ERR_CODE_FILE_IO_ERROR: u8 = 5;
/// Illegal argument.
pub const ERR_CODE_ILLEGAL_ARG: u8 = 6;

// ============================================================================
// Text Types
// ============================================================================

/// Plain text message.
pub const TXT_TYPE_PLAIN: u8 = 0;
/// CLI/command data.
pub const TXT_TYPE_CLI_DATA: u8 = 1;
/// Signed plain text message.
pub const TXT_TYPE_SIGNED_PLAIN: u8 = 2;

// ============================================================================
// Advertisement Types
// ============================================================================

/// Chat node advertisement type.
pub const ADV_TYPE_CHAT: u8 = 1;
/// Repeater node advertisement type.
pub const ADV_TYPE_REPEATER: u8 = 2;
/// Room server advertisement type.
pub const ADV_TYPE_ROOM_SERVER: u8 = 3;

// ============================================================================
// Sizes
// ============================================================================

/// Size of a public key in bytes.
pub const PUB_KEY_SIZE: usize = 32;
/// Size of a signature in bytes.
pub const SIGNATURE_SIZE: usize = 64;
/// Maximum path size in bytes.
pub const MAX_PATH_SIZE: usize = 64;
/// Maximum frame size.
pub const MAX_FRAME_SIZE: usize = 256;
/// Maximum sign data length.
pub const MAX_SIGN_DATA_LEN: usize = 8 * 1024;
/// Size of public key prefix used in messages.
pub const PUB_KEY_PREFIX_SIZE: usize = 6;

// ============================================================================
// Telemetry Modes
// ============================================================================

/// Telemetry disabled.
pub const TELEM_MODE_DISABLED: u8 = 0;
/// Telemetry allowed for flagged contacts.
pub const TELEM_MODE_ALLOW_FLAGS: u8 = 1;
/// Telemetry allowed for all contacts.
pub const TELEM_MODE_ALLOW_ALL: u8 = 2;

// ============================================================================
// Telemetry Permissions
// ============================================================================

/// Base telemetry permission (battery, etc.).
pub const TELEM_PERM_BASE: u8 = 0x01;
/// Location telemetry permission.
pub const TELEM_PERM_LOCATION: u8 = 0x02;
/// Environment telemetry permission.
pub const TELEM_PERM_ENVIRONMENT: u8 = 0x04;

// ============================================================================
// Advertisement Location Policy
// ============================================================================

/// Don't include location in advertisements.
pub const ADVERT_LOC_NONE: u8 = 0;
/// Include location in advertisements.
pub const ADVERT_LOC_INCLUDE: u8 = 1;
