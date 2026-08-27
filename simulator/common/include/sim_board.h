#pragma once

#include <MeshCore.h>
#include <atomic>

// ============================================================================
// Simulated Board
// ============================================================================
// Implements mesh::MainBoard interface for simulation.

class KeyValueStore;  // forward decl (only used as an opaque no-op argument below)

class SimBoard : public mesh::MainBoard {
public:
    SimBoard();
    
    void init();
    
    // mesh::MainBoard interface
    uint16_t getBattMilliVolts() override;
    const char* getManufacturerName() const override;
    void reboot() override;
    void powerOff() override;
    uint8_t getStartupReason() const override;

    // Matches the no-op on real board headers (ESP32Board.h etc.): dynamic prefs
    // have no hardware side effects to simulate.
    void attachDynamicPrefs(KeyValueStore* prefs) { }
    
    // Simulation state
    bool wasRebootRequested() const { return reboot_requested_; }
    bool wasPowerOffRequested() const { return poweroff_requested_; }
    void clearRebootRequest() { reboot_requested_ = false; }
    void clearPowerOffRequest() { poweroff_requested_ = false; }
    
    // Configuration
    void setBatteryMilliVolts(uint16_t mv) { battery_mv_ = mv; }

private:
    uint16_t battery_mv_;
    std::atomic<bool> reboot_requested_;
    std::atomic<bool> poweroff_requested_;
};
