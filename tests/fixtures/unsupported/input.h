#pragma once

struct Point {
  float x;
  float y;
};

class Vehicle {
 public:
  void drive();
};

enum class Mode { Manual, Auto };

template <typename T>
T identity(T value);

extern int global_counter;

// Fonksiyon desteklenir ama Point tipi henüz desteklenmez.
void move_to(Point target);

int sum(int count, ...);
