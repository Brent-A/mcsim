#include "sim_board.h"
#include "sim_context.h"

SimBoard::SimBoard() 
    : battery_mv_(4200)
    , reboot_requested_(false)
    , poweroff_requested_(false)
{
}

void SimBoard::init() {
    reboot_requested_ = false;
    poweroff_requested_ = false;
}

uint16_t SimBoard::getBattMilliVolts() {
    return battery_mv_;
}

const char* SimBoard::getManufacturerName() const {
    return "Simulator";
}

void SimBoard::reboot() {
    reboot_requested_ = true;
    
    // Signal the reboot in the step result
    if (g_sim_ctx) {
        g_sim_ctx->step_result.reason = SIM_YIELD_REBOOT;
    }
}

void SimBoard::powerOff() {
    poweroff_requested_ = true;
    
    // Signal power off in the step result
    if (g_sim_ctx) {
        g_sim_ctx->step_result.reason = SIM_YIELD_POWER_OFF;
    }
}

uint8_t SimBoard::getStartupReason() const {
    return BD_STARTUP_NORMAL;
}
