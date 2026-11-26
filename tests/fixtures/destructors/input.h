#pragma once

namespace geo {

// Destructor bildirilmemiş: örtük, public, sanal değil.
class Plain {
 public:
  Plain();
};

// Sanal metodu var ama destructor'ı sanal değil: alt sınıfı `delete` ile silmek UB.
class Leaky {
 public:
  ~Leaky();
  virtual int id() const;
};

// Arayüz: sanal destructor'lı.
class Shape {
 public:
  virtual ~Shape();
  virtual double area() const = 0;
};

// Destructor'ı örtük, ama tabanınki sanal olduğu için o da sanal.
class Circle : public Shape {
 public:
  explicit Circle(double r);
  double area() const override;
};

// Sanal üyesi yalnızca private: yine de polimorfik.
class Hidden {
 public:
  Hidden();

 private:
  virtual void hook();
};

// Dışarıdan silinemez.
class Guarded {
 public:
  Guarded();

 protected:
  ~Guarded();
};

class Immortal {
 public:
  Immortal();
  ~Immortal() = delete;
};

}  // namespace geo
