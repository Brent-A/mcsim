// ============================================================================
// Simulator Node Base - Common utilities for node implementations
// ============================================================================
#ifndef SIM_NODE_BASE_H
#define SIM_NODE_BASE_H

#include "sim_context.h"
#include "sim_api.h"
#include "sim_radio.h"
#include "sim_board.h"
#include "sim_clock.h"

#include <thread>
#include <chrono>
#include <cstdio>

// ============================================================================
// Thread-local global instances (defined in each DLL's sim_main.cpp)
// ============================================================================
// These are the instances that firmware code uses via the macros in sim_prefix.h
extern thread_local SimBoard _sim_board_instance;
extern thread_local SimRadio _sim_radio_instance;
extern thread_local SimRTCClock _sim_rtc_instance;

// ============================================================================
// Helper Macros
// ============================================================================

// Use this to declare extern "C" API functions with proper export
#define SIM_EXPORT extern "C" SIM_API

// ============================================================================
// Node Configuration Helpers
// ============================================================================

inline void sim_config_set_identity(SimNodeConfig* config, 
                                    const uint8_t* prv_key, 
                                    const uint8_t* pub_key) {
    if (prv_key) memcpy(config->private_key, prv_key, SIM_PRV_KEY_SIZE);
    if (pub_key) memcpy(config->public_key, pub_key, SIM_PUB_KEY_SIZE);
}

inline void sim_config_set_lora(SimNodeConfig* config,
                                float freq, float bw, int sf, int cr, int tx_power) {
    config->lora_freq = freq;
    config->lora_bw = bw;
    config->lora_sf = sf;
    config->lora_cr = cr;
    config->lora_tx_power = tx_power;
}

// ============================================================================
// SimNodeImpl - Base implementation for all node types
// ============================================================================

struct SimNodeImpl {
    SimContext ctx;
    std::thread node_thread;
    
    // Configuration
    SimNodeConfig config;
    
    // Pointers to the thread-local instances used by this node's firmware thread.
    // These are set in threadMain() and used by coordinator API calls.
    // This allows cross-thread communication (coordinator injecting packets, etc.)
    SimRadio* radio_ptr = nullptr;
    SimBoard* board_ptr = nullptr;
    SimRTCClock* rtc_ptr = nullptr;
    
    // Virtual methods for node-specific behavior (implemented in each DLL)
    virtual void setup() = 0;
    virtual void loop() = 0;
    virtual const char* getNodeType() const = 0;
    
    SimNodeImpl() = default;
    virtual ~SimNodeImpl() = default;
    
    // Thread entry point
    void threadMain() {
        // Set thread-local context
        g_sim_ctx = &ctx;
        
        // Initialize thread-local global subsystems (used by firmware via macros)
        // These are the instances that the firmware sees as board, radio_driver, etc.
        _sim_radio_instance.configure(config.lora_freq, config.lora_bw, 
                                       config.lora_sf, config.lora_cr, config.lora_tx_power);
        _sim_radio_instance.begin();
        _sim_board_instance.init();
        _sim_rtc_instance.setCurrentTime(config.initial_rtc);
        
        // Store pointers to the thread-local instances for cross-thread access
        // (coordinator thread calling sim_inject_radio_rx, etc.)
        radio_ptr = &_sim_radio_instance;
        board_ptr = &_sim_board_instance;
        rtc_ptr = &_sim_rtc_instance;
        
        // Initialize SimContext subsystems (used internally by the simulation framework)
        ctx.rng.seed(config.rng_seed);
        ctx.millis_clock.setMillis(config.initial_millis);
        ctx.rtc_clock.setCurrentTime(config.initial_rtc);
        ctx.filesystem.begin();
        
        // Run setup
        setup();
        
        // Main loop
        while (ctx.state.load() != SimContext::State::SHUTDOWN) {
            // Wait for step_begin signal
            {
                std::unique_lock<std::mutex> lock(ctx.step_mutex);
                ctx.step_cv.wait(lock, [this] {
                    auto state = ctx.state.load();
                    return state == SimContext::State::RUNNING ||
                           state == SimContext::State::SHUTDOWN;
                });
                
                if (ctx.state.load() == SimContext::State::SHUTDOWN) {
                    break;
                }
            }
            
            // Clear step result
            memset(&ctx.step_result, 0, sizeof(ctx.step_result));
            ctx.step_result.reason = SIM_YIELD_IDLE;
            
            // Reset per-step loop iteration counter
            ctx.spin_config.loop_iterations_this_step = 0;
            
            // Double-loop idle detection:
            // Run the loop until we get two consecutive iterations without output,
            // or until a TX/reboot/power-off condition is triggered.
            // This ensures the firmware has fully processed available input before yielding.
            int loops_without_output = 0;
            while (loops_without_output < 2) {
                // Track output state before loop iteration
                size_t serial_tx_before = ctx.getSerialTxBufferSize();
                bool had_pending_tx_before = _sim_radio_instance.hasPendingTx();
                
                // Run one loop iteration
                loop();
                
                // Track loop iteration counts for determinism verification
                ctx.spin_config.loop_iterations_this_step++;
                ctx.spin_config.total_loop_iterations++;
                
                // Check for immediate yield conditions (TX, reboot, power-off)
                if (_sim_radio_instance.hasPendingTx() && !had_pending_tx_before) {
                    // TX started - yield immediately for radio handling
                    break;
                }
                
                if (_sim_board_instance.wasRebootRequested()) {
                    ctx.step_result.reason = SIM_YIELD_REBOOT;
                    break;
                }
                
                if (_sim_board_instance.wasPowerOffRequested()) {
                    ctx.step_result.reason = SIM_YIELD_POWER_OFF;
                    break;
                }
                
                // Check if any output was produced during this loop iteration
                bool had_serial_output = ctx.getSerialTxBufferSize() > serial_tx_before;
                bool had_radio_tx = _sim_radio_instance.hasPendingTx();
                bool had_output = had_serial_output || had_radio_tx;
                
                if (had_output) {
                    // Output produced - reset idle counter
                    loops_without_output = 0;
                    if (had_radio_tx) {
                        // TX needs immediate handling
                        break;
                    }
                } else {
                    // No output - increment idle counter
                    loops_without_output++;
                }
            }
            
            // Log loop iterations if enabled (for determinism debugging)
            if (ctx.spin_config.log_loop_iterations) {
                printf("[LOOP] Step completed: %u iterations this step, %llu total\n",
                       ctx.spin_config.loop_iterations_this_step,
                       (unsigned long long)ctx.spin_config.total_loop_iterations);
            }
            
            // Check for radio TX or other yield conditions
            // Note: We check the thread-local global _sim_radio_instance because
            // that's what the firmware uses (via radio_driver macro)
            if (_sim_radio_instance.hasPendingTx()) {
                // TX started - we already set step_result in startSendRaw
            } else if (_sim_board_instance.wasRebootRequested()) {
                ctx.step_result.reason = SIM_YIELD_REBOOT;
            } else if (_sim_board_instance.wasPowerOffRequested()) {
                ctx.step_result.reason = SIM_YIELD_POWER_OFF;
            } else {
                
                // Clear expired wake times
                ctx.wake_registry.clearExpired(ctx.current_millis);
                
                // Idle - use wake time registry if available, otherwise default
                ctx.step_result.reason = SIM_YIELD_IDLE;
                uint64_t next_wake = ctx.wake_registry.getNextWakeTime();
                if (next_wake != UINT64_MAX) {
                    ctx.step_result.wake_millis = next_wake;
                } else {
                    ctx.step_result.wake_millis = ctx.current_millis + 100; // Default: wake in 100ms
                }
            }
            
            // Finalize step result (copy logs, serial TX, etc.)
            ctx.finalizeStepResult();
            
            // Signal step complete
            {
                std::lock_guard<std::mutex> lock(ctx.step_mutex);
                ctx.state.store(SimContext::State::YIELDED);
            }
            ctx.step_cv.notify_all();
        }
    }
};

#endif // SIM_NODE_BASE_H
