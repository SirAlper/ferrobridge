// SPDX-License-Identifier: GPL-3.0-or-later
//
// AP_Math'in kendi dışından istediği dört parça (M5 Task 0'da bağlayıcı hatalarından çıkarıldı).
// ArduPilot'un gerçek gerçeklemeleri AP_HAL ve AP_Param'ı çeker; demo için bunlar yeterli.
//
// Sınırlar:
// - Yalnızca AP_Math'i tek başına bağlamak için. ArduPilot'un gerçek `AP_InternalError.cpp`'si ve
//   HAL'ı ile birlikte bağlanırsa semboller iki kez tanımlanır.
// - Sayaçlar atomik değil (ArduPilot'taki gibi); AP_Math'i birden fazla iş parçacığından
//   çağıran kod internal error sayaçlarında veri yarışına girer.
// - Özel döndürmeler (`ROTATION_CUSTOM_*`) desteklenmiyor: çağrılırsa süreç `abort` eder,
//   Rust'a `Err` olarak dönmez.

#include <AP_CustomRotations/AP_CustomRotations.h>
#include <AP_HAL/AP_HAL.h>
#include <AP_InternalError/AP_InternalError.h>

#include <cstdarg>
#include <cstdio>
#include <cstdlib>

namespace {
AP_InternalError internal_error;
}

// Hata kaydedilir, program durmaz (ArduPilot'ta da öyle).
void AP_InternalError::error(const AP_InternalError::error_t e, uint16_t line) {
    internal_errors |= uint32_t(e);
    total_error_count++;
    last_line = line;
}

namespace AP {
AP_InternalError &internalerror() { return internal_error; }

// `ROTATION_CUSTOM_*` dışındaki döndürmeler bunu hiç çağırmaz.
AP_CustomRotations &custom_rotations() {
    AP_HAL::panic("AP_CustomRotations bu demoda yok");
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
