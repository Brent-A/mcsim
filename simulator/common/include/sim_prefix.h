// ============================================================================
// Simulator Prefix Header - Force-included before all other headers
// ============================================================================
// This header is injected by the compiler (-include / /FI) before any
// source file is processed. It sets up the simulation environment and
// can redirect global symbols to simulation-specific implementations.

#ifndef SIM_PREFIX_H
#define SIM_PREFIX_H

// CRITICAL: Define NOMINMAX before any Windows headers to prevent Windows.h
// from defining min/max macros that conflict with std::min/std::max
#ifndef NOMINMAX
#define NOMINMAX
#endif

// Mark that we're in simulation mode
#define MESHCORE_SIMULATOR 1

// Platform definition for conditional compilation in firmware
#ifndef ESP32
#define ESP32 1
#endif

// Disable features not needed in simulation
#define NO_RADIOLIB 1

// Disable CLI rescue mode (uses VLAs which MSVC doesn't support)
#define NO_CLI_RESCUE 1

// Firmware config defaults
#ifndef MAX_NEIGHBOURS
#define MAX_NEIGHBOURS 16
#endif

#ifndef MAX_CLIENTS
#define MAX_CLIENTS 32
#endif

#ifndef MAX_CONTACTS
#define MAX_CONTACTS 100
#endif

#ifndef MAX_GROUP_CHANNELS
#define MAX_GROUP_CHANNELS 8
#endif

// Pre-define FILESYSTEM to our SPIFFSClass to prevent IdentityStore.h from redefining it
#ifdef __cplusplus
class SPIFFSClass;
#define FILESYSTEM SPIFFSClass
#endif

// ============================================================================
// Global Symbol Redirection Strategy
// ============================================================================
// The firmware expects global variables: board, radio_driver, rtc_clock, sensors
// But "board" conflicts with function parameter names in CommonCLI.h, etc.
//
// Solution: Use mangled internal names, then #define the expected names.
// We temporarily #undef around headers that use these as parameter names.

// Internal names for the global instances (defined in each DLL's sim_main.cpp)
// IMPORTANT: These must be thread_local so each simulation thread gets its own instance
#ifdef __cplusplus
class SimBoard;
class SimRadio;
class SimRTCClock;
class EnvironmentSensorManager;

extern thread_local SimBoard _sim_board_instance;
extern thread_local SimRadio _sim_radio_instance;
extern thread_local SimRTCClock _sim_rtc_instance;
extern thread_local EnvironmentSensorManager _sim_sensors_instance;
#endif

// Macro redirections - map firmware's expected names to our internal names
#define board           _sim_board_instance
#define radio_driver    _sim_radio_instance
#define rtc_clock       _sim_rtc_instance
#define sensors         _sim_sensors_instance

// ============================================================================
// Helper macros to temporarily disable redirections
// ============================================================================
// Use these around #include of headers that have conflicting parameter names

#define SIM_UNDEF_GLOBALS() \
    _Pragma("push_macro(\"board\")") \
    _Pragma("push_macro(\"radio_driver\")") \
    _Pragma("push_macro(\"rtc_clock\")") \
    _Pragma("push_macro(\"sensors\")") \
    _Pragma("warning(push)") \
    _Pragma("warning(disable: 4005)") \
    /* Undefine to allow parameter names */ \
    /* These are redefined by pop below */

#define SIM_RESTORE_GLOBALS() \
    _Pragma("warning(pop)") \
    _Pragma("pop_macro(\"sensors\")") \
    _Pragma("pop_macro(\"rtc_clock\")") \
    _Pragma("pop_macro(\"radio_driver\")") \
    _Pragma("pop_macro(\"board\")")

// For headers with conflicting parameter names, we need to #undef before include
// This is handled in sim_compat.h which wraps problematic includes

#endif // SIM_PREFIX_H

// ============================================================================
// Arduino min/max compatibility - OUTSIDE include guard
// ============================================================================
// These are defined outside the include guard so they get redefined every time
// this header is force-included. This ensures they survive any #undef in 
// system headers like <algorithm> or <windows.h>
//
// NOTE: We do NOT define 'abs' as a macro because it breaks MSVC's <chrono>
// and other standard library headers. Arduino code should use std::abs or
// the standard abs() function.

// Only define if we have a compiler that supports this approach
#ifdef __cplusplus

// Include <algorithm> to get std::min/std::max and <type_traits> for common_type
#include <algorithm>
#include <type_traits>
#include <cstdlib>  // for std::abs

// Template implementations - use guard to avoid redefinition
#ifndef _SIM_MIN_MAX_IMPL_DEFINED
#define _SIM_MIN_MAX_IMPL_DEFINED

template<typename T, typename U>
inline auto _sim_min_impl(T a, U b) -> typename std::common_type<T, U>::type {
    using R = typename std::common_type<T, U>::type;
    return (static_cast<R>(a) < static_cast<R>(b)) ? static_cast<R>(a) : static_cast<R>(b);
}

template<typename T, typename U>
inline auto _sim_max_impl(T a, U b) -> typename std::common_type<T, U>::type {
    using R = typename std::common_type<T, U>::type;
    return (static_cast<R>(a) > static_cast<R>(b)) ? static_cast<R>(a) : static_cast<R>(b);
}

#endif // _SIM_MIN_MAX_IMPL_DEFINED

// Force-define min/max macros (not abs - it breaks STL)
#undef min
#undef max

#define min(a, b) _sim_min_impl(a, b)
#define max(a, b) _sim_max_impl(a, b)

// abs: just bring std::abs into scope (don't use macro)
using std::abs;

#endif // __cplusplus
