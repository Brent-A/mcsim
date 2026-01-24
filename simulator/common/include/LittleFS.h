#pragma once

// ============================================================================
// LittleFS Stub for Simulation
// ============================================================================
// Used by RP2040 platform. Maps to SimFilesystem.

#include "SPIFFS.h"

// LittleFS is an alias for SPIFFS in simulation
#define LittleFS SPIFFS
