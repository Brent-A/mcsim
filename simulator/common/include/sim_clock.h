#pragma once

#include <MeshCore.h>
#include <Dispatcher.h>
#include <cstdint>

// ============================================================================
// Simulated Millisecond Clock
// ============================================================================
// Implements mesh::MillisecondClock interface.
// Time is controlled externally by the coordinator.

class SimMillisClock : public mesh::MillisecondClock {
public:
    SimMillisClock() : current_millis_(0) {}
    
    unsigned long getMillis() override {
        return static_cast<unsigned long>(current_millis_);
    }
    
    void setMillis(uint64_t millis) {
        current_millis_ = millis;
    }
    
    uint64_t getMillis64() const {
        return current_millis_;
    }

private:
    uint64_t current_millis_;
};

// ============================================================================
// Simulated RTC Clock
// ============================================================================
// Implements mesh::RTCClock interface.
// Time is controlled externally by the coordinator.

class SimRTCClock : public mesh::RTCClock {
public:
    SimRTCClock() : current_time_(0) {}
    
    uint32_t getCurrentTime() override {
        return current_time_;
    }
    
    void setCurrentTime(uint32_t time) override {
        current_time_ = time;
    }
    
    void tick() override {
        // No-op in simulation - time is externally controlled
    }

private:
    uint32_t current_time_;
};
