#pragma once

#include <Dispatcher.h>
#include <queue>
#include <mutex>
#include <cstring>

// ============================================================================
// Configuration Constants
// ============================================================================

/// Maximum RX queue depth - simulates radio FIFO buffer.
/// When queue is full, new packets are dropped (overflow).
static constexpr size_t RX_QUEUE_DEPTH = 4;

// ============================================================================
// Simulated Radio
// ============================================================================
// Implements mesh::Radio interface for simulation.
// - RX packets are injected by the coordinator via sim_inject_radio_rx()
// - TX packets are captured and reported back to the coordinator
// - State changes are notified via sim_notify_state_change()

struct RxPacket {
    uint8_t data[256];
    size_t len;
    float rssi;
    float snr;
};

class SimRadio : public mesh::Radio {
public:
    SimRadio();
    
    // Configuration
    void configure(float freq, float bw, uint8_t sf, uint8_t cr, uint8_t tx_power);
    
    // mesh::Radio interface
    void begin() override;
    int recvRaw(uint8_t* bytes, int sz) override;
    uint32_t getEstAirtimeFor(int len_bytes) override;
    float packetScore(float snr, int packet_len) override;
    bool startSendRaw(const uint8_t* bytes, int len) override;
    bool isSendComplete() override;
    void onSendFinished() override;
    bool isInRecvMode() const override;
    bool isReceiving() override;
    float getLastRSSI() const override;
    float getLastSNR() const override;
    int getNoiseFloor() const override;
    
    // Statistics interface (used by firmware)
    uint32_t getPacketsRecv() const { return packets_recv_; }
    uint32_t getPacketsSent() const { return packets_sent_; }
    void resetStats() { packets_recv_ = 0; packets_sent_ = 0; total_tx_airtime_ = 0; total_rx_airtime_ = 0; }
    uint32_t getTotalTxAirtime() const { return total_tx_airtime_; }
    uint32_t getTotalRxAirtime() const { return total_rx_airtime_; }
    
    // Simulation interface (called by coordinator)
    void injectRxPacket(const uint8_t* data, size_t len, float rssi, float snr);
    void notifyTxComplete();
    void notifyStateChange(uint32_t state_version);
    
    // Check if there's a pending TX (for yield)
    bool hasPendingTx() const { return tx_pending_; }
    
    // Get pending TX data
    const uint8_t* getTxData() const { return tx_data_; }
    size_t getTxLen() const { return tx_len_; }
    uint32_t getTxAirtime() const;  // Implemented in cpp
    
    // Clear pending TX (after coordinator retrieves it)
    void clearPendingTx() { tx_pending_ = false; }

private:
    // Check for polling spin and yield if necessary
    void checkForSpin();
    
    // Configuration
    float freq_;
    float bw_;
    uint8_t sf_;
    uint8_t cr_;
    uint8_t tx_power_;
    
    // RX queue (packets injected by coordinator)
    std::queue<RxPacket> rx_queue_;
    std::mutex rx_mutex_;
    
    // Last received packet stats
    float last_rssi_;
    float last_snr_;
    
    // TX state
    bool tx_pending_;
    bool tx_in_progress_;
    uint8_t tx_data_[256];
    size_t tx_len_;
    
    // Statistics
    uint32_t packets_recv_;
    uint32_t packets_sent_;
    uint32_t total_tx_airtime_;
    uint32_t total_rx_airtime_;
    
    // State tracking for spin detection (per design doc)
    uint32_t state_version_;          // Incremented on any state change
    uint32_t last_polled_version_;    // Last observed state version
    int poll_count_;                  // Number of polls with unchanged state
    
    // State
    bool recv_mode_;
};
