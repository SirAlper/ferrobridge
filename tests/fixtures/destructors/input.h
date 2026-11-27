#pragma once

namespace geo {

// No destructor declared: implicit, public, not virtual.
class Plain {
 public:
  Plain();
};

// Has a virtual method but its destructor is not virtual: deleting a subclass with `delete` is UB.
class Leaky {
 public:
  ~Leaky();
  virtual int id() const;
};

// An interface: with a virtual destructor.
class Shape {
 public:
  virtual ~Shape();
  virtual double area() const = 0;
};

// Its destructor is implicit, but virtual because its base's is.
class Circle : public Shape {
 public:
  explicit Circle(double r);
  double area() const override;
};

// Its only virtual member is private: still polymorphic.
class Hidden {
 public:
  Hidden();

 private:
  virtual void hook();
};

// Cannot be deleted from outside.
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
