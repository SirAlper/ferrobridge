//! IR tiplerini bridge tiplerine çeviren kurallar (Obsidian `03 Mimari` tip tablosu ve
//! güvenlik kuralları). Bir tip güvenle ifade edilemiyorsa nedeniyle reddedilir; hiçbir
//! durumda tahmin yürütülmez. Header'ın söylemediği bilgi (pointer'ların sahipliği) yalnızca
//! `ferrobridge.toml` açıklamasından gelir.

use std::collections::HashMap;

use ferrobridge_ir::{ConstantValue, Destructor, QualType, Type};

use crate::config::{ParamAnnotation, ReturnAnnotation};
use crate::plan::{BridgeType, IntType, Pass, Returns, TypeRef};

const REASON_POINTER_PARAM: &str = "Ham pointer'ın sahibi ve ömrü header'dan anlaşılamıyor; \
     `ferrobridge.toml`'da `borrowed` olarak açıklanmalı (`ferrobridge suggest` taslak basar).";
const REASON_POINTER_RETURN: &str = "Dönen ham pointer'ın sahibi header'dan anlaşılamıyor; \
     `ferrobridge.toml`'da açıklanmalı (`ferrobridge suggest` taslak basar).";
const REASON_MUT_PRIMITIVE_REF: &str = "Const olmayan primitif referans (ör. `double&`) \
     saklanabilir; `ferrobridge.toml`'da `borrowed` olarak açıklanmalı.";
const REASON_CLASS_BY_VALUE: &str = "Sınıfı değerle almak kopyalama/taşıma gerektiriyor; henüz \
     desteklenmiyor (`const T&` kullanın).";
const REASON_STRING_BY_VALUE: &str =
    "`std::string`'i değerle almak henüz desteklenmiyor (`const std::string&` kullanın).";
const REASON_VECTOR_BY_VALUE: &str =
    "`std::vector`'ü değerle almak henüz desteklenmiyor (`const std::vector<T>&` kullanın).";
const REASON_RVALUE: &str = "Rvalue referanslar (`T&&`) henüz desteklenmiyor.";
const REASON_REF_RETURN_FREE: &str = "Serbest veya statik fonksiyonun döndürdüğü referansın \
     ömrü belirsiz (`'static` olmalı); sonraki milestone'da ele alınacak.";
const REASON_BORROWED_RETURN_FREE: &str = "`borrowed` dönüş yalnızca metotlarda destekleniyor \
     (referans nesneye bağlanır); serbest/statik fonksiyonlarda ömür `'static` olmalı, sonraki \
     milestone'da ele alınacak.";
const REASON_OWNED_REF: &str =
    "Referans dönüşünün sahipliği devredilemez; `owned` yalnızca pointer dönüşlerinde geçerli.";
const REASON_OWNED_CONST: &str = "`const T*` dönüşü `owned` olarak bağlanmıyor: Rust tarafı \
     nesneyi değiştirebilir hale gelirdi.";
const REASON_OWNED_PRIMITIVE: &str = "`owned` yalnızca sınıf, `std::string` ve `std::vector` \
     pointer'larında destekleniyor (cxx `UniquePtr<primitif>` desteklemiyor).";
const REASON_MUT_REF_FROM_CONST: &str =
    "Const metot değiştirilebilir referans döndüremez (Rust ödünç alma kuralları).";
const REASON_ABSTRACT_BY_VALUE: &str = "Soyut sınıf değerle döndürülemez.";
const REASON_VOID_PARAM: &str = "`void` parametre olamaz.";
const REASON_VOID_POINTER: &str = "`void*` tipsiz bir pointer; güvenli bir Rust karşılığı yok.";
const REASON_C_STRING: &str =
    "`char*` C string'leri henüz desteklenmiyor (`std::string` kullanın).";
const REASON_POINTER_TO_POINTER: &str = "Pointer'a pointer (`T**`) desteklenmiyor.";
const REASON_UNSUPPORTED_POINTEE: &str = "Bu tipe pointer henüz desteklenmiyor.";
const REASON_UNSUPPORTED_REF: &str = "Bu tipe referans henüz desteklenmiyor.";
const REASON_UNSUPPORTED_UNIQUE: &str = "`std::unique_ptr` yalnızca bu header'daki sınıflar, \
     `std::string` ve `std::vector` için destekleniyor.";
const REASON_VECTOR_BOOL: &str =
    "`std::vector<bool>` C++'ta bit paketli özel bir tip; cxx desteklemiyor.";
const REASON_VECTOR_ELEMENT: &str = "`std::vector` elemanı yalnızca sabit genişlikli tamsayı, \
     `float`/`double`, `std::string` ya da bu header'daki bir sınıf olabilir.";
const REASON_FLOAT_SIZE: &str = "Yalnızca `float` ve `double` destekleniyor.";
const REASON_CONSTANT_VALUE: &str = "Sabitin değeri Rust tipine sığmıyor (`char` sabitleri \
     yalnızca 0–127 arasında bağlanır: `c_char`'ın işareti platforma bağlı).";
const REASON_SLICE_NOT_POINTER: &str = "`slice` yalnızca pointer parametrelerine uygulanabilir.";
const REASON_SLICE_ELEMENT: &str = "`slice` yalnızca sayı dizilerini gösteren pointer'lara \
     uygulanabilir (sabit genişlikli tamsayı, `float`, `double`).";
const REASON_SLICE_CHAR: &str = "`char` dizileri `slice` olarak bağlanmıyor (işareti platforma \
     bağlı); bayt dizileri için header'da `uint8_t` kullanılmalı.";
const REASON_SLICE_LENGTH: &str =
    "`slice` uzunluk parametresi değerle geçirilen bir tamsayı olmalı.";

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

/// Bridge'de adı geçebilecek bir sınıf.
#[derive(Clone)]
pub(crate) struct KnownClass {
    pub(crate) type_ref: TypeRef,
    pub(crate) is_abstract: bool,
    pub(crate) is_polymorphic: bool,
    pub(crate) destructor: Destructor,
}

/// Bridge'de adı geçebilecek sınıf ve enum'lar: (namespace, ad) → bilgi.
#[derive(Default)]
pub(crate) struct Known {
    pub(crate) classes: HashMap<(Vec<String>, String), KnownClass>,
    pub(crate) enums: HashMap<(Vec<String>, String), TypeRef>,
}

/// Açıklama eklenince çözülebilecek bir reddin türü.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Need {
    /// Parametreye `borrowed` açıklaması.
    Param,
    /// Dönüşe bu açıklamalardan biri.
    Return(Vec<ReturnAnnotation>),
}

/// Bir tipin reddedilme nedeni.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rejection {
    pub(crate) reason: String,
    /// Açıklama eklenirse bağlanabilir mi?
    pub(crate) need: Option<Need>,
}

impl From<String> for Rejection {
    fn from(reason: String) -> Self {
        Rejection { reason, need: None }
    }
}

impl From<&str> for Rejection {
    fn from(reason: &str) -> Self {
        Rejection::from(reason.to_owned())
    }
}

/// Bağlanabilen bir parametre.
#[derive(Debug)]
pub(crate) struct ParamRule {
    pub(crate) ty: BridgeType,
    pub(crate) pass: Pass,
    /// Verilen açıklama gerçekten kullanıldı mı (kullanılmadıysa gereksizdir)?
    pub(crate) applied: bool,
}

/// Bağlanabilen bir dönüş.
#[derive(Debug)]
pub(crate) struct ReturnRule {
    pub(crate) returns: Returns,
    /// Verilen açıklama gerçekten kullanıldı mı?
    pub(crate) applied: bool,
}

/// Bir parametre tipini çevirir.
///
/// `slice` açıklamaları burada değil, fonksiyon düzeyinde (uzunluk parametresiyle birlikte)
/// işlenir; buraya yalnızca `borrowed` gelir.
pub(crate) fn param_type(
    qt: &QualType,
    known: &Known,
    annotation: Option<&ParamAnnotation>,
) -> Result<ParamRule, Rejection> {
    let borrowed = annotation == Some(&ParamAnnotation::Borrowed);
    let direct = |ty| ParamRule {
        ty,
        pass: Pass::Direct,
        applied: false,
    };
    match &qt.ty {
        Type::Void => Err(REASON_VOID_PARAM.into()),
        Type::Record { .. } => Err(REASON_CLASS_BY_VALUE.into()),
        Type::String => Err(REASON_STRING_BY_VALUE.into()),
        Type::Vector { .. } => Err(REASON_VECTOR_BY_VALUE.into()),
        Type::UniquePtr { pointee } => Ok(direct(BridgeType::UniquePtr(Box::new(unique_inner(
            pointee, known,
        )?)))),
        Type::LValueRef { referent } if is_mut_primitive(referent) => {
            if !borrowed {
                return Err(Rejection {
                    reason: REASON_MUT_PRIMITIVE_REF.to_owned(),
                    need: Some(Need::Param),
                });
            }
            Ok(ParamRule {
                applied: true,
                ..direct(mutable_ref(non_ref(&referent.ty, known)?))
            })
        }
        Type::LValueRef { referent } => Ok(direct(ref_type(referent, known)?)),
        Type::Pointer { pointee } => {
            let inner = pointee_type(pointee, known)?;
            if !borrowed {
                return Err(Rejection {
                    reason: REASON_POINTER_PARAM.to_owned(),
                    need: Some(Need::Param),
                });
            }
            Ok(ParamRule {
                ty: BridgeType::Ref {
                    mutable: !pointee.is_const,
                    inner: Box::new(inner),
                },
                pass: Pass::AddressOf,
                applied: true,
            })
        }
        other => Ok(direct(non_ref(other, known)?)),
    }
}

/// Bir dönüş tipini çevirir. `receiver_mutable`: metotsa alıcının değiştirilebilir olup
/// olmadığı; serbest/statik fonksiyonsa `None`.
pub(crate) fn return_type(
    qt: &QualType,
    known: &Known,
    receiver_mutable: Option<bool>,
    annotation: Option<ReturnAnnotation>,
) -> Result<ReturnRule, Rejection> {
    let plain = |returns| ReturnRule {
        returns,
        applied: false,
    };
    match &qt.ty {
        Type::Void => Ok(plain(Returns::Unit)),
        Type::Record { name, namespace } => {
            let class = class(namespace, name, known)?;
            if class.is_abstract {
                return Err(REASON_ABSTRACT_BY_VALUE.into());
            }
            deletable(&class, false)?;
            Ok(plain(Returns::BoxedClass(class.type_ref)))
        }
        Type::String => Ok(plain(Returns::OwnedString)),
        Type::Vector { element } => {
            Ok(plain(Returns::OwnedVector(vector_element(element, known)?)))
        }
        Type::UniquePtr { pointee } => Ok(plain(Returns::Value(BridgeType::UniquePtr(Box::new(
            unique_inner(pointee, known)?,
        ))))),
        Type::LValueRef { referent } => ref_return(referent, known, receiver_mutable, annotation)
            .map(|(ty, applied)| ReturnRule {
                returns: Returns::Value(ty),
                applied,
            }),
        Type::Pointer { pointee } => pointer_return(pointee, known, receiver_mutable, annotation),
        other => Ok(plain(Returns::Value(non_ref(other, known)?))),
    }
}

/// Metodun döndürdüğü `T&`. Primitif `T&` açıklama ister; diğerleri tip tablosuna uyar.
fn ref_return(
    referent: &QualType,
    known: &Known,
    receiver_mutable: Option<bool>,
    annotation: Option<ReturnAnnotation>,
) -> Result<(BridgeType, bool), Rejection> {
    let Some(receiver_mutable) = receiver_mutable else {
        return Err(REASON_REF_RETURN_FREE.into());
    };
    if annotation == Some(ReturnAnnotation::Owned) {
        return Err(REASON_OWNED_REF.into());
    }
    if is_mut_primitive(referent) {
        if annotation != Some(ReturnAnnotation::Borrowed) {
            return Err(Rejection {
                reason: REASON_MUT_PRIMITIVE_REF.to_owned(),
                need: Some(Need::Return(vec![ReturnAnnotation::Borrowed])),
            });
        }
        if !receiver_mutable {
            return Err(REASON_MUT_REF_FROM_CONST.into());
        }
        return Ok((mutable_ref(non_ref(&referent.ty, known)?), true));
    }
    let reference = ref_type(referent, known)?;
    if matches!(reference, BridgeType::Ref { mutable: true, .. }) && !receiver_mutable {
        return Err(REASON_MUT_REF_FROM_CONST.into());
    }
    Ok((reference, false))
}

/// Dönen `T*`: açıklama olmadan bağlanmaz.
fn pointer_return(
    pointee: &QualType,
    known: &Known,
    receiver_mutable: Option<bool>,
    annotation: Option<ReturnAnnotation>,
) -> Result<ReturnRule, Rejection> {
    let inner = pointee_type(pointee, known)?;
    let ownable = matches!(
        inner,
        BridgeType::Class(_) | BridgeType::String | BridgeType::Vector(_)
    ) && !pointee.is_const;
    match annotation {
        None => {
            let mut options = Vec::new();
            if ownable {
                options.push(ReturnAnnotation::Owned);
            }
            if receiver_mutable.is_some() {
                options.push(ReturnAnnotation::Borrowed);
            }
            if options.is_empty() {
                // Serbest/statik fonksiyon ve sahiplenilemeyen pointer: hiçbir açıklama
                // işe yaramıyor.
                let owned = if pointee.is_const {
                    REASON_OWNED_CONST
                } else {
                    REASON_OWNED_PRIMITIVE
                };
                return Err(format!("{owned} {REASON_BORROWED_RETURN_FREE}").into());
            }
            Err(Rejection {
                reason: REASON_POINTER_RETURN.to_owned(),
                need: Some(Need::Return(options)),
            })
        }
        Some(ReturnAnnotation::Owned) => {
            if pointee.is_const {
                return Err(REASON_OWNED_CONST.into());
            }
            match &inner {
                BridgeType::Class(_) => {
                    if let Type::Record { name, namespace } = &pointee.ty {
                        deletable(&class(namespace, name, known)?, true)?;
                    }
                }
                BridgeType::String | BridgeType::Vector(_) => {}
                _ => return Err(REASON_OWNED_PRIMITIVE.into()),
            }
            Ok(ReturnRule {
                returns: Returns::OwnedPointer(inner),
                applied: true,
            })
        }
        Some(ReturnAnnotation::Borrowed) => {
            let Some(receiver_mutable) = receiver_mutable else {
                return Err(REASON_BORROWED_RETURN_FREE.into());
            };
            Ok(ReturnRule {
                returns: Returns::BorrowedPointer {
                    mutable: !pointee.is_const && receiver_mutable,
                    inner,
                },
                applied: true,
            })
        }
    }
}

/// Bir sabitin Rust tipi; değer tipe uymuyorsa ya da sığmıyorsa reddedilir.
pub(crate) fn constant_type(ty: &Type, value: ConstantValue) -> Result<BridgeType, String> {
    let bridge = non_ref(ty, &Known::default())?;
    let as_int = |value: ConstantValue| match value {
        ConstantValue::Signed(v) => Some(i128::from(v)),
        ConstantValue::Unsigned(v) => Some(i128::from(v)),
        _ => None,
    };
    let fits = match (&bridge, value) {
        (BridgeType::Bool, ConstantValue::Bool(_))
        | (BridgeType::F32 | BridgeType::F64, ConstantValue::Float(_)) => true,
        (BridgeType::Int(int), value) => {
            let (min, max) = int.range();
            as_int(value).is_some_and(|v| (min..=max).contains(&v))
        }
        // `c_char` bir platformda işaretli, diğerinde işaretsiz: yalnızca ikisinde de aynı
        // olan değerler.
        (BridgeType::Char, value) => as_int(value).is_some_and(|v| (0..=127).contains(&v)),
        _ => false,
    };
    if fits {
        Ok(bridge)
    } else {
        Err(REASON_CONSTANT_VALUE.to_owned())
    }
}

/// `slice` açıklamalı bir parametre: eleman tipi ve dizinin değiştirilebilir olup olmadığı.
pub(crate) fn slice_element(qt: &QualType, known: &Known) -> Result<(BridgeType, bool), String> {
    let Type::Pointer { pointee } = &qt.ty else {
        return Err(REASON_SLICE_NOT_POINTER.to_owned());
    };
    match &pointee.ty {
        Type::Integer { .. } | Type::Float { .. } => match non_ref(&pointee.ty, known)? {
            ty @ (BridgeType::Int(_) | BridgeType::F32 | BridgeType::F64) => {
                Ok((ty, !pointee.is_const))
            }
            _ => Err(REASON_SLICE_CHAR.to_owned()),
        },
        _ => Err(REASON_SLICE_ELEMENT.to_owned()),
    }
}

/// `slice` uzunluk parametresinin C++ temel tipi (ör. `"unsigned short"`). Rust'a görünmediği
/// için `size_t` gibi platforma bağlı tipler de kabul edilir.
pub(crate) fn slice_length(qt: &QualType) -> Result<String, String> {
    match &qt.ty {
        Type::Integer { builtin, .. } => Ok(builtin.clone()),
        _ => Err(REASON_SLICE_LENGTH.to_owned()),
    }
}

/// Sayı dizisi olabilecek bir pointer mı (`slice` önerisi için)?
pub(crate) fn could_be_slice(qt: &QualType, known: &Known) -> bool {
    slice_element(qt, known).is_ok()
}

/// `const` olmayan primitif ya da enum mu (`double&` gibi)?
fn is_mut_primitive(referent: &QualType) -> bool {
    !referent.is_const
        && matches!(
            referent.ty,
            Type::Bool | Type::Integer { .. } | Type::Float { .. } | Type::Enum { .. }
        )
}

fn mutable_ref(inner: BridgeType) -> BridgeType {
    BridgeType::Ref {
        mutable: true,
        inner: Box::new(inner),
    }
}

/// Bir pointer'ın gösterebileceği (açıklamayla referansa çevrilebilecek) tipler.
fn pointee_type(pointee: &QualType, known: &Known) -> Result<BridgeType, Rejection> {
    match &pointee.ty {
        Type::Record { name, namespace } => {
            Ok(BridgeType::Class(class(namespace, name, known)?.type_ref))
        }
        Type::String => Ok(BridgeType::String),
        Type::Vector { element } => Ok(BridgeType::Vector(Box::new(vector_element(
            element, known,
        )?))),
        Type::Integer {
            builtin,
            fixed_width: false,
            ..
        } if builtin == "char" => Err(REASON_C_STRING.into()),
        Type::Bool | Type::Integer { .. } | Type::Float { .. } | Type::Enum { .. } => {
            Ok(non_ref(&pointee.ty, known)?)
        }
        Type::Void => Err(REASON_VOID_POINTER.into()),
        Type::Pointer { .. } => Err(REASON_POINTER_TO_POINTER.into()),
        Type::Unsupported { spelling, reason } => Err(format!("`{spelling}`: {reason}").into()),
        _ => Err(REASON_UNSUPPORTED_POINTEE.into()),
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
        Type::Pointer { .. } => Err(REASON_POINTER_PARAM.to_owned()),
        Type::RValueRef { .. } => Err(REASON_RVALUE.to_owned()),
        Type::Unsupported { spelling, reason } => Err(format!("`{spelling}`: {reason}")),
        Type::Void
        | Type::Record { .. }
        | Type::String
        | Type::UniquePtr { .. }
        | Type::Vector { .. }
        | Type::LValueRef { .. } => Err(REASON_UNSUPPORTED_REF.to_owned()),
    }
}

/// `T&` / `const T&` için tip tablosu (const olmayan primitifler hariç; onlar açıklama ister).
fn ref_type(referent: &QualType, known: &Known) -> Result<BridgeType, Rejection> {
    let mutable = !referent.is_const;
    let inner = match &referent.ty {
        Type::Record { name, namespace } => {
            BridgeType::Class(class(namespace, name, known)?.type_ref)
        }
        Type::String => BridgeType::String,
        Type::Vector { element } => BridgeType::Vector(Box::new(vector_element(element, known)?)),
        Type::Bool | Type::Integer { .. } | Type::Float { .. } | Type::Enum { .. } => {
            if mutable {
                return Err(Rejection {
                    reason: REASON_MUT_PRIMITIVE_REF.to_owned(),
                    need: Some(Need::Param),
                });
            }
            non_ref(&referent.ty, known)?
        }
        Type::Pointer { .. } => return Err(REASON_POINTER_PARAM.into()),
        Type::Unsupported { spelling, reason } => {
            return Err(format!("`{spelling}`: {reason}").into());
        }
        _ => return Err(REASON_UNSUPPORTED_REF.into()),
    };
    Ok(BridgeType::Ref {
        mutable,
        inner: Box::new(inner),
    })
}

/// `std::vector` eleman tipi: cxx'in `CxxVector<T>` için kabul ettikleri.
fn vector_element(element: &QualType, known: &Known) -> Result<BridgeType, String> {
    match &element.ty {
        Type::Bool => Err(REASON_VECTOR_BOOL.to_owned()),
        Type::Integer { .. } | Type::Float { .. } => match non_ref(&element.ty, known)? {
            ty @ (BridgeType::Int(_) | BridgeType::F32 | BridgeType::F64) => Ok(ty),
            _ => Err(REASON_VECTOR_ELEMENT.to_owned()),
        },
        Type::String => Ok(BridgeType::String),
        Type::Record { name, namespace } => {
            let class = class(namespace, name, known)?;
            // `std::vector<T>`'nin kendi destructor'ı elemanları siler.
            deletable(&class, false)?;
            Ok(BridgeType::Class(class.type_ref))
        }
        Type::Unsupported { spelling, reason } => Err(format!("`{spelling}`: {reason}")),
        _ => Err(REASON_VECTOR_ELEMENT.to_owned()),
    }
}

fn unique_inner(pointee: &QualType, known: &Known) -> Result<BridgeType, String> {
    match &pointee.ty {
        Type::Record { name, namespace } => {
            let class = class(namespace, name, known)?;
            deletable(&class, false)?;
            Ok(BridgeType::Class(class.type_ref))
        }
        Type::String => Ok(BridgeType::String),
        Type::Vector { element } => Ok(BridgeType::Vector(Box::new(vector_element(
            element, known,
        )?))),
        _ => Err(REASON_UNSUPPORTED_UNIQUE.to_owned()),
    }
}

/// Rust tarafının (`UniquePtr` düşünce) nesneyi `delete` ile silmesi güvenli mi?
///
/// `through_base_pointer`: nesne bir pointer'dan geliyor ve gerçek tipi bir alt sınıf
/// olabilir. O zaman polimorfik sınıfın destructor'ı sanal olmalı; değilse `delete` UB'dir.
pub(crate) fn deletable(class: &KnownClass, through_base_pointer: bool) -> Result<(), String> {
    let path = class.type_ref.cpp_path.trim_start_matches("::");
    match class.destructor {
        Destructor::NotPublic => Err(format!(
            "`{path}`'ın destructor'ı public değil; Rust tarafı nesneyi silemez, `UniquePtr` \
             üretilmiyor."
        )),
        Destructor::Deleted => Err(format!(
            "`{path}`'ın destructor'ı silinmiş (`= delete`); `UniquePtr` üretilmiyor."
        )),
        Destructor::Public { is_virtual: false }
            if through_base_pointer && class.is_polymorphic =>
        {
            Err(format!(
                "`{path}` polimorfik ama destructor'ı sanal değil: dönen pointer bir alt sınıfı \
                 gösteriyorsa `delete` tanımsız davranış olur, `UniquePtr` üretilmiyor."
            ))
        }
        Destructor::Public { .. } => Ok(()),
    }
}

fn class(namespace: &[String], name: &str, known: &Known) -> Result<KnownClass, String> {
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

/// Bir tipin okunabilir C++ yazımı (öneri ve uyarılarda kullanılır).
pub(crate) fn spell(qt: &QualType) -> String {
    let base = match &qt.ty {
        Type::Void => "void".to_owned(),
        Type::Bool => "bool".to_owned(),
        Type::Integer { spelling, .. } | Type::Float { spelling, .. } => spelling.clone(),
        Type::Pointer { pointee } => {
            let star = format!("{}*", spell(pointee));
            return if qt.is_const {
                format!("{star} const")
            } else {
                star
            };
        }
        Type::LValueRef { referent } => return format!("{}&", spell(referent)),
        Type::RValueRef { referent } => return format!("{}&&", spell(referent)),
        Type::Record { name, namespace } | Type::Enum { name, namespace } => {
            let mut path = namespace.clone();
            path.push(name.clone());
            path.join("::")
        }
        Type::String => "std::string".to_owned(),
        Type::UniquePtr { pointee } => format!("std::unique_ptr<{}>", spell(pointee)),
        Type::Vector { element } => format!("std::vector<{}>", spell(element)),
        Type::Unsupported { spelling, .. } => spelling.clone(),
    };
    if qt.is_const {
        format!("const {base}")
    } else {
        base
    }
}
