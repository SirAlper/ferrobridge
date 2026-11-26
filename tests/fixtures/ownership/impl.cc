#include "ownership/input.h"

namespace own {
namespace {
std::int32_t widget_count = 0;
std::int32_t shape_count = 0;

class Square : public Shape {
 public:
  explicit Square(double side) : side_(side) { ++shape_count; }
  ~Square() override { --shape_count; }
  double area() const override { return side_ * side_; }

 private:
  double side_;
};
}  // namespace

std::int32_t live_widgets() noexcept { return widget_count; }
std::int32_t live_shapes() noexcept { return shape_count; }

Widget::Widget(std::int32_t id) : id_(id), weight_(0.0) { ++widget_count; }
Widget::Widget(const Widget* prototype, std::int32_t id)
    : id_(id), weight_(prototype->weight_) {
  ++widget_count;
}
Widget::~Widget() { --widget_count; }

std::int32_t Widget::id() const noexcept { return id_; }
void Widget::adopt(std::int32_t child_id) { child_.reset(new Widget(child_id)); }

Widget* Widget::child() { return child_.get(); }
const Widget* Widget::child_view() const { return child_.get(); }
const Widget* Widget::child_named(const std::string& name) const {
  if (child_ && name == std::to_string(child_->id_)) {
    return child_.get();
  }
  return nullptr;
}
double* Widget::weight_slot() { return &weight_; }
double& Widget::weight() { return weight_; }

void Widget::copy_id_from(const Widget* other) { id_ = other->id_; }
void Widget::swap_ids(Widget* other) { std::swap(id_, other->id_); }
void Widget::read_weight(double& out) const { out = weight_; }
void Widget::read_id(std::int32_t* out) const { *out = id_; }

void* Widget::raw_handle() { return this; }
Widget* Widget::unannotated() { return this; }

Shape::~Shape() = default;
std::int32_t Leaky::id() const { return 7; }

Widget* make_widget(std::int32_t id) { return new Widget(id); }
Widget* make_null_widget() { return nullptr; }
std::string* make_label(const std::string& text) { return new std::string("[" + text + "]"); }
Shape* make_square(double side) { return new Square(side); }
Leaky* make_leaky() { return new Leaky(); }

void scale(double* value, double factor) { *value *= factor; }

}  // namespace own
