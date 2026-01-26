#pragma once

#include <string>
#include <map>
#include <vector>
#include <mutex>
#include <cstdint>
#include <cstring>

// ============================================================================
// Simulated In-Memory Filesystem
// ============================================================================
// Provides a simple in-memory filesystem that persists across node reboots.
// The coordinator can pre-populate files or inspect them.

class SimFile {
public:
    SimFile() : position_(0) {}

    std::vector<uint8_t> data;
    size_t position_;

    size_t size() const { return data.size(); }

    void seek(size_t pos) {
        position_ = (pos <= data.size()) ? pos : data.size();
    }

    size_t read(uint8_t* buffer, size_t len) {
        size_t avail = data.size() - position_;
        size_t to_read = (len < avail) ? len : avail;
        if (to_read > 0) {
            memcpy(buffer, data.data() + position_, to_read);
            position_ += to_read;
        }
        return to_read;
    }

    size_t write(const uint8_t* buffer, size_t len) {
        // Extend if needed
        if (position_ + len > data.size()) {
            data.resize(position_ + len);
        }
        memcpy(data.data() + position_, buffer, len);
        position_ += len;
        return len;
    }
};

class SimFilesystem {
public:
    SimFilesystem() : mounted_(false) {}

    // Mount/unmount
    bool begin() { mounted_ = true; return true; }
    void end() { mounted_ = false; }
    bool isMounted() const { return mounted_; }

    // File operations
    bool exists(const char* path);
    bool remove(const char* path);

    // Open for reading (returns nullptr if not found)
    SimFile* openRead(const char* path);

    // Open for writing (creates if doesn't exist, truncates if exists)
    SimFile* openWrite(const char* path);

    // Open for appending
    SimFile* openAppend(const char* path);

    // Close file handle
    void close(SimFile* file);

    // Direct access for coordinator
    int writeFile(const char* path, const uint8_t* data, size_t len);
    int readFile(const char* path, uint8_t* data, size_t max_len);

    // Clear all files (for testing)
    void clear();

    // Format filesystem (alias for clear)
    void format() { clear(); }

    // Storage info
    size_t usedBytes() const {
        size_t total = 0;
        for (const auto& kv : files_) {
            total += kv.second.size();
        }
        return total;
    }

    size_t totalBytes() const {
        // Simulated filesystem has unlimited space (4MB as a placeholder)
        return 4 * 1024 * 1024;
    }

private:
    bool mounted_;
    std::map<std::string, std::vector<uint8_t>> files_;
    std::map<SimFile*, std::string> open_files_;  // Track which file each handle refers to
    std::mutex mutex_;

    std::string normalizePath(const char* path);
};
