//! Header'da bulunan üst seviye öğeler.

use serde::{Deserialize, Serialize};

use crate::QualType;

/// Header'dan okunan bir öğe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Item {
    /// Serbest (sınıf üyesi olmayan) bir fonksiyon.
    Function(Function),
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
