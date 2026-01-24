#pragma once

// ============================================================================
// UIScreen Stub for Simulation
// ============================================================================

class UIScreen {
public:
    virtual ~UIScreen() = default;
    virtual void draw() {}
    virtual void onButton() {}
};
