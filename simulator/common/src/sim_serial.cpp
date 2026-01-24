#include "sim_serial.h"

void SimSerial::injectRx(const uint8_t* data, size_t len) {
    std::lock_guard<std::mutex> lock(rx_mutex_);
    for (size_t i = 0; i < len; i++) {
        rx_queue_.push(data[i]);
    }
}

size_t SimSerial::available() {
    std::lock_guard<std::mutex> lock(rx_mutex_);
    return rx_queue_.size();
}

int SimSerial::read() {
    std::lock_guard<std::mutex> lock(rx_mutex_);
    if (rx_queue_.empty()) {
        return -1;
    }
    uint8_t byte = rx_queue_.front();
    rx_queue_.pop();
    return byte;
}

size_t SimSerial::readBytes(uint8_t* buffer, size_t len) {
    std::lock_guard<std::mutex> lock(rx_mutex_);
    size_t count = 0;
    while (count < len && !rx_queue_.empty()) {
        buffer[count++] = rx_queue_.front();
        rx_queue_.pop();
    }
    return count;
}

void SimSerial::write(uint8_t byte) {
    std::lock_guard<std::mutex> lock(tx_mutex_);
    tx_buffer_.push_back(byte);
}

void SimSerial::write(const uint8_t* data, size_t len) {
    std::lock_guard<std::mutex> lock(tx_mutex_);
    tx_buffer_.insert(tx_buffer_.end(), data, data + len);
}

size_t SimSerial::collectTx(uint8_t* buffer, size_t max_len) {
    std::lock_guard<std::mutex> lock(tx_mutex_);
    size_t len = (std::min)(tx_buffer_.size(), max_len);
    if (len > 0) {
        memcpy(buffer, tx_buffer_.data(), len);
        tx_buffer_.erase(tx_buffer_.begin(), tx_buffer_.begin() + len);
    }
    return len;
}
