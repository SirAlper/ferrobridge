#pragma once

#include <memory>

namespace geo {

// Its fields are public but it has no constructor: in Rust it can only be used by reference.
struct Point {
  float x;
  float y;
};

class Vehicle {
 public:
  explicit Vehicle(int wheels);
  Vehicle(int wheels, double odometer);
  Vehicle(const Vehicle&) = delete;
  Vehicle& operator=(const Vehicle&) = delete;
  virtual ~Vehicle();

  int wheels() const;
  double odometer() const noexcept;
  void drive(double km);
  virtual int honk() const;
  static int max_wheels();

  // Throws an exception: becomes Err in Rust.
  void break_down();
  void throw_unknown();

  // A class returned by value: wrapped in a unique_ptr in the shim.
  Point position() const;
  const Point& home() const;
  void move_to(const Point& target);
  void move_to(float x, float y);

  // Not supported in this version.
  bool operator==(const Vehicle& other) const;
  long total_km() const;
  void* raw_handle();
  void read_odometer(double& out) const;
  int public_counter;

 private:
  int wheels_;
  double odometer_;
  Point position_;
  Point home_;
};

class Shape {
 public:
  virtual ~Shape() = default;
  virtual double area() const = 0;
};

class Square : public Shape {
 public:
  explicit Square(double side);
  double area() const override;

 private:
  double side_;
};

std::unique_ptr<Shape> make_unit_square();
int clamp(int value, int lo, int hi);
double clamp(double value, double lo, double hi);
double length(const Point& p);

}  // namespace geo

// The same name, a different namespace: must not clash on the Rust side.
namespace physics {
double length(double meters);
}  // namespace physics
