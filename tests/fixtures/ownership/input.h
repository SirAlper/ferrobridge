#pragma once

#include <cstdint>
#include <memory>
#include <string>

namespace own {

// Canlı nesne sayaçları: constructor artırır, destructor azaltır. Sahipliğin Rust'a
// gerçekten geçtiğini (UniquePtr düşünce nesnenin silindiğini) testler bununla kanıtlar.
std::int32_t live_widgets() noexcept;
std::int32_t live_shapes() noexcept;

class Widget {
 public:
  explicit Widget(std::int32_t id);
  Widget(const Widget* prototype, std::int32_t id);
  ~Widget();

  std::int32_t id() const noexcept;
  void adopt(std::int32_t child_id);

  // `borrowed` dönüşler: nesnenin içini gösterir; null ise Rust'ta Err.
  Widget* child();
  const Widget* child_view() const;
  const Widget* child_named(const std::string& name) const;
  double* weight_slot();
  double& weight();

  // `borrowed` parametreler: yalnızca çağrı süresince kullanılır.
  void copy_id_from(const Widget* other);
  void swap_ids(Widget* other);
  void read_weight(double& out) const;
  void read_id(std::int32_t* out) const;

  // Bilerek açıklanmamış: kod üretilmemeli.
  void* raw_handle();
  Widget* unannotated();

 private:
  std::int32_t id_;
  double weight_;
  std::unique_ptr<Widget> child_;
};

// Arayüz + fabrika: gerçek tip impl.cc'deki bir alt sınıf.
class Shape {
 public:
  virtual ~Shape();
  virtual double area() const = 0;
};

// Polimorfik ama destructor'ı sanal değil: `owned` açıklansa bile reddedilmeli.
class Leaky {
 public:
  virtual std::int32_t id() const;
};

// `owned` dönüşler: nesne `new` ile ayrılır, silmek çağırana düşer.
Widget* make_widget(std::int32_t id);
Widget* make_null_widget();
std::string* make_label(const std::string& text);
Shape* make_square(double side);
Leaky* make_leaky();

void scale(double* value, double factor);

}  // namespace own
