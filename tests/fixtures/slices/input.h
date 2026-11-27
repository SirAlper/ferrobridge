// Pointer + length pairs: with the `{ slice = "..." }` annotation in `ferrobridge.toml` they
// become a single `&[T]` / `&mut [T]` in Rust.
#pragma once

#include <cstddef>
#include <cstdint>

namespace buf {

// CRC-8 (polynomial 0x07, initial value 0). The length is `uint8_t`: a slice longer than 255 must be `Err`.
uint8_t crc8(const uint8_t* p, uint8_t len);

// The length comes first and is `size_t` (never visible to Rust).
double sum(std::size_t n, const double* values);

// Changes the array in place: `&mut [f32]`.
void scale(float* data, uint32_t count, float factor);

// Two separate slices; the lengths are signed.
int64_t dot(const int32_t* a, int32_t a_len, const int32_t* b, int32_t b_len);

// `noexcept`, but still `Result` in Rust because the length check can throw.
uint16_t count_nonzero(const uint8_t* bytes, uint16_t size) noexcept;

// Not annotated: whether the pointer is an array or a single object is unknown, not bound.
uint8_t first(const uint8_t* p, uint8_t len);

class Buffer {
public:
    Buffer();
    void append(const uint8_t* data, uint32_t len);
    uint32_t total() const;

private:
    uint32_t total_ = 0;
};

}  // namespace buf
