#include "classes/input.h"

#include <cmath>
#include <stdexcept>

namespace geo {

Vehicle::Vehicle(int wheels) : Vehicle(wheels, 0.0) {}
Vehicle::Vehicle(int wheels, double odometer)
    : public_counter(0), wheels_(wheels), odometer_(odometer), position_{0, 0}, home_{1, 2} {}
Vehicle::~Vehicle() = default;

int Vehicle::wheels() const { return wheels_; }
double Vehicle::odometer() const noexcept { return odometer_; }
void Vehicle::drive(double km) {
  odometer_ += km;
  position_.x += static_cast<float>(km);
}
int Vehicle::honk() const { return wheels_ * 10; }
int Vehicle::max_wheels() { return 18; }

void Vehicle::break_down() { throw std::runtime_error("engine failure"); }
void Vehicle::throw_unknown() { throw 42; }

Point Vehicle::position() const { return position_; }
const Point& Vehicle::home() const { return home_; }
void Vehicle::move_to(const Point& target) { position_ = target; }
void Vehicle::move_to(float x, float y) { position_ = Point{x, y}; }

bool Vehicle::operator==(const Vehicle& other) const { return wheels_ == other.wheels_; }
long Vehicle::total_km() const { return static_cast<long>(odometer_); }
void* Vehicle::raw_handle() { return this; }
void Vehicle::read_odometer(double& out) const { out = odometer_; }

Square::Square(double side) : side_(side) {}
double Square::area() const { return side_ * side_; }

std::unique_ptr<Shape> make_unit_square() { return std::make_unique<Square>(1.0); }
int clamp(int value, int lo, int hi) { return value < lo ? lo : (value > hi ? hi : value); }
double clamp(double value, double lo, double hi) {
  return value < lo ? lo : (value > hi ? hi : value);
}
double length(const Point& p) { return std::sqrt(p.x * p.x + p.y * p.y); }

}  // namespace geo

namespace physics {
double length(double meters) { return meters; }
}  // namespace physics
