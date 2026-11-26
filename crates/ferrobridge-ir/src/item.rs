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
    /// Açıkça bildirilmiş public constructor'lar (`= delete` olanlar hariç).
    pub constructors: Vec<Constructor>,
    /// Public metotlar (statik olanlar dahil).
    pub methods: Vec<Method>,
    /// Bu sürümde desteklenmeyen public üyeler (alanlar, operatörler, iç içe tipler...).
    pub unsupported_members: Vec<Unsupported>,
    /// Tanımın kaynaktaki yeri.
    pub location: SourceLocation,
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
