//! Responses from the companion firmware.

use crate::constants::*;
use crate::error::*;
use crate::types::*;

/// Responses received from the companion firmware.
#[derive(Debug, Clone)]
pub enum Response {
    /// Generic OK response.
    Ok,

    /// Error response from firmware.
    Error(FirmwareErrorCode),

    /// Feature disabled.
    Disabled,

    /// Start of contacts list.
    ContactsStart {
        /// Total number of contacts.
        total_count: u32,
    },

    /// A single contact.
    Contact(ContactInfo),

    /// End of contacts list.
    EndOfContacts {
        /// Most recent lastmod timestamp.
        most_recent_lastmod: u32,
    },

    /// Self info (response to AppStart).
    SelfInfo(SelfInfo),

    /// Message sent response.
    Sent {
        /// Whether message was sent as flood.
        is_flood: bool,
        /// Expected ACK hash (or tag).
        expected_ack: u32,
        /// Estimated timeout in milliseconds.
        est_timeout_ms: u32,
    },

    /// Current time.
    CurrentTime {
        /// Unix timestamp in seconds.
        time_secs: u32,
    },

    /// No more messages in queue.
    NoMoreMessages,

    /// Exported contact data.
    ExportedContact {
        /// Raw packet data.
        data: Vec<u8>,
    },

    /// Battery and storage info.
    BatteryAndStorage(BatteryAndStorage),

    /// Device info.
    DeviceInfo(DeviceInfo),

    /// Private key export.
    PrivateKey {
        /// Identity data (64 bytes).
        identity: [u8; 64],
    },

    /// Contact message received (legacy v2).
    ContactMessageV2(ReceivedContactMessage),

    /// Contact message received (v3+).
    ContactMessageV3(ReceivedContactMessage),

    /// Channel message received (legacy v2).
    ChannelMessageV2(ReceivedChannelMessage),

    /// Channel message received (v3+).
    ChannelMessageV3(ReceivedChannelMessage),

    /// Channel info.
    ChannelInfo(ChannelInfo),

    /// Signing started.
    SignStart {
        /// Maximum data length.
        max_len: u32,
    },

    /// Signature result.
    Signature {
        /// The signature (64 bytes).
        signature: [u8; SIGNATURE_SIZE],
    },

    /// Custom variables.
    CustomVars {
        /// Variables as "name:value,name:value,..." string.
        vars: String,
    },

    /// Advertisement path.
    AdvertPath {
        /// Receive timestamp.
        recv_timestamp: u32,
        /// Path length.
        path_len: u8,
        /// Path data.
        path: Vec<u8>,
    },

    /// Tuning parameters.
    TuningParams(TuningParams),

    /// Core statistics.
    StatsCore(CoreStats),

    /// Radio statistics.
    StatsRadio(RadioStats),

    /// Packet statistics.
    StatsPackets(PacketStats),
}

/// Push notifications from the firmware (unsolicited).
#[derive(Debug, Clone)]
pub enum PushNotification {
    /// Advertisement received.
    Advert {
        /// Advertiser's public key.
        public_key: PublicKey,
    },

    /// New advertisement (when auto-add disabled).
    NewAdvert(ContactInfo),

    /// Path to a contact was updated.
    PathUpdated {
        /// Contact's public key.
        public_key: PublicKey,
    },

    /// Message send confirmed (ACK received).
    SendConfirmed {
        /// ACK hash that was confirmed.
        ack_hash: u32,
        /// Round-trip time in milliseconds.
        trip_time_ms: u32,
    },

    /// Message waiting in queue.
    MessageWaiting,

    /// Raw data received.
    RawData {
        /// SNR (scaled by 4).
        snr_x4: i8,
        /// RSSI.
        rssi: i8,
        /// Payload data.
        payload: Vec<u8>,
    },

    /// Login succeeded.
    LoginSuccess {
        /// Is admin.
        is_admin: bool,
        /// Server's public key prefix.
        server_prefix: PublicKeyPrefix,
        /// Server timestamp (v7+).
        server_timestamp: Option<u32>,
        /// ACL permissions (v7+).
        acl_permissions: Option<u8>,
        /// Firmware version level (v7+).
        firmware_ver_level: Option<u8>,
    },

    /// Login failed.
    LoginFail {
        /// Server's public key prefix.
        server_prefix: PublicKeyPrefix,
    },

    /// Status response from server.
    StatusResponse {
        /// Server's public key prefix.
        server_prefix: PublicKeyPrefix,
        /// Status data.
        data: Vec<u8>,
    },

    /// Raw RX data log.
    LogRxData {
        /// SNR (scaled by 4).
        snr_x4: i8,
        /// RSSI.
        rssi: i8,
        /// Raw packet data.
        raw: Vec<u8>,
    },

    /// Trace data received.
    TraceData {
        /// Path length.
        path_len: u8,
        /// Flags.
        flags: u8,
        /// Tag.
        tag: u32,
        /// Auth code.
        auth_code: u32,
        /// Path hashes.
        path_hashes: Vec<u8>,
        /// Path SNRs.
        path_snrs: Vec<u8>,
        /// Final SNR (to this node, scaled by 4).
        final_snr_x4: i8,
    },

    /// Telemetry response.
    TelemetryResponse {
        /// Responder's public key prefix.
        responder_prefix: PublicKeyPrefix,
        /// Telemetry data.
        data: Vec<u8>,
    },

    /// Binary response.
    BinaryResponse {
        /// Tag to match to request.
        tag: u32,
        /// Response data.
        data: Vec<u8>,
    },

    /// Path discovery response.
    PathDiscoveryResponse {
        /// Target's public key prefix.
        target_prefix: PublicKeyPrefix,
        /// Outbound path length.
        out_path_len: u8,
        /// Outbound path.
        out_path: Vec<u8>,
        /// Inbound path length.
        in_path_len: u8,
        /// Inbound path.
        in_path: Vec<u8>,
    },

    /// Control data received (v8+).
    ControlData {
        /// SNR (scaled by 4).
        snr_x4: i8,
        /// RSSI.
        rssi: i8,
        /// Path length.
        path_len: u8,
        /// Payload data.
        payload: Vec<u8>,
    },
}

/// Either a response or a push notification.
#[derive(Debug, Clone)]
pub enum Message {
    /// A response to a command.
    Response(Response),
    /// An unsolicited push notification.
    Push(PushNotification),
}

impl Message {
    /// Decode a message from a frame.
    pub fn decode(frame: &[u8]) -> Result<Self, ProtocolError> {
        if frame.is_empty() {
            return Err(ProtocolError::FrameTooShort {
                expected: 1,
                actual: 0,
            });
        }

        let code = frame[0];

        // Push notifications have high bit set
        if code & 0x80 != 0 {
            Ok(Message::Push(PushNotification::decode(frame)?))
        } else {
            Ok(Message::Response(Response::decode(frame)?))
        }
    }
}

impl Response {
    /// Decode a response from a frame.
    pub fn decode(frame: &[u8]) -> Result<Self, ProtocolError> {
        if frame.is_empty() {
            return Err(ProtocolError::FrameTooShort {
                expected: 1,
                actual: 0,
            });
        }

        let code = frame[0];

        match code {
            RESP_CODE_OK => Ok(Response::Ok),

            RESP_CODE_ERR => {
                if frame.len() < 2 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 2,
                        actual: frame.len(),
                    });
                }
                Ok(Response::Error(FirmwareErrorCode::from(frame[1])))
            }

            RESP_CODE_DISABLED => Ok(Response::Disabled),

            RESP_CODE_CONTACTS_START => {
                if frame.len() < 5 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 5,
                        actual: frame.len(),
                    });
                }
                let total_count = u32::from_le_bytes([frame[1], frame[2], frame[3], frame[4]]);
                Ok(Response::ContactsStart { total_count })
            }

            RESP_CODE_CONTACT => {
                let contact = decode_contact(&frame[1..])?;
                Ok(Response::Contact(contact))
            }

            RESP_CODE_END_OF_CONTACTS => {
                if frame.len() < 5 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 5,
                        actual: frame.len(),
                    });
                }
                let most_recent_lastmod =
                    u32::from_le_bytes([frame[1], frame[2], frame[3], frame[4]]);
                Ok(Response::EndOfContacts {
                    most_recent_lastmod,
                })
            }

            RESP_CODE_SELF_INFO => {
                let info = decode_self_info(&frame[1..])?;
                Ok(Response::SelfInfo(info))
            }

            RESP_CODE_SENT => {
                if frame.len() < 10 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 10,
                        actual: frame.len(),
                    });
                }
                let is_flood = frame[1] != 0;
                let expected_ack = u32::from_le_bytes([frame[2], frame[3], frame[4], frame[5]]);
                let est_timeout_ms = u32::from_le_bytes([frame[6], frame[7], frame[8], frame[9]]);
                Ok(Response::Sent {
                    is_flood,
                    expected_ack,
                    est_timeout_ms,
                })
            }

            RESP_CODE_CURR_TIME => {
                if frame.len() < 5 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 5,
                        actual: frame.len(),
                    });
                }
                let time_secs = u32::from_le_bytes([frame[1], frame[2], frame[3], frame[4]]);
                Ok(Response::CurrentTime { time_secs })
            }

            RESP_CODE_NO_MORE_MESSAGES => Ok(Response::NoMoreMessages),

            RESP_CODE_EXPORT_CONTACT => {
                Ok(Response::ExportedContact {
                    data: frame[1..].to_vec(),
                })
            }

            RESP_CODE_BATT_AND_STORAGE => {
                if frame.len() < 11 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 11,
                        actual: frame.len(),
                    });
                }
                let battery_millivolts = u16::from_le_bytes([frame[1], frame[2]]);
                let storage_used_kb =
                    u32::from_le_bytes([frame[3], frame[4], frame[5], frame[6]]);
                let storage_total_kb =
                    u32::from_le_bytes([frame[7], frame[8], frame[9], frame[10]]);
                Ok(Response::BatteryAndStorage(BatteryAndStorage {
                    battery_millivolts,
                    storage_used_kb,
                    storage_total_kb,
                }))
            }

            RESP_CODE_DEVICE_INFO => {
                let info = decode_device_info(&frame[1..])?;
                Ok(Response::DeviceInfo(info))
            }

            RESP_CODE_PRIVATE_KEY => {
                if frame.len() < 65 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 65,
                        actual: frame.len(),
                    });
                }
                let mut identity = [0u8; 64];
                identity.copy_from_slice(&frame[1..65]);
                Ok(Response::PrivateKey { identity })
            }

            RESP_CODE_CONTACT_MSG_RECV => {
                let msg = decode_contact_message_v2(&frame[1..])?;
                Ok(Response::ContactMessageV2(msg))
            }

            RESP_CODE_CONTACT_MSG_RECV_V3 => {
                let msg = decode_contact_message_v3(&frame[1..])?;
                Ok(Response::ContactMessageV3(msg))
            }

            RESP_CODE_CHANNEL_MSG_RECV => {
                let msg = decode_channel_message_v2(&frame[1..])?;
                Ok(Response::ChannelMessageV2(msg))
            }

            RESP_CODE_CHANNEL_MSG_RECV_V3 => {
                let msg = decode_channel_message_v3(&frame[1..])?;
                Ok(Response::ChannelMessageV3(msg))
            }

            RESP_CODE_CHANNEL_INFO => {
                let info = decode_channel_info(&frame[1..])?;
                Ok(Response::ChannelInfo(info))
            }

            RESP_CODE_SIGN_START => {
                if frame.len() < 6 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 6,
                        actual: frame.len(),
                    });
                }
                let max_len = u32::from_le_bytes([frame[2], frame[3], frame[4], frame[5]]);
                Ok(Response::SignStart { max_len })
            }

            RESP_CODE_SIGNATURE => {
                if frame.len() < 1 + SIGNATURE_SIZE {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 1 + SIGNATURE_SIZE,
                        actual: frame.len(),
                    });
                }
                let mut signature = [0u8; SIGNATURE_SIZE];
                signature.copy_from_slice(&frame[1..1 + SIGNATURE_SIZE]);
                Ok(Response::Signature { signature })
            }

            RESP_CODE_CUSTOM_VARS => {
                let vars = String::from_utf8_lossy(&frame[1..]).to_string();
                Ok(Response::CustomVars { vars })
            }

            RESP_CODE_ADVERT_PATH => {
                if frame.len() < 6 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 6,
                        actual: frame.len(),
                    });
                }
                let recv_timestamp = u32::from_le_bytes([frame[1], frame[2], frame[3], frame[4]]);
                let path_len = frame[5];
                let path = frame[6..].to_vec();
                Ok(Response::AdvertPath {
                    recv_timestamp,
                    path_len,
                    path,
                })
            }

            RESP_CODE_TUNING_PARAMS => {
                if frame.len() < 9 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 9,
                        actual: frame.len(),
                    });
                }
                let rx_delay_base = u32::from_le_bytes([frame[1], frame[2], frame[3], frame[4]]);
                let airtime_factor = u32::from_le_bytes([frame[5], frame[6], frame[7], frame[8]]);
                Ok(Response::TuningParams(TuningParams {
                    rx_delay_base,
                    airtime_factor,
                }))
            }

            RESP_CODE_STATS => {
                if frame.len() < 2 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 2,
                        actual: frame.len(),
                    });
                }
                let stats_type = frame[1];
                match stats_type {
                    STATS_TYPE_CORE => {
                        if frame.len() < 11 {
                            return Err(ProtocolError::FrameTooShort {
                                expected: 11,
                                actual: frame.len(),
                            });
                        }
                        let battery_mv = u16::from_le_bytes([frame[2], frame[3]]);
                        let uptime_secs =
                            u32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]);
                        let error_flags = u16::from_le_bytes([frame[8], frame[9]]);
                        let queue_len = frame[10];
                        Ok(Response::StatsCore(CoreStats {
                            battery_mv,
                            uptime_secs,
                            error_flags,
                            queue_len,
                        }))
                    }
                    STATS_TYPE_RADIO => {
                        if frame.len() < 14 {
                            return Err(ProtocolError::FrameTooShort {
                                expected: 14,
                                actual: frame.len(),
                            });
                        }
                        let noise_floor = i16::from_le_bytes([frame[2], frame[3]]);
                        let last_rssi = frame[4] as i8;
                        let last_snr_x4 = frame[5] as i8;
                        let tx_air_secs =
                            u32::from_le_bytes([frame[6], frame[7], frame[8], frame[9]]);
                        let rx_air_secs =
                            u32::from_le_bytes([frame[10], frame[11], frame[12], frame[13]]);
                        Ok(Response::StatsRadio(RadioStats {
                            noise_floor,
                            last_rssi,
                            last_snr_x4,
                            tx_air_secs,
                            rx_air_secs,
                        }))
                    }
                    STATS_TYPE_PACKETS => {
                        if frame.len() < 26 {
                            return Err(ProtocolError::FrameTooShort {
                                expected: 26,
                                actual: frame.len(),
                            });
                        }
                        let recv = u32::from_le_bytes([frame[2], frame[3], frame[4], frame[5]]);
                        let sent = u32::from_le_bytes([frame[6], frame[7], frame[8], frame[9]]);
                        let sent_flood =
                            u32::from_le_bytes([frame[10], frame[11], frame[12], frame[13]]);
                        let sent_direct =
                            u32::from_le_bytes([frame[14], frame[15], frame[16], frame[17]]);
                        let recv_flood =
                            u32::from_le_bytes([frame[18], frame[19], frame[20], frame[21]]);
                        let recv_direct =
                            u32::from_le_bytes([frame[22], frame[23], frame[24], frame[25]]);
                        Ok(Response::StatsPackets(PacketStats {
                            recv,
                            sent,
                            sent_flood,
                            sent_direct,
                            recv_flood,
                            recv_direct,
                        }))
                    }
                    _ => Err(ProtocolError::InvalidData(format!(
                        "unknown stats type: {}",
                        stats_type
                    ))),
                }
            }

            _ => Err(ProtocolError::UnknownResponse(code)),
        }
    }
}

impl PushNotification {
    /// Decode a push notification from a frame.
    pub fn decode(frame: &[u8]) -> Result<Self, ProtocolError> {
        if frame.is_empty() {
            return Err(ProtocolError::FrameTooShort {
                expected: 1,
                actual: 0,
            });
        }

        let code = frame[0];

        match code {
            PUSH_CODE_ADVERT => {
                if frame.len() < 1 + PUB_KEY_SIZE {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 1 + PUB_KEY_SIZE,
                        actual: frame.len(),
                    });
                }
                let public_key = PublicKey::from_slice(&frame[1..1 + PUB_KEY_SIZE]).unwrap();
                Ok(PushNotification::Advert { public_key })
            }

            PUSH_CODE_NEW_ADVERT => {
                let contact = decode_contact(&frame[1..])?;
                Ok(PushNotification::NewAdvert(contact))
            }

            PUSH_CODE_PATH_UPDATED => {
                if frame.len() < 1 + PUB_KEY_SIZE {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 1 + PUB_KEY_SIZE,
                        actual: frame.len(),
                    });
                }
                let public_key = PublicKey::from_slice(&frame[1..1 + PUB_KEY_SIZE]).unwrap();
                Ok(PushNotification::PathUpdated { public_key })
            }

            PUSH_CODE_SEND_CONFIRMED => {
                if frame.len() < 9 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 9,
                        actual: frame.len(),
                    });
                }
                let ack_hash = u32::from_le_bytes([frame[1], frame[2], frame[3], frame[4]]);
                let trip_time_ms = u32::from_le_bytes([frame[5], frame[6], frame[7], frame[8]]);
                Ok(PushNotification::SendConfirmed {
                    ack_hash,
                    trip_time_ms,
                })
            }

            PUSH_CODE_MSG_WAITING => Ok(PushNotification::MessageWaiting),

            PUSH_CODE_RAW_DATA => {
                if frame.len() < 4 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 4,
                        actual: frame.len(),
                    });
                }
                let snr_x4 = frame[1] as i8;
                let rssi = frame[2] as i8;
                // frame[3] is reserved
                let payload = frame[4..].to_vec();
                Ok(PushNotification::RawData {
                    snr_x4,
                    rssi,
                    payload,
                })
            }

            PUSH_CODE_LOGIN_SUCCESS => {
                if frame.len() < 8 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 8,
                        actual: frame.len(),
                    });
                }
                let is_admin = frame[1] != 0;
                let server_prefix =
                    PublicKeyPrefix::from_slice(&frame[2..2 + PUB_KEY_PREFIX_SIZE]).unwrap();

                // Optional v7+ fields
                let (server_timestamp, acl_permissions, firmware_ver_level) = if frame.len() >= 14 {
                    let ts = u32::from_le_bytes([frame[8], frame[9], frame[10], frame[11]]);
                    let acl = if frame.len() >= 13 {
                        Some(frame[12])
                    } else {
                        None
                    };
                    let fw = if frame.len() >= 14 {
                        Some(frame[13])
                    } else {
                        None
                    };
                    (Some(ts), acl, fw)
                } else {
                    (None, None, None)
                };

                Ok(PushNotification::LoginSuccess {
                    is_admin,
                    server_prefix,
                    server_timestamp,
                    acl_permissions,
                    firmware_ver_level,
                })
            }

            PUSH_CODE_LOGIN_FAIL => {
                if frame.len() < 8 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 8,
                        actual: frame.len(),
                    });
                }
                let server_prefix =
                    PublicKeyPrefix::from_slice(&frame[2..2 + PUB_KEY_PREFIX_SIZE]).unwrap();
                Ok(PushNotification::LoginFail { server_prefix })
            }

            PUSH_CODE_STATUS_RESPONSE => {
                if frame.len() < 8 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 8,
                        actual: frame.len(),
                    });
                }
                let server_prefix =
                    PublicKeyPrefix::from_slice(&frame[2..2 + PUB_KEY_PREFIX_SIZE]).unwrap();
                let data = frame[8..].to_vec();
                Ok(PushNotification::StatusResponse {
                    server_prefix,
                    data,
                })
            }

            PUSH_CODE_LOG_RX_DATA => {
                if frame.len() < 3 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 3,
                        actual: frame.len(),
                    });
                }
                let snr_x4 = frame[1] as i8;
                let rssi = frame[2] as i8;
                let raw = frame[3..].to_vec();
                Ok(PushNotification::LogRxData { snr_x4, rssi, raw })
            }

            PUSH_CODE_TRACE_DATA => {
                if frame.len() < 12 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 12,
                        actual: frame.len(),
                    });
                }
                // frame[1] = reserved
                let path_len = frame[2];
                let flags = frame[3];
                let tag = u32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]);
                let auth_code = u32::from_le_bytes([frame[8], frame[9], frame[10], frame[11]]);

                let path_sz = flags & 0x03;
                let path_hashes_end = 12 + path_len as usize;
                if frame.len() < path_hashes_end {
                    return Err(ProtocolError::FrameTooShort {
                        expected: path_hashes_end,
                        actual: frame.len(),
                    });
                }
                let path_hashes = frame[12..path_hashes_end].to_vec();

                let snr_count = (path_len as usize) >> path_sz;
                let path_snrs_end = path_hashes_end + snr_count;
                if frame.len() < path_snrs_end + 1 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: path_snrs_end + 1,
                        actual: frame.len(),
                    });
                }
                let path_snrs = frame[path_hashes_end..path_snrs_end].to_vec();
                let final_snr_x4 = frame[path_snrs_end] as i8;

                Ok(PushNotification::TraceData {
                    path_len,
                    flags,
                    tag,
                    auth_code,
                    path_hashes,
                    path_snrs,
                    final_snr_x4,
                })
            }

            PUSH_CODE_TELEMETRY_RESPONSE => {
                if frame.len() < 8 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 8,
                        actual: frame.len(),
                    });
                }
                // frame[1] = reserved
                let responder_prefix =
                    PublicKeyPrefix::from_slice(&frame[2..2 + PUB_KEY_PREFIX_SIZE]).unwrap();
                let data = frame[8..].to_vec();
                Ok(PushNotification::TelemetryResponse {
                    responder_prefix,
                    data,
                })
            }

            PUSH_CODE_BINARY_RESPONSE => {
                if frame.len() < 6 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 6,
                        actual: frame.len(),
                    });
                }
                // frame[1] = reserved
                let tag = u32::from_le_bytes([frame[2], frame[3], frame[4], frame[5]]);
                let data = frame[6..].to_vec();
                Ok(PushNotification::BinaryResponse { tag, data })
            }

            PUSH_CODE_PATH_DISCOVERY_RESPONSE => {
                if frame.len() < 9 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 9,
                        actual: frame.len(),
                    });
                }
                // frame[1] = reserved
                let target_prefix =
                    PublicKeyPrefix::from_slice(&frame[2..2 + PUB_KEY_PREFIX_SIZE]).unwrap();
                let out_path_len = frame[8];
                let out_path_end = 9 + out_path_len as usize;
                if frame.len() < out_path_end + 1 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: out_path_end + 1,
                        actual: frame.len(),
                    });
                }
                let out_path = frame[9..out_path_end].to_vec();
                let in_path_len = frame[out_path_end];
                let in_path = frame[out_path_end + 1..].to_vec();

                Ok(PushNotification::PathDiscoveryResponse {
                    target_prefix,
                    out_path_len,
                    out_path,
                    in_path_len,
                    in_path,
                })
            }

            PUSH_CODE_CONTROL_DATA => {
                if frame.len() < 4 {
                    return Err(ProtocolError::FrameTooShort {
                        expected: 4,
                        actual: frame.len(),
                    });
                }
                let snr_x4 = frame[1] as i8;
                let rssi = frame[2] as i8;
                let path_len = frame[3];
                let payload = frame[4..].to_vec();
                Ok(PushNotification::ControlData {
                    snr_x4,
                    rssi,
                    path_len,
                    payload,
                })
            }

            _ => Err(ProtocolError::UnknownResponse(code)),
        }
    }
}

// ============================================================================
// Helper decode functions
// ============================================================================

fn decode_contact(data: &[u8]) -> Result<ContactInfo, ProtocolError> {
    // Minimum size: 32 (pubkey) + 1 (type) + 1 (flags) + 1 (path_len) + 16 (path) + 32 (name) + 4 (timestamp) = 87
    if data.len() < 87 {
        return Err(ProtocolError::FrameTooShort {
            expected: 87,
            actual: data.len(),
        });
    }

    let mut contact = ContactInfo::default();

    let mut i = 0;

    // Public key
    contact.public_key = PublicKey::from_slice(&data[i..i + PUB_KEY_SIZE]).unwrap();
    i += PUB_KEY_SIZE;

    // Type, flags, path_len
    contact.contact_type = data[i];
    i += 1;
    contact.flags = data[i];
    i += 1;
    contact.out_path_len = data[i] as i8;
    i += 1;

    // Path
    contact.out_path.copy_from_slice(&data[i..i + MAX_PATH_SIZE]);
    i += MAX_PATH_SIZE;

    // Name (32 bytes, null-terminated)
    let name_bytes = &data[i..i + 32];
    let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(32);
    contact.name = String::from_utf8_lossy(&name_bytes[..name_end]).to_string();
    i += 32;

    // Last advert timestamp
    contact.last_advert_timestamp = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
    i += 4;

    // Optional fields
    if data.len() >= i + 8 {
        contact.gps_lat = i32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        i += 4;
        contact.gps_lon = i32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        i += 4;

        if data.len() >= i + 4 {
            contact.lastmod = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        }
    }

    Ok(contact)
}

fn decode_self_info(data: &[u8]) -> Result<SelfInfo, ProtocolError> {
    // Minimum: 1 + 1 + 1 + 32 + 4 + 4 + 1 + 1 + 1 + 1 + 4 + 4 + 1 + 1 = 57
    if data.len() < 57 {
        return Err(ProtocolError::FrameTooShort {
            expected: 57,
            actual: data.len(),
        });
    }

    let mut info = SelfInfo::default();
    let mut i = 0;

    info.advert_type = data[i];
    i += 1;
    info.tx_power_dbm = data[i];
    i += 1;
    info.max_tx_power_dbm = data[i];
    i += 1;
    info.public_key = PublicKey::from_slice(&data[i..i + PUB_KEY_SIZE]).unwrap();
    i += PUB_KEY_SIZE;
    info.gps_lat = i32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
    i += 4;
    info.gps_lon = i32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
    i += 4;
    info.multi_acks = data[i];
    i += 1;
    info.advert_loc_policy = data[i];
    i += 1;
    info.telemetry_modes = data[i];
    i += 1;
    info.manual_add_contacts = data[i];
    i += 1;
    info.freq_khz = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
    i += 4;
    info.bandwidth_hz = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
    i += 4;
    info.spreading_factor = data[i];
    i += 1;
    info.coding_rate = data[i];
    i += 1;

    // Node name is the rest
    let name_bytes = &data[i..];
    let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(name_bytes.len());
    info.node_name = String::from_utf8_lossy(&name_bytes[..name_end]).to_string();

    Ok(info)
}

fn decode_device_info(data: &[u8]) -> Result<DeviceInfo, ProtocolError> {
    // Minimum: 1 + 1 + 1 + 4 + 12 + 40 + 20 = 79
    if data.len() < 79 {
        return Err(ProtocolError::FrameTooShort {
            expected: 79,
            actual: data.len(),
        });
    }

    let mut info = DeviceInfo::default();
    let mut i = 0;

    info.firmware_version_code = data[i];
    i += 1;
    info.max_contacts_half = data[i];
    i += 1;
    info.max_group_channels = data[i];
    i += 1;
    info.ble_pin = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
    i += 4;

    // Build date (12 bytes)
    let build_date = &data[i..i + 12];
    let end = build_date.iter().position(|&b| b == 0).unwrap_or(12);
    info.build_date = String::from_utf8_lossy(&build_date[..end]).to_string();
    i += 12;

    // Manufacturer (40 bytes)
    let manufacturer = &data[i..i + 40];
    let end = manufacturer.iter().position(|&b| b == 0).unwrap_or(40);
    info.manufacturer = String::from_utf8_lossy(&manufacturer[..end]).to_string();
    i += 40;

    // Firmware version (20 bytes)
    let firmware_version = &data[i..i + 20];
    let end = firmware_version.iter().position(|&b| b == 0).unwrap_or(20);
    info.firmware_version = String::from_utf8_lossy(&firmware_version[..end]).to_string();

    Ok(info)
}

fn decode_channel_info(data: &[u8]) -> Result<ChannelInfo, ProtocolError> {
    // 1 (index) + 32 (name) + 16 (secret) = 49
    if data.len() < 49 {
        return Err(ProtocolError::FrameTooShort {
            expected: 49,
            actual: data.len(),
        });
    }

    let mut info = ChannelInfo::default();

    info.index = data[0];

    // Name (32 bytes)
    let name = &data[1..33];
    let end = name.iter().position(|&b| b == 0).unwrap_or(32);
    info.name = String::from_utf8_lossy(&name[..end]).to_string();

    // Secret (16 bytes)
    info.secret.copy_from_slice(&data[33..49]);

    Ok(info)
}

fn decode_contact_message_v2(data: &[u8]) -> Result<ReceivedContactMessage, ProtocolError> {
    // 6 (prefix) + 1 (path_len) + 1 (txt_type) + 4 (timestamp) = 12
    if data.len() < 12 {
        return Err(ProtocolError::FrameTooShort {
            expected: 12,
            actual: data.len(),
        });
    }

    let sender_prefix = PublicKeyPrefix::from_slice(&data[0..6]).unwrap();
    let path_len = data[6];
    let text_type = TextType::from(data[7]);
    let timestamp = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);

    // Handle extra bytes for signed messages
    let (extra, text_start) = if text_type == TextType::SignedPlain && data.len() > 12 {
        (data[12..16].to_vec(), 16)
    } else {
        (Vec::new(), 12)
    };

    let text = String::from_utf8_lossy(&data[text_start..]).to_string();

    Ok(ReceivedContactMessage {
        sender_prefix,
        path_len,
        text_type,
        timestamp,
        snr_x4: None,
        extra,
        text,
    })
}

fn decode_contact_message_v3(data: &[u8]) -> Result<ReceivedContactMessage, ProtocolError> {
    // 3 (snr + reserved) + 6 (prefix) + 1 (path_len) + 1 (txt_type) + 4 (timestamp) = 15
    if data.len() < 15 {
        return Err(ProtocolError::FrameTooShort {
            expected: 15,
            actual: data.len(),
        });
    }

    let snr_x4 = data[0] as i8;
    // data[1], data[2] = reserved
    let sender_prefix = PublicKeyPrefix::from_slice(&data[3..9]).unwrap();
    let path_len = data[9];
    let text_type = TextType::from(data[10]);
    let timestamp = u32::from_le_bytes([data[11], data[12], data[13], data[14]]);

    // Handle extra bytes for signed messages
    let (extra, text_start) = if text_type == TextType::SignedPlain && data.len() > 15 {
        (data[15..19].to_vec(), 19)
    } else {
        (Vec::new(), 15)
    };

    let text = String::from_utf8_lossy(&data[text_start..]).to_string();

    Ok(ReceivedContactMessage {
        sender_prefix,
        path_len,
        text_type,
        timestamp,
        snr_x4: Some(snr_x4),
        extra,
        text,
    })
}

fn decode_channel_message_v2(data: &[u8]) -> Result<ReceivedChannelMessage, ProtocolError> {
    // 1 (channel_idx) + 1 (path_len) + 1 (txt_type) + 4 (timestamp) = 7
    if data.len() < 7 {
        return Err(ProtocolError::FrameTooShort {
            expected: 7,
            actual: data.len(),
        });
    }

    let channel_idx = data[0];
    let path_len = data[1];
    let text_type = TextType::from(data[2]);
    let timestamp = u32::from_le_bytes([data[3], data[4], data[5], data[6]]);
    let text = String::from_utf8_lossy(&data[7..]).to_string();

    Ok(ReceivedChannelMessage {
        channel_idx,
        path_len,
        text_type,
        timestamp,
        snr_x4: None,
        text,
    })
}

fn decode_channel_message_v3(data: &[u8]) -> Result<ReceivedChannelMessage, ProtocolError> {
    // 3 (snr + reserved) + 1 (channel_idx) + 1 (path_len) + 1 (txt_type) + 4 (timestamp) = 10
    if data.len() < 10 {
        return Err(ProtocolError::FrameTooShort {
            expected: 10,
            actual: data.len(),
        });
    }

    let snr_x4 = data[0] as i8;
    // data[1], data[2] = reserved
    let channel_idx = data[3];
    let path_len = data[4];
    let text_type = TextType::from(data[5]);
    let timestamp = u32::from_le_bytes([data[6], data[7], data[8], data[9]]);
    let text = String::from_utf8_lossy(&data[10..]).to_string();

    Ok(ReceivedChannelMessage {
        channel_idx,
        path_len,
        text_type,
        timestamp,
        snr_x4: Some(snr_x4),
        text,
    })
}
