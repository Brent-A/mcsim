#pragma once

// ============================================================================
// Sensor Manager Stub for Simulation
// ============================================================================

#include <cstdint>
#include <cstring>
#include <CayenneLPP.h>

// Permission flags
#define TELEM_PERM_BASE         0x01   // 'base' permission includes battery
#define TELEM_PERM_LOCATION     0x02
#define TELEM_PERM_ENVIRONMENT  0x04   // permission to access environment sensors

#define TELEM_CHANNEL_SELF   1   // LPP data channel for 'self' device

// Location provider stub
class LocationProvider {
public:
    virtual ~LocationProvider() = default;
    virtual void begin() {}
    virtual void loop() {}
    virtual bool hasLocation() const { return false; }
    virtual double getLatitude() const { return 0.0; }
    virtual double getLongitude() const { return 0.0; }
    virtual float getAltitude() const { return 0.0f; }
    virtual uint8_t getSatellites() const { return 0; }
};

// Sensor reading structure
struct SensorReading {
    float temperature;
    float humidity;
    float pressure;
    float battery_voltage;
    bool has_temperature;
    bool has_humidity;
    bool has_pressure;
    
    SensorReading() : temperature(0), humidity(0), pressure(0), battery_voltage(0),
                      has_temperature(false), has_humidity(false), has_pressure(false) {}
};

// Sensor manager stub - matches real SensorManager interface
class SensorManager {
public:
    // Location data (accessed by CommonCLI and firmware)
    double node_lat, node_lon;
    double node_altitude;
    
    SensorManager() : node_lat(0), node_lon(0), node_altitude(0) {}
    virtual ~SensorManager() = default;
    
    virtual bool begin() { return true; }
    virtual void loop() {}
    
    // Query sensors and fill telemetry data
    virtual bool querySensors(uint8_t requester_permissions, CayenneLPP& telemetry) { 
        (void)requester_permissions;
        (void)telemetry;
        return false; 
    }
    
    // Settings interface (used by CommonCLI)
    virtual int getNumSettings() const { return 0; }
    virtual const char* getSettingName(int idx) const { 
        (void)idx;
        return nullptr; 
    }
    virtual const char* getSettingValue(int idx) const { 
        (void)idx;
        return nullptr; 
    }
    virtual bool setSettingValue(const char* name, const char* value) { 
        (void)name; (void)value;
        return false; 
    }
    
    virtual LocationProvider* getLocationProvider() { return nullptr; }
    
    // Helper function to get setting by key
    const char* getSettingByKey(const char* key) {
        int num = getNumSettings();
        for (int i = 0; i < num; i++) {
            if (strcmp(getSettingName(i), key) == 0) {
                return getSettingValue(i);
            }
        }
        return nullptr;
    }
};
