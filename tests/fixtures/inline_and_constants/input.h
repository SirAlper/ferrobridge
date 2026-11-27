// `static inline` fonksiyonlar ve sabitler (ArduPilot `AP_Math.h` ve `definitions.h` gibi).
#pragma once

#include <cmath>
#include <cstdint>

// Tanımı header'da olan iç bağlantılı fonksiyonlar bağlanır: header'ı include eden shim'in
// çeviri biriminde tanımlıdırlar.
static inline int add_one(int x) { return x + 1; }
static inline constexpr float half(float x) { return x * 0.5f; }
[[maybe_unused]] static double triple(double x) { return 3 * x; }

// Tanımı görünmeyen iç bağlantılı fonksiyon bağlanamaz.
[[maybe_unused]] static int hidden(int x);

namespace {
// Anonim namespace'teki fonksiyonun Rust'tan yazılabilecek bir adı yok.
inline int anonymous(int x) { return x; }
}

// Sonlu ve derleme anında kesin değeri olan sabitler.
static const double WGS84_A = 6378137.0;
static const double WGS84_IF = 298.257223563;
static const double WGS84_F = ((double)1.0 / WGS84_IF);
static const float GRAVITY_MSS = 9.80665f;
static const float TENTH = 0.1;
constexpr int32_t MAX_WHEELS = 8;
constexpr int8_t OFFSET = -5;
const uint64_t ALL_BITS = 0xFFFFFFFFFFFFFFFFull;
constexpr bool ENABLED = true;
constexpr char LETTER = 'A';

namespace units {
constexpr double FEET_PER_METER = 3.28084;
}

// Başlatıcısında fonksiyon çağrısı olan sabitler: değer çalışma anında libm ile hesaplanabilir,
// derleme anında hesaplanan değerle aynı olduğu kesin değil.
static const double ROOT_TWO = sqrt(2.0);
static const double TWO_ROOT_TWO = 2 * ROOT_TWO;
static const float QUIET_NAN = nanf("0x4152");

// Sonlu olmayan değer: `f64::INFINITY` yazılabilirdi, ama NaN'larla aynı kural uygulanır.
static const double TOO_BIG = 1e308 * 10;

// Sabit olmayan, değeri olmayan ya da sayı olmayan değişkenler.
inline int counter = 0;  // `inline`: header birden fazla çeviri birimine girer
extern const int EXTERNAL;
const char* const NAME = "x";

// Sabitlerin C++'ta çalışma anındaki değerleri: e2e testi Rust `const`'larının bunlarla bit bit
// aynı olduğunu doğrular.
double wgs84_f_at_runtime();
float tenth_at_runtime();
