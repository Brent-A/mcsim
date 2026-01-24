//! Packet encoding and decoding.
//!
//! This module provides functions for encoding MeshCore packets to bytes
//! and decoding bytes back to packets, following the official MeshCore
//! packet structure specification.
//!
//! ## Packet Format
//!
//! | Field           | Size (bytes)                     | Description                                               |
//! |-----------------|----------------------------------|-----------------------------------------------------------|
//! | header          | 1                                | Contains routing type, payload type, and payload version. |
//! | transport_codes | 4 (optional)                     | 2x 16-bit transport codes (if ROUTE_TYPE_TRANSPORT_*)     |
//! | path_len        | 1                                | Length of the path field in bytes.                        |
//! | path            | up to 64 (`MAX_PATH_SIZE`)       | Stores the routing path if applicable.                    |
//! | payload         | up to 184 (`MAX_PACKET_PAYLOAD`) | The actual data being transmitted.                        |

use crate::{
    AckPayload, AdvertFlags, AdvertPayload, AnonRequestPayload, ControlPayload, ControlSubType,
    EncryptedHeader, GroupMessagePayload, MeshCorePacket, MultipartPayload, PacketError,
    PacketHeader, PacketPayload, PathPayload, PayloadType, PayloadVersion, RequestPayload,
    ResponsePayload, TextMessagePayload, TracePayload, TransportCodes,
    MAX_PACKET_PAYLOAD, MAX_PACKET_SIZE, MAX_PATH_SIZE,
};

// ============================================================================
// Encoding Functions
// ============================================================================

/// Encode a packet to bytes.
pub fn encode_packet(packet: &MeshCorePacket) -> Vec<u8> {
    let mut buf = Vec::with_capacity(MAX_PACKET_SIZE);

    // 1. Header byte (route_type + payload_type + version)
    buf.push(packet.header.encode_header_byte());

    // 2. Transport codes (optional, 4 bytes)
    if let Some(ref codes) = packet.header.transport_codes {
        buf.extend_from_slice(&codes.encode());
    }

    // 3. Path length (1 byte)
    buf.push(packet.path.len() as u8);

    // 4. Path (variable length)
    buf.extend_from_slice(&packet.path);

    // 5. Payload (variable length)
    let payload_bytes = encode_payload(&packet.payload);
    buf.extend_from_slice(&payload_bytes);

    buf
}

/// Encode payload to bytes.
pub fn encode_payload(payload: &PacketPayload) -> Vec<u8> {
    match payload {
        PacketPayload::Advert(advert) => encode_advert_payload(advert),
        PacketPayload::Ack(ack) => encode_ack_payload(ack),
        PacketPayload::Path(path) => encode_path_payload(path),
        PacketPayload::Request(req) => encode_request_payload(req),
        PacketPayload::Response(resp) => encode_response_payload(resp),
        PacketPayload::TextMessage(text) => encode_text_message_payload(text),
        PacketPayload::AnonRequest(anon) => encode_anon_request_payload(anon),
        PacketPayload::GroupText(group) | PacketPayload::GroupData(group) => {
            encode_group_message_payload(group)
        }
        PacketPayload::Trace(trace) => trace.data.clone(),
        PacketPayload::Multipart(multi) => multi.data.clone(),
        PacketPayload::Control(ctrl) => encode_control_payload(ctrl),
        PacketPayload::Raw(data) => data.clone(),
    }
}

/// Encode advertisement payload.
/// Format: public_key(32) + timestamp(4) + signature(64) + appdata
fn encode_advert_payload(advert: &AdvertPayload) -> Vec<u8> {
    let mut buf = Vec::with_capacity(100 + advert.name.as_ref().map_or(0, |n| n.len()));

    // Public key (32 bytes)
    buf.extend_from_slice(&advert.public_key);

    // Timestamp (4 bytes, little-endian)
    buf.extend_from_slice(&advert.timestamp.to_le_bytes());

    // Signature (64 bytes)
    buf.extend_from_slice(&advert.signature);

    // Appdata: flags (1 byte)
    buf.push(advert.flags.to_byte());

    // Appdata: latitude (4 bytes, optional)
    if advert.flags.has_location {
        if let Some(lat) = advert.latitude {
            buf.extend_from_slice(&lat.to_le_bytes());
        }
    }

    // Appdata: longitude (4 bytes, optional)
    if advert.flags.has_location {
        if let Some(lon) = advert.longitude {
            buf.extend_from_slice(&lon.to_le_bytes());
        }
    }

    // Appdata: feature1 (2 bytes, optional)
    if advert.flags.has_feature1 {
        if let Some(f1) = advert.feature1 {
            buf.extend_from_slice(&f1.to_le_bytes());
        }
    }

    // Appdata: feature2 (2 bytes, optional)
    if advert.flags.has_feature2 {
        if let Some(f2) = advert.feature2 {
            buf.extend_from_slice(&f2.to_le_bytes());
        }
    }

    // Appdata: name (rest of payload, optional)
    if advert.flags.has_name {
        if let Some(ref name) = advert.name {
            buf.extend_from_slice(name.as_bytes());
        }
    }

    buf
}

/// Encode acknowledgement payload.
/// Format: checksum(4)
fn encode_ack_payload(ack: &AckPayload) -> Vec<u8> {
    ack.checksum.to_le_bytes().to_vec()
}

/// Encode encrypted header (common for REQ, RESPONSE, TXT_MSG, PATH).
/// Format: dest_hash(1) + src_hash(1) + mac(2)
fn encode_encrypted_header(header: &EncryptedHeader) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4);
    buf.push(header.dest_hash);
    buf.push(header.src_hash);
    buf.extend_from_slice(&header.mac.to_le_bytes());
    buf
}

/// Encode path payload.
/// Format: encrypted_header(4) + ciphertext
fn encode_path_payload(path: &PathPayload) -> Vec<u8> {
    let mut buf = encode_encrypted_header(&path.header);
    buf.extend_from_slice(&path.ciphertext);
    buf
}

/// Encode request payload.
/// Format: encrypted_header(4) + ciphertext
fn encode_request_payload(req: &RequestPayload) -> Vec<u8> {
    let mut buf = encode_encrypted_header(&req.header);
    buf.extend_from_slice(&req.ciphertext);
    buf
}

/// Encode response payload.
/// Format: encrypted_header(4) + ciphertext
fn encode_response_payload(resp: &ResponsePayload) -> Vec<u8> {
    let mut buf = encode_encrypted_header(&resp.header);
    buf.extend_from_slice(&resp.ciphertext);
    buf
}

/// Encode text message payload.
/// Format: encrypted_header(4) + ciphertext
fn encode_text_message_payload(text: &TextMessagePayload) -> Vec<u8> {
    let mut buf = encode_encrypted_header(&text.header);
    buf.extend_from_slice(&text.ciphertext);
    buf
}

/// Encode anonymous request payload.
/// Format: dest_hash(1) + public_key(32) + mac(2) + ciphertext
fn encode_anon_request_payload(anon: &AnonRequestPayload) -> Vec<u8> {
    let mut buf = Vec::with_capacity(35 + anon.ciphertext.len());
    buf.push(anon.dest_hash);
    buf.extend_from_slice(&anon.public_key);
    buf.extend_from_slice(&anon.mac.to_le_bytes());
    buf.extend_from_slice(&anon.ciphertext);
    buf
}

/// Encode group message payload.
/// Format: channel_hash(1) + mac(2) + ciphertext
fn encode_group_message_payload(group: &GroupMessagePayload) -> Vec<u8> {
    let mut buf = Vec::with_capacity(3 + group.ciphertext.len());
    buf.push(group.channel_hash);
    buf.extend_from_slice(&group.mac.to_le_bytes());
    buf.extend_from_slice(&group.ciphertext);
    buf
}

/// Encode control payload.
/// Format: flags(1) + data
fn encode_control_payload(ctrl: &ControlPayload) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + ctrl.data.len());
    let flags = ctrl.sub_type.to_flags() | (ctrl.flags_lower & 0x0F);
    buf.push(flags);
    buf.extend_from_slice(&ctrl.data);
    buf
}

// ============================================================================
// Decoding Functions
// ============================================================================

/// Decode a packet from bytes.
pub fn decode_packet(data: &[u8]) -> Result<MeshCorePacket, PacketError> {
    if data.is_empty() {
        return Err(PacketError::DecodeError {
            offset: 0,
            message: "Empty packet data".to_string(),
        });
    }

    let mut offset = 0;

    // 1. Header byte
    let header_byte = data[offset];
    let mut header = PacketHeader::from_header_byte(header_byte)?;
    offset += 1;

    // 2. Transport codes (optional, 4 bytes)
    if header.route_type.has_transport_codes() {
        if offset + 4 > data.len() {
            return Err(PacketError::DecodeError {
                offset,
                message: "Not enough data for transport codes".to_string(),
            });
        }
        header.transport_codes = Some(TransportCodes::decode(&data[offset..offset + 4]));
        offset += 4;
    }

    // 3. Path length (1 byte)
    if offset >= data.len() {
        return Err(PacketError::DecodeError {
            offset,
            message: "Not enough data for path length".to_string(),
        });
    }
    let path_len = data[offset] as usize;
    header.path_len = path_len as u8;
    offset += 1;

    // Validate path length
    if path_len > MAX_PATH_SIZE {
        return Err(PacketError::DecodeError {
            offset: offset - 1,
            message: format!("Path length {} exceeds maximum {}", path_len, MAX_PATH_SIZE),
        });
    }

    // 4. Path (variable length)
    if offset + path_len > data.len() {
        return Err(PacketError::DecodeError {
            offset,
            message: format!(
                "Not enough data for path: need {} bytes, have {}",
                path_len,
                data.len() - offset
            ),
        });
    }
    let path = data[offset..offset + path_len].to_vec();
    offset += path_len;

    // 5. Payload (rest of packet)
    let payload_data = &data[offset..];
    if payload_data.len() > MAX_PACKET_PAYLOAD {
        return Err(PacketError::DecodeError {
            offset,
            message: format!(
                "Payload length {} exceeds maximum {}",
                payload_data.len(),
                MAX_PACKET_PAYLOAD
            ),
        });
    }

    let payload = decode_payload(header.payload_type, header.version, payload_data)?;

    Ok(MeshCorePacket {
        header,
        path,
        payload,
    })
}

/// Decode payload based on type.
fn decode_payload(
    payload_type: PayloadType,
    version: PayloadVersion,
    data: &[u8],
) -> Result<PacketPayload, PacketError> {
    match payload_type {
        PayloadType::Advert => Ok(PacketPayload::Advert(decode_advert_payload(data)?)),
        PayloadType::Ack => Ok(PacketPayload::Ack(decode_ack_payload(data)?)),
        PayloadType::Path => Ok(PacketPayload::Path(decode_path_payload(data, version)?)),
        PayloadType::Request => Ok(PacketPayload::Request(decode_request_payload(data, version)?)),
        PayloadType::Response => {
            Ok(PacketPayload::Response(decode_response_payload(data, version)?))
        }
        PayloadType::TextMessage => {
            Ok(PacketPayload::TextMessage(decode_text_message_payload(data, version)?))
        }
        PayloadType::AnonRequest => {
            Ok(PacketPayload::AnonRequest(decode_anon_request_payload(data)?))
        }
        PayloadType::GroupText => {
            Ok(PacketPayload::GroupText(decode_group_message_payload(data)?))
        }
        PayloadType::GroupData => {
            Ok(PacketPayload::GroupData(decode_group_message_payload(data)?))
        }
        PayloadType::Trace => Ok(PacketPayload::Trace(TracePayload {
            data: data.to_vec(),
        })),
        PayloadType::Multipart => Ok(PacketPayload::Multipart(MultipartPayload {
            data: data.to_vec(),
        })),
        PayloadType::Control => Ok(PacketPayload::Control(decode_control_payload(data)?)),
        PayloadType::RawCustom => Ok(PacketPayload::Raw(data.to_vec())),
    }
}

/// Decode advertisement payload.
fn decode_advert_payload(data: &[u8]) -> Result<AdvertPayload, PacketError> {
    // Minimum size: public_key(32) + timestamp(4) + signature(64) + flags(1) = 101 bytes
    if data.len() < 101 {
        return Err(PacketError::DecodeError {
            offset: 0,
            message: format!(
                "Advertisement payload too short: {} bytes (minimum 101)",
                data.len()
            ),
        });
    }

    let mut offset = 0;

    // Public key (32 bytes)
    let mut public_key = [0u8; 32];
    public_key.copy_from_slice(&data[offset..offset + 32]);
    offset += 32;

    // Timestamp (4 bytes)
    let timestamp = u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]);
    offset += 4;

    // Signature (64 bytes)
    let mut signature = [0u8; 64];
    signature.copy_from_slice(&data[offset..offset + 64]);
    offset += 64;

    // Appdata: flags (1 byte)
    let flags = AdvertFlags::from_byte(data[offset]);
    offset += 1;

    // Appdata: latitude (4 bytes, optional)
    let latitude = if flags.has_location && offset + 4 <= data.len() {
        let lat = i32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]);
        offset += 4;
        Some(lat)
    } else {
        None
    };

    // Appdata: longitude (4 bytes, optional)
    let longitude = if flags.has_location && offset + 4 <= data.len() {
        let lon = i32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]);
        offset += 4;
        Some(lon)
    } else {
        None
    };

    // Appdata: feature1 (2 bytes, optional)
    let feature1 = if flags.has_feature1 && offset + 2 <= data.len() {
        let f1 = u16::from_le_bytes([data[offset], data[offset + 1]]);
        offset += 2;
        Some(f1)
    } else {
        None
    };

    // Appdata: feature2 (2 bytes, optional)
    let feature2 = if flags.has_feature2 && offset + 2 <= data.len() {
        let f2 = u16::from_le_bytes([data[offset], data[offset + 1]]);
        offset += 2;
        Some(f2)
    } else {
        None
    };

    // Appdata: name (rest of payload, optional)
    let name = if flags.has_name && offset < data.len() {
        Some(String::from_utf8_lossy(&data[offset..]).to_string())
    } else {
        None
    };

    Ok(AdvertPayload {
        public_key,
        timestamp,
        signature,
        flags,
        latitude,
        longitude,
        feature1,
        feature2,
        name,
    })
}

/// Decode acknowledgement payload.
fn decode_ack_payload(data: &[u8]) -> Result<AckPayload, PacketError> {
    if data.len() < 4 {
        return Err(PacketError::DecodeError {
            offset: 0,
            message: format!("ACK payload too short: {} bytes (minimum 4)", data.len()),
        });
    }

    let checksum = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    Ok(AckPayload { checksum })
}

/// Decode encrypted header.
fn decode_encrypted_header(data: &[u8], version: PayloadVersion) -> Result<(EncryptedHeader, usize), PacketError> {
    let hash_size = version.hash_size();
    let mac_size = version.mac_size();
    let header_size = hash_size * 2 + mac_size;

    if data.len() < header_size {
        return Err(PacketError::DecodeError {
            offset: 0,
            message: format!(
                "Not enough data for encrypted header: {} bytes (minimum {})",
                data.len(),
                header_size
            ),
        });
    }

    let dest_hash = data[0];
    let src_hash = data[hash_size];
    let mac = u16::from_le_bytes([data[hash_size * 2], data[hash_size * 2 + 1]]);

    Ok((EncryptedHeader { dest_hash, src_hash, mac }, header_size))
}

/// Decode path payload.
fn decode_path_payload(data: &[u8], version: PayloadVersion) -> Result<PathPayload, PacketError> {
    let (header, header_size) = decode_encrypted_header(data, version)?;
    let ciphertext = data[header_size..].to_vec();
    Ok(PathPayload { header, ciphertext })
}

/// Decode request payload.
fn decode_request_payload(data: &[u8], version: PayloadVersion) -> Result<RequestPayload, PacketError> {
    let (header, header_size) = decode_encrypted_header(data, version)?;
    let ciphertext = data[header_size..].to_vec();
    Ok(RequestPayload { header, ciphertext })
}

/// Decode response payload.
fn decode_response_payload(data: &[u8], version: PayloadVersion) -> Result<ResponsePayload, PacketError> {
    let (header, header_size) = decode_encrypted_header(data, version)?;
    let ciphertext = data[header_size..].to_vec();
    Ok(ResponsePayload { header, ciphertext })
}

/// Decode text message payload.
fn decode_text_message_payload(data: &[u8], version: PayloadVersion) -> Result<TextMessagePayload, PacketError> {
    let (header, header_size) = decode_encrypted_header(data, version)?;
    let ciphertext = data[header_size..].to_vec();
    Ok(TextMessagePayload { header, ciphertext })
}

/// Decode anonymous request payload.
fn decode_anon_request_payload(data: &[u8]) -> Result<AnonRequestPayload, PacketError> {
    // Minimum size: dest_hash(1) + public_key(32) + mac(2) = 35 bytes
    if data.len() < 35 {
        return Err(PacketError::DecodeError {
            offset: 0,
            message: format!(
                "Anonymous request payload too short: {} bytes (minimum 35)",
                data.len()
            ),
        });
    }

    let dest_hash = data[0];

    let mut public_key = [0u8; 32];
    public_key.copy_from_slice(&data[1..33]);

    let mac = u16::from_le_bytes([data[33], data[34]]);

    let ciphertext = data[35..].to_vec();

    Ok(AnonRequestPayload {
        dest_hash,
        public_key,
        mac,
        ciphertext,
    })
}

/// Decode group message payload.
fn decode_group_message_payload(data: &[u8]) -> Result<GroupMessagePayload, PacketError> {
    // Minimum size: channel_hash(1) + mac(2) = 3 bytes
    if data.len() < 3 {
        return Err(PacketError::DecodeError {
            offset: 0,
            message: format!(
                "Group message payload too short: {} bytes (minimum 3)",
                data.len()
            ),
        });
    }

    let channel_hash = data[0];
    let mac = u16::from_le_bytes([data[1], data[2]]);
    let ciphertext = data[3..].to_vec();

    Ok(GroupMessagePayload {
        channel_hash,
        mac,
        ciphertext,
    })
}

/// Decode control payload.
fn decode_control_payload(data: &[u8]) -> Result<ControlPayload, PacketError> {
    if data.is_empty() {
        return Err(PacketError::DecodeError {
            offset: 0,
            message: "Control payload is empty".to_string(),
        });
    }

    let flags = data[0];
    let sub_type = ControlSubType::from_flags(flags);
    let flags_lower = flags & 0x0F;
    let payload_data = data[1..].to_vec();

    Ok(ControlPayload {
        sub_type,
        flags_lower,
        data: payload_data,
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RouteType;

    #[test]
    fn test_advert_roundtrip() {
        let advert = AdvertPayload::repeater([0xAA; 32], 1234567890, [0xBB; 64], "TestNode");
        let packet = MeshCorePacket::advert(advert);

        let encoded = encode_packet(&packet);
        let decoded = decode_packet(&encoded).unwrap();

        assert_eq!(decoded.header.route_type, RouteType::Flood);
        assert_eq!(decoded.header.payload_type, PayloadType::Advert);

        if let PacketPayload::Advert(payload) = &decoded.payload {
            assert_eq!(payload.public_key, [0xAA; 32]);
            assert_eq!(payload.timestamp, 1234567890);
            assert_eq!(payload.signature, [0xBB; 64]);
            assert!(payload.flags.is_repeater);
            assert!(payload.flags.has_name);
            assert_eq!(payload.name.as_deref(), Some("TestNode"));
        } else {
            panic!("Expected Advert payload");
        }
    }

    #[test]
    fn test_advert_with_location_roundtrip() {
        let advert = AdvertPayload::new([0x11; 32], 1000000, [0x22; 64], "GPS Node")
            .with_location(37.7749, -122.4194);
        let packet = MeshCorePacket::advert(advert);

        let encoded = encode_packet(&packet);
        let decoded = decode_packet(&encoded).unwrap();

        if let PacketPayload::Advert(payload) = &decoded.payload {
            assert!(payload.flags.has_location);
            let lat = payload.latitude_f64().unwrap();
            let lon = payload.longitude_f64().unwrap();
            assert!((lat - 37.7749).abs() < 0.0001);
            assert!((lon - (-122.4194)).abs() < 0.0001);
        } else {
            panic!("Expected Advert payload");
        }
    }

    #[test]
    fn test_ack_roundtrip() {
        let packet = MeshCorePacket::ack(0xDEADBEEF);

        let encoded = encode_packet(&packet);
        let decoded = decode_packet(&encoded).unwrap();

        assert_eq!(decoded.header.payload_type, PayloadType::Ack);

        if let PacketPayload::Ack(payload) = &decoded.payload {
            assert_eq!(payload.checksum, 0xDEADBEEF);
        } else {
            panic!("Expected Ack payload");
        }
    }

    #[test]
    fn test_text_message_roundtrip() {
        let packet = MeshCorePacket::text_message(0xAA, 0xBB, 0x1234, vec![1, 2, 3, 4, 5]);

        let encoded = encode_packet(&packet);
        let decoded = decode_packet(&encoded).unwrap();

        assert_eq!(decoded.header.route_type, RouteType::Direct);
        assert_eq!(decoded.header.payload_type, PayloadType::TextMessage);

        if let PacketPayload::TextMessage(payload) = &decoded.payload {
            assert_eq!(payload.header.dest_hash, 0xAA);
            assert_eq!(payload.header.src_hash, 0xBB);
            assert_eq!(payload.header.mac, 0x1234);
            assert_eq!(payload.ciphertext, vec![1, 2, 3, 4, 5]);
        } else {
            panic!("Expected TextMessage payload");
        }
    }

    #[test]
    fn test_group_text_roundtrip() {
        let packet = MeshCorePacket::group_text(0x42, 0xABCD, vec![10, 20, 30]);

        let encoded = encode_packet(&packet);
        let decoded = decode_packet(&encoded).unwrap();

        assert_eq!(decoded.header.route_type, RouteType::Flood);
        assert_eq!(decoded.header.payload_type, PayloadType::GroupText);

        if let PacketPayload::GroupText(payload) = &decoded.payload {
            assert_eq!(payload.channel_hash, 0x42);
            assert_eq!(payload.mac, 0xABCD);
            assert_eq!(payload.ciphertext, vec![10, 20, 30]);
        } else {
            panic!("Expected GroupText payload");
        }
    }

    #[test]
    fn test_packet_with_path() {
        let mut packet = MeshCorePacket::ack(0x12345678);
        packet.path = vec![0x01, 0x02, 0x03]; // 3-node path

        let encoded = encode_packet(&packet);
        let decoded = decode_packet(&encoded).unwrap();

        assert_eq!(decoded.header.path_len, 3);
        assert_eq!(decoded.path, vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_transport_codes() {
        let mut packet = MeshCorePacket::ack(0xAAAA);
        packet.header.route_type = RouteType::TransportFlood;
        packet.header.transport_codes = Some(TransportCodes {
            code1: 0x1234,
            code2: 0x5678,
        });

        let encoded = encode_packet(&packet);
        let decoded = decode_packet(&encoded).unwrap();

        assert_eq!(decoded.header.route_type, RouteType::TransportFlood);
        let codes = decoded.header.transport_codes.unwrap();
        assert_eq!(codes.code1, 0x1234);
        assert_eq!(codes.code2, 0x5678);
    }

    #[test]
    fn test_raw_payload_roundtrip() {
        let raw_data = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        let packet = MeshCorePacket::new(RouteType::Flood, PacketPayload::Raw(raw_data.clone()));

        // Need to set the header payload type manually for raw
        let mut packet = packet;
        packet.header.payload_type = PayloadType::RawCustom;

        let encoded = encode_packet(&packet);
        let decoded = decode_packet(&encoded).unwrap();

        if let PacketPayload::Raw(data) = &decoded.payload {
            assert_eq!(data, &raw_data);
        } else {
            panic!("Expected Raw payload");
        }
    }

    #[test]
    fn test_decode_empty_packet() {
        let result = decode_packet(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_too_short() {
        // Just a header byte, no path length
        let data = [0x11]; // Flood + Advert
        let result = decode_packet(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_control_payload_roundtrip() {
        let ctrl = ControlPayload {
            sub_type: ControlSubType::DiscoverRequest,
            flags_lower: 0x01,
            data: vec![0x02, 0x03, 0x04],
        };
        let packet = MeshCorePacket::new(RouteType::Flood, PacketPayload::Control(ctrl));

        let encoded = encode_packet(&packet);
        let decoded = decode_packet(&encoded).unwrap();

        assert_eq!(decoded.header.payload_type, PayloadType::Control);

        if let PacketPayload::Control(payload) = &decoded.payload {
            assert_eq!(payload.sub_type, ControlSubType::DiscoverRequest);
            assert_eq!(payload.flags_lower, 0x01);
            assert_eq!(payload.data, vec![0x02, 0x03, 0x04]);
        } else {
            panic!("Expected Control payload");
        }
    }
}
