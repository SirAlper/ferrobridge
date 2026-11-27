// A header that does not compile on its own: needs `prelude.h` for `Vec2` and `uint8_t`.
#pragma once

float length(const Vec2& v);
uint8_t checksum(uint8_t seed);
