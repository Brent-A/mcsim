#pragma once

// Prevent Arduino min/max macros from interfering with std::min/max
#ifdef min
#undef min
#endif
#ifdef max
#undef max
#endif

#include "sim_api.h"
#include "sim_radio.h"
#include "sim_board.h"
#include "sim_clock.h"
#include "sim_rng.h"
#include "sim_serial.h"
#include "sim_filesystem.h"

#include <mutex>
#include <condition_variable>
#include <cstring>
#include <thread>
#include <atomic>
#include <queue>
#include <string>
#include <vector>
#include <set>
#include <cstdint>

// ============================================================================
// Spin Detection Configuration
// ============================================================================
// Configuration for deterministic spin detection during firmware loop execution.
// Spin detection prevents the firmware from busy-waiting indefinitely on
// unchanged radio state.

struct SpinDetectionConfig {
    /// Number of consecutive polls without state change before yielding.
    /// Default: 3 polls (matching original hardcoded behavior).
    int threshold = 3;

    /// Enable debug logging when spin is detected.
    /// When true, logs a message each time spin detection triggers.
    bool log_spin_detection = false;

    /// Enable debug logging for loop iterations per step.
    /// When true, logs the number of loop iterations at step end.
    bool log_loop_iterations = false;

    /// Track total spin detections for metrics/debugging.
    uint32_t spin_detection_count = 0;

    /// Track loop iterations for the current step (reset each step).
    uint32_t loop_iterations_this_step = 0;

    /// Track total loop iterations across all steps.
    uint64_t total_loop_iterations = 0;
};

// ============================================================================
// Wake Time Registry
// ============================================================================
// Tracks scheduled wake times for deterministic time advancement.
// Each node has its own registry to track when it needs to wake up.

struct WakeTimeRegistry {
    std::set<uint64_t> pending_wake_times;

    void registerWakeTime(uint64_t millis) {
        pending_wake_times.insert(millis);
    }

    uint64_t getNextWakeTime() const {
        if (pending_wake_times.empty()) {
            return UINT64_MAX;
        }
        return *pending_wake_times.begin();
    }

    void clearExpired(uint64_t current_millis) {
        auto it = pending_wake_times.begin();
        while (it != pending_wake_times.end() && *it <= current_millis) {
            it = pending_wake_times.erase(it);
        }
    }

    void clear() {
        pending_wake_times.clear();
    }
};

// ============================================================================
// Simulation Context
// ============================================================================
// Each node instance has its own SimContext, stored in thread-local storage.
// This allows the Arduino stubs and firmware code to access the correct
// instance's state without requiring code changes to the firmware.

struct SimContext {
    // Subsystems (implementations of MeshCore interfaces)
    SimRadio radio;
    SimBoard board;
    SimMillisClock millis_clock;
    SimRTCClock rtc_clock;
    SimRNG rng;
    SimSerial serial;
    SimFilesystem filesystem;

    // Wake time registry for deterministic time advancement
    WakeTimeRegistry wake_registry;

    // Spin detection configuration for deterministic work per step
    SpinDetectionConfig spin_config;

    // Current simulation time
    uint64_t current_millis;
    uint32_t current_rtc_secs;

    // Step result (accumulated during step)
    SimStepResult step_result;

    // Log buffer for Serial output
    std::string log_buffer;
    std::mutex log_mutex;

    // Serial TX buffer
    std::vector<uint8_t> serial_tx_buffer;
    std::mutex serial_tx_mutex;

    // Thread synchronization
    std::mutex step_mutex;
    std::condition_variable step_cv;

    enum class State {
        IDLE,           // Waiting for step_begin
        RUNNING,        // Processing loop
        YIELDED,        // Completed step, waiting for result retrieval
        SHUTDOWN        // Thread should exit
    };
    std::atomic<State> state{State::IDLE};

    // Initialization
    SimContext() : current_millis(0), current_rtc_secs(0) {
        memset(&step_result, 0, sizeof(step_result));
    }

    // Append to log buffer (called by Serial.print stub)
    void appendLog(const char* str, size_t len) {
        std::lock_guard<std::mutex> lock(log_mutex);
        log_buffer.append(str, len);
    }

    // Append to serial TX buffer
    void appendSerialTx(const uint8_t* data, size_t len) {
        std::lock_guard<std::mutex> lock(serial_tx_mutex);
        serial_tx_buffer.insert(serial_tx_buffer.end(), data, data + len);
    }

    // Get current serial TX buffer size (for output tracking)
    size_t getSerialTxBufferSize() const {
        // Note: This method is intended to be called from the same thread
        // that writes to the buffer (during loop execution), so no lock needed
        return serial_tx_buffer.size();
    }

    // Collect accumulated output into step_result
    void finalizeStepResult() {
        // Copy log buffer
        {
            std::lock_guard<std::mutex> lock(log_mutex);
            // Use (std::min) to prevent macro expansion
            size_t copy_len = (std::min)(log_buffer.size(), sizeof(step_result.log_output) - 1);
            memcpy(step_result.log_output, log_buffer.c_str(), copy_len);
            step_result.log_output[copy_len] = '\0';
            step_result.log_output_len = copy_len;
            log_buffer.clear();
        }

        // Copy serial TX buffer
        {
            std::lock_guard<std::mutex> lock(serial_tx_mutex);
            size_t copy_len = (std::min)(serial_tx_buffer.size(), sizeof(step_result.serial_tx_data));
            memcpy(step_result.serial_tx_data, serial_tx_buffer.data(), copy_len);
            step_result.serial_tx_len = copy_len;
            serial_tx_buffer.clear();
        }

        step_result.current_millis = current_millis;
    }
};

// Thread-local context pointer
// Set by each node's thread before running firmware code
extern thread_local SimContext* g_sim_ctx;

// Helper macros for accessing context from stubs
#define SIM_CTX() (g_sim_ctx)
#define SIM_RADIO() (g_sim_ctx->radio)
#define SIM_BOARD() (g_sim_ctx->board)
#define SIM_MILLIS() (g_sim_ctx->millis_clock)
#define SIM_RTC() (g_sim_ctx->rtc_clock)
#define SIM_RNG() (g_sim_ctx->rng)
#define SIM_SERIAL() (g_sim_ctx->serial)
#define SIM_FS() (g_sim_ctx->filesystem)

// Re-define Arduino min/max macros after using std::min/max above
// This ensures firmware code can use min() and max() as expected
#ifndef _SIM_MIN_MAX_IMPL_DEFINED
#define _SIM_MIN_MAX_IMPL_DEFINED

template<typename T, typename U>
inline auto _sim_min_impl(T a, U b) -> typename std::common_type<T, U>::type {
    using R = typename std::common_type<T, U>::type;
    return (static_cast<R>(a) < static_cast<R>(b)) ? static_cast<R>(a) : static_cast<R>(b);
}

template<typename T, typename U>
inline auto _sim_max_impl(T a, U b) -> typename std::common_type<T, U>::type {
    using R = typename std::common_type<T, U>::type;
    return (static_cast<R>(a) > static_cast<R>(b)) ? static_cast<R>(a) : static_cast<R>(b);
}

#endif // _SIM_MIN_MAX_IMPL_DEFINED

#undef min
#undef max
#define min(a, b) _sim_min_impl(a, b)
#define max(a, b) _sim_max_impl(a, b)
