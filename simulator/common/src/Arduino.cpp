#include "sim_context.h"
#include "Arduino.h"
#include <cstdio>
#include <cstdarg>

// Thread-local context pointer
thread_local SimContext* g_sim_ctx = nullptr;

// Global instances for Serial, SPI, Wire
SimSerialClass Serial;
SimSerialClass Serial1;
SPIClass SPI;
TwoWire Wire;

// ============================================================================
// Time Functions
// ============================================================================

unsigned long millis() {
    if (g_sim_ctx) {
        return static_cast<unsigned long>(g_sim_ctx->current_millis);
    }
    return 0;
}

unsigned long micros() {
    if (g_sim_ctx) {
        return static_cast<unsigned long>(g_sim_ctx->current_millis * 1000);
    }
    return 0;
}

void delay(unsigned long ms) {
    // In simulation, delay is a no-op since time is externally controlled
    // The coordinator advances time between steps
    (void)ms;
}

void delayMicroseconds(unsigned int us) {
    (void)us;
}

void yield() {
    // No-op in simulation
}

// ============================================================================
// SimSerialClass Implementation
// ============================================================================

int SimSerialClass::available() {
    if (g_sim_ctx) {
        return static_cast<int>(g_sim_ctx->serial.available());
    }
    return 0;
}

int SimSerialClass::read() {
    if (g_sim_ctx) {
        return g_sim_ctx->serial.read();
    }
    return -1;
}

int SimSerialClass::peek() {
    // Not implemented - would need queue modification
    return -1;
}

size_t SimSerialClass::write(uint8_t c) {
    if (g_sim_ctx) {
        // Log to the log buffer (for console display)
        char str[2] = {static_cast<char>(c), '\0'};
        g_sim_ctx->appendLog(str, 1);
        // Also send to serial TX buffer (for TCP transmission)
        g_sim_ctx->appendSerialTx(&c, 1);
        return 1;
    }
    return 0;
}

size_t SimSerialClass::write(const uint8_t* buffer, size_t size) {
    if (g_sim_ctx && size > 0) {
        g_sim_ctx->appendLog(reinterpret_cast<const char*>(buffer), size);
        // Also send to serial TX buffer (for TCP transmission)
        g_sim_ctx->appendSerialTx(buffer, size);
        return size;
    }
    return 0;
}

// Note: print/println/printf methods are inherited from Print via Stream

// ============================================================================
// Print Class Implementation
// ============================================================================

size_t Print::write(const uint8_t* buffer, size_t size) {
    size_t n = 0;
    for (size_t i = 0; i < size; i++) {
        n += write(buffer[i]);
    }
    return n;
}

size_t Print::print(const char* str) {
    if (!str) return 0;
    return write(reinterpret_cast<const uint8_t*>(str), strlen(str));
}

size_t Print::print(char c) {
    return write(static_cast<uint8_t>(c));
}

size_t Print::print(int n, int base) {
    char buf[34];
    snprintf(buf, sizeof(buf), base == 16 ? "%x" : "%d", n);
    return print(buf);
}

size_t Print::print(unsigned int n, int base) {
    char buf[34];
    snprintf(buf, sizeof(buf), base == 16 ? "%x" : "%u", n);
    return print(buf);
}

size_t Print::print(long n, int base) {
    char buf[34];
    snprintf(buf, sizeof(buf), base == 16 ? "%lx" : "%ld", n);
    return print(buf);
}

size_t Print::print(unsigned long n, int base) {
    char buf[34];
    snprintf(buf, sizeof(buf), base == 16 ? "%lx" : "%lu", n);
    return print(buf);
}

size_t Print::print(double n, int digits) {
    char buf[64];
    snprintf(buf, sizeof(buf), "%.*f", digits, n);
    return print(buf);
}

size_t Print::print(const String& str) {
    return print(str.c_str());
}

size_t Print::println() {
    return print("\n");
}

size_t Print::println(const char* str) {
    return print(str) + println();
}

size_t Print::println(char c) {
    return print(c) + println();
}

size_t Print::println(int n, int base) {
    return print(n, base) + println();
}

size_t Print::println(unsigned int n, int base) {
    return print(n, base) + println();
}

size_t Print::println(long n, int base) {
    return print(n, base) + println();
}

size_t Print::println(unsigned long n, int base) {
    return print(n, base) + println();
}

size_t Print::println(double n, int digits) {
    return print(n, digits) + println();
}

size_t Print::println(const String& str) {
    return print(str) + println();
}

size_t Print::printf(const char* format, ...) {
    char buf[512];
    va_list args;
    va_start(args, format);
    int len = vsnprintf(buf, sizeof(buf), format, args);
    va_end(args);
    
    if (len > 0) {
        return write(reinterpret_cast<const uint8_t*>(buf), len);
    }
    return 0;
}

// ============================================================================
// Stream Implementation
// ============================================================================

size_t Stream::readBytes(uint8_t* buffer, size_t length) {
    size_t count = 0;
    while (count < length) {
        int c = read();
        if (c < 0) break;
        buffer[count++] = static_cast<uint8_t>(c);
    }
    return count;
}

String Stream::readString() {
    String result;
    int c;
    while ((c = read()) >= 0) {
        result += static_cast<char>(c);
    }
    return result;
}

String Stream::readStringUntil(char terminator) {
    String result;
    int c;
    while ((c = read()) >= 0) {
        if (c == terminator) break;
        result += static_cast<char>(c);
    }
    return result;
}

// ============================================================================
// HardwareSerial Implementation
// ============================================================================

void HardwareSerial::begin(unsigned long baud, uint32_t config, int8_t rxPin, int8_t txPin) {
    (void)baud; (void)config; (void)rxPin; (void)txPin;
}

void HardwareSerial::end() {}

int HardwareSerial::available() {
    return Serial.available();
}

int HardwareSerial::read() {
    return Serial.read();
}

int HardwareSerial::peek() {
    return Serial.peek();
}

size_t HardwareSerial::write(uint8_t c) {
    return Serial.write(c);
}

size_t HardwareSerial::write(const uint8_t* buffer, size_t size) {
    return Serial.write(buffer, size);
}

void HardwareSerial::flush() {}

// ============================================================================
// String Implementation
// ============================================================================

String::String(const char* str) : buffer_(nullptr), len_(0), capacity_(0) {
    if (str) {
        len_ = strlen(str);
        ensureCapacity(len_ + 1);
        memcpy(buffer_, str, len_ + 1);
    }
}

String::String(const String& other) : buffer_(nullptr), len_(0), capacity_(0) {
    if (other.len_ > 0) {
        len_ = other.len_;
        ensureCapacity(len_ + 1);
        memcpy(buffer_, other.buffer_, len_ + 1);
    }
}

String::String(String&& other) noexcept 
    : buffer_(other.buffer_), len_(other.len_), capacity_(other.capacity_) {
    other.buffer_ = nullptr;
    other.len_ = 0;
    other.capacity_ = 0;
}

String::~String() {
    delete[] buffer_;
}

String& String::operator=(const String& other) {
    if (this != &other) {
        if (other.len_ > 0) {
            ensureCapacity(other.len_ + 1);
            memcpy(buffer_, other.buffer_, other.len_ + 1);
            len_ = other.len_;
        } else {
            len_ = 0;
            if (buffer_) buffer_[0] = '\0';
        }
    }
    return *this;
}

String& String::operator=(String&& other) noexcept {
    if (this != &other) {
        delete[] buffer_;
        buffer_ = other.buffer_;
        len_ = other.len_;
        capacity_ = other.capacity_;
        other.buffer_ = nullptr;
        other.len_ = 0;
        other.capacity_ = 0;
    }
    return *this;
}

String& String::operator=(const char* str) {
    if (str) {
        len_ = strlen(str);
        ensureCapacity(len_ + 1);
        memcpy(buffer_, str, len_ + 1);
    } else {
        len_ = 0;
        if (buffer_) buffer_[0] = '\0';
    }
    return *this;
}

String& String::operator+=(const String& other) {
    if (other.len_ > 0) {
        ensureCapacity(len_ + other.len_ + 1);
        memcpy(buffer_ + len_, other.buffer_, other.len_ + 1);
        len_ += other.len_;
    }
    return *this;
}

String& String::operator+=(const char* str) {
    if (str) {
        size_t slen = strlen(str);
        ensureCapacity(len_ + slen + 1);
        memcpy(buffer_ + len_, str, slen + 1);
        len_ += slen;
    }
    return *this;
}

String& String::operator+=(char c) {
    ensureCapacity(len_ + 2);
    buffer_[len_++] = c;
    buffer_[len_] = '\0';
    return *this;
}

bool String::operator==(const String& other) const {
    if (len_ != other.len_) return false;
    if (len_ == 0) return true;
    return memcmp(buffer_, other.buffer_, len_) == 0;
}

bool String::operator==(const char* str) const {
    if (!str) return len_ == 0;
    return strcmp(c_str(), str) == 0;
}

char String::operator[](unsigned int index) const {
    if (index >= len_) return '\0';
    return buffer_[index];
}

char& String::operator[](unsigned int index) {
    static char dummy = '\0';
    if (index >= len_) return dummy;
    return buffer_[index];
}

void String::reserve(unsigned int size) {
    ensureCapacity(size);
}

int String::indexOf(char c) const {
    if (!buffer_) return -1;
    for (unsigned int i = 0; i < len_; i++) {
        if (buffer_[i] == c) return i;
    }
    return -1;
}

int String::indexOf(const char* str) const {
    if (!buffer_ || !str) return -1;
    const char* found = strstr(buffer_, str);
    return found ? static_cast<int>(found - buffer_) : -1;
}

String String::substring(unsigned int from, unsigned int to) const {
    if (from >= len_) return String();
    if (to > len_) to = len_;
    if (from >= to) return String();
    
    String result;
    result.len_ = to - from;
    result.ensureCapacity(result.len_ + 1);
    memcpy(result.buffer_, buffer_ + from, result.len_);
    result.buffer_[result.len_] = '\0';
    return result;
}

void String::trim() {
    if (!buffer_ || len_ == 0) return;
    
    unsigned int start = 0;
    while (start < len_ && isspace(buffer_[start])) start++;
    
    unsigned int end = len_;
    while (end > start && isspace(buffer_[end - 1])) end--;
    
    if (start > 0 || end < len_) {
        len_ = end - start;
        memmove(buffer_, buffer_ + start, len_);
        buffer_[len_] = '\0';
    }
}

void String::toLowerCase() {
    if (!buffer_) return;
    for (unsigned int i = 0; i < len_; i++) {
        buffer_[i] = tolower(buffer_[i]);
    }
}

void String::toUpperCase() {
    if (!buffer_) return;
    for (unsigned int i = 0; i < len_; i++) {
        buffer_[i] = toupper(buffer_[i]);
    }
}

long String::toInt() const {
    return buffer_ ? atol(buffer_) : 0;
}

float String::toFloat() const {
    return buffer_ ? static_cast<float>(atof(buffer_)) : 0.0f;
}

void String::ensureCapacity(unsigned int cap) {
    if (cap <= capacity_) return;
    
    unsigned int newCap = capacity_ ? capacity_ * 2 : 16;
    while (newCap < cap) newCap *= 2;
    
    char* newBuf = new char[newCap];
    if (buffer_) {
        memcpy(newBuf, buffer_, len_ + 1);
        delete[] buffer_;
    } else {
        newBuf[0] = '\0';
    }
    buffer_ = newBuf;
    capacity_ = newCap;
}

String operator+(const String& lhs, const String& rhs) {
    String result = lhs;
    result += rhs;
    return result;
}

String operator+(const String& lhs, const char* rhs) {
    String result = lhs;
    result += rhs;
    return result;
}

String operator+(const char* lhs, const String& rhs) {
    String result(lhs);
    result += rhs;
    return result;
}
