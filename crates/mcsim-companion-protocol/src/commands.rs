//! Commands that can be sent to the companion firmware.

use crate::constants::*;
use crate::types::*;

/// Commands that can be sent to the companion firmware.
#[derive(Debug, Clone)]
pub enum Command {
    /// Query device information. First command to send.
    DeviceQuery {
        /// Protocol version the app understands.
        app_version: u8,
    },

    /// Start the app connection and get self info.
    AppStart {
        /// Reserved bytes (7 bytes).
        reserved: [u8; 7],
        /// App name string.
        app_name: String,
    },

    /// Send a text message to a contact.
    SendTextMessage {
        /// Message type (plain, CLI data, etc.).
        text_type: TextType,
        /// Retry attempt number.
        attempt: u8,
        /// Message timestamp.
        timestamp: u32,
        /// Recipient's public key prefix.
        recipient_prefix: PublicKeyPrefix,
        /// Message text.
        text: String,
    },

    /// Send a text message to a channel.
    SendChannelTextMessage {
        /// Message type (should be Plain).
        text_type: TextType,
        /// Channel index.
        channel_idx: u8,
        /// Message timestamp.
        timestamp: u32,
        /// Message text.
        text: String,
    },

    /// Get the list of contacts.
    GetContacts {
        /// Optional 'since' filter (only return contacts modified after this time).
        since: Option<u32>,
    },

    /// Get the current device time.
    GetDeviceTime,

    /// Set the device time.
    SetDeviceTime {
        /// Unix timestamp in seconds.
        time_secs: u32,
    },

    /// Send a self-advertisement.
    SendSelfAdvert {
        /// Whether to flood (true) or zero-hop (false).
        flood: bool,
    },

    /// Set the advertisement name.
    SetAdvertName {
        /// New name.
        name: String,
    },

    /// Set advertisement latitude/longitude.
    SetAdvertLatLon {
        /// Latitude in microdegrees.
        lat: i32,
        /// Longitude in microdegrees.
        lon: i32,
        /// Altitude (optional, for future support).
        alt: Option<i32>,
    },

    /// Add or update a contact.
    AddUpdateContact {
        /// Contact information.
        contact: ContactInfo,
    },

    /// Remove a contact.
    RemoveContact {
        /// Contact's public key.
        public_key: PublicKey,
    },

    /// Reset the path to a contact.
    ResetPath {
        /// Contact's public key.
        public_key: PublicKey,
    },

    /// Get a contact by public key.
    GetContactByKey {
        /// Contact's public key.
        public_key: PublicKey,
    },

    /// Share a contact via zero-hop broadcast.
    ShareContact {
        /// Contact's public key.
        public_key: PublicKey,
    },

    /// Export a contact (or self if no key provided).
    ExportContact {
        /// Contact's public key (None = export self).
        public_key: Option<PublicKey>,
    },

    /// Import a contact from blob.
    ImportContact {
        /// Contact blob data.
        data: Vec<u8>,
    },

    /// Sync the next message from the offline queue.
    SyncNextMessage,

    /// Set radio parameters.
    SetRadioParams {
        /// Radio parameters.
        params: RadioParams,
    },

    /// Set radio TX power.
    SetRadioTxPower {
        /// TX power in dBm.
        power_dbm: u8,
    },

    /// Set tuning parameters.
    SetTuningParams {
        /// Tuning parameters.
        params: TuningParams,
    },

    /// Get tuning parameters.
    GetTuningParams,

    /// Set other parameters.
    SetOtherParams {
        /// Manual add contacts flag.
        manual_add_contacts: u8,
        /// Telemetry modes (optional).
        telemetry_modes: Option<u8>,
        /// Advertisement location policy (optional).
        advert_loc_policy: Option<u8>,
        /// Multi-ACK count (optional).
        multi_acks: Option<u8>,
    },

    /// Reboot the device.
    Reboot,

    /// Get battery and storage info.
    GetBatteryAndStorage,

    /// Export private key.
    ExportPrivateKey,

    /// Import private key.
    ImportPrivateKey {
        /// Identity data (64 bytes).
        identity: [u8; 64],
    },

    /// Send raw data.
    SendRawData {
        /// Path (empty for flood).
        path: Vec<u8>,
        /// Payload data.
        payload: Vec<u8>,
    },

    /// Send login request.
    SendLogin {
        /// Server's public key.
        public_key: PublicKey,
        /// Password.
        password: String,
    },

    /// Send status request.
    SendStatusRequest {
        /// Server's public key.
        public_key: PublicKey,
    },

    /// Check if there's an active connection.
    HasConnection {
        /// Server's public key.
        public_key: PublicKey,
    },

    /// Logout from a server.
    Logout {
        /// Server's public key.
        public_key: PublicKey,
    },

    /// Get channel information.
    GetChannel {
        /// Channel index.
        index: u8,
    },

    /// Set channel information.
    SetChannel {
        /// Channel info.
        channel: ChannelInfo,
    },

    /// Start signing operation.
    SignStart,

    /// Provide data for signing.
    SignData {
        /// Data to sign.
        data: Vec<u8>,
    },

    /// Finish signing and get signature.
    SignFinish,

    /// Send trace path.
    SendTracePath {
        /// Trace tag.
        tag: u32,
        /// Auth code.
        auth: u32,
        /// Flags.
        flags: u8,
        /// Path data.
        path: Vec<u8>,
    },

    /// Set device PIN.
    SetDevicePin {
        /// PIN code (0 = disabled, otherwise 6 digits).
        pin: u32,
    },

    /// Send telemetry request.
    SendTelemetryRequest {
        /// Target's public key (None = self).
        public_key: Option<PublicKey>,
        /// Reserved bytes.
        reserved: [u8; 3],
    },

    /// Get custom variables.
    GetCustomVars,

    /// Set a custom variable.
    SetCustomVar {
        /// Variable name.
        name: String,
        /// Variable value.
        value: String,
    },

    /// Get advertisement path.
    GetAdvertPath {
        /// Reserved byte.
        reserved: u8,
        /// Contact's public key.
        public_key: PublicKey,
    },

    /// Send binary request.
    SendBinaryRequest {
        /// Target's public key.
        public_key: PublicKey,
        /// Request data.
        data: Vec<u8>,
    },

    /// Factory reset.
    FactoryReset,

    /// Send path discovery request.
    SendPathDiscoveryRequest {
        /// Reserved byte.
        reserved: u8,
        /// Target's public key.
        public_key: PublicKey,
    },

    /// Set flood scope (v8+).
    SetFloodScope {
        /// Reserved byte.
        reserved: u8,
        /// Scope key (None = null scope).
        key: Option<[u8; 16]>,
    },

    /// Send control data (v8+).
    SendControlData {
        /// Control data (first byte must have bit 7 set).
        data: Vec<u8>,
    },

    /// Get statistics (v8+).
    GetStats {
        /// Stats type (Core, Radio, or Packets).
        stats_type: u8,
    },
}

impl Command {
    /// Get the command code for this command.
    pub fn code(&self) -> u8 {
        match self {
            Command::DeviceQuery { .. } => CMD_DEVICE_QUERY,
            Command::AppStart { .. } => CMD_APP_START,
            Command::SendTextMessage { .. } => CMD_SEND_TXT_MSG,
            Command::SendChannelTextMessage { .. } => CMD_SEND_CHANNEL_TXT_MSG,
            Command::GetContacts { .. } => CMD_GET_CONTACTS,
            Command::GetDeviceTime => CMD_GET_DEVICE_TIME,
            Command::SetDeviceTime { .. } => CMD_SET_DEVICE_TIME,
            Command::SendSelfAdvert { .. } => CMD_SEND_SELF_ADVERT,
            Command::SetAdvertName { .. } => CMD_SET_ADVERT_NAME,
            Command::SetAdvertLatLon { .. } => CMD_SET_ADVERT_LATLON,
            Command::AddUpdateContact { .. } => CMD_ADD_UPDATE_CONTACT,
            Command::RemoveContact { .. } => CMD_REMOVE_CONTACT,
            Command::ResetPath { .. } => CMD_RESET_PATH,
            Command::GetContactByKey { .. } => CMD_GET_CONTACT_BY_KEY,
            Command::ShareContact { .. } => CMD_SHARE_CONTACT,
            Command::ExportContact { .. } => CMD_EXPORT_CONTACT,
            Command::ImportContact { .. } => CMD_IMPORT_CONTACT,
            Command::SyncNextMessage => CMD_SYNC_NEXT_MESSAGE,
            Command::SetRadioParams { .. } => CMD_SET_RADIO_PARAMS,
            Command::SetRadioTxPower { .. } => CMD_SET_RADIO_TX_POWER,
            Command::SetTuningParams { .. } => CMD_SET_TUNING_PARAMS,
            Command::GetTuningParams => CMD_GET_TUNING_PARAMS,
            Command::SetOtherParams { .. } => CMD_SET_OTHER_PARAMS,
            Command::Reboot => CMD_REBOOT,
            Command::GetBatteryAndStorage => CMD_GET_BATT_AND_STORAGE,
            Command::ExportPrivateKey => CMD_EXPORT_PRIVATE_KEY,
            Command::ImportPrivateKey { .. } => CMD_IMPORT_PRIVATE_KEY,
            Command::SendRawData { .. } => CMD_SEND_RAW_DATA,
            Command::SendLogin { .. } => CMD_SEND_LOGIN,
            Command::SendStatusRequest { .. } => CMD_SEND_STATUS_REQ,
            Command::HasConnection { .. } => CMD_HAS_CONNECTION,
            Command::Logout { .. } => CMD_LOGOUT,
            Command::GetChannel { .. } => CMD_GET_CHANNEL,
            Command::SetChannel { .. } => CMD_SET_CHANNEL,
            Command::SignStart => CMD_SIGN_START,
            Command::SignData { .. } => CMD_SIGN_DATA,
            Command::SignFinish => CMD_SIGN_FINISH,
            Command::SendTracePath { .. } => CMD_SEND_TRACE_PATH,
            Command::SetDevicePin { .. } => CMD_SET_DEVICE_PIN,
            Command::SendTelemetryRequest { .. } => CMD_SEND_TELEMETRY_REQ,
            Command::GetCustomVars => CMD_GET_CUSTOM_VARS,
            Command::SetCustomVar { .. } => CMD_SET_CUSTOM_VAR,
            Command::GetAdvertPath { .. } => CMD_GET_ADVERT_PATH,
            Command::SendBinaryRequest { .. } => CMD_SEND_BINARY_REQ,
            Command::FactoryReset => CMD_FACTORY_RESET,
            Command::SendPathDiscoveryRequest { .. } => CMD_SEND_PATH_DISCOVERY_REQ,
            Command::SetFloodScope { .. } => CMD_SET_FLOOD_SCOPE,
            Command::SendControlData { .. } => CMD_SEND_CONTROL_DATA,
            Command::GetStats { .. } => CMD_GET_STATS,
        }
    }

    /// Encode the command to bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(MAX_FRAME_SIZE);

        match self {
            Command::DeviceQuery { app_version } => {
                buf.push(CMD_DEVICE_QUERY);
                buf.push(*app_version);
            }

            Command::AppStart { reserved, app_name } => {
                buf.push(CMD_APP_START);
                buf.extend_from_slice(reserved);
                buf.extend_from_slice(app_name.as_bytes());
            }

            Command::SendTextMessage {
                text_type,
                attempt,
                timestamp,
                recipient_prefix,
                text,
            } => {
                buf.push(CMD_SEND_TXT_MSG);
                buf.push((*text_type).into());
                buf.push(*attempt);
                buf.extend_from_slice(&timestamp.to_le_bytes());
                buf.extend_from_slice(recipient_prefix.as_bytes());
                buf.extend_from_slice(text.as_bytes());
            }

            Command::SendChannelTextMessage {
                text_type,
                channel_idx,
                timestamp,
                text,
            } => {
                buf.push(CMD_SEND_CHANNEL_TXT_MSG);
                buf.push((*text_type).into());
                buf.push(*channel_idx);
                buf.extend_from_slice(&timestamp.to_le_bytes());
                buf.extend_from_slice(text.as_bytes());
            }

            Command::GetContacts { since } => {
                buf.push(CMD_GET_CONTACTS);
                if let Some(since) = since {
                    buf.extend_from_slice(&since.to_le_bytes());
                }
            }

            Command::GetDeviceTime => {
                buf.push(CMD_GET_DEVICE_TIME);
            }

            Command::SetDeviceTime { time_secs } => {
                buf.push(CMD_SET_DEVICE_TIME);
                buf.extend_from_slice(&time_secs.to_le_bytes());
            }

            Command::SendSelfAdvert { flood } => {
                buf.push(CMD_SEND_SELF_ADVERT);
                buf.push(if *flood { 1 } else { 0 });
            }

            Command::SetAdvertName { name } => {
                buf.push(CMD_SET_ADVERT_NAME);
                buf.extend_from_slice(name.as_bytes());
            }

            Command::SetAdvertLatLon { lat, lon, alt } => {
                buf.push(CMD_SET_ADVERT_LATLON);
                buf.extend_from_slice(&lat.to_le_bytes());
                buf.extend_from_slice(&lon.to_le_bytes());
                if let Some(alt) = alt {
                    buf.extend_from_slice(&alt.to_le_bytes());
                }
            }

            Command::AddUpdateContact { contact } => {
                buf.push(CMD_ADD_UPDATE_CONTACT);
                buf.extend_from_slice(contact.public_key.as_bytes());
                buf.push(contact.contact_type);
                buf.push(contact.flags);
                buf.push(contact.out_path_len as u8);
                buf.extend_from_slice(&contact.out_path);
                // Name is 32 bytes, null-padded
                let mut name_buf = [0u8; 32];
                let name_bytes = contact.name.as_bytes();
                let len = name_bytes.len().min(31);
                name_buf[..len].copy_from_slice(&name_bytes[..len]);
                buf.extend_from_slice(&name_buf);
                buf.extend_from_slice(&contact.last_advert_timestamp.to_le_bytes());
                buf.extend_from_slice(&contact.gps_lat.to_le_bytes());
                buf.extend_from_slice(&contact.gps_lon.to_le_bytes());
                // Note: We intentionally omit lastmod to let firmware use RTC time.
                // If we send lastmod=0, the contact gets filtered out by GetContacts.
            }

            Command::RemoveContact { public_key } => {
                buf.push(CMD_REMOVE_CONTACT);
                buf.extend_from_slice(public_key.as_bytes());
            }

            Command::ResetPath { public_key } => {
                buf.push(CMD_RESET_PATH);
                buf.extend_from_slice(public_key.as_bytes());
            }

            Command::GetContactByKey { public_key } => {
                buf.push(CMD_GET_CONTACT_BY_KEY);
                buf.extend_from_slice(public_key.as_bytes());
            }

            Command::ShareContact { public_key } => {
                buf.push(CMD_SHARE_CONTACT);
                buf.extend_from_slice(public_key.as_bytes());
            }

            Command::ExportContact { public_key } => {
                buf.push(CMD_EXPORT_CONTACT);
                if let Some(pk) = public_key {
                    buf.extend_from_slice(pk.as_bytes());
                }
            }

            Command::ImportContact { data } => {
                buf.push(CMD_IMPORT_CONTACT);
                buf.extend_from_slice(data);
            }

            Command::SyncNextMessage => {
                buf.push(CMD_SYNC_NEXT_MESSAGE);
            }

            Command::SetRadioParams { params } => {
                buf.push(CMD_SET_RADIO_PARAMS);
                buf.extend_from_slice(&params.freq_khz.to_le_bytes());
                buf.extend_from_slice(&params.bandwidth_hz.to_le_bytes());
                buf.push(params.spreading_factor);
                buf.push(params.coding_rate);
            }

            Command::SetRadioTxPower { power_dbm } => {
                buf.push(CMD_SET_RADIO_TX_POWER);
                buf.push(*power_dbm);
            }

            Command::SetTuningParams { params } => {
                buf.push(CMD_SET_TUNING_PARAMS);
                buf.extend_from_slice(&params.rx_delay_base.to_le_bytes());
                buf.extend_from_slice(&params.airtime_factor.to_le_bytes());
            }

            Command::GetTuningParams => {
                buf.push(CMD_GET_TUNING_PARAMS);
            }

            Command::SetOtherParams {
                manual_add_contacts,
                telemetry_modes,
                advert_loc_policy,
                multi_acks,
            } => {
                buf.push(CMD_SET_OTHER_PARAMS);
                buf.push(*manual_add_contacts);
                if let Some(tm) = telemetry_modes {
                    buf.push(*tm);
                    if let Some(alp) = advert_loc_policy {
                        buf.push(*alp);
                        if let Some(ma) = multi_acks {
                            buf.push(*ma);
                        }
                    }
                }
            }

            Command::Reboot => {
                buf.push(CMD_REBOOT);
                buf.extend_from_slice(b"reboot");
            }

            Command::GetBatteryAndStorage => {
                buf.push(CMD_GET_BATT_AND_STORAGE);
            }

            Command::ExportPrivateKey => {
                buf.push(CMD_EXPORT_PRIVATE_KEY);
            }

            Command::ImportPrivateKey { identity } => {
                buf.push(CMD_IMPORT_PRIVATE_KEY);
                buf.extend_from_slice(identity);
            }

            Command::SendRawData { path, payload } => {
                buf.push(CMD_SEND_RAW_DATA);
                buf.push(path.len() as u8);
                buf.extend_from_slice(path);
                buf.extend_from_slice(payload);
            }

            Command::SendLogin { public_key, password } => {
                buf.push(CMD_SEND_LOGIN);
                buf.extend_from_slice(public_key.as_bytes());
                buf.extend_from_slice(password.as_bytes());
            }

            Command::SendStatusRequest { public_key } => {
                buf.push(CMD_SEND_STATUS_REQ);
                buf.extend_from_slice(public_key.as_bytes());
            }

            Command::HasConnection { public_key } => {
                buf.push(CMD_HAS_CONNECTION);
                buf.extend_from_slice(public_key.as_bytes());
            }

            Command::Logout { public_key } => {
                buf.push(CMD_LOGOUT);
                buf.extend_from_slice(public_key.as_bytes());
            }

            Command::GetChannel { index } => {
                buf.push(CMD_GET_CHANNEL);
                buf.push(*index);
            }

            Command::SetChannel { channel } => {
                buf.push(CMD_SET_CHANNEL);
                buf.push(channel.index);
                // Name is 32 bytes, null-padded
                let mut name_buf = [0u8; 32];
                let name_bytes = channel.name.as_bytes();
                let len = name_bytes.len().min(31);
                name_buf[..len].copy_from_slice(&name_bytes[..len]);
                buf.extend_from_slice(&name_buf);
                buf.extend_from_slice(&channel.secret);
            }

            Command::SignStart => {
                buf.push(CMD_SIGN_START);
            }

            Command::SignData { data } => {
                buf.push(CMD_SIGN_DATA);
                buf.extend_from_slice(data);
            }

            Command::SignFinish => {
                buf.push(CMD_SIGN_FINISH);
            }

            Command::SendTracePath { tag, auth, flags, path } => {
                buf.push(CMD_SEND_TRACE_PATH);
                buf.extend_from_slice(&tag.to_le_bytes());
                buf.extend_from_slice(&auth.to_le_bytes());
                buf.push(*flags);
                buf.extend_from_slice(path);
            }

            Command::SetDevicePin { pin } => {
                buf.push(CMD_SET_DEVICE_PIN);
                buf.extend_from_slice(&pin.to_le_bytes());
            }

            Command::SendTelemetryRequest { public_key, reserved } => {
                buf.push(CMD_SEND_TELEMETRY_REQ);
                buf.extend_from_slice(reserved);
                if let Some(pk) = public_key {
                    buf.push(0); // padding to get to offset 4
                    buf.extend_from_slice(pk.as_bytes());
                }
            }

            Command::GetCustomVars => {
                buf.push(CMD_GET_CUSTOM_VARS);
            }

            Command::SetCustomVar { name, value } => {
                buf.push(CMD_SET_CUSTOM_VAR);
                buf.extend_from_slice(name.as_bytes());
                buf.push(b':');
                buf.extend_from_slice(value.as_bytes());
            }

            Command::GetAdvertPath { reserved, public_key } => {
                buf.push(CMD_GET_ADVERT_PATH);
                buf.push(*reserved);
                buf.extend_from_slice(public_key.as_bytes());
            }

            Command::SendBinaryRequest { public_key, data } => {
                buf.push(CMD_SEND_BINARY_REQ);
                buf.extend_from_slice(public_key.as_bytes());
                buf.extend_from_slice(data);
            }

            Command::FactoryReset => {
                buf.push(CMD_FACTORY_RESET);
                buf.extend_from_slice(b"reset");
            }

            Command::SendPathDiscoveryRequest { reserved, public_key } => {
                buf.push(CMD_SEND_PATH_DISCOVERY_REQ);
                buf.push(*reserved);
                buf.extend_from_slice(public_key.as_bytes());
            }

            Command::SetFloodScope { reserved, key } => {
                buf.push(CMD_SET_FLOOD_SCOPE);
                buf.push(*reserved);
                if let Some(k) = key {
                    buf.extend_from_slice(k);
                }
            }

            Command::SendControlData { data } => {
                buf.push(CMD_SEND_CONTROL_DATA);
                buf.extend_from_slice(data);
            }

            Command::GetStats { stats_type } => {
                buf.push(CMD_GET_STATS);
                buf.push(*stats_type);
            }
        }

        buf
    }
}
