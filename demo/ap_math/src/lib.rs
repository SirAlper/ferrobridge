//! ArduPilot `AP_Math`, ferrobridge'in derleme sırasında ürettiği güvenli Rust API'siyle.
//!
//! Her modül bir `AP_Math` header'ına karşılık gelir. Bağlanmayan öğeler ve açıklamalara
//! (`annotations/*.toml`) dayanan varsayımlar, üretilen dosyaların
//! (`$OUT_DIR/ferrobridge/<modül>/bridge.rs`) başında listelenir.
//!
//! Template'ler (`Vector3f`, `Matrix3f`, `Quaternion`) henüz bağlanmıyor (Milestone 6).
//!
//! Bu crate'te elle yazılmış `unsafe` yok (`deny(unsafe_code)`). Bridge modüllerindeki
//! `#[allow(unsafe_code)]` yalnızca cxx'in ürettiği iç katmanı (`ffi`) kapsar; testler
//! `forbid(unsafe_code)` ile derlenir.

#![deny(unsafe_code)]

/// `AP_Math/AP_Math.h`: açı dönüşümleri, sınırlama, sarma, birim dönüşümleri.
#[allow(unsafe_code)] // cxx'in ürettiği `ffi` katmanı.
pub mod ap_math {
    ferrobridge::include_bridge!("ap_math");
}

/// `AP_Math/control.h`: hız/ivme şekillendirme ve kontrolcü yardımcıları.
#[allow(unsafe_code)] // cxx'in ürettiği `ffi` katmanı.
pub mod control {
    ferrobridge::include_bridge!("control");
}

/// `AP_Math/crc.h`: CRC ve sağlama toplamları.
#[allow(unsafe_code)] // cxx'in ürettiği `ffi` katmanı.
pub mod crc {
    ferrobridge::include_bridge!("crc");
}

/// `AP_Math/location.h`: konum yardımcıları.
#[allow(unsafe_code)] // cxx'in ürettiği `ffi` katmanı.
pub mod location {
    ferrobridge::include_bridge!("location");
}

/// `AP_Math/SCurve.h`: S-eğrisi yol planlayıcı.
#[allow(unsafe_code)] // cxx'in ürettiği `ffi` katmanı.
pub mod scurve {
    ferrobridge::include_bridge!("scurve");
}

/// `AP_Math/SplineCurve.h`: spline yol planlayıcı.
#[allow(unsafe_code)] // cxx'in ürettiği `ffi` katmanı.
pub mod spline_curve {
    ferrobridge::include_bridge!("spline_curve");
}

/// `AP_Math/rotations.h`: `Rotation` enum'u.
#[allow(unsafe_code)] // cxx'in ürettiği `ffi` katmanı.
pub mod rotations {
    ferrobridge::include_bridge!("rotations");
}

/// `AP_Math/definitions.h`: fiziksel sabitler ve WGS84 değerleri.
#[allow(unsafe_code)] // cxx'in ürettiği `ffi` katmanı.
pub mod definitions {
    ferrobridge::include_bridge!("definitions");
}
