//! ferrobridge'in dilden bağımsız ara modeli (IR).
//!
//! Parser'ın ürettiği, analiz ve kod üretimi katmanlarının tükettiği veri yapıları burada
//! tanımlanır. Bu crate başka hiçbir ferrobridge crate'ine bağımlı değildir; parser değişse
//! bile IR aynı kalır (bkz. `docs/adr/0001-libclang-via-clang-sys.md`).
//!
//! İçerik (fonksiyonlar, tipler, sınıflar) Milestone 1'de eklenecek.

use serde::{Deserialize, Serialize};

/// IR'ın JSON biçiminin sürümü. Biçim uyumsuz şekilde değiştiğinde artırılır.
pub const IR_FORMAT_VERSION: u32 = 0;

/// Bir veya birden çok C++ header'ından okunan sembollerin kökü.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Module {
    /// Bu modülün üretildiği IR biçim sürümü.
    pub format_version: u32,
}

impl Module {
    /// Güncel biçim sürümüyle boş bir modül oluşturur.
    pub fn new() -> Self {
        Self {
            format_version: IR_FORMAT_VERSION,
        }
    }
}

impl Default for Module {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_round_trips_through_json() {
        let module = Module::new();
        let json = serde_json::to_string(&module).unwrap();
        let back: Module = serde_json::from_str(&json).unwrap();
        assert_eq!(back, module);
        assert_eq!(back.format_version, IR_FORMAT_VERSION);
    }
}
