#pragma once

// ============================================================================
// RTClib Stub for Simulation
// ============================================================================
// Provides minimal RTClib compatibility for firmware that includes it.

#include <cstdint>

// DateTime class (from RTClib)
class DateTime {
public:
    DateTime(uint32_t t = 0) : unixtime_(t) {}
    DateTime(uint16_t year, uint8_t month, uint8_t day,
             uint8_t hour = 0, uint8_t minute = 0, uint8_t second = 0) {
        // Simplified - doesn't handle all edge cases
        // Good enough for simulation
        unixtime_ = 0; // Would need proper calculation
        (void)year; (void)month; (void)day;
        (void)hour; (void)minute; (void)second;
    }
    
    uint32_t unixtime() const { return unixtime_; }
    
    uint16_t year() const { return 2024; }  // Stub
    uint8_t month() const { return 1; }
    uint8_t day() const { return 1; }
    uint8_t hour() const { return 0; }
    uint8_t minute() const { return 0; }
    uint8_t second() const { return 0; }
    
private:
    uint32_t unixtime_;
};

// RTC_DS3231 stub
class RTC_DS3231 {
public:
    bool begin() { return true; }
    DateTime now() { return DateTime(0); }
    void adjust(const DateTime& dt) { (void)dt; }
    bool lostPower() { return false; }
};

// RTC_PCF8523 stub
class RTC_PCF8523 {
public:
    bool begin() { return true; }
    DateTime now() { return DateTime(0); }
    void adjust(const DateTime& dt) { (void)dt; }
    bool lostPower() { return false; }
};

// RV3028 stub (commonly used in MeshCore)
class RV3028 {
public:
    bool begin() { return true; }
    bool setTime(uint8_t sec, uint8_t min, uint8_t hour, 
                 uint8_t weekday, uint8_t date, uint8_t month, uint16_t year) {
        (void)sec; (void)min; (void)hour; (void)weekday;
        (void)date; (void)month; (void)year;
        return true;
    }
    uint32_t getUNIX() { return 0; }
    bool setUNIX(uint32_t t) { (void)t; return true; }
};
