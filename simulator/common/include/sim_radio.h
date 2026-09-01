#pragma once

#include <Dispatcher.h>
// Windowed channel-health stand-ins exist only on channel-stats firmware branches
// (WindowedPercent.h + the Radio virtuals are that feature's, unmerged in dev). Guard so
// the sim compiles against any MeshCore checkout (e.g. feature/flood-corridor = plain dev).
#if __has_include(<helpers/WindowedPercent.h>)
#include <helpers/WindowedPercent.h>
#define SIM_HAS_CHANSTATS 1
#endif
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

struct RxPacket
{
    uint8_t data[256];
    size_t len;
    float rssi;
    float snr;
};

class SimRadio : public mesh::Radio
{
public:
    SimRadio();

    // Configuration
    void configure(float freq, float bw, uint8_t sf, uint8_t cr, uint8_t tx_power);
    void setParams(float freq, float bw, uint8_t sf, uint8_t cr) { configure(freq, bw, sf, cr, tx_power_); }
    void setTxPower(int8_t tx_power) { tx_power_ = static_cast<uint8_t>(tx_power); }

    // mesh::Radio interface
    void begin() override;
    int recvRaw(uint8_t *bytes, int sz) override;
    uint32_t getEstAirtimeFor(int len_bytes) override;
    float packetScore(float snr, int packet_len) override;
    bool startSendRaw(const uint8_t *bytes, int len) override;
    bool isSendComplete() override;
    void onSendFinished() override;
    bool isInRecvMode() const override;
    bool isReceiving() override;
    float getLastRSSI() const override;
    float getLastSNR() const override;
    int getNoiseFloor() const override;
    void loop() override;
#ifdef SIM_HAS_CHANSTATS
    // NOTE: no `override` — like isChannelNoisy() below, the base mesh::Radio only declares
    // these virtuals on channel-stats firmware branches; on others `override` would not
    // compile. Virtuality is inherited where the base provides it, so omitting `override`
    // works on both.
    bool hasChannelHealth() { return true; }
    uint8_t getChannelUtilizationPct() { return busy_win_.pct(); }
    uint8_t getRxDeafnessPct() { return deaf_win_.pct(); }
    void getRxQualityCounts(uint16_t& good, uint16_t& total) {
        uint16_t ev, bad;
        err_win_.counts(ev, bad);
        total = ev;      // all reception attempts
        good = ev - bad; // ...of which decoded OK
    }
    bool getRxQualityPct(uint8_t& pct) {
        uint16_t ev, bad;
        err_win_.counts(ev, bad);
        if (ev == 0) { pct = 0; return false; }  // nothing observed yet: no verdict
        pct = static_cast<uint8_t>(((ev - bad) * 100u) / ev);
        return true;
    }
#endif

    // Non-invasive LBT helpers for the swarm-relay channel-busy check (MyMesh::isResendChannelActive).
    // Sim has no live-RSSI/CAD concept: treat "packets queued" as mid-receive, and the last injected
    // packet's RSSI as the live channel energy (noise floor when idle).
    bool isReceivingPacket() { return isReceiving(); }
    float getCurrentRSSI() { return isReceiving() ? getLastRSSI() : (float)getNoiseFloor(); }

    // Quiet-dwell gate energy probe (Dispatcher samples this every DWELL_SAMPLE_INTERVAL while in RX).
    // Sim has no continuous RSSI, so latch "noisy" on each STRONG injected packet (a neighbor or
    // interferer went on-air) and let the dwell probe consume the latch one-shot: each strong
    // injection then triggers exactly one dwell deferral window via the Dispatcher's
    // last_channel_noisy_ms. This stands in for hardware's live-RSSI-above-margin detection and
    // makes the dwell gate (and the path-staggered release) exercisable in sim. Sim-only behavior.
    //
    // NOTE: no `override` — the base mesh::Radio only declares isChannelNoisy() virtual on
    // quiet-dwell-based firmware branches; on others (e.g. flood-suppression-coverage) it is
    // absent and `override` would not compile. Virtuality is inherited where the base provides it,
    // so omitting `override` works on both.
    bool isChannelNoisy();

    // Hardware-specific stubs (no-op in simulation)
    // Returns bool to match the firmware's radio wrapper API (RadioLibWrappers.h),
    // so example code that does `return radio_driver.setRxBoostedGainMode(enable);`
    // compiles uniformly in sim and on real hardware.
    bool setRxBoostedGainMode(bool enabled) { rx_boosted_gain_ = enabled; return true; }
    bool getRxBoostedGainMode() const { return rx_boosted_gain_; }

    // Statistics interface (used by firmware)
    uint32_t getPacketsRecv() const { return packets_recv_; }
    uint32_t getPacketsSent() const { return packets_sent_; }
    uint32_t getPacketsRecvErrors() const { return packets_recv_errors_; }
    void resetStats()
    {
        packets_recv_ = 0;
        packets_sent_ = 0;
        packets_recv_errors_ = 0;
        total_tx_airtime_ = 0;
        total_rx_airtime_ = 0;
    }
    uint32_t getTotalTxAirtime() const { return total_tx_airtime_; }
    uint32_t getTotalRxAirtime() const { return total_rx_airtime_; }

    // Simulation interface (called by coordinator)
    void injectRxPacket(const uint8_t *data, size_t len, float rssi, float snr);
    void notifyTxComplete();
    void notifyStateChange(uint32_t state_version);

    // Check if there's a pending TX (for yield)
    bool hasPendingTx() const { return tx_pending_; }

    // Get pending TX data
    const uint8_t *getTxData() const { return tx_data_; }
    size_t getTxLen() const { return tx_len_; }
    uint32_t getTxAirtime() const; // Implemented in cpp

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
    bool rx_boosted_gain_;

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
    uint32_t packets_recv_errors_;
    uint32_t total_tx_airtime_;
    uint32_t total_rx_airtime_;

    // State tracking for spin detection (per design doc)
    uint32_t state_version_;       // Incremented on any state change
    uint32_t last_polled_version_; // Last observed state version
    int poll_count_;               // Number of polls with unchanged state

    // State
    bool recv_mode_;
    bool channel_noisy_latched_;   // sim-only: set on strong RX injection, consumed one-shot by isChannelNoisy()

#ifdef SIM_HAS_CHANSTATS
    // Windowed channel-health stand-ins (see loop() in the .cpp): sim has no continuous
    // RSSI, so utilization is approximated from windowed airtime-counter deltas and
    // deafness from wall time spent with the receiver off (== TX window incl. turnaround).
    WindowedPercent busy_win_;
    WindowedPercent deaf_win_;
    WindowedCountedRatio<> err_win_;
    uint32_t last_loop_ms_ = 0;
    uint32_t last_air_ = 0;         // previous total_tx_airtime_ + total_rx_airtime_
    uint32_t last_recv_cnt_ = 0;    // previous packets_recv_ (for deltas)
    uint32_t last_err_cnt_ = 0;     // previous packets_recv_errors_ (for deltas)
#endif
};
