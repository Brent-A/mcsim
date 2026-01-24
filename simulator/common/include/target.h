#pragma once

// ============================================================================
// Simulator Target Header
// ============================================================================
// This replaces the hardware-specific target.h for simulation.
// Each node DLL has its own set of globals defined in sim_main.cpp
//
// NOTE: The global symbols (board, radio_driver, etc.) are redirected via
// macros in sim_prefix.h which is force-included before all source files.
// The actual instances use internal names (_sim_board_instance, etc.)

#include "Arduino.h"
#include "sim_board.h"
#include "sim_radio.h"
#include "sim_clock.h"
#include "sim_rng.h"
#include "sim_context.h"
#include "SPIFFS.h"

// Sensor manager stub - inherits from base SensorManager
#include "helpers/SensorManager.h"

class EnvironmentSensorManager : public SensorManager {
public:
    EnvironmentSensorManager() : SensorManager() {}
    bool begin() override { return true; }
    void loop() override {}
};

// Global extern declarations using internal names
// The macros in sim_prefix.h redirect board -> _sim_board_instance, etc.
// These are defined in each node's sim_main.cpp as thread_local
extern thread_local SimBoard _sim_board_instance;
extern thread_local SimRadio _sim_radio_instance;
extern thread_local SimRTCClock _sim_rtc_instance;
extern thread_local EnvironmentSensorManager _sim_sensors_instance;

// Radio helper functions (stubs)
// Note: these use the macro names which get redirected to internal names
inline bool radio_init() {
    return true;
}

inline uint32_t radio_get_rng_seed() {
    // Use the global RNG from SimContext if available, else return fixed seed
    if (g_sim_ctx) {
        return g_sim_ctx->rng.next();
    }
    return 12345;
}

inline void radio_set_params(float freq, float bw, uint8_t sf, uint8_t cr) {
    // Use internal name directly to avoid macro issues in this header
    _sim_radio_instance.configure(freq, bw, sf, cr, 20);
}

inline void radio_set_tx_power(uint8_t dbm) {
    // No-op in simulation
    (void)dbm;
}

inline mesh::LocalIdentity radio_new_identity() {
    if (g_sim_ctx) {
        return mesh::LocalIdentity(&g_sim_ctx->rng);
    }
    // Fallback - should not happen
    static SimRNG fallback_rng;
    return mesh::LocalIdentity(&fallback_rng);
}
