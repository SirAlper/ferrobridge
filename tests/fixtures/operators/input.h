// Üye operatörler: değer tiplerinde `std::ops` trait'leri (hata dönmeyenler için) ve her
// durumda adlı metotlar (`op_add`...).
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
    // İç çarpım: aynı operatörün başka sağ taraf tipli overload'u.
    float operator*(const V2& o) const noexcept { return x * o.x + y * o.y; }
    // Çapraz çarpım (ArduPilot'taki gibi `%`).
    float operator%(const V2& o) const noexcept { return x * o.y - y * o.x; }
    V2& operator+=(const V2& o) noexcept {
        x += o.x;
        y += o.y;
        return *this;
    }
    bool operator==(const V2& o) const noexcept { return x == o.x && y == o.y; }
    bool operator!=(const V2& o) const noexcept { return !(*this == o); }
    // Karşılaştırma: adlı metot kalır.
    bool operator<(const V2& o) const noexcept { return x < o.x; }
    // `noexcept` değil: yalnızca `Result` dönen adlı metot.
    V2 operator/(float k) const;
    // Atama: bağlanmaz.
    V2& operator=(const V2&) = default;
    // İndeks: sınır denetimi olmadığı için bağlanmaz (güvenli Rust'tan UB olurdu).
    const float& operator[](int i) const noexcept { return i == 0 ? x : y; }
};

// Opak sınıfın operatörü: adlı metot.
class Counter {
public:
    Counter();
    Counter& operator+=(int n);
    int value() const;

private:
    int n_;
};

}  // namespace geo
