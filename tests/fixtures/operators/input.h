// Member operators: `std::ops` traits on value types (for those that do not return errors) and
// named methods in every case (`op_add`...).
#pragma once

namespace geo {

struct V2 {
    float x, y;

    V2() noexcept : x(0), y(0) {}
    V2(float a, float b) noexcept : x(a), y(b) {}

    V2 operator+(const V2& o) const noexcept { return V2(x + o.x, y + o.y); }
    V2 operator-(const V2& o) const noexcept { return V2(x - o.x, y - o.y); }
    V2 operator-() const noexcept { return V2(-x, -y); }
    V2 operator*(float k) const noexcept { return V2(x * k, y * k); }
    // Dot product: an overload of the same operator with another right-hand type.
    float operator*(const V2& o) const noexcept { return x * o.x + y * o.y; }
    // Cross product (`%`, as in ArduPilot).
    float operator%(const V2& o) const noexcept { return x * o.y - y * o.x; }
    V2& operator+=(const V2& o) noexcept {
        x += o.x;
        y += o.y;
        return *this;
    }
    bool operator==(const V2& o) const noexcept { return x == o.x && y == o.y; }
    bool operator!=(const V2& o) const noexcept { return !(*this == o); }
    // Comparison: stays a named method.
    bool operator<(const V2& o) const noexcept { return x < o.x; }
    // Not `noexcept`: only a named method returning `Result`.
    V2 operator/(float k) const;
    // Assignment: not bound.
    V2& operator=(const V2&) = default;
    // Index: not bound because there is no bounds check (it would be UB from safe Rust).
    const float& operator[](int i) const noexcept { return i == 0 ? x : y; }
};

// An operator of an opaque class: a named method.
class Counter {
public:
    Counter();
    Counter& operator+=(int n);
    int value() const;

private:
    int n_;
};

}  // namespace geo
