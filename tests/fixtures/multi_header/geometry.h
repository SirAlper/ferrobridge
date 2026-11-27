// Bridge'in iki header'ından biri: tipler burada tanımlı, `input.h`'taki fonksiyonlar kullanır.
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
