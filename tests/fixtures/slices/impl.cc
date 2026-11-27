#include "slices/input.h"

namespace buf {

uint8_t crc8(const uint8_t* p, uint8_t len) {
    uint8_t crc = 0;
    for (uint8_t i = 0; i < len; ++i) {
        crc ^= p[i];
        for (int bit = 0; bit < 8; ++bit) {
            crc = (crc & 0x80) ? static_cast<uint8_t>((crc << 1) ^ 0x07) : static_cast<uint8_t>(crc << 1);
        }
    }
    return crc;
}

double sum(std::size_t n, const double* values) {
    double total = 0;
    for (std::size_t i = 0; i < n; ++i) {
        total += values[i];
    }
    return total;
}

void scale(float* data, uint32_t count, float factor) {
    for (uint32_t i = 0; i < count; ++i) {
        data[i] *= factor;
    }
}

int64_t dot(const int32_t* a, int32_t a_len, const int32_t* b, int32_t b_len) {
    int32_t n = a_len < b_len ? a_len : b_len;
    int64_t total = 0;
    for (int32_t i = 0; i < n; ++i) {
        total += static_cast<int64_t>(a[i]) * b[i];
    }
    return total;
}

uint16_t count_nonzero(const uint8_t* bytes, uint16_t size) noexcept {
    uint16_t count = 0;
    for (uint16_t i = 0; i < size; ++i) {
        count += bytes[i] != 0 ? 1 : 0;
    }
    return count;
}

uint8_t first(const uint8_t* p, uint8_t len) { return len > 0 ? p[0] : 0; }

Buffer::Buffer() = default;

void Buffer::append(const uint8_t*, uint32_t len) { total_ += len; }

uint32_t Buffer::total() const { return total_; }

}  // namespace buf
