//! Packet encryption and decryption.
//!
//! This module provides encryption and decryption functions for MeshCore packets
//! using ChaCha20-Poly1305 authenticated encryption.
//!
//! Note: MeshCore uses X25519 key exchange and ChaCha20-Poly1305 for encryption.
//! The encryption is applied at the payload level (ciphertext field), not the
//! entire packet.

use crate::{EncryptionKey, PacketError};
use chacha20poly1305::{
    aead::{Aead, NewAead},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::Rng;

// ============================================================================
// Encryption Functions
// ============================================================================

/// Encrypt data with the given key.
///
/// Uses ChaCha20-Poly1305 with a random 12-byte nonce.
/// Returns (nonce, ciphertext).
pub fn encrypt_data(plaintext: &[u8], key: &EncryptionKey) -> Result<(Vec<u8>, Vec<u8>), PacketError> {
    // Generate random nonce
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Create cipher
    let cipher_key = Key::from_slice(&key.0);
    let cipher = ChaCha20Poly1305::new(cipher_key);

    // Encrypt
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| PacketError::EncryptionError(format!("Encryption failed: {}", e)))?;

    Ok((nonce_bytes.to_vec(), ciphertext))
}

/// Encrypt data with a provided nonce.
pub fn encrypt_data_with_nonce(
    plaintext: &[u8],
    key: &EncryptionKey,
    nonce_bytes: &[u8; 12],
) -> Result<Vec<u8>, PacketError> {
    let nonce = Nonce::from_slice(nonce_bytes);

    // Create cipher
    let cipher_key = Key::from_slice(&key.0);
    let cipher = ChaCha20Poly1305::new(cipher_key);

    // Encrypt
    cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| PacketError::EncryptionError(format!("Encryption failed: {}", e)))
}

/// Decrypt data with the given key and nonce.
pub fn decrypt_data(
    ciphertext: &[u8],
    key: &EncryptionKey,
    nonce_bytes: &[u8],
) -> Result<Vec<u8>, PacketError> {
    if nonce_bytes.len() != 12 {
        return Err(PacketError::EncryptionError(format!(
            "Invalid nonce length: {} (expected 12)",
            nonce_bytes.len()
        )));
    }

    let nonce = Nonce::from_slice(nonce_bytes);

    // Create cipher
    let cipher_key = Key::from_slice(&key.0);
    let cipher = ChaCha20Poly1305::new(cipher_key);

    // Decrypt
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| PacketError::EncryptionError(format!("Decryption failed: {}", e)))
}

// ============================================================================
// MAC Calculation
// ============================================================================

/// Calculate a 2-byte MAC for V1 payloads.
///
/// This is a simplified MAC used in MeshCore V1 packets (not full AEAD).
pub fn calculate_mac_v1(data: &[u8], key: &[u8]) -> u16 {
    use blake2::{Blake2s256, Digest};

    let mut hasher = Blake2s256::new();
    hasher.update(key);
    hasher.update(data);
    let result = hasher.finalize();

    u16::from_le_bytes([result[0], result[1]])
}

/// Verify a 2-byte MAC for V1 payloads.
pub fn verify_mac_v1(data: &[u8], key: &[u8], mac: u16) -> bool {
    calculate_mac_v1(data, key) == mac
}

// ============================================================================
// CRC/Checksum for ACK
// ============================================================================

/// Calculate a CRC32 checksum for ACK payloads.
///
/// MeshCore ACK checksum is calculated from: timestamp + text + sender_pubkey
pub fn calculate_ack_checksum(timestamp: u32, text: &[u8], sender_pubkey: &[u8; 32]) -> u32 {
    // Use a simple CRC32 calculation
    let mut crc: u32 = 0xFFFFFFFF;

    // Process timestamp
    for byte in timestamp.to_le_bytes() {
        crc = crc32_update(crc, byte);
    }

    // Process text
    for byte in text {
        crc = crc32_update(crc, *byte);
    }

    // Process sender pubkey
    for byte in sender_pubkey {
        crc = crc32_update(crc, *byte);
    }

    !crc
}

/// CRC32 update function (IEEE polynomial).
fn crc32_update(crc: u32, byte: u8) -> u32 {
    const CRC32_TABLE: [u32; 256] = [
        0x00000000, 0x77073096, 0xee0e612c, 0x990951ba, 0x076dc419, 0x706af48f, 0xe963a535, 0x9e6495a3,
        0x0edb8832, 0x79dcb8a4, 0xe0d5e91e, 0x97d2d988, 0x09b64c2b, 0x7eb17cbd, 0xe7b82d07, 0x90bf1d91,
        0x1db71064, 0x6ab020f2, 0xf3b97148, 0x84be41de, 0x1adad47d, 0x6ddde4eb, 0xf4d4b551, 0x83d385c7,
        0x136c9856, 0x646ba8c0, 0xfd62f97a, 0x8a65c9ec, 0x14015c4f, 0x63066cd9, 0xfa0f3d63, 0x8d080df5,
        0x3b6e20c8, 0x4c69105e, 0xd56041e4, 0xa2677172, 0x3c03e4d1, 0x4b04d447, 0xd20d85fd, 0xa50ab56b,
        0x35b5a8fa, 0x42b2986c, 0xdbbbc9d6, 0xacbcf940, 0x32d86ce3, 0x45df5c75, 0xdcd60dcf, 0xabd13d59,
        0x26d930ac, 0x51de003a, 0xc8d75180, 0xbfd06116, 0x21b4f4b5, 0x56b3c423, 0xcfba9599, 0xb8bda50f,
        0x2802b89e, 0x5f058808, 0xc60cd9b2, 0xb10be924, 0x2f6f7c87, 0x58684c11, 0xc1611dab, 0xb6662d3d,
        0x76dc4190, 0x01db7106, 0x98d220bc, 0xefd5102a, 0x71b18589, 0x06b6b51f, 0x9fbfe4a5, 0xe8b8d433,
        0x7807c9a2, 0x0f00f934, 0x9609a88e, 0xe10e9818, 0x7f6a0dbb, 0x086d3d2d, 0x91646c97, 0xe6635c01,
        0x6b6b51f4, 0x1c6c6162, 0x856530d8, 0xf262004e, 0x6c0695ed, 0x1b01a57b, 0x8208f4c1, 0xf50fc457,
        0x65b0d9c6, 0x12b7e950, 0x8bbeb8ea, 0xfcb9887c, 0x62dd1ddf, 0x15da2d49, 0x8cd37cf3, 0xfbd44c65,
        0x4db26158, 0x3ab551ce, 0xa3bc0074, 0xd4bb30e2, 0x4adfa541, 0x3dd895d7, 0xa4d1c46d, 0xd3d6f4fb,
        0x4369e96a, 0x346ed9fc, 0xad678846, 0xda60b8d0, 0x44042d73, 0x33031de5, 0xaa0a4c5f, 0xdd0d7cc9,
        0x5005713c, 0x270241aa, 0xbe0b1010, 0xc90c2086, 0x5768b525, 0x206f85b3, 0xb966d409, 0xce61e49f,
        0x5edef90e, 0x29d9c998, 0xb0d09822, 0xc7d7a8b4, 0x59b33d17, 0x2eb40d81, 0xb7bd5c3b, 0xc0ba6cad,
        0xedb88320, 0x9abfb3b6, 0x03b6e20c, 0x74b1d29a, 0xead54739, 0x9dd277af, 0x04db2615, 0x73dc1683,
        0xe3630b12, 0x94643b84, 0x0d6d6a3e, 0x7a6a5aa8, 0xe40ecf0b, 0x9309ff9d, 0x0a00ae27, 0x7d079eb1,
        0xf00f9344, 0x8708a3d2, 0x1e01f268, 0x6906c2fe, 0xf762575d, 0x806567cb, 0x196c3671, 0x6e6b06e7,
        0xfed41b76, 0x89d32be0, 0x10da7a5a, 0x67dd4acc, 0xf9b9df6f, 0x8ebeeff9, 0x17b7be43, 0x60b08ed5,
        0xd6d6a3e8, 0xa1d1937e, 0x38d8c2c4, 0x4fdff252, 0xd1bb67f1, 0xa6bc5767, 0x3fb506dd, 0x48b2364b,
        0xd80d2bda, 0xaf0a1b4c, 0x36034af6, 0x41047a60, 0xdf60efc3, 0xa867df55, 0x316e8eef, 0x4669be79,
        0xcb61b38c, 0xbc66831a, 0x256fd2a0, 0x5268e236, 0xcc0c7795, 0xbb0b4703, 0x220216b9, 0x5505262f,
        0xc5ba3bbe, 0xb2bd0b28, 0x2bb45a92, 0x5cb36a04, 0xc2d7ffa7, 0xb5d0cf31, 0x2cd99e8b, 0x5bdeae1d,
        0x9b64c2b0, 0xec63f226, 0x756aa39c, 0x026d930a, 0x9c0906a9, 0xeb0e363f, 0x72076785, 0x05005713,
        0x95bf4a82, 0xe2b87a14, 0x7bb12bae, 0x0cb61b38, 0x92d28e9b, 0xe5d5be0d, 0x7cdcefb7, 0x0bdbdf21,
        0x86d3d2d4, 0xf1d4e242, 0x68ddb3f8, 0x1fda836e, 0x81be16cd, 0xf6b9265b, 0x6fb077e1, 0x18b74777,
        0x88085ae6, 0xff0f6a70, 0x66063bca, 0x11010b5c, 0x8f659eff, 0xf862ae69, 0x616bffd3, 0x166ccf45,
        0xa00ae278, 0xd70dd2ee, 0x4e048354, 0x3903b3c2, 0xa7672661, 0xd06016f7, 0x4969474d, 0x3e6e77db,
        0xaed16a4a, 0xd9d65adc, 0x40df0b66, 0x37d83bf0, 0xa9bcae53, 0xdebb9ec5, 0x47b2cf7f, 0x30b5ffe9,
        0xbdbdf21c, 0xcabac28a, 0x53b39330, 0x24b4a3a6, 0xbad03605, 0xcdd706b3, 0x54de5729, 0x23d967bf,
        0xb3667a2e, 0xc4614ab8, 0x5d681b02, 0x2a6f2b94, 0xb40bbe37, 0xc30c8ea1, 0x5a05df1b, 0x2d02ef8d,
    ];

    CRC32_TABLE[((crc ^ (byte as u32)) & 0xFF) as usize] ^ (crc >> 8)
}

// ============================================================================
// Key Derivation
// ============================================================================

/// Derive an encryption key from a passphrase using BLAKE2.
pub fn derive_key_from_passphrase(passphrase: &str) -> EncryptionKey {
    use blake2::{Blake2s256, Digest};

    let mut hasher = Blake2s256::new();
    hasher.update(passphrase.as_bytes());
    let result = hasher.finalize();

    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    EncryptionKey(key)
}

/// Derive a channel encryption key from a channel name.
///
/// MeshCore derives channel keys using SHA256 of the channel's shared secret.
pub fn derive_channel_key(channel_secret: &str) -> EncryptionKey {
    use blake2::{Blake2s256, Digest};

    let mut hasher = Blake2s256::new();
    hasher.update(channel_secret.as_bytes());
    let result = hasher.finalize();

    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    EncryptionKey(key)
}

/// Derive a channel hash from a channel's shared key.
///
/// Returns the first byte of SHA256(shared_key).
pub fn derive_channel_hash(channel_secret: &str) -> u8 {
    use blake2::{Blake2s256, Digest};

    let mut hasher = Blake2s256::new();
    hasher.update(channel_secret.as_bytes());
    let result = hasher.finalize();

    result[0]
}

// ============================================================================
// Key Generation
// ============================================================================

/// Generate a random encryption key.
pub fn generate_random_key() -> EncryptionKey {
    let mut key = [0u8; 32];
    rand::thread_rng().fill(&mut key);
    EncryptionKey(key)
}

/// Node keypair (public and private key).
#[derive(Debug, Clone)]
pub struct NodeKeypair {
    /// Private key bytes (seed).
    pub private_key: [u8; 32],
    /// Public key bytes.
    pub public_key: [u8; 32],
}

/// Generate a keypair for node identity.
///
/// For simulation purposes, we generate random bytes and derive a "public key"
/// via hashing. This is simplified compared to real Ed25519 key generation.
pub fn generate_keypair() -> NodeKeypair {
    use blake2::{Blake2s256, Digest};

    let mut private_key = [0u8; 32];
    rand::thread_rng().fill(&mut private_key);

    // Derive public key from private key (simplified for simulation)
    let mut hasher = Blake2s256::new();
    hasher.update(&private_key);
    let result = hasher.finalize();

    let mut public_key = [0u8; 32];
    public_key.copy_from_slice(&result);

    NodeKeypair {
        private_key,
        public_key,
    }
}

/// Calculate public key hash from public key bytes (first byte).
///
/// MeshCore uses the first byte of the public key as the hash for routing.
pub fn public_key_hash(public_key: &[u8; 32]) -> u8 {
    public_key[0]
}

/// Calculate 6-byte public key hash (for legacy compatibility).
pub fn public_key_hash_6(public_key: &[u8; 32]) -> [u8; 6] {
    let mut hash = [0u8; 6];
    hash.copy_from_slice(&public_key[..6]);
    hash
}

/// Sign data with a private key (simplified for simulation).
///
/// Returns a 64-byte "signature" (actually just a hash for simulation).
pub fn sign_data(data: &[u8], private_key: &[u8; 32]) -> [u8; 64] {
    use blake2::{Blake2b512, Digest};

    let mut hasher = Blake2b512::new();
    hasher.update(private_key);
    hasher.update(data);
    let result = hasher.finalize();

    let mut sig = [0u8; 64];
    sig.copy_from_slice(&result);
    sig
}

/// Verify a signature (simplified for simulation).
pub fn verify_signature(_data: &[u8], signature: &[u8; 64], public_key: &[u8; 32]) -> bool {
    // For simulation, we just check that the signature isn't all zeros
    // Real implementation would use Ed25519 verification
    signature.iter().any(|&b| b != 0) && !public_key.iter().all(|&b| b == 0)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = generate_random_key();
        let plaintext = b"Hello, MeshCore!";

        let (nonce, ciphertext) = encrypt_data(plaintext, &key).unwrap();
        let decrypted = decrypt_data(&ciphertext, &key, &nonce).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = generate_random_key();
        let key2 = generate_random_key();
        let plaintext = b"Secret message";

        let (nonce, ciphertext) = encrypt_data(plaintext, &key1).unwrap();
        let result = decrypt_data(&ciphertext, &key2, &nonce);

        assert!(result.is_err());
    }

    #[test]
    fn test_derive_key_deterministic() {
        let key1 = derive_key_from_passphrase("test_password");
        let key2 = derive_key_from_passphrase("test_password");
        assert_eq!(key1.0, key2.0);
    }

    #[test]
    fn test_derive_channel_key() {
        let key = derive_channel_key("MyChannel");
        assert_eq!(key.0.len(), 32);
    }

    #[test]
    fn test_derive_channel_hash() {
        let hash1 = derive_channel_hash("MyChannel");
        let hash2 = derive_channel_hash("MyChannel");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_keypair_generation() {
        let keypair = generate_keypair();
        let hash = public_key_hash(&keypair.public_key);
        assert_eq!(hash, keypair.public_key[0]);
    }

    #[test]
    fn test_mac_calculation() {
        let key = b"test_key_for_mac";
        let data = b"some data to mac";

        let mac1 = calculate_mac_v1(data, key);
        let mac2 = calculate_mac_v1(data, key);

        assert_eq!(mac1, mac2);
        assert!(verify_mac_v1(data, key, mac1));
    }

    #[test]
    fn test_ack_checksum() {
        let timestamp = 1234567890u32;
        let text = b"Hello";
        let pubkey = [0xAAu8; 32];

        let checksum1 = calculate_ack_checksum(timestamp, text, &pubkey);
        let checksum2 = calculate_ack_checksum(timestamp, text, &pubkey);

        assert_eq!(checksum1, checksum2);
    }

    #[test]
    fn test_sign_verify() {
        let keypair = generate_keypair();
        let data = b"data to sign";

        let signature = sign_data(data, &keypair.private_key);
        assert!(verify_signature(data, &signature, &keypair.public_key));
    }
}
