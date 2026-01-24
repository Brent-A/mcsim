#pragma once

// ============================================================================
// CayenneLPP Stub for Simulation
// ============================================================================
// Minimal stub for telemetry data encoding

#include <cstdint>
#include <cstring>

#define LPP_DIGITAL_INPUT       0       // 1 byte
#define LPP_DIGITAL_OUTPUT      1       // 1 byte
#define LPP_ANALOG_INPUT        2       // 2 bytes, 0.01 signed
#define LPP_ANALOG_OUTPUT       3       // 2 bytes, 0.01 signed
#define LPP_LUMINOSITY          101     // 2 bytes, 1 lux unsigned
#define LPP_PRESENCE            102     // 1 byte, 1
#define LPP_TEMPERATURE         103     // 2 bytes, 0.1°C signed
#define LPP_RELATIVE_HUMIDITY   104     // 1 byte, 0.5% unsigned
#define LPP_ACCELEROMETER       113     // 2 bytes per axis, 0.001G
#define LPP_BAROMETRIC_PRESSURE 115     // 2 bytes 0.1 hPa unsigned
#define LPP_GYROMETER           134     // 2 bytes per axis, 0.01 °/s
#define LPP_GPS                 136     // 3 byte lon/lat 0.0001 °, 3 bytes alt 0.01m

#define LPP_MAX_BUFFER_SIZE     51

class CayenneLPP {
public:
    CayenneLPP(uint8_t size = LPP_MAX_BUFFER_SIZE) : maxsize_(size), cursor_(0) {
        buffer_ = new uint8_t[maxsize_];
        memset(buffer_, 0, maxsize_);
    }
    
    ~CayenneLPP() {
        delete[] buffer_;
    }
    
    void reset() {
        cursor_ = 0;
        memset(buffer_, 0, maxsize_);
    }
    
    uint8_t getSize() const { return cursor_; }
    uint8_t* getBuffer() { return buffer_; }
    const uint8_t* getBuffer() const { return buffer_; }
    
    uint8_t copy(uint8_t* dest) const {
        memcpy(dest, buffer_, cursor_);
        return cursor_;
    }
    
    // Add digital input
    uint8_t addDigitalInput(uint8_t channel, uint8_t value) {
        if (cursor_ + 3 > maxsize_) return 0;
        buffer_[cursor_++] = channel;
        buffer_[cursor_++] = LPP_DIGITAL_INPUT;
        buffer_[cursor_++] = value;
        return cursor_;
    }
    
    // Add digital output
    uint8_t addDigitalOutput(uint8_t channel, uint8_t value) {
        if (cursor_ + 3 > maxsize_) return 0;
        buffer_[cursor_++] = channel;
        buffer_[cursor_++] = LPP_DIGITAL_OUTPUT;
        buffer_[cursor_++] = value;
        return cursor_;
    }
    
    // Add analog input
    uint8_t addAnalogInput(uint8_t channel, float value) {
        if (cursor_ + 4 > maxsize_) return 0;
        int16_t val = static_cast<int16_t>(value * 100);
        buffer_[cursor_++] = channel;
        buffer_[cursor_++] = LPP_ANALOG_INPUT;
        buffer_[cursor_++] = (val >> 8) & 0xFF;
        buffer_[cursor_++] = val & 0xFF;
        return cursor_;
    }
    
    // Add analog output
    uint8_t addAnalogOutput(uint8_t channel, float value) {
        if (cursor_ + 4 > maxsize_) return 0;
        int16_t val = static_cast<int16_t>(value * 100);
        buffer_[cursor_++] = channel;
        buffer_[cursor_++] = LPP_ANALOG_OUTPUT;
        buffer_[cursor_++] = (val >> 8) & 0xFF;
        buffer_[cursor_++] = val & 0xFF;
        return cursor_;
    }
    
    // Add temperature
    uint8_t addTemperature(uint8_t channel, float celsius) {
        if (cursor_ + 4 > maxsize_) return 0;
        int16_t val = static_cast<int16_t>(celsius * 10);
        buffer_[cursor_++] = channel;
        buffer_[cursor_++] = LPP_TEMPERATURE;
        buffer_[cursor_++] = (val >> 8) & 0xFF;
        buffer_[cursor_++] = val & 0xFF;
        return cursor_;
    }
    
    // Add relative humidity
    uint8_t addRelativeHumidity(uint8_t channel, float humidity) {
        if (cursor_ + 3 > maxsize_) return 0;
        buffer_[cursor_++] = channel;
        buffer_[cursor_++] = LPP_RELATIVE_HUMIDITY;
        buffer_[cursor_++] = static_cast<uint8_t>(humidity * 2);
        return cursor_;
    }
    
    // Add barometric pressure
    uint8_t addBarometricPressure(uint8_t channel, float hpa) {
        if (cursor_ + 4 > maxsize_) return 0;
        uint16_t val = static_cast<uint16_t>(hpa * 10);
        buffer_[cursor_++] = channel;
        buffer_[cursor_++] = LPP_BAROMETRIC_PRESSURE;
        buffer_[cursor_++] = (val >> 8) & 0xFF;
        buffer_[cursor_++] = val & 0xFF;
        return cursor_;
    }
    
    // Add luminosity
    uint8_t addLuminosity(uint8_t channel, uint16_t lux) {
        if (cursor_ + 4 > maxsize_) return 0;
        buffer_[cursor_++] = channel;
        buffer_[cursor_++] = LPP_LUMINOSITY;
        buffer_[cursor_++] = (lux >> 8) & 0xFF;
        buffer_[cursor_++] = lux & 0xFF;
        return cursor_;
    }
    
    // Add GPS
    uint8_t addGPS(uint8_t channel, float latitude, float longitude, float altitude) {
        if (cursor_ + 11 > maxsize_) return 0;
        int32_t lat = static_cast<int32_t>(latitude * 10000);
        int32_t lon = static_cast<int32_t>(longitude * 10000);
        int32_t alt = static_cast<int32_t>(altitude * 100);
        
        buffer_[cursor_++] = channel;
        buffer_[cursor_++] = LPP_GPS;
        buffer_[cursor_++] = (lat >> 16) & 0xFF;
        buffer_[cursor_++] = (lat >> 8) & 0xFF;
        buffer_[cursor_++] = lat & 0xFF;
        buffer_[cursor_++] = (lon >> 16) & 0xFF;
        buffer_[cursor_++] = (lon >> 8) & 0xFF;
        buffer_[cursor_++] = lon & 0xFF;
        buffer_[cursor_++] = (alt >> 16) & 0xFF;
        buffer_[cursor_++] = (alt >> 8) & 0xFF;
        buffer_[cursor_++] = alt & 0xFF;
        return cursor_;
    }
    
    // Add presence
    uint8_t addPresence(uint8_t channel, uint8_t value) {
        if (cursor_ + 3 > maxsize_) return 0;
        buffer_[cursor_++] = channel;
        buffer_[cursor_++] = LPP_PRESENCE;
        buffer_[cursor_++] = value;
        return cursor_;
    }
    
    // Add accelerometer
    uint8_t addAccelerometer(uint8_t channel, float x, float y, float z) {
        if (cursor_ + 8 > maxsize_) return 0;
        int16_t vx = static_cast<int16_t>(x * 1000);
        int16_t vy = static_cast<int16_t>(y * 1000);
        int16_t vz = static_cast<int16_t>(z * 1000);
        
        buffer_[cursor_++] = channel;
        buffer_[cursor_++] = LPP_ACCELEROMETER;
        buffer_[cursor_++] = (vx >> 8) & 0xFF;
        buffer_[cursor_++] = vx & 0xFF;
        buffer_[cursor_++] = (vy >> 8) & 0xFF;
        buffer_[cursor_++] = vy & 0xFF;
        buffer_[cursor_++] = (vz >> 8) & 0xFF;
        buffer_[cursor_++] = vz & 0xFF;
        return cursor_;
    }
    
    // Add gyrometer
    uint8_t addGyrometer(uint8_t channel, float x, float y, float z) {
        if (cursor_ + 8 > maxsize_) return 0;
        int16_t vx = static_cast<int16_t>(x * 100);
        int16_t vy = static_cast<int16_t>(y * 100);
        int16_t vz = static_cast<int16_t>(z * 100);
        
        buffer_[cursor_++] = channel;
        buffer_[cursor_++] = LPP_GYROMETER;
        buffer_[cursor_++] = (vx >> 8) & 0xFF;
        buffer_[cursor_++] = vx & 0xFF;
        buffer_[cursor_++] = (vy >> 8) & 0xFF;
        buffer_[cursor_++] = vy & 0xFF;
        buffer_[cursor_++] = (vz >> 8) & 0xFF;
        buffer_[cursor_++] = vz & 0xFF;
        return cursor_;
    }
    
    // Add voltage (using analog input type with 0.01V resolution)
    uint8_t addVoltage(uint8_t channel, float volts) {
        return addAnalogInput(channel, volts);
    }
    
private:
    uint8_t* buffer_;
    uint8_t maxsize_;
    uint8_t cursor_;
};
