#include "sim_filesystem.h"
#include "sim_context.h"
#include "SPIFFS.h"

// Thread-local SPIFFS instance
// Each simulation thread gets its own SPIFFS instance to ensure filesystem isolation
// between nodes running in the same DLL
thread_local SPIFFSClass SPIFFS;

// Helper to get the filesystem from context
SimFilesystem& getSimFilesystem() {
    return g_sim_ctx->filesystem;
}

std::string SimFilesystem::normalizePath(const char* path) {
    std::string p(path);
    // Remove leading slash for internal storage
    while (!p.empty() && p[0] == '/') {
        p = p.substr(1);
    }
    return p;
}

bool SimFilesystem::exists(const char* path) {
    std::lock_guard<std::mutex> lock(mutex_);
    std::string normalized = normalizePath(path);
    return files_.find(normalized) != files_.end();
}

bool SimFilesystem::remove(const char* path) {
    std::lock_guard<std::mutex> lock(mutex_);
    std::string normalized = normalizePath(path);
    auto it = files_.find(normalized);
    if (it != files_.end()) {
        files_.erase(it);
        return true;
    }
    return false;
}

SimFile* SimFilesystem::openRead(const char* path) {
    std::lock_guard<std::mutex> lock(mutex_);
    std::string normalized = normalizePath(path);
    
    auto it = files_.find(normalized);
    if (it == files_.end()) {
        return nullptr;
    }
    
    SimFile* file = new SimFile();
    file->data = it->second;
    file->position_ = 0;
    open_files_[file] = normalized;
    return file;
}

SimFile* SimFilesystem::openWrite(const char* path) {
    std::lock_guard<std::mutex> lock(mutex_);
    std::string normalized = normalizePath(path);
    
    // Create or truncate
    files_[normalized].clear();
    
    SimFile* file = new SimFile();
    file->position_ = 0;
    open_files_[file] = normalized;
    return file;
}

SimFile* SimFilesystem::openAppend(const char* path) {
    std::lock_guard<std::mutex> lock(mutex_);
    std::string normalized = normalizePath(path);
    
    // Create if doesn't exist
    auto& data = files_[normalized];
    
    SimFile* file = new SimFile();
    file->data = data;
    file->position_ = file->data.size();
    open_files_[file] = normalized;
    return file;
}

void SimFilesystem::close(SimFile* file) {
    if (!file) return;
    
    std::lock_guard<std::mutex> lock(mutex_);
    
    auto it = open_files_.find(file);
    if (it != open_files_.end()) {
        // Write back to storage
        files_[it->second] = file->data;
        open_files_.erase(it);
    }
    
    delete file;
}

int SimFilesystem::writeFile(const char* path, const uint8_t* data, size_t len) {
    std::lock_guard<std::mutex> lock(mutex_);
    std::string normalized = normalizePath(path);
    
    files_[normalized].assign(data, data + len);
    return static_cast<int>(len);
}

int SimFilesystem::readFile(const char* path, uint8_t* data, size_t max_len) {
    std::lock_guard<std::mutex> lock(mutex_);
    std::string normalized = normalizePath(path);
    
    auto it = files_.find(normalized);
    if (it == files_.end()) {
        return -1;
    }
    
    size_t len = (std::min)(it->second.size(), max_len);
    memcpy(data, it->second.data(), len);
    return static_cast<int>(len);
}

void SimFilesystem::clear() {
    std::lock_guard<std::mutex> lock(mutex_);
    files_.clear();
    // Note: This doesn't handle open file handles - they become invalid
}
