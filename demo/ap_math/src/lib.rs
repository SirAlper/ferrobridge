//! ArduPilot `AP_Math`, ferrobridge'in derleme sırasında ürettiği güvenli Rust API'siyle.
//!
//! Bütün `AP_Math` header'ları tek bir bridge'dir: `Vector3f`, `Matrix3f`, `Quaternion` gerçek
//! Rust struct'larıdır (alanlar, metotlar, `+ - * /` operatörleri); CRC'ler `&[u8]` alır;
//! ArduPilot exception kullanmadığı için (`ferrobridge.toml`: `[library] exceptions = false`)
//! fonksiyonlar doğrudan değer döndürür.
//!
//! Bağlanmayan öğeler ve açıklamalara dayanan varsayımlar üretilen dosyanın
//! (`$OUT_DIR/ferrobridge/ap_math/bridge.rs`) başında listelenir.
//!
//! Bu crate'te elle yazılmış `unsafe` yok (`deny(unsafe_code)`). Bridge modülündeki
//! `#[allow(unsafe_code)]` yalnızca üretilen iç katmanı (cxx'in `ffi`'si ve değer tiplerinin
//! `ExternType` gerekçeli impl'leri) kapsar; testler ve örnekler `forbid(unsafe_code)` ile
//! derlenir.

#![deny(unsafe_code)]

/// `AP_Math`'in bütün header'ları.
#[allow(unsafe_code)] // Üretilen iç katman (cxx `ffi`, `ExternType`).
pub mod ap_math {
    ferrobridge::include_bridge!("ap_math");
}

pub use ap_math::*;
