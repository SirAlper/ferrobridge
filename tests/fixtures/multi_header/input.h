// Başka bir header'da (`geometry.h`) tanımlı tipleri kullanan fonksiyonlar.
#pragma once

#include "geometry.h"
#include "outside/units.h"

namespace geo {

double distance(const Point& a, const Point& b);
Axis dominant(const Point& p);
double in_meters(const units::Meters& m);

}  // namespace geo
