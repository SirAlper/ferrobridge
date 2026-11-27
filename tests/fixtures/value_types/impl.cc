#include "value_types/input.h"

#include <cmath>

namespace geo {

double Line::length() const { return std::hypot(to.x - from.x, to.y - from.y); }

template <typename T>
void V3<T>::normalize() {
    T n = std::sqrt(dot(*this));
    x /= n;
    y /= n;
    z /= n;
}
template struct V3<float>;

Pt midpoint(Pt a, const Pt& b) { return Pt((a.x + b.x) / 2, (a.y + b.y) / 2); }
Line make_line(Pt from, Pt to) { return Line{from, to}; }
V3<float> cross(const V3<float>& a, const V3<float>& b) {
    return V3<float>(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x);
}

Pair<int, float> make_pair(int a, float b) { return Pair<int, float>{a, b}; }

Account::Account() : balance(0), hidden_(0.5) {}
double Account::fee() const { return hidden_; }

}  // namespace geo
