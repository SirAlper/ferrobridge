#pragma once

#include <memory>

namespace geo {

// Alanları public ama constructor'ı yok: Rust'ta yalnızca referansla kullanılabilir.
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

  // Exception fırlatır: Rust'ta Err olur.
  void break_down();
  void throw_unknown();

  // Değerle dönen sınıf: shim'de unique_ptr'a sarılır.
  Point position() const;
  const Point& home() const;
  void move_to(const Point& target);
  void move_to(float x, float y);

  // Bu sürümde desteklenmeyenler.
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

// Aynı isim, farklı namespace: Rust tarafında çakışmamalı.
namespace physics {
double length(double meters);
}  // namespace physics
