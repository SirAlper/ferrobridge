// Value types: classes whose fields are all public and that are trivially copyable become `#[repr(C)]`
// structs in Rust (ADR-0010). A class with a private field stays opaque.
#pragma once

namespace geo {

struct Pt {
    double x;
    double y;

    Pt() : x(0), y(0) {}
    Pt(double x0, double y0) : x(x0), y(y0) {}
    double norm2() const { return x * x + y * y; }
    void scale(double k) { x *= k; y *= k; }
};

// A value type containing value types.
struct Line {
    Pt from;
    Pt to;
    double length() const;
};

// A template instantiation that is a value type (`ferrobridge.toml`: `Vec3f = "geo::V3<float>"`).
template <typename T>
struct V3 {
    T x, y, z;

    V3() : x(0), y(0), z(0) {}
    V3(T a, T b, T c) : x(a), y(b), z(c) {}
    T dot(const V3<T>& o) const { return x * o.x + y * o.y + z * o.z; }
    V3<T> scaled(T k) const { return V3<T>(x * k, y * k, z * k); }
    void normalize();
};

// A two-parameter template instantiation: its spelling contains a comma (an alias must be used
// for the `offsetof` macro).
template <typename A, typename B>
struct Pair {
    A first;
    B second;
};

// A `const` field: writing it from Rust would change a `const` object in C++; stays opaque.
struct Frozen {
    const double limit;
    Frozen() : limit(1.5) {}
    double get() const { return limit; }
};

Pt midpoint(Pt a, const Pt& b);
Pair<int, float> make_pair(int a, float b);
Line make_line(Pt from, Pt to);
V3<float> cross(const V3<float>& a, const V3<float>& b);

// Has a private field: stays opaque, its public field is reported.
class Account {
public:
    Account();
    double balance;
    double fee() const;

private:
    double hidden_;
};

}  // namespace geo
