//! IR tiplerini bridge tiplerine çeviren kurallar (Obsidian `03 Mimari` tip tablosu ve
//! güvenlik kuralları). Bir tip güvenle ifade edilemiyorsa nedeniyle reddedilir; hiçbir
//! durumda tahmin yürütülmez.

use std::collections::HashMap;

use ferrobridge_ir::{QualType, Type};

use crate::plan::{BridgeType, IntType, Returns, TypeRef};

pub(crate) const REASON_POINTER: &str = "Ham pointer'ın sahibi ve ömrü header'dan anlaşılamıyor; \
     `ferrobridge.toml` açıklaması gerekecek (Milestone 3).";
pub(crate) const REASON_MUT_PRIMITIVE_REF: &str =
    "Const olmayan primitif referans (ör. `int&`) için açıklama gerekiyor (Milestone 3).";
const REASON_CLASS_BY_VALUE: &str = "Sınıfı değerle almak kopyalama/taşıma gerektiriyor; henüz \
     desteklenmiyor (`const T&` kullanın).";
const REASON_STRING_BY_VALUE: &str =
    "`std::string`'i değerle almak henüz desteklenmiyor (`const std::string&` kullanın).";
const REASON_RVALUE: &str = "Rvalue referanslar (`T&&`) henüz desteklenmiyor.";
const REASON_REF_RETURN_FREE: &str = "Serbest veya statik fonksiyonun döndürdüğü referansın ömrü \
     belirsiz; açıklama gerekecek (Milestone 3).";
const REASON_MUT_REF_FROM_CONST: &str =
    "Const metot değiştirilebilir referans döndüremez (Rust ödünç alma kuralları).";
const REASON_ABSTRACT_BY_VALUE: &str = "Soyut sınıf değerle döndürülemez.";
const REASON_VOID_PARAM: &str = "`void` parametre olamaz.";
const REASON_UNSUPPORTED_REF: &str = "Bu tipe referans henüz desteklenmiyor.";
const REASON_UNSUPPORTED_UNIQUE: &str =
    "`std::unique_ptr` yalnızca bu header'daki sınıflar ve `std::string` için destekleniyor.";
const REASON_FLOAT_SIZE: &str = "Yalnızca `float` ve `double` destekleniyor.";

/// Her platformda aynı genişlikte olan C++ temel tamsayı tipleri. `long` (Windows'ta 32,
/// diğerlerinde 64 bit) ve `long long` (`std::int64_t` bazı platformlarda `long`) dışarıda.
const PORTABLE_BUILTINS: [&str; 6] = [
    "signed char",
    "unsigned char",
    "short",
    "unsigned short",
    "int",
    "unsigned int",
];

/// Bridge'de adı geçebilecek sınıf ve enum'lar: (namespace, ad) → bilgi.
#[derive(Default)]
pub(crate) struct Known {
    pub(crate) classes: HashMap<(Vec<String>, String), (TypeRef, bool)>,
    pub(crate) enums: HashMap<(Vec<String>, String), TypeRef>,
}

/// Bir parametre tipini çevirir.
pub(crate) fn param_type(qt: &QualType, known: &Known) -> Result<BridgeType, String> {
    match &qt.ty {
        Type::Void => Err(REASON_VOID_PARAM.to_owned()),
        Type::Record { .. } => Err(REASON_CLASS_BY_VALUE.to_owned()),
        Type::String => Err(REASON_STRING_BY_VALUE.to_owned()),
        Type::UniquePtr { pointee } => Ok(BridgeType::UniquePtr(Box::new(unique_inner(
            pointee, known,
        )?))),
        Type::LValueRef { referent } => ref_type(referent, known),
        other => non_ref(other, known),
    }
}

/// Bir dönüş tipini çevirir. `receiver_mutable`: metotsa alıcının değiştirilebilir olup
/// olmadığı; serbest/statik fonksiyonsa `None`.
pub(crate) fn return_type(
    qt: &QualType,
    known: &Known,
    receiver_mutable: Option<bool>,
) -> Result<Returns, String> {
    match &qt.ty {
        Type::Void => Ok(Returns::Unit),
        Type::Record { name, namespace } => {
            let (type_ref, is_abstract) = class(namespace, name, known)?;
            if is_abstract {
                Err(REASON_ABSTRACT_BY_VALUE.to_owned())
            } else {
                Ok(Returns::BoxedClass(type_ref))
            }
        }
        Type::String => Ok(Returns::OwnedString),
        Type::UniquePtr { pointee } => Ok(Returns::Value(BridgeType::UniquePtr(Box::new(
            unique_inner(pointee, known)?,
        )))),
        Type::LValueRef { referent } => {
            let Some(receiver_mutable) = receiver_mutable else {
                return Err(REASON_REF_RETURN_FREE.to_owned());
            };
            let reference = ref_type(referent, known)?;
            if matches!(reference, BridgeType::Ref { mutable: true, .. }) && !receiver_mutable {
                return Err(REASON_MUT_REF_FROM_CONST.to_owned());
            }
            Ok(Returns::Value(reference))
        }
        other => non_ref(other, known).map(Returns::Value),
    }
}

/// Değerle taşınabilen tipler ve kesin reddedilenler.
fn non_ref(ty: &Type, known: &Known) -> Result<BridgeType, String> {
    match ty {
        Type::Bool => Ok(BridgeType::Bool),
        Type::Integer {
            spelling,
            builtin,
            size_bytes,
            signed,
            fixed_width,
        } => {
            if builtin == "char" && !fixed_width {
                return Ok(BridgeType::Char);
            }
            if !fixed_width && !PORTABLE_BUILTINS.contains(&builtin.as_str()) {
                return Err(format!(
                    "`{spelling}` ({builtin}) platformlar arasında farklı genişlikte olabilir; \
                     `std::int64_t` gibi sabit genişlikli bir tip kullanın."
                ));
            }
            IntType::from_layout(*size_bytes, *signed)
                .map(BridgeType::Int)
                .ok_or_else(|| format!("`{spelling}` için uygun tamsayı tipi yok."))
        }
        Type::Float { size_bytes, .. } => match size_bytes {
            4 => Ok(BridgeType::F32),
            8 => Ok(BridgeType::F64),
            _ => Err(REASON_FLOAT_SIZE.to_owned()),
        },
        Type::Enum { name, namespace } => known
            .enums
            .get(&(namespace.clone(), name.clone()))
            .cloned()
            .map(BridgeType::Enum)
            .ok_or_else(|| not_defined(namespace, name)),
        Type::Pointer { .. } => Err(REASON_POINTER.to_owned()),
        Type::RValueRef { .. } => Err(REASON_RVALUE.to_owned()),
        Type::Unsupported { spelling, reason } => Err(format!("`{spelling}`: {reason}")),
        Type::Void
        | Type::Record { .. }
        | Type::String
        | Type::UniquePtr { .. }
        | Type::LValueRef { .. } => Err(REASON_UNSUPPORTED_REF.to_owned()),
    }
}

/// `T&` / `const T&` için tip tablosu.
fn ref_type(referent: &QualType, known: &Known) -> Result<BridgeType, String> {
    let mutable = !referent.is_const;
    let inner = match &referent.ty {
        Type::Record { name, namespace } => BridgeType::Class(class(namespace, name, known)?.0),
        Type::String => BridgeType::String,
        Type::Bool | Type::Integer { .. } | Type::Float { .. } | Type::Enum { .. } => {
            if mutable {
                return Err(REASON_MUT_PRIMITIVE_REF.to_owned());
            }
            non_ref(&referent.ty, known)?
        }
        Type::Pointer { .. } => return Err(REASON_POINTER.to_owned()),
        Type::Unsupported { spelling, reason } => return Err(format!("`{spelling}`: {reason}")),
        _ => return Err(REASON_UNSUPPORTED_REF.to_owned()),
    };
    Ok(BridgeType::Ref {
        mutable,
        inner: Box::new(inner),
    })
}

fn unique_inner(pointee: &QualType, known: &Known) -> Result<BridgeType, String> {
    match &pointee.ty {
        Type::Record { name, namespace } => Ok(BridgeType::Class(class(namespace, name, known)?.0)),
        Type::String => Ok(BridgeType::String),
        _ => Err(REASON_UNSUPPORTED_UNIQUE.to_owned()),
    }
}

fn class(namespace: &[String], name: &str, known: &Known) -> Result<(TypeRef, bool), String> {
    known
        .classes
        .get(&(namespace.to_vec(), name.to_owned()))
        .cloned()
        .ok_or_else(|| not_defined(namespace, name))
}

fn not_defined(namespace: &[String], name: &str) -> String {
    let mut path = namespace.to_vec();
    path.push(name.to_owned());
    format!(
        "`{}` bu header'da tanımlı değil; başka header'lardaki tipler henüz bağlanamıyor.",
        path.join("::")
    )
}

/// Enum'un altta yatan tipi için tamsayı tipi.
pub(crate) fn enum_repr(underlying: &Type) -> Result<IntType, String> {
    match non_ref(underlying, &Known::default())? {
        BridgeType::Int(int) => Ok(int),
        BridgeType::Char => Err("`char` tabanlı enum'lar desteklenmiyor.".to_owned()),
        _ => Err("Enum'un altta yatan tipi tamsayı değil.".to_owned()),
    }
}
