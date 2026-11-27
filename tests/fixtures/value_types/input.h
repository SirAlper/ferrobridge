// Değer tipleri: bütün alanları public ve bit bit kopyalanabilen sınıflar Rust'ta `#[repr(C)]`
// struct olur (ADR-0010). Private alanı olan sınıf opak kalır.
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

// İçinde değer tipleri olan değer tipi.
struct Line {
    Pt from;
    Pt to;
    double length() const;
};

// Değer tipi olan template örneklemesi (`ferrobridge.toml`: `Vec3f = "geo::V3<float>"`).
template <typename T>
struct V3 {
    T x, y, z;

    V3() : x(0), y(0), z(0) {}
    V3(T a, T b, T c) : x(a), y(b), z(c) {}
    T dot(const V3<T>& o) const { return x * o.x + y * o.y + z * o.z; }
    V3<T> scaled(T k) const { return V3<T>(x * k, y * k, z * k); }
    void normalize();
};

// İki parametreli template örneklemesi: tip yazımında virgül var (`offsetof` makrosu için takma
// ad kullanılmalı).
template <typename A, typename B>
struct Pair {
    A first;
    B second;
};

// `const` alan: Rust'tan yazılırsa C++'ta `const` nesneyi değiştirmek olurdu; opak kalır.
struct Frozen {
    const double limit;
    Frozen() : limit(1.5) {}
    double get() const { return limit; }
};

Pt midpoint(Pt a, const Pt& b);
Pair<int, float> make_pair(int a, float b);
Line make_line(Pt from, Pt to);
V3<float> cross(const V3<float>& a, const V3<float>& b);

// Private alanı var: opak kalır, public alanı raporlanır.
class Account {
public:
    Account();
    double balance;
    double fee() const;

private:
    double hidden_;
};

}  // namespace geo
