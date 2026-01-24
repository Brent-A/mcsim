// Room Server Node Simulation Entry Point

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

// Include the room server's MyMesh implementation
#include "MyMesh.h"

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
// Room Server-specific SimNode implementation
// ============================================================================

struct RoomServerSimNode : public SimNodeImpl {
    // Firmware objects
    SimRNG fast_rng;
    SimpleMeshTables tables;
    std::unique_ptr<MyMesh> mesh;
    
    // CLI command buffer (matches firmware's main.cpp)
    char command[160];
    
    RoomServerSimNode() : mesh(nullptr) {
        command[0] = 0;
    }
    
    ~RoomServerSimNode() override {
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
        
        // Create the mesh instance using the global board/radio references
        // (which are redirected via macros to _sim_board_instance etc.)
        mesh = std::make_unique<MyMesh>(
            _sim_board_instance,
            _sim_radio_instance,
            ctx.millis_clock,
            fast_rng,
            _sim_rtc_instance,
            tables
        );
        
        // Set the identity from config (both private and public keys)
        // readFrom expects: prv_key (64 bytes) followed by pub_key (32 bytes)
        uint8_t identity_data[PRV_KEY_SIZE + PUB_KEY_SIZE];
        memcpy(identity_data, config.private_key, PRV_KEY_SIZE);
        memcpy(identity_data + PRV_KEY_SIZE, config.public_key, PUB_KEY_SIZE);
        mesh->self_id.readFrom(identity_data, sizeof(identity_data));
        
        // Initialize the mesh using the SPIFFS global filesystem
        mesh->begin(&SPIFFS);
        
        // Set the node name from config (used for advertisements)
        if (config.node_name[0] != '\0') {
            NodePrefs* prefs = mesh->getNodePrefs();
            strncpy(prefs->node_name, config.node_name, sizeof(prefs->node_name) - 1);
            prefs->node_name[sizeof(prefs->node_name) - 1] = '\0';
        }
        
        // Reset CLI command buffer
        command[0] = 0;
    }
    
    void loop() override {
        // Process serial CLI commands (matches firmware's main.cpp loop)
        int len = strlen(command);
        while (Serial.available() && len < (int)sizeof(command) - 1) {
            char c = Serial.read();
            if (c != '\n') {
                command[len++] = c;
                command[len] = 0;
                Serial.print(c);
            }
            if (c == '\r') break;
        }
        if (len == (int)sizeof(command) - 1) {  // command buffer full
            command[sizeof(command) - 1] = '\r';
        }
        
        if (len > 0 && command[len - 1] == '\r') {  // received complete line
            Serial.print('\n');
            command[len - 1] = 0;  // replace newline with C string null terminator
            char reply[160];
            mesh->handleCommand(0, command, reply);  // NOTE: there is no sender_timestamp via serial!
            if (reply[0]) {
                Serial.print("  -> ");
                Serial.println(reply);
            }
            
            command[0] = 0;  // reset command buffer
        }
        
        // Run mesh loop
        if (mesh) {
            mesh->loop();
        }
        
        ctx.rtc_clock.tick();
    }
    
    const char* getNodeType() const override {
        return "room_server";
    }
};

// ============================================================================
// C API Implementation
// ============================================================================

extern "C" {

SIM_API SimNodeHandle sim_create(const SimNodeConfig* config) {
    if (!config) return nullptr;
    
    auto* node = new RoomServerSimNode();
    node->config = *config;
    
    // Apply spin detection config from SimNodeConfig
    node->ctx.spin_config.threshold = config->spin_detection_threshold;
    node->ctx.spin_config.log_spin_detection = config->log_spin_detection != 0;
    node->ctx.spin_config.log_loop_iterations = config->log_loop_iterations != 0;
    // Note: idle_loops_before_yield is used in sim_node_base.cpp for yield logic
    
    // Start the node thread
    node->node_thread = std::thread(&RoomServerSimNode::threadMain, node);
    
    return node;
}

SIM_API void sim_destroy(SimNodeHandle node) {
    if (!node) return;
    
    auto* room_server = static_cast<RoomServerSimNode*>(node);
    delete room_server;
}

SIM_API void sim_reboot(SimNodeHandle node, const SimNodeConfig* config) {
    if (!node || !config) return;
    
    auto* room_server = static_cast<RoomServerSimNode*>(node);
    
    // Wait for node to be idle
    {
        std::unique_lock<std::mutex> lock(room_server->ctx.step_mutex);
        room_server->ctx.step_cv.wait(lock, [room_server] {
            return room_server->ctx.state.load() == SimContext::State::IDLE ||
                   room_server->ctx.state.load() == SimContext::State::YIELDED;
        });
    }
    
    // Reset subsystems (but preserve filesystem)
    // Use the pointers to the firmware thread's instances
    room_server->config = *config;
    if (room_server->radio_ptr) {
        room_server->radio_ptr->configure(config->lora_freq, config->lora_bw,
                                          config->lora_sf, config->lora_cr, config->lora_tx_power);
        room_server->radio_ptr->begin();
    }
    if (room_server->board_ptr) {
        room_server->board_ptr->init();
    }
    room_server->ctx.rng.seed(config->rng_seed);
    room_server->ctx.millis_clock.setMillis(config->initial_millis);
    room_server->ctx.rtc_clock.setCurrentTime(config->initial_rtc);
    
    // Re-run setup
    room_server->setup();
}

SIM_API const char* sim_get_node_type(void) {
    return "room_server";
}

// Room server doesn't use frame-based serial interface, provide stubs
SIM_API void sim_inject_serial_frame(SimNodeHandle node,
                                      const uint8_t* data, size_t len) {
    (void)node; (void)data; (void)len;
    // Room server uses byte-based Serial, not frame-based interface
}

SIM_API size_t sim_collect_serial_frame(SimNodeHandle node,
                                         uint8_t* buffer, size_t max_len) {
    (void)node; (void)buffer; (void)max_len;
    // Room server uses byte-based Serial, not frame-based interface
    return 0;
}

} // extern "C"
