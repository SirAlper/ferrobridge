#pragma once

// Basit serbest fonksiyonlar.
int add(int a, int b);
void reset();
double scale(double value, float factor) noexcept;
bool is_ready(int) noexcept(false);

// Overload: aynı isim, farklı parametreler.
int clamp(int value, int lo, int hi);
double clamp(double value, double lo, double hi);

// Header içinde tanımlı fonksiyon.
inline int twice(int x) { return x * 2; }
