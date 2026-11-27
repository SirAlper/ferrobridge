#include "multi_header/input.h"

#include <cmath>

namespace geo {

Point::Point(double x, double y) : x_(x), y_(y) {}
double Point::x() const { return x_; }
double Point::y() const { return y_; }

double distance(const Point& a, const Point& b) { return std::hypot(a.x() - b.x(), a.y() - b.y()); }
Axis dominant(const Point& p) { return std::abs(p.x()) >= std::abs(p.y()) ? Axis::X : Axis::Y; }
double in_meters(const units::Meters& m) { return m.value(); }

}  // namespace geo

namespace units {
Meters::Meters(double value) : value_(value) {}
double Meters::value() const { return value_; }
}  // namespace units
