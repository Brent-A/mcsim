// Stub source for target.h - provides implementations for target-specific functions
// Note: The global instances (board, radio_driver, rtc_clock, sensors) are defined
// in each DLL's sim_main.cpp, not here. This file only provides stub implementations
// for any functions declared in target.h that need bodies.

#include "target.h"

// No global sensor manager instance here - it's defined in sim_main.cpp
// The macro 'sensors' expands to '_sim_sensors_instance' which is defined there.
