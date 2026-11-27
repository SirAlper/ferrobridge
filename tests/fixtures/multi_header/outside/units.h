// A header not included in the bridge: a function using the type here is not bound, and the
// message names this header.
#pragma once

namespace units {

class Meters {
public:
    explicit Meters(double value);
    double value() const;

private:
    double value_;
};

}  // namespace units
