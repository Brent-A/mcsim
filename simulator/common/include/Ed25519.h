#pragma once

// ============================================================================
// Ed25519 Stub for Simulation
// ============================================================================
// This provides the Ed25519 class API used by MeshCore.
// The actual crypto is provided by lib/ed25519/ed_25519.h

#include <cstdint>
#include <cstddef>

// Include the actual ed25519 implementation
extern "C" {
#include "ed_25519.h"
}

class Ed25519 {
public:
    // Verify a signature
    // sig: 64-byte signature
    // publicKey: 32-byte public key
    // message: message that was signed
    // len: length of message
    static bool verify(const uint8_t* sig, const uint8_t* publicKey, 
                       const void* message, size_t len) {
        return ed25519_verify(sig, static_cast<const uint8_t*>(message), 
                             static_cast<int>(len), publicKey) != 0;
    }
    
    // Sign a message
    // sig: output 64-byte signature
    // privateKey: 64-byte private key
    // publicKey: 32-byte public key
    // message: message to sign
    // len: length of message
    static void sign(uint8_t* sig, const uint8_t* privateKey, 
                    const uint8_t* publicKey, const void* message, size_t len) {
        ed25519_sign(sig, static_cast<const uint8_t*>(message), 
                    static_cast<int>(len), publicKey, privateKey);
    }
    
    // Generate a key pair from seed
    // publicKey: output 32-byte public key
    // privateKey: output 64-byte private key
    // seed: 32-byte random seed
    static void generatePrivateKey(uint8_t* privateKey, const uint8_t* seed) {
        uint8_t publicKey[32];
        ed25519_create_keypair(publicKey, privateKey, seed);
    }
    
    // Derive public key from private key
    static void derivePublicKey(uint8_t* publicKey, const uint8_t* privateKey) {
        ed25519_derive_pub(publicKey, privateKey);
    }
};
