// Companion Node Simulation Entry Point

#define SIM_DLL_EXPORT 1

#include "sim_api.h"
#include "sim_context.h"
#include "sim_radio.h"
#include "sim_board.h"
#include "sim_clock.h"
#include "sim_rng.h"
#include "sim_node_base.h"
#include "target.h"

#include <Mesh.h>
#include <helpers/SimpleMeshTables.h>
#include <helpers/StaticPoolPacketManager.h>
#include <helpers/IdentityStore.h>

// Include the companion's MyMesh and DataStore implementations
#include "MyMesh.h"
#include "DataStore.h"
#include <helpers/ArduinoSerialInterface.h>

#include <thread>
#include <memory>

// ============================================================================
// Global variables expected by firmware
// ============================================================================
// These use internal names; sim_prefix.h redirects board -> _sim_board_instance, etc.
// IMPORTANT: These must be thread_local so each simulation thread gets its own instance
thread_local SimBoard _sim_board_instance;
thread_local SimRadio _sim_radio_instance;
thread_local SimRTCClock _sim_rtc_instance;
thread_local EnvironmentSensorManager _sim_sensors_instance;

// Note: g_sim_ctx is defined in Arduino.cpp (sim_common library)

// ============================================================================
// Companion-specific SimNode implementation
// ============================================================================

struct CompanionSimNode : public SimNodeImpl {
    // Firmware objects
    SimRNG fast_rng;
    SimpleMeshTables tables;
    std::unique_ptr<DataStore> store;
    std::unique_ptr<MyMesh> mesh;
    ArduinoSerialInterface serial_interface;
    
    CompanionSimNode() : mesh(nullptr), store(nullptr) {}
    
    ~CompanionSimNode() override {
        // Shutdown the thread
        {
            std::lock_guard<std::mutex> lock(ctx.step_mutex);
            ctx.state.store(SimContext::State::SHUTDOWN);
        }
        ctx.step_cv.notify_all();
        
        if (node_thread.joinable()) {
            node_thread.join();
        }
    }
    
    void setup() override {
        // Initialize the RNG with the configured seed
        fast_rng.seed(config.rng_seed);
        
        // Create the data store (uses the SPIFFS global filesystem and RTC)
        store = std::make_unique<DataStore>(SPIFFS, _sim_rtc_instance);
        store->begin();
        
        // Create the mesh instance
        // Note: Companion's MyMesh constructor takes radio, rng, rtc, tables, store, ui=NULL
        mesh = std::make_unique<MyMesh>(
            _sim_radio_instance,
            fast_rng,
            _sim_rtc_instance,
            tables,
            *store,
            nullptr  // no UI
        );
        
        // Initialize the mesh (companion uses begin(has_display) signature)
        // NOTE: begin() will try to load identity from DataStore or generate a new one,
        // so we must set our configured identity AFTER begin() to override it.
        mesh->begin(false);  // no display
        
        // Set the identity from config (both private and public keys)
        // This MUST be done after begin() because begin() loads/creates an identity.
        // readFrom expects: prv_key (64 bytes) followed by pub_key (32 bytes)
        uint8_t identity_data[PRV_KEY_SIZE + PUB_KEY_SIZE];
        memcpy(identity_data, config.private_key, PRV_KEY_SIZE);
        memcpy(identity_data + PRV_KEY_SIZE, config.public_key, PUB_KEY_SIZE);
        mesh->self_id.readFrom(identity_data, sizeof(identity_data));
        
        // Set the node name from config (used for advertisements)
        if (config.node_name[0] != '\0') {
            NodePrefs* prefs = mesh->getNodePrefs();
            strncpy(prefs->node_name, config.node_name, sizeof(prefs->node_name) - 1);
            prefs->node_name[sizeof(prefs->node_name) - 1] = '\0';
        }
        
        // Initialize the serial interface with the simulated Serial stream
        serial_interface.begin(Serial);
        
        // Start the serial interface (must be done after begin() but before loop())
        mesh->startInterface(serial_interface);
    }
    
    void loop() override {
        if (mesh) {
            mesh->loop();
        }
        ctx.rtc_clock.tick();
    }
    
    const char* getNodeType() const override {
        return "companion";
    }
};

// ============================================================================
// C API Implementation
// ============================================================================

extern "C" {

SIM_API SimNodeHandle sim_create(const SimNodeConfig* config) {
    if (!config) return nullptr;
    
    auto* node = new CompanionSimNode();
    node->config = *config;
    
    // Apply spin detection config from SimNodeConfig
    node->ctx.spin_config.threshold = config->spin_detection_threshold;
    node->ctx.spin_config.log_spin_detection = config->log_spin_detection != 0;
    node->ctx.spin_config.log_loop_iterations = config->log_loop_iterations != 0;
    // Note: idle_loops_before_yield is used in sim_node_base.cpp for yield logic
    
    // Start the node thread
    node->node_thread = std::thread(&CompanionSimNode::threadMain, node);
    
    return node;
}

SIM_API void sim_destroy(SimNodeHandle node) {
    if (!node) return;
    
    auto* companion = static_cast<CompanionSimNode*>(node);
    delete companion;
}

SIM_API void sim_reboot(SimNodeHandle node, const SimNodeConfig* config) {
    if (!node || !config) return;
    
    auto* companion = static_cast<CompanionSimNode*>(node);
    
    // Wait for node to be idle
    {
        std::unique_lock<std::mutex> lock(companion->ctx.step_mutex);
        companion->ctx.step_cv.wait(lock, [companion] {
            return companion->ctx.state.load() == SimContext::State::IDLE ||
                   companion->ctx.state.load() == SimContext::State::YIELDED;
        });
    }
    
    // Reset subsystems (but preserve filesystem)
    // Use the pointers to the firmware thread's instances
    companion->config = *config;
    if (companion->radio_ptr) {
        companion->radio_ptr->configure(config->lora_freq, config->lora_bw,
                                        config->lora_sf, config->lora_cr, config->lora_tx_power);
        companion->radio_ptr->begin();
    }
    if (companion->board_ptr) {
        companion->board_ptr->init();
    }
    companion->ctx.rng.seed(config->rng_seed);
    companion->ctx.millis_clock.setMillis(config->initial_millis);
    companion->ctx.rtc_clock.setCurrentTime(config->initial_rtc);
    
    // Re-run setup
    companion->setup();
}

SIM_API const char* sim_get_node_type(void) {
    return "companion";
}

// Frame-based serial API - companion uses ArduinoSerialInterface with byte-based SimSerial
// These are stubs since the framing is handled internally by ArduinoSerialInterface
SIM_API void sim_inject_serial_frame(SimNodeHandle node,
                                      const uint8_t* data, size_t len) {
    (void)node; (void)data; (void)len;
    // Not used - companion uses byte-based sim_inject_serial_rx instead
}

SIM_API size_t sim_collect_serial_frame(SimNodeHandle node,
                                         uint8_t* buffer, size_t max_len) {
    (void)node; (void)buffer; (void)max_len;
    // Not used - companion uses byte-based serial TX via SimSerial
    return 0;
}

} // extern "C"
