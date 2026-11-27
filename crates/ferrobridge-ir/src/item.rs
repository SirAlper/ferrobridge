//! Header'da bulunan üst seviye öğeler.

use serde::{Deserialize, Serialize};

use crate::{QualType, Type};

/// Header'dan okunan bir öğe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Item {
    /// Serbest (sınıf üyesi olmayan) bir fonksiyon.
    Function(Function),
    /// Bir sınıf veya struct tanımı.
    Class(Class),
    /// Bir enum tanımı.
    Enum(Enum),
    /// Değeri derleme anında kesin olarak bilinen, sayı ya da `bool` tipli bir sabit.
    Constant(Constant),
    /// Bu sürümde desteklenmeyen bir yapı. Sessizce atlanmak yerine nedeniyle kaydedilir.
    Unsupported(Unsupported),
}

/// Serbest bir fonksiyon bildirimi.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Function {
    /// Fonksiyonun adı (namespace'siz).
    pub name: String,
    /// Dıştan içe namespace yolu (`geo::detail::f` için `["geo", "detail"]`).
    pub namespace: Vec<String>,
    /// Parametreler, sırasıyla.
    pub params: Vec<Param>,
    /// Dönüş tipi.
    pub return_type: QualType,
    /// Fonksiyon exception fırlatmayacağını garanti ediyor mu (`noexcept`, `throw()`)?
    pub noexcept: bool,
    /// Bildirimin kaynaktaki yeri.
    pub location: SourceLocation,
}

/// Bir sınıf veya struct tanımı. Yalnızca public üyeler alınır.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Class {
    /// Sınıfın adı.
    pub name: String,
    /// Dıştan içe namespace yolu.
    pub namespace: Vec<String>,
    /// Saf sanal metodu olduğu için doğrudan oluşturulamıyor mu?
    pub is_abstract: bool,
    /// Polimorfik mi: kendisi ya da bir taban sınıfı (erişimi ne olursa olsun) sanal bir
    /// fonksiyon bildiriyor mu? Polimorfik bir sınıfa işaret eden pointer aslında bir alt
    /// sınıfı gösteriyor olabilir; böyle bir nesneyi `delete` ile silmek sanal destructor ister.
    pub is_polymorphic: bool,
    /// Destructor'ın erişimi ve sanallığı. Açıkça bildirilmemişse örtük destructor kaydedilir.
    pub destructor: Destructor,
    /// Açıkça bildirilmiş public constructor'lar (`= delete` olanlar hariç).
    pub constructors: Vec<Constructor>,
    /// Public metotlar (statik olanlar dahil).
    pub methods: Vec<Method>,
    /// Bu sürümde desteklenmeyen public üyeler (operatörler, iç içe tipler...).
    pub unsupported_members: Vec<Unsupported>,
    /// Statik olmayan veri üyeleri (erişimi ne olursa olsun), bildirim sırasıyla.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<Field>,
    /// Bellek düzeni; libclang hesaplayamadıysa (ör. eksik tip) `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Layout>,
    /// Kullanıcı kopyalama/taşıma constructor'ı, atama operatörü ya da destructor bildirmiş mi
    /// (`= default` olanlar hariç)? Böyle bir sınıf bit bit kopyalanamayabilir.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub user_copy_or_move: bool,
    /// Bir template örneklemesiyse (`[templates]`) hangisi; `name` o zaman Rust adıdır.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_of: Option<TemplateInstance>,
    /// Tanımın kaynaktaki yeri.
    pub location: SourceLocation,
}

/// Bir sınıfın statik olmayan veri üyesi.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    /// Alanın adı.
    pub name: String,
    /// Tipi; bit alanları ve diziler `Type::Unsupported`.
    #[serde(rename = "type")]
    pub ty: QualType,
    /// Public mi?
    pub public: bool,
    /// Sınıfın başından itibaren bit cinsinden ofset; hesaplanamadıysa `None`.
    pub offset_bits: Option<u64>,
    /// Bildirimin kaynaktaki yeri.
    pub location: SourceLocation,
}

/// Bir tipin bellek düzeni.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layout {
    /// `sizeof`
    pub size_bytes: u64,
    /// `alignof`
    pub align_bytes: u64,
}

/// Bir sınıf template'inin örneklemesi.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateInstance {
    /// Template'in adı (ör. `Vector3`).
    pub template: String,
    /// Örneklemenin tam C++ yazımı, namespace'iyle (ör. `Vector3<float>`, `geo::Box<int>`).
    pub spelling: String,
}

/// Bir sınıfın destructor'ı. Rust'ta `UniquePtr<T>` düşünce `delete` çalıştığından, sahipliği
/// Rust'a geçen her nesne için güvenli olup olmadığı buna bakılarak belirlenir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Destructor {
    /// Public ve silinmemiş. Açıkça bildirilmemiş (örtük) destructor da buraya girer.
    Public {
        /// Sanal mı (kendisi `virtual` bildirilmiş ya da bir taban sınıfınınki sanal)?
        is_virtual: bool,
    },
    /// `private` veya `protected`: sınıf dışından silinemez.
    NotPublic,
    /// `= delete`
    Deleted,
}

/// Bir constructor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constructor {
    /// Parametreler, sırasıyla.
    pub params: Vec<Param>,
    /// Exception fırlatmama garantisi var mı?
    pub noexcept: bool,
    /// Bildirimin kaynaktaki yeri.
    pub location: SourceLocation,
}

/// Bir üye fonksiyon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Method {
    /// Metodun adı.
    pub name: String,
    /// Parametreler, sırasıyla (`this` hariç).
    pub params: Vec<Param>,
    /// Dönüş tipi.
    pub return_type: QualType,
    /// `const` metot mu (nesneyi değiştirmiyor)?
    pub is_const: bool,
    /// Statik metot mu?
    pub is_static: bool,
    /// Sanal metot mu?
    pub is_virtual: bool,
    /// Exception fırlatmama garantisi var mı?
    pub noexcept: bool,
    /// Operatör metoduysa operatörün kendisi (`"+"`, `"+="`, `"=="`...); `name` o zaman
    /// `operator+` gibidir.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    /// Bildirimin kaynaktaki yeri.
    pub location: SourceLocation,
}

/// Bir enum tanımı.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Enum {
    /// Enum'un adı.
    pub name: String,
    /// Dıştan içe namespace yolu.
    pub namespace: Vec<String>,
    /// `enum class` mı?
    pub scoped: bool,
    /// Altta yatan tamsayı tipi.
    pub underlying: Type,
    /// Değerler, kaynak sırasıyla.
    pub variants: Vec<EnumVariant>,
    /// Tanımın kaynaktaki yeri.
    pub location: SourceLocation,
}

/// Bir enum değeri.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumVariant {
    /// Değerin adı.
    pub name: String,
    /// Sayısal değeri.
    pub value: i64,
}

/// `const`/`constexpr` bir değişken; değeri header'dan hesaplandı.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constant {
    /// Sabitin adı.
    pub name: String,
    /// Dıştan içe namespace yolu.
    pub namespace: Vec<String>,
    /// Sabitin tipi: [`Type::Integer`], [`Type::Float`] ya da [`Type::Bool`].
    #[serde(rename = "type")]
    pub ty: Type,
    /// Hesaplanan değer.
    pub value: ConstantValue,
    /// Bildirimin kaynaktaki yeri.
    pub location: SourceLocation,
}

/// Bir sabitin değeri.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ConstantValue {
    /// İşaretli tamsayı tipli sabitin değeri.
    Signed(i64),
    /// İşaretsiz tamsayı tipli sabitin değeri.
    Unsigned(u64),
    /// Kayan noktalı sayı. Her zaman sonlu: NaN ve sonsuz değerli sabitler IR'a girmez. `float`
    /// sabitlerde değer, `f32`'ye kayıpsız dönüşen bir `f64`'tür.
    Float(f64),
    /// `bool`.
    Bool(bool),
}

// `f64` `Eq` değildir, çünkü NaN kendisine eşit değildir. IR'daki değerler her zaman sonlu
// olduğu için (bkz. `ConstantValue::Float`) eşitlik burada bir denklik ilişkisidir.
impl Eq for ConstantValue {}

/// Bir fonksiyon parametresi.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Param {
    /// Parametre adı; header'da isimsizse `None`.
    pub name: Option<String>,
    /// Parametrenin tipi.
    #[serde(rename = "type")]
    pub ty: QualType,
}

/// Desteklenmeyen bir yapı.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unsupported {
    /// Yapının türü (ör. `class`, `enum`, `function_template`).
    pub construct: String,
    /// Namespace'iyle birlikte adı (ör. `geo::Vehicle`); isimsizse boş.
    pub name: String,
    /// Neden desteklenmediği ve ne zaman desteklenmesinin planlandığı.
    pub reason: String,
    /// Bildirimin kaynaktaki yeri.
    pub location: SourceLocation,
}

/// Kaynak dosyadaki bir konum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    /// Okunan header'a göre dosya adı (mutlak yol içermez).
    pub file: String,
    /// 1'den başlayan satır numarası.
    pub line: u32,
    /// 1'den başlayan sütun numarası.
    pub column: u32,
}
