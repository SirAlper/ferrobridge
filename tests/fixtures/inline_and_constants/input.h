// `static inline` functions and constants (like ArduPilot's `AP_Math.h` and `definitions.h`).
#pragma once

#include <cmath>
#include <cstdint>

// Internal-linkage functions defined in the header are bound: they are defined in the translation
// unit of the shim that includes the header.
static inline int add_one(int x) { return x + 1; }
static inline constexpr float half(float x) { return x * 0.5f; }
[[maybe_unused]] static double triple(double x) { return 3 * x; }

// An internal-linkage function whose definition is not visible cannot be bound.
[[maybe_unused]] static int hidden(int x);

namespace {
// A function in an anonymous namespace has no name that can be written from Rust.
inline int anonymous(int x) { return x; }
}

// Constants that are finite and have an exact value at compile time.
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

// Constants with a function call in the initializer: the value may be computed at run time with
// libm, and it is not certain to be the same as the value computed at compile time.
static const double ROOT_TWO = sqrt(2.0);
static const double TWO_ROOT_TWO = 2 * ROOT_TWO;
static const float QUIET_NAN = nanf("0x4152");

// A non-finite value: `f64::INFINITY` could be written, but the same rule as for NaNs applies.
static const double TOO_BIG = 1e308 * 10;

// Variables that are not constant, have no value or are not numbers.
inline int counter = 0;  // `inline`: the header enters more than one translation unit
extern const int EXTERNAL;
const char* const NAME = "x";

// The run-time values of the constants in C++: the e2e test checks that the Rust `const`s are
// bit-for-bit identical to them.
double wgs84_f_at_runtime();
float tenth_at_runtime();
