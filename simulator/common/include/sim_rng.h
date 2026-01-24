#pragma once

#include <Mesh.h>
#include <cstdint>

// ============================================================================
// Simulated RNG
// ============================================================================
// Implements mesh::RNG interface with deterministic seeding.
// Uses a simple xorshift algorithm for reproducibility.

class SimRNG : public mesh::RNG {
public:
    SimRNG() : state_(1) {}
    
    void seed(uint32_t seed) {
        state_ = seed ? seed : 1;  // Ensure non-zero state
    }
    
    void random(uint8_t* dest, size_t sz) override {
        for (size_t i = 0; i < sz; i++) {
            dest[i] = static_cast<uint8_t>(next() & 0xFF);
        }
    }
    
    uint32_t next() {
        // xorshift32
        uint32_t x = state_;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        state_ = x;
        return x;
    }

private:
    uint32_t state_;
};
