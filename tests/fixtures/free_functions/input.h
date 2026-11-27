#pragma once

// Simple free functions.
int add(int a, int b);
void reset();
double scale(double value, float factor) noexcept;
bool is_ready(int) noexcept(false);

// Overloads: the same name, different parameters.
int clamp(int value, int lo, int hi);
double clamp(double value, double lo, double hi);

// A function defined in the header.
inline int twice(int x) { return x * 2; }
