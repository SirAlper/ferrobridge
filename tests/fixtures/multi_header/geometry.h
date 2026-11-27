// One of the bridge's two headers: the types are defined here, and the functions in `input.h` use them.
#pragma once

namespace geo {

class Point {
public:
    Point(double x, double y);
    double x() const;
    double y() const;

private:
    double x_;
    double y_;
};

enum class Axis { X, Y };

}  // namespace geo
