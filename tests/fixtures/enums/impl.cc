#include "enums/input.h"

namespace io {

Mode next(Mode current) {
  switch (current) {
    case Mode::Manual:
      return Mode::Auto;
    case Mode::Auto:
      return Mode::Hybrid;
    default:
      return Mode::Manual;
  }
}

Level level_for(Color color) noexcept { return color == Red ? Level::High : Level::Low; }
Mode broken_mode() noexcept { return static_cast<Mode>(42); }

}  // namespace io
