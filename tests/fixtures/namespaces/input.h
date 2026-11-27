#pragma once

namespace geo {
float length(float x, float y);

namespace detail {
float hypot2(float x, float y);
}  // namespace detail
}  // namespace geo

// The same name, a different namespace.
namespace physics {
float length(float meters);
}  // namespace physics

extern "C" {
int c_api_version();
}

// Functions that cannot be called from outside.
namespace {
int hidden();
}
static int internal_helper();
