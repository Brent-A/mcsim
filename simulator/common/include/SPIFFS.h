#pragma once

// ============================================================================
// SPIFFS Stub for Simulation
// ============================================================================
// Wraps the SimFilesystem to provide SPIFFS-like API

#include "sim_filesystem.h"
#include "Arduino.h"  // For Stream base class
#include <cstdint>

// Forward declare fs::FS for inheritance
namespace fs {
class FS;
}

// Forward declare the filesystem access
struct SimContext;
extern thread_local SimContext* g_sim_ctx;
SimFilesystem& getSimFilesystem();

// File wrapper - inherits from Stream so it can be passed to readFrom/writeTo
class File : public Stream {
public:
    File() : fs_(nullptr), file_(nullptr), path_() {}
    File(SimFilesystem* fs, SimFile* file, const char* path) 
        : fs_(fs), file_(file), path_(path ? path : "") {}
    
    ~File() {
        close();
    }
    
    // Move semantics
    File(File&& other) noexcept 
        : fs_(other.fs_), file_(other.file_), path_(std::move(other.path_)) {
        other.fs_ = nullptr;
        other.file_ = nullptr;
    }
    
    File& operator=(File&& other) noexcept {
        if (this != &other) {
            close();
            fs_ = other.fs_;
            file_ = other.file_;
            path_ = std::move(other.path_);
            other.fs_ = nullptr;
            other.file_ = nullptr;
        }
        return *this;
    }
    
    // No copy
    File(const File&) = delete;
    File& operator=(const File&) = delete;
    
    operator bool() const { return file_ != nullptr; }
    
    size_t size() const {
        return file_ ? file_->size() : 0;
    }
    
    // Stream interface - read
    int available() override {
        if (!file_) return 0;
        return static_cast<int>(file_->size() - file_->position_);
    }
    
    int read() override {
        uint8_t c;
        if (read(&c, 1) == 1) return c;
        return -1;
    }
    
    int peek() override {
        if (!file_) return -1;
        size_t pos = file_->position_;
        int c = read();
        file_->seek(pos);
        return c;
    }
    
    size_t read(uint8_t* buf, size_t size) {
        return file_ ? file_->read(buf, size) : 0;
    }
    
    size_t readBytes(char* buf, size_t size) {
        return read(reinterpret_cast<uint8_t*>(buf), size);
    }
    
    // Stream interface - write  
    size_t write(uint8_t c) override {
        return file_ ? file_->write(&c, 1) : 0;
    }
    
    size_t write(const uint8_t* buf, size_t size) override {
        return file_ ? file_->write(buf, size) : 0;
    }
    
    void flush() {
        // No-op in simulation
    }
    
    void seek(size_t pos) {
        if (file_) file_->seek(pos);
    }
    
    size_t position() const {
        return file_ ? file_->position_ : 0;
    }
    
    void close() {
        if (fs_ && file_) {
            fs_->close(file_);
            file_ = nullptr;
        }
    }
    
    const char* name() const {
        return path_.c_str();
    }
    
    // Directory iteration support (for companion_radio)
    File openNextFile() {
        // In SPIFFS, there aren't real directories, so this is mostly a no-op
        // Return an invalid file to signal end of iteration
        return File();
    }
    
    bool isDirectory() const {
        // SPIFFS doesn't have real directories
        return false;
    }

private:
    SimFilesystem* fs_;
    SimFile* file_;
    std::string path_;
};

// Forward declare fs::FS for inheritance
namespace fs {
class FS {
public:
    virtual ~FS() = default;
    virtual File open(const char* path, const char* mode = "r", bool create = false) = 0;
    virtual bool exists(const char* path) = 0;
    virtual bool remove(const char* path) = 0;
    virtual bool mkdir(const char* path) = 0;
    virtual bool rmdir(const char* path) { (void)path; return true; }
    virtual bool format() { return true; }
};
}

// SPIFFS class - inherits from fs::FS for compatibility with firmware code
class SPIFFSClass : public fs::FS {
public:
    bool begin(bool formatOnFail = false) {
        (void)formatOnFail;
        return getSimFilesystem().begin();
    }
    
    void end() {
        getSimFilesystem().end();
    }
    
    // Support all open variants (override from fs::FS)
    File open(const char* path, const char* mode = "r", bool create = false) override {
        (void)create;  // We always create on write
        SimFilesystem& fs = getSimFilesystem();
        SimFile* file = nullptr;
        
        if (strcmp(mode, "r") == 0) {
            file = fs.openRead(path);
        } else if (strcmp(mode, "w") == 0) {
            file = fs.openWrite(path);
        } else if (strcmp(mode, "a") == 0) {
            file = fs.openAppend(path);
        }
        
        return File(&fs, file, path);
    }
    
    bool exists(const char* path) override {
        return getSimFilesystem().exists(path);
    }
    
    bool remove(const char* path) override {
        return getSimFilesystem().remove(path);
    }
    
    bool mkdir(const char* path) override {
        (void)path;
        return true;  // Directories are implicit
    }
    
    bool format() override {
        // Clear all files in the simulated filesystem
        getSimFilesystem().format();
        return true;
    }
    
    // Storage info methods for companion_radio DataStore
    size_t usedBytes() const {
        return getSimFilesystem().usedBytes();
    }
    
    size_t totalBytes() const {
        return getSimFilesystem().totalBytes();
    }
};

// Thread-local SPIFFS instance - each simulation thread gets its own
// This ensures filesystem isolation between nodes running in the same DLL
extern thread_local SPIFFSClass SPIFFS;

// Define fs::SPIFFSFS for compatibility with firmware code
namespace fs {
    using SPIFFSFS = SPIFFSClass;
}

// Define FILESYSTEM type for firmware compatibility
// Only define if not already defined by another header
#ifndef FILESYSTEM
#define FILESYSTEM SPIFFSClass
#endif
