// `[library] exceptions = false`: none of these is `noexcept`, but the library says it does not
// throw (like code compiled with `-fno-exceptions`, such as ArduPilot).
#pragma once

#include <cstdint>

namespace calc {

int32_t add(int32_t a, int32_t b);

struct Money {
    int64_t cents;
    Money operator+(const Money& o) const { return Money{cents + o.cents}; }
};

// A slice: ferrobridge's length check still returns `Result`.
uint32_t total(const uint8_t* bytes, uint8_t len);

// Breaks the promise: throws for a negative input. Called from Rust, the program must end.
int32_t checked_sqrt(int32_t x);

}  // namespace calc
