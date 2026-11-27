// A header that includes itself indirectly (like the `vectorN.h` -> `AP_Math.h` ->
// `vectorN.h` cycle in ArduPilot). Read as the main file, `#pragma once` does not work and `Counter`
// is defined twice.
#pragma once

#include "helper.h"

class Counter {
public:
    Counter();
    int value() const;
};

int twice(int x);
