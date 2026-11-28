// Class templates: instantiated with concrete types through `[templates]` (like `Vector3<T>` in
// ArduPilot). Members are read from the pattern, with `T` replaced by the instantiation's argument.
#pragma once

#include <cstdint>

namespace geo {

template <typename T> class Vec2;

template <typename T>
class Vec3 {
public:
    T x, y, z;

    constexpr Vec3() : x(0), y(0), z(0) {}
    constexpr Vec3(T x0, T y0, T z0) : x(x0), y(y0), z(z0) {}

    T length_squared() const { return x * x + y * y + z * z; }
    Vec3<T> scaled(T k) const { return Vec3<T>(x * k, y * k, z * k); }
    Vec3 negated() const { return Vec3(-x, -y, -z); }
    T dot(const Vec3<T>& other) const { return x * other.x + y * other.y + z * other.z; }
    void set_xy(const Vec2<T>& v) { x = v.x; y = v.y; }

    // A member template: its own parameter cannot be substituted.
    template <typename U> U first_as() const { return U(x); }
};

template <typename T>
class Vec2 {
public:
    T x, y;
    constexpr Vec2(T x0, T y0) : x(x0), y(y0) {}
};

// A non-type parameter: cannot be instantiated.
template <typename T, int N>
class Fixed {
public:
    T values[N];
};

// Not instantiated: the message says it can be instantiated with `[templates]`.
template <typename T>
class Unused {
public:
    T value;
};

// Explicit specializations (like PX4's `math::is_floating_point<float>`): not bound. As classes
// they would be several Rust types with the same name.
template <typename U> struct is_real { static constexpr bool value = false; };
template <> struct is_real<float> { static constexpr bool value = true; };
template <> struct is_real<double> { static constexpr bool value = true; };

// Concrete uses outside the template.
Vec3<float> cross(const Vec3<float>& a, const Vec3<float>& b);
double norm(const Vec3<double>& v);

// A bit field: the field's type is recorded as unsupported.
struct Packed {
    int flags : 3;
    int count;
};

}  // namespace geo
