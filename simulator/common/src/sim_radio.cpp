#include "sim_radio.h"
#include "sim_context.h"
#include <cmath>
#include <cstdio>

SimRadio::SimRadio() 
    : freq_(915.0f)
    , bw_(250.0f)
    , sf_(11)
    , cr_(5)
    , tx_power_(20)
    , last_rssi_(-100.0f)
    , last_snr_(0.0f)
    , tx_pending_(false)
    , tx_in_progress_(false)
    , tx_len_(0)
    , packets_recv_(0)
    , packets_sent_(0)
    , total_tx_airtime_(0)
    , total_rx_airtime_(0)
    , state_version_(0)
    , last_polled_version_(0)
    , poll_count_(0)
    , recv_mode_(false)
{
}

void SimRadio::configure(float freq, float bw, uint8_t sf, uint8_t cr, uint8_t tx_power) {
    freq_ = freq;
    bw_ = bw;
    sf_ = sf;
    cr_ = cr;
    tx_power_ = tx_power;
}

void SimRadio::begin() {
    recv_mode_ = true;
    tx_pending_ = false;
    tx_in_progress_ = false;
    state_version_++;  // State changed - entering receive mode
}

int SimRadio::recvRaw(uint8_t* bytes, int sz) {
    std::lock_guard<std::mutex> lock(rx_mutex_);
    
    if (rx_queue_.empty()) {
        return 0;
    }
    
    RxPacket& pkt = rx_queue_.front();
    int len = static_cast<int>(pkt.len);
    if (len > sz) len = sz;
    
    memcpy(bytes, pkt.data, len);
    last_rssi_ = pkt.rssi;
    last_snr_ = pkt.snr;
    
    rx_queue_.pop();
    
    // Update statistics
    packets_recv_++;
    total_rx_airtime_ += getEstAirtimeFor(len);
    
    return len;
}

uint32_t SimRadio::getEstAirtimeFor(int len_bytes) {
    // LoRa airtime calculation (simplified)
    // Based on Semtech SX1276 datasheet formulas
    
    float bw_hz = bw_ * 1000.0f;
    float ts = pow(2.0f, sf_) / bw_hz;  // Symbol time in seconds
    
    // Preamble: 8 symbols + 4.25
    float t_preamble = (8.0f + 4.25f) * ts;
    
    // Payload symbols (simplified calculation)
    int de = (bw_ <= 125.0f && sf_ >= 11) ? 1 : 0;  // Low data rate optimize
    int ih = 0;  // Implicit header off
    int crc = 1; // CRC on
    
    float payload_symbols = 8.0f + fmax(
        ceil((8.0f * len_bytes - 4.0f * sf_ + 28.0f + 16.0f * crc - 20.0f * ih) / 
             (4.0f * (sf_ - 2.0f * de))) * (cr_ + 4.0f),
        0.0f
    );
    
    float t_payload = payload_symbols * ts;
    
    // Total time in milliseconds
    return static_cast<uint32_t>((t_preamble + t_payload) * 1000.0f);
}

float SimRadio::packetScore(float snr, int packet_len) {
    // Score based on SNR - higher is better
    // Normalize to roughly 0-1 range
    return (snr + 20.0f) / 40.0f;
}

bool SimRadio::startSendRaw(const uint8_t* bytes, int len) {
    if (tx_in_progress_) {
        return false;
    }
    
    if (len > static_cast<int>(sizeof(tx_data_))) {
        len = sizeof(tx_data_);
    }
    
    memcpy(tx_data_, bytes, len);
    tx_len_ = len;
    tx_pending_ = true;
    tx_in_progress_ = true;
    recv_mode_ = false;
    state_version_++;  // State changed - starting TX
    
    // Update statistics
    packets_sent_++;
    total_tx_airtime_ += getEstAirtimeFor(len);
    
    // Signal to SimContext that we have a TX event
    if (g_sim_ctx) {
        g_sim_ctx->step_result.reason = SIM_YIELD_RADIO_TX_START;
        memcpy(g_sim_ctx->step_result.radio_tx_data, tx_data_, tx_len_);
        g_sim_ctx->step_result.radio_tx_len = tx_len_;
        g_sim_ctx->step_result.radio_tx_airtime_ms = getEstAirtimeFor(len);
    }
    
    return true;
}

// Check for polling spin and yield if necessary.
// This prevents firmware from deadlocking while polling radio state.
// Uses configurable threshold from SimContext for deterministic work per step.
void SimRadio::checkForSpin() {
    if (state_version_ != last_polled_version_) {
        // State changed since last poll - reset counter and yield
        last_polled_version_ = state_version_;
        poll_count_ = 0;
        // Yield to give coordinator a chance to process
        if (g_sim_ctx) {
            g_sim_ctx->step_result.reason = SIM_YIELD_IDLE;
        }
        return;
    }
    
    // State unchanged
    poll_count_++;
    
    // Get configurable threshold (default to 3 if context unavailable)
    int threshold = 3;
    if (g_sim_ctx) {
        threshold = g_sim_ctx->spin_config.threshold;
    }
    
    if (poll_count_ >= threshold) {
        // Firmware is spinning on unchanged state - yield until
        // either radio state changes or time advances
        
        // Log spin detection if enabled
        if (g_sim_ctx) {
            g_sim_ctx->spin_config.spin_detection_count++;
            
            if (g_sim_ctx->spin_config.log_spin_detection) {
                printf("[SPIN] Radio spin detected after %d polls (threshold=%d, total_spins=%u)\n",
                       poll_count_, threshold, g_sim_ctx->spin_config.spin_detection_count);
            }
            
            g_sim_ctx->step_result.reason = SIM_YIELD_IDLE;
        }
        
        poll_count_ = 0;
    }
}

bool SimRadio::isSendComplete() {
    checkForSpin();
    // TX is complete when coordinator calls notifyTxComplete()
    return !tx_in_progress_;
}

void SimRadio::onSendFinished() {
    tx_pending_ = false;
    recv_mode_ = true;
    state_version_++;  // State changed - TX finished
}

void SimRadio::notifyTxComplete() {
    tx_in_progress_ = false;
    state_version_++;  // State changed - TX complete notification
}

void SimRadio::notifyStateChange(uint32_t new_state_version) {
    // Called by coordinator when radio state changes externally
    state_version_ = new_state_version;
}

bool SimRadio::isInRecvMode() const {
    // Note: This is const so we can't call checkForSpin() here
    // The caller should use isSendComplete() for spin detection
    return recv_mode_ && !tx_in_progress_;
}

bool SimRadio::isReceiving() {
    checkForSpin();
    // In simulation, we don't have a concept of "currently receiving"
    // Return true if there are packets in the queue
    std::lock_guard<std::mutex> lock(rx_mutex_);
    return !rx_queue_.empty();
}

float SimRadio::getLastRSSI() const {
    return last_rssi_;
}

float SimRadio::getLastSNR() const {
    return last_snr_;
}

int SimRadio::getNoiseFloor() const {
    return -120;  // Typical noise floor
}

void SimRadio::injectRxPacket(const uint8_t* data, size_t len, float rssi, float snr) {
    std::lock_guard<std::mutex> lock(rx_mutex_);
    
    // Check queue depth - drop if full (simulates FIFO overflow)
    if (rx_queue_.size() >= RX_QUEUE_DEPTH) {
        // Packet dropped - queue is full
        // Optionally could report this back to coordinator for trace analysis
        return;
    }
    
    RxPacket pkt;
    if (len > sizeof(pkt.data)) {
        len = sizeof(pkt.data);
    }
    memcpy(pkt.data, data, len);
    pkt.len = len;
    pkt.rssi = rssi;
    pkt.snr = snr;
    
    rx_queue_.push(pkt);
    state_version_++;  // State changed - packet arrived
}

uint32_t SimRadio::getTxAirtime() const {
    // Use same formula as getEstAirtimeFor but as const method
    float bw_hz = bw_ * 1000.0f;
    float ts = pow(2.0f, static_cast<float>(sf_)) / bw_hz;
    float t_preamble = (8.0f + 4.25f) * ts;
    int de = (bw_ <= 125.0f && sf_ >= 11) ? 1 : 0;
    float payload_symbols = 8.0f + fmax(
        ceil((8.0f * tx_len_ - 4.0f * sf_ + 28.0f + 16.0f - 0.0f) / 
             (4.0f * (sf_ - 2.0f * de))) * (cr_ + 4.0f),
        0.0f
    );
    float t_payload = payload_symbols * ts;
    return static_cast<uint32_t>((t_preamble + t_payload) * 1000.0f);
}
