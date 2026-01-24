// ============================================================================
// Simulator Compatibility Header - Safe includes for problematic headers
// ============================================================================
// Some MeshCore headers use "board" as a parameter name, which conflicts
// with our macro redirection. This header provides safe wrappers.

#ifndef SIM_COMPAT_H
#define SIM_COMPAT_H

// Temporarily disable our global macros for headers with conflicting names
#ifdef board
#undef board
#endif
#ifdef radio_driver  
#undef radio_driver
#endif
#ifdef rtc_clock
#undef rtc_clock
#endif
#ifdef sensors
#undef sensors
#endif

// Now include the problematic headers safely
#include <helpers/CommonCLI.h>
#include <helpers/StatsFormatHelper.h>

// Re-enable the macros
#define board           _sim_board_instance
#define radio_driver    _sim_radio_instance
#define rtc_clock       _sim_rtc_instance
#define sensors         _sim_sensors_instance

#endif // SIM_COMPAT_H
