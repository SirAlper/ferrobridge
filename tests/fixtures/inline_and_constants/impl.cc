// Most functions are defined in the header (`static inline`, `constexpr`). Here there are only
// two functions returning the run-time values of constants.
#include "inline_and_constants/input.h"

double wgs84_f_at_runtime() { return WGS84_F; }

float tenth_at_runtime() { return TENTH; }
