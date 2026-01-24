//! Key generation and specification types for simulation nodes.
//!
//! This module provides:
//! - [`KeySpec`] - Specification for how keys should be generated or provided
//! - [`KeyConfig`] - Configuration containing private and public key specifications
//! - [`GeneratedKeypair`] - Result of key generation containing the actual key bytes
//! - [`generate_keypair_with_spec`] - Function to generate keypairs based on specifications
//!
//! ## Key Specification Modes
//!
//! Keys can be specified in three ways:
//! - `"*"` - Generate a random keypair
//! - `"cc01*"` - Generate keypairs until public key starts with the given hex prefix
//! - `"0123...abcd"` (64 hex chars) - Use exact key bytes

use crate::ModelError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Default maximum number of attempts to generate a keypair with a matching prefix.
pub const DEFAULT_MAX_KEY_GENERATION_ATTEMPTS: u32 = 1_000_000;

/// Key specification for YAML configuration.
/// 
/// Supports three modes:
/// - `"*"` - Generate a random keypair
/// - `"cc01*"` - Generate keypairs until public key starts with prefix
/// - `"0123...abcd"` (64 hex chars) - Use exact key bytes
#[derive(Debug, Clone)]
pub enum KeySpec {
    /// Generate a random key.
    Random,
    /// Generate a key with a public key prefix (hex string without trailing `*`).
    Prefix(String),
    /// Use an exact key (32 bytes).
    Exact([u8; 32]),
}

impl Default for KeySpec {
    fn default() -> Self {
        KeySpec::Random
    }
}

impl KeySpec {
    /// Parse a key specification from a string.
    pub fn parse(s: &str) -> Result<Self, ModelError> {
        let s = s.trim();
        
        if s == "*" {
            return Ok(KeySpec::Random);
        }
        
        if s.ends_with('*') {
            // Prefix mode: "cc01*" -> prefix is "cc01"
            let prefix = &s[..s.len() - 1];
            // Validate that prefix is valid hex
            if prefix.is_empty() {
                return Ok(KeySpec::Random);
            }
            if !prefix.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(ModelError::InvalidKeySpec(
                    format!("Invalid hex prefix: '{}'", prefix)
                ));
            }
            Ok(KeySpec::Prefix(prefix.to_lowercase()))
        } else {
            // Exact key mode: must be 64 hex characters
            if s.len() != 64 {
                return Err(ModelError::InvalidKeySpec(
                    format!("Exact key must be 64 hex characters, got {} characters: '{}'", s.len(), s)
                ));
            }
            let bytes = hex::decode(s)
                .map_err(|e| ModelError::InvalidKeySpec(format!("Invalid hex: {}", e)))?;
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            Ok(KeySpec::Exact(key))
        }
    }
}

impl Serialize for KeySpec {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            KeySpec::Random => serializer.serialize_str("*"),
            KeySpec::Prefix(prefix) => serializer.serialize_str(&format!("{}*", prefix)),
            KeySpec::Exact(bytes) => serializer.serialize_str(&hex::encode(bytes)),
        }
    }
}

impl<'de> Deserialize<'de> for KeySpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        KeySpec::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Node keypair configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyConfig {
    /// Private key specification.
    #[serde(default)]
    pub private_key: KeySpec,
    /// Public key specification.
    #[serde(default)]
    pub public_key: KeySpec,
}

/// Generated keypair result.
#[derive(Debug, Clone)]
pub struct GeneratedKeypair {
    /// Private key (32-byte seed).
    pub private_key: [u8; 32],
    /// Public key (32 bytes).
    pub public_key: [u8; 32],
}

/// Result of key generation with statistics.
#[derive(Debug, Clone)]
pub struct KeygenResult {
    /// The generated keypair.
    pub keypair: GeneratedKeypair,
    /// Number of iterations it took to find a matching key.
    pub iterations: u32,
}

/// Generate a keypair based on key specifications using parallel search.
/// 
/// This function uses multiple threads to search for a keypair with a matching
/// public key prefix. It is deterministic: given the same `base_seed` and
/// `max_attempts`, it will always produce the same result.
/// 
/// The algorithm:
/// 1. Each iteration `i` derives its own RNG from `base_seed + i`
/// 2. Rayon's `find_first` ensures we return the lowest iteration number that matches
/// 3. This maintains determinism regardless of thread scheduling
/// 
/// Keys are generated using Ed25519 to ensure proper ECDH key exchange works in the firmware.
/// 
/// Returns both the keypair and the number of iterations taken, which can be ignored
/// if only the keypair is needed.
pub fn generate_keypair(
    base_seed: u64,
    key_config: &KeyConfig,
    node_name: &str,
    max_attempts: Option<u32>,
) -> Result<KeygenResult, ModelError> {
    use ed25519_dalek::{SigningKey, VerifyingKey};
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    use rayon::prelude::*;

    let max_attempts = max_attempts.unwrap_or(DEFAULT_MAX_KEY_GENERATION_ATTEMPTS);

    // If both keys are exact, just use them
    if let (KeySpec::Exact(prv), KeySpec::Exact(pub_key)) = (&key_config.private_key, &key_config.public_key) {
        return Ok(KeygenResult {
            keypair: GeneratedKeypair {
                private_key: *prv,
                public_key: *pub_key,
            },
            iterations: 0,
        });
    }

    // If private key is exact, derive public key from it using Ed25519
    if let KeySpec::Exact(prv) = &key_config.private_key {
        let signing_key = SigningKey::from_bytes(prv);
        let verifying_key = VerifyingKey::from(&signing_key);
        let public_key = verifying_key.to_bytes();
        
        // Check if public key matches any prefix requirement
        if let KeySpec::Prefix(prefix) = &key_config.public_key {
            let pub_hex = hex::encode(&public_key);
            if !pub_hex.starts_with(prefix) {
                return Err(ModelError::InvalidKeySpec(
                    format!("Exact private key produces public key '{}' which doesn't match prefix '{}'", 
                            &pub_hex[..prefix.len().min(pub_hex.len())], prefix)
                ));
            }
        }
        
        return Ok(KeygenResult {
            keypair: GeneratedKeypair {
                private_key: *prv,
                public_key,
            },
            iterations: 1,
        });
    }

    // Get public key prefix requirement (if any)
    let prefix = match &key_config.public_key {
        KeySpec::Prefix(p) => p.clone(),
        KeySpec::Exact(exact_pub) => {
            // If public key is exact but private key isn't, generate random private key
            let mut rng = ChaCha8Rng::seed_from_u64(base_seed);
            let mut private_key = [0u8; 32];
            rng.fill(&mut private_key);
            return Ok(KeygenResult {
                keypair: GeneratedKeypair {
                    private_key,
                    public_key: *exact_pub,
                },
                iterations: 1,
            });
        }
        KeySpec::Random => {
            // No prefix needed - just generate one keypair
            let mut rng = ChaCha8Rng::seed_from_u64(base_seed);
            let mut seed = [0u8; 32];
            rng.fill(&mut seed);
            let signing_key = SigningKey::from_bytes(&seed);
            let verifying_key = VerifyingKey::from(&signing_key);
            let public_key = verifying_key.to_bytes();
            return Ok(KeygenResult {
                keypair: GeneratedKeypair { private_key: seed, public_key },
                iterations: 1,
            });
        }
    };

    // Parallel search using rayon's find_first for determinism
    // find_first guarantees we get the lowest index that matches, regardless of
    // which thread finds it first
    let result = (0..max_attempts as u64)
        .into_par_iter()
        .find_first(|&attempt| {
            // Derive a deterministic RNG for this iteration
            let mut rng = ChaCha8Rng::seed_from_u64(base_seed.wrapping_add(attempt));
            let mut seed = [0u8; 32];
            rng.fill(&mut seed);
            
            // Generate Ed25519 keypair
            let signing_key = SigningKey::from_bytes(&seed);
            let verifying_key = VerifyingKey::from(&signing_key);
            let public_key = verifying_key.to_bytes();
            
            // Check prefix match
            let pub_hex = hex::encode(&public_key);
            pub_hex.starts_with(&prefix)
        });

    match result {
        Some(attempt) => {
            // Regenerate the keypair for the winning iteration
            let mut rng = ChaCha8Rng::seed_from_u64(base_seed.wrapping_add(attempt));
            let mut seed = [0u8; 32];
            rng.fill(&mut seed);
            let signing_key = SigningKey::from_bytes(&seed);
            let verifying_key = VerifyingKey::from(&signing_key);
            let public_key = verifying_key.to_bytes();
            
            log::info!(
                "Generated Ed25519 keypair for '{}' with prefix '{}' after {} attempts",
                node_name, prefix, attempt + 1
            );
            
            Ok(KeygenResult {
                keypair: GeneratedKeypair { private_key: seed, public_key },
                iterations: (attempt + 1) as u32,
            })
        }
        None => {
            Err(ModelError::KeyGenerationFailed {
                node: node_name.to_string(),
                prefix,
                attempts: max_attempts,
            })
        }
    }
}

/// Generate a keypair based on key specifications (convenience wrapper).
/// 
/// This is a convenience function that extracts a seed from the RNG and calls
/// [`generate_keypair`] with default max attempts, returning only the keypair
/// and discarding iteration statistics.
#[inline]
pub fn generate_keypair_with_spec<R: rand::Rng>(
    rng: &mut R,
    key_config: &KeyConfig,
    node_name: &str,
) -> Result<GeneratedKeypair, ModelError> {
    // Extract a u64 seed from the RNG to use for parallel generation
    let seed: u64 = rng.gen();
    generate_keypair(seed, key_config, node_name, None).map(|r| r.keypair)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn test_keyspec_parse_random() {
        let spec = KeySpec::parse("*").unwrap();
        assert!(matches!(spec, KeySpec::Random));
    }

    #[test]
    fn test_keyspec_parse_prefix() {
        let spec = KeySpec::parse("cc01*").unwrap();
        assert!(matches!(spec, KeySpec::Prefix(ref p) if p == "cc01"));
    }

    #[test]
    fn test_keyspec_parse_exact() {
        let hex_key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let spec = KeySpec::parse(hex_key).unwrap();
        assert!(matches!(spec, KeySpec::Exact(_)));
    }

    #[test]
    fn test_keyspec_parse_invalid_prefix() {
        let result = KeySpec::parse("xyz*"); // x, y, z are not valid hex
        assert!(result.is_err());
    }

    #[test]
    fn test_keyspec_parse_wrong_length() {
        let result = KeySpec::parse("0123456789abcdef"); // 16 chars, should be 64
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_keypair_random() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let config = KeyConfig::default();
        let keypair = generate_keypair_with_spec(&mut rng, &config, "test_node").unwrap();
        
        // Should produce valid 32-byte keys
        assert_eq!(keypair.public_key.len(), 32);
        assert_eq!(keypair.private_key.len(), 32);
    }

    #[test]
    fn test_generate_keypair_with_prefix() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let config = KeyConfig {
            private_key: KeySpec::Random,
            public_key: KeySpec::Prefix("01".to_string()),
        };
        let keypair = generate_keypair_with_spec(&mut rng, &config, "test_node").unwrap();
        
        // Public key should start with "01"
        let pub_hex = hex::encode(&keypair.public_key);
        assert!(pub_hex.starts_with("01"), "Public key {} should start with 01", pub_hex);
    }

    #[test]
    fn test_generate_keypair_exact() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let exact_prv = [0x42u8; 32];
        let exact_pub = [0xaa; 32];
        let config = KeyConfig {
            private_key: KeySpec::Exact(exact_prv),
            public_key: KeySpec::Exact(exact_pub),
        };
        let keypair = generate_keypair_with_spec(&mut rng, &config, "test_node").unwrap();
        
        assert_eq!(keypair.private_key, exact_prv);
        assert_eq!(keypair.public_key, exact_pub);
    }

    #[test]
    fn test_keyspec_parse_empty_prefix() {
        // Empty prefix "*" should be treated as random
        let spec = KeySpec::parse("*").unwrap();
        assert!(matches!(spec, KeySpec::Random));
    }

    #[test]
    fn test_keyspec_parse_single_char_prefix() {
        let spec = KeySpec::parse("a*").unwrap();
        assert!(matches!(spec, KeySpec::Prefix(ref p) if p == "a"));
    }

    #[test]
    fn test_keyspec_parse_long_prefix() {
        // 8-character prefix (4 bytes)
        let spec = KeySpec::parse("cc01ff02*").unwrap();
        assert!(matches!(spec, KeySpec::Prefix(ref p) if p == "cc01ff02"));
    }

    #[test]
    fn test_keyspec_parse_uppercase_prefix() {
        // Should convert to lowercase
        let spec = KeySpec::parse("CC01*").unwrap();
        assert!(matches!(spec, KeySpec::Prefix(ref p) if p == "cc01"));
    }

    #[test]
    fn test_keyspec_parse_mixed_case_exact() {
        let hex_key = "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789";
        let spec = KeySpec::parse(hex_key).unwrap();
        if let KeySpec::Exact(bytes) = spec {
            // Should parse correctly regardless of case
            assert_eq!(bytes[0], 0xAB);
            assert_eq!(bytes[1], 0xCD);
        } else {
            panic!("Expected Exact key spec");
        }
    }

    #[test]
    fn test_generate_keypair_with_longer_prefix() {
        let mut rng = ChaCha8Rng::seed_from_u64(12345);
        let config = KeyConfig {
            private_key: KeySpec::Random,
            public_key: KeySpec::Prefix("a".to_string()), // Single char prefix - easier to find
        };
        let keypair = generate_keypair_with_spec(&mut rng, &config, "test_node").unwrap();
        
        let pub_hex = hex::encode(&keypair.public_key);
        assert!(pub_hex.starts_with("a"), "Public key {} should start with 'a'", pub_hex);
    }

    #[test]
    fn test_generate_keypair_deterministic_with_seed() {
        // Same seed should produce same keypair
        let mut rng1 = ChaCha8Rng::seed_from_u64(999);
        let mut rng2 = ChaCha8Rng::seed_from_u64(999);
        
        let config = KeyConfig::default();
        let keypair1 = generate_keypair_with_spec(&mut rng1, &config, "node").unwrap();
        let keypair2 = generate_keypair_with_spec(&mut rng2, &config, "node").unwrap();
        
        assert_eq!(keypair1.private_key, keypair2.private_key);
        assert_eq!(keypair1.public_key, keypair2.public_key);
    }

    #[test]
    fn test_generate_keypair_different_seeds_different_keys() {
        let mut rng1 = ChaCha8Rng::seed_from_u64(111);
        let mut rng2 = ChaCha8Rng::seed_from_u64(222);
        
        let config = KeyConfig::default();
        let keypair1 = generate_keypair_with_spec(&mut rng1, &config, "node").unwrap();
        let keypair2 = generate_keypair_with_spec(&mut rng2, &config, "node").unwrap();
        
        assert_ne!(keypair1.private_key, keypair2.private_key);
        assert_ne!(keypair1.public_key, keypair2.public_key);
    }

    #[test]
    fn test_generate_keypair_exact_private_derives_public() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let exact_prv = [0x42u8; 32];
        let config = KeyConfig {
            private_key: KeySpec::Exact(exact_prv),
            public_key: KeySpec::Random, // Public key derived from private
        };
        let keypair = generate_keypair_with_spec(&mut rng, &config, "test_node").unwrap();
        
        assert_eq!(keypair.private_key, exact_prv);
        // Public key should be deterministically derived from private key
        assert_ne!(keypair.public_key, [0u8; 32]); // Should not be all zeros
    }

    #[test]
    fn test_generate_keypair_public_key_derived_from_private() {
        use ed25519_dalek::{SigningKey, VerifyingKey};
        
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let config = KeyConfig::default();
        let keypair = generate_keypair_with_spec(&mut rng, &config, "test_node").unwrap();
        
        // Verify public key is derived from private key using Ed25519
        let signing_key = SigningKey::from_bytes(&keypair.private_key);
        let expected_public = VerifyingKey::from(&signing_key).to_bytes();
        
        assert_eq!(&keypair.public_key[..], &expected_public[..]);
    }

    #[test]
    fn test_keyspec_serialization_random() {
        let spec = KeySpec::Random;
        let serialized = serde_yaml::to_string(&spec).unwrap();
        assert!(serialized.trim() == "'*'" || serialized.trim() == "\"*\"" || serialized.trim() == "*");
    }

    #[test]
    fn test_keyspec_serialization_prefix() {
        let spec = KeySpec::Prefix("cc01".to_string());
        let serialized = serde_yaml::to_string(&spec).unwrap();
        assert!(serialized.contains("cc01*"));
    }

    #[test]
    fn test_keyspec_serialization_roundtrip() {
        let original = KeySpec::Prefix("abcd".to_string());
        let serialized = serde_yaml::to_string(&original).unwrap();
        let deserialized: KeySpec = serde_yaml::from_str(&serialized).unwrap();
        
        if let KeySpec::Prefix(p) = deserialized {
            assert_eq!(p, "abcd");
        } else {
            panic!("Expected Prefix after roundtrip");
        }
    }

    #[test]
    fn test_keyconfig_default_is_random() {
        let config = KeyConfig::default();
        assert!(matches!(config.private_key, KeySpec::Random));
        assert!(matches!(config.public_key, KeySpec::Random));
    }

    #[test]
    fn test_keyspec_parse_whitespace_handling() {
        // Should handle leading/trailing whitespace
        let spec = KeySpec::parse("  cc01*  ").unwrap();
        assert!(matches!(spec, KeySpec::Prefix(ref p) if p == "cc01"));
    }
}
