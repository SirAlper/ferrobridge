#pragma once

#include <cstdint>

namespace io {

enum class Mode { Manual, Auto, Hybrid = 10 };
enum class Level : std::uint8_t { Low = 1, High = 200 };
enum Color { Red, Green = 5, Blue };
enum class Offset : short { Minus = -3, Zero = 0 };

Mode next(Mode current);
Level level_for(Color color) noexcept;
// Enum'da tanımlı olmayan bir değer: güvenli katmanda `Error::UnknownEnumValue` olmalı.
Mode broken_mode() noexcept;

}  // namespace io
