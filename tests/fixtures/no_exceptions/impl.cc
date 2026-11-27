#include "no_exceptions/input.h"

#include <cmath>
#include <stdexcept>

namespace calc {

int32_t add(int32_t a, int32_t b) { return a + b; }

uint32_t total(const uint8_t* bytes, uint8_t len) {
    uint32_t sum = 0;
    for (uint8_t i = 0; i < len; ++i) {
        sum += bytes[i];
    }
    return sum;
}

int32_t checked_sqrt(int32_t x) {
    if (x < 0) {
        throw std::domain_error("negatif sayının karekökü");
    }
    return static_cast<int32_t>(std::sqrt(x));
}

}  // namespace calc
