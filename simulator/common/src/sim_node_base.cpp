#include "../include/sim_node_base.h"
#include "sim_context.h"
#include "sim_api.h"

#include <thread>
#include <chrono>

// SimNodeImpl is defined in sim_node_base.h

// ============================================================================
// Common API Implementation (used by all DLLs)
// ============================================================================

// Note: sim_create and sim_destroy are implemented in each node's sim_main.cpp
// because they need to instantiate the node-specific SimNodeImpl subclass.

extern "C" {

SIM_API void sim_step_begin(SimNodeHandle node, uint64_t sim_millis, uint32_t sim_rtc_secs) {
    if (!node) return;
    
    // Update time
    node->ctx.current_millis = sim_millis;
    node->ctx.current_rtc_secs = sim_rtc_secs;
    node->ctx.millis_clock.setMillis(sim_millis);
    node->ctx.rtc_clock.setCurrentTime(sim_rtc_secs);
    
    // Clear board flags (use pointer to firmware thread's board instance)
    if (node->board_ptr) {
        node->board_ptr->clearRebootRequest();
        node->board_ptr->clearPowerOffRequest();
    }
    
    // Signal node thread to run
    {
        std::lock_guard<std::mutex> lock(node->ctx.step_mutex);
        node->ctx.state.store(SimContext::State::RUNNING);
    }
    node->ctx.step_cv.notify_all();
}

SIM_API SimStepResult sim_step_wait(SimNodeHandle node) {
    SimStepResult result = {};
    if (!node) {
        result.reason = SIM_YIELD_ERROR;
        snprintf(result.error_msg, sizeof(result.error_msg), "Invalid node handle");
        return result;
    }
    
    // Wait for node to yield
    {
        std::unique_lock<std::mutex> lock(node->ctx.step_mutex);
        node->ctx.step_cv.wait(lock, [node] {
            return node->ctx.state.load() == SimContext::State::YIELDED ||
                   node->ctx.state.load() == SimContext::State::SHUTDOWN;
        });
    }
    
    // Copy result
    result = node->ctx.step_result;
    
    // Reset state to idle for next step
    {
        std::lock_guard<std::mutex> lock(node->ctx.step_mutex);
        node->ctx.state.store(SimContext::State::IDLE);
    }
    
    return result;
}

SIM_API SimStepResult sim_step(SimNodeHandle node, uint64_t sim_millis, uint32_t sim_rtc_secs) {
    sim_step_begin(node, sim_millis, sim_rtc_secs);
    return sim_step_wait(node);
}

SIM_API void sim_inject_radio_rx(SimNodeHandle node, 
                                  const uint8_t* data, size_t len,
                                  float rssi, float snr) {
    if (!node || !node->radio_ptr) return;
    // Use the pointer to the firmware thread's radio instance
    node->radio_ptr->injectRxPacket(data, len, rssi, snr);
}

SIM_API void sim_inject_serial_rx(SimNodeHandle node,
                                   const uint8_t* data, size_t len) {
    if (!node) return;
    node->ctx.serial.injectRx(data, len);
}

SIM_API void sim_notify_tx_complete(SimNodeHandle node) {
    if (!node || !node->radio_ptr) return;
    // Use the pointer to the firmware thread's radio instance
    node->radio_ptr->notifyTxComplete();
}

SIM_API void sim_notify_state_change(SimNodeHandle node, uint32_t state_version) {
    if (!node || !node->radio_ptr) return;
    // Use the pointer to the firmware thread's radio instance
    node->radio_ptr->notifyStateChange(state_version);
}

SIM_API void sim_get_public_key(SimNodeHandle node, uint8_t* out_key) {
    if (!node || !out_key) return;
    memcpy(out_key, node->config.public_key, SIM_PUB_KEY_SIZE);
}

SIM_API int sim_fs_write(SimNodeHandle node, const char* path, 
                          const uint8_t* data, size_t len) {
    if (!node) return -1;
    return node->ctx.filesystem.writeFile(path, data, len);
}

SIM_API int sim_fs_read(SimNodeHandle node, const char* path,
                         uint8_t* data, size_t max_len) {
    if (!node) return -1;
    return node->ctx.filesystem.readFile(path, data, max_len);
}

SIM_API int sim_fs_exists(SimNodeHandle node, const char* path) {
    if (!node) return 0;
    return node->ctx.filesystem.exists(path) ? 1 : 0;
}

SIM_API int sim_fs_remove(SimNodeHandle node, const char* path) {
    if (!node) return 0;
    return node->ctx.filesystem.remove(path) ? 1 : 0;
}

} // extern "C"
