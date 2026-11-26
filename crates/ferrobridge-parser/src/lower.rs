//! libclang AST'sini IR'a çevirme.
//!
//! Yalnızca ayrıştırılan ana dosyadaki bildirimler alınır; `#include` edilen dosyalar alınmaz.
//! Desteklenmeyen hiçbir yapı sessizce atlanmaz: `Item::Unsupported`,
//! `Class::unsupported_members` ya da `Type::Unsupported` olarak nedeniyle kaydedilir.
//!
//! Bilerek kaydedilmeyenler (bağlanacak bir API olmadıkları için): ileri bildirimler
//! (`class Foo;`), typedef/using'ler (kullanıldıkları tipte çözülür), `= delete` ile silinmiş
//! fonksiyonlar, destructor'lar (`UniquePtr` tarafından çağrılır), private/protected üyeler.

use std::path::Path;

use clang_sys as cx;
use ferrobridge_ir::{
    Class, Constructor, Destructor, Enum, EnumVariant, Function, Item, Method, Module, Param,
    QualType, SourceLocation, Type, Unsupported,
};

use crate::clang::{self, Cursor, TranslationUnit};

/// Anonim namespace'in IR'daki adı.
const ANONYMOUS_NAMESPACE: &str = "(anonim)";

const REASON_UNION: &str = "Union'lar henüz desteklenmiyor.";
const REASON_ANONYMOUS: &str = "İsimsiz sınıf ve enum'lar Rust'tan adlandırılamaz.";
const REASON_NESTED: &str = "İç içe tipler (sınıf içinde tanımlı sınıf/enum) v0.2 hedefi.";
const REASON_TEMPLATE: &str = "Template'ler MVP kapsamı dışında (v0.2 hedefi).";
const REASON_VARIABLE: &str = "Global ve statik değişkenler henüz desteklenmiyor.";
const REASON_FIELD: &str = "Alanlara doğrudan erişim v0.2 hedefi; erişim metotları kullanın.";
const REASON_OPERATOR: &str = "Operatörler v0.2 hedefi.";
const REASON_CONVERSION: &str = "Dönüşüm operatörleri v0.2 hedefi.";
const REASON_INHERITANCE: &str =
    "Kalıtım v0.2 hedefi: base sınıfın metotları bu sınıf üzerinden bağlanmıyor.";
const REASON_INTERNAL: &str =
    "İç bağlantılı fonksiyonlar (static veya anonim namespace içinde) header dışından çağrılamaz.";
const REASON_VARIADIC: &str = "Değişken sayıda argüman alan (`...`) fonksiyonlar desteklenmiyor.";
const REASON_ENUM_RANGE: &str = "Enum değeri 64 bit işaretli tamsayıya sığmıyor.";
const REASON_STD_TYPE: &str = "Bu standart kütüphane tipi henüz desteklenmiyor.";
const REASON_CUSTOM_DELETER: &str = "Özel silicili `std::unique_ptr` desteklenmiyor.";
const REASON_CUSTOM_ALLOCATOR: &str = "Özel allocator'lı `std::vector` desteklenmiyor.";
const REASON_UNKNOWN_DECL: &str = "Bu bildirim türü henüz tanınmıyor.";
const REASON_UNKNOWN_TYPE: &str = "Bu tip henüz desteklenmiyor.";

/// Typedef zincirinde en fazla bu kadar adım izlenir (döngülere karşı güvenlik sınırı).
const MAX_TYPEDEF_DEPTH: usize = 16;
/// Taban sınıf zincirinde en fazla bu kadar derine inilir; ötesi bilinmiyor sayılır.
const MAX_BASE_DEPTH: usize = 32;

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
            cx::CXCursor_StructDecl | cx::CXCursor_ClassDecl if cursor.is_definition() => {
                self.class(cursor);
            }
            cx::CXCursor_EnumDecl if cursor.is_definition() => self.enumeration(cursor),
            cx::CXCursor_UnionDecl if cursor.is_definition() => {
                self.unsupported(cursor, "union", REASON_UNION);
            }
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
            // Bağlanacak bir API değil (modül belgesine bakın).
            cx::CXCursor_StructDecl
            | cx::CXCursor_ClassDecl
            | cx::CXCursor_EnumDecl
            | cx::CXCursor_UnionDecl
            | cx::CXCursor_TypedefDecl
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
        if cursor.is_deleted() {
            return;
        }
        let linkage = cursor.linkage();
        if linkage == cx::CXLinkage_Internal || linkage == cx::CXLinkage_UniqueExternal {
            return self.unsupported(cursor, "function", REASON_INTERNAL);
        }
        if cursor.ty().is_variadic() {
            return self.unsupported(cursor, "variadic_function", REASON_VARIADIC);
        }
        self.items.push(Item::Function(Function {
            name: cursor.spelling(),
            namespace: self.namespace.clone(),
            params: params(cursor),
            return_type: lower_type(cursor.result_type()),
            noexcept: is_noexcept(cursor.exception_spec()),
            location: location(cursor),
        }));
    }

    fn class(&mut self, cursor: Cursor<'_>) {
        if cursor.is_anonymous() {
            return self.unsupported(cursor, "class", REASON_ANONYMOUS);
        }
        let name = cursor.spelling();
        let qualified = self.qualified(&name);
        let lifetime = record_lifetime(cursor, 0);
        let mut class = Class {
            name,
            namespace: self.namespace.clone(),
            is_abstract: cursor.is_abstract_record(),
            is_polymorphic: lifetime.polymorphic,
            destructor: lifetime.destructor(),
            constructors: Vec::new(),
            methods: Vec::new(),
            unsupported_members: Vec::new(),
            location: location(cursor),
        };
        for member in cursor.children() {
            class_member(&mut class, &qualified, member);
        }
        self.items.push(Item::Class(class));
    }

    fn enumeration(&mut self, cursor: Cursor<'_>) {
        if cursor.is_anonymous() {
            return self.unsupported(cursor, "enum", REASON_ANONYMOUS);
        }
        let underlying = lower_type(cursor.enum_integer_type()).ty;
        let unsigned = matches!(underlying, Type::Integer { signed: false, .. });
        let mut variants = Vec::new();
        for constant in cursor.children() {
            if constant.kind() != cx::CXCursor_EnumConstantDecl {
                continue;
            }
            let value = if unsigned {
                i64::try_from(constant.enum_unsigned_value()).ok()
            } else {
                Some(constant.enum_value())
            };
            let Some(value) = value else {
                return self.unsupported(cursor, "enum", REASON_ENUM_RANGE);
            };
            variants.push(EnumVariant {
                name: constant.spelling(),
                value,
            });
        }
        self.items.push(Item::Enum(Enum {
            name: cursor.spelling(),
            namespace: self.namespace.clone(),
            scoped: cursor.is_scoped_enum(),
            underlying,
            variants,
            location: location(cursor),
        }));
    }

    fn unsupported(&mut self, cursor: Cursor<'_>, construct: &str, reason: &str) {
        let name = self.qualified(&cursor.spelling());
        self.items.push(Item::Unsupported(unsupported(
            cursor, construct, name, reason,
        )));
    }

    /// Geçerli namespace'le birlikte tam ad (`geo::detail::f`).
    fn qualified(&self, name: &str) -> String {
        let mut path = self.namespace.clone();
        if !name.is_empty() {
            path.push(name.to_owned());
        }
        path.join("::")
    }
}

/// Bir sınıfın silinmesiyle ilgili bilgiler (taban sınıflardan gelenler dahil).
struct RecordLifetime {
    /// Kendisi ya da bir tabanı sanal fonksiyon bildiriyor.
    polymorphic: bool,
    /// Açıkça bildirilmiş destructor (yoksa `None`).
    declared: Option<DeclaredDestructor>,
    /// Bir tabanın destructor'ı sanal: bu sınıfınki de sanaldır.
    base_virtual: bool,
    /// Bir tabanın destructor'ı private veya silinmiş: örtük destructor silinmiş sayılır.
    base_blocks_implicit: bool,
}

/// Açıkça bildirilmiş bir destructor'ın özellikleri.
#[derive(Clone, Copy)]
struct DeclaredDestructor {
    access: cx::CX_CXXAccessSpecifier,
    deleted: bool,
    is_virtual: bool,
}

impl RecordLifetime {
    /// Tanımı görülemeyen bir taban için en kısıtlayıcı varsayım: polimorfik, sanal
    /// destructor'ı yok.
    fn unknown() -> Self {
        RecordLifetime {
            polymorphic: true,
            declared: None,
            base_virtual: false,
            base_blocks_implicit: false,
        }
    }

    fn destructor(&self) -> Destructor {
        match self.declared {
            Some(d) if d.deleted => Destructor::Deleted,
            Some(d) if d.access != cx::CX_CXXPublic => Destructor::NotPublic,
            Some(d) => Destructor::Public {
                is_virtual: d.is_virtual || self.base_virtual,
            },
            None if self.base_blocks_implicit => Destructor::Deleted,
            None => Destructor::Public {
                is_virtual: self.base_virtual,
            },
        }
    }

    /// Bu sınıfın destructor'ı alt sınıflarda sanallığı devrediyor mu?
    fn has_virtual_destructor(&self) -> bool {
        self.declared.is_some_and(|d| d.is_virtual) || self.base_virtual
    }

    /// Alt sınıfın örtük destructor'ı bu tabanın destructor'ını çağıramaz mı?
    fn blocks_implicit_in_derived(&self) -> bool {
        match self.declared {
            Some(d) => d.deleted || d.access == cx::CX_CXXPrivate,
            None => self.base_blocks_implicit,
        }
    }
}

/// Bir sınıfın destructor'ını ve polimorfikliğini, taban sınıflarına özyinelemeyle bakarak
/// bulur. Üyelerin (alanların) destructor'ları dikkate alınmaz: silinmiş bir alan
/// destructor'ı örtük destructor'ı siler, ama bu UB değil derleme hatasına yol açar.
fn record_lifetime(record: Cursor<'_>, depth: usize) -> RecordLifetime {
    if depth > MAX_BASE_DEPTH || !record.is_definition() {
        return RecordLifetime::unknown();
    }
    let mut info = RecordLifetime {
        polymorphic: false,
        declared: None,
        base_virtual: false,
        base_blocks_implicit: false,
    };
    for member in record.children() {
        match member.kind() {
            cx::CXCursor_CXXBaseSpecifier => {
                let base = record_lifetime(member.ty().canonical().declaration(), depth + 1);
                info.polymorphic |= base.polymorphic;
                info.base_virtual |= base.has_virtual_destructor();
                info.base_blocks_implicit |= base.blocks_implicit_in_derived();
            }
            cx::CXCursor_Destructor => {
                let is_virtual = member.is_virtual_method();
                info.polymorphic |= is_virtual;
                info.declared = Some(DeclaredDestructor {
                    access: member.access(),
                    deleted: member.is_deleted(),
                    is_virtual,
                });
            }
            cx::CXCursor_CXXMethod => info.polymorphic |= member.is_virtual_method(),
            _ => {}
        }
    }
    info
}

/// Bir sınıf üyesini sınıfa ekler. Yalnızca public üyeler dikkate alınır.
fn class_member(class: &mut Class, class_name: &str, member: Cursor<'_>) {
    let kind = member.kind();
    // Kalıtımın erişimi (public/private base) değil, varlığı önemli: bildirilir.
    if kind == cx::CXCursor_CXXBaseSpecifier {
        let name = format!("{class_name} : {}", member.spelling());
        class
            .unsupported_members
            .push(unsupported(member, "base_class", name, REASON_INHERITANCE));
        return;
    }
    if member.access() != cx::CX_CXXPublic {
        return;
    }
    let member_name = format!("{class_name}::{}", member.spelling());
    match kind {
        cx::CXCursor_Constructor if !member.is_deleted() => {
            class.constructors.push(Constructor {
                params: params(member),
                noexcept: is_noexcept(member.exception_spec()),
                location: location(member),
            });
        }
        cx::CXCursor_CXXMethod if !member.is_deleted() => {
            if member.spelling().starts_with("operator") {
                class.unsupported_members.push(unsupported(
                    member,
                    "operator",
                    member_name,
                    REASON_OPERATOR,
                ));
                return;
            }
            class.methods.push(Method {
                name: member.spelling(),
                params: params(member),
                return_type: lower_type(member.result_type()),
                is_const: member.is_const_method(),
                is_static: member.is_static_method(),
                is_virtual: member.is_virtual_method(),
                noexcept: is_noexcept(member.exception_spec()),
                location: location(member),
            });
        }
        cx::CXCursor_ConversionFunction => class.unsupported_members.push(unsupported(
            member,
            "conversion_operator",
            member_name,
            REASON_CONVERSION,
        )),
        cx::CXCursor_FieldDecl => {
            class
                .unsupported_members
                .push(unsupported(member, "field", member_name, REASON_FIELD))
        }
        cx::CXCursor_VarDecl => class.unsupported_members.push(unsupported(
            member,
            "static_variable",
            member_name,
            REASON_VARIABLE,
        )),
        cx::CXCursor_StructDecl
        | cx::CXCursor_ClassDecl
        | cx::CXCursor_UnionDecl
        | cx::CXCursor_EnumDecl => class.unsupported_members.push(unsupported(
            member,
            "nested_type",
            member_name,
            REASON_NESTED,
        )),
        cx::CXCursor_FunctionTemplate => class.unsupported_members.push(unsupported(
            member,
            "method_template",
            member_name,
            REASON_TEMPLATE,
        )),
        // Bağlanacak bir API değil (modül belgesine bakın).
        cx::CXCursor_Constructor
        | cx::CXCursor_CXXMethod
        | cx::CXCursor_Destructor
        | cx::CXCursor_CXXAccessSpecifier
        | cx::CXCursor_FriendDecl
        | cx::CXCursor_TypedefDecl
        | cx::CXCursor_TypeAliasDecl
        | cx::CXCursor_UsingDeclaration
        | cx::CXCursor_StaticAssert => {}
        _ => {
            let construct = member.kind_spelling();
            class.unsupported_members.push(unsupported(
                member,
                &construct,
                member_name,
                REASON_UNKNOWN_DECL,
            ));
        }
    }
}

fn unsupported(cursor: Cursor<'_>, construct: &str, name: String, reason: &str) -> Unsupported {
    Unsupported {
        construct: construct.to_owned(),
        name,
        reason: reason.to_owned(),
        location: location(cursor),
    }
}

fn params(cursor: Cursor<'_>) -> Vec<Param> {
    cursor
        .arguments()
        .into_iter()
        .map(|arg| {
            let name = arg.spelling();
            Param {
                name: (!name.is_empty()).then_some(name),
                ty: lower_type(arg.ty()),
            }
        })
        .collect()
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
        _ if integer_info(kind).is_some() => {
            let (builtin, signed) = integer_info(kind).unwrap_or(("", false));
            let size_bytes = canonical.size_of().unwrap_or(0);
            Type::Integer {
                spelling: unqualified_spelling(ty),
                builtin: builtin.to_owned(),
                size_bytes,
                signed,
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
        cx::CXType_Record => lower_record(ty, canonical),
        cx::CXType_Enum => match scope_path(canonical.declaration()) {
            Some((namespace, name)) => Type::Enum { name, namespace },
            None => unsupported_type(ty, REASON_NESTED),
        },
        _ => unsupported_type(ty, REASON_UNKNOWN_TYPE),
    };
    QualType {
        is_const,
        ty: lowered,
    }
}

/// Sınıf tiplerini çevirir: `std::string` ve `std::unique_ptr<T>` özel olarak tanınır.
///
/// Tanıma ad metniyle değil, bildirimin namespace zinciri ve template argümanlarıyla yapılır;
/// libc++'ın `std::__1` gibi inline namespace'leri yok sayılır.
fn lower_record(ty: clang::Type<'_>, canonical: clang::Type<'_>) -> Type {
    let declaration = canonical.declaration();
    if declaration.kind() == cx::CXCursor_UnionDecl {
        return unsupported_type(ty, REASON_UNION);
    }
    let Some((namespace, name)) = scope_path(declaration) else {
        return unsupported_type(ty, REASON_NESTED);
    };
    let args = canonical.template_args();
    if namespace.first().is_some_and(|ns| ns == "std") {
        let is_char = |t: &clang::Type<'_>| {
            matches!(t.canonical().kind(), cx::CXType_Char_S | cx::CXType_Char_U)
        };
        return match (name.as_str(), args.as_slice()) {
            ("basic_string", [first, ..]) if namespace.len() == 1 && is_char(first) => Type::String,
            ("vector", [element, allocator]) if namespace.len() == 1 => {
                if is_std_decl(allocator.canonical().declaration(), "allocator") {
                    Type::Vector {
                        element: Box::new(lower_type(*element)),
                    }
                } else {
                    unsupported_type(ty, REASON_CUSTOM_ALLOCATOR)
                }
            }
            ("unique_ptr", [pointee, deleter]) if namespace.len() == 1 => {
                let deleter = deleter.canonical().declaration();
                if deleter.spelling() == "default_delete" {
                    Type::UniquePtr {
                        pointee: Box::new(lower_type(*pointee)),
                    }
                } else {
                    unsupported_type(ty, REASON_CUSTOM_DELETER)
                }
            }
            _ => unsupported_type(ty, REASON_STD_TYPE),
        };
    }
    if !args.is_empty() {
        return unsupported_type(ty, REASON_TEMPLATE);
    }
    Type::Record { name, namespace }
}

/// Bildirim doğrudan `std` namespace'inde (inline namespace'ler hariç) ve bu adda mı?
fn is_std_decl(declaration: Cursor<'_>, expected: &str) -> bool {
    scope_path(declaration)
        .is_some_and(|(namespace, name)| namespace == ["std"] && name == expected)
}

/// Bir bildirimin namespace yolu ve adı. Inline namespace'ler atlanır. Sınıf veya fonksiyon
/// içinde tanımlıysa ya da isimsizse `None`.
fn scope_path(declaration: Cursor<'_>) -> Option<(Vec<String>, String)> {
    let name = declaration.spelling();
    if name.is_empty() || declaration.is_anonymous() {
        return None;
    }
    let mut namespace = Vec::new();
    let mut parent = declaration.semantic_parent();
    for _ in 0..MAX_TYPEDEF_DEPTH * 4 {
        match parent.kind() {
            cx::CXCursor_TranslationUnit => {
                namespace.reverse();
                return Some((namespace, name));
            }
            cx::CXCursor_Namespace => {
                if !parent.is_inline_namespace() {
                    let segment = parent.spelling();
                    namespace.push(if segment.is_empty() {
                        ANONYMOUS_NAMESPACE.to_owned()
                    } else {
                        segment
                    });
                }
            }
            cx::CXCursor_LinkageSpec => {}
            _ => return None,
        }
        parent = parent.semantic_parent();
    }
    None
}

/// Tamsayı tipiyse C++ temel tip adı ve işaretli olup olmadığı; tamsayı değilse `None`.
/// `char16_t`, `wchar_t` ve 128 bitlik tipler bilerek dışarıda.
fn integer_info(kind: cx::CXTypeKind) -> Option<(&'static str, bool)> {
    match kind {
        cx::CXType_Char_S => Some(("char", true)),
        cx::CXType_Char_U => Some(("char", false)),
        cx::CXType_SChar => Some(("signed char", true)),
        cx::CXType_UChar => Some(("unsigned char", false)),
        cx::CXType_Short => Some(("short", true)),
        cx::CXType_UShort => Some(("unsigned short", false)),
        cx::CXType_Int => Some(("int", true)),
        cx::CXType_UInt => Some(("unsigned int", false)),
        cx::CXType_Long => Some(("long", true)),
        cx::CXType_ULong => Some(("unsigned long", false)),
        cx::CXType_LongLong => Some(("long long", true)),
        cx::CXType_ULongLong => Some(("unsigned long long", false)),
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
