//! C++ tiplerinin IR karşılıkları.

use serde::{Deserialize, Serialize};

/// `const` niteleyicisiyle birlikte bir tip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualType {
    /// Tip `const` ile nitelenmiş mi?
    #[serde(rename = "const")]
    pub is_const: bool,
    /// Niteleyicisiz tip.
    #[serde(rename = "type")]
    pub ty: Type,
}

/// Bir C++ tipi.
///
/// Desteklenmeyen tipler atılmaz; [`Type::Unsupported`] olarak nedeniyle birlikte saklanır.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Type {
    /// `void`
    Void,
    /// `bool`
    Bool,
    /// Karakter ve tamsayı tipleri (`char`, `int`, `std::int32_t`, `size_t`...).
    Integer {
        /// Kaynakta yazıldığı haliyle tip adı (typedef adı korunur).
        spelling: String,
        /// Typedef'ler çözüldükten sonraki C++ temel tipi (ör. `size_t` için
        /// `"unsigned long"`). Platformlar arası taşınabilirlik buna göre değerlendirilir.
        builtin: String,
        /// Hedef platformda bayt cinsinden boyut.
        size_bytes: u64,
        /// İşaretli mi?
        signed: bool,
        /// `std::int32_t` gibi her platformda aynı genişlikte olan bir tip mi?
        fixed_width: bool,
    },
    /// Kayan noktalı tipler (`float`, `double`).
    Float {
        /// Kaynakta yazıldığı haliyle tip adı.
        spelling: String,
        /// Hedef platformda bayt cinsinden boyut.
        size_bytes: u64,
    },
    /// Ham pointer (`T*`). Sahiplik bilgisi burada yoktur; analiz katmanında belirlenir.
    Pointer {
        /// Gösterilen tip.
        pointee: Box<QualType>,
    },
    /// Lvalue referans (`T&`).
    #[serde(rename = "lvalue_ref")]
    LValueRef {
        /// Başvurulan tip.
        referent: Box<QualType>,
    },
    /// Rvalue referans (`T&&`).
    #[serde(rename = "rvalue_ref")]
    RValueRef {
        /// Başvurulan tip.
        referent: Box<QualType>,
    },
    /// Bir sınıf veya struct (ör. `geo::Vehicle`).
    Record {
        /// Sınıfın adı.
        name: String,
        /// Dıştan içe namespace yolu.
        namespace: Vec<String>,
    },
    /// Bir enum (ör. `geo::Mode`).
    Enum {
        /// Enum'un adı.
        name: String,
        /// Dıştan içe namespace yolu.
        namespace: Vec<String>,
    },
    /// `std::string`
    String,
    /// `std::unique_ptr<T>` (varsayılan silicisiyle).
    UniquePtr {
        /// Sahip olunan tip.
        pointee: Box<QualType>,
    },
    /// Bu sürümde desteklenmeyen bir tip.
    Unsupported {
        /// Kaynakta yazıldığı haliyle tip.
        spelling: String,
        /// Neden desteklenmediği.
        reason: String,
    },
}
