// Pointer + uzunluk çiftleri: `ferrobridge.toml`'daki `{ slice = "..." }` açıklamasıyla Rust'ta
// tek bir `&[T]` / `&mut [T]` olurlar.
#pragma once

#include <cstddef>
#include <cstdint>

namespace buf {

// CRC-8 (polinom 0x07, başlangıç 0). Uzunluk `uint8_t`: 255'ten uzun slice `Err` olmalı.
uint8_t crc8(const uint8_t* p, uint8_t len);

// Uzunluk önce geliyor ve `size_t` (Rust'a hiç görünmüyor).
double sum(std::size_t n, const double* values);

// Diziyi yerinde değiştirir: `&mut [f32]`.
void scale(float* data, uint32_t count, float factor);

// İki ayrı slice; uzunluklar işaretli.
int64_t dot(const int32_t* a, int32_t a_len, const int32_t* b, int32_t b_len);

// `noexcept`, ama uzunluk kontrolü exception fırlatabildiği için Rust'ta yine `Result`.
uint16_t count_nonzero(const uint8_t* bytes, uint16_t size) noexcept;

// Açıklamasız: pointer'ın dizi mi tek nesne mi olduğu bilinmiyor, bağlanmaz.
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
