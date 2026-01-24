#pragma once

#include <queue>
#include <mutex>
#include <cstdint>
#include <cstddef>

// ============================================================================
// Simulated Serial Interface
// ============================================================================
// Provides a buffer for serial RX (injected by coordinator) and TX (collected).
// The actual TCP socket is managed by the Rust coordinator.

class SimSerial {
public:
    SimSerial() : enabled_(false) {}
    
    // Enable/disable
    void enable() { enabled_ = true; }
    void disable() { enabled_ = false; }
    bool isEnabled() const { return enabled_; }
    
    // RX interface (coordinator injects data)
    void injectRx(const uint8_t* data, size_t len);
    
    // Check how many bytes are available to read
    size_t available();
    
    // Read a single byte (returns -1 if none available)
    int read();
    
    // Read multiple bytes
    size_t readBytes(uint8_t* buffer, size_t len);
    
    // TX interface (firmware writes data)
    void write(uint8_t byte);
    void write(const uint8_t* data, size_t len);
    
    // Collect TX data (coordinator retrieves and sends over TCP)
    size_t collectTx(uint8_t* buffer, size_t max_len);

private:
    bool enabled_;
    
    std::queue<uint8_t> rx_queue_;
    std::mutex rx_mutex_;
    
    std::vector<uint8_t> tx_buffer_;
    std::mutex tx_mutex_;
};
