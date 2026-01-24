#pragma once

// ============================================================================
// Internal Filesystem Stub for Simulation
// ============================================================================
// Used by NRF52 and STM32 platforms. Maps to SimFilesystem.

#include "SPIFFS.h"

// InternalFS is an alias for SPIFFS in simulation
#define InternalFS SPIFFS
