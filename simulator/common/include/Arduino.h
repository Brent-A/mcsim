#pragma once

// ============================================================================
// Arduino API Stub for Simulation
// ============================================================================
// This header provides Arduino-compatible APIs that delegate to the
// simulation context (SimContext). The context is accessed via thread-local
// storage, allowing multiple node instances to run in separate threads.

// IMPORTANT: Disable min/max/abs macros that conflict with MSVC standard library
// We must define NOMINMAX before any Windows headers
#ifndef NOMINMAX
#define NOMINMAX
#endif

// Include standard headers FIRST before any macro definitions
#include <cstdint>
#include <cstddef>
#include <cstring>
#include <cstdio>
#include <cstdarg>
#include <cmath>
#include <algorithm>
#include <chrono>
#include <string>
#include <mutex>
#include <condition_variable>

// Forward declare SimContext access
struct SimContext;
extern thread_local SimContext* g_sim_ctx;

// ============================================================================
// Basic Types
// ============================================================================

typedef bool boolean;
typedef uint8_t byte;

// ============================================================================
// Time Functions
// ============================================================================

unsigned long millis();
unsigned long micros();
void delay(unsigned long ms);
void delayMicroseconds(unsigned int us);

// ============================================================================
// Math Functions (use inline functions, NOT macros, to avoid STL conflicts)
// ============================================================================

template<typename T>
inline T arduino_min(T a, T b) { return (a < b) ? a : b; }

template<typename T>
inline T arduino_max(T a, T b) { return (a > b) ? a : b; }

// NOTE: arduino_abs is intentionally NOT a macro - use std::abs instead
// Defining abs as macro breaks MSVC's standard library headers

// Only define min/max macros (NOT abs - it breaks STL)
#ifndef ARDUINO_NO_MACROS
  // Force define (undef first in case Windows defined them)
  #undef min
  #undef max
  #define min(a,b) arduino_min(a,b)
  #define max(a,b) arduino_max(a,b)
#endif

// constrain with different types - use common_type to handle mixed int/float
#ifndef constrain
template<typename T, typename U, typename V>
inline auto constrain(T amt, U low, V high) -> decltype(amt + low + high) {
    using ResultType = decltype(amt + low + high);
    ResultType a = static_cast<ResultType>(amt);
    ResultType l = static_cast<ResultType>(low);
    ResultType h = static_cast<ResultType>(high);
    return (a < l) ? l : ((a > h) ? h : a);
}
#endif

#ifndef map
inline long map(long x, long in_min, long in_max, long out_min, long out_max) {
    return (x - in_min) * (out_max - out_min) / (in_max - in_min) + out_min;
}
#endif

inline long random(long max) {
    // Simple implementation - could be improved
    return rand() % max;
}

inline long random(long min, long max) {
    return min + (rand() % (max - min));
}

inline void randomSeed(unsigned long seed) {
    srand(static_cast<unsigned int>(seed));
}

// ============================================================================
// Pin Definitions (no-op in simulation)
// ============================================================================

#define INPUT           0x0
#define OUTPUT          0x1
#define INPUT_PULLUP    0x2
#define INPUT_PULLDOWN  0x3

#define LOW             0x0
#define HIGH            0x1

#define CHANGE          0x1
#define FALLING         0x2
#define RISING          0x3

inline void pinMode(uint8_t pin, uint8_t mode) { (void)pin; (void)mode; }
inline void digitalWrite(uint8_t pin, uint8_t val) { (void)pin; (void)val; }
inline int digitalRead(uint8_t pin) { (void)pin; return LOW; }
inline int analogRead(uint8_t pin) { (void)pin; return 0; }
inline void analogWrite(uint8_t pin, int val) { (void)pin; (void)val; }
inline void analogReadResolution(int bits) { (void)bits; }
inline uint32_t analogReadMilliVolts(uint8_t pin) { (void)pin; return 0; }
inline void adcAttachPin(uint8_t pin) { (void)pin; }

// ============================================================================
// Serial Class - Forward declaration (full implementation after Stream)
// ============================================================================

class SimSerialClass;
extern SimSerialClass Serial;
extern SimSerialClass Serial1;

// ============================================================================
// String Class (simplified)
// ============================================================================

class String {
public:
    String() : buffer_(nullptr), len_(0), capacity_(0) {}
    String(const char* str);
    String(const String& other);
    String(String&& other) noexcept;
    ~String();
    
    String& operator=(const String& other);
    String& operator=(String&& other) noexcept;
    String& operator=(const char* str);
    
    String& operator+=(const String& other);
    String& operator+=(const char* str);
    String& operator+=(char c);
    
    bool operator==(const String& other) const;
    bool operator==(const char* str) const;
    bool operator!=(const String& other) const { return !(*this == other); }
    bool operator!=(const char* str) const { return !(*this == str); }
    
    char operator[](unsigned int index) const;
    char& operator[](unsigned int index);
    
    const char* c_str() const { return buffer_ ? buffer_ : ""; }
    unsigned int length() const { return len_; }
    bool isEmpty() const { return len_ == 0; }
    
    void reserve(unsigned int size);
    
    int indexOf(char c) const;
    int indexOf(const char* str) const;
    String substring(unsigned int from, unsigned int to = 0xFFFFFFFF) const;
    
    void trim();
    void toLowerCase();
    void toUpperCase();
    
    long toInt() const;
    float toFloat() const;

private:
    char* buffer_;
    unsigned int len_;
    unsigned int capacity_;
    
    void ensureCapacity(unsigned int cap);
};

String operator+(const String& lhs, const String& rhs);
String operator+(const String& lhs, const char* rhs);
String operator+(const char* lhs, const String& rhs);

// ============================================================================
// Print/Printable Interface
// ============================================================================

#define DEC 10
#define HEX 16
#define OCT 8
#define BIN 2

class Print {
public:
    virtual size_t write(uint8_t) = 0;
    virtual size_t write(const uint8_t* buffer, size_t size);
    
    size_t print(const char* str);
    size_t print(char c);
    size_t print(int n, int base = DEC);
    size_t print(unsigned int n, int base = DEC);
    size_t print(long n, int base = DEC);
    size_t print(unsigned long n, int base = DEC);
    size_t print(double n, int digits = 2);
    size_t print(const String& str);
    
    size_t println();
    size_t println(const char* str);
    size_t println(char c);
    size_t println(int n, int base = DEC);
    size_t println(unsigned int n, int base = DEC);
    size_t println(long n, int base = DEC);
    size_t println(unsigned long n, int base = DEC);
    size_t println(double n, int digits = 2);
    size_t println(const String& str);
    
    size_t printf(const char* format, ...);
};

// ============================================================================
// Stream Interface
// ============================================================================

class Stream : public Print {
public:
    virtual int available() = 0;
    virtual int read() = 0;
    virtual int peek() = 0;
    
    size_t readBytes(uint8_t* buffer, size_t length);
    size_t readBytes(char* buffer, size_t length) {
        return readBytes((uint8_t*)buffer, length);
    }
    
    String readString();
    String readStringUntil(char terminator);
    
    void setTimeout(unsigned long timeout) { timeout_ = timeout; }
    unsigned long getTimeout() const { return timeout_; }

protected:
    unsigned long timeout_ = 1000;
};

// ============================================================================
// HardwareSerial (alias for Serial)
// ============================================================================

class HardwareSerial : public Stream {
public:
    HardwareSerial(int uart_nr = 0) : uart_nr_(uart_nr) {}
    
    void begin(unsigned long baud, uint32_t config = 0, int8_t rxPin = -1, int8_t txPin = -1);
    void end();
    
    int available() override;
    int read() override;
    int peek() override;
    size_t write(uint8_t c) override;
    size_t write(const uint8_t* buffer, size_t size) override;
    
    void flush();
    
    operator bool() const { return true; }

private:
    int uart_nr_;
};

// ============================================================================
// SimSerialClass - Full implementation inheriting from Stream
// ============================================================================

class SimSerialClass : public Stream {
public:
    void begin(unsigned long baud) { (void)baud; }
    void end() {}
    
    int available() override;
    int read() override;
    int peek() override;
    
    size_t write(uint8_t c) override;
    size_t write(const uint8_t* buffer, size_t size) override;
    size_t write(const char* str) { return write((const uint8_t*)str, strlen(str)); }
    
    void flush() {}
    
    operator bool() const { return true; }
};

// ============================================================================
// SPI Stub
// ============================================================================

#define SPI_MODE0 0
#define SPI_MODE1 1
#define SPI_MODE2 2
#define SPI_MODE3 3

#define MSBFIRST 1
#define LSBFIRST 0

class SPISettings {
public:
    SPISettings() : clock_(1000000), bitOrder_(MSBFIRST), dataMode_(SPI_MODE0) {}
    SPISettings(uint32_t clock, uint8_t bitOrder, uint8_t dataMode)
        : clock_(clock), bitOrder_(bitOrder), dataMode_(dataMode) {}
    
    uint32_t clock_;
    uint8_t bitOrder_;
    uint8_t dataMode_;
};

class SPIClass {
public:
    SPIClass(uint8_t spi_bus = 0) : spi_bus_(spi_bus) {}
    
    void begin(int8_t sck = -1, int8_t miso = -1, int8_t mosi = -1, int8_t ss = -1) {
        (void)sck; (void)miso; (void)mosi; (void)ss;
    }
    void end() {}
    
    void beginTransaction(SPISettings settings) { (void)settings; }
    void endTransaction() {}
    
    uint8_t transfer(uint8_t data) { (void)data; return 0; }
    uint16_t transfer16(uint16_t data) { (void)data; return 0; }
    void transfer(void* buf, size_t count) { (void)buf; (void)count; }
    
    void setBitOrder(uint8_t bitOrder) { (void)bitOrder; }
    void setDataMode(uint8_t dataMode) { (void)dataMode; }
    void setClockDivider(uint8_t clockDiv) { (void)clockDiv; }

private:
    uint8_t spi_bus_;
};

extern SPIClass SPI;

// ============================================================================
// Wire (I2C) Stub
// ============================================================================

class TwoWire {
public:
    TwoWire(uint8_t bus = 0) : bus_(bus) {}
    
    void begin(int sda = -1, int scl = -1) { (void)sda; (void)scl; }
    void end() {}
    
    void beginTransmission(uint8_t address) { (void)address; }
    uint8_t endTransmission(bool sendStop = true) { (void)sendStop; return 0; }
    
    size_t write(uint8_t data) { (void)data; return 1; }
    size_t write(const uint8_t* data, size_t len) { (void)data; (void)len; return len; }
    
    uint8_t requestFrom(uint8_t address, uint8_t quantity, bool sendStop = true) {
        (void)address; (void)quantity; (void)sendStop;
        return 0;
    }
    
    int available() { return 0; }
    int read() { return -1; }

private:
    uint8_t bus_;
};

extern TwoWire Wire;

// ============================================================================
// Yield function
// ============================================================================

void yield();

// ============================================================================
// ESP32-specific stubs (commonly used)
// ============================================================================

inline void esp_restart() {
    // Will be handled via SimBoard::reboot()
}

inline uint32_t esp_random() {
    return static_cast<uint32_t>(rand());
}

// CPU frequency (no-op in sim)
inline void setCpuFrequencyMhz(uint32_t mhz) { (void)mhz; }
inline uint32_t getCpuFrequencyMhz() { return 240; }

// Reset reason
enum esp_reset_reason_t {
    ESP_RST_UNKNOWN = 0,
    ESP_RST_POWERON = 1,
    ESP_RST_SW = 3,
    ESP_RST_PANIC = 4,
    ESP_RST_DEEPSLEEP = 5,
};

inline esp_reset_reason_t esp_reset_reason() {
    return ESP_RST_POWERON;
}

// NeoPixel stub
inline void neopixelWrite(uint8_t pin, uint8_t r, uint8_t g, uint8_t b) {
    (void)pin; (void)r; (void)g; (void)b;
}

// ============================================================================
// Arduino min/max macros - MUST BE DEFINED LAST  
// ============================================================================
// These are re-defined here at the end of the file to ensure they survive
// any #undef that might happen in included headers like <algorithm>
// NOTE: abs is NOT defined as macro - use std::abs instead (macro breaks STL)

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

// Force-define min/max macros (undef any existing definitions first)
#undef min
#undef max

#define min(a, b) _sim_min_impl(a, b)
#define max(a, b) _sim_max_impl(a, b)
