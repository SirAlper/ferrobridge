#pragma once

#include <cstdint>
#include <memory>
#include <string>

namespace own {

// Live object counters: the constructor increments, the destructor decrements. With them the tests
// prove that ownership really passes to Rust (the object is deleted when the UniquePtr is dropped).
std::int32_t live_widgets() noexcept;
std::int32_t live_shapes() noexcept;

class Widget {
 public:
  explicit Widget(std::int32_t id);
  Widget(const Widget* prototype, std::int32_t id);
  ~Widget();

  std::int32_t id() const noexcept;
  void adopt(std::int32_t child_id);

  // `borrowed` returns: point inside the object; Err in Rust if null.
  Widget* child();
  const Widget* child_view() const;
  const Widget* child_named(const std::string& name) const;
  double* weight_slot();
  double& weight();

  // `borrowed` parameters: used only during the call.
  void copy_id_from(const Widget* other);
  void swap_ids(Widget* other);
  void read_weight(double& out) const;
  void read_id(std::int32_t* out) const;

  // Deliberately not annotated: no code must be generated.
  void* raw_handle();
  Widget* unannotated();

 private:
  std::int32_t id_;
  double weight_;
  std::unique_ptr<Widget> child_;
};

// Interface + factory: the real type is a subclass in impl.cc.
class Shape {
 public:
  virtual ~Shape();
  virtual double area() const = 0;
};

// Polymorphic, but its destructor is not virtual: must be rejected even if annotated `owned`.
class Leaky {
 public:
  virtual std::int32_t id() const;
};

// `owned` returns: the object is allocated with `new`; deleting it is up to the caller.
Widget* make_widget(std::int32_t id);
Widget* make_null_widget();
std::string* make_label(const std::string& text);
Shape* make_square(double side);
Leaky* make_leaky();

void scale(double* value, double factor);

}  // namespace own
