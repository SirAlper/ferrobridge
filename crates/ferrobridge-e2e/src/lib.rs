//! Uçtan uca testler için üretilmiş bridge'ler.
//!
//! `src/generated/` altındaki dosyalar `ferrobridge generate` çıktısıdır; elle düzenlemeyin.
//! Yeniden üretme komutu `README.md`'de. `ferrobridge-cli`'deki bir test, commit'li dosyaların
//! üreticinin güncel çıktısıyla birebir aynı olduğunu doğrular.
//!
//! C++ tarafı `tests/fixtures/<ad>/input.h` ve `impl.cc`; `build.rs` hepsini derler.

/// `tests/fixtures/classes`: sınıflar, constructor'lar, metotlar, overload'lar, exception'lar.
#[path = "generated/classes/bridge.rs"]
pub mod classes;

/// `tests/fixtures/enums`: scoped/unscoped enum'lar, farklı alt tipler, negatif değerler.
#[path = "generated/enums/bridge.rs"]
pub mod enums;

/// `tests/fixtures/strings`: `std::string` parametreleri ve dönüşleri.
#[path = "generated/strings/bridge.rs"]
pub mod strings;

/// `tests/fixtures/ownership`: `ferrobridge.toml` açıklamalı sahip olunan/ödünç pointer'lar,
/// çıkış parametreleri, canlı nesne sayaçları.
#[path = "generated/ownership/bridge.rs"]
pub mod ownership;

/// `tests/fixtures/vectors`: `std::vector` parametreleri ve dönüşleri.
#[path = "generated/vectors/bridge.rs"]
pub mod vectors;

/// `tests/fixtures/slices`: `{ slice = "len" }` açıklamalı pointer + uzunluk çiftleri.
#[path = "generated/slices/bridge.rs"]
pub mod slices;

/// `tests/fixtures/inline_and_constants`: `static inline` fonksiyonlar ve sabitler.
#[path = "generated/inline_and_constants/bridge.rs"]
pub mod inline_and_constants;
