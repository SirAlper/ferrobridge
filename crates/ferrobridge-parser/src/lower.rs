//! libclang AST'sini IR'a çevirme.
//!
//! Yalnızca ayrıştırılan ana dosyadaki bildirimler alınır; `#include` edilen dosyalar alınmaz.
//! Desteklenmeyen hiçbir yapı sessizce atlanmaz: `Item::Unsupported` ya da
//! `Type::Unsupported` olarak nedeniyle kaydedilir.

use std::path::Path;

use clang_sys as cx;
use ferrobridge_ir::{Function, Item, Module, Param, QualType, SourceLocation, Type, Unsupported};

use crate::clang::{self, Cursor, TranslationUnit};

/// Anonim namespace'in IR'daki adı.
const ANONYMOUS_NAMESPACE: &str = "(anonim)";

const REASON_RECORD: &str = "Sınıf ve struct'lar Milestone 2'de desteklenecek.";
const REASON_ENUM: &str = "Enum'lar Milestone 2'de desteklenecek.";
const REASON_TEMPLATE: &str = "Template'ler MVP kapsamı dışında (v0.2 hedefi).";
const REASON_VARIABLE: &str = "Global değişkenler henüz desteklenmiyor.";
const REASON_INTERNAL: &str =
    "İç bağlantılı fonksiyonlar (static veya anonim namespace içinde) header dışından çağrılamaz.";
const REASON_VARIADIC: &str = "Değişken sayıda argüman alan (`...`) fonksiyonlar desteklenmiyor.";
const REASON_UNKNOWN_DECL: &str = "Bu bildirim türü henüz tanınmıyor.";
const REASON_UNKNOWN_TYPE: &str = "Bu tip henüz desteklenmiyor.";

/// Typedef zincirinde en fazla bu kadar adım izlenir (döngülere karşı güvenlik sınırı).
const MAX_TYPEDEF_DEPTH: usize = 16;

/// Translation unit'teki ana dosya bildirimlerini IR modülüne çevirir.
pub(crate) fn lower(tu: &TranslationUnit<'_>) -> Module {
    let mut lowering = Lowering::default();
    lowering.visit_children(tu.cursor());
    Module {
        items: lowering.items,
        ..Module::new()
    }
}

#[derive(Default)]
struct Lowering {
    namespace: Vec<String>,
    items: Vec<Item>,
}

impl Lowering {
    fn visit_children(&mut self, parent: Cursor<'_>) {
        for child in parent.children() {
            if child.is_in_main_file() {
                self.visit(child);
            }
        }
    }

    fn visit(&mut self, cursor: Cursor<'_>) {
        match cursor.kind() {
            cx::CXCursor_Namespace => {
                let name = cursor.spelling();
                self.namespace.push(if name.is_empty() {
                    ANONYMOUS_NAMESPACE.to_owned()
                } else {
                    name
                });
                self.visit_children(cursor);
                self.namespace.pop();
            }
            cx::CXCursor_LinkageSpec => self.visit_children(cursor),
            cx::CXCursor_FunctionDecl => self.function(cursor),
            cx::CXCursor_StructDecl => self.unsupported(cursor, "struct", REASON_RECORD),
            cx::CXCursor_ClassDecl => self.unsupported(cursor, "class", REASON_RECORD),
            cx::CXCursor_UnionDecl => self.unsupported(cursor, "union", REASON_RECORD),
            cx::CXCursor_EnumDecl => self.unsupported(cursor, "enum", REASON_ENUM),
            cx::CXCursor_FunctionTemplate => {
                self.unsupported(cursor, "function_template", REASON_TEMPLATE);
            }
            cx::CXCursor_ClassTemplate | cx::CXCursor_ClassTemplatePartialSpecialization => {
                self.unsupported(cursor, "class_template", REASON_TEMPLATE);
            }
            cx::CXCursor_TypeAliasTemplateDecl => {
                self.unsupported(cursor, "alias_template", REASON_TEMPLATE);
            }
            cx::CXCursor_VarDecl => self.unsupported(cursor, "variable", REASON_VARIABLE),
            // Bunlar tek başına bağlanacak bir API değil: typedef ve alias'lar kullanıldıkları
            // tipte çözülür, diğerleri yalnızca derleme zamanında anlam taşır.
            cx::CXCursor_TypedefDecl
            | cx::CXCursor_TypeAliasDecl
            | cx::CXCursor_UsingDirective
            | cx::CXCursor_UsingDeclaration
            | cx::CXCursor_NamespaceAlias
            | cx::CXCursor_StaticAssert => {}
            _ => {
                let construct = cursor.kind_spelling();
                self.unsupported(cursor, &construct, REASON_UNKNOWN_DECL);
            }
        }
    }

    fn function(&mut self, cursor: Cursor<'_>) {
        let linkage = cursor.linkage();
        if linkage == cx::CXLinkage_Internal || linkage == cx::CXLinkage_UniqueExternal {
            return self.unsupported(cursor, "function", REASON_INTERNAL);
        }
        if cursor.ty().is_variadic() {
            return self.unsupported(cursor, "variadic_function", REASON_VARIADIC);
        }
        let params = cursor
            .arguments()
            .into_iter()
            .map(|arg| {
                let name = arg.spelling();
                Param {
                    name: (!name.is_empty()).then_some(name),
                    ty: lower_type(arg.ty()),
                }
            })
            .collect();
        self.items.push(Item::Function(Function {
            name: cursor.spelling(),
            namespace: self.namespace.clone(),
            params,
            return_type: lower_type(cursor.result_type()),
            noexcept: is_noexcept(cursor.exception_spec()),
            location: location(cursor),
        }));
    }

    fn unsupported(&mut self, cursor: Cursor<'_>, construct: &str, reason: &str) {
        let mut path = self.namespace.clone();
        let name = cursor.spelling();
        if !name.is_empty() {
            path.push(name);
        }
        self.items.push(Item::Unsupported(Unsupported {
            construct: construct.to_owned(),
            name: path.join("::"),
            reason: reason.to_owned(),
            location: location(cursor),
        }));
    }
}

/// `noexcept`, `noexcept(true)` ve `throw()` exception fırlatmama garantisi verir.
/// `noexcept(ifade)` değeri bilinemediği için ihtiyatlı davranılır: garanti yok sayılır.
fn is_noexcept(spec: i32) -> bool {
    matches!(
        spec,
        cx::CXCursor_ExceptionSpecificationKind_BasicNoexcept
            | cx::CXCursor_ExceptionSpecificationKind_DynamicNone
            | cx::CXCursor_ExceptionSpecificationKind_NoThrow
    )
}

fn location(cursor: Cursor<'_>) -> SourceLocation {
    let clang::Location { file, line, column } = cursor.location();
    let file = Path::new(&file)
        .file_name()
        .map_or(file.clone(), |name| name.to_string_lossy().into_owned());
    SourceLocation { file, line, column }
}

fn lower_type(ty: clang::Type<'_>) -> QualType {
    let is_const = ty.is_const();
    let canonical = ty.canonical();
    let kind = canonical.kind();
    let lowered = match kind {
        cx::CXType_Void => Type::Void,
        cx::CXType_Bool => Type::Bool,
        _ if integer_signedness(kind).is_some() => {
            let size_bytes = canonical.size_of().unwrap_or(0);
            Type::Integer {
                spelling: unqualified_spelling(ty),
                size_bytes,
                signed: integer_signedness(kind).unwrap_or(false),
                fixed_width: is_fixed_width(ty, size_bytes),
            }
        }
        cx::CXType_Float | cx::CXType_Double => Type::Float {
            spelling: unqualified_spelling(ty),
            size_bytes: canonical.size_of().unwrap_or(0),
        },
        cx::CXType_Pointer => Type::Pointer {
            pointee: Box::new(lower_type(pointee_of(ty, canonical))),
        },
        cx::CXType_LValueReference => Type::LValueRef {
            referent: Box::new(lower_type(pointee_of(ty, canonical))),
        },
        cx::CXType_RValueReference => Type::RValueRef {
            referent: Box::new(lower_type(pointee_of(ty, canonical))),
        },
        cx::CXType_Record => unsupported_type(ty, REASON_RECORD),
        cx::CXType_Enum => unsupported_type(ty, REASON_ENUM),
        _ => unsupported_type(ty, REASON_UNKNOWN_TYPE),
    };
    QualType {
        is_const,
        ty: lowered,
    }
}

/// Tamsayı tipiyse işaretli olup olmadığı; tamsayı değilse `None`.
/// `char16_t`, `wchar_t` ve 128 bitlik tipler bilerek dışarıda.
fn integer_signedness(kind: cx::CXTypeKind) -> Option<bool> {
    match kind {
        cx::CXType_Char_S
        | cx::CXType_SChar
        | cx::CXType_Short
        | cx::CXType_Int
        | cx::CXType_Long
        | cx::CXType_LongLong => Some(true),
        cx::CXType_Char_U
        | cx::CXType_UChar
        | cx::CXType_UShort
        | cx::CXType_UInt
        | cx::CXType_ULong
        | cx::CXType_ULongLong => Some(false),
        _ => None,
    }
}

/// Pointer/referansın gösterdiği tip. Typedef'li pointer'larda (ör. `typedef int* IntPtr`)
/// şekerli tipten alınamazsa kanonik tipten alınır.
fn pointee_of<'tu>(ty: clang::Type<'tu>, canonical: clang::Type<'tu>) -> clang::Type<'tu> {
    let pointee = ty.pointee();
    if pointee.kind() == cx::CXType_Invalid {
        canonical.pointee()
    } else {
        pointee
    }
}

fn unsupported_type(ty: clang::Type<'_>, reason: &str) -> Type {
    Type::Unsupported {
        spelling: unqualified_spelling(ty),
        reason: reason.to_owned(),
    }
}

/// Kaynakta yazıldığı haliyle tip adı; `const` ayrı alanda tutulduğu için çıkarılır.
fn unqualified_spelling(ty: clang::Type<'_>) -> String {
    let spelling = ty.spelling();
    spelling
        .strip_prefix("const ")
        .map_or(spelling.clone(), str::to_owned)
}

/// Tip, typedef zincirinin herhangi bir adımında `std::int32_t` gibi sabit genişlikli bir
/// tipe ulaşıyor mu?
///
/// Ad üzerinden tanınır, çünkü libclang `std::int32_t`'yi (std içindeki `using ::int32_t;`)
/// bildirimi olmayan "unexposed" bir tip olarak verir ve zincir orada izlenemez. Aynı adı
/// taşıyan ama farklı boyutta bir kullanıcı tipine güvenmemek için gerçek boyut da
/// karşılaştırılır.
fn is_fixed_width(ty: clang::Type<'_>, size_bytes: u64) -> bool {
    let mut current = ty;
    for _ in 0..MAX_TYPEDEF_DEPTH {
        if let Some(bits) = fixed_width_bits(&unqualified_spelling(current)) {
            return u64::from(bits) == size_bytes * 8;
        }
        current = match current.kind() {
            cx::CXType_Elaborated => current.named(),
            cx::CXType_Typedef => current.declaration().typedef_underlying(),
            _ => return false,
        };
    }
    false
}

/// `int32_t`, `std::uint8_t` gibi bir adın bit genişliği; böyle bir ad değilse `None`.
fn fixed_width_bits(spelling: &str) -> Option<u32> {
    let name = spelling.rsplit("::").next()?;
    let digits = name
        .strip_prefix('u')
        .unwrap_or(name)
        .strip_prefix("int")?
        .strip_suffix("_t")?;
    match digits {
        "8" => Some(8),
        "16" => Some(16),
        "32" => Some(32),
        "64" => Some(64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_fixed_width_names() {
        assert_eq!(fixed_width_bits("int32_t"), Some(32));
        assert_eq!(fixed_width_bits("std::uint8_t"), Some(8));
        assert_eq!(fixed_width_bits("::std::int64_t"), Some(64));
        assert_eq!(fixed_width_bits("int"), None);
        assert_eq!(fixed_width_bits("int128_t"), None);
        assert_eq!(fixed_width_bits("uintptr_t"), None);
        assert_eq!(fixed_width_bits("my_id"), None);
    }
}
