// A header that must be included before `input.h` (like ArduPilot's `SCurve.h` needing
// `AP_Math.h`). The declarations here must not enter the IR.
#pragma once

#include <cstdint>

struct Vec2 {
    float x;
    float y;
};

int prelude_only(int x);
