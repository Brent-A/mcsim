#pragma once

// ============================================================================
// Base64 Stub for Simulation
// ============================================================================
// Simple base64 encode/decode implementation

#include <cstdint>
#include <cstring>

// Base64 alphabet
static const char base64_chars[] = 
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

inline int base64_char_value(char c) {
    if (c >= 'A' && c <= 'Z') return c - 'A';
    if (c >= 'a' && c <= 'z') return c - 'a' + 26;
    if (c >= '0' && c <= '9') return c - '0' + 52;
    if (c == '+') return 62;
    if (c == '/') return 63;
    return -1;
}

// Decode base64 string into binary buffer
// Returns number of decoded bytes
inline int decode_base64(const unsigned char* input, size_t input_len, unsigned char* output) {
    int out_idx = 0;
    int value = 0;
    int bits = 0;
    
    for (size_t i = 0; i < input_len; i++) {
        char c = (char)input[i];
        if (c == '=' || c == '\0') break;  // Padding or end
        if (c == ' ' || c == '\n' || c == '\r') continue;  // Skip whitespace
        
        int v = base64_char_value(c);
        if (v < 0) continue;  // Skip invalid chars
        
        value = (value << 6) | v;
        bits += 6;
        
        if (bits >= 8) {
            bits -= 8;
            output[out_idx++] = (value >> bits) & 0xFF;
        }
    }
    
    return out_idx;
}

// Encode binary buffer into base64 string
// Returns length of encoded string (not including null terminator)
inline size_t encode_base64(const unsigned char* input, size_t input_len, char* output) {
    size_t out_idx = 0;
    int value = 0;
    int bits = 0;
    
    for (size_t i = 0; i < input_len; i++) {
        value = (value << 8) | input[i];
        bits += 8;
        
        while (bits >= 6) {
            bits -= 6;
            output[out_idx++] = base64_chars[(value >> bits) & 0x3F];
        }
    }
    
    // Handle remaining bits
    if (bits > 0) {
        value <<= (6 - bits);
        output[out_idx++] = base64_chars[value & 0x3F];
    }
    
    // Add padding
    while (out_idx % 4 != 0) {
        output[out_idx++] = '=';
    }
    
    output[out_idx] = '\0';
    return out_idx;
}
