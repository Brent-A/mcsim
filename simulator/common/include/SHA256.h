#pragma once

// ============================================================================
// SHA256 Stub for Simulation
// ============================================================================
// Provides a minimal SHA256 implementation for packet hashing.

#include <cstdint>
#include <cstddef>
#include <cstring>

class SHA256 {
public:
    static const size_t HASH_SIZE = 32;
    static const size_t BLOCK_SIZE = 64;

    SHA256() {
        reset();
        memset(hmac_key_, 0, sizeof(hmac_key_));
        hmac_mode_ = false;
    }

    void reset() {
        // Initialize hash values (first 32 bits of fractional parts of square roots of first 8 primes)
        state_[0] = 0x6a09e667;
        state_[1] = 0xbb67ae85;
        state_[2] = 0x3c6ef372;
        state_[3] = 0xa54ff53a;
        state_[4] = 0x510e527f;
        state_[5] = 0x9b05688c;
        state_[6] = 0x1f83d9ab;
        state_[7] = 0x5be0cd19;
        count_ = 0;
        buffer_len_ = 0;
    }

    // HMAC mode: reset and set key
    void resetHMAC(const void* key, size_t keyLen) {
        reset();
        hmac_mode_ = true;
        
        // If key is longer than block size, hash it
        if (keyLen > BLOCK_SIZE) {
            SHA256 keyHash;
            keyHash.update(key, keyLen);
            keyHash.finalize(hmac_key_, HASH_SIZE);
            keyLen = HASH_SIZE;
        } else {
            memset(hmac_key_, 0, BLOCK_SIZE);
            memcpy(hmac_key_, key, keyLen);
        }
        
        // XOR key with ipad (0x36) and hash it
        uint8_t ipad[BLOCK_SIZE];
        for (size_t i = 0; i < BLOCK_SIZE; i++) {
            ipad[i] = hmac_key_[i] ^ 0x36;
        }
        update(ipad, BLOCK_SIZE);
    }
    
    // Finalize HMAC
    void finalizeHMAC(const void* key, size_t keyLen, void* hash, size_t hashLen) {
        (void)key;
        (void)keyLen;
        
        // Finalize inner hash
        uint8_t innerHash[HASH_SIZE];
        finalize(innerHash, HASH_SIZE);
        
        // Now compute outer hash: H(K XOR opad || innerHash)
        reset();
        
        // XOR key with opad (0x5c)
        uint8_t opad[BLOCK_SIZE];
        for (size_t i = 0; i < BLOCK_SIZE; i++) {
            opad[i] = hmac_key_[i] ^ 0x5c;
        }
        update(opad, BLOCK_SIZE);
        update(innerHash, HASH_SIZE);
        finalize(hash, hashLen);
        
        hmac_mode_ = false;
    }

    void update(const void* data, size_t len) {
        const uint8_t* bytes = static_cast<const uint8_t*>(data);
        
        while (len > 0) {
            size_t to_copy = BLOCK_SIZE - buffer_len_;
            if (to_copy > len) to_copy = len;
            
            memcpy(buffer_ + buffer_len_, bytes, to_copy);
            buffer_len_ += to_copy;
            bytes += to_copy;
            len -= to_copy;
            count_ += to_copy;
            
            if (buffer_len_ == BLOCK_SIZE) {
                processBlock(buffer_);
                buffer_len_ = 0;
            }
        }
    }

    void finalize(void* hash, size_t len) {
        // Save state for HMAC
        uint64_t saved_count = count_;
        
        // Pad message
        uint8_t pad[BLOCK_SIZE];
        size_t pad_len = (buffer_len_ < 56) ? (56 - buffer_len_) : (120 - buffer_len_);
        
        memset(pad, 0, pad_len);
        pad[0] = 0x80;
        update(pad, pad_len);
        
        // Append length in bits (big-endian) - use saved count
        uint64_t bits = saved_count * 8;
        uint8_t len_bytes[8];
        for (int i = 7; i >= 0; i--) {
            len_bytes[i] = bits & 0xFF;
            bits >>= 8;
        }
        update(len_bytes, 8);
        
        // Output hash (big-endian)
        uint8_t* out = static_cast<uint8_t*>(hash);
        size_t out_len = (len < HASH_SIZE) ? len : HASH_SIZE;
        for (size_t i = 0; i < out_len; i++) {
            out[i] = (state_[i / 4] >> (24 - (i % 4) * 8)) & 0xFF;
        }
    }

private:
    uint32_t state_[8];
    uint8_t buffer_[BLOCK_SIZE];
    size_t buffer_len_;
    uint64_t count_;
    uint8_t hmac_key_[BLOCK_SIZE];  // For HMAC mode
    bool hmac_mode_;

    static uint32_t rotr(uint32_t x, int n) {
        return (x >> n) | (x << (32 - n));
    }

    static uint32_t ch(uint32_t x, uint32_t y, uint32_t z) {
        return (x & y) ^ (~x & z);
    }

    static uint32_t maj(uint32_t x, uint32_t y, uint32_t z) {
        return (x & y) ^ (x & z) ^ (y & z);
    }

    static uint32_t sigma0(uint32_t x) {
        return rotr(x, 2) ^ rotr(x, 13) ^ rotr(x, 22);
    }

    static uint32_t sigma1(uint32_t x) {
        return rotr(x, 6) ^ rotr(x, 11) ^ rotr(x, 25);
    }

    static uint32_t gamma0(uint32_t x) {
        return rotr(x, 7) ^ rotr(x, 18) ^ (x >> 3);
    }

    static uint32_t gamma1(uint32_t x) {
        return rotr(x, 17) ^ rotr(x, 19) ^ (x >> 10);
    }

    void processBlock(const uint8_t* block) {
        static const uint32_t K[64] = {
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
            0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
            0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
            0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
            0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
            0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
            0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
            0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
            0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
            0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
            0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
            0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
            0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
            0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2
        };

        uint32_t W[64];
        
        // Prepare message schedule
        for (int i = 0; i < 16; i++) {
            W[i] = (block[i*4] << 24) | (block[i*4+1] << 16) | 
                   (block[i*4+2] << 8) | block[i*4+3];
        }
        for (int i = 16; i < 64; i++) {
            W[i] = gamma1(W[i-2]) + W[i-7] + gamma0(W[i-15]) + W[i-16];
        }

        // Working variables
        uint32_t a = state_[0];
        uint32_t b = state_[1];
        uint32_t c = state_[2];
        uint32_t d = state_[3];
        uint32_t e = state_[4];
        uint32_t f = state_[5];
        uint32_t g = state_[6];
        uint32_t h = state_[7];

        // Compression
        for (int i = 0; i < 64; i++) {
            uint32_t t1 = h + sigma1(e) + ch(e, f, g) + K[i] + W[i];
            uint32_t t2 = sigma0(a) + maj(a, b, c);
            h = g;
            g = f;
            f = e;
            e = d + t1;
            d = c;
            c = b;
            b = a;
            a = t1 + t2;
        }

        // Update state
        state_[0] += a;
        state_[1] += b;
        state_[2] += c;
        state_[3] += d;
        state_[4] += e;
        state_[5] += f;
        state_[6] += g;
        state_[7] += h;
    }
};
