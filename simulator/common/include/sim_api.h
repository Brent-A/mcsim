#pragma once

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// DLL export macro
#ifdef _WIN32
    #ifdef SIM_DLL_EXPORT
        #define SIM_API __declspec(dllexport)
    #elif defined(SIM_STATIC_LIB)
        #define SIM_API  // No decoration for static library
    #else
        #define SIM_API __declspec(dllimport)
    #endif
#else
    #define SIM_API __attribute__((visibility("default")))
#endif

// ============================================================================
// Configuration Structures
// ============================================================================

#define SIM_MAX_NODE_NAME 32
#define SIM_PUB_KEY_SIZE 32
#define SIM_PRV_KEY_SIZE 64

typedef struct {
    // Identity (Ed25519 keypair)
    uint8_t public_key[SIM_PUB_KEY_SIZE];
    uint8_t private_key[SIM_PRV_KEY_SIZE];
    
    // Radio configuration
    float lora_freq;          // Frequency in MHz (e.g., 915.0)
    float lora_bw;            // Bandwidth in kHz (e.g., 250.0)
    uint8_t lora_sf;          // Spreading factor (7-12)
    uint8_t lora_cr;          // Coding rate (5-8)
    uint8_t lora_tx_power;    // TX power in dBm
    
    // Timing
    uint64_t initial_millis;  // Initial millisecond clock value
    uint32_t initial_rtc;     // Initial RTC time (Unix timestamp)
    
    // RNG
    uint32_t rng_seed;        // Seed for deterministic RNG
    
    // Node identification
    char node_name[SIM_MAX_NODE_NAME];
    
    // Spin detection configuration
    uint32_t spin_detection_threshold;   // Poll count before detecting spin/yield
    uint32_t idle_loops_before_yield;    // Idle loop count before yield
    uint8_t log_spin_detection;          // Enable debug logging for spin detection (bool as u8)
    uint8_t log_loop_iterations;         // Enable debug logging for loop iterations (bool as u8)
    uint8_t _padding[2];                 // Alignment padding
    
    // Reserved for future use
    uint8_t _reserved[56];               // Reduced from 64 to account for new fields
} SimNodeConfig;

// ============================================================================
// Yield/Step Results
// ============================================================================

typedef enum {
    SIM_YIELD_IDLE = 0,           // Node is idle, waiting for wake_millis
    SIM_YIELD_RADIO_TX_START,     // Node started radio transmission
    SIM_YIELD_RADIO_TX_COMPLETE,  // Radio transmission completed
    SIM_YIELD_REBOOT,             // Node requested reboot
    SIM_YIELD_POWER_OFF,          // Node requested power off
    SIM_YIELD_ERROR,              // An error occurred
} SimYieldReason;

#define SIM_MAX_RADIO_PACKET 256
#define SIM_MAX_SERIAL_TX 32768   // 32KB to handle large contact list responses (~151 bytes per contact)
#define SIM_MAX_LOG_OUTPUT 4096

typedef struct {
    SimYieldReason reason;
    
    // Timing
    uint64_t current_millis;      // Sim time when step completed
    uint64_t wake_millis;         // Next wake time requested by node
    
    // Radio TX data (valid when reason == SIM_YIELD_RADIO_TX_START)
    uint8_t radio_tx_data[SIM_MAX_RADIO_PACKET];
    size_t radio_tx_len;
    uint32_t radio_tx_airtime_ms; // Estimated airtime in milliseconds
    
    // Serial TX output (accumulated during step)
    uint8_t serial_tx_data[SIM_MAX_SERIAL_TX];
    size_t serial_tx_len;
    
    // Log output from Serial.print() calls
    char log_output[SIM_MAX_LOG_OUTPUT];
    size_t log_output_len;
    
    // Error message (if reason == SIM_YIELD_ERROR)
    char error_msg[256];
} SimStepResult;

// ============================================================================
// Opaque Node Handle
// ============================================================================

typedef struct SimNodeImpl* SimNodeHandle;

// ============================================================================
// Lifecycle API
// ============================================================================

// Create a new node instance with the given configuration.
// The node thread is started but waits for sim_step_begin().
SIM_API SimNodeHandle sim_create(const SimNodeConfig* config);

// Destroy the node and free all resources.
// The node thread is terminated.
SIM_API void sim_destroy(SimNodeHandle node);

// Reboot the node (preserves filesystem state, resets everything else).
// Can only be called when node is in yielded state.
SIM_API void sim_reboot(SimNodeHandle node, const SimNodeConfig* config);

// ============================================================================
// Async Step API
// ============================================================================

// Begin a simulation step. The node thread wakes up and processes.
// This function returns immediately (non-blocking).
// Call sim_step_wait() to wait for completion.
SIM_API void sim_step_begin(SimNodeHandle node, uint64_t sim_millis, uint32_t sim_rtc_secs);

// Wait for the current step to complete.
// Blocks until the node yields.
// Returns the step result with yield reason, timing, and any output data.
SIM_API SimStepResult sim_step_wait(SimNodeHandle node);

// Combined step (blocking): equivalent to sim_step_begin() + sim_step_wait()
SIM_API SimStepResult sim_step(SimNodeHandle node, uint64_t sim_millis, uint32_t sim_rtc_secs);

// ============================================================================
// Event Injection API (call before sim_step_begin)
// ============================================================================

// Inject a received radio packet into the node's RX queue.
SIM_API void sim_inject_radio_rx(SimNodeHandle node, 
                                  const uint8_t* data, size_t len,
                                  float rssi, float snr);

// Inject received serial data (as if received from TCP bridge).
SIM_API void sim_inject_serial_rx(SimNodeHandle node,
                                   const uint8_t* data, size_t len);

// Inject a serial frame (for frame-based interfaces like companion's BaseSerialInterface)
SIM_API void sim_inject_serial_frame(SimNodeHandle node,
                                      const uint8_t* data, size_t len);

// Collect a transmitted serial frame (for frame-based interfaces)
// Returns the frame length, or 0 if no frames pending
SIM_API size_t sim_collect_serial_frame(SimNodeHandle node,
                                         uint8_t* buffer, size_t max_len);

// Notify that a previous TX completed (coordinator confirms propagation done)
SIM_API void sim_notify_tx_complete(SimNodeHandle node);

// Notify the radio of a state change (coordinator-driven state version update).
// This is used to synchronize radio state between the coordinator and the DLL.
SIM_API void sim_notify_state_change(SimNodeHandle node, uint32_t state_version);

// ============================================================================
// Query API
// ============================================================================

// Get the node type string (e.g., "repeater", "room_server", "companion")
SIM_API const char* sim_get_node_type(void);

// Get the public key of the node (after creation)
SIM_API void sim_get_public_key(SimNodeHandle node, uint8_t* out_key);

// ============================================================================
// Filesystem API (for coordinator to pre-populate or inspect)
// ============================================================================

// Write a file to the node's in-memory filesystem.
// Can be called before sim_create() to pre-populate, or while node is yielded.
SIM_API int sim_fs_write(SimNodeHandle node, const char* path, 
                          const uint8_t* data, size_t len);

// Read a file from the node's in-memory filesystem.
// Returns bytes read, or -1 on error.
SIM_API int sim_fs_read(SimNodeHandle node, const char* path,
                         uint8_t* data, size_t max_len);

// Check if a file exists.
SIM_API int sim_fs_exists(SimNodeHandle node, const char* path);

// Delete a file.
SIM_API int sim_fs_remove(SimNodeHandle node, const char* path);

#ifdef __cplusplus
}
#endif
