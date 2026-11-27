// Bridge'e dahil olmayan bir header: buradaki tipi kullanan fonksiyon bağlanmaz, mesaj bu
// header'ın adını söyler.
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
