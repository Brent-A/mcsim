#pragma once

// ============================================================================
// Display Driver Stub for Simulation
// ============================================================================
// UI is not simulated, so this is a no-op stub.

#include <cstdint>

class DisplayDriver {
public:
    virtual ~DisplayDriver() = default;
    virtual bool begin() { return true; }
    virtual void startFrame() {}
    virtual void endFrame() {}
    virtual void setCursor(int x, int y) { (void)x; (void)y; }
    virtual void print(const char* str) { (void)str; }
    virtual void println(const char* str = "") { (void)str; }
    virtual int width() const { return 128; }
    virtual int height() const { return 64; }
    virtual void clear() {}
};
