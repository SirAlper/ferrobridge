// SPDX-License-Identifier: GPL-3.0-or-later
//
// The four pieces AP_Math needs from outside itself (found from linker errors in M5 Task 0).
// ArduPilot's real implementations pull in AP_HAL and AP_Param; these are enough for the demo.
//
// Limits:
// - Only for linking AP_Math on its own. If linked together with ArduPilot's real
//   `AP_InternalError.cpp` and HAL, the symbols are defined twice.
// - The counters are not atomic (as in ArduPilot); code calling AP_Math from more than one
//   thread races on the internal error counters.
// - Custom rotations (`ROTATION_CUSTOM_*`) are not supported: if called, the process `abort`s,
//   it does not come back to Rust as `Err`.

#include <AP_CustomRotations/AP_CustomRotations.h>
#include <AP_HAL/AP_HAL.h>
#include <AP_InternalError/AP_InternalError.h>

#include <cstdarg>
#include <cstdio>
#include <cstdlib>

namespace {
AP_InternalError internal_error;
}

// The error is recorded; the program does not stop (the same as in ArduPilot).
void AP_InternalError::error(const AP_InternalError::error_t e, uint16_t line) {
    internal_errors |= uint32_t(e);
    total_error_count++;
    last_line = line;
}

namespace AP {
AP_InternalError &internalerror() { return internal_error; }

// Rotations other than `ROTATION_CUSTOM_*` never call this.
AP_CustomRotations &custom_rotations() {
    AP_HAL::panic("AP_CustomRotations is not available in this demo");
}
}  // namespace AP

void AP_CustomRotations::from_rotation(enum Rotation, QuaternionD &) { AP::custom_rotations(); }
void AP_CustomRotations::from_rotation(enum Rotation, Quaternion &) { AP::custom_rotations(); }
void AP_CustomRotations::rotate(enum Rotation, Vector3d &) { AP::custom_rotations(); }
void AP_CustomRotations::rotate(enum Rotation, Vector3f &) { AP::custom_rotations(); }

void AP_HAL::panic(const char *errormsg, ...) {
    va_list ap;
    va_start(ap, errormsg);
    vfprintf(stderr, errormsg, ap);
    va_end(ap);
    fputc('\n', stderr);
    std::abort();
}
