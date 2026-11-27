#include "operators/input.h"

#include <stdexcept>

namespace geo {

V2 V2::operator/(float k) const {
    if (k == 0) {
        throw std::domain_error("division by zero");
    }
    return V2(x / k, y / k);
}

Counter::Counter() : n_(0) {}
Counter& Counter::operator+=(int n) {
    n_ += n;
    return *this;
}
int Counter::value() const { return n_; }

}  // namespace geo
