// `input.h`'tan önce include edilmesi gereken header (ArduPilot'taki `SCurve.h`'ın
// `AP_Math.h`'a ihtiyacı gibi). Buradaki bildirimler IR'a girmemeli.
#pragma once

#include <cstdint>

struct Vec2 {
    float x;
    float y;
};

int prelude_only(int x);
