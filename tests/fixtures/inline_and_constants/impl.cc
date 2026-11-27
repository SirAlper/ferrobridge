// Fonksiyonların çoğu header'da tanımlı (`static inline`, `constexpr`). Burada yalnızca
// sabitlerin çalışma anındaki değerlerini döndüren iki fonksiyon var.
#include "inline_and_constants/input.h"

double wgs84_f_at_runtime() { return WGS84_F; }

float tenth_at_runtime() { return TENTH; }
