// Kendini dolaylı olarak include eden header (ArduPilot'taki `vectorN.h` -> `AP_Math.h` ->
// `vectorN.h` döngüsü gibi). Ana dosya olarak okunursa `#pragma once` işlemez ve `Counter`
// iki kez tanımlanır.
#pragma once

#include "helper.h"

class Counter {
public:
    Counter();
    int value() const;
};

int twice(int x);
