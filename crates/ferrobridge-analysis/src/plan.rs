//! Analizin çıktısı: kod üretiminin tükettiği bağlama planı.

use std::fmt;

use ferrobridge_ir::{EnumVariant, SourceLocation};

use crate::config::ReturnAnnotation;

/// Bir header için neyin, nasıl bağlanacağı.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BridgePlan {
    /// Bağlanan enum'lar.
    pub enums: Vec<EnumPlan>,
    /// Bağlanan sınıflar (opak tip olarak) ve üyeleri.
    pub classes: Vec<ClassPlan>,
    /// Bağlanan serbest fonksiyonlar.
    pub functions: Vec<FunctionPlan>,
    /// Bağlanamayan öğeler ve nedenleri.
    pub skipped: Vec<Skipped>,
    /// Bağlanan ama kullanıcının bilmesi gereken durumlar.
    pub notes: Vec<Skipped>,
    /// Üretilen kodun dayandığı, `ferrobridge.toml`'daki sözler (güven noktaları).
    pub assumptions: Vec<Assumption>,
    /// Açıklama eklenirse bağlanabilecek yerler (`ferrobridge suggest` bunları basar).
    pub suggestions: Vec<Suggestion>,
    /// Gereksiz açıklamalar: hata değil, ama dosyada yanlış bir inanç olabilir.
    pub annotation_warnings: Vec<AnnotationIssue>,
}

/// Üretilen kodun dayandığı bir açıklama.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assumption {
    /// Öğenin tam C++ adı (ör. `geo::make_widget`).
    pub name: String,
    /// Neye uygulandığı (ör. "dönüş", "parametre `out`").
    pub subject: String,
    /// Açıklamanın değeri (`owned`, `borrowed`).
    pub annotation: String,
    /// Kullanıcının bu açıklamayla verdiği söz.
    pub promise: String,
    /// Açıklamanın yeri (`ferrobridge.toml:3`; yalnızca dosya adı).
    pub origin: String,
}

/// Açıklama gerektiği için bağlanamayan bir yer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// `ferrobridge.toml`'daki anahtar (tam C++ adı).
    pub key: String,
    /// Okunabilir imza (ör. `geo::Widget::attach(geo::Widget* target)`).
    pub signature: String,
    /// Açıklanması gereken yer.
    pub target: SuggestionTarget,
    /// Açıklanacak C++ tipi (ör. `geo::Widget*`).
    pub cpp_type: String,
    /// Bildirimin kaynaktaki yeri.
    pub location: SourceLocation,
}

/// Önerinin hedefi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionTarget {
    /// Dönüş değeri; seçilebilecek açıklamalarla.
    Return {
        /// Bu bağlamda anlamlı açıklamalar.
        options: Vec<ReturnAnnotation>,
    },
    /// Bir parametre (adı; isimsizse `argN`). Tek seçenek `borrowed`.
    Param {
        /// Parametre adı.
        name: String,
    },
}

/// Açıklama dosyasıyla header arasındaki bir uyumsuzluk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationIssue {
    /// Açıklamanın yeri (`ferrobridge.toml:3`).
    pub origin: String,
    /// Sorun ve çözümü.
    pub message: String,
}

impl fmt::Display for AnnotationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.origin, self.message)
    }
}

/// Bağlanamayan bir öğe ya da bir not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// Öğenin tam C++ adı (ör. `geo::Vehicle::total_km`).
    pub name: String,
    /// Neden ve (varsa) ne zaman desteklenmesinin planlandığı.
    pub reason: String,
    /// Bildirimin kaynaktaki yeri.
    pub location: SourceLocation,
}

/// Bridge'de adı geçen bir sınıf veya enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef {
    /// Bridge içindeki Rust adı (çakışmalarda namespace ön ekli: `PhysicsVehicle`).
    pub rust_name: String,
    /// Tam nitelenmiş C++ yolu (ör. `::geo::Vehicle`).
    pub cpp_path: String,
}

/// Bağlanan bir sınıf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassPlan {
    /// Tipin adları.
    pub type_ref: TypeRef,
    /// C++'taki adı.
    pub cpp_name: String,
    /// C++ namespace yolu.
    pub namespace: Vec<String>,
    /// Fabrika fonksiyonu olarak bağlanan constructor'lar.
    pub constructors: Vec<FunctionPlan>,
    /// Bağlanan metotlar (statikler dahil).
    pub methods: Vec<FunctionPlan>,
}

/// Bağlanan bir enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumPlan {
    /// Tipin adları.
    pub type_ref: TypeRef,
    /// C++'taki adı.
    pub cpp_name: String,
    /// C++ namespace yolu.
    pub namespace: Vec<String>,
    /// Altta yatan tamsayı tipi.
    pub repr: IntType,
    /// Değerler.
    pub variants: Vec<EnumVariant>,
}

/// Rust ile C++ arasında her platformda aynı olan tamsayı tipleri.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntType {
    /// `i8` / `std::int8_t`
    I8,
    /// `u8` / `std::uint8_t`
    U8,
    /// `i16` / `std::int16_t`
    I16,
    /// `u16` / `std::uint16_t`
    U16,
    /// `i32` / `std::int32_t`
    I32,
    /// `u32` / `std::uint32_t`
    U32,
    /// `i64` / `std::int64_t`
    I64,
    /// `u64` / `std::uint64_t`
    U64,
}

impl IntType {
    /// Boyut ve işaretten tip; desteklenmeyen boyutta `None`.
    pub fn from_layout(size_bytes: u64, signed: bool) -> Option<Self> {
        Some(match (size_bytes, signed) {
            (1, true) => Self::I8,
            (1, false) => Self::U8,
            (2, true) => Self::I16,
            (2, false) => Self::U16,
            (4, true) => Self::I32,
            (4, false) => Self::U32,
            (8, true) => Self::I64,
            (8, false) => Self::U64,
            _ => return None,
        })
    }

    /// Rust'taki adı (`"i32"`).
    pub fn rust_name(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::U8 => "u8",
            Self::I16 => "i16",
            Self::U16 => "u16",
            Self::I32 => "i32",
            Self::U32 => "u32",
            Self::I64 => "i64",
            Self::U64 => "u64",
        }
    }

    /// cxx'in kullandığı C++ adı (`"::std::int32_t"`).
    pub fn cpp_name(self) -> &'static str {
        match self {
            Self::I8 => "::std::int8_t",
            Self::U8 => "::std::uint8_t",
            Self::I16 => "::std::int16_t",
            Self::U16 => "::std::uint16_t",
            Self::I32 => "::std::int32_t",
            Self::U32 => "::std::uint32_t",
            Self::I64 => "::std::int64_t",
            Self::U64 => "::std::uint64_t",
        }
    }
}

/// Bridge sınırındaki bir tip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeType {
    /// `bool`
    Bool,
    /// C++ `char` / Rust `c_char`
    Char,
    /// Sabit genişlikli tamsayı.
    Int(IntType),
    /// `f32` / `float`
    F32,
    /// `f64` / `double`
    F64,
    /// Opak sınıf (yalnızca referans veya `UniquePtr` arkasında kullanılır).
    Class(TypeRef),
    /// Paylaşılan enum.
    Enum(TypeRef),
    /// `CxxString` / `std::string` (yalnızca referans veya `UniquePtr` arkasında).
    String,
    /// `UniquePtr<T>` / `std::unique_ptr<T>`
    UniquePtr(Box<BridgeType>),
    /// `CxxVector<T>` / `std::vector<T>` (yalnızca referans veya `UniquePtr` arkasında).
    Vector(Box<BridgeType>),
    /// `&T` (`mutable: false`). `mutable: true` ise opak tipler (sınıf, `std::string`,
    /// `std::vector`) için `Pin<&mut T>`, primitif ve enum'lar için `&mut T`.
    Ref {
        /// Değiştirilebilir referans mı?
        mutable: bool,
        /// Başvurulan tip.
        inner: Box<BridgeType>,
    },
}

/// Bir fonksiyonun dönüşü.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Returns {
    /// `void`
    Unit,
    /// Olduğu gibi döner.
    Value(BridgeType),
    /// C++'ta değerle dönen sınıf; shim `std::unique_ptr`'a sarar, Rust'a `UniquePtr<T>` gelir.
    BoxedClass(TypeRef),
    /// C++'ta değerle dönen `std::string`; shim sarar, Rust'a `UniquePtr<CxxString>` gelir.
    OwnedString,
    /// C++'ta değerle dönen `std::vector<T>` (eleman tipi); shim sarar, Rust'a
    /// `UniquePtr<CxxVector<T>>` gelir.
    OwnedVector(BridgeType),
    /// `"owned"` açıklamalı `T*` (gösterilen tip): shim `std::unique_ptr`'a sarar, Rust'a
    /// `UniquePtr<T>` gelir. Null gelirse boş `UniquePtr` olur.
    OwnedPointer(BridgeType),
    /// `"borrowed"` açıklamalı `T*` (yalnızca metotlar): shim null kontrolü yapıp referansa
    /// çevirir; null ise exception → `Err`. Referansın ömrü alıcıya bağlıdır.
    BorrowedPointer {
        /// Değiştirilebilir referans mı?
        mutable: bool,
        /// Gösterilen tip.
        inner: BridgeType,
    },
}

/// Çağrılan C++ öğesi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Callee {
    /// Serbest fonksiyon.
    Free {
        /// C++ namespace yolu.
        namespace: Vec<String>,
        /// C++ adı.
        cpp_name: String,
    },
    /// Üye fonksiyon.
    Method {
        /// Sahibi olan sınıf.
        class: TypeRef,
        /// C++ adı.
        cpp_name: String,
        /// `false`: `self: &T` (const metot); `true`: `self: Pin<&mut T>`.
        receiver_mutable: bool,
    },
    /// Statik üye fonksiyon (shim üzerinden çağrılır).
    Static {
        /// Sahibi olan sınıf.
        class: TypeRef,
        /// C++ adı.
        cpp_name: String,
    },
    /// Constructor (shim'deki fabrika fonksiyonu üzerinden çağrılır).
    Constructor {
        /// Oluşturulan sınıf.
        class: TypeRef,
    },
}

/// Bağlanan bir fonksiyon, metot veya constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionPlan {
    /// Bridge içindeki Rust adı.
    pub rust_name: String,
    /// Çağrılan C++ öğesi.
    pub callee: Callee,
    /// Parametreler (`this` hariç).
    pub params: Vec<ParamPlan>,
    /// Dönüş.
    pub returns: Returns,
    /// `noexcept` değilse `true`: Rust tarafında `Result<T>` döner.
    pub fallible: bool,
    /// Bildirimin kaynaktaki yeri.
    pub location: SourceLocation,
}

impl FunctionPlan {
    /// Doğrudan cxx ile çağrılamayıp C++ shim'i gerektiriyor mu?
    pub fn needs_shim(&self) -> bool {
        matches!(
            self.callee,
            Callee::Static { .. } | Callee::Constructor { .. }
        ) || matches!(
            self.returns,
            Returns::BoxedClass(_)
                | Returns::OwnedString
                | Returns::OwnedVector(_)
                | Returns::OwnedPointer(_)
                | Returns::BorrowedPointer { .. }
        ) || self.params.iter().any(|p| p.pass == Pass::AddressOf)
    }
}

/// Bir parametre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamPlan {
    /// Rust'taki adı (anahtar kelimelerden kaçınılmış, isimsizse `argN`).
    pub rust_name: String,
    /// Tipi.
    pub ty: BridgeType,
    /// C++ fonksiyonuna nasıl geçirildiği.
    pub pass: Pass,
}

/// Bir parametrenin C++ fonksiyonuna geçirilişi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    /// Olduğu gibi (cxx doğrudan çağırabilir).
    Direct,
    /// C++ pointer bekliyor, Rust referans veriyor: shim adresini alıp geçirir (`&p`).
    AddressOf,
}
