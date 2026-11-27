// `[library] exceptions = false`: hiçbiri `noexcept` değil ama kütüphane exception
// fırlatmayacağını söylüyor (ArduPilot gibi `-fno-exceptions` ile derlenen kodlar).
#pragma once

#include <cstdint>

namespace calc {

int32_t add(int32_t a, int32_t b);

struct Money {
    int64_t cents;
    Money operator+(const Money& o) const { return Money{cents + o.cents}; }
};

// Slice: ferrobridge'in uzunluk kontrolü yine `Result` döner.
uint32_t total(const uint8_t* bytes, uint8_t len);

// Sözü bozar: negatif girdi için fırlatır. Rust'tan çağrılınca program sonlanmalı.
int32_t checked_sqrt(int32_t x);

}  // namespace calc
